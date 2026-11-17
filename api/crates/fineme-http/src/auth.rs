use axum::extract::{Request, State};
use axum::http::header;
use axum::middleware::Next;
use axum::response::Response;
use fineme_domain::ids::UserId;

use crate::AppState;
use crate::error::ApiError;

/// Utilisateur authentifié, déposé dans la requête par le middleware.
#[derive(Debug, Clone, Copy)]
pub struct AuthUser(pub UserId);

/// Jeton d'un en-tête `Authorization: Bearer <jeton>`.
pub fn bearer_token(header_value: Option<&str>) -> Option<&str> {
    let value = header_value?;
    let token = value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))?
        .trim();
    (!token.is_empty()).then_some(token)
}

/// Middleware : refuse (401) toute requête sans jeton valide.
pub async fn require_auth(
    State(app): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let header_value = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    let token = bearer_token(header_value).ok_or_else(ApiError::unauthenticated)?;
    let user = app.auth.authenticate(token).await?;
    request.extensions_mut().insert(AuthUser(user));
    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::bearer_token;

    #[test]
    fn extracts_bearer_tokens_only() {
        assert_eq!(bearer_token(Some("Bearer abc.def")), Some("abc.def"));
        assert_eq!(bearer_token(Some("bearer  abc ")), Some("abc"));
        assert_eq!(bearer_token(Some("Basic abc")), None);
        assert_eq!(bearer_token(Some("Bearer ")), None);
        assert_eq!(bearer_token(Some("abc")), None);
        assert_eq!(bearer_token(None), None);
    }
}
