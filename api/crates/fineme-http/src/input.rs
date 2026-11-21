//! DTO d'entrée (JSON → commandes des cas d'usage). Un champ nullable distingue « absent » de `null`.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use fineme_application::alerts::{AlertPatch, RulePatch};
use fineme_application::bundle::{Bundle, BundleAccount, BundleGoal, BundleMember, BundleOwner};
use fineme_application::commands::{
    AccountInput, GoalInput, HoldingInput, LoanInput, OperationImportInput, OperationInput,
    OwnerInput, Patch, PropertyInput, SecurityRef, TransactionInput, TransactionPatch,
};
use fineme_application::performance::BenchmarkInput;
use serde::{Deserialize, Deserializer};
use uuid::Uuid;

/// Désérialise « absent / null / valeur » en `Option<Option<T>>`
/// (avec `#[serde(default, deserialize_with = "nullable")]`).
fn nullable<'de, D, T>(deserializer: D) -> Result<Patch<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Deserialize)]
pub struct ViewQuery {
    pub member_id: Option<Uuid>,
}

#[derive(Deserialize)]
pub struct PerformanceQuery {
    pub member_id: Option<Uuid>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
}

#[derive(Deserialize)]
pub struct BenchmarkBody {
    name: String,
    symbol: Option<String>,
    isin: Option<String>,
    kind: Option<String>,
    currency: Option<String>,
}

impl From<BenchmarkBody> for BenchmarkInput {
    fn from(b: BenchmarkBody) -> Self {
        Self {
            name: b.name,
            symbol: b.symbol,
            isin: b.isin,
            kind: b.kind.unwrap_or_else(|| "security".into()),
            currency: b.currency.unwrap_or_else(|| "EUR".into()),
        }
    }
}

#[derive(Deserialize)]
pub struct RangeQuery {
    pub range: Option<String>,
}

#[derive(Deserialize)]
pub struct HistoryQuery {
    pub range: Option<String>,
    pub member_id: Option<Uuid>,
}

#[derive(Deserialize)]
pub struct HistoryReviewQuery {
    pub member_id: Option<Uuid>,
    /// Seuil de variation, fraction (0.2 = 20 %).
    pub threshold: Option<f64>,
}

#[derive(Deserialize)]
pub struct DateRangeQuery {
    pub from: NaiveDate,
    pub to: NaiveDate,
}

#[derive(Deserialize)]
pub struct HistoryCorrectionBody {
    pub gross_assets: f64,
    pub liabilities: f64,
}

#[derive(Deserialize)]
pub struct HistoryResetBody {
    pub confirmation: Option<String>,
}

#[derive(Deserialize)]
pub struct PageQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Deserialize)]
pub struct LimitQuery {
    pub limit: Option<i64>,
}

#[derive(Deserialize)]
pub struct SetupBody {
    pub email: String,
    pub password: String,
    pub name: String,
    /// Code de première configuration, exigé si `SETUP_TOKEN` est défini côté serveur.
    pub setup_token: Option<String>,
}

#[derive(Deserialize)]
pub struct LoginBody {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct LoginTotpBody {
    pub challenge: String,
    pub code: String,
}

#[derive(Deserialize)]
pub struct TotpCodeBody {
    pub code: String,
}

#[derive(Deserialize)]
pub struct TotpDisableBody {
    pub password: String,
    pub code: String,
}

#[derive(Deserialize)]
pub struct PasswordChangeBody {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Deserialize)]
pub struct PasswordResetBody {
    pub email: String,
    /// Code de secours, comparé à `RECOVERY_TOKEN` côté serveur.
    pub recovery_token: Option<String>,
    pub new_password: String,
}

#[derive(Deserialize)]
pub struct MemberBody {
    pub name: Option<String>,
    pub color: Option<String>,
}

#[derive(Deserialize)]
pub struct OwnerBody {
    member_id: Uuid,
    share: f64,
}

#[derive(Default, Deserialize)]
pub struct PropertyBody {
    #[serde(default, deserialize_with = "nullable")]
    address: Patch<String>,
    category: Option<String>,
    building_type: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    surface_m2: Patch<f64>,
    purchase_price: Option<f64>,
    purchase_fees: Option<f64>,
    #[serde(default, deserialize_with = "nullable")]
    purchased_at: Patch<NaiveDate>,
    current_value: Option<f64>,
    monthly_rent: Option<f64>,
    monthly_charges: Option<f64>,
    yearly_property_tax: Option<f64>,
}

impl From<PropertyBody> for PropertyInput {
    fn from(b: PropertyBody) -> Self {
        Self {
            address: b.address,
            category: b.category,
            building_type: b.building_type,
            surface_m2: b.surface_m2,
            purchase_price: b.purchase_price,
            purchase_fees: b.purchase_fees,
            purchased_at: b.purchased_at,
            current_value: b.current_value,
            monthly_rent: b.monthly_rent,
            monthly_charges: b.monthly_charges,
            yearly_property_tax: b.yearly_property_tax,
        }
    }
}

#[derive(Default, Deserialize)]
pub struct LoanBody {
    #[serde(default, deserialize_with = "nullable")]
    property_account_id: Patch<Uuid>,
    principal: Option<f64>,
    interest_rate_pct: Option<f64>,
    insurance_rate_pct: Option<f64>,
    duration_months: Option<i32>,
    start_date: Option<NaiveDate>,
    /// Bundle uniquement : compte immobilier financé, désigné par son `external_id`.
    property_external_id: Option<String>,
}

impl From<LoanBody> for LoanInput {
    fn from(b: LoanBody) -> Self {
        Self {
            property_account_id: b.property_account_id,
            principal: b.principal,
            interest_rate_pct: b.interest_rate_pct,
            insurance_rate_pct: b.insurance_rate_pct,
            duration_months: b.duration_months,
            start_date: b.start_date,
        }
    }
}

#[derive(Default, Deserialize)]
pub struct AccountBody {
    name: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    institution: Patch<String>,
    kind: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    envelope: Patch<String>,
    currency: Option<String>,
    cash_balance: Option<f64>,
    #[serde(default, deserialize_with = "nullable")]
    annual_yield_pct: Patch<f64>,
    #[serde(default, deserialize_with = "nullable")]
    opened_at: Patch<NaiveDate>,
    #[serde(default)]
    owners: Option<Vec<OwnerBody>>,
    #[serde(default)]
    property: Option<PropertyBody>,
    #[serde(default)]
    loan: Option<LoanBody>,
}

impl From<AccountBody> for AccountInput {
    fn from(b: AccountBody) -> Self {
        Self {
            name: b.name,
            institution: b.institution,
            kind: b.kind,
            envelope: b.envelope,
            currency: b.currency,
            cash_balance: b.cash_balance,
            annual_yield_pct: b.annual_yield_pct,
            opened_at: b.opened_at,
            owners: b.owners.map(|owners| {
                owners
                    .into_iter()
                    .map(|o| OwnerInput {
                        member_id: o.member_id,
                        share: o.share,
                    })
                    .collect()
            }),
            property: b.property.map(PropertyInput::from),
            loan: b.loan.map(LoanInput::from),
        }
    }
}

#[derive(Default, Deserialize)]
pub struct HoldingBody {
    kind: Option<String>,
    name: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    isin: Patch<String>,
    #[serde(default, deserialize_with = "nullable")]
    symbol: Patch<String>,
    #[serde(default, deserialize_with = "nullable")]
    security_type: Patch<String>,
    quantity: Option<f64>,
    #[serde(default, deserialize_with = "nullable")]
    buy_price: Patch<f64>,
    current_price: Option<f64>,
    currency: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    expense_ratio: Patch<f64>,
    price_source: Option<String>,
}

impl From<HoldingBody> for HoldingInput {
    fn from(b: HoldingBody) -> Self {
        Self {
            kind: b.kind,
            name: b.name,
            isin: b.isin,
            symbol: b.symbol,
            security_type: b.security_type,
            quantity: b.quantity,
            buy_price: b.buy_price,
            current_price: b.current_price,
            currency: b.currency,
            expense_ratio: b.expense_ratio,
            price_source: b.price_source,
        }
    }
}

#[derive(Deserialize)]
pub struct HoldingsBulkBody {
    pub holdings: Vec<HoldingBody>,
    #[serde(default)]
    pub replace: bool,
}

#[derive(Deserialize)]
pub struct TransactionBody {
    date: NaiveDate,
    label: String,
    amount: f64,
    category: Option<String>,
}

impl From<TransactionBody> for TransactionInput {
    fn from(b: TransactionBody) -> Self {
        Self {
            date: b.date,
            label: b.label,
            amount: b.amount,
            category: b.category,
        }
    }
}

#[derive(Default, Deserialize)]
pub struct TransactionPatchBody {
    #[serde(default, deserialize_with = "nullable")]
    category: Patch<String>,
    #[serde(default, deserialize_with = "nullable")]
    internal_transfer: Patch<bool>,
}

impl From<TransactionPatchBody> for TransactionPatch {
    fn from(b: TransactionPatchBody) -> Self {
        Self {
            category: b.category,
            internal_transfer: b.internal_transfer,
        }
    }
}

#[derive(Deserialize)]
pub struct BudgetQuery {
    pub month: Option<String>,
    pub member_id: Option<Uuid>,
}

#[derive(Deserialize)]
pub struct AlertsQuery {
    #[serde(default)]
    pub archived: bool,
}

#[derive(Deserialize)]
pub struct AlertPatchBody {
    pub read: Option<bool>,
    pub archived: Option<bool>,
}

impl From<AlertPatchBody> for AlertPatch {
    fn from(body: AlertPatchBody) -> Self {
        Self {
            read: body.read,
            archived: body.archived,
        }
    }
}

#[derive(Deserialize)]
pub struct AlertRuleBody {
    pub enabled: Option<bool>,
    pub threshold_pct: Option<f64>,
    pub ceiling: Option<f64>,
}

impl From<AlertRuleBody> for RulePatch {
    fn from(body: AlertRuleBody) -> Self {
        Self {
            enabled: body.enabled,
            threshold_pct: body.threshold_pct,
            ceiling: body.ceiling,
        }
    }
}

#[derive(Deserialize)]
pub struct BudgetTargetBody {
    pub category: String,
    pub monthly_amount: Option<f64>,
}

#[derive(Deserialize)]
pub struct TransactionsBulkBody {
    pub transactions: Vec<TransactionBody>,
    pub new_cash_balance: Option<f64>,
}

#[derive(Deserialize)]
pub struct OperationBody {
    kind: String,
    date: NaiveDate,
    holding_id: Option<Uuid>,
    quantity: Option<f64>,
    unit_price: Option<f64>,
    amount: Option<f64>,
    fees: Option<f64>,
    note: Option<String>,
}

impl From<OperationBody> for OperationInput {
    fn from(b: OperationBody) -> Self {
        Self {
            kind: b.kind,
            date: b.date,
            holding_id: b.holding_id,
            quantity: b.quantity,
            unit_price: b.unit_price,
            amount: b.amount,
            fees: b.fees,
            note: b.note,
        }
    }
}

#[derive(Deserialize)]
pub struct SecurityBody {
    name: String,
    isin: Option<String>,
}

#[derive(Deserialize)]
pub struct OperationImportBody {
    kind: String,
    date: NaiveDate,
    security: Option<SecurityBody>,
    quantity: Option<f64>,
    unit_price: Option<f64>,
    amount: Option<f64>,
    fees: Option<f64>,
    note: Option<String>,
}

impl From<OperationImportBody> for OperationImportInput {
    fn from(b: OperationImportBody) -> Self {
        Self {
            kind: b.kind,
            date: b.date,
            security: b.security.map(|s| SecurityRef {
                name: s.name,
                isin: s.isin,
            }),
            quantity: b.quantity,
            unit_price: b.unit_price,
            amount: b.amount,
            fees: b.fees,
            note: b.note,
        }
    }
}

#[derive(Deserialize)]
pub struct OperationsBulkBody {
    pub operations: Vec<OperationImportBody>,
}

#[derive(Deserialize)]
pub struct ImportProfileBody {
    #[serde(default)]
    pub template: Option<String>,
    pub columns: BTreeMap<String, String>,
}

#[derive(Default, Deserialize)]
pub struct GoalBody {
    name: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    icon: Patch<String>,
    kind: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    target_amount: Patch<f64>,
    #[serde(default, deserialize_with = "nullable")]
    target_date: Patch<NaiveDate>,
    #[serde(default, deserialize_with = "nullable")]
    monthly_contribution: Patch<f64>,
    #[serde(default, deserialize_with = "nullable")]
    expected_return_pct: Patch<f64>,
    #[serde(default, deserialize_with = "nullable")]
    monthly_expenses: Patch<f64>,
    #[serde(default, deserialize_with = "nullable")]
    coverage_months: Patch<f64>,
    #[serde(default)]
    account_ids: Option<Vec<Uuid>>,
    #[serde(default)]
    member_ids: Option<Vec<Uuid>>,
}

impl From<GoalBody> for GoalInput {
    fn from(b: GoalBody) -> Self {
        Self {
            name: b.name,
            icon: b.icon,
            kind: b.kind,
            target_amount: b.target_amount,
            target_date: b.target_date,
            monthly_contribution: b.monthly_contribution,
            expected_return_pct: b.expected_return_pct,
            monthly_expenses: b.monthly_expenses,
            coverage_months: b.coverage_months,
            account_ids: b.account_ids,
            member_ids: b.member_ids,
        }
    }
}

#[derive(Deserialize)]
pub struct ConnectionBody {
    pub provider: String,
}

// ---------------------------------------------------------------- bundle

#[derive(Deserialize)]
struct BundleMemberBody {
    name: String,
    color: Option<String>,
    is_primary: Option<bool>,
}

#[derive(Deserialize)]
struct BundleOwnerBody {
    member: String,
    share: f64,
}

#[derive(Deserialize)]
struct BundleAccountBody {
    external_id: String,
    owners: Vec<BundleOwnerBody>,
    holdings: Option<Vec<HoldingBody>>,
    #[serde(flatten)]
    account: AccountBody,
}

#[derive(Deserialize)]
struct BundleGoalBody {
    #[serde(default)]
    account_external_ids: Vec<String>,
    #[serde(default)]
    members: Vec<String>,
    #[serde(flatten)]
    goal: GoalBody,
}

#[derive(Deserialize)]
pub struct BundleBody {
    version: u32,
    #[serde(default)]
    members: Vec<BundleMemberBody>,
    #[serde(default)]
    accounts: Vec<BundleAccountBody>,
    #[serde(default)]
    goals: Vec<BundleGoalBody>,
}

impl From<BundleBody> for Bundle {
    fn from(body: BundleBody) -> Self {
        Self {
            version: body.version,
            members: body
                .members
                .into_iter()
                .map(|m| BundleMember {
                    name: m.name,
                    color: m.color,
                    is_primary: m.is_primary,
                })
                .collect(),
            accounts: body
                .accounts
                .into_iter()
                .map(|mut a| BundleAccount {
                    external_id: a.external_id,
                    property_external_id: a
                        .account
                        .loan
                        .as_mut()
                        .and_then(|loan| loan.property_external_id.take()),
                    owners: a
                        .owners
                        .into_iter()
                        .map(|o| BundleOwner {
                            member: o.member,
                            share: o.share,
                        })
                        .collect(),
                    holdings: a
                        .holdings
                        .map(|list| list.into_iter().map(HoldingInput::from).collect()),
                    account: a.account.into(),
                })
                .collect(),
            goals: body
                .goals
                .into_iter()
                .map(|g| BundleGoal {
                    account_external_ids: g.account_external_ids,
                    members: g.members,
                    goal: g.goal.into(),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn absent_null_and_value_are_distinguished() {
        let absent: AccountInput = serde_json::from_value::<AccountBody>(json!({}))
            .unwrap()
            .into();
        assert_eq!(absent, AccountInput::default());
        let body = json!({ "institution": null, "envelope": "pea", "annual_yield_pct": 1.7, "opened_at": null });
        let input: AccountInput = serde_json::from_value::<AccountBody>(body).unwrap().into();
        assert_eq!(input.institution, Some(None));
        assert_eq!(input.envelope, Some(Some("pea".into())));
        assert_eq!(input.annual_yield_pct, Some(Some(1.7)));
        assert_eq!(input.opened_at, Some(None));
        assert_eq!(input.name, None);
    }

    #[test]
    fn nested_inputs_and_integer_amounts_are_accepted() {
        let body = json!({
            "name": "Maison", "kind": "real_estate", "cash_balance": 0,
            "owners": [{ "member_id": "00000000-0000-0000-0000-000000000001", "share": 1 }],
            "property": { "purchase_price": 250000, "address": null, "purchased_at": "2022-03-01" },
            "loan": { "principal": 1000, "duration_months": 12, "start_date": "2026-06-01", "property_account_id": null }
        });
        let input: AccountInput = serde_json::from_value::<AccountBody>(body).unwrap().into();
        assert_eq!(input.owners.unwrap()[0].share, 1.0);
        let property = input.property.unwrap();
        assert_eq!(property.purchase_price, Some(250_000.0));
        assert_eq!(property.address, Some(None));
        assert_eq!(
            property.purchased_at,
            Some(NaiveDate::from_ymd_opt(2022, 3, 1))
        );
        let loan = input.loan.unwrap();
        assert_eq!(
            (loan.duration_months, loan.property_account_id),
            (Some(12), Some(None))
        );
    }

    #[test]
    fn invalid_types_are_rejected() {
        assert!(
            serde_json::from_value::<AccountBody>(json!({ "cash_balance": "beaucoup" })).is_err()
        );
        assert!(serde_json::from_value::<AccountBody>(json!({ "opened_at": "hier" })).is_err());
        assert!(serde_json::from_value::<HoldingsBulkBody>(json!({ "replace": true })).is_err());
        assert!(
            serde_json::from_value::<GoalBody>(json!({ "account_ids": ["pas-un-uuid"] })).is_err()
        );
    }

    #[test]
    fn bundle_flattens_account_fields_and_extracts_loan_link() {
        let body = json!({
            "version": 1,
            "members": [{ "name": "Alice" }],
            "accounts": [{
                "external_id": "pret-1", "name": "Prêt", "kind": "loan", "institution": null,
                "owners": [{ "member": "Alice", "share": 0.5 }],
                "loan": { "principal": 1000, "interest_rate_pct": 1, "duration_months": 12,
                          "start_date": "2026-06-01", "property_external_id": "maison-1" }
            }, {
                "external_id": "pea-1", "name": "PEA", "kind": "investment", "owners": [],
                "holdings": [{ "name": "ETF", "quantity": 2, "buy_price": null }]
            }],
            "goals": [{ "name": "Matelas", "kind": "safety_net", "target_date": null,
                        "account_external_ids": ["pea-1"], "members": ["Alice"] }]
        });
        let bundle: Bundle = serde_json::from_value::<BundleBody>(body).unwrap().into();
        assert_eq!(bundle.version, 1);
        let loan = &bundle.accounts[0];
        assert_eq!(loan.property_external_id.as_deref(), Some("maison-1"));
        assert_eq!(loan.account.name.as_deref(), Some("Prêt"));
        assert_eq!(loan.account.institution, Some(None));
        // Les détenteurs par nom ne se retrouvent pas dans les champs du compte.
        assert_eq!(loan.account.owners, None);
        assert_eq!(
            loan.owners[0],
            BundleOwner {
                member: "Alice".into(),
                share: 0.5
            }
        );
        let pea = &bundle.accounts[1];
        let holding = &pea.holdings.as_ref().unwrap()[0];
        assert_eq!(
            (holding.quantity, holding.buy_price),
            (Some(2.0), Some(None))
        );
        assert_eq!(bundle.goals[0].goal.target_date, Some(None));
        assert_eq!(
            bundle.goals[0].account_external_ids,
            vec!["pea-1".to_string()]
        );
    }

    #[test]
    fn bundle_lists_default_to_empty() {
        let bundle: Bundle = serde_json::from_value::<BundleBody>(json!({ "version": 1 }))
            .unwrap()
            .into();
        assert!(bundle.members.is_empty() && bundle.accounts.is_empty() && bundle.goals.is_empty());
        assert!(serde_json::from_value::<BundleBody>(json!({})).is_err());
    }
}
