//! Journal des connexions : chaque tentative de connexion ou de réinitialisation, réussie ou non.

use chrono::{DateTime, Utc};

/// Longueur maximale conservée d'un user-agent (au-delà, il est tronqué).
pub const MAX_USER_AGENT_CHARS: usize = 200;
/// Longueur maximale conservée d'une adresse IP (une IPv6 complète tient largement).
const MAX_IP_CHARS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginAction {
    Login,
    /// Seconde étape d'une connexion avec second facteur (code TOTP ou de secours).
    Totp,
    Reset,
}

impl LoginAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::Totp => "totp",
            Self::Reset => "reset",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "login" => Some(Self::Login),
            "totp" => Some(Self::Totp),
            "reset" => Some(Self::Reset),
            _ => None,
        }
    }
}

/// Origine d'une tentative, telle que vue par l'adaptateur entrant.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoginOrigin {
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

fn truncated(value: Option<&str>, max: usize) -> Option<String> {
    let value = value?.trim();
    (!value.is_empty()).then(|| value.chars().take(max).collect())
}

impl LoginOrigin {
    /// Origine nettoyée : valeurs vides retirées, IP et user-agent tronqués.
    pub fn new(ip: Option<&str>, user_agent: Option<&str>) -> Self {
        Self {
            ip: truncated(ip, MAX_IP_CHARS),
            user_agent: truncated(user_agent, MAX_USER_AGENT_CHARS),
        }
    }

    /// Clé du limiteur de tentatives : l'IP, ou une clé commune quand elle est inconnue.
    pub fn throttle_key(&self) -> &str {
        self.ip.as_deref().unwrap_or("inconnue")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginEvent {
    pub at: DateTime<Utc>,
    pub action: LoginAction,
    pub success: bool,
    pub origin: LoginOrigin,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_is_trimmed_and_truncated() {
        let long = "a".repeat(500);
        let origin = LoginOrigin::new(Some(" 203.0.113.7 "), Some(&long));
        assert_eq!(origin.ip.as_deref(), Some("203.0.113.7"));
        assert_eq!(origin.throttle_key(), "203.0.113.7");
        assert_eq!(
            origin.user_agent.map(|ua| ua.chars().count()),
            Some(MAX_USER_AGENT_CHARS)
        );
        let empty = LoginOrigin::new(Some("  "), None);
        assert_eq!(empty, LoginOrigin::default());
        assert_eq!(empty.throttle_key(), "inconnue");
    }

    #[test]
    fn actions_round_trip() {
        for action in [LoginAction::Login, LoginAction::Totp, LoginAction::Reset] {
            assert_eq!(LoginAction::parse(action.as_str()), Some(action));
        }
        assert_eq!(LoginAction::parse("autre"), None);
    }
}
