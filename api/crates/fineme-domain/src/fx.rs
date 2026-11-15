//! Devises et taux de change (base EUR).

use std::collections::HashMap;

use crate::error::DomainError;

/// Code devise ISO 4217 : exactement trois lettres, en majuscules.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Currency(String);

impl Currency {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        let code = value.trim().to_ascii_uppercase();
        if code.len() == 3 && code.chars().all(|c| c.is_ascii_alphabetic()) {
            Ok(Self(code))
        } else {
            Err(DomainError::InvalidCurrency)
        }
    }

    pub fn eur() -> Self {
        Self("EUR".to_string())
    }

    pub fn is_eur(&self) -> bool {
        self.0 == "EUR"
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Currency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Taux de change : unités de devise pour 1 EUR. Seuls les taux finis et strictement positifs sont conservés.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FxRates(HashMap<Currency, f64>);

impl FxRates {
    pub fn new(rates: impl IntoIterator<Item = (Currency, f64)>) -> Self {
        Self(
            rates
                .into_iter()
                .filter(|(currency, rate)| !currency.is_eur() && rate.is_finite() && *rate > 0.0)
                .collect(),
        )
    }

    /// Taux connu pour la devise (1.0 pour l'euro).
    pub fn rate(&self, currency: &Currency) -> Option<f64> {
        if currency.is_eur() {
            Some(1.0)
        } else {
            self.0.get(currency).copied()
        }
    }

    /// Convertit un montant en EUR. Taux manquant : repli sur 1.0 (le montant est renvoyé tel quel).
    pub fn to_eur(&self, amount: f64, currency: &Currency) -> f64 {
        amount / self.rate(currency).unwrap_or(1.0)
    }

    /// Convertit un prix entre deux devises ; `None` si un taux manque.
    pub fn convert(&self, price: f64, from: &Currency, to: &Currency) -> Option<f64> {
        if from == to {
            return Some(price);
        }
        Some(price / self.rate(from)? * self.rate(to)?)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Currency, f64)> {
        self.0.iter().map(|(currency, rate)| (currency, *rate))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cur(code: &str) -> Currency {
        Currency::parse(code).unwrap()
    }

    fn rates() -> FxRates {
        FxRates::new([
            (cur("USD"), 1.25),
            (cur("GBP"), 0.8),
            (cur("XXX"), 0.0),
            (cur("YYY"), f64::NAN),
        ])
    }

    #[test]
    fn currency_is_three_uppercase_letters() {
        assert_eq!(cur(" usd ").as_str(), "USD");
        for invalid in ["", "EU", "EURO", "E1R", "€€€"] {
            assert_eq!(Currency::parse(invalid), Err(DomainError::InvalidCurrency));
        }
    }

    #[test]
    fn invalid_rates_are_dropped() {
        assert_eq!(rates().len(), 2);
        assert_eq!(rates().rate(&cur("XXX")), None);
    }

    #[test]
    fn converts_to_eur() {
        assert_eq!(rates().to_eur(100.0, &Currency::eur()), 100.0);
        assert!((rates().to_eur(125.0, &cur("USD")) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn missing_rate_falls_back_to_one() {
        assert_eq!(rates().to_eur(100.0, &cur("JPY")), 100.0);
        assert_eq!(FxRates::default().to_eur(42.0, &cur("USD")), 42.0);
    }

    #[test]
    fn converts_between_currencies() {
        let r = rates();
        assert_eq!(
            r.convert(10.0, &Currency::eur(), &Currency::eur()),
            Some(10.0)
        );
        assert_eq!(r.convert(125.0, &cur("USD"), &Currency::eur()), Some(100.0));
        assert_eq!(r.convert(100.0, &Currency::eur(), &cur("USD")), Some(125.0));
        assert!((r.convert(125.0, &cur("USD"), &cur("GBP")).unwrap() - 80.0).abs() < 1e-9);
        assert_eq!(r.convert(1.0, &cur("JPY"), &Currency::eur()), None);
        assert_eq!(r.convert(1.0, &cur("JPY"), &cur("JPY")), Some(1.0));
    }
}
