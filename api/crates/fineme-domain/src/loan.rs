//! Prêts : amortissement à mensualités constantes.

use chrono::{Months, NaiveDate};

use crate::error::{DomainError, non_negative};
use crate::ids::{AccountId, LoanId};

pub const MAX_DURATION_MONTHS: u32 = 1200;

/// Taux annuel en pourcentage (ex. 4.0), fini et positif ou nul.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct AnnualRatePct(f64);

impl AnnualRatePct {
    pub const ZERO: AnnualRatePct = AnnualRatePct(0.0);

    pub fn new(value: f64, field: &'static str) -> Result<Self, DomainError> {
        non_negative(value, field).map(Self)
    }

    pub const fn value(&self) -> f64 {
        self.0
    }

    /// Taux mensuel en fraction (annuel / 12).
    pub fn monthly_fraction(&self) -> f64 {
        self.0 / 100.0 / 12.0
    }
}

/// Conditions d'un prêt. Invariants : capital > 0, taux ≥ 0, durée de 1 à 1200 mois.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoanTerms {
    principal: f64,
    interest_rate: AnnualRatePct,
    insurance_rate: AnnualRatePct,
    duration_months: u32,
    start_date: NaiveDate,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleRow {
    pub month: u32,
    pub date: NaiveDate,
    /// Mensualité hors assurance.
    pub payment: f64,
    pub interest: f64,
    pub principal: f64,
    pub insurance: f64,
    pub remaining: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoanSummary {
    pub monthly_payment: f64,
    pub monthly_insurance: f64,
    pub remaining_capital: f64,
    pub end_date: NaiveDate,
    pub total_interest: f64,
    pub total_insurance: f64,
}

impl LoanTerms {
    pub fn new(
        principal: f64,
        interest_rate_pct: f64,
        insurance_rate_pct: f64,
        duration_months: i64,
        start_date: NaiveDate,
    ) -> Result<Self, DomainError> {
        if !principal.is_finite() {
            return Err(DomainError::NotFinite("principal"));
        }
        if principal <= 0.0 {
            return Err(DomainError::Rule("Le capital emprunté doit être positif"));
        }
        let interest_rate = AnnualRatePct::new(interest_rate_pct, "interest_rate_pct")?;
        let insurance_rate = AnnualRatePct::new(insurance_rate_pct, "insurance_rate_pct")?;
        if !(1..=i64::from(MAX_DURATION_MONTHS)).contains(&duration_months) {
            return Err(DomainError::Rule(
                "La durée du prêt doit être comprise entre 1 et 1200 mois",
            ));
        }
        Ok(Self {
            principal,
            interest_rate,
            insurance_rate,
            duration_months: duration_months as u32,
            start_date,
        })
    }

    pub fn principal(&self) -> f64 {
        self.principal
    }
    pub fn interest_rate(&self) -> AnnualRatePct {
        self.interest_rate
    }
    pub fn insurance_rate(&self) -> AnnualRatePct {
        self.insurance_rate
    }
    pub fn duration_months(&self) -> u32 {
        self.duration_months
    }
    pub fn start_date(&self) -> NaiveDate {
        self.start_date
    }

    /// Mensualité constante hors assurance.
    pub fn monthly_payment(&self) -> f64 {
        let n = f64::from(self.duration_months);
        let r = self.interest_rate.monthly_fraction();
        if r.abs() < 1e-12 {
            self.principal / n
        } else {
            self.principal * r / (1.0 - (1.0 + r).powf(-n))
        }
    }

    /// Assurance mensuelle, calculée sur le capital initial.
    pub fn monthly_insurance(&self) -> f64 {
        self.principal * self.insurance_rate.monthly_fraction()
    }

    /// Date de la k-ième échéance (la première tombe un mois après `start_date`).
    pub fn payment_date(&self, k: u32) -> NaiveDate {
        self.start_date
            .checked_add_months(Months::new(k))
            .unwrap_or(self.start_date)
    }

    pub fn end_date(&self) -> NaiveDate {
        self.payment_date(self.duration_months)
    }

    /// Tableau d'amortissement complet.
    pub fn schedule(&self) -> Vec<ScheduleRow> {
        let r = self.interest_rate.monthly_fraction();
        let payment = self.monthly_payment();
        let insurance = self.monthly_insurance();
        let mut remaining = self.principal;
        let mut rows = Vec::with_capacity(self.duration_months as usize);
        for k in 1..=self.duration_months {
            let interest = remaining * r;
            // La dernière échéance solde exactement le capital (arrondis flottants).
            let principal = if k == self.duration_months {
                remaining
            } else {
                (payment - interest).min(remaining)
            };
            remaining = (remaining - principal).max(0.0);
            rows.push(ScheduleRow {
                month: k,
                date: self.payment_date(k),
                payment: principal + interest,
                interest,
                principal,
                insurance,
                remaining,
            });
        }
        rows
    }

    /// Capital restant dû à une date : `principal` avant la première échéance, 0 après la dernière.
    pub fn remaining_capital(&self, today: NaiveDate) -> f64 {
        self.summary(today).remaining_capital
    }

    pub fn summary(&self, today: NaiveDate) -> LoanSummary {
        let schedule = self.schedule();
        LoanSummary {
            monthly_payment: self.monthly_payment(),
            monthly_insurance: self.monthly_insurance(),
            remaining_capital: schedule
                .iter()
                .take_while(|row| row.date <= today)
                .last()
                .map_or(self.principal, |row| row.remaining),
            end_date: self.end_date(),
            total_interest: schedule.iter().map(|row| row.interest).sum(),
            total_insurance: self.monthly_insurance() * f64::from(self.duration_months),
        }
    }
}

/// Prêt porté par un compte de type `loan`, éventuellement lié au compte immobilier qu'il finance.
#[derive(Debug, Clone, PartialEq)]
pub struct Loan {
    pub id: LoanId,
    pub property_account_id: Option<AccountId>,
    pub terms: LoanTerms,
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

    fn loan() -> LoanTerms {
        LoanTerms::new(200_000.0, 4.0, 0.3, 240, d(2024, 1, 15)).unwrap()
    }

    #[test]
    fn terms_reject_invalid_values() {
        let start = d(2024, 1, 1);
        assert!(LoanTerms::new(0.0, 4.0, 0.0, 12, start).is_err());
        assert!(LoanTerms::new(-5.0, 4.0, 0.0, 12, start).is_err());
        assert_eq!(
            LoanTerms::new(f64::NAN, 4.0, 0.0, 12, start),
            Err(DomainError::NotFinite("principal"))
        );
        assert_eq!(
            LoanTerms::new(1000.0, -1.0, 0.0, 12, start),
            Err(DomainError::Negative("interest_rate_pct"))
        );
        assert_eq!(
            LoanTerms::new(1000.0, 1.0, -0.1, 12, start),
            Err(DomainError::Negative("insurance_rate_pct"))
        );
        assert!(LoanTerms::new(1000.0, 1.0, 0.0, 0, start).is_err());
        assert!(LoanTerms::new(1000.0, 1.0, 0.0, 1201, start).is_err());
        assert!(LoanTerms::new(1000.0, 0.0, 0.0, 1200, start).is_ok());
    }

    #[test]
    fn monthly_payment_matches_standard_formula() {
        // 200 000 € sur 20 ans à 4 % : mensualité de référence 1 211,96 €.
        assert!(close(loan().monthly_payment(), 1211.96, 0.01));
        assert!(close(loan().monthly_insurance(), 50.0, 1e-9));
    }

    #[test]
    fn zero_rate_loan_is_linear() {
        let l = LoanTerms::new(12_000.0, 0.0, 0.0, 12, d(2024, 1, 15)).unwrap();
        assert!(close(l.monthly_payment(), 1000.0, 1e-9));
        assert!(close(l.schedule()[5].remaining, 6000.0, 1e-6));
        assert!(close(l.summary(d(2030, 1, 1)).total_interest, 0.0, 1e-9));
    }

    #[test]
    fn schedule_amortizes_to_zero() {
        let s = loan().schedule();
        assert_eq!(s.len(), 240);
        assert_eq!(s[0].date, d(2024, 2, 15));
        assert!(close(s[0].interest, 200_000.0 * 0.04 / 12.0, 1e-6));
        assert_eq!(s[239].remaining, 0.0);
        assert_eq!(s[239].date, d(2044, 1, 15));
        assert!(close(
            s.iter().map(|r| r.principal).sum::<f64>(),
            200_000.0,
            1e-4
        ));
        assert!(s.windows(2).all(|w| w[1].remaining < w[0].remaining));
        assert!(
            s.iter()
                .all(|r| close(r.payment, loan().monthly_payment(), 1e-6))
        );
    }

    #[test]
    fn before_start_date_the_whole_principal_is_due() {
        assert_eq!(loan().remaining_capital(d(2023, 6, 1)), 200_000.0);
        // Entre le déblocage et la première échéance, rien n'est encore remboursé.
        assert_eq!(loan().remaining_capital(d(2024, 2, 14)), 200_000.0);
    }

    #[test]
    fn remaining_capital_decreases_with_each_payment() {
        let l = loan();
        let after_one = l.remaining_capital(d(2024, 2, 15));
        assert!(close(
            after_one,
            200_000.0 - (l.monthly_payment() - 200_000.0 * 0.04 / 12.0),
            1e-6
        ));
        assert!(l.remaining_capital(d(2034, 1, 15)) < after_one);
    }

    #[test]
    fn after_end_date_nothing_is_due() {
        assert_eq!(loan().remaining_capital(d(2044, 1, 15)), 0.0);
        assert_eq!(loan().remaining_capital(d(2050, 1, 1)), 0.0);
    }

    #[test]
    fn summary_totals() {
        let l = loan();
        let sum = l.summary(d(2024, 2, 15));
        assert_eq!(sum.end_date, d(2044, 1, 15));
        assert!(close(sum.total_insurance, 12_000.0, 1e-6));
        assert!(close(
            sum.total_interest,
            l.monthly_payment() * 240.0 - 200_000.0,
            0.01
        ));
    }

    #[test]
    fn end_of_month_start_dates_clamp() {
        let l = LoanTerms::new(1200.0, 0.0, 0.0, 2, d(2024, 1, 31)).unwrap();
        assert_eq!(l.payment_date(1), d(2024, 2, 29));
        assert_eq!(l.end_date(), d(2024, 3, 31));
    }
}
