//! Rafraîchissement « best effort » des cours et des taux de change :
//! un échec n'interrompt jamais l'opération, le prix existant reste en place.
//! Les fournisseurs sont essayés dans l'ordre : un fournisseur suivant ne reçoit que les positions
//! que les précédents n'ont pas su coter (repli).

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use fineme_domain::fx::{Currency, FxRates};
use fineme_domain::ids::HoldingId;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::overview::history_start;
use crate::ports::{
    Clock, FxProvider, FxRateRepository, PortfolioRepository, PricePoint, PriceProvider,
    QuoteRequest,
};

type Outcomes = HashMap<HoldingId, Result<f64, String>>;

/// Retient le résultat d'un fournisseur : un succès remplace un échec précédent,
/// deux échecs cumulent leurs raisons (sans doublon).
fn merge_outcome(outcomes: &mut Outcomes, holding: HoldingId, outcome: Result<f64, String>) {
    match (outcomes.get_mut(&holding), outcome) {
        (Some(Err(previous)), Err(reason)) => {
            if !previous.split(" ; ").any(|known| known == reason) {
                previous.push_str(" ; ");
                previous.push_str(&reason);
            }
        }
        (_, outcome) => {
            outcomes.insert(holding, outcome);
        }
    }
}

fn is_priced(outcomes: &Outcomes, holding: HoldingId) -> bool {
    matches!(outcomes.get(&holding), Some(Ok(_)))
}

#[derive(Debug, Clone, PartialEq)]
pub struct FailedPrice {
    pub holding_id: HoldingId,
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RefreshReport {
    pub updated: u64,
    pub failed: Vec<FailedPrice>,
}

pub struct PriceService {
    pub(crate) portfolio: Arc<dyn PortfolioRepository>,
    pub(crate) fx_rates: Arc<dyn FxRateRepository>,
    pub(crate) fx_provider: Arc<dyn FxProvider>,
    pub(crate) price_providers: Vec<Arc<dyn PriceProvider>>,
    pub(crate) clock: Arc<dyn Clock>,
}

impl PriceService {
    /// Taux de change connus (base EUR) et date de dernière mise à jour.
    pub async fn fx(&self) -> AppResult<(FxRates, Option<DateTime<Utc>>)> {
        Ok(self.fx_rates.rates().await?)
    }

    async fn refresh_fx(&self) {
        let fetched = match self.fx_provider.eur_rates().await {
            Ok(rates) => rates,
            Err(reason) => return tracing::warn!("taux de change non mis à jour : {reason}"),
        };
        let rates = FxRates::new(fetched.into_iter().filter_map(|(code, rate)| {
            Currency::parse(&code).ok().map(|currency| (currency, rate))
        }));
        if let Err(error) = self.fx_rates.save_rates(&rates, self.clock.now()).await {
            tracing::warn!("taux de change non enregistrés : {error}");
        }
    }

    /// Met à jour les taux puis le cours de chaque position prise en charge par un fournisseur.
    /// Les positions sans fournisseur (fonds euro, autres) sont ignorées sans être comptées en échec.
    pub async fn refresh(&self) -> AppResult<RefreshReport> {
        self.refresh_fx().await;
        let (rates, _) = self.fx_rates.rates().await?;
        let holdings = self.portfolio.load().await?.holdings;

        let mut outcomes = Outcomes::new();
        for provider in &self.price_providers {
            let requests: Vec<QuoteRequest> = holdings
                .iter()
                .filter(|holding| {
                    provider.handles(holding.kind) && !is_priced(&outcomes, holding.id)
                })
                .map(|holding| QuoteRequest {
                    holding_id: holding.id,
                    isin: holding.isin.clone(),
                    symbol: holding.symbol.clone(),
                    currency: holding.currency.clone(),
                })
                .collect();
            if requests.is_empty() {
                continue;
            }
            let mut quotes = provider.quotes(&requests).await;
            for request in requests {
                let outcome = match quotes.remove(&request.holding_id) {
                    None => Err("aucune réponse du fournisseur de cours".to_string()),
                    Some(Err(reason)) => Err(reason.to_string()),
                    Some(Ok(quote)) if !quote.price.is_finite() || quote.price <= 0.0 => {
                        Err("cours invalide".to_string())
                    }
                    // Le cours est ramené dans la devise de la position.
                    Some(Ok(quote)) => rates
                        .convert(quote.price, &quote.currency, &request.currency)
                        .ok_or_else(|| {
                            format!(
                                "taux de change {}/{} indisponible",
                                quote.currency, request.currency
                            )
                        }),
                };
                merge_outcome(&mut outcomes, request.holding_id, outcome);
            }
        }

        let mut report = RefreshReport::default();
        let mut prices = Vec::new();
        for holding in &holdings {
            match outcomes.remove(&holding.id) {
                Some(Ok(price)) => prices.push((holding.id, price)),
                Some(Err(reason)) => {
                    tracing::warn!("cours non mis à jour pour {} : {reason}", holding.name);
                    report.failed.push(FailedPrice {
                        holding_id: holding.id,
                        name: holding.name.clone(),
                        reason,
                    });
                }
                None => {}
            }
        }
        match self
            .portfolio
            .update_prices(&prices, self.clock.now())
            .await
        {
            Ok(()) => report.updated = prices.len() as u64,
            Err(error) => {
                tracing::error!("enregistrement des cours impossible : {error}");
                for (holding_id, _) in prices {
                    let name = holdings
                        .iter()
                        .find(|h| h.id == holding_id)
                        .map(|h| h.name.clone())
                        .unwrap_or_default();
                    report.failed.push(FailedPrice {
                        holding_id,
                        name,
                        reason: "enregistrement impossible".to_string(),
                    });
                }
            }
        }
        Ok(report)
    }

    /// Historique des cours récupérés pour une position, sur une période (`1m`, `3m`, `1y`, `all`).
    pub async fn history(
        &self,
        holding_id: Uuid,
        range: Option<&str>,
    ) -> AppResult<Vec<PricePoint>> {
        let since = history_start(range, self.clock.today())?;
        let id = HoldingId::from_uuid(holding_id);
        if !self
            .portfolio
            .load()
            .await?
            .holdings
            .iter()
            .any(|holding| holding.id == id)
        {
            return Err(AppError::NotFound("Position introuvable"));
        }
        Ok(self.portfolio.price_history(id, since).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::HoldingInput;
    use crate::testing::{TestApp, holding_input};
    use fineme_domain::holding::PriceSource;
    use fineme_domain::ids::AccountId;

    async fn add(
        t: &TestApp,
        account: AccountId,
        kind: &str,
        name: &str,
        symbol: Option<&str>,
        currency: &str,
    ) -> HoldingId {
        let input = HoldingInput {
            kind: Some(kind.into()),
            symbol: Some(symbol.map(str::to_string)),
            currency: Some(currency.into()),
            ..holding_input(name, 2.0, Some(10.0), 10.0)
        };
        t.app
            .holdings
            .create(account.as_uuid(), &input)
            .await
            .unwrap()
            .holding
            .id
    }

    async fn price_of(t: &TestApp, account: AccountId, id: HoldingId) -> (f64, PriceSource) {
        let detail = t.app.accounts.get(account.as_uuid(), None).await.unwrap();
        let holding = detail
            .holdings
            .into_iter()
            .find(|v| v.holding.id == id)
            .unwrap()
            .holding;
        (holding.current_price, holding.price_source)
    }

    #[tokio::test]
    async fn refresh_updates_fx_rates_and_prices_per_provider() {
        let t = TestApp::household().await;
        let account = t.investment(t.alice, 0.0).await;
        let stock = add(&t, account, "security", "Action Démo", Some("DEMO"), "USD").await;
        let coin = add(&t, account, "crypto", "Jeton Démo", Some("DMO"), "EUR").await;
        t.fx.set(&[("USD", 1.25), ("???", 2.0), ("GBP", -1.0)]);
        t.securities.set("DEMO", 50.0, "USD");
        t.crypto.set("DMO", 3.5, "EUR");
        t.clock.advance_days(1);

        let report = t.app.prices.refresh().await.unwrap();
        assert_eq!(
            report,
            RefreshReport {
                updated: 2,
                failed: vec![]
            }
        );
        assert_eq!(
            price_of(&t, account, stock).await,
            (50.0, PriceSource::Auto)
        );
        assert_eq!(price_of(&t, account, coin).await, (3.5, PriceSource::Auto));
        let (rates, updated_at) = t.app.prices.fx().await.unwrap();
        // Seuls les taux valides sont conservés.
        assert_eq!(rates.len(), 1);
        assert_eq!(updated_at, Some(t.clock.now()));
    }

    #[tokio::test]
    async fn quote_in_another_currency_is_converted_to_the_holding_currency() {
        let t = TestApp::household().await;
        let account = t.investment(t.alice, 0.0).await;
        let stock = add(&t, account, "security", "Action Démo", Some("DEMO"), "EUR").await;
        t.fx.set(&[("USD", 1.25)]);
        t.securities.set("DEMO", 125.0, "USD");
        assert_eq!(t.app.prices.refresh().await.unwrap().updated, 1);
        assert!((price_of(&t, account, stock).await.0 - 100.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn failures_are_reported_and_keep_the_manual_price() {
        let t = TestApp::household().await;
        let account = t.investment(t.alice, 0.0).await;
        let unknown = add(&t, account, "security", "Inconnue", Some("NOPE"), "EUR").await;
        let no_rate = add(&t, account, "security", "Sans taux", Some("JPN"), "EUR").await;
        let bad = add(&t, account, "crypto", "Cours nul", Some("ZERO"), "EUR").await;
        t.securities.set("JPN", 1000.0, "JPY");
        t.crypto.set("ZERO", 0.0, "EUR");
        let report = t.app.prices.refresh().await.unwrap();
        assert_eq!(report.updated, 0);
        let reasons: Vec<(HoldingId, &str)> = report
            .failed
            .iter()
            .map(|f| (f.holding_id, f.reason.as_str()))
            .collect();
        assert_eq!(
            reasons,
            vec![
                (unknown, "cours introuvable"),
                // Le repli n'a pas trouvé ce titre non plus : les deux raisons sont gardées.
                (
                    no_rate,
                    "taux de change JPY/EUR indisponible ; cours introuvable"
                ),
                (bad, "cours invalide")
            ]
        );
        assert_eq!(report.failed[0].name, "Inconnue");
        assert_eq!(
            price_of(&t, account, unknown).await,
            (10.0, PriceSource::Manual)
        );
    }

    #[tokio::test]
    async fn fallback_provider_prices_only_what_the_primary_missed() {
        let t = TestApp::household().await;
        let account = t.investment(t.alice, 0.0).await;
        let listed = add(
            &t,
            account,
            "security",
            "Cotée partout",
            Some("BOTH"),
            "EUR",
        )
        .await;
        let rescued = add(&t, account, "security", "Repli", Some("BACKUP"), "EUR").await;
        t.securities.set("BOTH", 50.0, "EUR");
        t.fallback.set("BOTH", 99.0, "EUR");
        t.fallback.set("BACKUP", 12.5, "EUR");

        let report = t.app.prices.refresh().await.unwrap();
        assert_eq!((report.updated, report.failed), (2, vec![]));
        // Le fournisseur principal garde la main quand il répond.
        assert_eq!(
            price_of(&t, account, listed).await,
            (50.0, PriceSource::Auto)
        );
        assert_eq!(
            price_of(&t, account, rescued).await,
            (12.5, PriceSource::Auto)
        );
        assert_eq!(t.fallback.requested(), vec!["BACKUP".to_string()]);
    }

    #[tokio::test]
    async fn refresh_records_one_history_point_per_day() {
        let t = TestApp::household().await;
        let account = t.investment(t.alice, 0.0).await;
        let stock = add(&t, account, "security", "Action Démo", Some("DEMO"), "USD").await;
        t.set_rate("USD", 1.25);
        t.securities.set("DEMO", 40.0, "USD");
        t.app.prices.refresh().await.unwrap();
        t.securities.set("DEMO", 41.0, "USD");
        t.app.prices.refresh().await.unwrap();
        t.clock.advance_days(1);
        t.securities.set("DEMO", 42.0, "USD");
        t.app.prices.refresh().await.unwrap();

        let points = t.app.prices.history(stock.as_uuid(), None).await.unwrap();
        let prices: Vec<(f64, &str)> = points
            .iter()
            .map(|p| (p.price, p.currency.as_str()))
            .collect();
        // Le dernier cours du jour remplace le précédent.
        assert_eq!(prices, vec![(41.0, "USD"), (42.0, "USD")]);
        assert!(points[0].date < points[1].date);
        assert_eq!(
            t.app
                .prices
                .history(stock.as_uuid(), Some("1m"))
                .await
                .unwrap()
                .len(),
            2
        );
    }

    #[tokio::test]
    async fn history_of_an_unknown_holding_or_with_a_bad_range_is_refused() {
        let t = TestApp::household().await;
        let account = t.investment(t.alice, 0.0).await;
        let manual = add(&t, account, "security", "Saisie", None, "EUR").await;
        assert!(matches!(
            t.app.prices.history(Uuid::new_v4(), None).await,
            Err(AppError::NotFound(_))
        ));
        assert!(matches!(
            t.app.prices.history(manual.as_uuid(), Some("5y")).await,
            Err(AppError::Invalid(_))
        ));
        // Un cours saisi à la main n'entre pas dans l'historique.
        assert!(
            t.app
                .prices
                .history(manual.as_uuid(), None)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn unsupported_kinds_are_skipped_silently() {
        let t = TestApp::household().await;
        let account = t.investment(t.alice, 0.0).await;
        let fund = add(&t, account, "fonds_euro", "Fonds euro", None, "EUR").await;
        add(&t, account, "other", "Divers", None, "EUR").await;
        assert_eq!(
            t.app.prices.refresh().await.unwrap(),
            RefreshReport::default()
        );
        assert_eq!(
            price_of(&t, account, fund).await,
            (10.0, PriceSource::Manual)
        );
    }

    #[tokio::test]
    async fn fx_provider_failure_does_not_fail_the_refresh() {
        let t = TestApp::household().await;
        let account = t.investment(t.alice, 0.0).await;
        add(&t, account, "security", "Action Démo", Some("DEMO"), "EUR").await;
        t.set_rate("USD", 1.1);
        t.fx.fail("service injoignable");
        t.securities.set("DEMO", 42.0, "EUR");
        assert_eq!(t.app.prices.refresh().await.unwrap().updated, 1);
        // Les anciens taux sont conservés.
        assert_eq!(t.app.prices.fx().await.unwrap().0.len(), 1);
    }

    #[tokio::test]
    async fn storage_failure_turns_updates_into_failures() {
        let t = TestApp::household().await;
        let account = t.investment(t.alice, 0.0).await;
        add(&t, account, "security", "Action Démo", Some("DEMO"), "EUR").await;
        t.securities.set("DEMO", 42.0, "EUR");
        t.store.fail_price_updates();
        let report = t.app.prices.refresh().await.unwrap();
        assert_eq!(report.updated, 0);
        assert_eq!(report.failed[0].reason, "enregistrement impossible");
    }
}
