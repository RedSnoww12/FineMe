use chrono::{DateTime, NaiveDate, Utc};

use crate::fx::{Currency, FxRates};
use crate::ids::{AccountId, HoldingId};
use crate::property::string_enum;

string_enum!(HoldingKind { Security => "security", Crypto => "crypto", FondsEuro => "fonds_euro", Other => "other" }
    invalid = "Type de position invalide (security, crypto, fonds_euro, other)");

string_enum!(PriceSource { Manual => "manual", Auto => "auto" }
    invalid = "price_source doit valoir manual ou auto");

/// Position (titre, crypto, fonds euro…) détenue dans un compte.
#[derive(Debug, Clone, PartialEq)]
pub struct Holding {
    pub id: HoldingId,
    pub account_id: AccountId,
    pub kind: HoldingKind,
    pub name: String,
    pub isin: Option<String>,
    /// Ticker ou code crypto (BTC).
    pub symbol: Option<String>,
    pub security_type: Option<String>,
    pub quantity: f64,
    /// PRU unitaire, dans `currency`.
    pub buy_price: Option<f64>,
    pub current_price: f64,
    pub currency: Currency,
    /// Frais courants annuels, en fraction (0.002 = 0,2 %).
    pub expense_ratio: Option<f64>,
    pub price_source: PriceSource,
    pub price_updated_at: Option<DateTime<Utc>>,
}

/// Au-delà de ce nombre de jours, un cours récupéré automatiquement est jugé ancien
/// (couvre un week-end prolongé, où les marchés ne cotent pas).
pub const STALE_PRICE_AFTER_DAYS: i64 = 4;

/// Valorisation d'une position, en EUR.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HoldingValuation {
    pub value: f64,
    pub invested: Option<f64>,
    pub gain: Option<f64>,
    /// Plus-value en pourcentage du prix de revient.
    pub gain_pct: Option<f64>,
}

impl Holding {
    /// Vrai si le cours est automatique et que sa dernière mise à jour date de plus de
    /// [`STALE_PRICE_AFTER_DAYS`] jours (ou n'est pas datée) : le fournisseur ne le met plus à jour.
    /// Un cours saisi à la main n'est jamais signalé : son ancienneté est un choix de l'utilisateur.
    pub fn price_is_stale(&self, today: NaiveDate) -> bool {
        self.price_source == PriceSource::Auto
            && self.price_updated_at.is_none_or(|updated_at| {
                (today - updated_at.date_naive()).num_days() > STALE_PRICE_AFTER_DAYS
            })
    }

    pub fn valuation(&self, rates: &FxRates) -> HoldingValuation {
        let value = rates.to_eur(self.quantity * self.current_price, &self.currency);
        let invested = self
            .buy_price
            .map(|price| rates.to_eur(self.quantity * price, &self.currency));
        let gain = invested.map(|cost| value - cost);
        let gain_pct = match (gain, invested) {
            (Some(gain), Some(cost)) if cost.abs() > 1e-9 => Some(gain / cost * 100.0),
            _ => None,
        };
        HoldingValuation {
            value,
            invested,
            gain,
            gain_pct,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holding(buy_price: Option<f64>, currency: &str) -> Holding {
        Holding {
            id: HoldingId::generate(),
            account_id: AccountId::generate(),
            kind: HoldingKind::Security,
            name: "ETF Monde".into(),
            isin: None,
            symbol: None,
            security_type: None,
            quantity: 10.0,
            buy_price,
            current_price: 125.0,
            currency: Currency::parse(currency).unwrap(),
            expense_ratio: None,
            price_source: PriceSource::Manual,
            price_updated_at: None,
        }
    }

    #[test]
    fn valuation_in_eur_with_gain() {
        let rates = FxRates::new([(Currency::parse("USD").unwrap(), 1.25)]);
        let v = holding(Some(100.0), "USD").valuation(&rates);
        assert!((v.value - 1000.0).abs() < 1e-9);
        assert!((v.invested.unwrap() - 800.0).abs() < 1e-9);
        assert!((v.gain.unwrap() - 200.0).abs() < 1e-9);
        assert!((v.gain_pct.unwrap() - 25.0).abs() < 1e-9);
    }

    #[test]
    fn unknown_buy_price_means_unknown_gain() {
        let v = holding(None, "EUR").valuation(&FxRates::default());
        assert_eq!(v.value, 1250.0);
        assert_eq!((v.invested, v.gain, v.gain_pct), (None, None, None));
    }

    #[test]
    fn zero_cost_has_no_gain_pct() {
        let v = holding(Some(0.0), "EUR").valuation(&FxRates::default());
        assert_eq!(v.gain, Some(1250.0));
        assert_eq!(v.gain_pct, None);
    }

    fn auto_price_updated(days_ago: Option<i64>) -> (Holding, NaiveDate) {
        let today = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
        let updated_at = days_ago.map(|days| {
            (today - chrono::Duration::days(days))
                .and_hms_opt(23, 59, 0)
                .unwrap()
                .and_utc()
        });
        let holding = Holding {
            price_source: PriceSource::Auto,
            price_updated_at: updated_at,
            ..holding(None, "EUR")
        };
        (holding, today)
    }

    #[test]
    fn auto_price_becomes_stale_after_the_threshold() {
        for (days_ago, stale) in [
            (0, false),
            (STALE_PRICE_AFTER_DAYS, false),
            (STALE_PRICE_AFTER_DAYS + 1, true),
        ] {
            let (holding, today) = auto_price_updated(Some(days_ago));
            assert_eq!(holding.price_is_stale(today), stale, "{days_ago} jours");
        }
    }

    #[test]
    fn undated_auto_price_is_stale_but_manual_price_never_is() {
        let (undated, today) = auto_price_updated(None);
        assert!(undated.price_is_stale(today));
        let (old, today) = auto_price_updated(Some(365));
        let manual = Holding {
            price_source: PriceSource::Manual,
            ..old
        };
        assert!(!manual.price_is_stale(today));
    }

    #[test]
    fn missing_fx_rate_uses_face_value() {
        assert_eq!(
            holding(None, "USD").valuation(&FxRates::default()).value,
            1250.0
        );
    }
}
