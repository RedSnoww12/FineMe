//! Patrimoine dans le temps : période relative à aujourd'hui, variation sur une période et comparaison de deux
//! instantanés (totaux, catégories d'actifs, comptes). Le choix de l'instantané en vigueur à une date (le dernier
//! au plus tard ce jour-là) est une lecture du port `SnapshotRepository::at`.

use chrono::{Datelike, Months, NaiveDate};

use crate::DomainError;
use crate::account::AccountKind;
use crate::ids::AccountId;
use crate::portfolio::{AccountSnapshot, Totals};

/// Période relative à aujourd'hui.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    OneMonth,
    ThreeMonths,
    SixMonths,
    YearToDate,
    OneYear,
    All,
}

impl Period {
    pub const ALL: [Self; 6] = [
        Self::OneMonth,
        Self::ThreeMonths,
        Self::SixMonths,
        Self::YearToDate,
        Self::OneYear,
        Self::All,
    ];

    pub fn parse(value: &str) -> Result<Self, DomainError> {
        Self::ALL
            .into_iter()
            .find(|period| period.as_str() == value.trim())
            .ok_or(DomainError::Rule(
                "Période invalide (1m, 3m, 6m, ytd, 1y ou all)",
            ))
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::OneMonth => "1m",
            Self::ThreeMonths => "3m",
            Self::SixMonths => "6m",
            Self::YearToDate => "ytd",
            Self::OneYear => "1y",
            Self::All => "all",
        }
    }

    /// Date de début : aujourd'hui moins la durée (un 31 devient le dernier jour d'un mois plus court),
    /// le 1er janvier pour `ytd`, aucune pour `all` (ni pour une date hors du calendrier de chrono, inatteignable
    /// avec l'horloge du serveur).
    pub fn start(self, today: NaiveDate) -> Option<NaiveDate> {
        let months = |n| today.checked_sub_months(Months::new(n));
        match self {
            Self::OneMonth => months(1),
            Self::ThreeMonths => months(3),
            Self::SixMonths => months(6),
            Self::OneYear => months(12),
            Self::YearToDate => NaiveDate::from_ymd_opt(today.year(), 1, 1),
            Self::All => None,
        }
    }
}

/// Variation du patrimoine net depuis le départ d'une période.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NetWorthChange {
    pub from_date: NaiveDate,
    pub net_worth_start: f64,
    pub delta: f64,
    /// Fraction (0.05 = +5 %) rapportée à |départ| ; `None` si le départ vaut 0.
    pub delta_pct: Option<f64>,
}

impl NetWorthChange {
    pub fn new(from_date: NaiveDate, net_worth_start: f64, current: f64) -> Self {
        let delta = current - net_worth_start;
        Self {
            from_date,
            net_worth_start,
            delta,
            delta_pct: (net_worth_start != 0.0).then(|| delta / net_worth_start.abs()),
        }
    }
}

/// Écart d'une grandeur entre deux dates.
#[derive(Debug, Clone, PartialEq)]
pub struct Delta<K> {
    pub key: K,
    pub from: f64,
    pub to: f64,
    pub delta: f64,
}

impl<K> Delta<K> {
    pub fn new(key: K, from: f64, to: f64) -> Self {
        Self {
            key,
            from,
            to,
            delta: to - from,
        }
    }
}

/// Totaux comparés, dans l'ordre de présentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TotalKey {
    GrossAssets,
    Liabilities,
    NetWorth,
}

impl TotalKey {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GrossAssets => "gross_assets",
            Self::Liabilities => "liabilities",
            Self::NetWorth => "net_worth",
        }
    }
}

/// Ordre de présentation des catégories d'actifs (les prêts n'en sont pas).
pub const ASSET_CATEGORIES: [AccountKind; 6] = [
    AccountKind::RealEstate,
    AccountKind::Checking,
    AccountKind::Savings,
    AccountKind::Other,
    AccountKind::Investment,
    AccountKind::Crypto,
];

/// Écart d'un compte : la valeur d'un prêt compte en négatif.
#[derive(Debug, Clone, PartialEq)]
pub struct AccountDelta {
    pub account_id: AccountId,
    /// Nom et type du dernier relevé.
    pub name: String,
    pub kind: AccountKind,
    pub from: f64,
    pub to: f64,
    pub delta: f64,
}

/// Un instantané tel que la comparaison le lit.
#[derive(Debug, Clone, Copy)]
pub struct SnapshotRef<'a> {
    pub totals: &'a Totals,
    /// `None` : instantané sans détail par compte (ancien ou corrigé à la main).
    pub accounts: Option<&'a [AccountSnapshot]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    pub totals: Vec<Delta<TotalKey>>,
    /// `None` : l'un des deux instantanés n'a pas de détail par compte.
    pub categories: Option<Vec<Delta<AccountKind>>>,
    pub accounts: Option<Vec<AccountDelta>>,
}

fn by_category(accounts: &[AccountSnapshot], kind: AccountKind) -> f64 {
    accounts
        .iter()
        .filter(|account| account.kind == kind)
        .map(|account| account.value)
        .sum()
}

fn signed(account: &AccountSnapshot) -> f64 {
    if account.kind.is_liability() {
        -account.value
    } else {
        account.value
    }
}

/// Écarts entre deux instantanés : totaux, catégories d'actifs non nulles à l'une des dates, et comptes triés
/// par écart absolu décroissant. Un compte présent à une seule date vaut 0 à l'autre.
pub fn compare(from: SnapshotRef<'_>, to: SnapshotRef<'_>) -> Comparison {
    let totals = vec![
        Delta::new(
            TotalKey::GrossAssets,
            from.totals.gross_assets,
            to.totals.gross_assets,
        ),
        Delta::new(
            TotalKey::Liabilities,
            from.totals.liabilities,
            to.totals.liabilities,
        ),
        Delta::new(
            TotalKey::NetWorth,
            from.totals.net_worth,
            to.totals.net_worth,
        ),
    ];
    let (Some(before), Some(after)) = (from.accounts, to.accounts) else {
        return Comparison {
            totals,
            categories: None,
            accounts: None,
        };
    };

    let categories = ASSET_CATEGORIES
        .into_iter()
        .map(|kind| Delta::new(kind, by_category(before, kind), by_category(after, kind)))
        .filter(|row| row.from != 0.0 || row.to != 0.0)
        .collect();

    let mut rows: Vec<AccountDelta> = before
        .iter()
        .map(|account| AccountDelta {
            account_id: account.account_id,
            name: account.name.clone(),
            kind: account.kind,
            from: signed(account),
            to: 0.0,
            delta: 0.0,
        })
        .collect();
    for account in after {
        match rows
            .iter_mut()
            .find(|row| row.account_id == account.account_id)
        {
            Some(row) => {
                row.name.clone_from(&account.name);
                row.kind = account.kind;
                row.to = signed(account);
            }
            None => rows.push(AccountDelta {
                account_id: account.account_id,
                name: account.name.clone(),
                kind: account.kind,
                from: 0.0,
                to: signed(account),
                delta: 0.0,
            }),
        }
    }
    for row in &mut rows {
        row.delta = row.to - row.from;
    }
    // Tri stable : à écart égal, l'ordre des relevés est conservé.
    rows.sort_by(|a, b| b.delta.abs().total_cmp(&a.delta.abs()));

    Comparison {
        totals,
        categories: Some(categories),
        accounts: Some(rows),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn account(id: AccountId, name: &str, kind: AccountKind, value: f64) -> AccountSnapshot {
        AccountSnapshot {
            account_id: id,
            name: name.into(),
            kind,
            value,
            gain: None,
        }
    }

    fn totals(gross: f64, liabilities: f64) -> Totals {
        Totals {
            gross_assets: gross,
            liabilities,
            net_worth: gross - liabilities,
        }
    }

    #[test]
    fn periods_round_trip_and_unknown_is_refused() {
        for period in Period::ALL {
            assert_eq!(Period::parse(period.as_str()), Ok(period));
        }
        assert!(Period::parse("2w").is_err());
        assert_eq!(Period::parse(" 1m "), Ok(Period::OneMonth));
    }

    #[test]
    fn period_start_clamps_to_the_end_of_shorter_months() {
        let today = day(2026, 3, 31);
        assert_eq!(Period::OneMonth.start(today), Some(day(2026, 2, 28)));
        assert_eq!(Period::ThreeMonths.start(today), Some(day(2025, 12, 31)));
        assert_eq!(Period::SixMonths.start(today), Some(day(2025, 9, 30)));
        assert_eq!(
            Period::OneYear.start(day(2024, 2, 29)),
            Some(day(2023, 2, 28))
        );
        assert_eq!(Period::YearToDate.start(today), Some(day(2026, 1, 1)));
        assert_eq!(Period::All.start(today), None);
    }

    #[test]
    fn net_worth_change_has_no_percentage_from_zero() {
        let change = NetWorthChange::new(day(2026, 1, 1), -200.0, 100.0);
        assert_eq!(change.delta, 300.0);
        assert_eq!(change.delta_pct, Some(1.5));
        assert_eq!(
            NetWorthChange::new(day(2026, 1, 1), 0.0, 50.0).delta_pct,
            None
        );
    }

    #[test]
    fn comparison_signs_loans_keeps_latest_names_and_sorts_by_change() {
        let (current, pea, loan, sold) = (
            AccountId::generate(),
            AccountId::generate(),
            AccountId::generate(),
            AccountId::generate(),
        );
        let before = [
            account(current, "Courant", AccountKind::Checking, 1_000.0),
            account(pea, "PEA", AccountKind::Investment, 5_000.0),
            account(loan, "Prêt", AccountKind::Loan, 20_000.0),
            account(sold, "Ancien livret", AccountKind::Savings, 300.0),
        ];
        let new_one = AccountId::generate();
        let after = [
            account(current, "Courant joint", AccountKind::Checking, 1_200.0),
            account(pea, "PEA", AccountKind::Investment, 7_000.0),
            account(loan, "Prêt", AccountKind::Loan, 19_000.0),
            account(new_one, "Bitcoin", AccountKind::Crypto, 50.0),
        ];
        let (t1, t2) = (totals(6_300.0, 20_000.0), totals(8_250.0, 19_000.0));
        let comparison = compare(
            SnapshotRef {
                totals: &t1,
                accounts: Some(&before),
            },
            SnapshotRef {
                totals: &t2,
                accounts: Some(&after),
            },
        );

        let keys: Vec<_> = comparison
            .totals
            .iter()
            .map(|row| row.key.as_str())
            .collect();
        assert_eq!(keys, ["gross_assets", "liabilities", "net_worth"]);
        assert_eq!(comparison.totals[2].delta, 2_950.0);

        let categories = comparison.categories.unwrap();
        let kinds: Vec<_> = categories.iter().map(|row| row.key).collect();
        assert_eq!(
            kinds,
            [
                AccountKind::Checking,
                AccountKind::Savings,
                AccountKind::Investment,
                AccountKind::Crypto
            ]
        );
        assert_eq!(categories[1].to, 0.0);

        let accounts = comparison.accounts.unwrap();
        let order: Vec<_> = accounts.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(
            order,
            ["PEA", "Prêt", "Ancien livret", "Courant joint", "Bitcoin"]
        );
        let loan_row = &accounts[1];
        assert_eq!(
            (loan_row.from, loan_row.to, loan_row.delta),
            (-20_000.0, -19_000.0, 1_000.0)
        );
        assert_eq!((accounts[2].from, accounts[2].to), (300.0, 0.0));
        assert_eq!((accounts[4].from, accounts[4].to), (0.0, 50.0));
    }

    #[test]
    fn comparison_without_detail_keeps_only_the_totals() {
        let t = totals(100.0, 0.0);
        let detailed = [account(
            AccountId::generate(),
            "Courant",
            AccountKind::Checking,
            100.0,
        )];
        let comparison = compare(
            SnapshotRef {
                totals: &t,
                accounts: None,
            },
            SnapshotRef {
                totals: &t,
                accounts: Some(&detailed),
            },
        );
        assert_eq!(comparison.totals.len(), 3);
        assert_eq!(comparison.categories, None);
        assert_eq!(comparison.accounts, None);
    }
}
