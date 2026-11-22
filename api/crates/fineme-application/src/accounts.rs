use std::sync::Arc;

use chrono::{DateTime, Utc};
use fineme_domain::DomainError;
use fineme_domain::account::{Account, AccountKind, AccountSource};
use fineme_domain::error::{finite, non_negative};
use fineme_domain::fx::Currency;
use fineme_domain::ids::{AccountId, LoanId, MemberId, PropertyId};
use fineme_domain::loan::{Loan, LoanTerms};
use fineme_domain::member::Member;
use fineme_domain::ownership::Ownership;
use fineme_domain::portfolio::{AccountDetail, AccountView};
use fineme_domain::property::{BuildingType, Property, PropertyCategory};
use fineme_domain::valuation::{EstimatedValue, Valuation};
use uuid::Uuid;

use crate::commands::{AccountInput, LoanInput, OwnerInput, PropertyInput, clean};
use crate::error::{AppError, AppResult};
use crate::ports::{AccountWrite, Clock, PortfolioRepository};
use crate::reader::{PortfolioReader, view_for};

/// Origine d'un nouveau compte.
#[derive(Debug, Clone)]
pub(crate) struct Origin {
    pub source: AccountSource,
    pub external_id: Option<String>,
}

impl Origin {
    pub fn manual() -> Self {
        Self {
            source: AccountSource::Manual,
            external_id: None,
        }
    }
}

pub(crate) fn ownership(owners: &[OwnerInput], members: &[Member]) -> AppResult<Ownership> {
    let ownership = Ownership::from_fractions(
        owners
            .iter()
            .map(|owner| (MemberId::from_uuid(owner.member_id), owner.share)),
    )?;
    if ownership
        .owners()
        .iter()
        .any(|(id, _)| !members.iter().any(|member| member.id == *id))
    {
        return Err(AppError::invalid(
            "Un des propriétaires ne correspond à aucun membre",
        ));
    }
    Ok(ownership)
}

/// Fusionne l'entrée avec le bien existant, ou en crée un (prix d'achat ou valeur actuelle requis).
/// Une valeur actuelle nouvelle ou modifiée est enregistrée comme estimation manuelle datée du jour.
fn merge_property(
    existing: Option<&Property>,
    input: &PropertyInput,
    now: DateTime<Utc>,
) -> AppResult<Property> {
    let mut property = match existing {
        Some(property) => property.clone(),
        None => {
            let base = input
                .current_value
                .or(input.purchase_price)
                .ok_or_else(|| {
                    AppError::invalid("Le bien doit indiquer purchase_price ou current_value")
                })?;
            Property {
                id: PropertyId::generate(),
                address: None,
                category: PropertyCategory::Primary,
                building_type: BuildingType::Apartment,
                surface_m2: None,
                purchase_price: input.purchase_price.unwrap_or(base),
                purchase_fees: 0.0,
                purchased_at: None,
                current_value: base,
                valuations: Vec::new(),
                monthly_rent: 0.0,
                monthly_charges: 0.0,
                yearly_property_tax: 0.0,
            }
        }
    };
    if let Some(address) = &input.address {
        property.address = clean(address.clone());
    }
    if let Some(category) = &input.category {
        property.category = PropertyCategory::parse(category)?;
    }
    if let Some(building_type) = &input.building_type {
        property.building_type = BuildingType::parse(building_type)?;
    }
    if let Some(surface) = input.surface_m2 {
        property.surface_m2 = surface;
    }
    if let Some(purchased_at) = input.purchased_at {
        property.purchased_at = purchased_at;
    }
    for (target, value, field) in [
        (
            &mut property.purchase_price,
            input.purchase_price,
            "purchase_price",
        ),
        (
            &mut property.purchase_fees,
            input.purchase_fees,
            "purchase_fees",
        ),
        (
            &mut property.monthly_rent,
            input.monthly_rent,
            "monthly_rent",
        ),
        (
            &mut property.monthly_charges,
            input.monthly_charges,
            "monthly_charges",
        ),
        (
            &mut property.yearly_property_tax,
            input.yearly_property_tax,
            "yearly_property_tax",
        ),
    ] {
        if let Some(value) = value {
            *target = non_negative(value, field)?;
        }
    }
    let declared = match existing {
        None => Some(property.current_value).filter(|value| *value > 0.0),
        Some(_) => input
            .current_value
            .map(|value| non_negative(value, "current_value"))
            .transpose()?
            .filter(|value| *value != property.current_value),
    };
    if let Some(value) = declared {
        let value =
            EstimatedValue::new(value).map_err(|_| DomainError::NotPositive("current_value"))?;
        property
            .valuations
            .push(Valuation::manual_today(value, now));
        property.refresh_current_value(now.date_naive());
    }
    property.check()?;
    Ok(property)
}

/// Fusionne l'entrée avec le prêt existant, ou en crée un. Le compte financé doit être un compte immobilier connu.
pub(crate) fn merge_loan(
    existing: Option<&Loan>,
    input: &LoanInput,
    accounts: &[Account],
) -> AppResult<Loan> {
    let missing = |field: &str| AppError::invalid(format!("Le prêt doit indiquer {field}"));
    let old = existing.map(|loan| loan.terms);
    let principal = input
        .principal
        .or(old.map(|t| t.principal()))
        .ok_or_else(|| missing("principal"))?;
    let interest = input
        .interest_rate_pct
        .or(old.map(|t| t.interest_rate().value()))
        .ok_or_else(|| missing("interest_rate_pct"))?;
    let insurance = input
        .insurance_rate_pct
        .or(old.map(|t| t.insurance_rate().value()))
        .unwrap_or(0.0);
    let duration = input
        .duration_months
        .map(i64::from)
        .or(old.map(|t| i64::from(t.duration_months())))
        .ok_or_else(|| missing("duration_months"))?;
    let start_date = input
        .start_date
        .or(old.map(|t| t.start_date()))
        .ok_or_else(|| missing("start_date"))?;
    let terms = LoanTerms::new(principal, interest, insurance, duration, start_date)?;

    let property_account_id = match input.property_account_id {
        Some(link) => link.map(AccountId::from_uuid),
        None => existing.and_then(|loan| loan.property_account_id),
    };
    if let Some(property_id) = property_account_id {
        let finances_real_estate = accounts
            .iter()
            .any(|account| account.id == property_id && account.kind == AccountKind::RealEstate);
        if !finances_real_estate {
            return Err(AppError::invalid(
                "property_account_id doit désigner un compte immobilier existant",
            ));
        }
    }
    Ok(Loan {
        id: existing.map_or_else(LoanId::generate, |loan| loan.id),
        property_account_id,
        terms,
    })
}

/// Construit l'agrégat compte à partir de l'entrée : création (`existing = None`) ou mise à jour partielle.
pub(crate) fn build_account(
    existing: Option<&Account>,
    input: &AccountInput,
    origin: Origin,
    members: &[Member],
    accounts: &[Account],
    now: DateTime<Utc>,
) -> AppResult<Account> {
    let kind = match (&input.kind, existing) {
        (Some(kind), _) => AccountKind::parse(kind)?,
        (None, Some(account)) => account.kind,
        (None, None) => return Err(AppError::invalid("Le type de compte (kind) est requis")),
    };
    let owners = match (&input.owners, existing) {
        (Some(owners), _) => ownership(owners, members)?,
        (None, Some(account)) => account.owners.clone(),
        (None, None) => {
            return Err(AppError::invalid(
                "La liste des propriétaires (owners) est requise",
            ));
        }
    };
    let mut account = match existing {
        Some(account) => Account {
            kind,
            owners,
            updated_at: now,
            ..account.clone()
        },
        None => Account {
            id: AccountId::generate(),
            name: String::new(),
            institution: None,
            kind,
            envelope: None,
            currency: Currency::eur(),
            cash_balance: 0.0,
            annual_yield_pct: None,
            management_fee_pct: None,
            source: origin.source,
            external_id: origin.external_id,
            connection_id: None,
            opened_at: None,
            updated_at: now,
            owners,
            property: None,
            loan: None,
        },
    };
    if let Some(name) = &input.name {
        account.name = name.trim().to_string();
    }
    if let Some(institution) = &input.institution {
        account.institution = clean(institution.clone());
    }
    if let Some(envelope) = &input.envelope {
        account.envelope = clean(envelope.clone());
    }
    if let Some(currency) = &input.currency {
        account.currency = Currency::parse(currency)?;
    }
    if let Some(cash_balance) = input.cash_balance {
        account.cash_balance = finite(cash_balance, "cash_balance")?;
    }
    if let Some(annual_yield_pct) = input.annual_yield_pct {
        account.annual_yield_pct = annual_yield_pct;
    }
    if let Some(management_fee_pct) = input.management_fee_pct {
        account.management_fee_pct = management_fee_pct;
    }
    if let Some(opened_at) = input.opened_at {
        account.opened_at = opened_at;
    }

    // Bien et prêt : présents si et seulement si le type du compte l'exige.
    if kind == AccountKind::RealEstate {
        if let Some(property) = &input.property {
            account.property = Some(merge_property(account.property.as_ref(), property, now)?);
        }
    } else if input.property.is_some() {
        return Err(AppError::invalid(
            "property n'est accepté que pour un compte immobilier",
        ));
    } else {
        account.property = None;
    }
    if kind == AccountKind::Loan {
        if let Some(loan) = &input.loan {
            account.loan = Some(merge_loan(account.loan.as_ref(), loan, accounts)?);
        }
    } else if input.loan.is_some() {
        return Err(AppError::invalid(
            "loan n'est accepté que pour un compte de prêt",
        ));
    } else {
        account.loan = None;
    }
    account.check()?;
    Ok(account)
}

/// Prêts qui finançaient `property_account` et doivent être déliés (le compte n'est plus immobilier).
pub(crate) fn unlink_loans(
    accounts: &[Account],
    property_account: AccountId,
    now: DateTime<Utc>,
) -> Vec<Account> {
    accounts
        .iter()
        .filter(|account| {
            account
                .loan
                .as_ref()
                .is_some_and(|loan| loan.property_account_id == Some(property_account))
        })
        .map(|account| {
            let mut unlinked = account.clone();
            if let Some(loan) = &mut unlinked.loan {
                loan.property_account_id = None;
            }
            unlinked.updated_at = now;
            unlinked
        })
        .collect()
}

pub struct AccountService {
    pub(crate) reader: PortfolioReader,
    pub(crate) repo: Arc<dyn PortfolioRepository>,
    pub(crate) clock: Arc<dyn Clock>,
}

impl AccountService {
    /// Comptes de la vue ; pour un membre, seulement ceux dont il détient une part.
    pub async fn list(&self, member_id: Option<Uuid>) -> AppResult<Vec<AccountView>> {
        let portfolio = self.reader.load().await?;
        let view = view_for(&portfolio, member_id)?;
        Ok(portfolio
            .accounts_in_view(view)
            .map(|account| portfolio.account_view(account, view))
            .collect())
    }

    pub async fn get(&self, id: Uuid, member_id: Option<Uuid>) -> AppResult<AccountDetail> {
        let portfolio = self.reader.load().await?;
        let view = view_for(&portfolio, member_id)?;
        let account = portfolio
            .account(AccountId::from_uuid(id))
            .ok_or(AppError::NotFound("Compte introuvable"))?;
        Ok(portfolio.account_detail(account, view))
    }

    pub async fn create(&self, input: &AccountInput) -> AppResult<AccountDetail> {
        let portfolio = self.reader.load().await?;
        let account = build_account(
            None,
            input,
            Origin::manual(),
            &portfolio.members,
            &portfolio.accounts,
            self.clock.now(),
        )?;
        self.repo
            .save_accounts(&[AccountWrite::account_only(account.clone())])
            .await?;
        self.get(account.id.as_uuid(), None).await
    }

    pub async fn update(&self, id: Uuid, input: &AccountInput) -> AppResult<AccountDetail> {
        let portfolio = self.reader.load().await?;
        let existing = portfolio
            .account(AccountId::from_uuid(id))
            .ok_or(AppError::NotFound("Compte introuvable"))?;
        let now = self.clock.now();
        let account = build_account(
            Some(existing),
            input,
            Origin::manual(),
            &portfolio.members,
            &portfolio.accounts,
            now,
        )?;
        let mut writes = vec![AccountWrite::account_only(account.clone())];
        if account.kind != AccountKind::RealEstate {
            let unlinked = unlink_loans(&portfolio.accounts, account.id, now);
            writes.extend(
                unlinked
                    .into_iter()
                    .filter(|loan| loan.id != account.id)
                    .map(AccountWrite::account_only),
            );
        }
        self.repo.save_accounts(&writes).await?;
        self.get(id, None).await
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        if self.repo.delete_account(AccountId::from_uuid(id)).await? {
            Ok(())
        } else {
            Err(AppError::NotFound("Compte introuvable"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{TestApp, date, loan_input, owners, property_input};

    fn savings(t: &TestApp) -> AccountInput {
        AccountInput {
            name: Some(" Livret ".into()),
            institution: Some(Some("Banque Fictive".into())),
            kind: Some("savings".into()),
            envelope: Some(Some("livret".into())),
            cash_balance: Some(10_000.0),
            annual_yield_pct: Some(Some(1.7)),
            opened_at: Some(Some(date(2020, 1, 15))),
            owners: Some(owners(&[(t.alice, 1.0)])),
            ..Default::default()
        }
    }

    fn message(result: AppResult<AccountDetail>) -> String {
        result.unwrap_err().to_string()
    }

    #[tokio::test]
    async fn create_plain_account_with_defaults() {
        let t = TestApp::household().await;
        let detail = t.app.accounts.create(&savings(&t)).await.unwrap();
        let account = &detail.view.account;
        assert_eq!(account.name, "Livret");
        assert_eq!(account.currency.as_str(), "EUR");
        assert_eq!(account.source, AccountSource::Manual);
        assert_eq!(account.updated_at, t.clock.now());
        assert_eq!(detail.view.valuation.value, 10_000.0);
        assert_eq!(detail.view.owned_value, 10_000.0);
        assert_eq!(detail.view.owners[0].name, "Alice");
        assert!(detail.holdings.is_empty());
    }

    #[tokio::test]
    async fn create_requires_kind_name_and_owners() {
        let t = TestApp::household().await;
        let no_kind = AccountInput {
            kind: None,
            ..savings(&t)
        };
        assert_eq!(
            message(t.app.accounts.create(&no_kind).await),
            "Le type de compte (kind) est requis"
        );
        let bad_kind = AccountInput {
            kind: Some("bogus".into()),
            ..savings(&t)
        };
        assert!(
            message(t.app.accounts.create(&bad_kind).await).starts_with("Type de compte invalide")
        );
        let no_name = AccountInput {
            name: None,
            ..savings(&t)
        };
        assert_eq!(
            message(t.app.accounts.create(&no_name).await),
            "Le nom du compte est requis"
        );
        let no_owners = AccountInput {
            owners: None,
            ..savings(&t)
        };
        assert_eq!(
            message(t.app.accounts.create(&no_owners).await),
            "La liste des propriétaires (owners) est requise"
        );
        assert!(t.store.state().accounts.is_empty());
    }

    #[tokio::test]
    async fn owners_must_be_valid_shares_of_known_members() {
        let t = TestApp::household().await;
        let over = AccountInput {
            owners: Some(owners(&[(t.alice, 0.7), (t.bob, 0.5)])),
            ..savings(&t)
        };
        assert_eq!(
            message(t.app.accounts.create(&over).await),
            "La somme des parts ne peut pas dépasser 100 %"
        );
        let unknown = AccountInput {
            owners: Some(owners(&[(MemberId::generate(), 1.0)])),
            ..savings(&t)
        };
        assert_eq!(
            message(t.app.accounts.create(&unknown).await),
            "Un des propriétaires ne correspond à aucun membre"
        );
        let empty = AccountInput {
            owners: Some(vec![]),
            ..savings(&t)
        };
        assert_eq!(
            message(t.app.accounts.create(&empty).await),
            "Au moins un propriétaire est requis"
        );
    }

    #[tokio::test]
    async fn shares_below_whole_are_accepted_and_weight_owned_value() {
        let t = TestApp::household().await;
        let id = t.checking(&[(t.alice, 0.6), (t.bob, 0.2)], 1000.0).await;
        let household = t.app.accounts.get(id.as_uuid(), None).await.unwrap();
        assert!((household.view.owned_value - 800.0).abs() < 1e-9);
        let bob = t
            .app
            .accounts
            .get(id.as_uuid(), Some(t.bob.as_uuid()))
            .await
            .unwrap();
        assert!((bob.view.owned_value - 200.0).abs() < 1e-9);
        assert_eq!(bob.view.valuation.value, 1000.0);
    }

    #[tokio::test]
    async fn real_estate_requires_a_property() {
        let t = TestApp::household().await;
        let input = AccountInput {
            kind: Some("real_estate".into()),
            ..savings(&t)
        };
        assert_eq!(
            message(t.app.accounts.create(&input).await),
            "property est requis pour un compte immobilier"
        );
        let empty_property = AccountInput {
            property: Some(PropertyInput::default()),
            ..input
        };
        assert_eq!(
            message(t.app.accounts.create(&empty_property).await),
            "Le bien doit indiquer purchase_price ou current_value"
        );
    }

    #[tokio::test]
    async fn property_defaults_and_validation() {
        let t = TestApp::household().await;
        let only_price = PropertyInput {
            purchase_price: Some(100_000.0),
            ..Default::default()
        };
        let input = AccountInput {
            kind: Some("real_estate".into()),
            property: Some(only_price),
            ..savings(&t)
        };
        let detail = t.app.accounts.create(&input).await.unwrap();
        let property = detail.view.account.property.clone().unwrap();
        assert_eq!(property.current_value, 100_000.0);
        assert_eq!(property.category, PropertyCategory::Primary);
        assert_eq!(property.building_type, BuildingType::Apartment);

        let negative = PropertyInput {
            monthly_rent: Some(-5.0),
            ..property_input()
        };
        let input = AccountInput {
            kind: Some("real_estate".into()),
            property: Some(negative),
            ..savings(&t)
        };
        assert_eq!(
            message(t.app.accounts.create(&input).await),
            "monthly_rent ne peut pas être négatif"
        );
        let bad_category = PropertyInput {
            category: Some("castle".into()),
            ..property_input()
        };
        let input = AccountInput {
            kind: Some("real_estate".into()),
            property: Some(bad_category),
            ..savings(&t)
        };
        assert!(
            message(t.app.accounts.create(&input).await).starts_with("Catégorie de bien invalide")
        );
    }

    #[tokio::test]
    async fn property_and_loan_are_refused_on_other_kinds() {
        let t = TestApp::household().await;
        let with_property = AccountInput {
            property: Some(property_input()),
            ..savings(&t)
        };
        assert_eq!(
            message(t.app.accounts.create(&with_property).await),
            "property n'est accepté que pour un compte immobilier"
        );
        let with_loan = AccountInput {
            loan: Some(loan_input(None)),
            ..savings(&t)
        };
        assert_eq!(
            message(t.app.accounts.create(&with_loan).await),
            "loan n'est accepté que pour un compte de prêt"
        );
    }

    #[tokio::test]
    async fn loan_account_linked_to_shared_property() {
        let t = TestApp::household().await;
        let (house, loan) = t.house_with_loan().await;
        let detail = t
            .app
            .accounts
            .get(loan.as_uuid(), Some(t.alice.as_uuid()))
            .await
            .unwrap();
        assert!(detail.view.account.kind.is_liability());
        let view = detail.loan.unwrap();
        assert_eq!(view.loan.property_account_id, Some(house));
        // Le prêt fictif démarre après la date fixe des tests : tout le capital reste dû.
        assert_eq!(view.summary.remaining_capital, 200_000.0);
        assert_eq!(detail.view.owned_value, 100_000.0);
    }

    #[tokio::test]
    async fn loan_value_follows_the_clock() {
        let t = TestApp::household().await;
        let (_, loan) = t.house_with_loan().await;
        t.clock.advance_days(365 * 3);
        let during = t
            .app
            .accounts
            .get(loan.as_uuid(), None)
            .await
            .unwrap()
            .view
            .valuation
            .value;
        assert!(during > 0.0 && during < 200_000.0);
        t.clock.advance_days(365 * 25);
        assert_eq!(
            t.app
                .accounts
                .get(loan.as_uuid(), None)
                .await
                .unwrap()
                .view
                .valuation
                .value,
            0.0
        );
    }

    #[tokio::test]
    async fn loan_requires_terms_and_a_real_estate_target() {
        let t = TestApp::household().await;
        let base = AccountInput {
            kind: Some("loan".into()),
            ..savings(&t)
        };
        assert_eq!(
            message(t.app.accounts.create(&base).await),
            "loan est requis pour un compte de prêt"
        );
        let partial = LoanInput {
            principal: Some(1000.0),
            ..Default::default()
        };
        let input = AccountInput {
            loan: Some(partial),
            ..base.clone()
        };
        assert_eq!(
            message(t.app.accounts.create(&input).await),
            "Le prêt doit indiquer interest_rate_pct"
        );
        let checking = t.checking(&[(t.alice, 1.0)], 0.0).await;
        let input = AccountInput {
            loan: Some(loan_input(Some(checking))),
            ..base.clone()
        };
        assert_eq!(
            message(t.app.accounts.create(&input).await),
            "property_account_id doit désigner un compte immobilier existant"
        );
        let zero = LoanInput {
            principal: Some(0.0),
            ..loan_input(None)
        };
        let input = AccountInput {
            loan: Some(zero),
            ..base
        };
        assert_eq!(
            message(t.app.accounts.create(&input).await),
            "Le capital emprunté doit être positif"
        );
    }

    #[tokio::test]
    async fn update_is_partial_and_null_clears() {
        let t = TestApp::household().await;
        let id = t
            .app
            .accounts
            .create(&savings(&t))
            .await
            .unwrap()
            .view
            .account
            .id
            .as_uuid();
        t.clock.advance_days(1);
        let patch = AccountInput {
            cash_balance: Some(12_000.0),
            institution: Some(None),
            ..Default::default()
        };
        let account = t
            .app
            .accounts
            .update(id, &patch)
            .await
            .unwrap()
            .view
            .account;
        assert_eq!(account.cash_balance, 12_000.0);
        assert_eq!(account.institution, None);
        assert_eq!(account.envelope.as_deref(), Some("livret"));
        assert_eq!(account.name, "Livret");
        assert_eq!(account.opened_at, Some(date(2020, 1, 15)));
        assert_eq!(account.updated_at, t.clock.now());
        assert!(matches!(
            t.app.accounts.update(Uuid::new_v4(), &patch).await,
            Err(AppError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn update_owners_replaces_the_list() {
        let t = TestApp::household().await;
        let id = t.checking(&[(t.alice, 1.0)], 100.0).await;
        let patch = AccountInput {
            owners: Some(owners(&[(t.bob, 0.5)])),
            ..Default::default()
        };
        let detail = t.app.accounts.update(id.as_uuid(), &patch).await.unwrap();
        assert_eq!(detail.view.owners.len(), 1);
        assert_eq!(detail.view.owners[0].name, "Bob");
        assert_eq!(detail.view.owned_value, 50.0);
    }

    #[tokio::test]
    async fn nested_property_patch_keeps_other_fields() {
        let t = TestApp::household().await;
        let (house, _) = t.house_with_loan().await;
        let patch = AccountInput {
            property: Some(PropertyInput {
                current_value: Some(310_000.0),
                address: Some(None),
                ..Default::default()
            }),
            ..Default::default()
        };
        let detail = t
            .app
            .accounts
            .update(house.as_uuid(), &patch)
            .await
            .unwrap();
        let property = detail.view.account.property.clone().unwrap();
        assert_eq!(property.current_value, 310_000.0);
        assert_eq!(property.monthly_rent, 1200.0);
        assert_eq!(property.address, None);
        assert_eq!(detail.view.valuation.gain(), Some(40_000.0));
    }

    #[tokio::test]
    async fn nested_loan_patch_keeps_terms_and_link() {
        let t = TestApp::household().await;
        let (house, loan) = t.house_with_loan().await;
        let patch = AccountInput {
            loan: Some(LoanInput {
                interest_rate_pct: Some(3.0),
                ..Default::default()
            }),
            ..Default::default()
        };
        let view = t
            .app
            .accounts
            .update(loan.as_uuid(), &patch)
            .await
            .unwrap()
            .loan
            .unwrap();
        assert_eq!(view.loan.terms.interest_rate().value(), 3.0);
        assert_eq!(view.loan.terms.principal(), 200_000.0);
        assert_eq!(view.loan.property_account_id, Some(house));
        let unlink = AccountInput {
            loan: Some(LoanInput {
                property_account_id: Some(None),
                ..Default::default()
            }),
            ..Default::default()
        };
        let view = t
            .app
            .accounts
            .update(loan.as_uuid(), &unlink)
            .await
            .unwrap()
            .loan
            .unwrap();
        assert_eq!(view.loan.property_account_id, None);
    }

    #[tokio::test]
    async fn changing_kind_drops_property_and_unlinks_loans() {
        let t = TestApp::household().await;
        let (house, loan) = t.house_with_loan().await;
        let patch = AccountInput {
            kind: Some("other".into()),
            ..Default::default()
        };
        let detail = t
            .app
            .accounts
            .update(house.as_uuid(), &patch)
            .await
            .unwrap();
        assert!(detail.view.account.property.is_none());
        let loan = t
            .app
            .accounts
            .get(loan.as_uuid(), None)
            .await
            .unwrap()
            .loan
            .unwrap();
        assert_eq!(loan.loan.property_account_id, None);
        // Redevenir immobilier exige de fournir à nouveau un bien.
        let back = AccountInput {
            kind: Some("real_estate".into()),
            ..Default::default()
        };
        assert!(matches!(
            t.app.accounts.update(house.as_uuid(), &back).await,
            Err(AppError::Domain(_))
        ));
    }

    #[tokio::test]
    async fn list_filters_by_member_and_rejects_unknown_member() {
        let t = TestApp::household().await;
        t.checking(&[(t.alice, 1.0)], 100.0).await;
        t.checking(&[(t.alice, 0.5), (t.bob, 0.5)], 200.0).await;
        assert_eq!(t.app.accounts.list(None).await.unwrap().len(), 2);
        let bob = t.app.accounts.list(Some(t.bob.as_uuid())).await.unwrap();
        assert_eq!(bob.len(), 1);
        assert_eq!(bob[0].owned_value, 100.0);
        assert!(matches!(
            t.app.accounts.list(Some(Uuid::new_v4())).await,
            Err(AppError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn foreign_currency_account_is_converted_or_falls_back() {
        let t = TestApp::household().await;
        let input = AccountInput {
            currency: Some("usd".into()),
            cash_balance: Some(125.0),
            ..savings(&t)
        };
        let id = t
            .app
            .accounts
            .create(&input)
            .await
            .unwrap()
            .view
            .account
            .id
            .as_uuid();
        // Sans taux connu : repli sur 1.0.
        assert_eq!(
            t.app
                .accounts
                .get(id, None)
                .await
                .unwrap()
                .view
                .valuation
                .value,
            125.0
        );
        t.set_rate("USD", 1.25);
        assert!(
            (t.app
                .accounts
                .get(id, None)
                .await
                .unwrap()
                .view
                .valuation
                .value
                - 100.0)
                .abs()
                < 1e-9
        );
        let bad = AccountInput {
            currency: Some("dollars".into()),
            ..savings(&t)
        };
        assert!(message(t.app.accounts.create(&bad).await).starts_with("Devise invalide"));
    }

    #[tokio::test]
    async fn delete_account() {
        let t = TestApp::household().await;
        let id = t.checking(&[(t.alice, 1.0)], 100.0).await;
        t.app.accounts.delete(id.as_uuid()).await.unwrap();
        assert!(matches!(
            t.app.accounts.delete(id.as_uuid()).await,
            Err(AppError::NotFound(_))
        ));
        assert!(matches!(
            t.app.accounts.get(id.as_uuid(), None).await,
            Err(AppError::NotFound(_))
        ));
    }
}
