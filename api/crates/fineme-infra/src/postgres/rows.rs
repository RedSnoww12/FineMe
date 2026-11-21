//! Lignes SQL et conversion vers le domaine. Une donnée stockée qui viole un invariant du domaine
//! est signalée comme erreur de persistance plutôt que propagée.

use chrono::{DateTime, NaiveDate, Utc};
use fineme_application::ports::{RepoError, RepoResult};
use fineme_domain::DomainError;
use fineme_domain::account::{Account, AccountKind, AccountSource};
use fineme_domain::budget::{BudgetTarget, CategoryRule};
use fineme_domain::connection::{Connection, ConnectionStatus};
use fineme_domain::fx::Currency;
use fineme_domain::goal::{Goal, GoalKind};
use fineme_domain::holding::{Holding, HoldingKind, PriceSource};
use fineme_domain::ids::{
    AccountId, BudgetRuleId, ConnectionId, GoalId, HoldingId, LoanId, MemberId, OperationId,
    PropertyId, TransactionId, ValuationId,
};
use fineme_domain::import_profile::{ImportKind, ImportProfile};
use fineme_domain::loan::{Loan, LoanTerms};
use fineme_domain::member::Member;
use fineme_domain::operation::{Movement, MovementFields, Operation, OperationKind};
use fineme_domain::ownership::Ownership;
use fineme_domain::portfolio::AccountSnapshot;
use fineme_domain::property::{BuildingType, Property, PropertyCategory};
use fineme_domain::transaction::Transaction;
use fineme_domain::valuation::{EstimatedValue, Valuation, ValuationDate, ValuationSource};
use sqlx::FromRow;
use uuid::Uuid;

pub(super) fn corrupt(table: &str, id: Uuid) -> impl Fn(DomainError) -> RepoError + '_ {
    move |error| RepoError::Backend(format!("donnée invalide dans {table} ({id}) : {error}"))
}

#[derive(FromRow)]
pub(super) struct MemberRow {
    id: Uuid,
    name: String,
    color: String,
    is_primary: bool,
}

impl From<MemberRow> for Member {
    fn from(row: MemberRow) -> Self {
        Member {
            id: MemberId::from_uuid(row.id),
            name: row.name,
            color: row.color,
            is_primary: row.is_primary,
        }
    }
}

#[derive(FromRow)]
pub(super) struct AccountRow {
    pub id: Uuid,
    name: String,
    institution: Option<String>,
    kind: String,
    envelope: Option<String>,
    currency: String,
    cash_balance: f64,
    annual_yield_pct: Option<f64>,
    source: String,
    external_id: Option<String>,
    connection_id: Option<Uuid>,
    opened_at: Option<NaiveDate>,
    updated_at: DateTime<Utc>,
}

impl AccountRow {
    pub fn into_account(
        self,
        owners: Vec<(Uuid, f64)>,
        property: Option<Property>,
        loan: Option<Loan>,
    ) -> RepoResult<Account> {
        let invalid = corrupt("accounts", self.id);
        Ok(Account {
            id: AccountId::from_uuid(self.id),
            name: self.name,
            institution: self.institution,
            kind: AccountKind::parse(&self.kind).map_err(&invalid)?,
            envelope: self.envelope,
            currency: Currency::parse(&self.currency).map_err(&invalid)?,
            cash_balance: self.cash_balance,
            annual_yield_pct: self.annual_yield_pct,
            source: AccountSource::parse(&self.source).map_err(&invalid)?,
            external_id: self.external_id,
            connection_id: self.connection_id.map(ConnectionId::from_uuid),
            opened_at: self.opened_at,
            updated_at: self.updated_at,
            owners: Ownership::from_fractions(
                owners
                    .into_iter()
                    .map(|(id, share)| (MemberId::from_uuid(id), share)),
            )
            .map_err(&invalid)?,
            property,
            loan,
        })
    }
}

#[derive(FromRow)]
pub(super) struct OwnerRow {
    pub account_id: Uuid,
    pub member_id: Uuid,
    pub share: f64,
}

#[derive(FromRow)]
pub(super) struct HoldingRow {
    id: Uuid,
    account_id: Uuid,
    kind: String,
    name: String,
    isin: Option<String>,
    symbol: Option<String>,
    security_type: Option<String>,
    quantity: f64,
    buy_price: Option<f64>,
    current_price: f64,
    currency: String,
    expense_ratio: Option<f64>,
    price_source: String,
    price_updated_at: Option<DateTime<Utc>>,
}

impl TryFrom<HoldingRow> for Holding {
    type Error = RepoError;

    fn try_from(row: HoldingRow) -> RepoResult<Self> {
        let invalid = corrupt("holdings", row.id);
        Ok(Holding {
            id: HoldingId::from_uuid(row.id),
            account_id: AccountId::from_uuid(row.account_id),
            kind: HoldingKind::parse(&row.kind).map_err(&invalid)?,
            name: row.name,
            isin: row.isin,
            symbol: row.symbol,
            security_type: row.security_type,
            quantity: row.quantity,
            buy_price: row.buy_price,
            current_price: row.current_price,
            currency: Currency::parse(&row.currency).map_err(&invalid)?,
            expense_ratio: row.expense_ratio,
            price_source: PriceSource::parse(&row.price_source).map_err(&invalid)?,
            price_updated_at: row.price_updated_at,
        })
    }
}

#[derive(FromRow)]
pub(super) struct PropertyRow {
    id: Uuid,
    pub account_id: Uuid,
    address: Option<String>,
    category: String,
    building_type: String,
    surface_m2: Option<f64>,
    purchase_price: f64,
    purchase_fees: f64,
    purchased_at: Option<NaiveDate>,
    current_value: f64,
    monthly_rent: f64,
    monthly_charges: f64,
    yearly_property_tax: f64,
}

impl TryFrom<PropertyRow> for Property {
    type Error = RepoError;

    fn try_from(row: PropertyRow) -> RepoResult<Self> {
        let invalid = corrupt("properties", row.id);
        Ok(Property {
            id: PropertyId::from_uuid(row.id),
            address: row.address,
            category: PropertyCategory::parse(&row.category).map_err(&invalid)?,
            building_type: BuildingType::parse(&row.building_type).map_err(&invalid)?,
            surface_m2: row.surface_m2,
            purchase_price: row.purchase_price,
            purchase_fees: row.purchase_fees,
            purchased_at: row.purchased_at,
            current_value: row.current_value,
            valuations: Vec::new(),
            monthly_rent: row.monthly_rent,
            monthly_charges: row.monthly_charges,
            yearly_property_tax: row.yearly_property_tax,
        })
    }
}

#[derive(FromRow)]
pub(super) struct ValuationRow {
    id: Uuid,
    pub account_id: Uuid,
    valued_on: NaiveDate,
    value: f64,
    source: String,
    price_per_m2: Option<f64>,
    note: Option<String>,
    recorded_at: DateTime<Utc>,
}

impl TryFrom<ValuationRow> for Valuation {
    type Error = RepoError;

    fn try_from(row: ValuationRow) -> RepoResult<Self> {
        let invalid = corrupt("property_valuations", row.id);
        Ok(Valuation {
            id: ValuationId::from_uuid(row.id),
            valued_on: ValuationDate::restore(row.valued_on),
            value: EstimatedValue::new(row.value).map_err(&invalid)?,
            source: ValuationSource::parse(&row.source).map_err(&invalid)?,
            price_per_m2: row.price_per_m2,
            note: row.note,
            recorded_at: row.recorded_at,
        })
    }
}

#[derive(FromRow)]
pub(super) struct LoanRow {
    id: Uuid,
    pub account_id: Uuid,
    property_account_id: Option<Uuid>,
    principal: f64,
    interest_rate_pct: f64,
    insurance_rate_pct: f64,
    duration_months: i32,
    start_date: NaiveDate,
}

impl TryFrom<LoanRow> for Loan {
    type Error = RepoError;

    fn try_from(row: LoanRow) -> RepoResult<Self> {
        let terms = LoanTerms::new(
            row.principal,
            row.interest_rate_pct,
            row.insurance_rate_pct,
            i64::from(row.duration_months),
            row.start_date,
        )
        .map_err(corrupt("loans", row.id))?;
        Ok(Loan {
            id: LoanId::from_uuid(row.id),
            property_account_id: row.property_account_id.map(AccountId::from_uuid),
            terms,
        })
    }
}

#[derive(FromRow)]
pub(super) struct TransactionRow {
    id: Uuid,
    account_id: Uuid,
    date: NaiveDate,
    label: String,
    amount: f64,
    category: Option<String>,
    manual_category: Option<String>,
    transfer_override: Option<bool>,
}

impl From<TransactionRow> for Transaction {
    fn from(row: TransactionRow) -> Self {
        Transaction {
            id: TransactionId::from_uuid(row.id),
            account_id: AccountId::from_uuid(row.account_id),
            date: row.date,
            label: row.label,
            amount: row.amount,
            category: row.category,
            manual_category: row.manual_category,
            transfer_override: row.transfer_override,
        }
    }
}

#[derive(FromRow)]
pub(super) struct OperationRow {
    id: Uuid,
    account_id: Uuid,
    holding_id: Option<Uuid>,
    kind: String,
    date: NaiveDate,
    quantity: Option<f64>,
    unit_price: Option<f64>,
    amount: Option<f64>,
    fees: f64,
    currency: String,
    note: Option<String>,
    recorded_at: DateTime<Utc>,
}

impl TryFrom<OperationRow> for Operation {
    type Error = RepoError;

    fn try_from(row: OperationRow) -> RepoResult<Self> {
        let invalid = corrupt("investment_operations", row.id);
        let kind = OperationKind::parse(&row.kind).map_err(&invalid)?;
        let movement = Movement::new(
            kind,
            MovementFields {
                holding: row.holding_id.map(HoldingId::from_uuid),
                quantity: row.quantity,
                unit_price: row.unit_price,
                amount: row.amount,
                fees: Some(row.fees),
            },
        )
        .map_err(&invalid)?;
        Ok(Operation {
            id: OperationId::from_uuid(row.id),
            account_id: AccountId::from_uuid(row.account_id),
            date: row.date,
            movement,
            currency: Currency::parse(&row.currency).map_err(&invalid)?,
            note: row.note,
            recorded_at: row.recorded_at,
        })
    }
}

#[derive(FromRow)]
pub(super) struct ImportProfileRow {
    account_id: Uuid,
    kind: String,
    template: Option<String>,
    fields: Vec<String>,
    headers: Vec<String>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<ImportProfileRow> for ImportProfile {
    type Error = RepoError;

    fn try_from(row: ImportProfileRow) -> RepoResult<Self> {
        let invalid = corrupt("import_profiles", row.account_id);
        let kind = ImportKind::parse(&row.kind).map_err(&invalid)?;
        ImportProfile::new(
            AccountId::from_uuid(row.account_id),
            kind,
            row.template,
            row.fields.into_iter().zip(row.headers),
            row.updated_at,
        )
        .map_err(&invalid)
    }
}

#[derive(FromRow)]
pub(super) struct GoalRow {
    pub id: Uuid,
    name: String,
    icon: Option<String>,
    kind: String,
    target_amount: Option<f64>,
    target_date: Option<NaiveDate>,
    monthly_contribution: Option<f64>,
    expected_return_pct: Option<f64>,
    monthly_expenses: Option<f64>,
    coverage_months: Option<f64>,
}

impl GoalRow {
    pub fn into_goal(
        self,
        account_ids: Vec<AccountId>,
        member_ids: Vec<MemberId>,
    ) -> RepoResult<Goal> {
        Ok(Goal {
            id: GoalId::from_uuid(self.id),
            kind: GoalKind::parse(&self.kind).map_err(corrupt("goals", self.id))?,
            name: self.name,
            icon: self.icon,
            target_amount: self.target_amount,
            target_date: self.target_date,
            monthly_contribution: self.monthly_contribution,
            expected_return_pct: self.expected_return_pct,
            monthly_expenses: self.monthly_expenses,
            coverage_months: self.coverage_months,
            account_ids,
            member_ids,
        })
    }
}

#[derive(FromRow)]
pub(super) struct SnapshotAccountRow {
    pub date: NaiveDate,
    account_id: Uuid,
    name: String,
    kind: String,
    value: f64,
    gain: Option<f64>,
}

impl TryFrom<SnapshotAccountRow> for AccountSnapshot {
    type Error = RepoError;

    fn try_from(row: SnapshotAccountRow) -> RepoResult<Self> {
        Ok(AccountSnapshot {
            account_id: AccountId::from_uuid(row.account_id),
            kind: AccountKind::parse(&row.kind)
                .map_err(corrupt("snapshot_accounts", row.account_id))?,
            name: row.name,
            value: row.value,
            gain: row.gain,
        })
    }
}

#[derive(FromRow)]
pub(super) struct ConnectionRow {
    id: Uuid,
    provider: String,
    institution: Option<String>,
    status: String,
    last_sync_at: Option<DateTime<Utc>>,
    error: Option<String>,
    external_id: Option<String>,
}

impl TryFrom<ConnectionRow> for Connection {
    type Error = RepoError;

    fn try_from(row: ConnectionRow) -> RepoResult<Self> {
        Ok(Connection {
            id: ConnectionId::from_uuid(row.id),
            status: ConnectionStatus::parse(&row.status).map_err(corrupt("connections", row.id))?,
            provider: row.provider,
            institution: row.institution,
            last_sync_at: row.last_sync_at,
            error: row.error,
            external_id: row.external_id,
        })
    }
}

#[derive(FromRow)]
pub(super) struct BudgetRuleRow {
    id: Uuid,
    pattern: String,
    category: String,
}

impl TryFrom<BudgetRuleRow> for CategoryRule {
    type Error = RepoError;
    fn try_from(row: BudgetRuleRow) -> Result<Self, RepoError> {
        CategoryRule::new(BudgetRuleId::from_uuid(row.id), &row.pattern, &row.category)
            .map_err(corrupt("budget_rules", row.id))
    }
}

#[derive(FromRow)]
pub(super) struct BudgetTargetRow {
    category: String,
    monthly_amount: f64,
}

impl TryFrom<BudgetTargetRow> for BudgetTarget {
    type Error = RepoError;
    fn try_from(row: BudgetTargetRow) -> Result<Self, RepoError> {
        BudgetTarget::new(&row.category, row.monthly_amount).map_err(|error| {
            RepoError::Backend(format!("donnée invalide dans budget_targets : {error}"))
        })
    }
}
