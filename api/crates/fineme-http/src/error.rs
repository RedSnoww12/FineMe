use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use fineme_application::AppError;
use fineme_application::ports::RepoError;
use serde_json::json;

/// Erreur renvoyée au client : statut HTTP + `{ "error": "message en français" }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
    /// Secondes à attendre avant de réessayer (en-tête `Retry-After`, réponses 429).
    pub retry_after: Option<u64>,
}

pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
            retry_after: None,
        }
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    pub fn unauthenticated() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "Authentification requise")
    }

    fn internal(error: &dyn std::fmt::Display) -> Self {
        // Le détail reste dans les logs ; le client reçoit un message générique.
        tracing::error!("erreur interne : {error}");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Erreur interne du serveur",
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response =
            (self.status, axum::Json(json!({ "error": self.message }))).into_response();
        if let Some(seconds) = self.retry_after {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, seconds.into());
        }
        response
    }
}

impl From<AppError> for ApiError {
    fn from(error: AppError) -> Self {
        match &error {
            AppError::Domain(_) | AppError::Invalid(_) => Self::bad_request(error.to_string()),
            AppError::NotFound(message) => Self::new(StatusCode::NOT_FOUND, *message),
            AppError::Conflict(message) => Self::new(StatusCode::CONFLICT, *message),
            AppError::Unauthorized(message) => Self::new(StatusCode::UNAUTHORIZED, *message),
            AppError::Forbidden(message) => Self::new(StatusCode::FORBIDDEN, *message),
            AppError::NotConfigured(message) => Self::new(StatusCode::NOT_IMPLEMENTED, *message),
            AppError::TooManyAttempts(seconds) => Self {
                retry_after: Some(*seconds),
                ..Self::new(StatusCode::TOO_MANY_REQUESTS, error.to_string())
            },
            AppError::Repository(RepoError::Conflict) => {
                Self::new(StatusCode::CONFLICT, "Cet élément existe déjà")
            }
            AppError::Repository(RepoError::Reference) => Self::new(
                StatusCode::CONFLICT,
                "Opération impossible : élément référencé ailleurs ou introuvable",
            ),
            AppError::Repository(RepoError::Backend(_))
            | AppError::Provider(_)
            | AppError::Internal(_) => Self::internal(&error),
        }
    }
}

impl From<JsonRejection> for ApiError {
    fn from(rejection: JsonRejection) -> Self {
        Self::bad_request(format!("Corps JSON invalide : {}", rejection.body_text()))
    }
}

impl From<PathRejection> for ApiError {
    fn from(_: PathRejection) -> Self {
        Self::bad_request("Identifiant invalide dans l'URL")
    }
}

impl From<QueryRejection> for ApiError {
    fn from(_: QueryRejection) -> Self {
        Self::bad_request("Paramètres de requête invalides")
    }
}

/// Extracteurs dont les rejets suivent le format d'erreur de l'API.
#[derive(axum::extract::FromRequest)]
#[from_request(via(axum::Json), rejection(ApiError))]
pub struct Json<T>(pub T);

#[derive(axum::extract::FromRequestParts)]
#[from_request(via(axum::extract::Path), rejection(ApiError))]
pub struct Path<T>(pub T);

#[derive(axum::extract::FromRequestParts)]
#[from_request(via(axum::extract::Query), rejection(ApiError))]
pub struct Query<T>(pub T);

#[cfg(test)]
mod tests {
    use super::*;
    use fineme_application::ports::ProviderError;
    use fineme_domain::DomainError;

    fn map(error: AppError) -> (u16, String) {
        let api = ApiError::from(error);
        (api.status.as_u16(), api.message)
    }

    #[test]
    fn client_errors_keep_their_french_message() {
        assert_eq!(
            map(DomainError::InvalidShare.into()),
            (400, "Chaque part doit être comprise entre 0 et 1".into())
        );
        assert_eq!(
            map(AppError::invalid("Adresse e-mail invalide")),
            (400, "Adresse e-mail invalide".into())
        );
        assert_eq!(
            map(AppError::NotFound("Compte introuvable")),
            (404, "Compte introuvable".into())
        );
        assert_eq!(
            map(AppError::Conflict("Un utilisateur existe déjà")),
            (409, "Un utilisateur existe déjà".into())
        );
        assert_eq!(
            map(AppError::Unauthorized("Session invalide ou expirée")).0,
            401
        );
        assert_eq!(
            map(AppError::Forbidden(
                "Code de configuration invalide ou manquant"
            )),
            (403, "Code de configuration invalide ou manquant".into())
        );
        assert_eq!(
            map(AppError::NotConfigured(
                "Ce fournisseur bancaire n'est pas configuré"
            ))
            .0,
            501
        );
    }

    #[test]
    fn too_many_attempts_is_a_429_with_retry_after() {
        let api = ApiError::from(AppError::TooManyAttempts(30));
        assert_eq!(api.status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(api.retry_after, Some(30));
        let response = api.into_response();
        assert_eq!(response.headers()[header::RETRY_AFTER], "30");
    }

    #[test]
    fn repository_constraints_become_conflicts() {
        assert_eq!(
            map(RepoError::Conflict.into()),
            (409, "Cet élément existe déjà".into())
        );
        assert_eq!(map(RepoError::Reference.into()).0, 409);
    }

    #[test]
    fn internal_details_are_not_leaked() {
        let generic = (500, "Erreur interne du serveur".to_string());
        assert_eq!(
            map(RepoError::Backend("relation \"x\" does not exist".into()).into()),
            generic
        );
        assert_eq!(
            map(ProviderError("jeton secret refusé".into()).into()),
            generic
        );
        assert_eq!(map(AppError::Internal("détail".into())), generic);
    }
}
