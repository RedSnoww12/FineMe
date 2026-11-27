//! Métadonnées de la ressource protégée (RFC 9728) : par elles, un client OAuth découvre le serveur d'autorisation
//! de `/mcp`. Publiques ; absentes quand le connecteur OAuth est désactivé.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use fineme_application::App;
use fineme_domain::access_token::TokenScope;

/// Chemin des métadonnées de `/mcp` ; la racine sert les mêmes aux clients qui n'essaient qu'elle.
pub const PATH: &str = "/.well-known/oauth-protected-resource/mcp";
const ROOT_PATH: &str = "/.well-known/oauth-protected-resource";

pub fn router(app: Arc<App>) -> Router {
    Router::new()
        .route(PATH, get(protected_resource))
        .route(ROOT_PATH, get(protected_resource))
        .with_state(app)
}

async fn protected_resource(State(app): State<Arc<App>>) -> Response {
    match app.auth.oauth_settings() {
        Ok(settings) => Json(serde_json::json!({
            "resource": settings.resource().as_str(),
            "authorization_servers": [settings.issuer.as_str()],
            "scopes_supported": [TokenScope::Read.as_str()],
            "bearer_methods_supported": ["header"],
            "resource_name": "FineMe",
        }))
        .into_response(),
        Err(error) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": error.to_string() })),
        )
            .into_response(),
    }
}
