use std::sync::Arc;

use chrono::{DateTime, Utc};
use fineme_domain::account::{Account, AccountSource};
use fineme_domain::error::finite;
use fineme_domain::fx::Currency;
use fineme_domain::holding::{Holding, HoldingKind, PriceSource};
use fineme_domain::ids::{AccountId, HoldingId};
use fineme_domain::portfolio::{HoldingView, Portfolio};
use uuid::Uuid;

use crate::commands::{HoldingInput, clean};
use crate::error::{AppError, AppResult};
use crate::ports::{AccountWrite, Clock, PortfolioRepository};
use crate::reader::PortfolioReader;

fn upper(value: &Option<String>) -> Option<String> {
    clean(value.clone()).map(|text| text.to_ascii_uppercase())
}

/// Construit une position à partir de l'entrée : création (`existing = None`) ou mise à jour partielle.
pub(crate) fn build_holding(
    existing: Option<&Holding>,
    account: &Account,
    input: &HoldingInput,
    now: DateTime<Utc>,
) -> AppResult<Holding> {
    let is_new = existing.is_none();
    let mut holding = existing.cloned().unwrap_or_else(|| Holding {
        id: HoldingId::generate(),
        account_id: account.id,
        kind: HoldingKind::Security,
        name: String::new(),
        isin: None,
        symbol: None,
        security_type: None,
        quantity: 0.0,
        buy_price: None,
        current_price: 0.0,
        currency: account.currency.clone(),
        expense_ratio: None,
        price_source: PriceSource::Manual,
        price_updated_at: None,
    });
    if let Some(kind) = &input.kind {
        holding.kind = HoldingKind::parse(kind)?;
    }
    if let Some(name) = &input.name {
        holding.name = name.trim().to_string();
    }
    if holding.name.is_empty() {
        return Err(AppError::invalid("Le nom de la position est requis"));
    }
    if let Some(isin) = &input.isin {
        holding.isin = upper(isin);
    }
    if let Some(symbol) = &input.symbol {
        holding.symbol = upper(symbol);
    }
    if let Some(security_type) = &input.security_type {
        holding.security_type = clean(security_type.clone());
    }
    match input.quantity {
        Some(quantity) => holding.quantity = finite(quantity, "quantity")?,
        None if is_new => return Err(AppError::invalid("La quantité de la position est requise")),
        None => {}
    }
    if let Some(buy_price) = input.buy_price {
        holding.buy_price = buy_price
            .map(|price| finite(price, "buy_price"))
            .transpose()?;
    }
    match input.current_price {
        // Un cours saisi à la main repasse la position en prix manuel.
        Some(price) => {
            holding.current_price = finite(price, "current_price")?;
            holding.price_source = PriceSource::Manual;
            holding.price_updated_at = Some(now);
        }
        // À la création, le cours par défaut est le PRU.
        None if is_new => holding.current_price = holding.buy_price.unwrap_or(0.0),
        None => {}
    }
    if let Some(currency) = &input.currency {
        holding.currency = Currency::parse(currency)?;
    }
    if let Some(expense_ratio) = input.expense_ratio {
        holding.expense_ratio = expense_ratio
            .map(|ratio| finite(ratio, "expense_ratio"))
            .transpose()?;
    }
    if let Some(price_source) = &input.price_source {
        holding.price_source = PriceSource::parse(price_source)?;
    }
    Ok(holding)
}

pub struct HoldingService {
    pub(crate) reader: PortfolioReader,
    pub(crate) repo: Arc<dyn PortfolioRepository>,
    pub(crate) clock: Arc<dyn Clock>,
}

/// Compte capable de porter des positions (ni immobilier ni prêt).
fn positions_account(portfolio: &Portfolio, account_id: Uuid) -> AppResult<&Account> {
    let account = portfolio
        .account(AccountId::from_uuid(account_id))
        .ok_or(AppError::NotFound("Compte introuvable"))?;
    if !account.kind.can_hold_positions() {
        return Err(AppError::invalid(
            "Ce type de compte ne peut pas contenir de positions",
        ));
    }
    Ok(account)
}

impl HoldingService {
    pub async fn create(&self, account_id: Uuid, input: &HoldingInput) -> AppResult<HoldingView> {
        let portfolio = self.reader.load().await?;
        let account = positions_account(&portfolio, account_id)?;
        let now = self.clock.now();
        let holding = build_holding(None, account, input, now)?;
        self.repo.save_holding(&holding, now).await?;
        Ok(HoldingView {
            valuation: holding.valuation(&portfolio.rates),
            holding,
        })
    }

    /// Import en masse (CSV) ; renvoie toutes les positions du compte après l'opération.
    /// Un compte saisi à la main devient un compte « csv ».
    pub async fn bulk(
        &self,
        account_id: Uuid,
        inputs: &[HoldingInput],
        replace: bool,
    ) -> AppResult<Vec<HoldingView>> {
        let portfolio = self.reader.load().await?;
        let account = positions_account(&portfolio, account_id)?;
        let now = self.clock.now();
        let mut holdings = if replace {
            Vec::new()
        } else {
            portfolio.holdings_of(account.id).to_vec()
        };
        for input in inputs {
            holdings.push(build_holding(None, account, input, now)?);
        }
        let mut account = account.clone();
        account.updated_at = now;
        if account.source == AccountSource::Manual {
            account.source = AccountSource::Csv;
        }
        let account_id = account.id;
        self.repo
            .save_accounts(&[AccountWrite {
                account,
                holdings: Some(holdings),
            }])
            .await?;
        Ok(self.reader.load().await?.holding_views(account_id))
    }

    pub async fn update(&self, id: Uuid, input: &HoldingInput) -> AppResult<HoldingView> {
        let portfolio = self.reader.load().await?;
        let id = HoldingId::from_uuid(id);
        let existing = portfolio
            .holdings
            .values()
            .flatten()
            .find(|holding| holding.id == id)
            .ok_or(AppError::NotFound("Position introuvable"))?;
        let account = portfolio
            .account(existing.account_id)
            .ok_or(AppError::NotFound("Compte introuvable"))?;
        let now = self.clock.now();
        let holding = build_holding(Some(existing), account, input, now)?;
        self.repo.save_holding(&holding, now).await?;
        Ok(HoldingView {
            valuation: holding.valuation(&portfolio.rates),
            holding,
        })
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        if self
            .repo
            .delete_holding(HoldingId::from_uuid(id), self.clock.now())
            .await?
        {
            Ok(())
        } else {
            Err(AppError::NotFound("Position introuvable"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{TestApp, holding_input};

    #[tokio::test]
    async fn create_with_defaults_from_account_and_buy_price() {
        let t = TestApp::household().await;
        let pea = t.investment(t.alice, 500.0).await;
        let input = HoldingInput {
            name: Some(" ETF Monde ".into()),
            isin: Some(Some("fr0000000001".into())),
            symbol: Some(Some("wld".into())),
            quantity: Some(10.0),
            buy_price: Some(Some(20.0)),
            ..Default::default()
        };
        let view = t.app.holdings.create(pea.as_uuid(), &input).await.unwrap();
        let h = &view.holding;
        assert_eq!(
            (h.name.as_str(), h.isin.as_deref(), h.symbol.as_deref()),
            ("ETF Monde", Some("FR0000000001"), Some("WLD"))
        );
        assert_eq!(h.kind, HoldingKind::Security);
        assert_eq!(h.currency.as_str(), "EUR");
        assert_eq!(h.current_price, 20.0);
        assert_eq!(h.price_source, PriceSource::Manual);
        assert_eq!(h.price_updated_at, None);
        assert_eq!(view.valuation.value, 200.0);
        assert_eq!(view.valuation.gain, Some(0.0));
    }

    #[tokio::test]
    async fn create_requires_name_and_quantity() {
        let t = TestApp::household().await;
        let pea = t.investment(t.alice, 0.0).await;
        let no_name = HoldingInput {
            name: None,
            ..holding_input("X", 1.0, None, 1.0)
        };
        assert!(matches!(
            t.app.holdings.create(pea.as_uuid(), &no_name).await,
            Err(AppError::Invalid(_))
        ));
        let no_quantity = HoldingInput {
            quantity: None,
            ..holding_input("X", 1.0, None, 1.0)
        };
        assert!(matches!(
            t.app.holdings.create(pea.as_uuid(), &no_quantity).await,
            Err(AppError::Invalid(_))
        ));
        let bad_kind = HoldingInput {
            kind: Some("nft".into()),
            ..holding_input("X", 1.0, None, 1.0)
        };
        assert!(matches!(
            t.app.holdings.create(pea.as_uuid(), &bad_kind).await,
            Err(AppError::Domain(_))
        ));
        let nan = holding_input("X", f64::NAN, None, 1.0);
        assert!(matches!(
            t.app.holdings.create(pea.as_uuid(), &nan).await,
            Err(AppError::Domain(_))
        ));
    }

    #[tokio::test]
    async fn positions_are_refused_on_real_estate_and_unknown_accounts() {
        let t = TestApp::household().await;
        let (house, loan) = t.house_with_loan().await;
        let input = holding_input("X", 1.0, None, 1.0);
        for id in [house, loan] {
            assert!(matches!(
                t.app.holdings.create(id.as_uuid(), &input).await,
                Err(AppError::Invalid(_))
            ));
        }
        assert!(matches!(
            t.app.holdings.create(Uuid::new_v4(), &input).await,
            Err(AppError::NotFound(_))
        ));
        assert!(matches!(
            t.app.holdings.bulk(house.as_uuid(), &[], true).await,
            Err(AppError::Invalid(_))
        ));
    }

    #[tokio::test]
    async fn manual_price_update_stamps_time_and_source() {
        let t = TestApp::household().await;
        let pea = t.investment(t.alice, 0.0).await;
        let id = t
            .app
            .holdings
            .create(pea.as_uuid(), &holding_input("X", 2.0, Some(10.0), 10.0))
            .await
            .unwrap()
            .holding
            .id;
        t.store.set_price_auto(id, 15.0);
        t.clock.advance_days(1);
        let patch = HoldingInput {
            current_price: Some(12.0),
            ..Default::default()
        };
        let view = t.app.holdings.update(id.as_uuid(), &patch).await.unwrap();
        assert_eq!(view.holding.price_source, PriceSource::Manual);
        assert_eq!(view.holding.price_updated_at, Some(t.clock.now()));
        assert_eq!(view.valuation.value, 24.0);
    }

    #[tokio::test]
    async fn update_is_partial_and_null_clears() {
        let t = TestApp::household().await;
        let pea = t.investment(t.alice, 0.0).await;
        let created = t
            .app
            .holdings
            .create(pea.as_uuid(), &holding_input("X", 2.0, Some(10.0), 11.0))
            .await
            .unwrap();
        let patch = HoldingInput {
            quantity: Some(3.0),
            buy_price: Some(None),
            ..Default::default()
        };
        let view = t
            .app
            .holdings
            .update(created.holding.id.as_uuid(), &patch)
            .await
            .unwrap();
        assert_eq!(view.holding.quantity, 3.0);
        assert_eq!(view.holding.buy_price, None);
        assert_eq!(view.holding.current_price, 11.0);
        assert_eq!(
            view.holding.price_updated_at,
            created.holding.price_updated_at
        );
        assert_eq!(view.valuation.invested, None);
        assert!(matches!(
            t.app.holdings.update(Uuid::new_v4(), &patch).await,
            Err(AppError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn bulk_append_then_replace_and_marks_account_as_csv() {
        let t = TestApp::household().await;
        let pea = t.investment(t.alice, 0.0).await;
        t.app
            .holdings
            .create(pea.as_uuid(), &holding_input("A", 1.0, None, 1.0))
            .await
            .unwrap();
        let batch = [
            holding_input("B", 1.0, None, 1.0),
            holding_input("C", 1.0, None, 1.0),
        ];
        assert_eq!(
            t.app
                .holdings
                .bulk(pea.as_uuid(), &batch, false)
                .await
                .unwrap()
                .len(),
            3
        );
        let replaced = t
            .app
            .holdings
            .bulk(pea.as_uuid(), &batch, true)
            .await
            .unwrap();
        assert_eq!(
            replaced
                .iter()
                .map(|v| v.holding.name.as_str())
                .collect::<Vec<_>>(),
            vec!["B", "C"]
        );
        assert_eq!(
            t.app
                .accounts
                .get(pea.as_uuid(), None)
                .await
                .unwrap()
                .view
                .account
                .source,
            AccountSource::Csv
        );
    }

    #[tokio::test]
    async fn bulk_is_all_or_nothing() {
        let t = TestApp::household().await;
        let pea = t.investment(t.alice, 0.0).await;
        let batch = [holding_input("B", 1.0, None, 1.0), HoldingInput::default()];
        assert!(
            t.app
                .holdings
                .bulk(pea.as_uuid(), &batch, true)
                .await
                .is_err()
        );
        assert!(
            t.app
                .accounts
                .get(pea.as_uuid(), None)
                .await
                .unwrap()
                .holdings
                .is_empty()
        );
    }

    #[tokio::test]
    async fn delete_holding() {
        let t = TestApp::household().await;
        let pea = t.investment(t.alice, 0.0).await;
        let id = t
            .app
            .holdings
            .create(pea.as_uuid(), &holding_input("X", 1.0, None, 1.0))
            .await
            .unwrap()
            .holding
            .id;
        t.app.holdings.delete(id.as_uuid()).await.unwrap();
        assert!(matches!(
            t.app.holdings.delete(id.as_uuid()).await,
            Err(AppError::NotFound(_))
        ));
    }
}
