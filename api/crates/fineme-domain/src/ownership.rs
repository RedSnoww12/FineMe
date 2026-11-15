//! Parts de détention d'un compte et vues (foyer / membre).

use crate::error::DomainError;
use crate::ids::MemberId;

/// Fraction de détention dans ]0, 1].
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Share(f64);

impl Share {
    pub const FULL: Share = Share(1.0);

    pub fn new(value: f64) -> Result<Self, DomainError> {
        if value.is_finite() && value > 0.0 && value <= 1.0 {
            Ok(Self(value))
        } else {
            Err(DomainError::InvalidShare)
        }
    }

    pub const fn value(&self) -> f64 {
        self.0
    }
}

/// Vue demandée : tout le foyer (somme des parts) ou la part d'un seul membre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Household,
    Member(MemberId),
}

impl View {
    pub fn member(&self) -> Option<MemberId> {
        match self {
            View::Household => None,
            View::Member(id) => Some(*id),
        }
    }
}

/// Répartition d'un compte entre ses détenteurs : au moins un, sans doublon, total ≤ 100 %.
/// Le total peut être inférieur à 100 % (part détenue hors foyer).
#[derive(Debug, Clone, PartialEq)]
pub struct Ownership(Vec<(MemberId, Share)>);

impl Ownership {
    pub fn new(owners: Vec<(MemberId, Share)>) -> Result<Self, DomainError> {
        if owners.is_empty() {
            return Err(DomainError::NoOwner);
        }
        for (index, (member, _)) in owners.iter().enumerate() {
            if owners[..index].iter().any(|(other, _)| other == member) {
                return Err(DomainError::DuplicateOwner);
            }
        }
        if owners.iter().map(|(_, share)| share.value()).sum::<f64>() > 1.0 + 1e-6 {
            return Err(DomainError::SharesExceedWhole);
        }
        Ok(Self(owners))
    }

    /// Construit la répartition à partir de fractions brutes.
    pub fn from_fractions(
        owners: impl IntoIterator<Item = (MemberId, f64)>,
    ) -> Result<Self, DomainError> {
        let shares = owners
            .into_iter()
            .map(|(member, share)| Ok((member, Share::new(share)?)))
            .collect::<Result<_, DomainError>>()?;
        Self::new(shares)
    }

    pub fn sole(member: MemberId) -> Self {
        Self(vec![(member, Share::FULL)])
    }

    pub fn owners(&self) -> &[(MemberId, Share)] {
        &self.0
    }

    pub fn includes(&self, member: MemberId) -> bool {
        self.0.iter().any(|(id, _)| *id == member)
    }

    /// Fraction concernée par la vue : part du membre, ou somme de toutes les parts pour le foyer.
    pub fn fraction(&self, view: View) -> f64 {
        let fraction: f64 = match view {
            View::Household => self.0.iter().map(|(_, share)| share.value()).sum(),
            View::Member(member) => self
                .0
                .iter()
                .filter(|(id, _)| *id == member)
                .map(|(_, share)| share.value())
                .sum(),
        };
        fraction + 0.0 // une somme vide vaut -0.0 : on normalise en 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_must_be_in_open_closed_unit_interval() {
        assert!(Share::new(0.5).is_ok());
        assert!(Share::new(1.0).is_ok());
        for invalid in [0.0, -0.1, 1.000_1, f64::NAN, f64::INFINITY] {
            assert_eq!(Share::new(invalid), Err(DomainError::InvalidShare));
        }
    }

    #[test]
    fn ownership_rejects_invalid_distributions() {
        let (a, b) = (MemberId::generate(), MemberId::generate());
        assert!(Ownership::from_fractions([(a, 0.5), (b, 0.5)]).is_ok());
        assert_eq!(Ownership::from_fractions([]), Err(DomainError::NoOwner));
        assert_eq!(
            Ownership::from_fractions([(a, 0.7), (b, 0.5)]),
            Err(DomainError::SharesExceedWhole)
        );
        assert_eq!(
            Ownership::from_fractions([(a, 0.5), (a, 0.5)]),
            Err(DomainError::DuplicateOwner)
        );
        assert_eq!(
            Ownership::from_fractions([(a, 0.0)]),
            Err(DomainError::InvalidShare)
        );
        assert_eq!(
            Ownership::from_fractions([(a, 1.2)]),
            Err(DomainError::InvalidShare)
        );
    }

    #[test]
    fn shares_may_sum_below_whole() {
        let (a, b) = (MemberId::generate(), MemberId::generate());
        let ownership = Ownership::from_fractions([(a, 0.5), (b, 0.3)]).unwrap();
        assert!((ownership.fraction(View::Household) - 0.8).abs() < 1e-12);
        assert!((ownership.fraction(View::Member(b)) - 0.3).abs() < 1e-12);
    }

    #[test]
    fn member_without_share_gets_positive_zero() {
        let ownership = Ownership::sole(MemberId::generate());
        let fraction = ownership.fraction(View::Member(MemberId::generate()));
        assert_eq!(fraction, 0.0);
        assert!(fraction.is_sign_positive());
        assert!(ownership.fraction(View::Household) == 1.0);
    }
}
