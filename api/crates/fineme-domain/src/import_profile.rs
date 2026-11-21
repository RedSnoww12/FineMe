//! Préférence d'import CSV d'un compte : quelle colonne du fichier alimente quel champ.
//! Les colonnes sont désignées par leur nom d'en-tête, pour survivre à un changement d'ordre.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::error::DomainError;
use crate::ids::AccountId;
use crate::property::string_enum;

string_enum!(ImportKind {
    Transactions => "transactions", Holdings => "holdings", Operations => "operations"
} invalid = "Type d'import invalide (transactions, holdings, operations)");

const MAX_HEADER_LEN: usize = 200;
const MAX_TEMPLATE_LEN: usize = 64;

impl ImportKind {
    /// Champs qu'un fichier de ce type peut alimenter.
    pub const fn fields(&self) -> &'static [&'static str] {
        match self {
            ImportKind::Transactions => &["date", "label", "amount", "debit", "credit", "category"],
            ImportKind::Holdings => &["name", "isin", "quantity", "buy_price", "current_price"],
            ImportKind::Operations => &[
                "date",
                "kind",
                "name",
                "isin",
                "quantity",
                "unit_price",
                "amount",
                "debit",
                "credit",
                "fees",
                "note",
            ],
        }
    }
}

/// Correspondance champ → nom de colonne mémorisée pour un compte et un type d'import.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportProfile {
    account_id: AccountId,
    kind: ImportKind,
    template: Option<String>,
    columns: BTreeMap<String, String>,
    updated_at: DateTime<Utc>,
}

impl ImportProfile {
    /// Valide la correspondance : champs connus du type, au moins une colonne, noms non vides.
    pub fn new(
        account_id: AccountId,
        kind: ImportKind,
        template: Option<String>,
        columns: impl IntoIterator<Item = (String, String)>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, DomainError> {
        let mut mapped = BTreeMap::new();
        for (field, header) in columns {
            if !kind.fields().contains(&field.as_str()) {
                return Err(DomainError::Rule(
                    "Champ inconnu pour ce type d'import dans la correspondance des colonnes",
                ));
            }
            let header = header.trim().to_string();
            if header.is_empty() || header.chars().count() > MAX_HEADER_LEN {
                return Err(DomainError::Rule(
                    "Chaque colonne doit avoir un nom non vide de 200 caractères au plus",
                ));
            }
            mapped.insert(field, header);
        }
        if mapped.is_empty() {
            return Err(DomainError::Rule(
                "La correspondance doit désigner au moins une colonne",
            ));
        }
        let template = template
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty());
        if template
            .as_ref()
            .is_some_and(|id| id.chars().count() > MAX_TEMPLATE_LEN)
        {
            return Err(DomainError::Rule(
                "L'identifiant du modèle d'import est trop long",
            ));
        }
        Ok(Self {
            account_id,
            kind,
            template,
            columns: mapped,
            updated_at,
        })
    }

    pub const fn account_id(&self) -> AccountId {
        self.account_id
    }
    pub const fn kind(&self) -> ImportKind {
        self.kind
    }
    pub fn template(&self) -> Option<&str> {
        self.template.as_deref()
    }
    pub const fn columns(&self) -> &BTreeMap<String, String> {
        &self.columns
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn columns(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(field, header)| (field.to_string(), header.to_string()))
            .collect()
    }

    fn profile(
        kind: ImportKind,
        template: Option<String>,
        pairs: &[(&str, &str)],
    ) -> Result<ImportProfile, DomainError> {
        ImportProfile::new(
            AccountId::generate(),
            kind,
            template,
            columns(pairs),
            DateTime::UNIX_EPOCH,
        )
    }

    #[test]
    fn keeps_known_fields_with_trimmed_headers() {
        let p = profile(
            ImportKind::Transactions,
            Some(" banque-generique ".into()),
            &[("date", " Date opération "), ("debit", "Débit")],
        )
        .unwrap();
        assert_eq!(p.columns()["date"], "Date opération");
        assert_eq!(p.template(), Some("banque-generique"));
        assert_eq!(p.kind().as_str(), "transactions");
    }

    #[test]
    fn refuses_unknown_fields_empty_headers_and_empty_mappings() {
        assert!(profile(ImportKind::Holdings, None, &[("label", "Libellé")]).is_err());
        assert!(profile(ImportKind::Operations, None, &[("kind", "  ")]).is_err());
        assert!(profile(ImportKind::Operations, None, &[]).is_err());
        let long = "x".repeat(201);
        assert!(profile(ImportKind::Operations, None, &[("note", long.as_str())]).is_err());
        assert!(ImportKind::parse("budget").is_err());
    }

    #[test]
    fn blank_template_is_absent_and_long_template_refused() {
        let blank = profile(ImportKind::Holdings, Some("  ".into()), &[("name", "Nom")]).unwrap();
        assert_eq!(blank.template(), None);
        assert!(
            profile(
                ImportKind::Holdings,
                Some("t".repeat(65)),
                &[("name", "Nom")]
            )
            .is_err()
        );
    }
}
