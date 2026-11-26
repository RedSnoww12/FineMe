//! Adaptateur entrant MCP (Model Context Protocol), en lecture seule : un client IA (Claude Code, Claude Desktop)
//! consulte le patrimoine par des tools qui appellent les cas d'usage existants. Aucune règle métier ici.
//!
//! Transport Streamable HTTP sans session, réponses JSON, sur `/mcp`. Contrôles, dans l'ordre : limiteur des
//! échecs par IP (429), jeton d'accès (401), hôte autorisé et absence d'en-tête `Origin` (403), corps ≤ 1 Mo.

mod auth;
mod error;
mod output;
mod params;
mod server;

use std::sync::Arc;

use axum::Router;
use axum::http::header;
use fineme_application::App;
use fineme_inbound::origin::TrustedProxyHops;
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use tower_http::sensitive_headers::SetSensitiveRequestHeadersLayer;
use tower_http::trace::TraceLayer;

pub use server::{FinemeMcp, INSTRUCTIONS};

/// Taille maximale d'un message MCP.
const MAX_BODY_BYTES: usize = 1024 * 1024;

/// Hôtes acceptés par défaut dans l'en-tête `Host` (protection contre le DNS rebinding).
pub const DEFAULT_ALLOWED_HOSTS: [&str; 3] = ["localhost", "127.0.0.1", "::1"];

#[derive(Debug, Clone)]
pub struct McpSettings {
    /// Hôtes (`hôte` ou `hôte:port`) acceptés dans l'en-tête `Host`.
    pub allowed_hosts: Vec<String>,
    /// Proxys de confiance devant l'API, pour lire l'IP du client (limiteur).
    pub trusted_proxy_hops: usize,
}

/// Routeur `/mcp`, à fusionner avec celui de l'API.
pub fn router(app: Arc<App>, settings: McpSettings) -> Router {
    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_sse_keep_alive(None)
        .with_allowed_hosts(settings.allowed_hosts)
        // Liste vide + validation forcée : toute requête portant un `Origin` (un navigateur) est refusée.
        .enforce_origin_validation()
        .with_max_request_body_bytes(MAX_BODY_BYTES);
    let factory_app = app.clone();
    let service = StreamableHttpService::new(
        move || Ok(FinemeMcp::new(factory_app.clone())),
        Arc::new(NeverSessionManager::default()),
        config,
    );
    Router::new()
        .nest_service("/mcp", service)
        .layer(axum::middleware::from_fn_with_state(
            app,
            auth::require_access_token,
        ))
        .layer(axum::Extension(TrustedProxyHops(
            settings.trusted_proxy_hops,
        )))
        .layer(TraceLayer::new_for_http())
        // Le jeton d'accès n'apparaît jamais dans les traces.
        .layer(SetSensitiveRequestHeadersLayer::new([
            header::AUTHORIZATION,
        ]))
}
