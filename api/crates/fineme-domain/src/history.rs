//! Nettoyage de l'historique du patrimoine : détection des points suspects, corrections et plages de dates.
//!
//! Règle de détection (documentée aussi dans `docs/API.md`) : la variation d'un point est l'écart de patrimoine
//! net avec le point précédent, rapporté à une base = max(|net précédent|, actifs bruts précédents). Prendre les
//! actifs bruts dans la base évite qu'un patrimoine net proche de zéro (gros actifs, grosse dette) fasse exploser
//! le pourcentage. Un point est suspect si |variation| dépasse le seuil (20 % par défaut) **et** si l'écart absolu
//! atteint `MIN_SUSPICIOUS_CHANGE` : les petites sommes bougent vite en pourcentage sans être aberrantes.
//! Un pic isolé (100 k€, 10 k€, 100 k€) ne signale que le pic : le point qui ramène la courbe au niveau d'avant
//! le point suspect est un retour à la normale.

use chrono::NaiveDate;

use crate::error::{DomainError, finite, non_negative};
use crate::portfolio::Totals;

/// Seuil par défaut : variation de plus de 20 % d'un point à l'autre.
pub const DEFAULT_ANOMALY_THRESHOLD: f64 = 0.20;
/// Écart absolu minimal (EUR) pour qu'une variation soit jugée anormale.
pub const MIN_SUSPICIOUS_CHANGE: f64 = 100.0;
/// Seuil maximal accepté (1 000 %) : au-delà, plus rien ne serait jamais signalé.
const MAX_ANOMALY_THRESHOLD: f64 = 10.0;

/// Seuil de variation, fraction dans ]0, 10] (0.2 = 20 %).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnomalyThreshold(f64);

impl AnomalyThreshold {
    pub fn new(value: f64) -> Result<Self, DomainError> {
        let value = finite(value, "le seuil de variation")?;
        if value <= 0.0 || value > MAX_ANOMALY_THRESHOLD {
            return Err(DomainError::Rule(
                "Le seuil de variation doit être compris entre 0 (exclu) et 10 (1 000 %)",
            ));
        }
        Ok(Self(value))
    }

    pub fn value(self) -> f64 {
        self.0
    }
}

impl Default for AnomalyThreshold {
    fn default() -> Self {
        Self(DEFAULT_ANOMALY_THRESHOLD)
    }
}

/// Plage de jours, bornes incluses, `from` ≤ `to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateRange {
    from: NaiveDate,
    to: NaiveDate,
}

impl DateRange {
    pub fn new(from: NaiveDate, to: NaiveDate) -> Result<Self, DomainError> {
        if from > to {
            return Err(DomainError::Rule(
                "La date de début doit précéder ou égaler la date de fin",
            ));
        }
        Ok(Self { from, to })
    }

    pub fn day(date: NaiveDate) -> Self {
        Self {
            from: date,
            to: date,
        }
    }

    pub fn from(self) -> NaiveDate {
        self.from
    }

    pub fn to(self) -> NaiveDate {
        self.to
    }

    pub fn contains(self, date: NaiveDate) -> bool {
        self.from <= date && date <= self.to
    }
}

/// Totaux saisis à la main pour corriger un point : montants finis et positifs, net = brut − dettes.
pub fn corrected_totals(gross_assets: f64, liabilities: f64) -> Result<Totals, DomainError> {
    let gross_assets = non_negative(gross_assets, "les actifs bruts")?;
    let liabilities = non_negative(liabilities, "les dettes")?;
    Ok(Totals {
        gross_assets,
        liabilities,
        net_worth: gross_assets - liabilities,
    })
}

/// Diagnostic d'un point de l'historique.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointReview {
    /// Variation depuis le point précédent, fraction de la base ; `None` pour le premier point ou une base nulle.
    pub change: Option<f64>,
    pub suspicious: bool,
}

/// Variation relative de `previous` à `current` ; `None` si la base est nulle.
fn change(previous: &Totals, current: &Totals) -> Option<f64> {
    let base = previous.net_worth.abs().max(previous.gross_assets);
    (base > 0.0).then(|| (current.net_worth - previous.net_worth) / base)
}

fn abnormal(previous: &Totals, current: &Totals, threshold: AnomalyThreshold) -> bool {
    let large_enough = (current.net_worth - previous.net_worth).abs() >= MIN_SUSPICIOUS_CHANGE;
    large_enough && change(previous, current).is_some_and(|c| c.abs() > threshold.value())
}

/// Diagnostique une série de points, du plus ancien au plus récent (un diagnostic par point, dans l'ordre).
pub fn review_points(points: &[Totals], threshold: AnomalyThreshold) -> Vec<PointReview> {
    let mut reviews: Vec<PointReview> = Vec::with_capacity(points.len());
    for (i, current) in points.iter().enumerate() {
        let review = match i.checked_sub(1).map(|p| &points[p]) {
            None => PointReview {
                change: None,
                suspicious: false,
            },
            Some(previous) => {
                let back_to_normal = i >= 2
                    && reviews[i - 1].suspicious
                    && !abnormal(&points[i - 2], current, threshold);
                PointReview {
                    change: change(previous, current),
                    suspicious: abnormal(previous, current, threshold) && !back_to_normal,
                }
            }
        };
        reviews.push(review);
    }
    reviews
}

#[cfg(test)]
mod tests {
    use super::*;

    fn net(value: f64) -> Totals {
        Totals {
            gross_assets: value.max(0.0),
            liabilities: (-value).max(0.0),
            net_worth: value,
        }
    }

    fn suspicious(values: &[f64]) -> Vec<bool> {
        let points: Vec<Totals> = values.iter().copied().map(net).collect();
        review_points(&points, AnomalyThreshold::default())
            .iter()
            .map(|r| r.suspicious)
            .collect()
    }

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 3, day).unwrap()
    }

    #[test]
    fn steady_history_has_no_suspicious_point() {
        assert_eq!(
            suspicious(&[100_000.0, 101_000.0, 99_500.0, 105_000.0]),
            vec![false; 4]
        );
        assert!(review_points(&[], AnomalyThreshold::default()).is_empty());
    }

    #[test]
    fn first_point_has_no_change() {
        let reviews = review_points(&[net(1000.0), net(1100.0)], AnomalyThreshold::default());
        assert_eq!(reviews[0].change, None);
        assert!((reviews[1].change.unwrap() - 0.1).abs() < 1e-12);
    }

    #[test]
    fn isolated_spike_flags_only_the_spike() {
        // Chute à 10 k€ un jour (cours aberrant), puis retour à la normale.
        assert_eq!(
            suspicious(&[100_000.0, 10_000.0, 100_000.0, 100_500.0]),
            vec![false, true, false, false]
        );
        assert_eq!(
            suspicious(&[100_000.0, 400_000.0, 101_000.0]),
            vec![false, true, false]
        );
    }

    #[test]
    fn lasting_step_is_flagged_once() {
        // Vente d'un bien : la courbe change de niveau et y reste.
        assert_eq!(
            suspicious(&[300_000.0, 100_000.0, 100_000.0]),
            vec![false, true, false]
        );
    }

    #[test]
    fn threshold_is_strict_and_configurable() {
        // Exactement 20 % : pas suspect ; un peu plus : suspect.
        assert_eq!(suspicious(&[10_000.0, 12_000.0]), vec![false, false]);
        assert_eq!(suspicious(&[10_000.0, 12_001.0]), vec![false, true]);
        let loose = AnomalyThreshold::new(0.5).unwrap();
        let points = [net(10_000.0), net(13_000.0)];
        assert!(!review_points(&points, loose)[1].suspicious);
    }

    #[test]
    fn small_amounts_are_never_suspicious() {
        // +300 % mais seulement 90 € d'écart.
        assert_eq!(suspicious(&[30.0, 120.0]), vec![false, false]);
        assert_eq!(suspicious(&[30.0, 130.0]), vec![false, true]);
    }

    #[test]
    fn base_includes_gross_assets_when_net_worth_is_near_zero() {
        // 300 k€ d'actifs, 299 k€ de dette : +1 k€ de net n'est pas une variation de 100 %.
        let before = Totals {
            gross_assets: 300_000.0,
            liabilities: 299_000.0,
            net_worth: 1_000.0,
        };
        let after = Totals {
            gross_assets: 301_000.0,
            ..before
        };
        let after = Totals {
            net_worth: 2_000.0,
            ..after
        };
        let reviews = review_points(&[before, after], AnomalyThreshold::default());
        assert!(!reviews[1].suspicious);
        assert!((reviews[1].change.unwrap() - 1_000.0 / 300_000.0).abs() < 1e-12);
    }

    #[test]
    fn zero_base_gives_no_change() {
        let reviews = review_points(&[net(0.0), net(50_000.0)], AnomalyThreshold::default());
        assert_eq!(
            reviews[1],
            PointReview {
                change: None,
                suspicious: false
            }
        );
    }

    #[test]
    fn negative_net_worth_uses_its_magnitude() {
        assert_eq!(suspicious(&[-10_000.0, -10_500.0]), vec![false, false]);
        assert_eq!(suspicious(&[-10_000.0, -20_000.0]), vec![false, true]);
    }

    #[test]
    fn threshold_rejects_out_of_range_values() {
        for value in [0.0, -0.1, 10.5, f64::NAN, f64::INFINITY] {
            assert!(AnomalyThreshold::new(value).is_err(), "{value}");
        }
        assert_eq!(AnomalyThreshold::new(10.0).unwrap().value(), 10.0);
        assert_eq!(
            AnomalyThreshold::default().value(),
            DEFAULT_ANOMALY_THRESHOLD
        );
    }

    #[test]
    fn date_range_is_ordered_and_inclusive() {
        assert!(DateRange::new(date(5), date(4)).is_err());
        let range = DateRange::new(date(2), date(4)).unwrap();
        assert!(range.contains(date(2)) && range.contains(date(4)));
        assert!(!range.contains(date(1)) && !range.contains(date(5)));
        assert_eq!(
            DateRange::day(date(3)),
            DateRange::new(date(3), date(3)).unwrap()
        );
        assert_eq!((range.from(), range.to()), (date(2), date(4)));
    }

    #[test]
    fn corrected_totals_derive_net_worth_and_reject_invalid_amounts() {
        assert_eq!(
            corrected_totals(250_000.0, 100_000.0).unwrap(),
            Totals {
                gross_assets: 250_000.0,
                liabilities: 100_000.0,
                net_worth: 150_000.0
            }
        );
        assert_eq!(corrected_totals(0.0, 10.0).unwrap().net_worth, -10.0);
        assert!(corrected_totals(-1.0, 0.0).is_err());
        assert!(corrected_totals(1.0, -1.0).is_err());
        assert!(corrected_totals(f64::NAN, 0.0).is_err());
    }
}
