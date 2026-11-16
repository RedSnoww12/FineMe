use fineme_domain::DomainError;

use crate::ports::{ProviderError, RepoError};

/// Erreur d'un cas d'usage. Les messages destinés au client sont en français.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// Règle métier ou invariant du domaine violé (requête invalide).
    #[error(transparent)]
    Domain(#[from] DomainError),
    /// Requête invalide détectée par le cas d'usage.
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    NotFound(&'static str),
    #[error("{0}")]
    Conflict(&'static str),
    #[error("{0}")]
    Unauthorized(&'static str),
    /// Action refusée faute de preuve suffisante (ex. code de première configuration).
    #[error("{0}")]
    Forbidden(&'static str),
    /// Fonction disponible seulement avec un fournisseur externe non configuré.
    #[error("{0}")]
    NotConfigured(&'static str),
    #[error("erreur de persistance : {0}")]
    Repository(#[from] RepoError),
    #[error("erreur d'un service externe : {0}")]
    Provider(#[from] ProviderError),
    #[error("erreur interne : {0}")]
    Internal(String),
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }
}
