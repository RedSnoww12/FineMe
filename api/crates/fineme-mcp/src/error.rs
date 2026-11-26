//! Erreurs d'un tool : renvoyées au client IA en `isError` avec un message en français qui dit quoi corriger,
//! jamais de trace interne.

use fineme_application::AppError;

/// Message montré quand l'échec vient du serveur et non de la demande.
const INTERNAL: &str = "FineMe n'a pas pu répondre (erreur interne). Réessayez dans un instant ; si cela persiste, \
     vérifiez que le serveur FineMe et sa base de données sont joignables.";

#[derive(Debug)]
pub enum ToolError {
    /// Paramètre absent ou mal formé.
    Invalid(String),
    App(AppError),
}

impl From<AppError> for ToolError {
    fn from(error: AppError) -> Self {
        Self::App(error)
    }
}

impl ToolError {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }

    /// Message destiné au client. Les erreurs internes sont journalisées côté serveur et masquées.
    pub fn message(&self) -> String {
        match self {
            Self::Invalid(message) => message.clone(),
            Self::App(AppError::NotFound("Membre introuvable")) => {
                "Membre introuvable : utilisez un member_id renvoyé par get_overview (champ members), \
                 ou omettez-le pour le foyer entier."
                    .to_string()
            }
            Self::App(
                error @ (AppError::Domain(_)
                | AppError::Invalid(_)
                | AppError::NotFound(_)
                | AppError::Conflict(_)
                | AppError::Unauthorized(_)
                | AppError::Forbidden(_)
                | AppError::TooManyAttempts(_)
                | AppError::NotConfigured(_)),
            ) => error.to_string(),
            Self::App(
                error @ (AppError::Repository(_) | AppError::Provider(_) | AppError::Internal(_)),
            ) => {
                tracing::error!("tool MCP en échec : {error}");
                INTERNAL.to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fineme_application::ports::RepoError;
    use fineme_domain::DomainError;

    #[test]
    fn user_errors_are_shown_and_internal_ones_are_hidden() {
        assert_eq!(
            ToolError::invalid("Date invalide").message(),
            "Date invalide"
        );
        assert_eq!(
            ToolError::from(AppError::Domain(DomainError::Rule("Période invalide"))).message(),
            "Période invalide"
        );
        assert!(
            ToolError::from(AppError::NotFound("Membre introuvable"))
                .message()
                .contains("get_overview")
        );
        let hidden = ToolError::from(AppError::Repository(RepoError::Backend(
            "connection refused: postgres://secret@host".into(),
        )))
        .message();
        assert_eq!(hidden, INTERNAL);
        assert!(!hidden.contains("postgres"));
    }
}
