//! Entrées des cas d'usage, en types simples. Un champ `Patch` distingue « absent » (inchangé),
//! « vide » (effacer) et « valeur ».

use chrono::NaiveDate;
use uuid::Uuid;

/// `None` = inchangé, `Some(None)` = effacer, `Some(Some(v))` = nouvelle valeur.
pub type Patch<T> = Option<Option<T>>;

#[derive(Debug, Clone, PartialEq)]
pub struct OwnerInput {
    pub member_id: Uuid,
    pub share: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PropertyInput {
    pub address: Patch<String>,
    pub category: Option<String>,
    pub building_type: Option<String>,
    pub surface_m2: Patch<f64>,
    pub purchase_price: Option<f64>,
    pub purchase_fees: Option<f64>,
    pub purchased_at: Patch<NaiveDate>,
    pub current_value: Option<f64>,
    pub monthly_rent: Option<f64>,
    pub monthly_charges: Option<f64>,
    pub yearly_property_tax: Option<f64>,
}

/// Estimation d'un bien : `value`, ou `price_per_m2` (valeur = surface du bien × prix).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ValuationInput {
    pub value: Option<f64>,
    pub price_per_m2: Option<f64>,
    pub valued_on: Option<NaiveDate>,
    pub source: Option<String>,
    pub note: Patch<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LoanInput {
    pub property_account_id: Patch<Uuid>,
    pub principal: Option<f64>,
    pub interest_rate_pct: Option<f64>,
    pub insurance_rate_pct: Option<f64>,
    pub duration_months: Option<i32>,
    pub start_date: Option<NaiveDate>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AccountInput {
    pub name: Option<String>,
    pub institution: Patch<String>,
    pub kind: Option<String>,
    pub envelope: Patch<String>,
    pub currency: Option<String>,
    pub cash_balance: Option<f64>,
    pub annual_yield_pct: Patch<f64>,
    pub opened_at: Patch<NaiveDate>,
    pub owners: Option<Vec<OwnerInput>>,
    pub property: Option<PropertyInput>,
    pub loan: Option<LoanInput>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct HoldingInput {
    pub kind: Option<String>,
    pub name: Option<String>,
    pub isin: Patch<String>,
    pub symbol: Patch<String>,
    pub security_type: Patch<String>,
    pub quantity: Option<f64>,
    pub buy_price: Patch<f64>,
    pub current_price: Option<f64>,
    pub currency: Option<String>,
    pub expense_ratio: Patch<f64>,
    pub price_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransactionInput {
    pub date: NaiveDate,
    pub label: String,
    pub amount: f64,
    pub category: Option<String>,
}

/// Choix manuels du budget sur un mouvement.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TransactionPatch {
    pub category: Patch<String>,
    pub internal_transfer: Patch<bool>,
}

/// Opération du journal d'un compte d'investissement (création ou remplacement complet).
#[derive(Debug, Clone, PartialEq)]
pub struct OperationInput {
    pub kind: String,
    pub date: NaiveDate,
    pub holding_id: Option<Uuid>,
    pub quantity: Option<f64>,
    pub unit_price: Option<f64>,
    pub amount: Option<f64>,
    pub fees: Option<f64>,
    pub note: Option<String>,
}

/// Titre désigné par une ligne d'import : rapproché d'une position du compte par ISIN, sinon par nom.
#[derive(Debug, Clone, PartialEq)]
pub struct SecurityRef {
    pub name: String,
    pub isin: Option<String>,
}

/// Ligne d'un import CSV d'opérations : comme `OperationInput`, mais le titre est désigné par son nom.
#[derive(Debug, Clone, PartialEq)]
pub struct OperationImportInput {
    pub kind: String,
    pub date: NaiveDate,
    pub security: Option<SecurityRef>,
    pub quantity: Option<f64>,
    pub unit_price: Option<f64>,
    pub amount: Option<f64>,
    pub fees: Option<f64>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct GoalInput {
    pub name: Option<String>,
    pub icon: Patch<String>,
    pub kind: Option<String>,
    pub target_amount: Patch<f64>,
    pub target_date: Patch<NaiveDate>,
    pub monthly_contribution: Patch<f64>,
    pub expected_return_pct: Patch<f64>,
    pub monthly_expenses: Patch<f64>,
    pub coverage_months: Patch<f64>,
    pub account_ids: Option<Vec<Uuid>>,
    pub member_ids: Option<Vec<Uuid>>,
}

/// Nettoie un texte optionnel : espaces retirés, vide → `None`.
pub fn clean(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}
