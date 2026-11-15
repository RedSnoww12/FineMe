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
    pub category: Option<String>,
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
