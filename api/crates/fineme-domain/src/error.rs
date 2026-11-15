/// Violation d'un invariant ou d'une règle métier. Les messages sont destinés à l'utilisateur (français).
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DomainError {
    #[error("Chaque part doit être comprise entre 0 et 1")]
    InvalidShare,
    #[error("Au moins un propriétaire est requis")]
    NoOwner,
    #[error("Un membre ne peut apparaître qu'une fois parmi les propriétaires")]
    DuplicateOwner,
    #[error("La somme des parts ne peut pas dépasser 100 %")]
    SharesExceedWhole,
    #[error("Devise invalide (code ISO à 3 lettres attendu)")]
    InvalidCurrency,
    #[error("Valeur invalide pour {0}")]
    NotFinite(&'static str),
    #[error("{0} ne peut pas être négatif")]
    Negative(&'static str),
    /// Règle métier exprimée directement par son message.
    #[error("{0}")]
    Rule(&'static str),
}

/// Vérifie qu'un nombre est fini (ni NaN ni infini).
pub fn finite(value: f64, field: &'static str) -> Result<f64, DomainError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(DomainError::NotFinite(field))
    }
}

/// Vérifie qu'un nombre est fini et positif ou nul.
pub fn non_negative(value: f64, field: &'static str) -> Result<f64, DomainError> {
    if finite(value, field)? < 0.0 {
        Err(DomainError::Negative(field))
    } else {
        Ok(value)
    }
}
