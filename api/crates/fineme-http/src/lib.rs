//! Adaptateur entrant HTTP : routes Axum sous `/api`, DTO JSON, authentification par jeton
//! et traduction des erreurs applicatives en réponses `{ "error": "..." }`.

mod auth;
mod error;
mod input;
mod oauth;
mod routes;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::Request;
use axum::http::{HeaderValue, Method, header};
use fineme_application::App;
use fineme_inbound::origin;
use tower_http::cors::CorsLayer;
use tower_http::sensitive_headers::SetSensitiveRequestHeadersLayer;
use tower_http::trace::TraceLayer;

pub(crate) type AppState = Arc<App>;

/// Taille maximale d'un corps de requête (imports CSV / bundle).
const MAX_BODY_BYTES: usize = 20 * 1024 * 1024;

fn cors_layer(origins: &str) -> CorsLayer {
    let allowed: Vec<HeaderValue> = origins
        .split(',')
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .filter_map(
            |origin| match HeaderValue::from_str(origin.trim_end_matches('/')) {
                Ok(value) => Some(value),
                Err(_) => {
                    tracing::warn!("origine CORS ignorée (invalide) : {origin}");
                    None
                }
            },
        )
        .collect();
    if allowed.is_empty() {
        tracing::warn!("CORS_ORIGINS est vide : aucune origine navigateur n'est autorisée");
    }
    CorsLayer::new()
        .allow_origin(allowed)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
        // Lisible par le front pour afficher le délai d'attente après un 429.
        .expose_headers([header::RETRY_AFTER])
        .max_age(Duration::from_secs(3600))
}

/// Chemin écrit dans les traces : l'identifiant d'une demande d'autorisation, qui permet de la refuser, est masqué.
fn traced_path(path: &str) -> &str {
    if path.starts_with("/api/oauth/requests/") {
        "/api/oauth/requests/*"
    } else {
        path
    }
}

/// Routeur complet de l'API. `cors_origins` : origines autorisées, séparées par des virgules ;
/// `trusted_proxy_hops` : nombre de proxys de confiance devant l'API (voir `origin`).
pub fn router(app: Arc<App>, cors_origins: &str, trusted_proxy_hops: usize) -> Router {
    Router::new()
        .nest("/api", routes::api(app.clone()))
        .merge(oauth::root())
        .fallback(routes::not_found)
        .method_not_allowed_fallback(routes::method_not_allowed)
        .layer(axum::extract::DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(axum::Extension(origin::TrustedProxyHops(
            trusted_proxy_hops,
        )))
        // Ni requête ni identifiant de demande OAuth dans les traces (voir `traced_path`).
        .layer(
            TraceLayer::new_for_http().make_span_with(|request: &Request| {
                tracing::debug_span!(
                    "request",
                    method = %request.method(),
                    path = traced_path(request.uri().path()),
                )
            }),
        )
        // Le jeton de session n'apparaît jamais dans les traces, même plus verbeuses.
        .layer(SetSensitiveRequestHeadersLayer::new([
            header::AUTHORIZATION,
        ]))
        .layer(cors_layer(cors_origins))
        .with_state(app)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traces_hide_the_authorization_request_identifier() {
        assert_eq!(
            traced_path("/api/oauth/requests/fm_oar_secret/deny"),
            "/api/oauth/requests/*"
        );
        assert_eq!(traced_path("/api/accounts"), "/api/accounts");
    }
}
