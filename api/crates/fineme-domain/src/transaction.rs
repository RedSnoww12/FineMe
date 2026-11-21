use chrono::NaiveDate;

use crate::ids::{AccountId, TransactionId};

/// Mouvement d'un compte ; montant signé dans la devise du compte.
#[derive(Debug, Clone, PartialEq)]
pub struct Transaction {
    pub id: TransactionId,
    pub account_id: AccountId,
    pub date: NaiveDate,
    pub label: String,
    pub amount: f64,
    /// Catégorie reçue à l'import (banque, CSV).
    pub category: Option<String>,
    /// Catégorie choisie à la main dans le budget ; prioritaire sur toute règle.
    pub manual_category: Option<String>,
    /// Marquage manuel de virement interne ; `None` = détection automatique.
    pub transfer_override: Option<bool>,
}

impl Transaction {
    /// Deux mouvements sont des doublons s'ils ont même compte, date, libellé et montant.
    pub fn is_duplicate_of(&self, other: &Transaction) -> bool {
        self.account_id == other.account_id
            && self.date == other.date
            && self.label == other.label
            && self.amount == other.amount
    }
}
