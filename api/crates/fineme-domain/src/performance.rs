//! Mesure de la performance d'un compte ou d'un portefeuille, à partir de ses valorisations successives
//! (instantanés) et de ses flux externes (argent apporté ou retiré par l'investisseur).
//!
//! - TWR (rendement pondéré par le temps) : chaîne les rendements des sous-périodes entre deux
//!   valorisations ; neutralise le montant et le moment des apports. Chaque sous-période suit la
//!   méthode de Dietz modifiée : un flux daté du jour d'une valorisation est inclus dans celle-ci
//!   (poids nul), un flux intermédiaire est pondéré par la part de la sous-période restant à courir.
//! - MWR (rendement pondéré par les capitaux, ou TRI) : taux annuel qui annule la valeur actuelle des
//!   flux (XIRR, base 365 jours) ; reflète l'effet du calendrier des apports.
//!
//! Une mesure impossible (série trop courte, aucun capital investi) vaut `None`, jamais zéro.

use std::collections::BTreeSet;

use chrono::{Datelike, NaiveDate};

use crate::error::DomainError;
use crate::fx::{Currency, FxRates};
use crate::holding::HoldingKind;
use crate::operation::{Movement, Operation};

const DAYS_PER_YEAR: f64 = 365.0;
/// Écart maximal toléré entre une date et le dernier cours connu de l'indice (week-ends, jours fériés).
pub const BENCHMARK_MAX_GAP_DAYS: i64 = 7;

/// Valeur d'un compte (ou d'un indice) à une date, en EUR.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Valuation {
    pub date: NaiveDate,
    pub value: f64,
}

/// Flux externe du point de vue du portefeuille : > 0 apport, < 0 retrait.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CashFlow {
    pub date: NaiveDate,
    pub amount: f64,
}

/// Point de la courbe base 100 (rendement cumulé pondéré par le temps).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IndexPoint {
    pub date: NaiveDate,
    pub value: f64,
}

/// Rendement d'une année civile, mesuré du `from` au `to` disponibles (année partielle possible).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct YearReturn {
    pub year: i32,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub twr: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PerformanceReport {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub start_value: f64,
    pub end_value: f64,
    /// Apports moins retraits de la période.
    pub net_flows: f64,
    /// Enrichissement hors apports : valeur finale − valeur initiale − flux nets.
    pub gain: f64,
    pub twr: Option<f64>,
    /// `None` sous un an : annualiser une période courte exagère le résultat.
    pub twr_annualized: Option<f64>,
    /// TRI ramené à la durée de la période.
    pub mwr: Option<f64>,
    pub mwr_annualized: Option<f64>,
    pub years: Vec<YearReturn>,
    pub index: Vec<IndexPoint>,
}

fn days_between(from: NaiveDate, to: NaiveDate) -> f64 {
    (to - from).num_days() as f64
}

/// Rendement d'une sous-période (Dietz modifiée) ; `None` sans capital engagé.
fn sub_period_return(start: &Valuation, end: &Valuation, flows: &[CashFlow]) -> Option<f64> {
    let length = days_between(start.date, end.date);
    let inside = flows
        .iter()
        .filter(|flow| flow.date > start.date && flow.date <= end.date);
    let (net, weighted) = inside.fold((0.0, 0.0), |(net, weighted), flow| {
        let weight = days_between(flow.date, end.date) / length;
        (net + flow.amount, weighted + weight * flow.amount)
    });
    let capital = start.value + weighted;
    (length > 0.0 && capital > 0.0).then(|| ((end.value - start.value - net) / capital).max(-1.0))
}

fn sorted(valuations: &[Valuation]) -> Vec<Valuation> {
    let mut sorted = valuations.to_vec();
    sorted.sort_by_key(|valuation| valuation.date);
    sorted.dedup_by_key(|valuation| valuation.date);
    sorted
}

/// Courbe base 100 à chaque valorisation et nombre de sous-périodes réellement mesurées.
fn chained(valuations: &[Valuation], flows: &[CashFlow]) -> (Vec<IndexPoint>, usize) {
    let mut index = Vec::with_capacity(valuations.len());
    let mut level = 100.0;
    let mut measured = 0;
    for (position, valuation) in valuations.iter().enumerate() {
        if position > 0
            && let Some(r) = sub_period_return(&valuations[position - 1], valuation, flows)
        {
            level *= 1.0 + r;
            measured += 1;
        }
        index.push(IndexPoint {
            date: valuation.date,
            value: level,
        });
    }
    (index, measured)
}

/// TWR de la période couverte par les valorisations ; `None` si aucune sous-période n'est mesurable.
pub fn twr(valuations: &[Valuation], flows: &[CashFlow]) -> Option<f64> {
    let (index, measured) = chained(&sorted(valuations), flows);
    let last = index.last()?;
    (measured > 0).then(|| last.value / 100.0 - 1.0)
}

/// Rendement annualisé d'un rendement total sur `days` jours ; `None` sous un an.
pub fn annualize(total: f64, days: i64) -> Option<f64> {
    if days < DAYS_PER_YEAR as i64 {
        return None;
    }
    if total <= -1.0 {
        return Some(-1.0);
    }
    Some((1.0 + total).powf(DAYS_PER_YEAR / days as f64) - 1.0)
}

fn npv(flows: &[(f64, f64)], rate: f64) -> f64 {
    flows
        .iter()
        .map(|(years, amount)| amount * (1.0 + rate).powf(-years))
        .sum()
}

fn npv_derivative(flows: &[(f64, f64)], rate: f64) -> f64 {
    flows
        .iter()
        .map(|(years, amount)| -years * amount * (1.0 + rate).powf(-years - 1.0))
        .sum()
}

fn newton(flows: &[(f64, f64)]) -> Option<f64> {
    let mut rate = 0.1;
    for _ in 0..100 {
        let slope = npv_derivative(flows, rate);
        if slope == 0.0 || !slope.is_finite() {
            return None;
        }
        let next = rate - npv(flows, rate) / slope;
        if !next.is_finite() || next <= -1.0 {
            return None;
        }
        if (next - rate).abs() < 1e-12 {
            return Some(next);
        }
        rate = next;
    }
    None
}

fn bisection(flows: &[(f64, f64)]) -> Option<f64> {
    let mut low = -1.0 + 1e-9;
    let mut high = 1.0;
    let value_at_low = npv(flows, low);
    while value_at_low * npv(flows, high) > 0.0 {
        high = high * 10.0 + 1.0;
        if high > 1e15 {
            return None;
        }
    }
    for _ in 0..300 {
        let middle = (low + high) / 2.0;
        let value = npv(flows, middle);
        if !value.is_finite() {
            return None;
        }
        if value_at_low * value > 0.0 {
            low = middle;
        } else {
            high = middle;
        }
    }
    Some((low + high) / 2.0)
}

/// Taux de rendement interne annuel de flux datés, du point de vue de l'investisseur
/// (< 0 : argent investi, > 0 : argent récupéré). Newton, puis dichotomie si Newton échoue.
/// `None` sans au moins un flux de chaque signe.
pub fn xirr(flows: &[CashFlow]) -> Option<f64> {
    let first = flows.iter().map(|flow| flow.date).min()?;
    let has_in = flows.iter().any(|flow| flow.amount < 0.0);
    let has_out = flows.iter().any(|flow| flow.amount > 0.0);
    if !has_in || !has_out {
        return None;
    }
    let timed: Vec<(f64, f64)> = flows
        .iter()
        .map(|flow| (days_between(first, flow.date) / DAYS_PER_YEAR, flow.amount))
        .collect();
    let tolerance = 1e-7 * timed.iter().map(|(_, a)| a.abs()).sum::<f64>();
    newton(&timed)
        .filter(|rate| npv(&timed, *rate).abs() <= tolerance)
        .or_else(|| bisection(&timed))
        .filter(|rate| rate.is_finite())
}

/// MWR annuel de la période : investir la valeur initiale et les apports, récupérer les retraits et
/// la valeur finale. Perte totale sans rien récupérer : −100 %.
pub fn mwr(valuations: &[Valuation], flows: &[CashFlow]) -> Option<f64> {
    let valuations = sorted(valuations);
    let (start, end) = (valuations.first()?, valuations.last()?);
    if start.date == end.date {
        return None;
    }
    let mut investor = vec![CashFlow {
        date: start.date,
        amount: -start.value,
    }];
    investor.extend(
        flows
            .iter()
            .filter(|flow| flow.date > start.date && flow.date <= end.date)
            .map(|flow| CashFlow {
                date: flow.date,
                amount: -flow.amount,
            }),
    );
    investor.push(CashFlow {
        date: end.date,
        amount: end.value,
    });
    let invested = investor.iter().any(|flow| flow.amount < 0.0);
    let recovered = investor.iter().any(|flow| flow.amount > 0.0);
    if invested && !recovered {
        return Some(-1.0);
    }
    xirr(&investor)
}

/// Rendement de chaque année civile couverte : de la dernière valorisation de l'année précédente
/// (ou, à défaut, de la première de l'année) à la dernière de l'année. Années dans l'ordre chronologique.
pub fn yearly_twr(valuations: &[Valuation], flows: &[CashFlow]) -> Vec<YearReturn> {
    let valuations = sorted(valuations);
    let years: BTreeSet<i32> = valuations.iter().map(|v| v.date.year()).collect();
    years
        .into_iter()
        .filter_map(|year| {
            let base = valuations
                .iter()
                .rev()
                .find(|v| v.date.year() < year)
                .or_else(|| valuations.iter().find(|v| v.date.year() == year))?;
            let mut window = vec![*base];
            window.extend(
                valuations
                    .iter()
                    .filter(|v| v.date.year() == year && v.date > base.date),
            );
            let to = window.last()?.date;
            (to > base.date).then(|| YearReturn {
                year,
                from: base.date,
                to,
                twr: twr(&window, flows),
            })
        })
        .collect()
}

/// Toutes les mesures de la période couverte par les valorisations ; `None` sans valorisation.
pub fn measure(valuations: &[Valuation], flows: &[CashFlow]) -> Option<PerformanceReport> {
    let valuations = sorted(valuations);
    let (start, end) = (*valuations.first()?, *valuations.last()?);
    let days = (end.date - start.date).num_days();
    let net_flows: f64 = flows
        .iter()
        .filter(|flow| flow.date > start.date && flow.date <= end.date)
        .map(|flow| flow.amount)
        .sum();
    let (index, _) = chained(&valuations, flows);
    let twr = twr(&valuations, flows);
    let mwr_annualized = mwr(&valuations, flows);
    let mwr = mwr_annualized.map(|rate| {
        if rate <= -1.0 {
            -1.0
        } else {
            (1.0 + rate).powf(days as f64 / DAYS_PER_YEAR) - 1.0
        }
    });
    Some(PerformanceReport {
        from: start.date,
        to: end.date,
        start_value: start.value,
        end_value: end.value,
        net_flows,
        gain: end.value - start.value - net_flows,
        twr,
        twr_annualized: twr.and_then(|total| annualize(total, days)),
        mwr,
        mwr_annualized: mwr_annualized.filter(|_| days >= DAYS_PER_YEAR as i64),
        years: yearly_twr(&valuations, flows),
        index,
    })
}

/// Flux externes d'un compte d'après son journal, convertis en EUR aux taux fournis.
///
/// - Journal avec versements ou retraits (espèces suivies) : seuls ceux-ci sont des flux externes ;
///   achats, ventes, dividendes et frais restent internes au compte.
/// - Journal sans versement ni retrait (seuls les titres sont suivis) : un achat est un apport
///   (montant + frais), une vente un retrait (montant − frais), un dividende un retrait (versé hors du
///   compte), des frais un apport (payés depuis l'extérieur).
pub fn external_flows(operations: &[Operation], rates: &FxRates) -> Vec<CashFlow> {
    let tracks_cash = operations
        .iter()
        .any(|op| matches!(op.movement, Movement::Deposit(_) | Movement::Withdrawal(_)));
    operations
        .iter()
        .filter_map(|op| {
            let amount = match (&op.movement, tracks_cash) {
                (Movement::Deposit(amount), true) => amount.value(),
                (Movement::Withdrawal(amount), true) => -amount.value(),
                (Movement::Buy(trade), false) => trade.gross() + trade.fees(),
                (Movement::Sell(trade), false) => -(trade.gross() - trade.fees()),
                (Movement::Dividend { amount, .. }, false) => -amount.value(),
                (Movement::Fee { amount, .. }, false) => amount.value(),
                _ => return None,
            };
            Some(CashFlow {
                date: op.date,
                amount: rates.to_eur(amount, &op.currency),
            })
        })
        .collect()
}

// ---------------------------------------------------------------- indice de référence

/// Indice de référence choisi par l'utilisateur, coté par les fournisseurs de cours existants.
#[derive(Debug, Clone, PartialEq)]
pub struct Benchmark {
    name: String,
    symbol: Option<String>,
    isin: Option<String>,
    kind: HoldingKind,
    currency: Currency,
}

fn non_blank(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

impl Benchmark {
    pub fn new(
        name: &str,
        symbol: Option<String>,
        isin: Option<String>,
        kind: HoldingKind,
        currency: Currency,
    ) -> Result<Self, DomainError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(DomainError::Rule("Le nom de l'indice est requis"));
        }
        let (symbol, isin) = (non_blank(symbol), non_blank(isin));
        if symbol.is_none() && isin.is_none() {
            return Err(DomainError::Rule(
                "Un ticker ou un ISIN est requis pour suivre l'indice",
            ));
        }
        if !matches!(kind, HoldingKind::Security | HoldingKind::Crypto) {
            return Err(DomainError::Rule(
                "L'indice doit être un titre coté (security) ou une crypto (crypto)",
            ));
        }
        Ok(Self {
            name: name.to_string(),
            symbol,
            isin: isin.map(|isin| isin.to_ascii_uppercase()),
            kind,
            currency,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn symbol(&self) -> Option<&str> {
        self.symbol.as_deref()
    }
    pub fn isin(&self) -> Option<&str> {
        self.isin.as_deref()
    }
    pub const fn kind(&self) -> HoldingKind {
        self.kind
    }
    pub fn currency(&self) -> &Currency {
        &self.currency
    }
}

/// Comparaison de la courbe base 100 du portefeuille à l'indice, sur la fenêtre où l'indice a des cours.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkComparison {
    pub from: NaiveDate,
    pub to: NaiveDate,
    /// Pour chaque point du portefeuille : l'indice ramené au niveau du portefeuille au début de la
    /// fenêtre ; `None` hors fenêtre ou si le dernier cours connu a plus de 7 jours.
    pub points: Vec<Option<f64>>,
    pub benchmark_return: f64,
    /// Rendement (TWR) du portefeuille sur la même fenêtre.
    pub portfolio_return: f64,
}

fn price_at(prices: &[Valuation], date: NaiveDate) -> Option<f64> {
    prices
        .iter()
        .rev()
        .find(|price| price.date <= date)
        .filter(|price| (date - price.date).num_days() <= BENCHMARK_MAX_GAP_DAYS)
        .map(|price| price.value)
}

/// `None` si l'indice n'a aucun cours exploitable sur au moins deux dates du portefeuille.
pub fn compare(index: &[IndexPoint], prices: &[Valuation]) -> Option<BenchmarkComparison> {
    let prices: Vec<Valuation> = sorted(prices)
        .into_iter()
        .filter(|price| price.value.is_finite() && price.value > 0.0)
        .collect();
    let start = index
        .iter()
        .position(|point| price_at(&prices, point.date).is_some())?;
    let base = index[start];
    let base_price = price_at(&prices, base.date)?;
    if base.value <= 0.0 {
        return None;
    }
    let points: Vec<Option<f64>> = index
        .iter()
        .enumerate()
        .map(|(position, point)| {
            (position >= start)
                .then(|| price_at(&prices, point.date))
                .flatten()
                .map(|price| price / base_price * base.value)
        })
        .collect();
    let end = points.iter().rposition(Option::is_some)?;
    if end == start {
        return None;
    }
    let end_level = points[end]?;
    Some(BenchmarkComparison {
        from: base.date,
        to: index[end].date,
        benchmark_return: end_level / base.value - 1.0,
        portfolio_return: index[end].value / base.value - 1.0,
        points,
    })
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::ids::{AccountId, HoldingId, OperationId};
    use crate::operation::{Amount, Trade};

    fn d(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }
    fn v(date: NaiveDate, value: f64) -> Valuation {
        Valuation { date, value }
    }
    fn f(date: NaiveDate, amount: f64) -> CashFlow {
        CashFlow { date, amount }
    }
    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn twr_without_flows_is_the_value_change() {
        let values = [v(d(2025, 1, 1), 100.0), v(d(2025, 6, 1), 110.0)];
        assert!(close(twr(&values, &[]).unwrap(), 0.10));
    }

    #[test]
    fn twr_chains_sub_periods_and_ignores_the_deposit_amount() {
        // +10 %, puis apport de 890 le jour du 2e point (inclus), puis +5 % sur 2 000.
        let values = [
            v(d(2025, 1, 1), 100.0),
            v(d(2025, 2, 1), 1_000.0),
            v(d(2025, 3, 1), 1_050.0),
        ];
        let flows = [f(d(2025, 2, 1), 890.0)];
        assert!(close(twr(&values, &flows).unwrap(), 1.10 * 1.05 - 1.0));
    }

    #[test]
    fn same_day_flow_belongs_to_that_day_and_mid_period_flow_is_weighted() {
        let start = v(d(2025, 1, 1), 100.0);
        let end = v(d(2025, 1, 11), 220.0);
        // Apport le jour même de la valorisation finale : (220 − 100 − 100) / 100.
        assert!(close(
            twr(&[start, end], &[f(d(2025, 1, 11), 100.0)]).unwrap(),
            0.20
        ));
        // Apport à mi-période : (220 − 100 − 100) / (100 + 0,5 × 100).
        assert!(close(
            twr(&[start, end], &[f(d(2025, 1, 6), 100.0)]).unwrap(),
            20.0 / 150.0
        ));
        // Un flux daté du premier point est déjà dans la valeur initiale.
        assert!(close(
            twr(&[start, end], &[f(d(2025, 1, 1), 500.0)]).unwrap(),
            1.20
        ));
    }

    #[test]
    fn total_loss_is_minus_one_hundred_percent() {
        let values = [v(d(2024, 1, 1), 1_000.0), v(d(2025, 1, 1), 0.0)];
        assert!(close(twr(&values, &[]).unwrap(), -1.0));
        assert!(close(mwr(&values, &[]).unwrap(), -1.0));
        let report = measure(&values, &[]).unwrap();
        assert_eq!(report.mwr, Some(-1.0));
        assert_eq!(report.twr_annualized, Some(-1.0));
    }

    #[test]
    fn too_short_or_empty_series_are_explicitly_undefined() {
        assert_eq!(twr(&[], &[]), None);
        assert_eq!(twr(&[v(d(2025, 1, 1), 100.0)], &[]), None);
        assert_eq!(mwr(&[v(d(2025, 1, 1), 100.0)], &[]), None);
        assert!(measure(&[], &[]).is_none());
        // Aucun capital engagé : rien à mesurer.
        let empty = [v(d(2025, 1, 1), 0.0), v(d(2025, 2, 1), 0.0)];
        assert_eq!(twr(&empty, &[]), None);
        assert_eq!(mwr(&empty, &[]), None);
        let single = measure(&[v(d(2025, 1, 1), 100.0)], &[]).unwrap();
        assert_eq!((single.twr, single.mwr), (None, None));
    }

    #[test]
    fn sub_periods_before_the_first_deposit_are_skipped() {
        let values = [
            v(d(2025, 1, 1), 0.0),
            v(d(2025, 1, 2), 1_000.0),
            v(d(2025, 1, 3), 1_100.0),
        ];
        let flows = [f(d(2025, 1, 2), 1_000.0)];
        // 0 → 1 000 : capital initial nul, sous-période ignorée ; puis +10 %.
        assert!(close(twr(&values, &flows).unwrap(), 0.10));
    }

    #[test]
    fn annualization_needs_at_least_a_year() {
        assert_eq!(annualize(0.10, 364), None);
        assert!(close(annualize(0.10, 365).unwrap(), 0.10));
        assert!(close(annualize(0.21, 730).unwrap(), 0.10));
    }

    #[test]
    fn xirr_matches_hand_computed_rates() {
        // −1 000 puis +1 100 un an plus tard : 10 %.
        let one_year = [f(d(2025, 1, 1), -1_000.0), f(d(2026, 1, 1), 1_100.0)];
        assert!(close(xirr(&one_year).unwrap(), 0.10));
        // −1 000, −1 000 un an après, +2 310 deux ans après : 1 000 × 1,1² + 1 000 × 1,1 = 2 310.
        let two_years = [
            f(d(2024, 1, 1), -1_000.0),
            f(d(2024, 12, 31), -1_000.0),
            f(d(2025, 12, 31), 2_310.0),
        ];
        assert!((xirr(&two_years).unwrap() - 0.10).abs() < 1e-3);
        // Flux d'un seul signe : indéfini.
        assert_eq!(
            xirr(&[f(d(2025, 1, 1), -1.0), f(d(2026, 1, 1), -1.0)]),
            None
        );
    }

    #[test]
    fn xirr_falls_back_to_bisection_on_extreme_rates() {
        // ×2 en dix jours : taux annuel de l'ordre de 10¹¹, hors de portée de Newton depuis 10 %.
        let flows = [f(d(2025, 1, 1), -1.0), f(d(2025, 1, 11), 2.0)];
        let rate = xirr(&flows).unwrap();
        let back = (1.0 + rate).powf(10.0 / DAYS_PER_YEAR);
        assert!((back - 2.0).abs() < 1e-6, "{rate}");
    }

    #[test]
    fn mwr_reflects_the_timing_of_deposits_unlike_twr() {
        // −10 % sur 100, puis apport de 1 000 (inclus le 1er juillet) et +10 % : TWR ≈ −1 %, MWR > 0.
        let values = [
            v(d(2024, 1, 1), 100.0),
            v(d(2024, 7, 1), 1_090.0),
            v(d(2025, 1, 1), 1_199.0),
        ];
        let flows = [f(d(2024, 7, 1), 1_000.0)];
        let report = measure(&values, &flows).unwrap();
        assert!(close(report.twr.unwrap(), 0.9 * 1.1 - 1.0));
        assert!(report.mwr.unwrap() > 0.0);
        assert!(close(report.net_flows, 1_000.0));
        assert!(close(report.gain, 99.0));
        assert!(report.twr_annualized.is_some() && report.mwr_annualized.is_some());
    }

    #[test]
    fn short_periods_are_not_annualized() {
        let values = [v(d(2025, 1, 1), 100.0), v(d(2025, 3, 1), 105.0)];
        let report = measure(&values, &[]).unwrap();
        assert!(close(report.twr.unwrap(), 0.05));
        assert!(close(report.mwr.unwrap(), 0.05));
        assert_eq!((report.twr_annualized, report.mwr_annualized), (None, None));
        assert_eq!(report.index.len(), 2);
        assert!(close(report.index[1].value, 105.0));
    }

    #[test]
    fn calendar_years_start_from_the_previous_year_end() {
        let values = [
            v(d(2024, 6, 1), 100.0),
            v(d(2024, 12, 31), 110.0),
            v(d(2025, 6, 30), 121.0),
        ];
        let years = yearly_twr(&values, &[]);
        assert_eq!(years.len(), 2);
        assert_eq!((years[0].year, years[0].from), (2024, d(2024, 6, 1)));
        assert!(close(years[0].twr.unwrap(), 0.10));
        assert_eq!(
            (years[1].year, years[1].from, years[1].to),
            (2025, d(2024, 12, 31), d(2025, 6, 30))
        );
        assert!(close(years[1].twr.unwrap(), 0.10));
        // Une seule valorisation dans l'année et aucune avant : année omise.
        assert!(yearly_twr(&[v(d(2025, 1, 1), 1.0)], &[]).is_empty());
    }

    fn operation(date: NaiveDate, movement: Movement) -> Operation {
        Operation {
            id: OperationId::generate(),
            account_id: AccountId::generate(),
            date,
            movement,
            currency: Currency::eur(),
            note: None,
            recorded_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        }
    }

    #[test]
    fn flows_follow_cash_deposits_when_the_journal_tracks_them() {
        let holding = HoldingId::generate();
        let buy = Movement::Buy(Trade::new(holding, 10.0, 50.0, 2.0).unwrap());
        let amount = |value| Amount::new(value, "amount").unwrap();
        let ops = [
            operation(d(2025, 1, 1), Movement::Deposit(amount(1_000.0))),
            operation(d(2025, 1, 2), buy),
            operation(d(2025, 2, 1), Movement::Withdrawal(amount(200.0))),
        ];
        let flows = external_flows(&ops, &FxRates::default());
        assert_eq!(
            flows,
            vec![f(d(2025, 1, 1), 1_000.0), f(d(2025, 2, 1), -200.0)]
        );
    }

    #[test]
    fn flows_follow_trades_when_only_positions_are_tracked() {
        let holding = HoldingId::generate();
        let amount = |value| Amount::new(value, "amount").unwrap();
        let ops = [
            operation(
                d(2025, 1, 2),
                Movement::Buy(Trade::new(holding, 10.0, 50.0, 2.0).unwrap()),
            ),
            operation(
                d(2025, 3, 1),
                Movement::Sell(Trade::new(holding, 4.0, 60.0, 1.0).unwrap()),
            ),
            operation(
                d(2025, 4, 1),
                Movement::Dividend {
                    holding: Some(holding),
                    amount: amount(5.0),
                },
            ),
            operation(
                d(2025, 5, 1),
                Movement::Fee {
                    holding: None,
                    amount: amount(3.0),
                },
            ),
        ];
        let amounts: Vec<f64> = external_flows(&ops, &FxRates::default())
            .iter()
            .map(|flow| flow.amount)
            .collect();
        assert_eq!(amounts, vec![502.0, -239.0, -5.0, 3.0]);
    }

    #[test]
    fn benchmark_requires_a_name_and_an_identifier() {
        let eur = Currency::eur;
        assert!(
            Benchmark::new(" ", Some("IWDA".into()), None, HoldingKind::Security, eur()).is_err()
        );
        assert!(
            Benchmark::new(
                "Monde",
                None,
                Some(" ".into()),
                HoldingKind::Security,
                eur()
            )
            .is_err()
        );
        assert!(
            Benchmark::new(
                "Monde",
                Some("X".into()),
                None,
                HoldingKind::FondsEuro,
                eur()
            )
            .is_err()
        );
        let ok = Benchmark::new(
            "Monde",
            None,
            Some("ie00demo0001".into()),
            HoldingKind::Security,
            eur(),
        )
        .unwrap();
        assert_eq!((ok.isin(), ok.symbol()), (Some("IE00DEMO0001"), None));
    }

    #[test]
    fn comparison_rebases_the_index_on_the_portfolio_at_the_first_common_date() {
        let index = [
            IndexPoint {
                date: d(2025, 1, 1),
                value: 100.0,
            },
            IndexPoint {
                date: d(2025, 1, 10),
                value: 110.0,
            },
            IndexPoint {
                date: d(2025, 1, 20),
                value: 121.0,
            },
        ];
        // L'indice n'a de cours qu'à partir du 9 (pris le 10) : fenêtre du 10 au 20.
        let prices = [v(d(2025, 1, 9), 50.0), v(d(2025, 1, 20), 55.0)];
        let comparison = compare(&index, &prices).unwrap();
        assert_eq!(
            (comparison.from, comparison.to),
            (d(2025, 1, 10), d(2025, 1, 20))
        );
        assert_eq!(comparison.points[0], None);
        assert!(close(comparison.points[1].unwrap(), 110.0));
        assert!(close(comparison.points[2].unwrap(), 121.0));
        assert!(close(comparison.benchmark_return, 0.10));
        assert!(close(comparison.portfolio_return, 0.10));
    }

    #[test]
    fn comparison_is_unavailable_without_enough_index_history() {
        let index = [
            IndexPoint {
                date: d(2025, 1, 1),
                value: 100.0,
            },
            IndexPoint {
                date: d(2025, 3, 1),
                value: 101.0,
            },
        ];
        assert!(compare(&index, &[]).is_none());
        // Un seul cours, trop ancien pour la seconde date : pas de fenêtre de comparaison.
        assert!(compare(&index, &[v(d(2025, 1, 1), 10.0)]).is_none());
    }
}
