//! Tests d'intégration du serveur MCP `/mcp` : routeur complet dans le processus, au-dessus d'un vrai Postgres
//! (testcontainers). Toutes les données sont fictives.

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use fineme_application::Settings;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};
use tower::ServiceExt;

const PASSWORD: &str = "mot-de-passe-de-test";
const PROTOCOL: &str = "2025-06-18";

struct Mcp {
    router: Router,
    session: String,
    pool: sqlx::PgPool,
    _container: ContainerAsync<Postgres>,
}

impl Mcp {
    /// Foyer configuré (Alice, courant de 1 500 €) et session JWT ouverte.
    async fn household() -> Self {
        let container = Postgres::default()
            .with_tag("17")
            .start()
            .await
            .expect("démarrage du conteneur Postgres");
        let port = container.get_host_port_ipv4(5432).await.unwrap();
        let host = container.get_host().await.unwrap();
        let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");
        let pool = fineme_infra::postgres::connect(&url).await.unwrap();
        fineme_infra::postgres::migrate(&pool).await.unwrap();
        let router = fineme_server::build_router(
            pool.clone(),
            "secret-de-test-0123456789abcdef0123",
            "http://localhost:5173",
            0,
            Settings::default(),
            None,
            vec!["localhost".into()],
        )
        .await
        .unwrap();
        let mut mcp = Self {
            router,
            session: String::new(),
            pool,
            _container: container,
        };
        let body = json!({ "email": "alice@exemple.test", "password": PASSWORD, "name": "Alice" });
        let (_, _, session) = mcp.rest("POST", "/api/auth/setup", Some(body)).await;
        mcp.session = session["token"].as_str().unwrap().to_string();
        let (_, _, members) = mcp.rest("GET", "/api/members", None).await;
        let alice = members[0]["id"].as_str().unwrap();
        let account = json!({
            "name": "Compte courant", "kind": "checking", "cash_balance": 1500.0,
            "owners": [{ "member_id": alice, "share": 1.0 }],
        });
        let (status, _, body) = mcp.rest("POST", "/api/accounts", Some(account)).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        mcp
    }

    async fn send(&self, request: Request<Body>) -> (StatusCode, HeaderMap, Value) {
        let response = self.router.clone().oneshot(request).await.unwrap();
        let (status, headers) = (response.status(), response.headers().clone());
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body = serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
        (status, headers, body)
    }

    async fn rest(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, HeaderMap, Value) {
        let mut request = Request::builder().method(method).uri(path);
        if !self.session.is_empty() {
            request = request.header(header::AUTHORIZATION, format!("Bearer {}", self.session));
        }
        let request = match body {
            Some(body) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string())),
            None => request.body(Body::empty()),
        };
        self.send(request.unwrap()).await
    }

    async fn token(&self) -> String {
        let body = json!({ "name": "Claude Code", "password": PASSWORD });
        let (status, _, created) = self.rest("POST", "/api/access-tokens", Some(body)).await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
        created["secret"].as_str().unwrap().to_string()
    }

    fn request(bearer: Option<&str>, host: &str) -> axum::http::request::Builder {
        let mut request = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header(header::HOST, host)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ACCEPT, "application/json, text/event-stream")
            .header("mcp-protocol-version", PROTOCOL);
        if let Some(bearer) = bearer {
            request = request.header(header::AUTHORIZATION, format!("Bearer {bearer}"));
        }
        request
    }

    /// Envoie un message JSON-RPC à `/mcp`.
    async fn rpc(&self, bearer: Option<&str>, message: Value) -> (StatusCode, HeaderMap, Value) {
        let request = Self::request(bearer, "localhost")
            .body(Body::from(message.to_string()))
            .unwrap();
        self.send(request).await
    }

    async fn call_tool(&self, bearer: &str, name: &str, arguments: Value) -> Value {
        let (status, _, body) = self
            .rpc(
                Some(bearer),
                json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call",
                        "params": { "name": name, "arguments": arguments } }),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["result"].clone()
    }
}

fn initialize() -> Value {
    json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "protocolVersion": PROTOCOL, "capabilities": {},
                    "clientInfo": { "name": "test", "version": "1.0" } }
    })
}

/// Texte JSON du premier contenu d'un résultat de tool.
fn payload(result: &Value) -> Value {
    serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap()
}

#[tokio::test]
async fn token_holder_lists_six_read_only_tools_and_reads_the_authoritative_total() {
    let mcp = Mcp::household().await;
    let token = mcp.token().await;

    let (status, _, init) = mcp.rpc(Some(&token), initialize()).await;
    assert_eq!(status, StatusCode::OK, "{init}");
    let instructions = init["result"]["instructions"].as_str().unwrap();
    assert!(instructions.contains("fait foi") && instructions.contains("null"));

    let (_, _, listed) = mcp
        .rpc(
            Some(&token),
            json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
        )
        .await;
    let tools = listed["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 6, "{listed}");
    assert!(
        tools
            .iter()
            .all(|tool| tool["annotations"]["readOnlyHint"] == json!(true))
    );

    let result = mcp
        .call_tool(&token, "get_overview", json!({ "period": "1m" }))
        .await;
    assert_eq!(result["isError"], json!(false), "{result}");
    let overview = payload(&result);
    assert_eq!(overview["net_worth"], json!(1500.0));
    assert_eq!(overview["view"]["name"], "Foyer");

    for (name, arguments) in [
        ("get_performance", json!({})),
        ("get_budget", json!({})),
        ("get_goals", json!({})),
        ("compare_dates", json!({ "date_a": "2026-01-01" })),
        ("list_alerts", json!({})),
    ] {
        let result = mcp.call_tool(&token, name, arguments).await;
        assert_eq!(result["isError"], json!(false), "{name} : {result}");
    }

    // Aucun tool n'écrit : ni instantané (contrairement à GET /api/overview), ni évaluation d'alertes.
    let snapshots: i64 = sqlx::query_scalar("SELECT count(*) FROM snapshots")
        .fetch_one(&mcp.pool)
        .await
        .unwrap();
    assert_eq!(snapshots, 0);
    let alerts: i64 = sqlx::query_scalar("SELECT count(*) FROM alerts")
        .fetch_one(&mcp.pool)
        .await
        .unwrap();
    assert_eq!(alerts, 0);

    // Chaque appel de tool est journalisé et visible dans Paramètres ; la dernière utilisation est datée.
    let (_, _, calls) = mcp.rest("GET", "/api/access-tokens/calls", None).await;
    let calls = calls.as_array().unwrap();
    assert_eq!(calls.len(), 6);
    assert_eq!(calls[5]["tool"], "get_overview");
    assert_eq!(calls[0]["token_name"], "Claude Code");
    let (_, _, tokens) = mcp.rest("GET", "/api/access-tokens", None).await;
    assert!(tokens[0]["last_used_at"].is_string());
}

#[tokio::test]
async fn tool_errors_are_flagged_in_french_and_unknown_tools_do_not_exist() {
    let mcp = Mcp::household().await;
    let token = mcp.token().await;
    let result = mcp
        .call_tool(&token, "get_overview", json!({ "period": "hebdo" }))
        .await;
    assert_eq!(result["isError"], json!(true));
    assert!(
        result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Période")
    );

    let result = mcp
        .call_tool(&token, "compare_dates", json!({ "date_a": "hier" }))
        .await;
    assert_eq!(result["isError"], json!(true));

    // Aucun tool d'écriture n'existe : un nom inventé est refusé par le protocole.
    let (_, _, body) = mcp
        .rpc(
            Some(&token),
            json!({ "jsonrpc": "2.0", "id": 9, "method": "tools/call",
                    "params": { "name": "create_account", "arguments": { "name": "Intrus" } } }),
        )
        .await;
    assert!(
        body.get("error").is_some() || body["result"]["isError"] == json!(true),
        "{body}"
    );
    let (_, _, accounts) = mcp.rest("GET", "/api/accounts", None).await;
    assert_eq!(accounts.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn missing_invalid_session_or_revoked_tokens_are_refused_before_reading_the_message() {
    let mcp = Mcp::household().await;
    let token = mcp.token().await;
    let session = mcp.session.clone();
    for bearer in [
        None,
        Some("fm_pat_inconnu"),
        Some("fm_pat_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
        Some(session.as_str()),
    ] {
        let (status, headers, body) = mcp.rpc(bearer, initialize()).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{bearer:?}");
        assert!(
            headers[header::WWW_AUTHENTICATE]
                .to_str()
                .unwrap()
                .starts_with("Bearer")
        );
        assert!(body["error"].is_string());
    }

    let (_, _, tokens) = mcp.rest("GET", "/api/access-tokens", None).await;
    let id = tokens[0]["id"].as_str().unwrap();
    let (status, _, _) = mcp
        .rest(
            "POST",
            &format!("/api/access-tokens/{id}/revoke"),
            Some(json!({ "password": PASSWORD })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = mcp.rpc(Some(&token), initialize()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "jeton révoqué");
}

#[tokio::test]
async fn browsers_foreign_hosts_and_other_methods_are_refused() {
    let mcp = Mcp::household().await;
    let token = mcp.token().await;
    // Toutes les méthodes et tous les chemins sous /mcp passent par le jeton.
    for (method, path) in [
        ("GET", "/mcp"),
        ("DELETE", "/mcp"),
        ("POST", "/mcp/"),
        ("POST", "/mcp/xyz"),
    ] {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, "localhost")
            .body(Body::empty())
            .unwrap();
        let (status, _, _) = mcp.send(request).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path}");
    }

    let message = initialize();

    let with_origin = Mcp::request(Some(&token), "localhost")
        .header(header::ORIGIN, "https://site-malveillant.example")
        .body(Body::from(message.to_string()))
        .unwrap();
    let (status, headers, _) = mcp.send(with_origin).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(headers.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());

    let request = Mcp::request(Some(&token), "fineme.attaquant.example")
        .body(Body::from(message.to_string()))
        .unwrap();
    let (status, _, _) = mcp.send(request).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let get = Request::builder()
        .method("GET")
        .uri("/mcp")
        .header(header::HOST, "localhost")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::ACCEPT, "text/event-stream")
        .body(Body::empty())
        .unwrap();
    let (status, _, _) = mcp.send(get).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
}
