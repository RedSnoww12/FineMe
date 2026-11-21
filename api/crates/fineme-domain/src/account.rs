use chrono::{DateTime, NaiveDate, Utc};

use crate::error::{DomainError, finite};
use crate::fx::Currency;
use crate::ids::{AccountId, ConnectionId};
use crate::loan::Loan;
use crate::ownership::Ownership;
use crate::property::{Property, string_enum};

string_enum!(AccountKind {
    Checking => "checking", Savings => "savings", Investment => "investment", Crypto => "crypto",
    RealEstate => "real_estate", Loan => "loan", Other => "other",
} invalid = "Type de compte invalide (checking, savings, investment, crypto, real_estate, loan, other)");

impl AccountKind {
    /// Un prêt est une dette : valeur positive, soustraite du patrimoine net.
    pub fn is_liability(&self) -> bool {
        matches!(self, AccountKind::Loan)
    }

    /// Épargne, titres et crypto (hors comptes courants, immobilier, dettes, autres).
    pub fn is_financial(&self) -> bool {
        matches!(
            self,
            AccountKind::Savings | AccountKind::Investment | AccountKind::Crypto
        )
    }

    /// Les comptes immobiliers et les prêts ne portent pas de positions.
    pub fn can_hold_positions(&self) -> bool {
        !matches!(self, AccountKind::RealEstate | AccountKind::Loan)
    }
}

string_enum!(AccountSource { Manual => "manual", Csv => "csv", Aggregator => "aggregator", Import => "import" }
    invalid = "Origine de compte invalide");

/// Compte du patrimoine avec ses détenteurs et, selon son type, son bien ou son prêt.
#[derive(Debug, Clone, PartialEq)]
pub struct Account {
    pub id: AccountId,
    pub name: String,
    pub institution: Option<String>,
    pub kind: AccountKind,
    /// Enveloppe fiscale, libre (pea, cto, assurance_vie, livret…).
    pub envelope: Option<String>,
    pub currency: Currency,
    /// Solde espèces dans la devise du compte.
    pub cash_balance: f64,
    pub annual_yield_pct: Option<f64>,
    pub source: AccountSource,
    pub external_id: Option<String>,
    pub connection_id: Option<ConnectionId>,
    pub opened_at: Option<NaiveDate>,
    pub updated_at: DateTime<Utc>,
    pub owners: Ownership,
    pub property: Option<Property>,
    pub loan: Option<Loan>,
}

impl Account {
    /// Cohérence de l'agrégat : nom, montants, et bien/prêt présents si et seulement si le type l'exige.
    pub fn check(&self) -> Result<(), DomainError> {
        if self.name.trim().is_empty() {
            return Err(DomainError::Rule("Le nom du compte est requis"));
        }
        finite(self.cash_balance, "cash_balance")?;
        if let Some(yield_pct) = self.annual_yield_pct {
            finite(yield_pct, "annual_yield_pct")?;
        }
        match (self.kind, &self.property) {
            (AccountKind::RealEstate, None) => {
                return Err(DomainError::Rule(
                    "property est requis pour un compte immobilier",
                ));
            }
            (AccountKind::RealEstate, Some(property)) => property.check()?,
            (_, Some(_)) => {
                return Err(DomainError::Rule(
                    "property n'est accepté que pour un compte immobilier",
                ));
            }
            (_, None) => {}
        }
        match (self.kind, &self.loan) {
            (AccountKind::Loan, None) => {
                Err(DomainError::Rule("loan est requis pour un compte de prêt"))
            }
            (AccountKind::Loan, Some(_)) | (_, None) => Ok(()),
            (_, Some(_)) => Err(DomainError::Rule(
                "loan n'est accepté que pour un compte de prêt",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{LoanId, MemberId, PropertyId};
    use crate::loan::LoanTerms;
    use crate::property::{BuildingType, PropertyCategory};

    pub fn account(kind: AccountKind) -> Account {
        Account {
            id: AccountId::generate(),
            name: "Compte".into(),
            institution: None,
            kind,
            envelope: None,
            currency: Currency::eur(),
            cash_balance: 0.0,
            annual_yield_pct: None,
            source: AccountSource::Manual,
            external_id: None,
            connection_id: None,
            opened_at: None,
            updated_at: DateTime::<Utc>::UNIX_EPOCH,
            owners: Ownership::sole(MemberId::generate()),
            property: None,
            loan: None,
        }
    }

    fn property() -> Property {
        Property {
            id: PropertyId::generate(),
            address: None,
            category: PropertyCategory::Primary,
            building_type: BuildingType::House,
            surface_m2: None,
            purchase_price: 100.0,
            purchase_fees: 0.0,
            purchased_at: None,
            current_value: 100.0,
            valuations: Vec::new(),
            monthly_rent: 0.0,
            monthly_charges: 0.0,
            yearly_property_tax: 0.0,
        }
    }

    fn loan() -> Loan {
        let start = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        Loan {
            id: LoanId::generate(),
            property_account_id: None,
            terms: LoanTerms::new(1000.0, 1.0, 0.0, 12, start).unwrap(),
        }
    }

    #[test]
    fn kinds_classification() {
        assert!(AccountKind::Loan.is_liability());
        assert!(!AccountKind::RealEstate.is_liability());
        assert!(AccountKind::Savings.is_financial() && AccountKind::Crypto.is_financial());
        assert!(!AccountKind::Checking.is_financial() && !AccountKind::Other.is_financial());
        assert!(
            !AccountKind::RealEstate.can_hold_positions()
                && !AccountKind::Loan.can_hold_positions()
        );
        assert_eq!(
            AccountKind::parse("real_estate"),
            Ok(AccountKind::RealEstate)
        );
        assert!(AccountKind::parse("bogus").is_err());
    }

    #[test]
    fn plain_account_is_consistent() {
        assert!(account(AccountKind::Checking).check().is_ok());
        assert!(
            Account {
                name: "  ".into(),
                ..account(AccountKind::Checking)
            }
            .check()
            .is_err()
        );
        assert!(
            Account {
                cash_balance: f64::NAN,
                ..account(AccountKind::Checking)
            }
            .check()
            .is_err()
        );
    }

    #[test]
    fn real_estate_requires_a_property_and_only_real_estate_has_one() {
        assert!(account(AccountKind::RealEstate).check().is_err());
        assert!(
            Account {
                property: Some(property()),
                ..account(AccountKind::RealEstate)
            }
            .check()
            .is_ok()
        );
        assert!(
            Account {
                property: Some(property()),
                ..account(AccountKind::Savings)
            }
            .check()
            .is_err()
        );
    }

    #[test]
    fn loan_account_requires_a_loan_and_only_loan_accounts_have_one() {
        assert!(account(AccountKind::Loan).check().is_err());
        assert!(
            Account {
                loan: Some(loan()),
                ..account(AccountKind::Loan)
            }
            .check()
            .is_ok()
        );
        assert!(
            Account {
                loan: Some(loan()),
                ..account(AccountKind::Checking)
            }
            .check()
            .is_err()
        );
    }
}
