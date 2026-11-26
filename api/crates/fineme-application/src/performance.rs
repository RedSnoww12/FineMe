//! Performance des comptes d'investissement (titres et crypto) et du portefeuille d'une vue,
//! mesurée sur l'historique des instantanés et les flux du journal d'opérations, et comparée à un
//! indice de référence choisi par l'utilisateur.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::NaiveDate;
use fineme_domain::account::AccountKind;
use fineme_domain::fx::{Currency, FxRates};
use fineme_domain::history::DateRange;
use fineme_domain::holding::HoldingKind;
use fineme_domain::ids::{AccountId, HoldingId};
use fineme_domain::ownership::View;
use fineme_domain::performance::{
    Benchmark, BenchmarkComparison, CashFlow, PerformanceReport, Valuation, compare,
    external_flows, measure,
};
use fineme_domain::portfolio::Portfolio;
use fineme_domain::timeline::Period;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::ports::{
    BenchmarkRepository, Clock, FxRateRepository, OperationRepository, PriceProvider, QuoteRequest,
    SnapshotPoint, SnapshotRepository,
};
use crate::reader::{PortfolioReader, view_for};

const NO_BENCHMARK_PRICE: &str = "Aucun cours n'est encore enregistré pour l'indice : son historique se constitue jour après jour (tâche quotidienne), complété par celui d'une position de même ISIN ou ticker";

/// Seuls les comptes de titres et de crypto entrent dans la mesure : les soldes des autres comptes
/// varient avec des flux (salaires, intérêts) que le journal ne décrit pas.
pub fn is_measured(kind: AccountKind) -> bool {
    matches!(kind, AccountKind::Investment | AccountKind::Crypto)
}

pub struct PerformanceService {
    pub(crate) reader: PortfolioReader,
    pub(crate) snapshots: Arc<dyn SnapshotRepository>,
    pub(crate) operations: Arc<dyn OperationRepository>,
    pub(crate) benchmarks: Arc<dyn BenchmarkRepository>,
    pub(crate) fx_rates: Arc<dyn FxRateRepository>,
    pub(crate) price_providers: Vec<Arc<dyn PriceProvider>>,
    pub(crate) clock: Arc<dyn Clock>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AccountPerformance {
    pub account_id: AccountId,
    pub name: String,
    pub kind: AccountKind,
    pub report: Option<PerformanceReport>,
}

/// Indice choisi et étendue de son historique de cours.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkInfo {
    pub benchmark: Benchmark,
    pub prices: usize,
    pub first_date: Option<NaiveDate>,
    pub last_date: Option<NaiveDate>,
    /// Raison de l'échec du dernier relevé de cours, le cas échéant.
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkResult {
    pub info: BenchmarkInfo,
    pub comparison: Option<BenchmarkComparison>,
    /// Explication quand la comparaison est absente ou ne couvre pas toute la période.
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PerformanceView {
    pub portfolio: Option<PerformanceReport>,
    pub accounts: Vec<AccountPerformance>,
    pub benchmark: Option<BenchmarkResult>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkInput {
    pub name: String,
    pub symbol: Option<String>,
    pub isin: Option<String>,
    pub kind: String,
    pub currency: String,
}

/// Points de la période : le dernier instantané détaillé au plus tard à `from` (point de départ),
/// puis ceux jusqu'à `to` inclus. Les points sans détail par compte sont écartés.
fn window(points: Vec<SnapshotPoint>, range: DateRange) -> Vec<SnapshotPoint> {
    let detailed: Vec<SnapshotPoint> = points
        .into_iter()
        .filter(|point| point.accounts.is_some() && point.date <= range.to())
        .collect();
    let start = detailed
        .iter()
        .rposition(|point| point.date <= range.from())
        .unwrap_or(0);
    detailed.into_iter().skip(start).collect()
}

/// Valeur de chaque compte mesuré, à chaque point (`None` : compte absent ce jour-là).
fn account_values(points: &[SnapshotPoint]) -> Vec<HashMap<AccountId, f64>> {
    points
        .iter()
        .map(|point| {
            point
                .accounts
                .iter()
                .flatten()
                .filter(|account| is_measured(account.kind))
                .map(|account| (account.account_id, account.value))
                .collect()
        })
        .collect()
}

/// Valorisation du portefeuille et flux corrigés de l'entrée et de la sortie des comptes :
/// un compte qui apparaît apporte sa première valeur (ses opérations jusque-là y sont incluses),
/// un compte qui disparaît retire sa dernière valeur.
fn portfolio_series(
    points: &[SnapshotPoint],
    values: &[HashMap<AccountId, f64>],
    flows: &HashMap<AccountId, Vec<CashFlow>>,
) -> (Vec<Valuation>, Vec<CashFlow>) {
    let valuations = points
        .iter()
        .zip(values)
        .map(|(point, accounts)| Valuation {
            date: point.date,
            value: accounts.values().sum(),
        })
        .collect();
    let mut all_flows = Vec::new();
    for (account, account_flows) in flows {
        let mut excluded: Vec<(NaiveDate, NaiveDate)> = Vec::new();
        for i in 1..points.len() {
            let (before, now) = (values[i - 1].get(account), values[i].get(account));
            let date = points[i].date;
            match (before, now) {
                (None, Some(value)) => {
                    all_flows.push(CashFlow {
                        date,
                        amount: *value,
                    });
                    excluded.push((points[i - 1].date, date));
                }
                (Some(value), None) => all_flows.push(CashFlow {
                    date,
                    amount: -value,
                }),
                _ => {}
            }
        }
        all_flows.extend(account_flows.iter().filter(|flow| {
            !excluded
                .iter()
                .any(|(after, until)| flow.date > *after && flow.date <= *until)
        }));
    }
    (valuations, all_flows)
}

impl PerformanceService {
    /// Performance de la vue depuis le début de la période relative (jusqu'à aujourd'hui).
    pub async fn over_period(
        &self,
        member_id: Option<Uuid>,
        period: Period,
    ) -> AppResult<PerformanceView> {
        let from = period.start(self.clock.today());
        self.performance(member_id, from, None).await
    }

    /// Performance de la vue sur la période (bornes facultatives : tout l'historique, jusqu'à aujourd'hui).
    pub async fn performance(
        &self,
        member_id: Option<Uuid>,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
    ) -> AppResult<PerformanceView> {
        let portfolio = self.reader.load().await?;
        let view = view_for(&portfolio, member_id)?;
        let today = self.clock.today();
        let floor = NaiveDate::from_ymd_opt(1900, 1, 1).unwrap_or(today);
        let range = DateRange::new(from.unwrap_or(floor), to.unwrap_or(today))?;

        let member = match view {
            View::Household => None,
            View::Member(id) => Some(id),
        };
        let points = window(self.snapshots.since(member, floor).await?, range);
        let values = account_values(&points);

        let mut ids: Vec<AccountId> = values.iter().flat_map(|m| m.keys().copied()).collect();
        ids.sort_unstable();
        ids.dedup();

        let mut flows: HashMap<AccountId, Vec<CashFlow>> = HashMap::new();
        let mut accounts = Vec::new();
        for id in ids {
            let account_flows = self.account_flows(&portfolio, view, id).await?;
            let series: Vec<Valuation> = points
                .iter()
                .zip(&values)
                .filter_map(|(point, accounts)| {
                    accounts.get(&id).map(|value| Valuation {
                        date: point.date,
                        value: *value,
                    })
                })
                .collect();
            let (name, kind) = describe(&points, id);
            accounts.push(AccountPerformance {
                account_id: id,
                name,
                kind,
                report: measure(&series, &account_flows),
            });
            flows.insert(id, account_flows);
        }
        accounts.sort_by(|a, b| a.name.cmp(&b.name));

        let (valuations, portfolio_flows) = portfolio_series(&points, &values, &flows);
        let report = if accounts.is_empty() {
            None
        } else {
            measure(&valuations, &portfolio_flows)
        };
        let benchmark = match &report {
            Some(report) => self.benchmark_result(report).await?,
            None => self.benchmark().await?.map(|info| BenchmarkResult {
                info,
                comparison: None,
                message: None,
            }),
        };
        Ok(PerformanceView {
            portfolio: report,
            accounts,
            benchmark,
        })
    }

    /// Flux externes du compte, en EUR, à la part détenue par la vue (compte supprimé : aucun flux).
    async fn account_flows(
        &self,
        portfolio: &Portfolio,
        view: View,
        id: AccountId,
    ) -> AppResult<Vec<CashFlow>> {
        let Some(account) = portfolio.accounts.iter().find(|a| a.id == id) else {
            return Ok(Vec::new());
        };
        let fraction = account.owners.fraction(view);
        let operations = self.operations.operations(id).await?;
        Ok(external_flows(&operations, &portfolio.rates)
            .into_iter()
            .map(|flow| CashFlow {
                amount: flow.amount * fraction,
                ..flow
            })
            .collect())
    }

    async fn benchmark_result(
        &self,
        report: &PerformanceReport,
    ) -> AppResult<Option<BenchmarkResult>> {
        let Some(info) = self.benchmark().await? else {
            return Ok(None);
        };
        let prices: Vec<Valuation> = self
            .benchmarks
            .benchmark_prices(report.from - chrono::Duration::days(31))
            .await?
            .into_iter()
            .map(|point| Valuation {
                date: point.date,
                value: point.price,
            })
            .collect();
        let comparison = compare(&report.index, &prices);
        let message = match (&comparison, info.first_date, info.last_date) {
            (_, None, _) | (_, _, None) => Some(NO_BENCHMARK_PRICE.to_string()),
            (None, Some(first), Some(last)) => Some(format!(
                "Historique de l'indice insuffisant pour la période : cours connus du {} au {}. Aucune comparaison n'est calculée plutôt que d'estimer les cours manquants.",
                french_date(first),
                french_date(last)
            )),
            (Some(comparison), _, _) if comparison.from > report.from => Some(format!(
                "L'indice n'a d'historique qu'à partir du {} : la comparaison commence à cette date.",
                french_date(comparison.from)
            )),
            (Some(_), _, _) => None,
        };
        Ok(Some(BenchmarkResult {
            info,
            comparison,
            message,
        }))
    }

    /// Indice choisi, ou `None`.
    pub async fn benchmark(&self) -> AppResult<Option<BenchmarkInfo>> {
        let Some(benchmark) = self.benchmarks.benchmark().await? else {
            return Ok(None);
        };
        Ok(Some(self.info(benchmark, None).await?))
    }

    async fn info(
        &self,
        benchmark: Benchmark,
        last_error: Option<String>,
    ) -> AppResult<BenchmarkInfo> {
        let floor = NaiveDate::from_ymd_opt(1900, 1, 1).unwrap_or(self.clock.today());
        let prices = self.benchmarks.benchmark_prices(floor).await?;
        Ok(BenchmarkInfo {
            prices: prices.len(),
            first_date: prices.first().map(|p| p.date),
            last_date: prices.last().map(|p| p.date),
            benchmark,
            last_error,
        })
    }

    /// Choisit (ou remplace) l'indice et relève son cours du jour. Un changement d'identifiant
    /// (ticker, ISIN, type, devise) efface les cours relevés pour l'ancien indice.
    pub async fn set_benchmark(&self, input: &BenchmarkInput) -> AppResult<BenchmarkInfo> {
        let benchmark = Benchmark::new(
            &input.name,
            input.symbol.clone(),
            input.isin.clone(),
            HoldingKind::parse(&input.kind)?,
            Currency::parse(&input.currency)?,
        )?;
        let reset_prices = match self.benchmarks.benchmark().await? {
            Some(previous) => !same_instrument(&previous, &benchmark),
            None => true,
        };
        self.benchmarks
            .save_benchmark(&benchmark, reset_prices)
            .await?;
        let last_error = self.record_price(&benchmark).await.err();
        self.info(benchmark, last_error).await
    }

    pub async fn delete_benchmark(&self) -> AppResult<()> {
        if !self.benchmarks.delete_benchmark().await? {
            return Err(AppError::NotFound("Aucun indice de référence choisi"));
        }
        Ok(())
    }

    /// Relève le cours du jour de l'indice (tâche quotidienne) ; un échec est journalisé, jamais bloquant.
    pub async fn record_benchmark_price(&self) {
        match self.benchmarks.benchmark().await {
            Ok(Some(benchmark)) => {
                if let Err(reason) = self.record_price(&benchmark).await {
                    tracing::warn!("cours de l'indice non relevé : {reason}");
                }
            }
            Ok(None) => {}
            Err(error) => tracing::warn!("indice de référence illisible : {error}"),
        }
    }

    /// Demande le cours aux fournisseurs du type de l'indice, dans l'ordre, et l'enregistre
    /// dans la devise de l'indice.
    async fn record_price(&self, benchmark: &Benchmark) -> Result<(), String> {
        let rates: FxRates = self
            .fx_rates
            .rates()
            .await
            .map(|(rates, _)| rates)
            .unwrap_or_default();
        let key = HoldingId::generate();
        let request = QuoteRequest {
            holding_id: key,
            isin: benchmark.isin().map(str::to_string),
            symbol: benchmark.symbol().map(str::to_string),
            currency: benchmark.currency().clone(),
        };
        let mut reasons = Vec::new();
        for provider in self
            .price_providers
            .iter()
            .filter(|p| p.handles(benchmark.kind()))
        {
            match provider
                .quotes(std::slice::from_ref(&request))
                .await
                .remove(&key)
            {
                Some(Ok(quote)) if quote.price.is_finite() && quote.price > 0.0 => {
                    let price = rates
                        .convert(quote.price, &quote.currency, benchmark.currency())
                        .ok_or_else(|| {
                            format!(
                                "taux de change {}/{} indisponible",
                                quote.currency,
                                benchmark.currency()
                            )
                        })?;
                    return self
                        .benchmarks
                        .record_benchmark_price(self.clock.today(), price)
                        .await
                        .map_err(|error| error.to_string());
                }
                Some(Ok(_)) => reasons.push("cours invalide".to_string()),
                Some(Err(reason)) => reasons.push(reason.to_string()),
                None => reasons.push("aucune réponse du fournisseur de cours".to_string()),
            }
        }
        if reasons.is_empty() {
            reasons.push("aucun fournisseur de cours pour ce type d'indice".to_string());
        }
        reasons.dedup();
        Err(reasons.join(" ; "))
    }
}

fn same_instrument(a: &Benchmark, b: &Benchmark) -> bool {
    a.symbol() == b.symbol()
        && a.isin() == b.isin()
        && a.kind() == b.kind()
        && a.currency() == b.currency()
}

/// Nom et type du compte au dernier instantané où il figure.
fn describe(points: &[SnapshotPoint], id: AccountId) -> (String, AccountKind) {
    points
        .iter()
        .rev()
        .flat_map(|point| point.accounts.iter().flatten())
        .find(|account| account.account_id == id)
        .map(|account| (account.name.clone(), account.kind))
        .unwrap_or((String::new(), AccountKind::Investment))
}

fn french_date(date: NaiveDate) -> String {
    date.format("%d/%m/%Y").to_string()
}

#[cfg(test)]
mod tests {
    use fineme_domain::portfolio::{AccountSnapshot, Snapshot, Totals};

    use super::*;
    use crate::commands::OperationInput;
    use crate::ports::Clock;
    use crate::testing::TestApp;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    fn snapshot(accounts: &[(AccountId, AccountKind, f64)]) -> Snapshot {
        let gross: f64 = accounts.iter().map(|(_, _, v)| v).sum();
        Snapshot {
            totals: Totals {
                gross_assets: gross,
                liabilities: 0.0,
                net_worth: gross,
            },
            accounts: accounts
                .iter()
                .map(|(id, kind, value)| AccountSnapshot {
                    account_id: *id,
                    name: "Compte".into(),
                    kind: *kind,
                    value: *value,
                    gain: None,
                })
                .collect(),
        }
    }

    fn deposit(date: NaiveDate, amount: f64) -> OperationInput {
        OperationInput {
            kind: "deposit".into(),
            date,
            holding_id: None,
            quantity: None,
            unit_price: None,
            amount: Some(amount),
            fees: None,
            note: None,
        }
    }

    fn benchmark_input() -> BenchmarkInput {
        BenchmarkInput {
            name: "ETF Monde (démo)".into(),
            symbol: Some("MONDE".into()),
            isin: None,
            kind: "security".into(),
            currency: "EUR".into(),
        }
    }

    #[tokio::test]
    async fn measures_accounts_and_portfolio_with_journal_deposits() {
        let t = TestApp::household().await;
        let pea = t.investment(t.alice, 0.0).await;
        let checking = t.checking(&[(t.alice, 1.0)], 0.0).await;
        let today = t.clock.today();
        let days: Vec<NaiveDate> = (0..3)
            .rev()
            .map(|n| today - chrono::Duration::days(n * 10))
            .collect();
        // PEA : 1 000 → 2 100 (apport de 1 000 le 2e jour) → 2 310 ; le compte courant est ignoré.
        t.app
            .operations
            .create(pea.as_uuid(), &deposit(days[1], 1_000.0))
            .await
            .unwrap();
        for (day, value) in days.iter().zip([1_000.0, 2_100.0, 2_310.0]) {
            let snap = snapshot(&[
                (pea, AccountKind::Investment, value),
                (checking, AccountKind::Checking, 5_000.0 + value),
            ]);
            t.store.upsert(*day, None, &snap).await.unwrap();
        }
        let view = t
            .app
            .performance
            .performance(None, None, None)
            .await
            .unwrap();
        assert_eq!(view.accounts.len(), 1);
        let account = view.accounts[0].report.as_ref().unwrap();
        assert!(close(account.twr.unwrap(), 1.1 * 1.1 - 1.0));
        let portfolio = view.portfolio.unwrap();
        assert!(close(portfolio.twr.unwrap(), 1.1 * 1.1 - 1.0));
        assert!(close(portfolio.net_flows, 1_000.0));
        assert!(view.benchmark.is_none());

        // Période restreinte : départ au dernier point au plus tard à `from`.
        let recent = t
            .app
            .performance
            .performance(None, Some(days[1] + chrono::Duration::days(1)), None)
            .await
            .unwrap();
        let recent = recent.portfolio.unwrap();
        assert_eq!(recent.from, days[1]);
        assert!(close(recent.twr.unwrap(), 0.10));
    }

    #[tokio::test]
    async fn an_account_entering_the_portfolio_is_a_contribution_not_a_gain() {
        let t = TestApp::household().await;
        let (a, b) = (AccountId::generate(), AccountId::generate());
        let today = t.clock.today();
        let day = |n: i64| today - chrono::Duration::days(n);
        let points = [
            (day(2), vec![(a, AccountKind::Investment, 1_000.0)]),
            (
                day(1),
                vec![
                    (a, AccountKind::Investment, 1_100.0),
                    (b, AccountKind::Crypto, 5_000.0),
                ],
            ),
            (day(0), vec![(b, AccountKind::Crypto, 5_500.0)]),
        ];
        for (date, accounts) in points {
            t.store
                .upsert(date, None, &snapshot(&accounts))
                .await
                .unwrap();
        }
        let view = t
            .app
            .performance
            .performance(None, None, None)
            .await
            .unwrap();
        // +10 % puis (5 500 + 1 100 sorti − 6 100) / 6 100.
        let expected = 1.1 * (1.0 + 500.0 / 6_100.0) - 1.0;
        assert!(close(view.portfolio.unwrap().twr.unwrap(), expected));
        // Comptes supprimés depuis : mesurés sur leurs seuls instantanés.
        assert_eq!(view.accounts.len(), 2);
    }

    #[tokio::test]
    async fn empty_history_gives_no_measure() {
        let t = TestApp::household().await;
        let view = t
            .app
            .performance
            .performance(None, None, None)
            .await
            .unwrap();
        assert!(view.portfolio.is_none() && view.accounts.is_empty());
        let unknown = t
            .app
            .performance
            .performance(Some(Uuid::new_v4()), None, None)
            .await;
        assert!(matches!(unknown, Err(AppError::NotFound(_))));
        let reversed = t
            .app
            .performance
            .performance(
                None,
                Some(t.clock.today()),
                Some(t.clock.today() - chrono::Duration::days(1)),
            )
            .await;
        assert!(matches!(reversed, Err(AppError::Domain(_))));
    }

    #[tokio::test]
    async fn relative_period_starts_at_the_matching_date() {
        let t = TestApp::household().await;
        let all = t
            .app
            .performance
            .over_period(None, Period::All)
            .await
            .unwrap();
        assert_eq!(
            all,
            t.app
                .performance
                .performance(None, None, None)
                .await
                .unwrap()
        );
        let today = t.clock.today();
        let month = t
            .app
            .performance
            .over_period(None, Period::OneMonth)
            .await
            .unwrap();
        let expected = t
            .app
            .performance
            .performance(None, Period::OneMonth.start(today), None)
            .await
            .unwrap();
        assert_eq!(month, expected);
    }

    #[tokio::test]
    async fn benchmark_records_its_quote_and_says_when_history_is_missing() {
        let t = TestApp::household().await;
        // Aucun cours : l'indice est enregistré, l'échec est signalé.
        let info = t
            .app
            .performance
            .set_benchmark(&benchmark_input())
            .await
            .unwrap();
        assert_eq!(info.prices, 0);
        assert!(info.last_error.is_some());

        t.securities.set("MONDE", 80.0, "EUR");
        t.app.performance.record_benchmark_price().await;
        let info = t.app.performance.benchmark().await.unwrap().unwrap();
        assert_eq!((info.prices, info.last_date), (1, Some(t.clock.today())));

        let pea = AccountId::generate();
        let today = t.clock.today();
        for (n, value) in [(20, 1_000.0), (0, 1_100.0)] {
            let snap = snapshot(&[(pea, AccountKind::Investment, value)]);
            t.store
                .upsert(today - chrono::Duration::days(n), None, &snap)
                .await
                .unwrap();
        }
        // Un seul cours, du jour : pas de comparaison, et on le dit.
        let view = t
            .app
            .performance
            .performance(None, None, None)
            .await
            .unwrap();
        let benchmark = view.benchmark.unwrap();
        assert!(benchmark.comparison.is_none());
        assert!(benchmark.message.unwrap().contains("insuffisant"));

        // Un cours plus ancien (historique d'une position de même ticker, par exemple).
        t.store
            .record_benchmark_price(today - chrono::Duration::days(20), 64.0)
            .await
            .unwrap();
        let view = t
            .app
            .performance
            .performance(None, None, None)
            .await
            .unwrap();
        let comparison = view.benchmark.unwrap().comparison.unwrap();
        assert!(close(comparison.benchmark_return, 0.25));
        assert!(close(comparison.portfolio_return, 0.10));

        // Changer d'indice efface l'historique de l'ancien.
        let other = BenchmarkInput {
            symbol: Some("AUTRE".into()),
            ..benchmark_input()
        };
        let info = t.app.performance.set_benchmark(&other).await.unwrap();
        assert_eq!(info.prices, 0);
        t.app.performance.delete_benchmark().await.unwrap();
        assert!(t.app.performance.benchmark().await.unwrap().is_none());
        assert!(matches!(
            t.app.performance.delete_benchmark().await,
            Err(AppError::NotFound(_))
        ));
    }
}
