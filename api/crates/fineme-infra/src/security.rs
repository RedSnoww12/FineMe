//! Mots de passe (argon2id) et jetons de session (JWT HS256, 7 jours, renouvelés par le client).

use argon2::{
    Argon2,
    password_hash::{PasswordHasher as _, PasswordVerifier as _, phc::PasswordHash},
};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use fineme_application::ports::{PasswordHasher, ProviderError, TokenService, TokenSubject};
use fineme_domain::ids::UserId;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Durée de vie d'un jeton ; le front le renouvelle (`POST /api/auth/refresh`) tant que la session est active.
pub const TOKEN_DAYS: i64 = 7;
/// Durée de vie d'un jeton de défi (seconde étape de connexion), en secondes.
pub const CHALLENGE_SECONDS: i64 = 300;
/// Usage porté par un jeton de défi ; un jeton de session n'en porte pas.
const CHALLENGE_PURPOSE: &str = "totp";

/// Hachage argon2id (paramètres par défaut de la crate, format PHC).
#[derive(Debug, Clone, Copy, Default)]
pub struct Argon2Hasher;

fn hash_blocking(password: &str) -> Result<String, ProviderError> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|e| ProviderError(format!("hachage du mot de passe impossible : {e}")))
}

fn verify_blocking(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|parsed| {
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        })
        .unwrap_or(false)
}

#[async_trait]
impl PasswordHasher for Argon2Hasher {
    async fn hash(&self, password: &str) -> Result<String, ProviderError> {
        let password = password.to_string();
        // Le hachage est coûteux : hors du fil d'exécution asynchrone.
        tokio::task::spawn_blocking(move || hash_blocking(&password))
            .await
            .map_err(|e| ProviderError(format!("tâche de hachage interrompue : {e}")))?
    }

    async fn verify(&self, password: &str, hash: &str) -> bool {
        let (password, hash) = (password.to_string(), hash.to_string());
        tokio::task::spawn_blocking(move || verify_blocking(&password, &hash))
            .await
            .unwrap_or(false)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: Uuid,
    exp: i64,
    iat: i64,
    /// Version de session ; absente des jetons émis avant son introduction, elle vaut alors 0.
    #[serde(default)]
    ver: i64,
    /// Usage particulier (`totp` pour un défi) ; absent pour un jeton de session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pur: Option<String>,
}

/// Jetons JWT signés HS256. L'expiration est comparée à l'instant fourni par l'appelant (port `Clock`).
#[derive(Clone)]
pub struct JwtTokens {
    secret: String,
}

impl JwtTokens {
    pub fn new(secret: impl Into<String>) -> Self {
        Self {
            secret: secret.into(),
        }
    }
}

impl JwtTokens {
    fn encode(
        &self,
        subject: TokenSubject,
        now: DateTime<Utc>,
        lifetime: Duration,
        purpose: Option<&str>,
    ) -> Result<String, ProviderError> {
        let claims = Claims {
            sub: subject.user.as_uuid(),
            iat: now.timestamp(),
            exp: (now + lifetime).timestamp(),
            ver: subject.session_version,
            pur: purpose.map(str::to_string),
        };
        EncodingKey::from_secret(self.secret.as_bytes())
            .and_then(|key| jsonwebtoken::encode(&Header::new(Algorithm::HS256), &claims, &key))
            .map_err(|e| ProviderError(format!("création du jeton impossible : {e}")))
    }

    /// Contenu d'un jeton authentique, non expiré et dont l'usage est exactement `purpose`.
    fn decode(
        &self,
        token: &str,
        now: DateTime<Utc>,
        purpose: Option<&str>,
    ) -> Option<TokenSubject> {
        let key = DecodingKey::from_secret(self.secret.as_bytes()).ok()?;
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = false;
        let claims = jsonwebtoken::decode::<Claims>(token, &key, &validation)
            .ok()?
            .claims;
        (claims.exp > now.timestamp() && claims.pur.as_deref() == purpose).then(|| TokenSubject {
            user: UserId::from_uuid(claims.sub),
            session_version: claims.ver,
        })
    }
}

impl TokenService for JwtTokens {
    fn issue(&self, subject: TokenSubject, now: DateTime<Utc>) -> Result<String, ProviderError> {
        self.encode(subject, now, Duration::days(TOKEN_DAYS), None)
    }

    fn verify(&self, token: &str, now: DateTime<Utc>) -> Option<TokenSubject> {
        self.decode(token, now, None)
    }

    fn issue_challenge(
        &self,
        subject: TokenSubject,
        now: DateTime<Utc>,
    ) -> Result<String, ProviderError> {
        self.encode(
            subject,
            now,
            Duration::seconds(CHALLENGE_SECONDS),
            Some(CHALLENGE_PURPOSE),
        )
    }

    fn verify_challenge(&self, token: &str, now: DateTime<Utc>) -> Option<TokenSubject> {
        self.decode(token, now, Some(CHALLENGE_PURPOSE))
    }

    fn challenge_seconds(&self) -> i64 {
        CHALLENGE_SECONDS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn password_hash_is_argon2id_and_verifies() {
        let hasher = Argon2Hasher;
        let hash = hasher.hash("phrase de passe fictive").await.unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(hasher.verify("phrase de passe fictive", &hash).await);
        assert!(!hasher.verify("autre phrase", &hash).await);
        assert!(!hasher.verify("x", "pas-un-hash").await);
    }

    #[tokio::test]
    async fn same_password_gets_different_salts() {
        let hasher = Argon2Hasher;
        assert_ne!(
            hasher.hash("identique").await.unwrap(),
            hasher.hash("identique").await.unwrap()
        );
    }

    fn subject(session_version: i64) -> TokenSubject {
        TokenSubject {
            user: UserId::generate(),
            session_version,
        }
    }

    #[test]
    fn token_roundtrip_and_tampering() {
        let tokens = JwtTokens::new("secret-de-test");
        let (subject, now) = (subject(3), Utc::now());
        let token = tokens.issue(subject, now).unwrap();
        assert_eq!(tokens.verify(&token, now), Some(subject));
        assert_eq!(JwtTokens::new("autre-secret").verify(&token, now), None);
        assert_eq!(tokens.verify("n'importe quoi", now), None);
        assert_eq!(tokens.verify(&format!("{token}x"), now), None);
    }

    #[test]
    fn token_expires_after_seven_days() {
        let tokens = JwtTokens::new("secret-de-test");
        let (subject, now) = (subject(0), Utc::now());
        let token = tokens.issue(subject, now).unwrap();
        assert_eq!(
            tokens.verify(&token, now + Duration::days(TOKEN_DAYS - 1)),
            Some(subject)
        );
        assert_eq!(
            tokens.verify(&token, now + Duration::days(TOKEN_DAYS)),
            None
        );
    }

    #[test]
    fn challenge_and_session_tokens_are_not_interchangeable() {
        let tokens = JwtTokens::new("secret-de-test");
        let (subject, now) = (subject(2), Utc::now());
        let challenge = tokens.issue_challenge(subject, now).unwrap();
        let session = tokens.issue(subject, now).unwrap();
        assert_eq!(tokens.verify_challenge(&challenge, now), Some(subject));
        assert_eq!(tokens.verify(&challenge, now), None);
        assert_eq!(tokens.verify_challenge(&session, now), None);
        assert_eq!(
            tokens.verify_challenge(&challenge, now + Duration::seconds(CHALLENGE_SECONDS)),
            None
        );
        assert_eq!(
            JwtTokens::new("autre-secret").verify_challenge(&challenge, now),
            None
        );
    }

    #[test]
    fn tokens_issued_before_session_versions_read_as_version_zero() {
        #[derive(Serialize)]
        struct LegacyClaims {
            sub: Uuid,
            exp: i64,
            iat: i64,
        }
        let (user, now) = (UserId::generate(), Utc::now());
        let legacy = LegacyClaims {
            sub: user.as_uuid(),
            iat: now.timestamp(),
            exp: (now + Duration::days(1)).timestamp(),
        };
        let token = jsonwebtoken::encode(
            &Header::new(Algorithm::HS256),
            &legacy,
            &EncodingKey::from_secret(b"secret-de-test").unwrap(),
        )
        .unwrap();
        assert_eq!(
            JwtTokens::new("secret-de-test").verify(&token, now),
            Some(TokenSubject {
                user,
                session_version: 0
            })
        );
    }
}
