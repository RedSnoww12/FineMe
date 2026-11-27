//! Jeton d'accès personnel en lecture seule, pour un client IA branché sur le serveur MCP.
//!
//! Le secret n'existe en clair qu'à la création : seule son empreinte (calculée par un port) est conservée.

use chrono::{DateTime, Utc};

use crate::DomainError;
use crate::ids::{AccessTokenId, UserId};

/// Préfixe reconnaissable d'un secret : il distingue un jeton d'accès d'un JWT dès la lecture.
pub const SECRET_PREFIX: &str = "fm_pat_";
/// Octets aléatoires d'un secret (256 bits).
pub const SECRET_BYTES: usize = 32;
/// Longueur de l'encodage base64url sans remplissage de 32 octets.
const ENCODED_LEN: usize = 43;
/// Nombre maximal de jetons actifs à la fois.
pub const MAX_ACTIVE_TOKENS: usize = 10;
const MAX_NAME_CHARS: usize = 60;
/// Derniers caractères du secret montrés dans l'indice.
const HINT_CHARS: usize = 4;

const BASE64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Encodage base64url sans remplissage (RFC 4648 §5).
pub(crate) fn base64url(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        let symbols = chunk.len() + 1;
        for i in 0..symbols {
            let index = (n >> (18 - 6 * i)) & 0x3f;
            out.push(char::from(BASE64URL[index as usize]));
        }
    }
    out
}

/// Vrai si tous les caractères appartiennent à l'alphabet base64url.
pub(crate) fn is_base64url(value: &str) -> bool {
    value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Portée d'un jeton : uniquement la lecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenScope {
    Read,
}

impl TokenScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        (value == "read").then_some(Self::Read)
    }
}

/// Nom choisi pour reconnaître un jeton (« Claude Code portable ») : 1 à 60 caractères.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessTokenName(String);

impl AccessTokenName {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        let value = value.trim();
        if value.is_empty() {
            return Err(DomainError::Rule("Le nom du jeton est requis"));
        }
        if value.chars().count() > MAX_NAME_CHARS {
            return Err(DomainError::Rule(
                "Le nom du jeton ne peut pas dépasser 60 caractères",
            ));
        }
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Secret d'un jeton : `fm_pat_` suivi de 32 octets en base64url. Ne se journalise pas (pas de `Debug` lisible).
#[derive(Clone, PartialEq, Eq)]
pub struct AccessTokenSecret(String);

impl std::fmt::Debug for AccessTokenSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AccessTokenSecret(***)")
    }
}

impl AccessTokenSecret {
    /// Secret construit à partir d'octets aléatoires fournis par un port.
    pub fn from_random(bytes: &[u8; SECRET_BYTES]) -> Self {
        Self(format!("{SECRET_PREFIX}{}", base64url(bytes)))
    }

    /// Secret reçu d'un client : `None` si la forme ne peut pas être celle d'un jeton d'accès.
    pub fn parse(value: &str) -> Option<Self> {
        let encoded = value.strip_prefix(SECRET_PREFIX)?;
        let well_formed = encoded.len() == ENCODED_LEN && is_base64url(encoded);
        well_formed.then(|| Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Indice affichable sans révéler le secret : préfixe et 4 derniers caractères.
    pub fn hint(&self) -> String {
        let tail: String = self
            .0
            .chars()
            .rev()
            .take(HINT_CHARS)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("{SECRET_PREFIX}…{tail}")
    }
}

/// Empreinte d'un secret (SHA-256 en hexadécimal minuscule) : seule forme conservée.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SecretDigest(String);

impl SecretDigest {
    pub fn from_bytes(digest: &[u8; 32]) -> Self {
        Self(digest.iter().map(|b| format!("{b:02x}")).collect())
    }

    /// Empreinte relue depuis le stockage : `None` si ce ne sont pas 64 caractères hexadécimaux minuscules.
    pub fn from_hex(hex: String) -> Option<Self> {
        let valid = hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        valid.then_some(Self(hex))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessToken {
    pub id: AccessTokenId,
    pub user: UserId,
    pub name: AccessTokenName,
    pub scope: TokenScope,
    pub hint: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl AccessToken {
    pub fn is_active(&self) -> bool {
        self.revoked_at.is_none()
    }
}

/// Appel d'un tool MCP, journalisé sans ses paramètres ni son résultat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpCall {
    pub at: DateTime<Utc>,
    pub token_id: AccessTokenId,
    pub token_name: String,
    pub tool: String,
    pub success: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_matches_the_rfc_vectors() {
        assert_eq!(base64url(b""), "");
        assert_eq!(base64url(b"f"), "Zg");
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(base64url(b"foo"), "Zm9v");
        assert_eq!(base64url(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
    }

    #[test]
    fn secret_is_prefixed_and_round_trips() {
        let secret = AccessTokenSecret::from_random(&[7; SECRET_BYTES]);
        assert!(secret.as_str().starts_with(SECRET_PREFIX));
        assert_eq!(secret.as_str().len(), SECRET_PREFIX.len() + ENCODED_LEN);
        assert_eq!(AccessTokenSecret::parse(secret.as_str()), Some(secret));
    }

    #[test]
    fn malformed_secrets_are_refused() {
        let valid = AccessTokenSecret::from_random(&[1; SECRET_BYTES]);
        let body = &valid.as_str()[SECRET_PREFIX.len()..];
        for candidate in [
            "".to_string(),
            body.to_string(),                           // sans préfixe
            format!("{SECRET_PREFIX}{}", &body[1..]),   // trop court
            format!("{SECRET_PREFIX}{body}A"),          // trop long
            format!("{SECRET_PREFIX}{}=", &body[1..]),  // caractère interdit
            "eyJhbGciOiJIUzI1NiJ9.e30.sig".to_string(), // un JWT
        ] {
            assert_eq!(AccessTokenSecret::parse(&candidate), None, "{candidate}");
        }
    }

    #[test]
    fn hint_and_debug_do_not_reveal_the_secret() {
        let secret = AccessTokenSecret::from_random(&[9; SECRET_BYTES]);
        let hint = secret.hint();
        assert!(hint.starts_with("fm_pat_…"));
        assert!(secret.as_str().ends_with(&hint["fm_pat_…".len()..]));
        assert_eq!(hint.chars().count(), "fm_pat_…".chars().count() + 4);
        assert_eq!(format!("{secret:?}"), "AccessTokenSecret(***)");
    }

    #[test]
    fn name_is_trimmed_and_bounded() {
        assert_eq!(
            AccessTokenName::parse("  Claude Code  ").map(|n| n.as_str().to_string()),
            Ok("Claude Code".into())
        );
        assert!(AccessTokenName::parse("   ").is_err());
        assert!(AccessTokenName::parse(&"é".repeat(60)).is_ok());
        assert!(AccessTokenName::parse(&"é".repeat(61)).is_err());
    }

    #[test]
    fn digest_is_lowercase_hex() {
        let digest = SecretDigest::from_bytes(&[0xab; 32]);
        assert_eq!(digest.as_str(), "ab".repeat(32));
        assert_eq!(SecretDigest::from_hex("ab".repeat(32)), Some(digest));
        for invalid in ["AB".repeat(32), "ab".repeat(31), "zz".repeat(32)] {
            assert_eq!(SecretDigest::from_hex(invalid), None);
        }
    }

    #[test]
    fn scope_round_trips() {
        assert_eq!(
            TokenScope::parse(TokenScope::Read.as_str()),
            Some(TokenScope::Read)
        );
        assert_eq!(TokenScope::parse("write"), None);
    }
}
