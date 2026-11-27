//! Identifiants typés : impossible de passer l'identifiant d'un membre là où un compte est attendu.

use uuid::Uuid;

macro_rules! typed_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(Uuid);

        impl $name {
            pub fn generate() -> Self {
                Self(Uuid::new_v4())
            }
            pub const fn from_uuid(value: Uuid) -> Self {
                Self(value)
            }
            pub const fn as_uuid(&self) -> Uuid {
                self.0
            }
        }

        impl From<Uuid> for $name {
            fn from(value: Uuid) -> Self {
                Self(value)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

typed_id!(UserId);
typed_id!(MemberId);
typed_id!(AccountId);
typed_id!(HoldingId);
typed_id!(PropertyId);
typed_id!(LoanId);
typed_id!(GoalId);
typed_id!(TransactionId);
typed_id!(ConnectionId);
typed_id!(BudgetRuleId);
typed_id!(OperationId);
typed_id!(AlertId);
typed_id!(ValuationId);
typed_id!(AccessTokenId);
typed_id!(OAuthGrantId);
