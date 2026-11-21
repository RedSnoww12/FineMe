use std::collections::HashMap;
use std::sync::Arc;

use fineme_domain::holding::Holding;
use fineme_domain::ids::{AccountId, MemberId};
use fineme_domain::ownership::View;
use fineme_domain::portfolio::Portfolio;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::ports::{Clock, FxRateRepository, PortfolioRepository};

/// Charge la photographie du patrimoine à la date du jour (comptes, positions, taux).
#[derive(Clone)]
pub struct PortfolioReader {
    pub(crate) repo: Arc<dyn PortfolioRepository>,
    pub(crate) fx: Arc<dyn FxRateRepository>,
    pub(crate) clock: Arc<dyn Clock>,
}

impl PortfolioReader {
    pub async fn load(&self) -> AppResult<Portfolio> {
        let mut data = self.repo.load().await?;
        let (rates, _) = self.fx.rates().await?;
        let today = self.clock.today();
        // La valeur d'un bien est celle de son estimation en vigueur ce jour.
        for property in data.accounts.iter_mut().filter_map(|a| a.property.as_mut()) {
            property.refresh_current_value(today);
        }
        let mut holdings: HashMap<AccountId, Vec<Holding>> = HashMap::new();
        for holding in data.holdings {
            holdings
                .entry(holding.account_id)
                .or_default()
                .push(holding);
        }
        let portfolio = Portfolio {
            members: data.members,
            accounts: data.accounts,
            holdings,
            rates,
            today,
        };
        warn_missing_rates(&portfolio);
        Ok(portfolio)
    }
}

/// Signale dans les logs les devises valorisées au taux de repli 1.0.
fn warn_missing_rates(portfolio: &Portfolio) {
    let account_currencies = portfolio.accounts.iter().map(|account| &account.currency);
    let holding_currencies = portfolio
        .holdings
        .values()
        .flatten()
        .map(|holding| &holding.currency);
    let mut missing: Vec<&str> = account_currencies
        .chain(holding_currencies)
        .filter(|currency| portfolio.rates.rate(currency).is_none())
        .map(|currency| currency.as_str())
        .collect();
    missing.sort_unstable();
    missing.dedup();
    for currency in missing {
        tracing::warn!("taux de change manquant pour {currency}, repli sur 1.0");
    }
}

/// Traduit le filtre `member_id` en vue ; le membre doit exister.
pub fn view_for(portfolio: &Portfolio, member_id: Option<Uuid>) -> AppResult<View> {
    match member_id.map(MemberId::from_uuid) {
        None => Ok(View::Household),
        Some(id) if portfolio.member(id).is_some() => Ok(View::Member(id)),
        Some(_) => Err(AppError::NotFound("Membre introuvable")),
    }
}
