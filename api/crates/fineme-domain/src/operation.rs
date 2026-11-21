//! Journal d'opérations d'un compte d'investissement et recalcul des positions.
//!
//! Méthode du prix de revient unitaire (PRU) moyen pondéré, telle qu'appliquée en France :
//! - un achat recalcule le PRU : (quantité × PRU + quantité achetée × cours + frais) / nouvelle quantité ;
//! - une vente ne change pas le PRU ; elle dégage une plus-value réalisée
//!   = quantité vendue × (cours − PRU) − frais de vente ;
//! - une vente totale remet la position à zéro (le PRU repart du prochain achat).

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Datelike, NaiveDate, Utc};

use crate::error::{DomainError, non_negative};
use crate::fx::{Currency, FxRates};
use crate::ids::{AccountId, HoldingId, OperationId};
use crate::property::string_enum;

string_enum!(OperationKind {
    Buy => "buy", Sell => "sell", Dividend => "dividend", Fee => "fee",
    Deposit => "deposit", Withdrawal => "withdrawal"
} invalid = "Type d'opération invalide (buy, sell, dividend, fee, deposit, withdrawal)");

/// En dessous de ce reste, une quantité est considérée comme soldée (erreurs d'arrondi des décimales).
const QUANTITY_EPSILON: f64 = 1e-9;

/// Montant strictement positif et fini.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Amount(f64);

impl Amount {
    pub fn new(value: f64, field: &'static str) -> Result<Self, DomainError> {
        if !value.is_finite() {
            Err(DomainError::NotFinite(field))
        } else if value <= 0.0 {
            Err(DomainError::NotPositive(field))
        } else {
            Ok(Self(value))
        }
    }

    pub const fn value(&self) -> f64 {
        self.0
    }
}

/// Achat ou vente d'une position : quantité > 0, cours ≥ 0, frais ≥ 0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trade {
    holding: HoldingId,
    quantity: Amount,
    unit_price: f64,
    fees: f64,
}

impl Trade {
    pub fn new(
        holding: HoldingId,
        quantity: f64,
        unit_price: f64,
        fees: f64,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            holding,
            quantity: Amount::new(quantity, "quantity")?,
            unit_price: non_negative(unit_price, "unit_price")?,
            fees: non_negative(fees, "fees")?,
        })
    }

    pub const fn holding(&self) -> HoldingId {
        self.holding
    }
    pub const fn quantity(&self) -> f64 {
        self.quantity.value()
    }
    pub const fn unit_price(&self) -> f64 {
        self.unit_price
    }
    pub const fn fees(&self) -> f64 {
        self.fees
    }
    /// Montant brut (quantité × cours), hors frais.
    pub fn gross(&self) -> f64 {
        self.quantity() * self.unit_price
    }
}

/// Ce que fait l'opération ; chaque variante ne porte que les champs qui ont un sens pour elle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Movement {
    Buy(Trade),
    Sell(Trade),
    /// Dividende ou coupon, éventuellement rattaché à une position.
    Dividend {
        holding: Option<HoldingId>,
        amount: Amount,
    },
    /// Frais hors achat/vente (droits de garde, frais de gestion…).
    Fee {
        holding: Option<HoldingId>,
        amount: Amount,
    },
    /// Versement d'espèces sur le compte.
    Deposit(Amount),
    /// Retrait d'espèces du compte.
    Withdrawal(Amount),
}

/// Champs bruts d'une opération, avant validation.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MovementFields {
    pub holding: Option<HoldingId>,
    pub quantity: Option<f64>,
    pub unit_price: Option<f64>,
    pub amount: Option<f64>,
    pub fees: Option<f64>,
}

impl Movement {
    /// Construit le mouvement d'un type donné ; les champs sans objet pour ce type sont ignorés.
    pub fn new(kind: OperationKind, fields: MovementFields) -> Result<Self, DomainError> {
        let amount = || {
            fields
                .amount
                .ok_or(DomainError::Rule("Le montant de l'opération est requis"))
                .and_then(|value| Amount::new(value, "amount"))
        };
        match kind {
            OperationKind::Buy | OperationKind::Sell => {
                let holding = fields.holding.ok_or(DomainError::Rule(
                    "Une position est requise pour un achat ou une vente",
                ))?;
                let quantity = fields
                    .quantity
                    .ok_or(DomainError::Rule("La quantité est requise"))?;
                let unit_price = fields
                    .unit_price
                    .ok_or(DomainError::Rule("Le cours unitaire est requis"))?;
                let trade = Trade::new(holding, quantity, unit_price, fields.fees.unwrap_or(0.0))?;
                Ok(if kind == OperationKind::Buy {
                    Movement::Buy(trade)
                } else {
                    Movement::Sell(trade)
                })
            }
            OperationKind::Dividend => Ok(Movement::Dividend {
                holding: fields.holding,
                amount: amount()?,
            }),
            OperationKind::Fee => Ok(Movement::Fee {
                holding: fields.holding,
                amount: amount()?,
            }),
            OperationKind::Deposit | OperationKind::Withdrawal => {
                if fields.holding.is_some() {
                    return Err(DomainError::Rule(
                        "Un versement ou un retrait ne concerne pas une position",
                    ));
                }
                let amount = amount()?;
                Ok(if kind == OperationKind::Deposit {
                    Movement::Deposit(amount)
                } else {
                    Movement::Withdrawal(amount)
                })
            }
        }
    }

    pub const fn kind(&self) -> OperationKind {
        match self {
            Movement::Buy(_) => OperationKind::Buy,
            Movement::Sell(_) => OperationKind::Sell,
            Movement::Dividend { .. } => OperationKind::Dividend,
            Movement::Fee { .. } => OperationKind::Fee,
            Movement::Deposit(_) => OperationKind::Deposit,
            Movement::Withdrawal(_) => OperationKind::Withdrawal,
        }
    }

    pub const fn trade(&self) -> Option<&Trade> {
        match self {
            Movement::Buy(trade) | Movement::Sell(trade) => Some(trade),
            _ => None,
        }
    }

    pub fn holding(&self) -> Option<HoldingId> {
        match self {
            Movement::Buy(trade) | Movement::Sell(trade) => Some(trade.holding()),
            Movement::Dividend { holding, .. } | Movement::Fee { holding, .. } => *holding,
            Movement::Deposit(_) | Movement::Withdrawal(_) => None,
        }
    }

    /// Montant de l'opération : brut (quantité × cours) pour un achat ou une vente.
    pub fn amount(&self) -> f64 {
        match self {
            Movement::Buy(trade) | Movement::Sell(trade) => trade.gross(),
            Movement::Dividend { amount, .. }
            | Movement::Fee { amount, .. }
            | Movement::Deposit(amount)
            | Movement::Withdrawal(amount) => amount.value(),
        }
    }

    /// Frais payés : ceux de l'achat ou de la vente, ou le montant d'une opération de frais.
    pub fn fees_paid(&self) -> f64 {
        match self {
            Movement::Buy(trade) | Movement::Sell(trade) => trade.fees(),
            Movement::Fee { amount, .. } => amount.value(),
            _ => 0.0,
        }
    }

    /// Vrai si les deux mouvements sont identiques (type, position, quantité, cours, montant, frais),
    /// aux erreurs d'arrondi près : sert à reconnaître une ligne déjà importée.
    pub fn same_as(&self, other: &Movement) -> bool {
        let close = |a: f64, b: f64| (a - b).abs() <= 1e-6 * a.abs().max(b.abs()).max(1.0);
        let same_trade = match (self.trade(), other.trade()) {
            (Some(a), Some(b)) => {
                close(a.quantity(), b.quantity()) && close(a.unit_price(), b.unit_price())
            }
            (None, None) => true,
            _ => false,
        };
        self.kind() == other.kind()
            && self.holding() == other.holding()
            && same_trade
            && close(self.amount(), other.amount())
            && close(self.fees_paid(), other.fees_paid())
    }
}

/// Opération du journal d'un compte. `currency` : devise de la position concernée, sinon du compte.
#[derive(Debug, Clone, PartialEq)]
pub struct Operation {
    pub id: OperationId,
    pub account_id: AccountId,
    pub date: NaiveDate,
    pub movement: Movement,
    pub currency: Currency,
    pub note: Option<String>,
    /// Moment de la saisie : départage les opérations d'un même jour.
    pub recorded_at: DateTime<Utc>,
}

impl Operation {
    /// Holding dont l'opération fait varier la quantité ou le PRU (achat ou vente).
    pub fn traded_holding(&self) -> Option<HoldingId> {
        self.movement.trade().map(Trade::holding)
    }
}

/// Quantité détenue et PRU (devise de la position) ; `average_price` est `None` sans quantité.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub quantity: f64,
    pub average_price: Option<f64>,
}

impl Position {
    pub const EMPTY: Position = Position {
        quantity: 0.0,
        average_price: None,
    };

    fn buy(self, trade: &Trade) -> Position {
        let held_cost = self.quantity * self.average_price.unwrap_or(0.0);
        let quantity = self.quantity + trade.quantity();
        Position {
            quantity,
            average_price: Some((held_cost + trade.gross() + trade.fees()) / quantity),
        }
    }

    /// Nouvelle position et plus-value réalisée.
    fn sell(self, trade: &Trade, date: NaiveDate) -> Result<(Position, f64), DomainError> {
        if trade.quantity() > self.quantity + QUANTITY_EPSILON {
            return Err(DomainError::Oversold {
                date,
                held: self.quantity,
            });
        }
        let average_price = self.average_price.unwrap_or(0.0);
        let gain = trade.quantity() * (trade.unit_price() - average_price) - trade.fees();
        let quantity = self.quantity - trade.quantity();
        let position = if quantity <= QUANTITY_EPSILON {
            Position::EMPTY
        } else {
            Position {
                quantity,
                average_price: self.average_price,
            }
        };
        Ok((position, gain))
    }
}

/// Montants d'une année civile, en EUR.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct YearSummary {
    pub year: i32,
    /// Plus-values (ou moins-values) réalisées par les ventes, nettes des frais.
    pub realized_gain: f64,
    pub dividends: f64,
    /// Tous les frais payés : achats, ventes et opérations de frais.
    pub fees: f64,
    pub deposits: f64,
    pub withdrawals: f64,
}

/// Résultat du rejeu chronologique d'un journal : positions finales et plus-value de chaque vente.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ledger {
    positions: HashMap<HoldingId, Position>,
    realized: HashMap<OperationId, f64>,
}

/// Opérations dans l'ordre chronologique (date, puis ordre de saisie).
pub fn chronological(operations: &[Operation]) -> Vec<&Operation> {
    let mut sorted: Vec<&Operation> = operations.iter().collect();
    sorted.sort_by_key(|op| (op.date, op.recorded_at, op.id));
    sorted
}

impl Ledger {
    /// Rejoue le journal ; refuse une vente supérieure à la quantité détenue à sa date.
    pub fn replay(operations: &[Operation]) -> Result<Self, DomainError> {
        let mut ledger = Ledger::default();
        for operation in chronological(operations) {
            match &operation.movement {
                Movement::Buy(trade) => {
                    let position = ledger.position(trade.holding()).buy(trade);
                    ledger.positions.insert(trade.holding(), position);
                }
                Movement::Sell(trade) => {
                    let (position, gain) = ledger
                        .position(trade.holding())
                        .sell(trade, operation.date)?;
                    ledger.positions.insert(trade.holding(), position);
                    ledger.realized.insert(operation.id, gain);
                }
                _ => {}
            }
        }
        Ok(ledger)
    }

    /// Position issue du journal ; vide si le journal ne contient aucun achat de ce titre.
    pub fn position(&self, holding: HoldingId) -> Position {
        self.positions
            .get(&holding)
            .copied()
            .unwrap_or(Position::EMPTY)
    }

    /// Plus-value réalisée par une vente (devise de l'opération).
    pub fn realized_gain(&self, operation: OperationId) -> Option<f64> {
        self.realized.get(&operation).copied()
    }

    /// Synthèse par année civile, de la plus récente à la plus ancienne, convertie en EUR
    /// aux taux fournis (taux actuels : approximation documentée).
    pub fn yearly(&self, operations: &[Operation], rates: &FxRates) -> Vec<YearSummary> {
        let mut years: BTreeMap<i32, YearSummary> = BTreeMap::new();
        for operation in operations {
            let year = operation.date.year();
            let entry = years.entry(year).or_insert(YearSummary {
                year,
                ..YearSummary::default()
            });
            let eur = |amount: f64| rates.to_eur(amount, &operation.currency);
            entry.fees += eur(operation.movement.fees_paid());
            match operation.movement {
                Movement::Sell(_) => {
                    entry.realized_gain += eur(self.realized_gain(operation.id).unwrap_or(0.0));
                }
                Movement::Dividend { amount, .. } => entry.dividends += eur(amount.value()),
                Movement::Deposit(amount) => entry.deposits += eur(amount.value()),
                Movement::Withdrawal(amount) => entry.withdrawals += eur(amount.value()),
                Movement::Buy(_) | Movement::Fee { .. } => {}
            }
        }
        years.into_values().rev().collect()
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn day(month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2025, month, day).unwrap()
    }

    struct Journal {
        account: AccountId,
        operations: Vec<Operation>,
    }

    impl Journal {
        fn new() -> Self {
            Self {
                account: AccountId::generate(),
                operations: Vec::new(),
            }
        }

        fn add(&mut self, date: NaiveDate, movement: Movement) -> OperationId {
            let id = OperationId::generate();
            let recorded_at = Utc
                .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
                .unwrap()
                .checked_add_signed(chrono::Duration::seconds(self.operations.len() as i64))
                .unwrap();
            self.operations.push(Operation {
                id,
                account_id: self.account,
                date,
                movement,
                currency: Currency::eur(),
                note: None,
                recorded_at,
            });
            id
        }

        fn buy(&mut self, date: NaiveDate, h: HoldingId, q: f64, price: f64, fees: f64) {
            self.add(date, Movement::Buy(Trade::new(h, q, price, fees).unwrap()));
        }

        fn sell(
            &mut self,
            date: NaiveDate,
            h: HoldingId,
            q: f64,
            price: f64,
            fees: f64,
        ) -> OperationId {
            self.add(date, Movement::Sell(Trade::new(h, q, price, fees).unwrap()))
        }

        fn ledger(&self) -> Ledger {
            Ledger::replay(&self.operations).unwrap()
        }
    }

    fn close(actual: f64, expected: f64) -> bool {
        (actual - expected).abs() < 1e-9
    }

    #[test]
    fn weighted_average_price_includes_purchase_fees() {
        // 10 × 100 + 5 € de frais, puis 10 × 130 + 5 € : PRU = 2 310 / 20 = 115,50.
        let etf = HoldingId::generate();
        let mut journal = Journal::new();
        journal.buy(day(1, 10), etf, 10.0, 100.0, 5.0);
        journal.buy(day(3, 10), etf, 10.0, 130.0, 5.0);
        let position = journal.ledger().position(etf);
        assert!(close(position.quantity, 20.0));
        assert!(close(position.average_price.unwrap(), 115.5));
    }

    #[test]
    fn sale_keeps_average_price_and_realizes_net_gain() {
        // PRU 115,50 ; vente de 8 à 150 avec 4 € de frais : 8 × 34,50 − 4 = 272.
        let etf = HoldingId::generate();
        let mut journal = Journal::new();
        journal.buy(day(1, 10), etf, 10.0, 100.0, 5.0);
        journal.buy(day(3, 10), etf, 10.0, 130.0, 5.0);
        let sale = journal.sell(day(6, 1), etf, 8.0, 150.0, 4.0);
        let ledger = journal.ledger();
        let position = ledger.position(etf);
        assert!(close(position.quantity, 12.0));
        assert!(close(position.average_price.unwrap(), 115.5));
        assert!(close(ledger.realized_gain(sale).unwrap(), 272.0));
    }

    #[test]
    fn full_sale_resets_the_position_and_next_buy_starts_fresh() {
        let etf = HoldingId::generate();
        let mut journal = Journal::new();
        journal.buy(day(1, 10), etf, 3.0, 100.0, 0.0);
        let sale = journal.sell(day(2, 10), etf, 3.0, 90.0, 0.0);
        assert_eq!(journal.ledger().position(etf), Position::EMPTY);
        assert!(close(journal.ledger().realized_gain(sale).unwrap(), -30.0));
        journal.buy(day(3, 10), etf, 2.0, 50.0, 1.0);
        let position = journal.ledger().position(etf);
        assert!(close(position.quantity, 2.0));
        assert!(close(position.average_price.unwrap(), 50.5));
    }

    #[test]
    fn fractional_quantities_sold_out_leave_no_dust() {
        let btc = HoldingId::generate();
        let mut journal = Journal::new();
        journal.buy(day(1, 1), btc, 0.1, 30_000.0, 0.0);
        journal.buy(day(1, 2), btc, 0.2, 30_000.0, 0.0);
        journal.sell(day(1, 3), btc, 0.3, 40_000.0, 0.0);
        assert_eq!(journal.ledger().position(btc), Position::EMPTY);
    }

    #[test]
    fn selling_more_than_held_at_that_date_is_refused() {
        let etf = HoldingId::generate();
        let mut journal = Journal::new();
        journal.buy(day(5, 1), etf, 10.0, 100.0, 0.0);
        // La vente est datée avant l'achat : rien n'est détenu à cette date.
        journal.sell(day(4, 1), etf, 1.0, 100.0, 0.0);
        assert_eq!(
            Ledger::replay(&journal.operations),
            Err(DomainError::Oversold {
                date: day(4, 1),
                held: 0.0
            })
        );
    }

    #[test]
    fn operations_of_a_same_day_follow_their_recording_order() {
        let etf = HoldingId::generate();
        let mut journal = Journal::new();
        journal.buy(day(1, 1), etf, 1.0, 100.0, 0.0);
        journal.sell(day(1, 1), etf, 1.0, 110.0, 0.0);
        assert_eq!(journal.ledger().position(etf), Position::EMPTY);
    }

    #[test]
    fn dividends_and_fees_do_not_change_the_position() {
        let etf = HoldingId::generate();
        let mut journal = Journal::new();
        journal.buy(day(1, 1), etf, 4.0, 25.0, 0.0);
        journal.add(
            day(2, 1),
            Movement::Dividend {
                holding: Some(etf),
                amount: Amount::new(12.0, "amount").unwrap(),
            },
        );
        journal.add(
            day(2, 2),
            Movement::Fee {
                holding: None,
                amount: Amount::new(3.0, "amount").unwrap(),
            },
        );
        let position = journal.ledger().position(etf);
        assert_eq!(position.quantity, 4.0);
        assert_eq!(position.average_price, Some(25.0));
    }

    #[test]
    fn yearly_summary_in_eur_most_recent_first() {
        let etf = HoldingId::generate();
        let mut journal = Journal::new();
        journal.add(
            day(1, 2),
            Movement::Deposit(Amount::new(5000.0, "amount").unwrap()),
        );
        journal.buy(day(1, 10), etf, 10.0, 100.0, 2.0);
        journal.sell(day(6, 1), etf, 5.0, 120.0, 1.0);
        journal.add(
            day(7, 1),
            Movement::Dividend {
                holding: Some(etf),
                amount: Amount::new(15.0, "amount").unwrap(),
            },
        );
        let next_year = NaiveDate::from_ymd_opt(2026, 2, 1).unwrap();
        journal.add(
            next_year,
            Movement::Withdrawal(Amount::new(300.0, "amount").unwrap()),
        );
        // Un dividende en dollars, converti au taux fourni (1 EUR = 1,25 USD).
        let id = journal.add(
            next_year,
            Movement::Dividend {
                holding: None,
                amount: Amount::new(50.0, "amount").unwrap(),
            },
        );
        let usd = Currency::parse("USD").unwrap();
        if let Some(op) = journal.operations.iter_mut().find(|op| op.id == id) {
            op.currency = usd.clone();
        }
        let rates = FxRates::new([(usd, 1.25)]);
        let years = journal.ledger().yearly(&journal.operations, &rates);
        assert_eq!(years.len(), 2);
        assert_eq!(years[0].year, 2026);
        assert!(close(years[0].withdrawals, 300.0));
        assert!(close(years[0].dividends, 40.0));
        let y2025 = years[1];
        assert_eq!(y2025.year, 2025);
        // PRU 100,20 ; 5 × (120 − 100,20) − 1 = 98.
        assert!(close(y2025.realized_gain, 98.0));
        assert!(close(y2025.dividends, 15.0));
        assert!(close(y2025.fees, 3.0));
        assert!(close(y2025.deposits, 5000.0));
        assert!(close(y2025.withdrawals, 0.0));
    }

    #[test]
    fn movement_fields_are_validated_per_kind() {
        let h = Some(HoldingId::generate());
        let trade = MovementFields {
            holding: h,
            quantity: Some(2.0),
            unit_price: Some(10.0),
            ..MovementFields::default()
        };
        let buy = Movement::new(OperationKind::Buy, trade).unwrap();
        assert_eq!(
            (buy.kind(), buy.amount(), buy.fees_paid()),
            (OperationKind::Buy, 20.0, 0.0)
        );
        for (kind, fields) in [
            (
                OperationKind::Buy,
                MovementFields {
                    holding: None,
                    ..trade
                },
            ),
            (
                OperationKind::Sell,
                MovementFields {
                    quantity: Some(0.0),
                    ..trade
                },
            ),
            (
                OperationKind::Sell,
                MovementFields {
                    unit_price: None,
                    ..trade
                },
            ),
            (
                OperationKind::Buy,
                MovementFields {
                    fees: Some(-1.0),
                    ..trade
                },
            ),
            (
                OperationKind::Dividend,
                MovementFields {
                    amount: None,
                    ..trade
                },
            ),
            (
                OperationKind::Fee,
                MovementFields {
                    amount: Some(f64::NAN),
                    ..trade
                },
            ),
            (
                OperationKind::Deposit,
                MovementFields {
                    amount: Some(10.0),
                    ..trade
                },
            ),
            (
                OperationKind::Withdrawal,
                MovementFields {
                    holding: None,
                    amount: Some(-5.0),
                    ..trade
                },
            ),
        ] {
            assert!(Movement::new(kind, fields).is_err(), "{kind:?} {fields:?}");
        }
        let deposit = Movement::new(
            OperationKind::Deposit,
            MovementFields {
                amount: Some(100.0),
                ..MovementFields::default()
            },
        )
        .unwrap();
        assert_eq!((deposit.holding(), deposit.amount()), (None, 100.0));
        assert!(OperationKind::parse("transfer").is_err());
    }

    #[test]
    fn same_movement_ignores_rounding_but_not_a_different_field() {
        let h = HoldingId::generate();
        let buy = |quantity: f64, price: f64, fees: f64| {
            Movement::Buy(Trade::new(h, quantity, price, fees).unwrap())
        };
        assert!(buy(3.0, 10.1, 1.0).same_as(&buy(3.0, 10.100_000_001, 1.0)));
        assert!(!buy(3.0, 10.1, 1.0).same_as(&buy(3.0, 10.1, 1.5)));
        assert!(!buy(3.0, 10.1, 1.0).same_as(&buy(2.0, 10.1, 1.0)));
        let sell = Movement::Sell(Trade::new(h, 3.0, 10.1, 1.0).unwrap());
        assert!(!buy(3.0, 10.1, 1.0).same_as(&sell));
        let deposit = Movement::Deposit(Amount::new(50.0, "amount").unwrap());
        let withdrawal = Movement::Withdrawal(Amount::new(50.0, "amount").unwrap());
        assert!(deposit.same_as(&deposit));
        assert!(!deposit.same_as(&withdrawal));
        let dividend = |holding| Movement::Dividend {
            holding,
            amount: Amount::new(4.0, "amount").unwrap(),
        };
        assert!(!dividend(Some(h)).same_as(&dividend(None)));
    }
}
