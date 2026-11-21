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
    /// Trop de tentatives d'authentification : réessayer après ce nombre de secondes.
    #[error("{}", too_many_attempts_message(*.0))]
    TooManyAttempts(u64),
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

/// « Trop de tentatives. Réessayez dans 30 secondes. » (en minutes au-delà d'une minute).
fn too_many_attempts_message(retry_after_secs: u64) -> String {
    let wait = if retry_after_secs <= 60 {
        let plural = if retry_after_secs > 1 { "s" } else { "" };
        format!("{retry_after_secs} seconde{plural}")
    } else {
        format!("{} minutes", retry_after_secs.div_ceil(60))
    };
    format!("Trop de tentatives. Réessayez dans {wait}.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn too_many_attempts_message_is_readable() {
        assert_eq!(
            AppError::TooManyAttempts(1).to_string(),
            "Trop de tentatives. Réessayez dans 1 seconde."
        );
        assert_eq!(
            AppError::TooManyAttempts(30).to_string(),
            "Trop de tentatives. Réessayez dans 30 secondes."
        );
        assert_eq!(
            AppError::TooManyAttempts(61).to_string(),
            "Trop de tentatives. Réessayez dans 2 minutes."
        );
    }
}
