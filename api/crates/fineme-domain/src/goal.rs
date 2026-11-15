//! Objectifs d'épargne : épargne de précaution et projets.

use chrono::{Datelike, NaiveDate};

use crate::error::DomainError;
use crate::ids::{AccountId, GoalId, MemberId};
use crate::property::string_enum;

string_enum!(GoalKind { SafetyNet => "safety_net", Project => "project" }
    invalid = "Type d'objectif invalide (safety_net ou project)");

string_enum!(GoalStatus {
    Completed => "completed", OnTrack => "on_track", Challenging => "challenging",
    Overdue => "overdue", Underfunded => "underfunded", Funded => "funded",
} invalid = "Statut d'objectif invalide");

#[derive(Debug, Clone, PartialEq)]
pub struct Goal {
    pub id: GoalId,
    pub name: String,
    pub icon: Option<String>,
    pub kind: GoalKind,
    pub target_amount: Option<f64>,
    pub target_date: Option<NaiveDate>,
    pub monthly_contribution: Option<f64>,
    /// Hypothèse de rendement annuel en %, ex. 7.
    pub expected_return_pct: Option<f64>,
    pub monthly_expenses: Option<f64>,
    pub coverage_months: Option<f64>,
    pub account_ids: Vec<AccountId>,
    pub member_ids: Vec<MemberId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GoalProgress {
    pub current_amount: f64,
    pub target_amount: f64,
    /// Avancement en pourcentage (non plafonné à 100).
    pub pct: f64,
    pub status: GoalStatus,
    /// Rendement annuel nominal nécessaire pour atteindre la cible à la date.
    pub required_return_pct: Option<f64>,
    pub projected_amount_at_date: Option<f64>,
    /// Épargne de précaution : nombre de mois de dépenses couverts.
    pub coverage_months: Option<f64>,
}

/// Nombre de mois entiers entre deux dates (0 si `to <= from`).
pub fn months_between(from: NaiveDate, to: NaiveDate) -> u32 {
    if to <= from {
        return 0;
    }
    let mut months = (to.year() - from.year()) * 12 + to.month() as i32 - from.month() as i32;
    if to.day() < from.day() {
        months -= 1;
    }
    months.max(0) as u32
}

/// Valeur future d'un capital et de versements mensuels (fin de mois) à un taux mensuel donné.
pub fn future_value(
    current: f64,
    monthly_contribution: f64,
    monthly_rate: f64,
    months: u32,
) -> f64 {
    let n = f64::from(months);
    if monthly_rate.abs() < 1e-12 {
        return current + monthly_contribution * n;
    }
    let growth = (1.0 + monthly_rate).powf(n);
    current * growth + monthly_contribution * (growth - 1.0) / monthly_rate
}

/// Rendement annuel nominal (%) nécessaire pour atteindre `target` en `months` mois.
/// 0 si les versements suffisent sans rendement ; `None` si inatteignable sous 100 %/an.
pub fn required_return_pct(
    current: f64,
    monthly_contribution: f64,
    target: f64,
    months: u32,
) -> Option<f64> {
    if months == 0 {
        return None;
    }
    if future_value(current, monthly_contribution, 0.0, months) >= target {
        return Some(0.0);
    }
    let (mut low, mut high) = (0.0_f64, 1.0 / 12.0);
    if future_value(current, monthly_contribution, high, months) < target {
        return None;
    }
    for _ in 0..80 {
        let middle = (low + high) / 2.0;
        if future_value(current, monthly_contribution, middle, months) < target {
            low = middle;
        } else {
            high = middle;
        }
    }
    Some(high * 12.0 * 100.0)
}

impl Goal {
    /// Un projet a un montant cible positif ; une épargne de précaution, des dépenses et une couverture positives.
    pub fn check(&self) -> Result<(), DomainError> {
        if self.name.trim().is_empty() {
            return Err(DomainError::Rule("Le nom de l'objectif est requis"));
        }
        let numbers = [
            self.target_amount,
            self.monthly_contribution,
            self.expected_return_pct,
            self.monthly_expenses,
            self.coverage_months,
        ];
        if numbers.iter().flatten().any(|value| !value.is_finite()) {
            return Err(DomainError::Rule("Valeur numérique invalide"));
        }
        let positive = |value: Option<f64>| value.is_some_and(|v| v > 0.0);
        match self.kind {
            GoalKind::Project if !positive(self.target_amount) => Err(DomainError::Rule(
                "Un projet doit avoir un montant cible (target_amount) positif",
            )),
            GoalKind::SafetyNet
                if !positive(self.monthly_expenses) || !positive(self.coverage_months) =>
            {
                Err(DomainError::Rule(
                    "Une épargne de précaution doit indiquer monthly_expenses et coverage_months (positifs)",
                ))
            }
            _ => Ok(()),
        }
    }

    /// Avancement de l'objectif pour un montant déjà détenu, à la date du jour.
    pub fn progress(&self, current_amount: f64, today: NaiveDate) -> GoalProgress {
        let pct_of = |target: f64| {
            if target > 0.0 {
                current_amount / target * 100.0
            } else {
                0.0
            }
        };

        if self.kind == GoalKind::SafetyNet {
            let expenses = self.monthly_expenses.unwrap_or(0.0);
            let target = expenses * self.coverage_months.unwrap_or(0.0);
            return GoalProgress {
                current_amount,
                target_amount: target,
                pct: pct_of(target),
                status: if current_amount >= target {
                    GoalStatus::Funded
                } else {
                    GoalStatus::Underfunded
                },
                required_return_pct: None,
                projected_amount_at_date: None,
                coverage_months: (expenses > 0.0).then(|| current_amount / expenses),
            };
        }

        let target = self.target_amount.unwrap_or(0.0);
        let contribution = self.monthly_contribution.unwrap_or(0.0);
        let expected_monthly = self.expected_return_pct.unwrap_or(0.0) / 100.0 / 12.0;
        let months = self.target_date.map(|date| months_between(today, date));
        let projected =
            months.map(|m| future_value(current_amount, contribution, expected_monthly, m));
        let completed = current_amount >= target;
        let required = match months {
            Some(m) if !completed => required_return_pct(current_amount, contribution, target, m),
            _ => None,
        };
        let status = if completed {
            GoalStatus::Completed
        } else {
            match (self.target_date, projected) {
                (Some(date), _) if date <= today => GoalStatus::Overdue,
                (Some(_), Some(p)) if p < target => GoalStatus::Challenging,
                _ => GoalStatus::OnTrack,
            }
        };
        GoalProgress {
            current_amount,
            target_amount: target,
            pct: pct_of(target),
            status,
            required_return_pct: required,
            projected_amount_at_date: projected,
            coverage_months: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn close(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() < eps
    }

    fn goal(kind: GoalKind) -> Goal {
        Goal {
            id: GoalId::generate(),
            name: "Objectif".into(),
            icon: None,
            kind,
            target_amount: None,
            target_date: None,
            monthly_contribution: None,
            expected_return_pct: None,
            monthly_expenses: None,
            coverage_months: None,
            account_ids: vec![],
            member_ids: vec![],
        }
    }

    fn project() -> Goal {
        Goal {
            target_amount: Some(20_000.0),
            target_date: Some(d(2028, 1, 1)),
            monthly_contribution: Some(500.0),
            expected_return_pct: Some(5.0),
            ..goal(GoalKind::Project)
        }
    }

    fn safety_net() -> Goal {
        Goal {
            monthly_expenses: Some(2000.0),
            coverage_months: Some(6.0),
            ..goal(GoalKind::SafetyNet)
        }
    }

    const TODAY: (i32, u32, u32) = (2026, 1, 1);
    fn today() -> NaiveDate {
        d(TODAY.0, TODAY.1, TODAY.2)
    }

    #[test]
    fn months_between_dates() {
        assert_eq!(months_between(d(2026, 1, 15), d(2027, 1, 15)), 12);
        assert_eq!(months_between(d(2026, 1, 15), d(2027, 1, 14)), 11);
        assert_eq!(months_between(d(2026, 1, 15), d(2025, 1, 1)), 0);
    }

    #[test]
    fn future_value_with_and_without_return() {
        assert!(close(future_value(1000.0, 100.0, 0.0, 12), 2200.0, 1e-9));
        assert!(close(
            future_value(1000.0, 0.0, 0.01, 12),
            1000.0 * 1.01_f64.powi(12),
            1e-9
        ));
    }

    #[test]
    fn required_return_cases() {
        assert_eq!(required_return_pct(1000.0, 100.0, 2000.0, 12), Some(0.0));
        let r = required_return_pct(10_000.0, 200.0, 40_000.0, 96).unwrap();
        assert!(close(
            future_value(10_000.0, 200.0, r / 100.0 / 12.0, 96),
            40_000.0,
            0.01
        ));
        assert_eq!(required_return_pct(0.0, 0.0, 1000.0, 12), None);
        assert_eq!(required_return_pct(100.0, 0.0, 1_000_000.0, 12), None);
        assert_eq!(required_return_pct(100.0, 10.0, 1000.0, 0), None);
    }

    #[test]
    fn check_rules_per_kind() {
        assert!(project().check().is_ok());
        assert!(safety_net().check().is_ok());
        assert!(goal(GoalKind::Project).check().is_err());
        assert!(
            Goal {
                target_amount: Some(0.0),
                ..project()
            }
            .check()
            .is_err()
        );
        assert!(
            Goal {
                coverage_months: None,
                ..safety_net()
            }
            .check()
            .is_err()
        );
        assert!(
            Goal {
                name: " ".into(),
                ..project()
            }
            .check()
            .is_err()
        );
        assert!(
            Goal {
                monthly_contribution: Some(f64::INFINITY),
                ..project()
            }
            .check()
            .is_err()
        );
    }

    #[test]
    fn safety_net_underfunded_then_funded() {
        let p = safety_net().progress(9000.0, today());
        assert_eq!(p.target_amount, 12_000.0);
        assert_eq!(p.status, GoalStatus::Underfunded);
        assert!(close(p.pct, 75.0, 1e-9));
        assert_eq!(p.coverage_months, Some(4.5));
        assert_eq!(
            safety_net().progress(12_000.0, today()).status,
            GoalStatus::Funded
        );
        assert!(safety_net().progress(18_000.0, today()).pct > 100.0);
    }

    #[test]
    fn project_on_track() {
        let p = project().progress(10_000.0, today());
        assert_eq!(p.status, GoalStatus::OnTrack);
        assert_eq!(p.required_return_pct, Some(0.0));
        assert!(p.projected_amount_at_date.unwrap() > 22_000.0);
        assert_eq!(p.coverage_months, None);
    }

    #[test]
    fn project_challenging_when_projection_falls_short() {
        let hard = Goal {
            monthly_contribution: Some(100.0),
            expected_return_pct: Some(2.0),
            ..project()
        };
        let p = hard.progress(10_000.0, today());
        assert_eq!(p.status, GoalStatus::Challenging);
        assert!(p.required_return_pct.unwrap() > 2.0);
    }

    #[test]
    fn project_completed_and_overdue() {
        let done = project().progress(25_000.0, today());
        assert_eq!(done.status, GoalStatus::Completed);
        assert_eq!(done.required_return_pct, None);
        let late = Goal {
            target_date: Some(d(2025, 6, 1)),
            ..project()
        };
        assert_eq!(late.progress(10_000.0, today()).status, GoalStatus::Overdue);
        let due_today = Goal {
            target_date: Some(today()),
            ..project()
        };
        assert_eq!(
            due_today.progress(10_000.0, today()).status,
            GoalStatus::Overdue
        );
    }

    #[test]
    fn project_without_date_is_on_track_with_no_projection() {
        let undated = Goal {
            target_date: None,
            ..project()
        };
        let p = undated.progress(10_000.0, today());
        assert_eq!(p.status, GoalStatus::OnTrack);
        assert_eq!(p.projected_amount_at_date, None);
        assert_eq!(p.required_return_pct, None);
    }
}
