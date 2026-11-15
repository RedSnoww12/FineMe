//! Tests d'intégration HTTP : le routeur Axum complet, dans le processus, au-dessus d'un vrai Postgres
//! (testcontainers). Chaque test a sa propre base jetable ; toutes les données sont fictives.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};
use tower::ServiceExt;

const PASSWORD: &str = "mot-de-passe-de-test";
const ORIGIN: &str = "http://localhost:5173";
/// Prêts fictifs démarrant dans un futur lointain : le capital restant dû ne dépend pas de la date du jour.
const FAR_FUTURE: &str = "2090-01-01";

struct Api {
    router: Router,
    token: String,
    _container: ContainerAsync<Postgres>,
}

impl Api {
    /// Base vide et migrée, sans utilisateur.
    async fn fresh() -> Self {
        let container = Postgres::default()
            .with_tag("17")
            .start()
            .await
            .expect("démarrage du conteneur Postgres");
        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("port du conteneur");
        let host = container.get_host().await.expect("hôte du conteneur");
        let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");
        let pool = fineme_infra::postgres::connect(&url).await.unwrap();
        fineme_infra::postgres::migrate(&pool).await.unwrap();
        let router =
            fineme_server::build_router(pool, "secret-de-test-0123456789abcdef0123", ORIGIN)
                .await
                .unwrap();
        Self {
            router,
            token: String::new(),
            _container: container,
        }
    }

    /// Base configurée : utilisatrice « Alice » connectée, plus un second membre « Bob ».
    async fn household() -> (Self, String, String) {
        let mut api = Self::fresh().await;
        let body = json!({ "email": "alice@exemple.test", "password": PASSWORD, "name": "Alice" });
        let (status, session) = api.call("POST", "/api/auth/setup", Some(body)).await;
        assert_eq!(status, StatusCode::CREATED);
        api.token = session["token"].as_str().unwrap().to_string();
        let (_, members) = api.call("GET", "/api/members", None).await;
        let alice = members[0]["id"].as_str().unwrap().to_string();
        let (status, bob) = api
            .call("POST", "/api/members", Some(json!({ "name": "Bob" })))
            .await;
        assert_eq!(status, StatusCode::CREATED);
        (api, alice, bob["id"].as_str().unwrap().to_string())
    }

    async fn send(&self, request: Request<Body>) -> (StatusCode, axum::http::HeaderMap, Value) {
        let response = self.router.clone().oneshot(request).await.unwrap();
        let (status, headers) = (response.status(), response.headers().clone());
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        (status, headers, body)
    }

    async fn call(&self, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let mut request = Request::builder().method(method).uri(path);
        if !self.token.is_empty() {
            request = request.header(header::AUTHORIZATION, format!("Bearer {}", self.token));
        }
        let request = match body {
            Some(body) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string())),
            None => request.body(Body::empty()),
        };
        let (status, _, body) = self.send(request.unwrap()).await;
        (status, body)
    }

    async fn get(&self, path: &str) -> Value {
        let (status, body) = self.call("GET", path, None).await;
        assert_eq!(status, StatusCode::OK, "GET {path} : {body}");
        body
    }

    async fn post(&self, path: &str, body: Value) -> Value {
        let (status, response) = self.call("POST", path, Some(body)).await;
        assert!(status.is_success(), "POST {path} : {status} {response}");
        response
    }

    async fn id_of(&self, path: &str, body: Value) -> String {
        self.post(path, body).await["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    /// Maison 300 k€ et prêt 200 k€ lié, tous deux à 50/50.
    async fn house_with_loan(&self, alice: &str, bob: &str) -> (String, String) {
        let owners =
            json!([{ "member_id": alice, "share": 0.5 }, { "member_id": bob, "share": 0.5 }]);
        let house = self
            .id_of("/api/accounts", json!({
                "name": "Maison", "kind": "real_estate", "owners": owners,
                "property": { "address": "1 rue des Exemples", "category": "rental", "building_type": "house",
                    "surface_m2": 90, "purchase_price": 250000, "purchase_fees": 20000, "purchased_at": "2022-03-01",
                    "current_value": 300000, "monthly_rent": 1200, "monthly_charges": 100, "yearly_property_tax": 1200 }
            }))
            .await;
        let loan = self
            .id_of("/api/accounts", json!({
                "name": "Prêt maison", "institution": "Banque Fictive", "kind": "loan", "owners": owners,
                "loan": { "property_account_id": house, "principal": 200000, "interest_rate_pct": 4.0,
                    "insurance_rate_pct": 0.3, "duration_months": 240, "start_date": FAR_FUTURE }
            }))
            .await;
        (house, loan)
    }
}

fn error_of(response: &(StatusCode, Value)) -> (u16, &str) {
    let keys: Vec<&String> = response.1.as_object().unwrap().keys().collect();
    assert_eq!(
        keys,
        vec!["error"],
        "forme d'erreur inattendue : {}",
        response.1
    );
    (response.0.as_u16(), response.1["error"].as_str().unwrap())
}

/// Bundle dont les comptes sont triés par identifiant externe, pour comparer deux exports.
fn by_external_id(mut bundle: Value) -> Value {
    if let Some(accounts) = bundle["accounts"].as_array_mut() {
        accounts.sort_by_key(|account| {
            account["external_id"]
                .as_str()
                .unwrap_or_default()
                .to_string()
        });
    }
    bundle
}

fn close(value: &Value, expected: f64) -> bool {
    (value.as_f64().unwrap() - expected).abs() < 1e-6
}

#[tokio::test]
async fn auth_flow_from_first_setup_to_protected_routes() {
    let mut api = Api::fresh().await;
    assert_eq!(api.get("/api/health").await, json!({ "status": "ok" }));
    assert_eq!(
        api.get("/api/auth/status").await,
        json!({ "setup_required": true })
    );

    // Sans jeton, toute route protégée répond 401, y compris l'import.
    for (method, path) in [
        ("GET", "/api/members"),
        ("GET", "/api/overview"),
        ("POST", "/api/import"),
        ("GET", "/api/auth/me"),
    ] {
        assert_eq!(
            error_of(&api.call(method, path, None).await),
            (401, "Authentification requise")
        );
    }

    let short = json!({ "email": "alice@exemple.test", "password": "court", "name": "Alice" });
    assert_eq!(
        error_of(&api.call("POST", "/api/auth/setup", Some(short)).await),
        (400, "Le mot de passe doit contenir au moins 10 caractères")
    );
    let body = json!({ "email": "Alice@Exemple.test", "password": PASSWORD, "name": "Alice" });
    let (status, session) = api
        .call("POST", "/api/auth/setup", Some(body.clone()))
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(session["user"]["email"], "alice@exemple.test");
    assert_eq!(session["user"].as_object().unwrap().len(), 3);
    assert_eq!(
        error_of(&api.call("POST", "/api/auth/setup", Some(body)).await),
        (409, "Un utilisateur existe déjà")
    );
    assert_eq!(
        api.get("/api/auth/status").await,
        json!({ "setup_required": false })
    );

    // Même réponse pour un mauvais mot de passe et un e-mail inconnu.
    let wrong = api
        .call(
            "POST",
            "/api/auth/login",
            Some(json!({ "email": "alice@exemple.test", "password": "mauvais-mot-de-passe" })),
        )
        .await;
    let unknown = api
        .call(
            "POST",
            "/api/auth/login",
            Some(json!({ "email": "inconnu@exemple.test", "password": PASSWORD })),
        )
        .await;
    assert_eq!(error_of(&wrong), (401, "E-mail ou mot de passe incorrect"));
    assert_eq!(wrong, unknown);

    let login = api
        .post(
            "/api/auth/login",
            json!({ "email": "ALICE@exemple.test", "password": PASSWORD }),
        )
        .await;
    api.token = login["token"].as_str().unwrap().to_string();
    assert_eq!(api.get("/api/auth/me").await, session["user"]);
    // La configuration a créé le membre principal.
    let members = api.get("/api/members").await;
    assert_eq!(members.as_array().unwrap().len(), 1);
    assert_eq!(
        (&members[0]["name"], &members[0]["is_primary"]),
        (&json!("Alice"), &json!(true))
    );

    api.token = "jeton.invalide".into();
    assert_eq!(
        error_of(&api.call("GET", "/api/auth/me", None).await),
        (401, "Session invalide ou expirée")
    );
}

#[tokio::test]
async fn error_responses_share_one_shape() {
    let (api, alice, _) = Api::household().await;
    assert_eq!(
        error_of(&api.call("GET", "/api/nulle-part", None).await),
        (404, "Route introuvable")
    );
    assert_eq!(
        error_of(&api.call("GET", "/hors-api", None).await),
        (404, "Route introuvable")
    );
    assert_eq!(
        error_of(&api.call("DELETE", "/api/health", None).await),
        (405, "Méthode non autorisée")
    );
    assert_eq!(
        error_of(&api.call("GET", "/api/accounts/pas-un-uuid", None).await),
        (400, "Identifiant invalide dans l'URL")
    );
    assert_eq!(
        error_of(
            &api.call(
                "GET",
                "/api/accounts/00000000-0000-0000-0000-000000000001",
                None
            )
            .await
        ),
        (404, "Compte introuvable")
    );
    assert_eq!(
        error_of(&api.call("GET", "/api/overview?member_id=abc", None).await),
        (400, "Paramètres de requête invalides")
    );
    assert_eq!(
        error_of(
            &api.call(
                "GET",
                "/api/overview?member_id=00000000-0000-0000-0000-000000000001",
                None
            )
            .await
        ),
        (404, "Membre introuvable")
    );
    assert_eq!(
        error_of(&api.call("GET", "/api/history?range=2y", None).await),
        (400, "Période invalide (1m, 3m, 1y ou all)")
    );

    // JSON mal formé ou de mauvais type.
    let request = Request::builder()
        .method("POST")
        .uri("/api/accounts")
        .header(header::AUTHORIZATION, format!("Bearer {}", api.token))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{ pas du json"))
        .unwrap();
    let (status, _, body) = api.send(request).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .starts_with("Corps JSON invalide")
    );
    let wrong_type = api
        .call(
            "POST",
            "/api/accounts",
            Some(json!({ "name": "X", "cash_balance": "beaucoup" })),
        )
        .await;
    assert!(error_of(&wrong_type).1.starts_with("Corps JSON invalide"));

    // Règles métier → 400 avec un message en français ; conflits → 409.
    let owners = json!([{ "member_id": alice, "share": 1 }]);
    let cases = [
        (
            json!({ "name": "X", "kind": "bogus", "owners": owners }),
            "Type de compte invalide (checking, savings, investment, crypto, real_estate, loan, other)",
        ),
        (
            json!({ "name": "X", "kind": "savings" }),
            "La liste des propriétaires (owners) est requise",
        ),
        (
            json!({ "name": "X", "kind": "real_estate", "owners": owners }),
            "property est requis pour un compte immobilier",
        ),
        (
            json!({ "name": "X", "kind": "loan", "owners": owners }),
            "loan est requis pour un compte de prêt",
        ),
        (
            json!({ "name": "X", "kind": "savings", "owners": [{ "member_id": alice, "share": 1.5 }] }),
            "Chaque part doit être comprise entre 0 et 1",
        ),
        (
            json!({ "name": "X", "kind": "savings", "owners": [{ "member_id": "00000000-0000-0000-0000-000000000001", "share": 1 }] }),
            "Un des propriétaires ne correspond à aucun membre",
        ),
        (
            json!({ "name": "X", "kind": "savings", "currency": "euros", "owners": owners }),
            "Devise invalide (code ISO à 3 lettres attendu)",
        ),
    ];
    for (body, message) in cases {
        assert_eq!(
            error_of(&api.call("POST", "/api/accounts", Some(body)).await),
            (400, message)
        );
    }
    assert_eq!(
        error_of(
            &api.call("POST", "/api/members", Some(json!({ "name": "Bob" })))
                .await
        ),
        (409, "Un membre porte déjà ce nom")
    );
    assert_eq!(
        error_of(
            &api.call("DELETE", &format!("/api/members/{alice}"), None)
                .await
        ),
        (409, "Le membre principal ne peut pas être supprimé")
    );
    assert!(
        api.get("/api/accounts")
            .await
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn accounts_with_real_estate_and_linked_loan() {
    let (api, alice, bob) = Api::household().await;
    let (house, loan) = api.house_with_loan(&alice, &bob).await;

    let detail = api.get(&format!("/api/accounts/{house}")).await;
    assert_eq!(detail["kind"], "real_estate");
    assert_eq!(detail["is_liability"], false);
    assert!(close(&detail["value"], 300_000.0) && close(&detail["owned_value"], 300_000.0));
    assert!(close(&detail["invested"], 270_000.0) && close(&detail["gain"], 30_000.0));
    assert_eq!(detail["property"]["account_id"], json!(house));
    assert_eq!(detail["property"]["purchased_at"], "2022-03-01");
    assert_eq!(detail["loan"], Value::Null);
    assert_eq!(detail["holdings"], json!([]));
    assert_eq!(
        detail["owners"],
        json!([
            { "member_id": alice, "name": "Alice", "share": 0.5 },
            { "member_id": bob, "name": "Bob", "share": 0.5 },
        ])
    );
    assert_eq!(
        (
            &detail["currency"],
            &detail["source"],
            &detail["external_id"]
        ),
        (&json!("EUR"), &json!("manual"), &Value::Null)
    );

    let detail = api
        .get(&format!("/api/accounts/{loan}?member_id={bob}"))
        .await;
    assert_eq!(detail["is_liability"], true);
    assert!(close(&detail["value"], 200_000.0) && close(&detail["owned_value"], 100_000.0));
    assert_eq!(detail["invested"], Value::Null);
    let terms = &detail["loan"];
    assert_eq!(terms["property_account_id"], json!(house));
    assert_eq!(
        (
            &terms["duration_months"],
            &terms["start_date"],
            &terms["end_date"]
        ),
        (&json!(240), &json!(FAR_FUTURE), &json!("2110-01-01"))
    );
    assert!(
        close(&terms["monthly_payment"], 1211.960658598819)
            && close(&terms["monthly_insurance"], 50.0)
    );
    assert!(
        close(&terms["remaining_capital"], 200_000.0) && close(&terms["total_insurance"], 12_000.0)
    );

    // Mise à jour partielle : `null` efface, un champ absent est conservé, le bien se modifie en profondeur.
    let patch =
        json!({ "institution": null, "property": { "current_value": 310000, "address": null } });
    let (status, updated) = api
        .call("PUT", &format!("/api/accounts/{house}"), Some(patch))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(close(&updated["value"], 310_000.0));
    assert_eq!(updated["property"]["address"], Value::Null);
    assert!(close(&updated["property"]["monthly_rent"], 1200.0));
    assert_eq!(updated["name"], "Maison");

    // Synthèse immobilière et tableau d'amortissement.
    let report = api.get("/api/real-estate").await;
    let item = &report["properties"][0];
    assert_eq!(item["loans"][0]["name"], "Prêt maison");
    assert_eq!(item["loans"][0]["account_id"], json!(loan));
    assert!(close(&item["equity"], 110_000.0) && close(&item["latent_gain"], 40_000.0));
    assert!(close(
        &item["gross_yield_pct"],
        1200.0 * 12.0 / 270_000.0 * 100.0
    ));
    assert!(
        close(&report["totals"]["remaining_debt"], 200_000.0)
            && close(&report["totals"]["monthly_rent"], 1200.0)
    );
    assert!(close(
        &report["totals"]["monthly_payments"],
        1261.960658598819
    ));
    let alice_report = api
        .get(&format!("/api/real-estate?member_id={alice}"))
        .await;
    assert!(
        close(&alice_report["totals"]["value"], 155_000.0)
            && close(&alice_report["totals"]["equity"], 55_000.0)
    );

    let schedule = api.get(&format!("/api/loans/{loan}/schedule")).await;
    let rows = schedule["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 240);
    assert_eq!(
        (&rows[0]["month"], &rows[0]["date"]),
        (&json!(1), &json!("2090-02-01"))
    );
    assert!(
        close(&rows[0]["interest"], 200_000.0 * 0.04 / 12.0) && close(&rows[239]["remaining"], 0.0)
    );
    assert_eq!(schedule["loan"]["id"], terms["id"]);
    assert_eq!(
        error_of(
            &api.call("GET", &format!("/api/loans/{house}/schedule"), None)
                .await
        ),
        (404, "Prêt introuvable")
    );

    // Suppression : le prêt survit, délié.
    assert_eq!(
        api.call("DELETE", &format!("/api/accounts/{house}"), None)
            .await,
        (StatusCode::NO_CONTENT, Value::Null)
    );
    assert_eq!(
        api.get(&format!("/api/accounts/{loan}")).await["loan"]["property_account_id"],
        Value::Null
    );
    assert!(
        api.get("/api/real-estate").await["properties"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn holdings_and_transactions_flow() {
    let (api, alice, _) = Api::household().await;
    let owners = json!([{ "member_id": alice, "share": 1 }]);
    let pea = api.id_of("/api/accounts", json!({ "name": "PEA", "kind": "investment", "envelope": "pea", "cash_balance": 500, "owners": owners })).await;

    let (status, holding) = api
        .call("POST", &format!("/api/accounts/{pea}/holdings"), Some(json!({
            "name": "ETF Démo", "isin": "fr0000000001", "security_type": "etf", "quantity": 100, "buy_price": 20,
            "current_price": 25, "expense_ratio": 0.0015
        })))
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        (
            &holding["kind"],
            &holding["isin"],
            &holding["currency"],
            &holding["price_source"]
        ),
        (
            &json!("security"),
            &json!("FR0000000001"),
            &json!("EUR"),
            &json!("manual")
        )
    );
    assert!(
        close(&holding["value"], 2500.0)
            && close(&holding["gain"], 500.0)
            && close(&holding["gain_pct"], 25.0)
    );

    let id = holding["id"].as_str().unwrap();
    let (status, updated) = api
        .call(
            "PUT",
            &format!("/api/holdings/{id}"),
            Some(json!({ "quantity": 110, "expense_ratio": null })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(close(&updated["value"], 2750.0));
    assert_eq!(updated["expense_ratio"], Value::Null);

    let bulk = api
        .post(
            &format!("/api/accounts/{pea}/holdings/bulk"),
            json!({ "replace": false, "holdings": [
            { "kind": "fonds_euro", "name": "Fonds euro", "quantity": 1, "current_price": 100 }
        ] }),
        )
        .await;
    assert_eq!(bulk.as_array().unwrap().len(), 2);
    let detail = api.get(&format!("/api/accounts/{pea}")).await;
    assert!(close(&detail["value"], 500.0 + 2750.0 + 100.0));
    // Une position sans PRU rend le prix de revient du compte inconnu ; l'import CSV change l'origine.
    assert_eq!(
        (&detail["invested"], &detail["source"]),
        (&Value::Null, &json!("csv"))
    );

    assert_eq!(
        api.call("DELETE", &format!("/api/holdings/{id}"), None)
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        error_of(
            &api.call("DELETE", &format!("/api/holdings/{id}"), None)
                .await
        ),
        (404, "Position introuvable")
    );

    // Transactions : dédoublonnage, nouveau solde, pagination.
    let checking = api.id_of("/api/accounts", json!({ "name": "Courant", "kind": "checking", "cash_balance": 3000, "owners": owners })).await;
    let batch = json!([
        { "date": "2026-01-01", "label": "Courses", "amount": -54.2, "category": "Alimentation" },
        { "date": "2026-01-02", "label": "Salaire", "amount": 2500 }
    ]);
    let path = format!("/api/accounts/{checking}/transactions/bulk");
    assert_eq!(
        api.post(
            &path,
            json!({ "transactions": batch, "new_cash_balance": 3500 })
        )
        .await,
        json!({ "inserted": 2, "skipped": 0 })
    );
    assert_eq!(
        api.post(&path, json!({ "transactions": batch })).await,
        json!({ "inserted": 0, "skipped": 2 })
    );
    let page = api
        .get(&format!(
            "/api/accounts/{checking}/transactions?limit=1&offset=1"
        ))
        .await;
    assert_eq!(page["total"], 2);
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        (
            &page["items"][0]["label"],
            &page["items"][0]["category"],
            &page["items"][0]["date"]
        ),
        (
            &json!("Courses"),
            &json!("Alimentation"),
            &json!("2026-01-01")
        )
    );
    assert!(close(
        &api.get(&format!("/api/accounts/{checking}")).await["cash_balance"],
        3500.0
    ));
    let tx = page["items"][0]["id"].as_str().unwrap();
    assert_eq!(
        api.call("DELETE", &format!("/api/transactions/{tx}"), None)
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        api.get(&format!("/api/accounts/{checking}/transactions"))
            .await["total"],
        1
    );
}

#[tokio::test]
async fn overview_for_household_and_member_with_history() {
    let (api, alice, bob) = Api::household().await;
    api.house_with_loan(&alice, &bob).await;
    // Compte joint dont 20 % sont détenus hors du foyer.
    api.post("/api/accounts", json!({ "name": "Joint", "institution": "Banque Fictive", "kind": "checking", "cash_balance": 1000,
        "owners": [{ "member_id": alice, "share": 0.6 }, { "member_id": bob, "share": 0.2 }] })).await;
    let pea = api.id_of("/api/accounts", json!({ "name": "PEA", "institution": "Courtier Fictif", "kind": "investment", "envelope": "pea",
        "cash_balance": 500, "owners": [{ "member_id": alice, "share": 1 }] })).await;
    api.post(&format!("/api/accounts/{pea}/holdings"), json!({ "name": "ETF Démo", "symbol": "demo", "quantity": 10, "buy_price": 20, "current_price": 25, "expense_ratio": 0.002 })).await;

    let household = api.get("/api/overview").await;
    assert!(close(&household["gross_assets"], 300_000.0 + 800.0 + 750.0));
    assert!(
        close(&household["liabilities"], 200_000.0) && close(&household["net_worth"], 101_550.0)
    );
    assert!(
        close(&household["financial_assets"], 750.0)
            && close(&household["invested"], 200.0)
            && close(&household["gain"], 50.0)
    );
    assert!(close(&household["weighted_expense_ratio"], 0.002));
    let categories: Vec<(&str, &str)> = household["allocation"]
        .as_array()
        .unwrap()
        .iter()
        .map(|slice| {
            (
                slice["category"].as_str().unwrap(),
                slice["label"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        categories,
        vec![
            ("checking", "Comptes courants"),
            ("investment", "Investissements"),
            ("real_estate", "Immobilier")
        ]
    );
    assert!(close(
        &json!(
            household["allocation"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s["pct"].as_f64().unwrap())
                .sum::<f64>()
        ),
        100.0
    ));
    assert_eq!(
        household["by_institution"][0],
        json!({ "institution": "Autre", "value": 300000.0 })
    );
    assert_eq!(
        household["by_envelope"],
        json!([{ "envelope": "pea", "value": 750.0 }])
    );
    assert_eq!(
        household["top_holdings"],
        json!([{ "name": "ETF Démo", "symbol": "DEMO", "value": 250.0, "gain_pct": 25.0, "account_name": "PEA" }])
    );
    let members = household["members"].as_array().unwrap();
    assert_eq!(members.len(), 2);
    assert_eq!(members[0]["member_id"], json!(alice));
    assert!(close(
        &members[1]["net_worth"],
        150_000.0 + 200.0 - 100_000.0
    ));

    let alice_view = api.get(&format!("/api/overview?member_id={alice}")).await;
    let bob_view = api.get(&format!("/api/overview?member_id={bob}")).await;
    assert!(close(
        &alice_view["gross_assets"],
        150_000.0 + 600.0 + 750.0
    ));
    assert!(
        close(&bob_view["gross_assets"], 150_000.0 + 200.0)
            && close(&bob_view["financial_assets"], 0.0)
    );
    assert_eq!(bob_view["weighted_expense_ratio"], Value::Null);
    assert!(close(
        &json!(alice_view["net_worth"].as_f64().unwrap() + bob_view["net_worth"].as_f64().unwrap()),
        101_550.0
    ));

    // Comptes filtrés par membre : Bob ne voit pas le PEA d'Alice.
    let names: Vec<String> = api
        .get(&format!("/api/accounts?member_id={bob}"))
        .await
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names, vec!["Maison", "Prêt maison", "Joint"]);

    // Chaque appel à la vue d'ensemble a mis à jour l'instantané du jour, pour chaque vue.
    let history = api.get("/api/history?range=1m").await;
    let points = history["points"].as_array().unwrap();
    assert_eq!(points.len(), 1);
    assert!(close(&points[0]["net_worth"], 101_550.0));
    assert_eq!(points[0].as_object().unwrap().len(), 4);
    let bob_history = api.get(&format!("/api/history?member_id={bob}")).await;
    assert!(close(&bob_history["points"][0]["net_worth"], 50_200.0));
}

#[tokio::test]
async fn goals_flow() {
    let (api, alice, bob) = Api::household().await;
    let savings = api
        .id_of(
            "/api/accounts",
            json!({ "name": "Livret", "kind": "savings", "cash_balance": 9000,
        "owners": [{ "member_id": alice, "share": 1 }] }),
        )
        .await;

    let (status, net) = api.call("POST", "/api/goals", Some(json!({ "name": "Matelas", "icon": "🛟", "kind": "safety_net",
        "monthly_expenses": 2000, "coverage_months": 6, "account_ids": [savings], "member_ids": [alice] }))).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        net["progress"],
        json!({ "current_amount": 9000.0, "target_amount": 12000.0, "pct": 75.0, "status": "underfunded",
        "required_return_pct": null, "projected_amount_at_date": null, "coverage_months": 4.5 })
    );
    assert_eq!(
        (
            &net["account_ids"],
            &net["member_ids"],
            &net["target_amount"]
        ),
        (&json!([savings]), &json!([alice]), &Value::Null)
    );

    let project = api.post("/api/goals", json!({ "name": "Voyage", "kind": "project", "target_amount": 8000, "account_ids": [savings], "member_ids": [alice, bob] })).await;
    assert_eq!(project["progress"]["status"], "completed");
    let id = project["id"].as_str().unwrap();
    let (status, updated) = api
        .call(
            "PUT",
            &format!("/api/goals/{id}"),
            Some(json!({ "target_amount": 20000, "target_date": "2020-01-01", "icon": null })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        (&updated["progress"]["status"], &updated["target_date"]),
        (&json!("overdue"), &json!("2020-01-01"))
    );
    assert_eq!(updated["member_ids"].as_array().unwrap().len(), 2);

    assert_eq!(
        error_of(
            &api.call(
                "POST",
                "/api/goals",
                Some(json!({ "name": "Flou", "kind": "project" }))
            )
            .await
        ),
        (
            400,
            "Un projet doit avoir un montant cible (target_amount) positif"
        )
    );
    assert_eq!(api.get("/api/goals").await.as_array().unwrap().len(), 2);
    assert_eq!(
        api.call("DELETE", &format!("/api/goals/{id}"), None)
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        error_of(&api.call("DELETE", &format!("/api/goals/{id}"), None).await),
        (404, "Objectif introuvable")
    );
    assert_eq!(api.get("/api/goals").await.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn bundle_round_trip_between_two_databases() {
    let (source, alice, bob) = Api::household().await;
    let (_, _) = source.house_with_loan(&alice, &bob).await;
    let pea = source
        .id_of(
            "/api/accounts",
            json!({ "name": "PEA", "kind": "investment", "envelope": "pea", "cash_balance": 500,
        "owners": [{ "member_id": alice, "share": 1 }] }),
        )
        .await;
    source
        .post(
            &format!("/api/accounts/{pea}/holdings"),
            json!({ "name": "ETF Démo", "quantity": 10, "buy_price": 20, "current_price": 25 }),
        )
        .await;
    source.post("/api/goals", json!({ "name": "Matelas", "kind": "safety_net", "monthly_expenses": 100, "coverage_months": 6,
        "account_ids": [pea], "member_ids": [alice, bob] })).await;

    let exported = source.get("/api/export").await;
    assert_eq!(exported["version"], 1);
    assert_eq!(
        exported["members"],
        json!([
            { "name": "Alice", "color": "#6366f1", "is_primary": true },
            { "name": "Bob", "color": "#ec4899", "is_primary": false },
        ])
    );
    let accounts = exported["accounts"].as_array().unwrap();
    assert_eq!(accounts.len(), 3);
    let loan = accounts.iter().find(|a| a["kind"] == "loan").unwrap();
    let house = accounts
        .iter()
        .find(|a| a["kind"] == "real_estate")
        .unwrap();
    assert_eq!(loan["loan"]["property_external_id"], house["external_id"]);
    assert_eq!(
        loan["owners"],
        json!([{ "member": "Alice", "share": 0.5 }, { "member": "Bob", "share": 0.5 }])
    );
    // Les champs nuls sont omis ; un prêt n'a pas de liste de positions.
    assert!(house.get("institution").is_none() && loan.get("holdings").is_none());
    assert_eq!(exported["goals"][0]["account_external_ids"], json!([pea]));

    // Réimport dans la même base : idempotent.
    let counts = json!({ "members": 2, "accounts": 3, "holdings": 1, "goals": 1 });
    let before = source.get("/api/overview").await;
    assert_eq!(source.post("/api/import", exported.clone()).await, counts);
    assert_eq!(source.get("/api/overview").await, before);
    assert_eq!(source.get("/api/export").await, exported);

    // Import dans une autre base, deux fois : même patrimoine, aucun doublon.
    let mut target = Api::fresh().await;
    let session = target
        .post(
            "/api/auth/setup",
            json!({ "email": "alice@exemple.test", "password": PASSWORD, "name": "Alice" }),
        )
        .await;
    target.token = session["token"].as_str().unwrap().to_string();
    assert_eq!(target.post("/api/import", exported.clone()).await, counts);
    assert_eq!(target.post("/api/import", exported.clone()).await, counts);
    let after = target.get("/api/overview").await;
    for key in [
        "gross_assets",
        "liabilities",
        "net_worth",
        "financial_assets",
        "invested",
        "gain",
    ] {
        assert!(close(&after[key], before[key].as_f64().unwrap()), "{key}");
    }
    assert_eq!(
        target.get("/api/accounts").await.as_array().unwrap().len(),
        3
    );
    assert_eq!(
        target.get("/api/members").await.as_array().unwrap().len(),
        2
    );
    let imported = target.get("/api/accounts").await;
    assert!(
        imported
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["source"] == "import")
    );
    assert_eq!(
        target.get("/api/real-estate").await["properties"][0]["loans"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let goals = target.get("/api/goals").await;
    assert_eq!(goals.as_array().unwrap().len(), 1);
    assert!(close(&goals[0]["progress"]["current_amount"], 750.0));
    // Même contenu ; seul l'ordre des comptes diffère (créés dans la même transaction, triés par nom).
    assert_eq!(
        by_external_id(target.get("/api/export").await),
        by_external_id(exported)
    );

    // Bundle invalide : rien n'est appliqué.
    let bad_version = target
        .call("POST", "/api/import", Some(json!({ "version": 2 })))
        .await;
    assert_eq!(
        error_of(&bad_version),
        (400, "Version de bundle non prise en charge (attendu : 1)")
    );
    let unknown_member = json!({ "version": 1, "members": [{ "name": "Chloé" }], "accounts": [
        { "external_id": "x-1", "name": "Livret", "kind": "savings", "owners": [{ "member": "Inconnu", "share": 1 }] }
    ] });
    assert_eq!(
        error_of(
            &target
                .call("POST", "/api/import", Some(unknown_member))
                .await
        ),
        (400, "Membre inconnu dans le bundle : Inconnu")
    );
    assert_eq!(
        target.get("/api/members").await.as_array().unwrap().len(),
        2
    );
}

#[tokio::test]
async fn connections_fx_and_cors() {
    let (api, _, _) = Api::household().await;
    assert_eq!(api.get("/api/connections/providers").await, json!([]));
    assert_eq!(api.get("/api/connections").await, json!([]));
    assert_eq!(
        error_of(
            &api.call(
                "POST",
                "/api/connections",
                Some(json!({ "provider": "demo" }))
            )
            .await
        ),
        (501, "Ce fournisseur bancaire n'est pas configuré")
    );
    let missing = "/api/connections/00000000-0000-0000-0000-000000000001";
    assert_eq!(
        error_of(&api.call("POST", &format!("{missing}/sync"), None).await),
        (404, "Connexion introuvable")
    );
    assert_eq!(
        error_of(&api.call("DELETE", missing, None).await),
        (404, "Connexion introuvable")
    );
    assert_eq!(
        api.get("/api/fx").await,
        json!({ "base": "EUR", "rates": {}, "updated_at": null })
    );

    // Pré-vol CORS : seule l'origine configurée est autorisée.
    let preflight = |origin: &'static str| {
        Request::builder()
            .method("OPTIONS")
            .uri("/api/members")
            .header(header::ORIGIN, origin)
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
            .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "authorization")
            .body(Body::empty())
            .unwrap()
    };
    let (status, headers, _) = api.send(preflight(ORIGIN)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(),
        ORIGIN
    );
    let (_, headers, _) = api.send(preflight("https://ailleurs.example")).await;
    assert!(headers.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
}
