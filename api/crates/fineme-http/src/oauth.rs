//! Serveur d'autorisation OAuth (connecteur claude.ai) : métadonnées, demande d'autorisation et point de jeton à la
//! racine ; consentement et gestion des autorisations sous `/api`. Les règles sont dans les cas d'usage.

use axum::extract::rejection::FormRejection;
use axum::extract::{DefaultBodyLimit, Form, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Router};
use fineme_application::AppError;
use fineme_application::oauth::{AuthorizeParams, TokenError, TokenRequest};
use fineme_domain::ids::OAuthGrantId;
use fineme_inbound::contract as output;
use fineme_inbound::origin::ClientOrigin;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult, Json, Path, Query};
use crate::input::IdentityConfirmationBody;

/// Taille maximale d'un corps envoyé sans session (point de jeton, consentement).
const MAX_PUBLIC_BODY_BYTES: usize = 16 * 1024;

type JsonResponse = ApiResult<axum::Json<Value>>;

/// Routes hors `/api` : sans session ni cookie.
pub fn root() -> Router<AppState> {
    Router::new()
        .route(
            "/.well-known/oauth-authorization-server",
            get(server_metadata),
        )
        .route("/oauth/authorize", get(authorize))
        .route(
            "/oauth/token",
            post(token).layer(DefaultBodyLimit::max(MAX_PUBLIC_BODY_BYTES)),
        )
}

/// Consentement : confirmé par le mot de passe et le second facteur, pas par la session web.
pub fn consent() -> Router<AppState> {
    Router::new()
        .route("/oauth/requests/{id}", get(request_view))
        .route("/oauth/requests/{id}/approve", post(approve))
        .route("/oauth/requests/{id}/deny", post(deny))
        .layer(DefaultBodyLimit::max(MAX_PUBLIC_BODY_BYTES))
}

/// Gestion des autorisations, sous JWT.
pub fn grants() -> Router<AppState> {
    Router::new()
        .route("/oauth/grants", get(grants_list))
        .route("/oauth/grants/{id}/revoke", post(grants_revoke))
}

async fn server_metadata(State(app): State<AppState>) -> JsonResponse {
    let issuer = app.auth.oauth_settings()?.issuer.as_str();
    Ok(axum::Json(json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{issuer}/oauth/authorize"),
        "token_endpoint": format!("{issuer}/oauth/token"),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
        "scopes_supported": ["read"],
        "client_id_metadata_document_supported": true,
        "authorization_response_iss_parameter_supported": true,
    })))
}

/// Paramètres d'une requête, chacun présent au plus une fois ; vide vaut absent (RFC 6749 §3.1 et §3.2).
struct Params(Vec<(String, String)>);

impl Params {
    fn new(pairs: Vec<(String, String)>) -> Option<Self> {
        let mut names: Vec<&str> = pairs.iter().map(|(name, _)| name.as_str()).collect();
        names.sort_unstable();
        let unique = names.windows(2).all(|pair| pair[0] != pair[1]);
        unique.then(|| {
            Self(
                pairs
                    .into_iter()
                    .filter(|(_, value)| !value.is_empty())
                    .collect(),
            )
        })
    }

    fn take(&mut self, name: &str) -> Option<String> {
        let index = self.0.iter().position(|(key, _)| key == name)?;
        Some(self.0.swap_remove(index).1)
    }
}

async fn authorize(
    State(app): State<AppState>,
    ClientOrigin(origin): ClientOrigin,
    Query(pairs): Query<Vec<(String, String)>>,
) -> ApiResult<Response> {
    // Connecteur désactivé : 404 avant toute lecture de la demande.
    app.auth.oauth_settings()?;
    let mut params =
        Params::new(pairs).ok_or_else(|| AppError::invalid("Paramètre présent deux fois"))?;
    let params = AuthorizeParams {
        response_type: params.take("response_type"),
        client_id: params.take("client_id"),
        redirect_uri: params.take("redirect_uri"),
        code_challenge: params.take("code_challenge"),
        code_challenge_method: params.take("code_challenge_method"),
        state: params.take("state"),
        scope: params.take("scope"),
        resource: params.take("resource"),
    };
    let target = app.auth.authorize(params, &origin).await?;
    let location = HeaderValue::from_str(&target)
        .map_err(|_| AppError::Internal("URL de redirection illisible".into()))?;
    Ok((
        StatusCode::FOUND,
        [
            (header::LOCATION, location),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
            (
                header::REFERRER_POLICY,
                HeaderValue::from_static("no-referrer"),
            ),
        ],
    )
        .into_response())
}

// ---------------------------------------------------------------- point de jeton (RFC 6749 §3.2 et §5)

/// Réponse du point de jeton : jamais mise en cache.
fn no_store(status: StatusCode, body: Value) -> Response {
    (
        status,
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::PRAGMA, "no-cache"),
        ],
        axum::Json(body),
    )
        .into_response()
}

fn oauth_error(status: StatusCode, code: &str, description: &str) -> Response {
    no_store(
        status,
        json!({ "error": code, "error_description": description }),
    )
}

fn invalid_request(description: &str) -> Response {
    oauth_error(StatusCode::BAD_REQUEST, "invalid_request", description)
}

fn token_failure(error: TokenError) -> Response {
    match error {
        TokenError::Refused { code, description } => {
            oauth_error(StatusCode::BAD_REQUEST, code.as_str(), description)
        }
        TokenError::App(error) => {
            let api = ApiError::from(error);
            let retry_after = api.retry_after;
            let code = if api.status.is_server_error() {
                "server_error"
            } else {
                "invalid_request"
            };
            let mut response = oauth_error(api.status, code, &api.message);
            if let Some(seconds) = retry_after {
                response
                    .headers_mut()
                    .insert(header::RETRY_AFTER, seconds.into());
            }
            response
        }
    }
}

/// La demande décrite par le formulaire, ou la réponse de refus.
fn token_request(mut params: Params) -> Result<TokenRequest, Box<Response>> {
    let grant_type = params.take("grant_type");
    let mut required = |name: &str| {
        params
            .take(name)
            .ok_or_else(|| Box::new(invalid_request(&format!("Paramètre manquant : {name}"))))
    };
    match grant_type.as_deref() {
        Some("authorization_code") => Ok(TokenRequest::AuthorizationCode {
            code: required("code")?,
            redirect_uri: required("redirect_uri")?,
            client_id: required("client_id")?,
            code_verifier: required("code_verifier")?,
            resource: params.take("resource"),
        }),
        Some("refresh_token") => Ok(TokenRequest::Refresh {
            refresh_token: required("refresh_token")?,
            client_id: required("client_id")?,
            scope: params.take("scope"),
            resource: params.take("resource"),
        }),
        Some(_) => Err(Box::new(oauth_error(
            StatusCode::BAD_REQUEST,
            "unsupported_grant_type",
            "Type d'autorisation non pris en charge",
        ))),
        None => Err(Box::new(invalid_request("Paramètre manquant : grant_type"))),
    }
}

/// Clients publics seulement : toute tentative d'authentification du client est refusée, en 401 avec le défi
/// quand elle passe par `Authorization` (RFC 6749 §5.2), en 400 quand elle vient du formulaire.
fn client_authentication(headers: &HeaderMap, params: &Params) -> Option<Response> {
    let refusal = |status| {
        oauth_error(
            status,
            "invalid_client",
            "Seuls les clients publics (sans secret) sont acceptés",
        )
    };
    if headers.contains_key(header::AUTHORIZATION) {
        let mut response = refusal(StatusCode::UNAUTHORIZED);
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_static("Basic realm=\"FineMe\""),
        );
        return Some(response);
    }
    params
        .0
        .iter()
        .any(|(name, _)| name == "client_secret" || name.starts_with("client_assertion"))
        .then(|| refusal(StatusCode::BAD_REQUEST))
}

async fn token(
    State(app): State<AppState>,
    ClientOrigin(origin): ClientOrigin,
    headers: HeaderMap,
    form: Result<Form<Vec<(String, String)>>, FormRejection>,
) -> Response {
    if let Err(error) = app.auth.oauth_settings() {
        return token_failure(TokenError::App(error));
    }
    let Ok(Form(pairs)) = form else {
        return invalid_request("Formulaire application/x-www-form-urlencoded attendu");
    };
    let Some(params) = Params::new(pairs) else {
        return invalid_request("Paramètre présent deux fois");
    };
    if let Some(refusal) = client_authentication(&headers, &params) {
        return refusal;
    }
    let request = match token_request(params) {
        Ok(request) => request,
        Err(refusal) => return *refusal,
    };
    match app.auth.token(request, &origin).await {
        Ok(tokens) => no_store(
            StatusCode::OK,
            json!({
                "access_token": tokens.access_token.as_str(),
                "token_type": "Bearer",
                "expires_in": tokens.expires_in,
                "refresh_token": tokens.refresh_token.as_str(),
                "scope": tokens.scope.as_str(),
            }),
        ),
        Err(error) => token_failure(error),
    }
}

// ---------------------------------------------------------------- consentement et autorisations

async fn request_view(State(app): State<AppState>, Path(id): Path<String>) -> JsonResponse {
    let view = app.auth.consent_view(&id).await?;
    Ok(axum::Json(output::consent_view(&view)))
}

/// L'URL renvoyée au front porte le code d'autorisation : jamais mise en cache.
fn redirect_to(target: String) -> Response {
    no_store(StatusCode::OK, json!({ "redirect_to": target }))
}

async fn approve(
    State(app): State<AppState>,
    ClientOrigin(origin): ClientOrigin,
    Path(id): Path<String>,
    Json(body): Json<IdentityConfirmationBody>,
) -> ApiResult<Response> {
    let target = app
        .auth
        .approve_request(&id, &body.password, body.code.as_deref(), &origin)
        .await?;
    Ok(redirect_to(target))
}

async fn deny(State(app): State<AppState>, Path(id): Path<String>) -> ApiResult<Response> {
    let target = app.auth.deny_request(&id).await?;
    Ok(redirect_to(target))
}

async fn grants_list(
    State(app): State<AppState>,
    Extension(AuthUser(user)): Extension<AuthUser>,
) -> JsonResponse {
    let grants = app.auth.oauth_grants(user).await?;
    Ok(axum::Json(Value::Array(
        grants.iter().map(output::oauth_grant).collect(),
    )))
}

async fn grants_revoke(
    State(app): State<AppState>,
    Extension(AuthUser(user)): Extension<AuthUser>,
    ClientOrigin(origin): ClientOrigin,
    Path(id): Path<Uuid>,
    Json(body): Json<IdentityConfirmationBody>,
) -> JsonResponse {
    let grant = app
        .auth
        .revoke_oauth_grant(
            user,
            OAuthGrantId::from_uuid(id),
            &body.password,
            body.code.as_deref(),
            &origin,
        )
        .await?;
    Ok(axum::Json(output::oauth_grant(&grant)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fineme_application::ports::RepoError;

    fn params(pairs: &[(&str, &str)]) -> Option<Params> {
        Params::new(
            pairs
                .iter()
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect(),
        )
    }

    #[test]
    fn parameters_are_unique_and_empty_ones_are_absent() {
        assert!(params(&[("state", "a"), ("state", "b")]).is_none());
        assert!(params(&[("state", ""), ("state", "b")]).is_none());
        let mut read = params(&[("state", ""), ("scope", "read")]).unwrap();
        assert_eq!(read.take("state"), None);
        assert_eq!(read.take("scope").as_deref(), Some("read"));
        assert_eq!(read.take("scope"), None);
    }

    #[test]
    fn client_authentication_is_refused_whatever_its_form() {
        let empty = HeaderMap::new();
        let public = params(&[("client_id", "x"), ("client_secret", "")]).unwrap();
        assert!(client_authentication(&empty, &public).is_none());
        for name in ["client_secret", "client_assertion", "client_assertion_type"] {
            let secret = params(&[(name, "s")]).unwrap();
            let refusal = client_authentication(&empty, &secret).unwrap();
            assert_eq!(refusal.status(), StatusCode::BAD_REQUEST, "{name}");
        }
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Basic eA=="),
        );
        let refusal = client_authentication(&headers, &public).unwrap();
        assert_eq!(refusal.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            refusal.headers()[header::WWW_AUTHENTICATE],
            "Basic realm=\"FineMe\""
        );
    }

    #[test]
    fn token_requests_name_what_is_missing_or_unsupported() {
        let refresh = params(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", "r"),
            ("client_id", "c"),
        ])
        .unwrap();
        assert!(matches!(
            token_request(refresh),
            Ok(TokenRequest::Refresh { scope: None, .. })
        ));
        for pairs in [&[("code", "x")][..], &[("grant_type", "password")]] {
            let refusal = token_request(params(pairs).unwrap()).err().unwrap();
            assert_eq!(refusal.status(), StatusCode::BAD_REQUEST);
        }
    }

    #[test]
    fn application_failures_keep_their_status_and_delay() {
        let throttled = token_failure(TokenError::App(AppError::TooManyAttempts(30)));
        assert_eq!(throttled.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(throttled.headers()[header::RETRY_AFTER], "30");
        assert_eq!(throttled.headers()[header::CACHE_CONTROL], "no-store");
        let broken = token_failure(TokenError::App(AppError::Repository(RepoError::Backend(
            "panne".into(),
        ))));
        assert!(broken.status().is_server_error());
        assert_eq!(broken.headers()[header::CACHE_CONTROL], "no-store");
    }
}
