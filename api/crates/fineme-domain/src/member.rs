use crate::ids::{MemberId, UserId};

/// Utilisateur unique de l'application (le login).
#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub id: UserId,
    pub email: String,
    pub name: String,
}

/// Membre du foyer, détenteur de parts dans les comptes.
#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    pub id: MemberId,
    pub name: String,
    pub color: String,
    pub is_primary: bool,
}

/// Couleurs attribuées par défaut aux membres, dans l'ordre de création.
pub const MEMBER_PALETTE: [&str; 6] = [
    "#6366f1", "#ec4899", "#10b981", "#f59e0b", "#06b6d4", "#8b5cf6",
];

pub fn default_color(index: usize) -> &'static str {
    MEMBER_PALETTE[index % MEMBER_PALETTE.len()]
}
