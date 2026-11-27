//! Parcours OAuth de bout en bout dans le processus : routeur réel, Postgres jetable et serveur de métadonnées
//! client simulé. Toutes les données sont fictives.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use fineme_application::Settings;
use fineme_application::oauth::OAuthSettings;
use fineme_domain::oauth::{ClientHost, CodeChallenge, PublicUrl};
use fineme_infra::client_metadata::HttpClientMetadata;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};
use tower::ServiceExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ISSUER: &str = "https://api.example.test";
const WEB: &str = "https://web.example.test";
const CLIENT: &str = "https://claude.ai/oauth/client.json";
const CALLBACK: &str = "https://claude.ai/api/mcp/auth_callback";
const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
const PASSWORD: &str = "mot-de-passe-de-test";

struct Server {
    router: Router,
    session: String,
    _metadata: MockServer,
    _container: ContainerAsync<Postgres>,
}

fn settings(oauth: bool) -> Settings {
    Settings {
        oauth: oauth.then(|| OAuthSettings {
            issuer: PublicUrl::parse(ISSUER).unwrap(),
            web: PublicUrl::parse(WEB).unwrap(),
            client_hosts: vec![ClientHost::parse("claude.ai").unwrap()],
        }),
        ..Settings::default()
    }
}

impl Server {
    async fn start(oauth: bool) -> Self {
        let container = Postgres::default()
            .with_tag("17")
            .start()
            .await
            .expect("démarrage du conteneur Postgres");
        let port = container.get_host_port_ipv4(5432).await.expect("port");
        let host = container.get_host().await.expect("hôte");
        let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");
        let pool = fineme_infra::postgres::connect(&url).await.unwrap();
        fineme_infra::postgres::migrate(&pool).await.unwrap();
        let metadata = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/oauth/client.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "client_id": CLIENT,
                "client_name": "Claude",
                "redirect_uris": [CALLBACK],
                "token_endpoint_auth_method": "none",
            })))
            .mount(&metadata)
            .await;
        let router = fineme_server::build_router_with_clients(
            pool,
            "secret-de-test-0123456789abcdef0123",
            WEB,
            settings(oauth),
            vec!["api.example.test".into()],
            Arc::new(HttpClientMetadata::served_by(&metadata.uri()).unwrap()),
        )
        .await
        .unwrap();
        let mut server = Self {
            router,
            session: String::new(),
            _metadata: metadata,
            _container: container,
        };
        let setup = json!({ "email": "alice@exemple.test", "password": PASSWORD, "name": "Alice" });
        let (status, _, body) = server.json("POST", "/api/auth/setup", Some(setup)).await;
        assert_eq!(status, StatusCode::CREATED);
        server.session = body["token"].as_str().unwrap().to_string();
        server
    }

    async fn send(&self, request: Request<Body>) -> (StatusCode, HeaderMap, Value) {
        let response = self.router.clone().oneshot(request).await.unwrap();
        let (status, headers) = (response.status(), response.headers().clone());
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, headers, body)
    }

    async fn json(
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

    async fn form(&self, body: &str) -> (StatusCode, HeaderMap, Value) {
        let request = Request::builder()
            .method("POST")
            .uri("/oauth/token")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(body.to_string()))
            .unwrap();
        self.send(request).await
    }

    /// Demande d'autorisation puis consentement : le code reçu par le client.
    async fn code(&self) -> String {
        let (status, headers, _) = self.get_anonymous(&authorize_uri(CALLBACK)).await;
        assert_eq!(status, StatusCode::FOUND);
        let location = headers[header::LOCATION].to_str().unwrap();
        let id = location
            .strip_prefix(&format!("{WEB}/oauth/consent?request="))
            .unwrap_or_else(|| panic!("{location}"));
        let (status, _, body) = self
            .json(
                "POST",
                &format!("/api/oauth/requests/{id}/approve"),
                Some(json!({ "password": PASSWORD })),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let redirect = body["redirect_to"].as_str().unwrap();
        assert!(redirect.starts_with(&format!("{CALLBACK}?code=")));
        redirect
            .split("code=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap()
            .to_string()
    }

    async fn get_anonymous(&self, uri: &str) -> (StatusCode, HeaderMap, Value) {
        let request = Request::builder().uri(uri).body(Body::empty()).unwrap();
        self.send(request).await
    }
}

fn challenge() -> String {
    let digest: [u8; 32] = Sha256::digest(VERIFIER.as_bytes()).into();
    CodeChallenge::from_digest(&digest).as_str().to_string()
}

fn authorize_uri(redirect: &str) -> String {
    format!(
        "/oauth/authorize?response_type=code&client_id={CLIENT}&redirect_uri={redirect}\
         &code_challenge={}&code_challenge_method=S256&state=xyz&scope=read&resource={ISSUER}/mcp",
        challenge()
    )
}

fn exchange(code: &str) -> String {
    format!(
        "grant_type=authorization_code&code={code}&redirect_uri={CALLBACK}&client_id={CLIENT}\
         &code_verifier={VERIFIER}&resource={ISSUER}/mcp"
    )
}

fn refresh(token: &str) -> String {
    format!("grant_type=refresh_token&refresh_token={token}&client_id={CLIENT}")
}

#[tokio::test]
async fn the_whole_flow_from_authorization_to_revocation() {
    let server = Server::start(true).await;

    let (status, _, metadata) = server
        .get_anonymous("/.well-known/oauth-authorization-server")
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(metadata["issuer"], ISSUER);
    assert_eq!(metadata["token_endpoint"], format!("{ISSUER}/oauth/token"));
    assert_eq!(
        metadata["code_challenge_methods_supported"],
        json!(["S256"])
    );
    assert_eq!(
        metadata["token_endpoint_auth_methods_supported"],
        json!(["none"])
    );
    assert_eq!(metadata["client_id_metadata_document_supported"], true);

    let code = server.code().await;
    let (status, headers, tokens) = server.form(&exchange(&code)).await;
    assert_eq!(status, StatusCode::OK, "{tokens}");
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    assert_eq!(tokens["token_type"], "Bearer");
    assert_eq!(tokens["expires_in"], 3600);
    assert_eq!(tokens["scope"], "read");
    let access = tokens["access_token"].as_str().unwrap();
    assert!(access.starts_with("fm_oat_"));

    // Un jeton OAuth n'ouvre aucune route /api.
    let as_oauth = Request::builder()
        .uri("/api/accounts")
        .header(header::AUTHORIZATION, format!("Bearer {access}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(server.send(as_oauth).await.0, StatusCode::UNAUTHORIZED);

    let first_refresh = tokens["refresh_token"].as_str().unwrap();
    let (status, _, rotated) = server.form(&refresh(first_refresh)).await;
    assert_eq!(status, StatusCode::OK, "{rotated}");
    assert_ne!(rotated["refresh_token"], tokens["refresh_token"]);

    let (_, _, grants) = server.json("GET", "/api/oauth/grants", None).await;
    assert_eq!(grants[0]["client_name"], "Claude");
    assert_eq!(grants[0]["client_id"], CLIENT);
    assert_eq!(grants[0]["redirect_host"], "claude.ai");
    assert_eq!(grants[0]["revoked_at"], Value::Null);

    let revoke = format!(
        "/api/oauth/grants/{}/revoke",
        grants[0]["id"].as_str().unwrap()
    );
    let wrong = json!({ "password": "faux-mot-de-passe" });
    let (status, _, _) = server.json("POST", &revoke, Some(wrong)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, revoked) = server
        .json("POST", &revoke, Some(json!({ "password": PASSWORD })))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(revoked["revoked_at"].is_string());

    let (status, _, error) = server
        .form(&refresh(rotated["refresh_token"].as_str().unwrap()))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["error"], "invalid_grant");

    let (_, _, history) = server.json("GET", "/api/auth/logins", None).await;
    assert!(
        history
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["action"] == "oauth")
    );
}

#[tokio::test]
async fn consent_can_be_denied_and_bad_requests_never_redirect() {
    let server = Server::start(true).await;

    let (status, headers, body) = server
        .get_anonymous(&authorize_uri("https://claude.ai/autre"))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!headers.contains_key(header::LOCATION));
    assert!(body["error"].is_string());
    let twice = format!("{}&state=encore", authorize_uri(CALLBACK));
    assert_eq!(
        server.get_anonymous(&twice).await.0,
        StatusCode::BAD_REQUEST
    );

    let (_, headers, _) = server.get_anonymous(&authorize_uri(CALLBACK)).await;
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    assert_eq!(headers[header::REFERRER_POLICY], "no-referrer");
    let location = headers[header::LOCATION].to_str().unwrap().to_string();
    let id = location.split("request=").nth(1).unwrap();
    let (status, _, view) = server
        .get_anonymous(&format!("/api/oauth/requests/{id}"))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(view["client_name"], "Claude");
    assert_eq!(view["redirect_host"], "claude.ai");
    assert_eq!(view.as_object().unwrap().len(), 4);

    let huge = json!({ "password": "a".repeat(20 * 1024) });
    let approve = format!("/api/oauth/requests/{id}/approve");
    let (status, _, refused) = server.json("POST", &approve, Some(huge)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(refused["error"].as_str().unwrap().contains("length limit"));
    let (status, _, _) = server.get_anonymous("/api/oauth/grants").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, headers, denied) = server
        .json("POST", &format!("/api/oauth/requests/{id}/deny"), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    assert_eq!(
        denied["redirect_to"],
        format!("{CALLBACK}?error=access_denied&state=xyz&iss=https%3A%2F%2Fapi.example.test")
    );
    let (status, _, _) = server
        .get_anonymous(&format!("/api/oauth/requests/{id}"))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_token_endpoint_speaks_oauth_errors() {
    let server = Server::start(true).await;
    for (body, error) in [
        ("grant_type=password&username=a", "unsupported_grant_type"),
        ("code=x", "invalid_request"),
        ("grant_type=authorization_code&code=x", "invalid_request"),
        (
            "grant_type=refresh_token&grant_type=refresh_token",
            "invalid_request",
        ),
        (&exchange("fm_oac_inconnu"), "invalid_grant"),
    ] {
        let (status, headers, response) = server.form(body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(response["error"], error, "{body}");
        assert!(response["error_description"].is_string());
        assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    }
    let (status, _, response) = server
        .form(&format!("{}&client_secret=s", exchange("x")))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(response["error"], "invalid_client");
    let oversized = format!("grant_type=refresh_token&x={}", "a".repeat(20 * 1024));
    let (status, _, response) = server.form(&oversized).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(response["error"], "invalid_request");
    let as_json = Request::builder()
        .method("POST")
        .uri("/oauth/token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let (status, _, response) = server.send(as_json).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(response["error"], "invalid_request");
}

#[tokio::test]
async fn without_public_urls_the_connector_does_not_exist() {
    let server = Server::start(false).await;
    for uri in [
        "/.well-known/oauth-authorization-server",
        &authorize_uri(CALLBACK),
        "/api/oauth/requests/fm_oar_x",
    ] {
        assert_eq!(
            server.get_anonymous(uri).await.0,
            StatusCode::NOT_FOUND,
            "{uri}"
        );
    }
    let twice = format!("{}&state=encore", authorize_uri(CALLBACK));
    assert_eq!(server.get_anonymous(&twice).await.0, StatusCode::NOT_FOUND);
    for body in [exchange("x"), "client_secret=s&a=1&a=2".to_string()] {
        assert_eq!(server.form(&body).await.0, StatusCode::NOT_FOUND, "{body}");
    }
    let (status, _, grants) = server.json("GET", "/api/oauth/grants", None).await;
    assert_eq!((status, grants), (StatusCode::OK, json!([])));
}
