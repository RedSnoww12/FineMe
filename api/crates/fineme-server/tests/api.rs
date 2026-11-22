//! Tests d'intégration HTTP : le routeur Axum complet, dans le processus, au-dessus d'un vrai Postgres
//! (testcontainers). Chaque test a sa propre base jetable ; toutes les données sont fictives.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use fineme_application::Settings;
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
    /// Accès direct à la base, pour préparer des données qu'aucune route ne crée (instantanés passés).
    pool: sqlx::PgPool,
    _container: ContainerAsync<Postgres>,
}

impl Api {
    /// Base vide et migrée, sans utilisateur.
    async fn fresh() -> Self {
        Self::fresh_with(Settings::default()).await
    }

    async fn fresh_with(settings: Settings) -> Self {
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
        let router = fineme_server::build_router(
            pool.clone(),
            "secret-de-test-0123456789abcdef0123",
            ORIGIN,
            1,
            settings,
            None,
        )
        .await
        .unwrap();
        Self {
            router,
            token: String::new(),
            pool,
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

    /// Connexion vue derrière le proxy : `forwarded_for` est l'en-tête `X-Forwarded-For` reçu.
    async fn login_via_proxy(
        &self,
        forwarded_for: &str,
        password: &str,
    ) -> (StatusCode, axum::http::HeaderMap, Value) {
        let body = json!({ "email": "alice@exemple.test", "password": password });
        let request = Request::builder()
            .method("POST")
            .uri("/api/auth/login")
            .header("x-forwarded-for", forwarded_for)
            .header(header::USER_AGENT, "Navigateur fictif/1.0")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        self.send(request).await
    }

    /// Déclenche la tâche quotidienne avec ce secret (sans le jeton de session).
    async fn daily(&self, secret: Option<&str>) -> (StatusCode, Value) {
        let mut request = Request::builder().method("POST").uri("/api/jobs/daily");
        if let Some(secret) = secret {
            request = request.header(header::AUTHORIZATION, format!("Bearer {secret}"));
        }
        let (status, _, body) = self.send(request.body(Body::empty()).unwrap()).await;
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
        json!({ "setup_required": true, "setup_token_required": false, "recovery_enabled": false })
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
        json!({ "setup_required": false, "setup_token_required": false, "recovery_enabled": false })
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
async fn first_setup_requires_the_configured_setup_token() {
    const CODE: &str = "code-de-configuration-fictif";
    let api = Api::fresh_with(Settings {
        setup_token: Some(CODE.into()),
        ..Settings::default()
    })
    .await;
    assert_eq!(
        api.get("/api/auth/status").await,
        json!({ "setup_required": true, "setup_token_required": true, "recovery_enabled": false })
    );

    let body = |token: Option<&str>| {
        let mut body =
            json!({ "email": "alice@exemple.test", "password": PASSWORD, "name": "Alice" });
        if let Some(token) = token {
            body["setup_token"] = json!(token);
        }
        body
    };
    for token in [None, Some("mauvais-code"), Some("")] {
        let response = api.call("POST", "/api/auth/setup", Some(body(token))).await;
        assert_eq!(
            error_of(&response),
            (403, "Code de configuration invalide ou manquant")
        );
    }
    // Rien n'a été créé par les tentatives refusées.
    assert_eq!(api.get("/api/auth/status").await["setup_required"], true);

    let (status, session) = api
        .call("POST", "/api/auth/setup", Some(body(Some(CODE))))
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(session["token"].is_string());
    assert_eq!(
        api.get("/api/auth/status").await,
        json!({ "setup_required": false, "setup_token_required": true, "recovery_enabled": false })
    );
    // Une fois l'utilisateur créé : 409 avec le bon code, 403 sans.
    let again = api
        .call("POST", "/api/auth/setup", Some(body(Some(CODE))))
        .await;
    assert_eq!(error_of(&again), (409, "Un utilisateur existe déjà"));
    assert_eq!(
        api.call("POST", "/api/auth/setup", Some(body(None)))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn changing_password_signs_out_every_other_device() {
    const NEW_PASSWORD: &str = "nouveau-mot-de-passe";
    let (mut api, _, _) = Api::household().await;
    let first_device = api.token.clone();
    let login = json!({ "email": "alice@exemple.test", "password": PASSWORD });
    let second_device = api.post("/api/auth/login", login.clone()).await["token"]
        .as_str()
        .unwrap()
        .to_string();

    let change =
        |current: &str, new: &str| json!({ "current_password": current, "new_password": new });
    // Mauvais mot de passe actuel : 403 (pas 401, la session reste ouverte).
    assert_eq!(
        error_of(
            &api.call(
                "PUT",
                "/api/auth/password",
                Some(change("mauvais-mot-de-passe", NEW_PASSWORD))
            )
            .await
        ),
        (403, "Mot de passe actuel incorrect")
    );
    assert_eq!(
        error_of(
            &api.call("PUT", "/api/auth/password", Some(change(PASSWORD, "court")))
                .await
        ),
        (400, "Le mot de passe doit contenir au moins 10 caractères")
    );
    assert_eq!(
        error_of(
            &api.call(
                "PUT",
                "/api/auth/password",
                Some(change(PASSWORD, PASSWORD))
            )
            .await
        ),
        (
            400,
            "Le nouveau mot de passe doit être différent de l'actuel"
        )
    );
    api.get("/api/auth/me").await;

    let (status, session) = api
        .call(
            "PUT",
            "/api/auth/password",
            Some(change(PASSWORD, NEW_PASSWORD)),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{session}");
    assert_eq!(session["user"]["email"], "alice@exemple.test");
    let renewed = session["token"].as_str().unwrap().to_string();

    for stale in [first_device, second_device] {
        api.token = stale;
        assert_eq!(
            error_of(&api.call("GET", "/api/members", None).await),
            (401, "Session invalide ou expirée")
        );
    }
    api.token = renewed;
    api.get("/api/members").await;

    api.token.clear();
    assert_eq!(
        api.call("POST", "/api/auth/login", Some(login)).await.0,
        StatusCode::UNAUTHORIZED
    );
    api.post(
        "/api/auth/login",
        json!({ "email": "alice@exemple.test", "password": NEW_PASSWORD }),
    )
    .await;
    assert_eq!(
        error_of(
            &api.call(
                "PUT",
                "/api/auth/password",
                Some(change(NEW_PASSWORD, PASSWORD))
            )
            .await
        ),
        (401, "Authentification requise")
    );
}

#[tokio::test]
async fn repeated_login_failures_are_throttled_per_ip_and_journaled() {
    let (mut api, _, _) = Api::household().await;
    // Le client prétend être 1.2.3.4 ; le proxy (dernière entrée) a vu 203.0.113.9.
    let attacker = "1.2.3.4, 203.0.113.9";
    for _ in 0..6 {
        let (status, _, _) = api.login_via_proxy(attacker, "mauvais-mot-de-passe").await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    let (status, headers, body) = api.login_via_proxy(attacker, PASSWORD).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(headers[header::RETRY_AFTER], "30");
    assert_eq!(
        body["error"],
        "Trop de tentatives. Réessayez dans 30 secondes."
    );
    // Changer l'entrée de tête ne contourne pas la limite.
    let (status, _, _) = api.login_via_proxy("5.6.7.8, 203.0.113.9", PASSWORD).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    // Une autre IP réelle se connecte normalement.
    let (status, _, session) = api.login_via_proxy("198.51.100.4", PASSWORD).await;
    assert_eq!(status, StatusCode::OK);
    api.token = session["token"].as_str().unwrap().to_string();

    let logins = api.get("/api/auth/logins").await;
    let logins = logins.as_array().unwrap();
    // Setup (non journalisé), 6 échecs puis 1 succès ; les refus 429 ne sont pas journalisés.
    assert_eq!(logins.len(), 7);
    assert_eq!(logins[0]["success"], true);
    assert_eq!(logins[0]["action"], "login");
    assert_eq!(logins[0]["ip"], "198.51.100.4");
    assert_eq!(logins[0]["user_agent"], "Navigateur fictif/1.0");
    assert!(logins[0]["at"].as_str().unwrap().starts_with("20"));
    assert_eq!(logins[1]["success"], false);
    assert_eq!(logins[1]["ip"], "203.0.113.9");
    assert_eq!(
        api.get("/api/auth/logins?limit=2")
            .await
            .as_array()
            .unwrap()
            .len(),
        2
    );

    // Le journal exige une session.
    api.token.clear();
    assert_eq!(
        api.call("GET", "/api/auth/logins", None).await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn refresh_renews_the_token_of_an_active_session() {
    let (mut api, _, _) = Api::household().await;
    let renewed = api.post("/api/auth/refresh", json!({})).await;
    assert_eq!(renewed["user"]["email"], "alice@exemple.test");
    api.token = renewed["token"].as_str().unwrap().to_string();
    api.get("/api/auth/me").await;
    api.token.clear();
    assert_eq!(
        api.call("POST", "/api/auth/refresh", None).await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn lost_password_is_reset_with_the_recovery_token_only() {
    const RECOVERY: &str = "code-de-secours-fictif-0123";
    const NEW_PASSWORD: &str = "nouveau-mot-de-passe";
    let reset = |token: Option<&str>, email: &str| {
        let mut body = json!({ "email": email, "new_password": NEW_PASSWORD });
        if let Some(token) = token {
            body["recovery_token"] = json!(token);
        }
        body
    };

    // Sans RECOVERY_TOKEN, la route existe mais la fonction est désactivée.
    let (api, _, _) = Api::household().await;
    assert_eq!(api.get("/api/auth/status").await["recovery_enabled"], false);
    assert_eq!(
        api.call(
            "POST",
            "/api/auth/reset",
            Some(reset(Some(RECOVERY), "alice@exemple.test"))
        )
        .await
        .0,
        StatusCode::NOT_IMPLEMENTED
    );

    let mut api = Api::fresh_with(Settings {
        recovery_token: Some(RECOVERY.into()),
        ..Settings::default()
    })
    .await;
    api.post(
        "/api/auth/setup",
        json!({ "email": "alice@exemple.test", "password": PASSWORD, "name": "Alice" }),
    )
    .await;
    assert_eq!(api.get("/api/auth/status").await["recovery_enabled"], true);
    let old_session = api
        .post(
            "/api/auth/login",
            json!({ "email": "alice@exemple.test", "password": PASSWORD }),
        )
        .await["token"]
        .as_str()
        .unwrap()
        .to_string();

    for token in [None, Some(""), Some("mauvais-code")] {
        assert_eq!(
            error_of(
                &api.call(
                    "POST",
                    "/api/auth/reset",
                    Some(reset(token, "alice@exemple.test"))
                )
                .await
            ),
            (403, "Code de secours invalide ou manquant")
        );
    }
    assert_eq!(
        error_of(
            &api.call(
                "POST",
                "/api/auth/reset",
                Some(reset(Some(RECOVERY), "inconnu@exemple.test"))
            )
            .await
        ),
        (404, "Aucun compte ne correspond à cette adresse e-mail")
    );

    let session = api
        .post(
            "/api/auth/reset",
            reset(Some(RECOVERY), "Alice@Exemple.test"),
        )
        .await;
    assert_eq!(session["user"]["email"], "alice@exemple.test");

    api.token = old_session;
    assert_eq!(
        api.call("GET", "/api/auth/me", None).await.0,
        StatusCode::UNAUTHORIZED
    );
    api.token = session["token"].as_str().unwrap().to_string();
    api.get("/api/auth/me").await;
    api.post(
        "/api/auth/login",
        json!({ "email": "alice@exemple.test", "password": NEW_PASSWORD }),
    )
    .await;
}

#[tokio::test]
async fn daily_job_requires_its_secret_and_records_todays_snapshot() {
    const CRON: &str = "secret-quotidien-fictif-0123";

    // Sans CRON_SECRET, la route existe mais la tâche est désactivée.
    let (api, _, _) = Api::household().await;
    assert_eq!(api.daily(Some(CRON)).await.0, StatusCode::NOT_IMPLEMENTED);

    let mut api = Api::fresh_with(Settings {
        cron_secret: Some(CRON.into()),
        ..Settings::default()
    })
    .await;
    let session = api
        .post(
            "/api/auth/setup",
            json!({ "email": "alice@exemple.test", "password": PASSWORD, "name": "Alice" }),
        )
        .await;
    api.token = session["token"].as_str().unwrap().to_string();
    let alice = api.get("/api/members").await[0]["id"].clone();
    let owners = json!([{ "member_id": alice, "share": 1 }]);
    api.post(
        "/api/accounts",
        json!({ "name": "Livret", "kind": "savings", "cash_balance": 1000, "owners": owners }),
    )
    .await;

    // Le jeton de session ne remplace pas le secret, et un mauvais secret est refusé.
    let session_token = api.token.clone();
    for secret in [
        None,
        Some(session_token.as_str()),
        Some("secret-quotidien-faux!"),
    ] {
        let (status, body) = api.daily(secret).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{secret:?}");
        assert!(body["error"].is_string());
    }
    assert_eq!(api.get("/api/history").await, json!({ "points": [] }));

    let (status, report) = api.daily(Some(CRON)).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["prices"], json!({ "updated": 0, "failed": [] }));
    // Les alertes sont évaluées après l'instantané (livret loin de son plafond : aucune).
    assert_eq!(
        report["alerts"],
        json!({ "created": 0, "reopened": 0, "resolved": 0 })
    );
    let points = api.get("/api/history").await["points"].clone();
    assert_eq!(points.as_array().unwrap().len(), 1);
    assert_eq!(points[0]["date"], report["snapshot_date"]);
    assert!(close(&points[0]["net_worth"], 1000.0));
}

#[tokio::test]
async fn management_fee_is_saved_exported_and_cleared() {
    let (api, alice, _) = Api::household().await;
    let owners = json!([{ "member_id": alice, "share": 1 }]);
    let created = api
        .post(
            "/api/accounts",
            json!({ "name": "Assurance-vie", "kind": "investment", "envelope": "assurance_vie",
                    "management_fee_pct": 0.6, "owners": owners }),
        )
        .await;
    assert_eq!(created["management_fee_pct"], json!(0.6));
    let id = created["id"].as_str().unwrap().to_string();
    assert_eq!(
        api.get("/api/accounts").await[0]["management_fee_pct"],
        json!(0.6)
    );

    let exported = api.get("/api/export").await;
    let account = exported["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == "Assurance-vie")
        .unwrap();
    assert_eq!(account["management_fee_pct"], json!(0.6));

    let (status, cleared) = api
        .call(
            "PUT",
            &format!("/api/accounts/{id}"),
            Some(json!({ "management_fee_pct": null })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{cleared}");
    assert_eq!(cleared["management_fee_pct"], Value::Null);
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
        (
            json!({ "name": "X", "kind": "investment", "management_fee_pct": 101, "owners": owners }),
            "Les frais de gestion doivent être compris entre 0 et 100 %",
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
    // Un cours saisi n'est jamais signalé comme ancien, et n'entre pas dans l'historique des cours.
    assert_eq!(holding["price_stale"], false);
    assert!(
        close(&holding["value"], 2500.0)
            && close(&holding["gain"], 500.0)
            && close(&holding["gain_pct"], 25.0)
    );

    let id = holding["id"].as_str().unwrap();
    assert_eq!(
        api.get(&format!("/api/holdings/{id}/prices?range=1y"))
            .await,
        json!({ "points": [] })
    );
    assert_eq!(
        api.call("GET", &format!("/api/holdings/{id}/prices?range=5y"), None)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        api.call(
            "GET",
            "/api/holdings/00000000-0000-4000-8000-000000000000/prices",
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
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
    assert_eq!(points[0].as_object().unwrap().len(), 5);
    // Détail par compte, trié par nom, à la part détenue par la vue.
    assert_eq!(
        points[0]["accounts"],
        json!([
            { "account_id": points[0]["accounts"][0]["account_id"], "name": "Joint", "kind": "checking", "value": 800.0, "gain": null },
            { "account_id": points[0]["accounts"][1]["account_id"], "name": "Maison", "kind": "real_estate", "value": 300000.0, "gain": null },
            { "account_id": pea, "name": "PEA", "kind": "investment", "value": 750.0, "gain": 50.0 },
            { "account_id": points[0]["accounts"][3]["account_id"], "name": "Prêt maison", "kind": "loan", "value": 200000.0, "gain": null },
        ])
    );
    let bob_history = api.get(&format!("/api/history?member_id={bob}")).await;
    assert!(close(&bob_history["points"][0]["net_worth"], 50_200.0));
    let bob_accounts: Vec<(&str, f64)> = bob_history["points"][0]["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| (a["name"].as_str().unwrap(), a["value"].as_f64().unwrap()))
        .collect();
    assert_eq!(
        bob_accounts,
        vec![
            ("Joint", 200.0),
            ("Maison", 150_000.0),
            ("Prêt maison", 100_000.0)
        ]
    );
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

/// Instantané du foyer `days_ago` jours avant aujourd'hui (selon Postgres), sans dette.
async fn seed_household_snapshot(api: &Api, days_ago: i32, net_worth: f64) {
    sqlx::query(
        "INSERT INTO snapshots (date, member_id, gross_assets, liabilities, net_worth, detailed) \
         VALUES (CURRENT_DATE - $1, NULL, $2, 0, $2, true)",
    )
    .bind(days_ago)
    .bind(net_worth)
    .execute(&api.pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn history_cleanup_routes_review_correct_delete_recompute_and_reset() {
    let (mut api, alice, _) = Api::household().await;
    api.post(
        "/api/accounts",
        json!({ "name": "Livret", "kind": "savings", "cash_balance": 100_000,
            "owners": [{ "member_id": alice, "share": 1 }] }),
    )
    .await;
    // Quatre jours passés, dont un pic aberrant il y a 3 jours, puis l'instantané du jour.
    for (days_ago, net) in [
        (4, 100_000.0),
        (3, 10_000.0),
        (2, 100_500.0),
        (1, 101_000.0),
    ] {
        seed_household_snapshot(&api, days_ago, net).await;
    }
    let (status, recomputed) = api.call("POST", "/api/history/recompute", None).await;
    assert_eq!(status, StatusCode::OK, "{recomputed}");
    let today = recomputed["snapshot_date"].as_str().unwrap().to_string();

    let review = api.get("/api/history/points").await;
    assert!(close(&review["threshold"], 0.2));
    let points = review["points"].as_array().unwrap();
    assert_eq!(points.len(), 5);
    let flagged: Vec<bool> = points
        .iter()
        .map(|p| p["suspicious"].as_bool().unwrap())
        .collect();
    assert_eq!(flagged, vec![false, true, false, false, false]);
    assert_eq!(points[0]["change"], Value::Null);
    assert_eq!(points[4]["date"], today.as_str());
    assert_eq!(points[4]["detailed"], true);
    let spike = points[1]["date"].as_str().unwrap().to_string();
    let first = points[0]["date"].as_str().unwrap().to_string();
    let yesterday = points[3]["date"].as_str().unwrap().to_string();

    // Paramètres invalides.
    let amounts = json!({ "gross_assets": 1, "liabilities": 0 });
    assert_eq!(
        error_of(
            &api.call("GET", "/api/history/points?threshold=0", None)
                .await
        )
        .0,
        400
    );
    assert_eq!(
        error_of(
            &api.call("PUT", "/api/history/points/hier", Some(amounts.clone()))
                .await
        ),
        (400, "Date invalide dans l'URL (AAAA-MM-JJ attendu)")
    );
    let today_path = format!("/api/history/points/{today}");
    assert_eq!(
        error_of(&api.call("PUT", &today_path, Some(amounts.clone())).await).0,
        400
    );
    assert_eq!(
        error_of(
            &api.call("PUT", "/api/history/points/2001-01-01", Some(amounts))
                .await
        ),
        (404, "Aucun point d'historique à cette date pour cette vue")
    );

    // Correction du pic : le point n'est plus suspect et le graphique lit la valeur saisie.
    let (status, corrected) = api
        .call(
            "PUT",
            &format!("/api/history/points/{spike}"),
            Some(json!({ "gross_assets": 100_300, "liabilities": 100 })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{corrected}");
    assert_eq!(corrected["corrected"], true);
    assert_eq!(corrected["detailed"], false);
    assert!(close(&corrected["net_worth"], 100_200.0));
    let history = api.get("/api/history").await;
    assert!(close(&history["points"][1]["net_worth"], 100_200.0));
    assert_eq!(history["points"][1]["accounts"], Value::Null);
    let review = api.get("/api/history/points").await;
    assert!(
        review["points"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["suspicious"] == false)
    );

    // Suppression d'un point, puis d'une plage.
    let reversed = format!("/api/history/points?from={yesterday}&to={first}");
    assert_eq!(error_of(&api.call("DELETE", &reversed, None).await).0, 400);
    let one = format!("/api/history/points?from={first}&to={first}");
    assert_eq!(
        api.call("DELETE", &one, None).await,
        (StatusCode::OK, json!({ "deleted_days": 1 }))
    );
    let range = format!("/api/history/points?from={spike}&to={yesterday}");
    assert_eq!(
        api.call("DELETE", &range, None).await,
        (StatusCode::OK, json!({ "deleted_days": 3 }))
    );
    let remaining = api.get("/api/history").await["points"].clone();
    assert_eq!(remaining.as_array().unwrap().len(), 1);

    // Remise à zéro : refusée sans le mot exact.
    let refused = api
        .call(
            "POST",
            "/api/history/reset",
            Some(json!({ "confirmation": "oui" })),
        )
        .await;
    assert_eq!(
        error_of(&refused),
        (
            400,
            "Confirmation manquante : saisissez EFFACER pour supprimer tout l'historique"
        )
    );
    assert_eq!(
        api.post("/api/history/reset", json!({ "confirmation": "EFFACER" }))
            .await,
        json!({ "deleted_days": 1 })
    );
    assert_eq!(api.get("/api/history").await, json!({ "points": [] }));

    // Toutes ces routes exigent le JWT.
    api.token.clear();
    for (method, path) in [
        ("GET", "/api/history/points"),
        ("PUT", "/api/history/points/2026-01-01"),
        (
            "DELETE",
            "/api/history/points?from=2026-01-01&to=2026-01-02",
        ),
        ("POST", "/api/history/recompute"),
        ("POST", "/api/history/reset"),
    ] {
        assert_eq!(
            error_of(&api.call(method, path, None).await),
            (401, "Authentification requise"),
            "{method} {path}"
        );
    }
}

#[tokio::test]
async fn budget_flow() {
    let (api, alice, bob) = Api::household().await;
    let joint = json!([{ "member_id": alice, "share": 0.5 }, { "member_id": bob, "share": 0.5 }]);
    let checking = api
        .id_of(
            "/api/accounts",
            json!({ "name": "Joint", "kind": "checking", "owners": joint }),
        )
        .await;
    let livret = api
        .id_of(
            "/api/accounts",
            json!({ "name": "Livret", "kind": "savings",
        "owners": [{ "member_id": alice, "share": 1 }] }),
        )
        .await;
    let batch = json!([
        { "date": "2026-01-01", "label": "VIR SALAIRE ACME", "amount": 3000 },
        { "date": "2026-01-04", "label": "PRLV LOYER", "amount": -1000 },
        { "date": "2026-01-10", "label": "CB LIBRAIRIE 1001", "amount": -40 },
        { "date": "2026-01-20", "label": "VIR VERS LIVRET", "amount": -500 }
    ]);
    api.post(
        &format!("/api/accounts/{checking}/transactions/bulk"),
        json!({ "transactions": batch }),
    )
    .await;
    api.post(
        &format!("/api/accounts/{livret}/transactions/bulk"),
        json!({ "transactions": [{ "date": "2026-01-21", "label": "VIR RECU", "amount": 500 }] }),
    )
    .await;

    let month = api.get("/api/budget?month=2026-01").await;
    assert_eq!(
        (
            &month["month"],
            &month["income"],
            &month["expenses"],
            &month["savings"],
            &month["savings_rate"]
        ),
        (
            &json!("2026-01"),
            &json!(3000.0),
            &json!(1040.0),
            &json!(1960.0),
            &json!(1960.0 / 3000.0)
        )
    );
    assert_eq!(month["internal_transfers"], 2);
    assert_eq!(month["trend"].as_array().unwrap().len(), 12);
    assert_eq!(
        month["categories"][0],
        json!({ "category": "Logement", "amount": 1000.0, "target": null })
    );
    assert_eq!(month["categories"][1]["category"], Value::Null);

    // Recatégorisation manuelle : la règle s'applique aux transactions suivantes de même libellé.
    let items = api.get("/api/budget/transactions?month=2026-01").await;
    let librairie = items
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["label"] == "CB LIBRAIRIE 1001")
        .unwrap();
    assert_eq!(librairie["category_source"], Value::Null);
    let id = librairie["id"].as_str().unwrap();
    let (status, updated) = api
        .call(
            "PUT",
            &format!("/api/transactions/{id}"),
            Some(json!({ "category": "Culture" })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(
        (&updated["manual_category"], &updated["internal_transfer"]),
        (&json!("Culture"), &Value::Null)
    );
    api.post(&format!("/api/accounts/{checking}/transactions/bulk"),
        json!({ "transactions": [{ "date": "2026-01-25", "label": "CB LIBRAIRIE 2501", "amount": -10 }] })).await;
    let items = api
        .get(&format!(
            "/api/budget/transactions?month=2026-01&member_id={bob}"
        ))
        .await;
    let later = items
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["label"] == "CB LIBRAIRIE 2501")
        .unwrap();
    assert_eq!(
        (
            &later["category"],
            &later["category_source"],
            &later["amount_eur"]
        ),
        (&json!("Culture"), &json!("rule"), &json!(-5.0))
    );
    // Bob ne voit pas le livret d'Alice.
    assert!(
        items
            .as_array()
            .unwrap()
            .iter()
            .all(|t| t["account_name"] == "Joint")
    );
    let rules = api.get("/api/budget/rules").await;
    assert_eq!(
        (&rules[0]["pattern"], &rules[0]["category"]),
        (&json!("cb librairie"), &json!("Culture"))
    );
    assert!(
        api.get("/api/budget/categories")
            .await
            .as_array()
            .unwrap()
            .contains(&json!("Culture"))
    );

    // Marquage manuel d'un virement interne.
    let loyer = items
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["label"] == "PRLV LOYER")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    api.call(
        "PUT",
        &format!("/api/transactions/{loyer}"),
        Some(json!({ "internal_transfer": true })),
    )
    .await;
    let month = api.get("/api/budget?month=2026-01").await;
    assert_eq!(month["expenses"], json!(50.0));

    // Cibles.
    let (status, targets) = api
        .call(
            "PUT",
            "/api/budget/targets",
            Some(json!({ "category": "Culture", "monthly_amount": 30 })),
        )
        .await;
    assert_eq!(
        (status, targets),
        (
            StatusCode::OK,
            json!([{ "category": "Culture", "monthly_amount": 30.0 }])
        )
    );
    let month = api.get("/api/budget?month=2026-01").await;
    assert_eq!(
        month["categories"][0],
        json!({ "category": "Culture", "amount": 50.0, "target": 30.0 })
    );
    assert_eq!(
        api.call(
            "PUT",
            "/api/budget/targets",
            Some(json!({ "category": "Culture", "monthly_amount": null }))
        )
        .await
        .1,
        json!([])
    );

    // Erreurs.
    let response = api.call("GET", "/api/budget?month=janvier", None).await;
    assert_eq!(
        error_of(&response),
        (400, "Mois invalide (format AAAA-MM attendu)")
    );
    let response = api
        .call(
            "PUT",
            "/api/budget/targets",
            Some(json!({ "category": "Culture", "monthly_amount": -1 })),
        )
        .await;
    assert_eq!(error_of(&response).0, 400);
    let unknown = "00000000-0000-0000-0000-000000000000";
    let response = api
        .call(
            "PUT",
            &format!("/api/transactions/{unknown}"),
            Some(json!({})),
        )
        .await;
    assert_eq!(error_of(&response).0, 404);
    let rule = rules[0]["id"].as_str().unwrap();
    assert_eq!(
        api.call("DELETE", &format!("/api/budget/rules/{rule}"), None)
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        api.call("DELETE", &format!("/api/budget/rules/{rule}"), None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn investment_operations_drive_positions_and_yearly_summary() {
    let (api, alice, _) = Api::household().await;
    let owners = json!([{ "member_id": alice, "share": 1 }]);
    let pea = api
        .id_of(
            "/api/accounts",
            json!({ "name": "PEA", "kind": "investment", "owners": owners }),
        )
        .await;
    let etf = api
        .id_of(
            &format!("/api/accounts/{pea}/holdings"),
            json!({ "name": "ETF Démo", "quantity": 0, "current_price": 140 }),
        )
        .await;
    let ops = format!("/api/accounts/{pea}/operations");

    for body in [
        json!({ "kind": "deposit", "date": "2025-01-02", "amount": 3000 }),
        json!({ "kind": "buy", "date": "2025-01-10", "holding_id": etf, "quantity": 10, "unit_price": 100, "fees": 5 }),
        json!({ "kind": "buy", "date": "2025-03-10", "holding_id": etf, "quantity": 10, "unit_price": 130, "fees": 5 }),
        json!({ "kind": "dividend", "date": "2025-07-01", "holding_id": etf, "amount": 12.5, "note": "Coupon" }),
    ] {
        let (status, created) = api.call("POST", &ops, Some(body)).await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
    }
    let sale = api
        .post(&ops, json!({ "kind": "sell", "date": "2025-06-01", "holding_id": etf, "quantity": 8, "unit_price": 150, "fees": 4 }))
        .await;
    assert_eq!(
        (&sale["kind"], &sale["holding_name"], &sale["currency"]),
        (&json!("sell"), &json!("ETF Démo"), &json!("EUR"))
    );
    assert!(close(&sale["amount"], 1200.0) && close(&sale["realized_gain"], 272.0));

    // La position suit le journal : 12 parts au PRU de 115,50 €, cours inchangé.
    let detail = api.get(&format!("/api/accounts/{pea}")).await;
    let holding = &detail["holdings"][0];
    assert!(close(&holding["quantity"], 12.0) && close(&holding["buy_price"], 115.5));
    assert!(close(&holding["value"], 1680.0));

    // Le formulaire de la position ne peut plus changer la quantité.
    let (status, _) = api
        .call(
            "PUT",
            &format!("/api/holdings/{etf}"),
            Some(json!({ "quantity": 1 })),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let list = api.get(&ops).await;
    let kinds: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|op| op["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, vec!["dividend", "sell", "buy", "buy", "deposit"]);
    assert_eq!(list[0]["note"], "Coupon");
    assert_eq!(list[4]["holding_id"], Value::Null);

    let summary = api.get(&format!("{ops}/summary")).await;
    assert_eq!(summary["currency"], "EUR");
    let year = &summary["years"][0];
    assert_eq!(year["year"], 2025);
    assert!(close(&year["realized_gain"], 272.0) && close(&year["dividends"], 12.5));
    assert!(close(&year["fees"], 14.0) && close(&year["deposits"], 3000.0));

    // Modifier la vente (15 au lieu de 8) rend chaque achat indispensable : en supprimer un est refusé.
    let sale_id = sale["id"].as_str().unwrap();
    let (status, edited) = api
        .call("PUT", &format!("/api/operations/{sale_id}"), Some(json!({
            "kind": "sell", "date": "2025-06-01", "holding_id": etf, "quantity": 15, "unit_price": 150, "fees": 4
        })))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(close(&edited["realized_gain"], 513.5));
    let first_buy = list[3]["id"].as_str().unwrap();
    let refused = api
        .call("DELETE", &format!("/api/operations/{first_buy}"), None)
        .await;
    assert_eq!(error_of(&refused).0, 400);
    assert!(error_of(&refused).1.contains("Vente impossible"));
    assert_eq!(
        api.call("DELETE", &format!("/api/operations/{sale_id}"), None)
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        api.call("DELETE", &format!("/api/operations/{sale_id}"), None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let holding = &api.get(&format!("/api/accounts/{pea}")).await["holdings"][0];
    assert!(close(&holding["quantity"], 20.0));

    // Validation : vente sans position, type inconnu.
    let missing = api
        .call(
            "POST",
            &ops,
            Some(json!({ "kind": "sell", "date": "2025-01-01", "quantity": 1, "unit_price": 1 })),
        )
        .await;
    assert_eq!(error_of(&missing).0, 400);
    let unknown = api
        .call(
            "POST",
            &ops,
            Some(json!({ "kind": "transfer", "date": "2025-01-01", "amount": 1 })),
        )
        .await;
    assert_eq!(error_of(&unknown).0, 400);
}

#[tokio::test]
async fn alerts_are_evaluated_deduplicated_read_archived_and_configured() {
    let (api, alice, _) = Api::household().await;
    let owners = json!([{ "member_id": alice, "share": 1 }]);
    api.post(
        "/api/accounts",
        json!({ "name": "Livret Fictif", "kind": "savings", "cash_balance": 950, "owners": owners }),
    )
    .await;
    assert_eq!(api.get("/api/alerts").await, json!([]));

    // Plafond abaissé à 1 000 € : le livret à 950 € dépasse le seuil de 90 %.
    let (status, rule) = api
        .call(
            "PUT",
            "/api/alerts/rules/savings_ceiling",
            Some(json!({ "ceiling": 1000 })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{rule}");
    assert_eq!(
        rule,
        json!({ "kind": "savings_ceiling", "enabled": true, "threshold_pct": 90.0, "ceiling": 1000.0 })
    );
    let report = api.post("/api/alerts/evaluate", json!({})).await;
    assert_eq!(
        report,
        json!({ "created": 1, "reopened": 0, "resolved": 0 })
    );
    // Réévaluée, la même condition ne crée pas de seconde alerte.
    assert_eq!(
        api.post("/api/alerts/evaluate", json!({})).await["created"],
        0
    );
    let alerts = api.get("/api/alerts").await;
    assert_eq!(alerts.as_array().unwrap().len(), 1);
    let alert = &alerts[0];
    assert_eq!(alert["kind"], "savings_ceiling");
    assert_eq!(alert["severity"], "warning");
    assert_eq!(
        (&alert["read"], &alert["archived"], &alert["active"]),
        (&json!(false), &json!(false), &json!(true))
    );
    assert!(alert["message"].as_str().unwrap().contains("95 %"));

    let path = format!("/api/alerts/{}", alert["id"].as_str().unwrap());
    let (status, read) = api.call("PUT", &path, Some(json!({ "read": true }))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(read["read"], true);
    assert_eq!(
        api.post("/api/alerts/read-all", json!({})).await,
        json!({ "updated": 0 })
    );
    api.call("PUT", &path, Some(json!({ "archived": true })))
        .await;
    assert_eq!(api.get("/api/alerts").await, json!([]));
    assert_eq!(
        api.get("/api/alerts?archived=true").await[0]["archived"],
        true
    );

    // Désactivée, la règle n'observe plus la condition : l'alerte est résolue.
    api.call(
        "PUT",
        "/api/alerts/rules/savings_ceiling",
        Some(json!({ "enabled": false })),
    )
    .await;
    assert_eq!(
        api.post("/api/alerts/evaluate", json!({})).await["resolved"],
        1
    );
    let rules = api.get("/api/alerts/rules").await;
    let rules = rules.as_array().unwrap();
    assert_eq!(rules.len(), 5);
    assert!(
        rules
            .iter()
            .any(|rule| rule["kind"] == "savings_ceiling" && rule["enabled"] == false)
    );

    let invalid = api
        .call(
            "PUT",
            "/api/alerts/rules/savings_ceiling",
            Some(json!({ "threshold_pct": 0 })),
        )
        .await;
    assert_eq!(error_of(&invalid).0, 400);
    let unknown = api
        .call("PUT", "/api/alerts/rules/inconnue", Some(json!({})))
        .await;
    assert_eq!(error_of(&unknown).0, 404);
    let missing = api
        .call(
            "PUT",
            "/api/alerts/00000000-0000-0000-0000-000000000000",
            Some(json!({ "read": true })),
        )
        .await;
    assert_eq!(error_of(&missing), (404, "Alerte introuvable"));
}

/// Instantané détaillé du foyer, `days_ago` jours avant aujourd'hui, pour un seul compte.
async fn seed_account_snapshot(api: &Api, days_ago: i32, account: &str, value: f64) {
    sqlx::query(
        "INSERT INTO snapshots (date, member_id, gross_assets, liabilities, net_worth, detailed) \
         VALUES (CURRENT_DATE - $1, NULL, $2, 0, $2, true)",
    )
    .bind(days_ago)
    .bind(value)
    .execute(&api.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO snapshot_accounts (date, member_id, account_id, name, kind, value) \
         VALUES (CURRENT_DATE - $1, NULL, CAST($2 AS UUID), 'PEA', 'investment', $3)",
    )
    .bind(days_ago)
    .bind(account)
    .bind(value)
    .execute(&api.pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn performance_measures_twr_and_reports_a_missing_benchmark_history() {
    let (api, alice, _) = Api::household().await;
    let owners = json!([{ "member_id": alice, "share": 1 }]);
    let pea = api
        .id_of(
            "/api/accounts",
            json!({ "name": "PEA", "kind": "investment", "owners": owners }),
        )
        .await;

    // Aucun historique : rien à mesurer, sans erreur.
    let empty = api.get("/api/performance").await;
    assert_eq!(empty["portfolio"], Value::Null);
    assert_eq!(empty["accounts"], json!([]));

    // 1 000 → 2 100 (versement de 1 000 il y a 10 jours) → 2 310 : deux fois +10 %.
    let deposit_day: String = sqlx::query_scalar("SELECT (CURRENT_DATE - 10)::text")
        .fetch_one(&api.pool)
        .await
        .unwrap();
    api.post(
        &format!("/api/accounts/{pea}/operations"),
        json!({ "kind": "deposit", "date": deposit_day, "amount": 1000 }),
    )
    .await;
    for (days_ago, value) in [(20, 1000.0), (10, 2100.0), (0, 2310.0)] {
        seed_account_snapshot(&api, days_ago, &pea, value).await;
    }
    let perf = api.get("/api/performance").await;
    let portfolio = &perf["portfolio"];
    assert!(close(&portfolio["twr"], 0.21), "{perf}");
    assert!(close(&portfolio["net_flows"], 1000.0));
    assert_eq!(portfolio["twr_annualized"], Value::Null);
    assert_eq!(portfolio["points"].as_array().unwrap().len(), 3);
    assert!(close(&portfolio["points"][2]["value"], 121.0));
    assert_eq!(perf["accounts"][0]["account_id"], json!(pea));
    assert_eq!(perf["benchmark"], Value::Null);

    // Indice crypto sans ticker : aucun fournisseur ne peut le coter (pas d'appel réseau),
    // il est enregistré sans cours, et c'est dit.
    assert_eq!(api.get("/api/performance/benchmark").await, Value::Null);
    let (status, info) = api
        .call(
            "PUT",
            "/api/performance/benchmark",
            Some(json!({ "name": "Indice démo", "isin": "XX0000DEMO01", "kind": "crypto", "currency": "EUR" })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{info}");
    assert_eq!(info["prices"], json!(0));
    assert!(info["last_error"].is_string());
    let perf = api.get("/api/performance").await;
    assert_eq!(perf["benchmark"]["comparison"], Value::Null);
    assert!(
        perf["benchmark"]["message"]
            .as_str()
            .unwrap()
            .contains("Aucun cours")
    );

    let (status, error) = api
        .call(
            "PUT",
            "/api/performance/benchmark",
            Some(json!({ "name": "Sans identifiant" })),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
    let (status, _) = api
        .call(
            "GET",
            "/api/performance?from=2026-02-01&to=2026-01-01",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = api.call("DELETE", "/api/performance/benchmark", None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = api.call("DELETE", "/api/performance/benchmark", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn operations_csv_import_reports_rows_and_ignores_a_second_import() {
    let (api, alice, _) = Api::household().await;
    let owners = json!([{ "member_id": alice, "share": 1 }]);
    let cto = api
        .id_of(
            "/api/accounts",
            json!({ "name": "CTO", "kind": "investment", "owners": owners }),
        )
        .await;
    let bulk = format!("/api/accounts/{cto}/operations/bulk");
    let body = json!({ "operations": [
        { "kind": "buy", "date": "2025-02-03", "security": { "name": "ETF Fictif", "isin": "fr0000000003" },
          "quantity": 4, "unit_price": 25, "fees": 1 },
        { "kind": "dividend", "date": "2025-05-02", "security": { "name": "etf fictif" }, "amount": 2.4 },
        { "kind": "sell", "date": "2025-06-01", "security": { "name": "ETF Fictif" }, "quantity": 10, "unit_price": 30 },
        { "kind": "deposit", "date": "2025-01-02", "amount": 200 }
    ] });
    let report = api.post(&bulk, body.clone()).await;
    assert_eq!(
        (&report["inserted"], &report["skipped"]),
        (&json!(3), &json!(0))
    );
    assert_eq!(report["errors"][0]["index"], 2);
    assert!(
        report["errors"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("Vente impossible")
    );

    let detail = api.get(&format!("/api/accounts/{cto}")).await;
    let holding = &detail["holdings"][0];
    assert_eq!(
        (&holding["name"], &holding["isin"]),
        (&json!("ETF Fictif"), &json!("FR0000000003"))
    );
    assert!(close(&holding["quantity"], 4.0) && close(&holding["buy_price"], 25.25));
    let list = api.get(&format!("/api/accounts/{cto}/operations")).await;
    assert_eq!(list[0]["holding_name"], "ETF Fictif");

    let again = api.post(&bulk, body).await;
    assert_eq!(
        (&again["inserted"], &again["skipped"]),
        (&json!(0), &json!(3))
    );

    let (missing_status, _) = api
        .call(
            "POST",
            &format!(
                "/api/accounts/{}/operations/bulk",
                "00000000-0000-4000-8000-000000000000"
            ),
            Some(json!({ "operations": [] })),
        )
        .await;
    assert_eq!(missing_status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn import_profiles_are_remembered_per_account_and_kind() {
    let (api, alice, _) = Api::household().await;
    let owners = json!([{ "member_id": alice, "share": 1 }]);
    let checking = api
        .id_of(
            "/api/accounts",
            json!({ "name": "Courant", "kind": "checking", "owners": owners }),
        )
        .await;
    let profiles = format!("/api/accounts/{checking}/import-profiles");
    assert_eq!(api.get(&profiles).await, json!([]));
    let (status, saved) = api
        .call(
            "PUT",
            &format!("{profiles}/transactions"),
            Some(json!({
                "template": "banque-releve-debit-credit",
                "columns": { "date": "Date opération", "label": "Libellé", "debit": "Débit", "credit": "Crédit" }
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["columns"]["debit"], "Débit");
    let list = api.get(&profiles).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["kind"], "transactions");
    assert_eq!(list[0]["template"], "banque-releve-debit-credit");
    assert!(list[0]["updated_at"].is_string());

    let unknown_field = api
        .call(
            "PUT",
            &format!("{profiles}/holdings"),
            Some(json!({ "columns": { "label": "Libellé" } })),
        )
        .await;
    assert_eq!(error_of(&unknown_field).0, 400);
    let unknown_kind = api
        .call(
            "PUT",
            &format!("{profiles}/budget"),
            Some(json!({ "columns": { "date": "Date" } })),
        )
        .await;
    assert_eq!(error_of(&unknown_kind).0, 400);
    let missing = api
        .call(
            "GET",
            &format!(
                "/api/accounts/{}/import-profiles",
                "00000000-0000-4000-8000-000000000000"
            ),
            None,
        )
        .await;
    assert_eq!(error_of(&missing).0, 404);
}

// ---------------------------------------------------------------- second facteur

/// Décode un secret base32 (RFC 4648, sans remplissage).
fn base32_decode(text: &str) -> Vec<u8> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let (mut buffer, mut bits, mut out) = (0u32, 0u32, Vec::new());
    for byte in text.bytes() {
        let value = ALPHABET.iter().position(|c| *c == byte).unwrap() as u32;
        buffer = (buffer << 5) | value;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    out
}

/// Code TOTP du pas courant décalé de `offset` pas, calculé comme une application d'authentification.
fn totp_code(secret_base32: &str, offset: i64) -> String {
    use fineme_application::ports::TotpSupport;
    use fineme_domain::totp::{counter_message, time_step, truncate};
    let step = time_step(chrono::Utc::now()) + offset;
    let mac = fineme_infra::totp::TotpCrypto::new("inutile")
        .hmac_sha1(&base32_decode(secret_base32), &counter_message(step));
    format!("{:06}", truncate(&mac))
}

impl Api {
    /// Requête publique vue derrière le proxy depuis l'IP `ip`.
    async fn public_post(
        &self,
        ip: &str,
        path: &str,
        body: Value,
    ) -> (StatusCode, axum::http::HeaderMap, Value) {
        let request = Request::builder()
            .method("POST")
            .uri(path)
            .header("x-forwarded-for", ip)
            .header(header::USER_AGENT, "Navigateur fictif/1.0")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        self.send(request).await
    }
}

#[tokio::test]
async fn second_factor_is_enabled_then_required_at_login_then_disabled() {
    let (mut api, _, _) = Api::household().await;
    let status = api.get("/api/auth/totp").await;
    assert_eq!(
        status,
        json!({ "enabled": false, "recovery_codes_remaining": 0 })
    );

    let enrollment = api.post("/api/auth/totp/setup", json!({})).await;
    let secret = enrollment["secret"].as_str().unwrap().to_string();
    assert!(
        enrollment["otpauth_uri"]
            .as_str()
            .unwrap()
            .starts_with("otpauth://totp/FineMe:alice%40exemple.test?secret=")
    );
    assert!(enrollment["qr_svg"].as_str().unwrap().contains("<svg"));
    // Le secret est chiffré au repos.
    let stored: Option<String> = sqlx::query_scalar("SELECT totp_pending_secret FROM users")
        .fetch_one(&api.pool)
        .await
        .unwrap();
    assert!(!stored.unwrap().contains(&secret));

    let (status, _) = api
        .call(
            "POST",
            "/api/auth/totp/enable",
            Some(json!({ "code": "abc" })),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let old_token = api.token.clone();
    let activation = api
        .post(
            "/api/auth/totp/enable",
            json!({ "code": totp_code(&secret, 0) }),
        )
        .await;
    let recovery: Vec<String> =
        serde_json::from_value(activation["recovery_codes"].clone()).unwrap();
    assert_eq!(recovery.len(), 10);
    // L'activation ferme les autres sessions.
    api.token = old_token;
    assert_eq!(
        api.call("GET", "/api/auth/me", None).await.0,
        StatusCode::UNAUTHORIZED
    );
    api.token = activation["token"].as_str().unwrap().to_string();
    assert_eq!(
        api.get("/api/auth/totp").await["recovery_codes_remaining"],
        10
    );

    // Connexion en deux étapes.
    let ip = "198.51.100.7";
    let (status, _, step) = api.login_via_proxy(ip, PASSWORD).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(step["totp_required"], true);
    assert_eq!(step["expires_in"], 300);
    assert!(step.get("token").is_none());
    let challenge = step["challenge"].as_str().unwrap().to_string();
    // Le défi n'ouvre aucune route protégée.
    let session_token = std::mem::replace(&mut api.token, challenge.clone());
    assert_eq!(
        api.call("GET", "/api/auth/me", None).await.0,
        StatusCode::UNAUTHORIZED
    );
    api.token = session_token;
    // Le code de l'activation est déjà utilisé : rejeu refusé.
    let replay = json!({ "challenge": challenge, "code": totp_code(&secret, 0) });
    let (status, _, _) = api.public_post(ip, "/api/auth/login/totp", replay).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let next = json!({ "challenge": challenge, "code": totp_code(&secret, 1) });
    let (status, _, session) = api.public_post(ip, "/api/auth/login/totp", next).await;
    assert_eq!(status, StatusCode::OK, "{session}");
    api.token = session["token"].as_str().unwrap().to_string();
    let logins = api.get("/api/auth/logins").await;
    assert_eq!(logins[0]["action"], "totp");
    assert_eq!(logins[0]["success"], true);
    assert_eq!(logins[1]["action"], "totp");
    assert_eq!(logins[1]["success"], false);

    // Code de secours : une seule fois.
    let (_, _, step) = api.login_via_proxy(ip, PASSWORD).await;
    let with_recovery = json!({ "challenge": step["challenge"], "code": recovery[0] });
    let (status, _, _) = api
        .public_post(ip, "/api/auth/login/totp", with_recovery.clone())
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = api
        .public_post(ip, "/api/auth/login/totp", with_recovery)
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Désactivation : mot de passe et code exigés.
    let (status, _) = api
        .call(
            "POST",
            "/api/auth/totp/disable",
            Some(json!({ "password": "mauvais-mot-de-passe", "code": recovery[1] })),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let disabled = api
        .post(
            "/api/auth/totp/disable",
            json!({ "password": PASSWORD, "code": recovery[1] }),
        )
        .await;
    api.token = disabled["token"].as_str().unwrap().to_string();
    assert_eq!(api.get("/api/auth/totp").await["enabled"], false);
    let (_, _, session) = api.login_via_proxy(ip, PASSWORD).await;
    assert!(session["token"].is_string());
}

#[tokio::test]
async fn wrong_second_factor_codes_are_throttled() {
    let (api, _, _) = Api::household().await;
    let secret = api.post("/api/auth/totp/setup", json!({})).await["secret"]
        .as_str()
        .unwrap()
        .to_string();
    let activation = api
        .post(
            "/api/auth/totp/enable",
            json!({ "code": totp_code(&secret, 0) }),
        )
        .await;
    assert!(activation["token"].is_string());

    let attacker = "203.0.113.9";
    let (_, _, step) = api.login_via_proxy(attacker, PASSWORD).await;
    let valid: Vec<String> = (-1..=2).map(|offset| totp_code(&secret, offset)).collect();
    let wrong = (0..10)
        .map(|n| n.to_string().repeat(6))
        .find(|code| !valid.contains(code))
        .unwrap();
    let attempt = json!({ "challenge": step["challenge"], "code": wrong });
    for attempt_number in 1..=6 {
        let (status, _, _) = api
            .public_post(attacker, "/api/auth/login/totp", attempt.clone())
            .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        if attempt_number < 6 {
            // Un mot de passe juste (nouveau défi) ne remet pas le compteur à zéro.
            let (status, _, _) = api.login_via_proxy(attacker, PASSWORD).await;
            assert_eq!(status, StatusCode::OK);
        }
    }
    let good = json!({ "challenge": step["challenge"], "code": totp_code(&secret, 1) });
    let (status, headers, _) = api
        .public_post(attacker, "/api/auth/login/totp", good)
        .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(headers[header::RETRY_AFTER], "30");
}

#[tokio::test]
async fn property_valuations_history_drives_current_value() {
    let (api, alice, bob) = Api::household().await;
    let (house, loan) = api.house_with_loan(&alice, &bob).await;
    let path = format!("/api/accounts/{house}/valuations");

    // La création du bien a enregistré sa valeur comme première estimation, datée du jour.
    let list = api.get(&path).await;
    let first = &list[0];
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert!(close(&first["value"], 300_000.0));
    assert_eq!(first["source"], "manual");
    assert_eq!(first["account_id"], json!(house));
    let today = first["valued_on"].as_str().unwrap().to_string();

    // Estimation ancienne : la valeur courante ne change pas.
    let (status, older) = api
        .call(
            "POST",
            &path,
            Some(json!({ "value": 250000, "valued_on": "2023-01-15", "source": "external", "note": "Avis fictif" })),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(older["note"], "Avis fictif");
    assert!(close(
        &api.get(&format!("/api/accounts/{house}")).await["value"],
        300_000.0
    ));

    // Prix au m² : surface (90 m²) × prix.
    let (status, by_m2) = api
        .call("POST", &path, Some(json!({ "price_per_m2": 3500 })))
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(close(&by_m2["value"], 315_000.0));
    assert_eq!(by_m2["source"], "price_per_m2");
    assert_eq!(by_m2["valued_on"], json!(today));
    let detail = api.get(&format!("/api/accounts/{house}")).await;
    assert!(
        close(&detail["value"], 315_000.0)
            && close(&detail["property"]["current_value"], 315_000.0)
    );
    assert!(close(
        &api.get("/api/real-estate").await["totals"]["value"],
        315_000.0
    ));
    let dates: Vec<Value> = api
        .get(&path)
        .await
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["valued_on"].clone())
        .collect();
    assert_eq!(dates, vec![json!(today), json!(today), json!("2023-01-15")]);

    // Modification et suppression.
    let by_m2_id = by_m2["id"].as_str().unwrap();
    let (status, edited) = api
        .call(
            "PUT",
            &format!("/api/valuations/{by_m2_id}"),
            Some(json!({ "value": 320000, "note": "Revue" })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        (&edited["source"], &edited["note"]),
        (&json!("manual"), &json!("Revue"))
    );
    assert!(close(
        &api.get("/api/overview").await["gross_assets"],
        320_000.0
    ));
    assert_eq!(
        api.call("DELETE", &format!("/api/valuations/{by_m2_id}"), None)
            .await,
        (StatusCode::NO_CONTENT, Value::Null)
    );
    assert!(close(
        &api.get(&format!("/api/accounts/{house}")).await["value"],
        300_000.0
    ));

    // Erreurs : valeur nulle, date future, compte non immobilier, dernière estimation.
    assert_eq!(
        error_of(&api.call("POST", &path, Some(json!({ "value": 0 }))).await),
        (400, "value doit être strictement positif")
    );
    assert_eq!(
        error_of(
            &api.call(
                "POST",
                &path,
                Some(json!({ "value": 1, "valued_on": "2999-01-01" }))
            )
            .await
        ),
        (
            400,
            "La date d'une estimation ne peut pas être dans le futur"
        )
    );
    assert_eq!(
        error_of(
            &api.call("GET", &format!("/api/accounts/{loan}/valuations"), None)
                .await
        ),
        (404, "Bien immobilier introuvable")
    );
    let older_id = older["id"].as_str().unwrap();
    assert_eq!(
        api.call("DELETE", &format!("/api/valuations/{older_id}"), None)
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    let last = api.get(&path).await[0]["id"].as_str().unwrap().to_string();
    assert_eq!(
        error_of(
            &api.call("DELETE", &format!("/api/valuations/{last}"), None)
                .await
        ),
        (
            409,
            "La dernière estimation d'un bien ne peut pas être supprimée"
        )
    );

    // L'ancienne route : modifier current_value ajoute une estimation manuelle.
    let (status, _) = api
        .call(
            "PUT",
            &format!("/api/accounts/{house}"),
            Some(json!({ "property": { "current_value": 330000 } })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let list = api.get(&path).await;
    assert_eq!(list.as_array().unwrap().len(), 2);
    assert!(close(&list[0]["value"], 330_000.0));

    // L'export porte l'historique.
    let exported = api.get("/api/export").await;
    let entry = exported["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["kind"] == "real_estate")
        .unwrap()
        .clone();
    assert_eq!(entry["valuations"].as_array().unwrap().len(), 2);
    assert!(close(&entry["valuations"][1]["value"], 330_000.0));
}
