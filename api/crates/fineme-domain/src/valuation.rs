//! Estimations datées de la valeur d'un bien immobilier.

use chrono::{DateTime, NaiveDate, Utc};

use crate::error::{DomainError, finite};
use crate::ids::ValuationId;
use crate::property::string_enum;

string_enum!(
    /// Origine d'une estimation.
    ValuationSource { Manual => "manual", PricePerM2 => "price_per_m2", External => "external" }
    invalid = "Source d'estimation invalide (manual, price_per_m2, external)");

/// Valeur estimée d'un bien : finie et strictement positive.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct EstimatedValue(f64);

impl EstimatedValue {
    pub fn new(value: f64) -> Result<Self, DomainError> {
        if finite(value, "value")? <= 0.0 {
            return Err(DomainError::NotPositive("value"));
        }
        Ok(Self(value))
    }

    pub const fn value(&self) -> f64 {
        self.0
    }
}

/// Date d'une estimation : jamais postérieure au jour de la saisie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ValuationDate(NaiveDate);

impl ValuationDate {
    pub fn new(date: NaiveDate, today: NaiveDate) -> Result<Self, DomainError> {
        if date > today {
            return Err(DomainError::Rule(
                "La date d'une estimation ne peut pas être dans le futur",
            ));
        }
        Ok(Self(date))
    }

    /// Date relue depuis le stockage, déjà validée lors de sa saisie.
    pub const fn restore(date: NaiveDate) -> Self {
        Self(date)
    }

    pub const fn date(&self) -> NaiveDate {
        self.0
    }
}

/// Valeur d'un bien par le prix au m² : surface × prix, tous deux strictement positifs.
pub fn value_from_price_per_m2(
    surface_m2: Option<f64>,
    price_per_m2: f64,
) -> Result<EstimatedValue, DomainError> {
    let surface = surface_m2.ok_or(DomainError::Rule(
        "La surface du bien est requise pour une estimation au prix du m²",
    ))?;
    if finite(surface, "surface_m2")? <= 0.0 {
        return Err(DomainError::NotPositive("surface_m2"));
    }
    if finite(price_per_m2, "price_per_m2")? <= 0.0 {
        return Err(DomainError::NotPositive("price_per_m2"));
    }
    EstimatedValue::new(surface * price_per_m2)
}

/// Estimation de la valeur d'un bien à une date.
#[derive(Debug, Clone, PartialEq)]
pub struct Valuation {
    pub id: ValuationId,
    pub valued_on: ValuationDate,
    pub value: EstimatedValue,
    pub source: ValuationSource,
    /// Prix au m² retenu, pour une estimation `price_per_m2`.
    pub price_per_m2: Option<f64>,
    pub note: Option<String>,
    /// Instant de la saisie : départage deux estimations du même jour.
    pub recorded_at: DateTime<Utc>,
}

impl Valuation {
    /// Estimation saisie à la main, datée du jour.
    pub fn manual_today(value: EstimatedValue, now: DateTime<Utc>) -> Self {
        Self {
            id: ValuationId::generate(),
            valued_on: ValuationDate::restore(now.date_naive()),
            value,
            source: ValuationSource::Manual,
            price_per_m2: None,
            note: None,
            recorded_at: now,
        }
    }

    fn order_key(&self) -> (NaiveDate, DateTime<Utc>) {
        (self.valued_on.date(), self.recorded_at)
    }
}

/// Estimation en vigueur à `today` : la plus récente datée au plus tard ce jour
/// (à date égale, la dernière saisie).
pub fn current_valuation(valuations: &[Valuation], today: NaiveDate) -> Option<&Valuation> {
    valuations
        .iter()
        .filter(|valuation| valuation.valued_on.date() <= today)
        .max_by_key(|valuation| valuation.order_key())
}

/// Trie les estimations de la plus récente à la plus ancienne.
pub fn newest_first(valuations: &mut [Valuation]) {
    valuations.sort_by_key(|valuation| std::cmp::Reverse(valuation.order_key()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn valuation(date: NaiveDate, value: f64, hour: u32) -> Valuation {
        Valuation {
            id: ValuationId::generate(),
            valued_on: ValuationDate::restore(date),
            value: EstimatedValue::new(value).unwrap(),
            source: ValuationSource::Manual,
            price_per_m2: None,
            note: None,
            recorded_at: Utc.with_ymd_and_hms(2026, 1, 1, hour, 0, 0).unwrap(),
        }
    }

    #[test]
    fn value_must_be_strictly_positive_and_finite() {
        assert_eq!(EstimatedValue::new(250_000.0).unwrap().value(), 250_000.0);
        assert_eq!(
            EstimatedValue::new(0.0),
            Err(DomainError::NotPositive("value"))
        );
        assert_eq!(
            EstimatedValue::new(-1.0),
            Err(DomainError::NotPositive("value"))
        );
        assert_eq!(
            EstimatedValue::new(f64::INFINITY),
            Err(DomainError::NotFinite("value"))
        );
    }

    #[test]
    fn date_cannot_be_in_the_future() {
        let today = day(2026, 6, 15);
        assert!(ValuationDate::new(today, today).is_ok());
        assert!(ValuationDate::new(day(2020, 1, 1), today).is_ok());
        assert!(ValuationDate::new(day(2026, 6, 16), today).is_err());
    }

    #[test]
    fn price_per_m2_multiplies_surface() {
        let value = value_from_price_per_m2(Some(42.5), 4000.0).unwrap();
        assert!((value.value() - 170_000.0).abs() < 1e-9);
        assert!(value_from_price_per_m2(None, 4000.0).is_err());
        assert_eq!(
            value_from_price_per_m2(Some(0.0), 4000.0),
            Err(DomainError::NotPositive("surface_m2"))
        );
        assert_eq!(
            value_from_price_per_m2(Some(40.0), -1.0),
            Err(DomainError::NotPositive("price_per_m2"))
        );
        assert_eq!(
            value_from_price_per_m2(Some(40.0), f64::NAN),
            Err(DomainError::NotFinite("price_per_m2"))
        );
    }

    #[test]
    fn current_is_latest_up_to_today() {
        let today = day(2026, 6, 15);
        let valuations = vec![
            valuation(day(2024, 1, 1), 200_000.0, 1),
            valuation(day(2026, 3, 1), 230_000.0, 1),
            valuation(day(2025, 1, 1), 210_000.0, 1),
        ];
        assert_eq!(
            current_valuation(&valuations, today).unwrap().value.value(),
            230_000.0
        );
        // Vue à une date antérieure : l'estimation postérieure est ignorée.
        assert_eq!(
            current_valuation(&valuations, day(2025, 6, 1))
                .unwrap()
                .value
                .value(),
            210_000.0
        );
        assert!(current_valuation(&valuations, day(2023, 1, 1)).is_none());
        assert!(current_valuation(&[], today).is_none());
    }

    #[test]
    fn same_day_keeps_last_recorded() {
        let today = day(2026, 6, 15);
        let valuations = vec![
            valuation(today, 300_000.0, 9),
            valuation(today, 305_000.0, 10),
        ];
        assert_eq!(
            current_valuation(&valuations, today).unwrap().value.value(),
            305_000.0
        );
        let mut sorted = valuations.clone();
        newest_first(&mut sorted);
        assert_eq!(sorted[0].value.value(), 305_000.0);
    }

    #[test]
    fn manual_today_is_dated_from_now() {
        let now = Utc.with_ymd_and_hms(2026, 6, 15, 8, 0, 0).unwrap();
        let v = Valuation::manual_today(EstimatedValue::new(1.0).unwrap(), now);
        assert_eq!(v.valued_on.date(), day(2026, 6, 15));
        assert_eq!(v.source, ValuationSource::Manual);
    }
}
