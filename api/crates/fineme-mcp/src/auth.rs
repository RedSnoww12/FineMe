//! Authentification de `/mcp` : jeton d'accès personnel (`Authorization: Bearer fm_pat_…`), vérifié avant toute
//! lecture du message MCP. Le JWT de session n'y est pas accepté.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use fineme_application::{App, AppError};
use fineme_http::origin::ClientOrigin;

/// En-tête de défi renvoyé avec un 401 (RFC 6750).
const CHALLENGE: &str = "Bearer realm=\"fineme-mcp\"";

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::trim)
}

fn refusal(error: &AppError) -> Response {
    let body = axum::Json(serde_json::json!({ "error": error.to_string() }));
    match error {
        AppError::TooManyAttempts(seconds) => {
            let mut response = (StatusCode::TOO_MANY_REQUESTS, body).into_response();
            if let Ok(value) = HeaderValue::from_str(&seconds.to_string()) {
                response.headers_mut().insert(header::RETRY_AFTER, value);
            }
            response
        }
        AppError::Unauthorized(_) => {
            let mut response = (StatusCode::UNAUTHORIZED, body).into_response();
            response.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                HeaderValue::from_static(CHALLENGE),
            );
            response
        }
        other => {
            tracing::error!("authentification MCP impossible : {other}");
            let body =
                serde_json::json!({ "error": "Erreur interne de FineMe : réessayez plus tard." });
            (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(body)).into_response()
        }
    }
}

/// Vérifie le jeton puis dépose l'identité (`AccessPrincipal`) dans les extensions de la requête, où les tools la
/// lisent.
pub async fn require_access_token(
    State(app): State<Arc<App>>,
    ClientOrigin(origin): ClientOrigin,
    mut request: Request,
    next: Next,
) -> Response {
    let presented = bearer(request.headers());
    match app.auth.authenticate_access_token(presented, &origin).await {
        Ok(principal) => {
            request.extensions_mut().insert(principal);
            next.run(request).await
        }
        Err(error) => refusal(&error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_reads_only_the_bearer_scheme() {
        let mut headers = HeaderMap::new();
        assert_eq!(bearer(&headers), None);
        headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Basic abc"));
        assert_eq!(bearer(&headers), None);
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer fm_pat_x "),
        );
        assert_eq!(bearer(&headers), Some("fm_pat_x"));
    }

    #[test]
    fn refusals_carry_the_challenge_or_the_delay() {
        let unauthorized = refusal(&AppError::Unauthorized("Jeton d'accès invalide"));
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(unauthorized.headers()[header::WWW_AUTHENTICATE], CHALLENGE);
        let throttled = refusal(&AppError::TooManyAttempts(30));
        assert_eq!(throttled.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(throttled.headers()[header::RETRY_AFTER], "30");
    }
}
