use std::sync::Arc;

use chrono::{DateTime, Utc};
use fineme_domain::DomainError;
use fineme_domain::account::Account;
use fineme_domain::ids::{AccountId, ValuationId};
use fineme_domain::loan::ScheduleRow;
use fineme_domain::portfolio::{LoanView, RealEstateReport};
use fineme_domain::property::Property;
use fineme_domain::valuation::{
    EstimatedValue, Valuation, ValuationDate, ValuationSource, newest_first,
    value_from_price_per_m2,
};
use uuid::Uuid;

use crate::commands::{ValuationInput, clean};
use crate::error::{AppError, AppResult};
use crate::ports::{AccountWrite, Clock, PortfolioRepository};
use crate::reader::{PortfolioReader, view_for};

pub struct RealEstateService {
    pub(crate) reader: PortfolioReader,
    pub(crate) repo: Arc<dyn PortfolioRepository>,
    pub(crate) clock: Arc<dyn Clock>,
}

/// Estimation et compte immobilier qui la porte.
#[derive(Debug, Clone, PartialEq)]
pub struct AccountValuation {
    pub account_id: AccountId,
    pub valuation: Valuation,
}

const PROPERTY_NOT_FOUND: AppError = AppError::NotFound("Bien immobilier introuvable");
const VALUATION_NOT_FOUND: AppError = AppError::NotFound("Estimation introuvable");

/// Valeur, source et prix au m² d'une estimation, depuis l'entrée ou l'estimation existante.
fn valuation_amount(
    existing: Option<&Valuation>,
    property: &Property,
    input: &ValuationInput,
) -> AppResult<(EstimatedValue, ValuationSource, Option<f64>)> {
    let source = input
        .source
        .as_deref()
        .map(ValuationSource::parse)
        .transpose()?;
    match (input.value, input.price_per_m2, existing) {
        (Some(_), Some(_), _) => Err(AppError::invalid(
            "Indiquer value ou price_per_m2, pas les deux",
        )),
        (None, Some(price), _) => Ok((
            value_from_price_per_m2(property.surface_m2, price)?,
            ValuationSource::PricePerM2,
            Some(price),
        )),
        (Some(value), None, _) => {
            if source == Some(ValuationSource::PricePerM2) {
                return Err(price_required());
            }
            let source = source
                .or(existing.map(|v| v.source))
                .filter(|s| *s != ValuationSource::PricePerM2)
                .unwrap_or(ValuationSource::Manual);
            Ok((EstimatedValue::new(value)?, source, None))
        }
        (None, None, Some(existing)) => match source {
            Some(ValuationSource::PricePerM2) if existing.source != ValuationSource::PricePerM2 => {
                Err(price_required())
            }
            Some(source) if source != existing.source => Ok((existing.value, source, None)),
            _ => Ok((existing.value, existing.source, existing.price_per_m2)),
        },
        (None, None, None) => Err(AppError::invalid("value ou price_per_m2 est requis")),
    }
}

fn price_required() -> AppError {
    AppError::invalid("price_per_m2 est requis pour une estimation au prix du m²")
}

/// Crée ou modifie une estimation : valeur strictement positive, date jamais future (défaut : aujourd'hui).
pub(crate) fn build_valuation(
    existing: Option<&Valuation>,
    property: &Property,
    input: &ValuationInput,
    now: DateTime<Utc>,
) -> AppResult<Valuation> {
    let (value, source, price_per_m2) = valuation_amount(existing, property, input)?;
    let date = input
        .valued_on
        .or(existing.map(|v| v.valued_on.date()))
        .unwrap_or(now.date_naive());
    let note = match &input.note {
        Some(note) => clean(note.clone()),
        None => existing.and_then(|v| v.note.clone()),
    };
    Ok(Valuation {
        id: existing.map_or_else(ValuationId::generate, |v| v.id),
        valued_on: ValuationDate::new(date, now.date_naive())?,
        value,
        source,
        price_per_m2,
        note,
        recorded_at: existing.map_or(now, |v| v.recorded_at),
    })
}

fn property_of(account: &Account) -> AppResult<&Property> {
    account.property.as_ref().ok_or(PROPERTY_NOT_FOUND)
}

impl RealEstateService {
    pub async fn report(&self, member_id: Option<Uuid>) -> AppResult<RealEstateReport> {
        let portfolio = self.reader.load().await?;
        let view = view_for(&portfolio, member_id)?;
        Ok(portfolio.real_estate(view))
    }

    /// Tableau d'amortissement du prêt porté par un compte.
    pub async fn loan_schedule(&self, account_id: Uuid) -> AppResult<(LoanView, Vec<ScheduleRow>)> {
        let portfolio = self.reader.load().await?;
        let view = portfolio
            .account(AccountId::from_uuid(account_id))
            .and_then(|account| portfolio.loan_view(account))
            .ok_or(AppError::NotFound("Prêt introuvable"))?;
        let rows = view.loan.terms.schedule();
        Ok((view, rows))
    }

    /// Estimations d'un bien, de la plus récente à la plus ancienne.
    pub async fn valuations(&self, account_id: Uuid) -> AppResult<Vec<Valuation>> {
        let portfolio = self.reader.load().await?;
        let account = portfolio
            .account(AccountId::from_uuid(account_id))
            .ok_or(PROPERTY_NOT_FOUND)?;
        let mut valuations = property_of(account)?.valuations.clone();
        newest_first(&mut valuations);
        Ok(valuations)
    }

    pub async fn add_valuation(
        &self,
        account_id: Uuid,
        input: &ValuationInput,
    ) -> AppResult<AccountValuation> {
        let portfolio = self.reader.load().await?;
        let mut account = portfolio
            .account(AccountId::from_uuid(account_id))
            .ok_or(PROPERTY_NOT_FOUND)?
            .clone();
        let now = self.clock.now();
        let valuation = build_valuation(None, property_of(&account)?, input, now)?;
        if let Some(property) = &mut account.property {
            property.valuations.push(valuation.clone());
        }
        let account_id = account.id;
        self.save(account, now).await?;
        Ok(AccountValuation {
            account_id,
            valuation,
        })
    }

    pub async fn update_valuation(
        &self,
        id: Uuid,
        input: &ValuationInput,
    ) -> AppResult<AccountValuation> {
        let id = ValuationId::from_uuid(id);
        let mut account = self.account_with(id).await?;
        let now = self.clock.now();
        let property = property_of(&account)?;
        let existing = property.valuation(id).ok_or(VALUATION_NOT_FOUND)?;
        let valuation = build_valuation(Some(existing), property, input, now)?;
        if let Some(slot) = account
            .property
            .as_mut()
            .and_then(|p| p.valuations.iter_mut().find(|v| v.id == id))
        {
            *slot = valuation.clone();
        }
        let account_id = account.id;
        self.save(account, now).await?;
        Ok(AccountValuation {
            account_id,
            valuation,
        })
    }

    /// Supprime une estimation ; la dernière d'un bien est refusée (409).
    pub async fn delete_valuation(&self, id: Uuid) -> AppResult<()> {
        let id = ValuationId::from_uuid(id);
        let mut account = self.account_with(id).await?;
        let now = self.clock.now();
        if let Some(property) = &mut account.property {
            property
                .remove_valuation(id, now.date_naive())
                .map_err(|error| match error {
                    DomainError::Rule(message) => AppError::Conflict(message),
                    other => other.into(),
                })?;
        }
        self.save(account, now).await
    }

    /// Compte immobilier portant l'estimation.
    async fn account_with(&self, id: ValuationId) -> AppResult<Account> {
        let portfolio = self.reader.load().await?;
        portfolio
            .accounts
            .into_iter()
            .find(|account| {
                account
                    .property
                    .as_ref()
                    .is_some_and(|p| p.valuation(id).is_some())
            })
            .ok_or(VALUATION_NOT_FOUND)
    }

    /// Enregistre le bien avec sa valeur courante recalculée.
    async fn save(&self, mut account: Account, now: DateTime<Utc>) -> AppResult<()> {
        if let Some(property) = &mut account.property {
            property.refresh_current_value(now.date_naive());
        }
        account.updated_at = now;
        self.repo
            .save_accounts(&[AccountWrite::account_only(account)])
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{AccountInput, PropertyInput};
    use crate::testing::{TestApp, date};

    #[tokio::test]
    async fn report_links_loans_and_weights_by_view() {
        let t = TestApp::household().await;
        t.house_with_loan().await;
        let household = t.app.real_estate.report(None).await.unwrap();
        assert_eq!(household.items.len(), 1);
        assert_eq!(household.items[0].loans.len(), 1);
        assert_eq!(household.totals.equity, 100_000.0);
        let alice = t
            .app
            .real_estate
            .report(Some(t.alice.as_uuid()))
            .await
            .unwrap();
        assert_eq!(alice.totals.value, 150_000.0);
        assert_eq!(alice.totals.remaining_debt, 100_000.0);
        assert_eq!(alice.totals.monthly_rent, 600.0);
        assert!(matches!(
            t.app.real_estate.report(Some(Uuid::new_v4())).await,
            Err(AppError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn report_is_empty_without_property() {
        let t = TestApp::household().await;
        t.checking(&[(t.alice, 1.0)], 10.0).await;
        let report = t.app.real_estate.report(None).await.unwrap();
        assert!(report.items.is_empty());
        assert_eq!(report.totals.value, 0.0);
    }

    #[tokio::test]
    async fn schedule_for_loan_account_only() {
        let t = TestApp::household().await;
        let (house, loan) = t.house_with_loan().await;
        let (view, rows) = t
            .app
            .real_estate
            .loan_schedule(loan.as_uuid())
            .await
            .unwrap();
        assert_eq!(rows.len(), 240);
        assert_eq!(rows[239].remaining, 0.0);
        assert_eq!(view.summary.end_date, rows[239].date);
        assert!(matches!(
            t.app.real_estate.loan_schedule(house.as_uuid()).await,
            Err(AppError::NotFound(_))
        ));
        assert!(matches!(
            t.app.real_estate.loan_schedule(Uuid::new_v4()).await,
            Err(AppError::NotFound(_))
        ));
    }

    fn by_value(value: f64, on: Option<chrono::NaiveDate>) -> ValuationInput {
        ValuationInput {
            value: Some(value),
            valued_on: on,
            ..Default::default()
        }
    }

    fn message<T: std::fmt::Debug>(result: AppResult<T>) -> String {
        result.unwrap_err().to_string()
    }

    fn current_value_input(value: f64) -> AccountInput {
        AccountInput {
            property: Some(PropertyInput {
                current_value: Some(value),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn creating_a_property_records_its_value_as_first_valuation() {
        let t = TestApp::household().await;
        let (house, _) = t.house_with_loan().await;
        let list = t.app.real_estate.valuations(house.as_uuid()).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].value.value(), 300_000.0);
        assert_eq!(list[0].source, ValuationSource::Manual);
        assert_eq!(list[0].valued_on.date(), t.clock.today());
    }

    #[tokio::test]
    async fn latest_valuation_drives_value_snapshot_and_report() {
        let t = TestApp::household().await;
        let (house, _) = t.house_with_loan().await;
        let id = house.as_uuid();
        // Estimation antérieure : la valeur courante reste la plus récente.
        t.app
            .real_estate
            .add_valuation(id, &by_value(280_000.0, Some(date(2025, 6, 1))))
            .await
            .unwrap();
        assert_eq!(
            t.app.real_estate.report(None).await.unwrap().totals.value,
            300_000.0
        );
        t.clock.advance_days(10);
        let added = t
            .app
            .real_estate
            .add_valuation(
                id,
                &ValuationInput {
                    price_per_m2: Some(3500.0),
                    note: Some(Some("  Agence fictive ".into())),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(added.account_id, house);
        assert_eq!(added.valuation.value.value(), 315_000.0);
        assert_eq!(added.valuation.source, ValuationSource::PricePerM2);
        assert_eq!(added.valuation.price_per_m2, Some(3500.0));
        assert_eq!(added.valuation.note.as_deref(), Some("Agence fictive"));
        let report = t.app.real_estate.report(None).await.unwrap();
        assert_eq!(report.totals.value, 315_000.0);
        let detail = t.app.accounts.get(id, None).await.unwrap();
        assert_eq!(
            detail.view.account.property.unwrap().current_value,
            315_000.0
        );
        let list = t.app.real_estate.valuations(id).await.unwrap();
        let dates: Vec<_> = list.iter().map(|v| v.valued_on.date()).collect();
        assert_eq!(
            dates,
            vec![t.clock.today(), date(2026, 1, 1), date(2025, 6, 1)]
        );

        // L'instantané quotidien suit la valeur courante.
        t.app.overview.record_snapshots().await.unwrap();
        let snapshots = t.store.state().snapshots;
        assert!(
            snapshots
                .iter()
                .any(|(member, point)| member.is_none() && point.totals.gross_assets == 315_000.0)
        );
    }

    #[tokio::test]
    async fn valuation_invariants_are_enforced() {
        let t = TestApp::household().await;
        let (house, loan) = t.house_with_loan().await;
        let id = house.as_uuid();
        let re = &t.app.real_estate;
        assert_eq!(
            message(re.add_valuation(id, &by_value(0.0, None)).await),
            "value doit être strictement positif"
        );
        assert_eq!(
            message(
                re.add_valuation(id, &by_value(1.0, Some(date(2026, 1, 2))))
                    .await
            ),
            "La date d'une estimation ne peut pas être dans le futur"
        );
        let both = ValuationInput {
            price_per_m2: Some(1.0),
            ..by_value(1.0, None)
        };
        assert!(re.add_valuation(id, &both).await.is_err());
        assert!(
            re.add_valuation(id, &ValuationInput::default())
                .await
                .is_err()
        );
        let bad_source = ValuationInput {
            source: Some("price_per_m2".into()),
            ..by_value(1.0, None)
        };
        assert!(re.add_valuation(id, &bad_source).await.is_err());
        assert!(matches!(
            re.add_valuation(loan.as_uuid(), &by_value(1.0, None)).await,
            Err(AppError::NotFound(_))
        ));
        assert!(matches!(
            re.valuations(Uuid::new_v4()).await,
            Err(AppError::NotFound(_))
        ));
        assert_eq!(re.valuations(id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn update_and_delete_keep_at_least_one_valuation() {
        let t = TestApp::household().await;
        let (house, _) = t.house_with_loan().await;
        let id = house.as_uuid();
        let re = &t.app.real_estate;
        let first = re.valuations(id).await.unwrap()[0].id.as_uuid();
        assert!(matches!(
            re.delete_valuation(first).await,
            Err(AppError::Conflict(_))
        ));
        let updated = re
            .update_valuation(
                first,
                &ValuationInput {
                    value: Some(305_000.0),
                    source: Some("external".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(updated.valuation.source, ValuationSource::External);
        assert_eq!(re.report(None).await.unwrap().totals.value, 305_000.0);

        let older = re
            .add_valuation(id, &by_value(250_000.0, Some(date(2024, 1, 1))))
            .await
            .unwrap();
        re.delete_valuation(first).await.unwrap();
        assert_eq!(re.report(None).await.unwrap().totals.value, 250_000.0);
        assert!(matches!(
            re.delete_valuation(older.valuation.id.as_uuid()).await,
            Err(AppError::Conflict(_))
        ));
        assert!(matches!(
            re.update_valuation(Uuid::new_v4(), &by_value(1.0, None))
                .await,
            Err(AppError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn editing_current_value_through_the_account_adds_a_valuation() {
        let t = TestApp::household().await;
        let (house, _) = t.house_with_loan().await;
        let id = house.as_uuid();
        t.clock.advance_days(1);
        t.app
            .accounts
            .update(id, &current_value_input(300_000.0))
            .await
            .unwrap();
        assert_eq!(t.app.real_estate.valuations(id).await.unwrap().len(), 1);
        t.app
            .accounts
            .update(id, &current_value_input(320_000.0))
            .await
            .unwrap();
        let list = t.app.real_estate.valuations(id).await.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].value.value(), 320_000.0);
        assert_eq!(list[0].valued_on.date(), t.clock.today());
        assert_eq!(
            message(t.app.accounts.update(id, &current_value_input(0.0)).await),
            "current_value doit être strictement positif"
        );
    }
}
