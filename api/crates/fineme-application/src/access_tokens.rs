//! Jetons d'accès personnels en lecture seule (serveur MCP) : création et révocation confirmées par le mot de
//! passe (et le second facteur), authentification d'un appel MCP, journal des appels.

use chrono::Duration;
use fineme_domain::access_token::{
    AccessToken, AccessTokenName, AccessTokenSecret, MAX_ACTIVE_TOKENS, McpCall, SECRET_BYTES,
    SecretDigest, TokenScope,
};
use fineme_domain::ids::{AccessTokenId, UserId};
use fineme_domain::login::LoginOrigin;
use fineme_domain::throttle::{LoginThrottle, retry_after_seconds};

use crate::auth::AuthService;
use crate::error::{AppError, AppResult};
use crate::secret::constant_time_eq;

pub const DEFAULT_MCP_CALLS: i64 = 50;
pub const MAX_MCP_CALLS: i64 = 200;
/// Durée de conservation du journal des appels MCP.
const MCP_CALLS_RETENTION_DAYS: i64 = 90;

const INVALID_ACCESS_TOKEN: &str =
    "Jeton d'accès invalide ou révoqué : créez-en un dans Paramètres > Accès IA";
const TOO_MANY_TOKENS: &str =
    "Déjà 10 jetons actifs : révoquez-en un dans Paramètres > Accès IA avant d'en créer un autre";
const UNKNOWN_TOKEN: &str = "Jeton d'accès introuvable";
const ALREADY_REVOKED: &str = "Ce jeton est déjà révoqué";

/// Jeton créé : le secret n'est renvoyé qu'ici, une seule fois.
#[derive(Debug, Clone, PartialEq)]
pub struct CreatedAccessToken {
    pub token: AccessToken,
    pub secret: AccessTokenSecret,
}

/// Identité d'un appel MCP authentifié par un jeton d'accès.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessPrincipal {
    pub user: UserId,
    pub token_id: AccessTokenId,
    pub token_name: String,
    pub scope: TokenScope,
}

impl AuthService {
    fn mcp_throttle(&self) -> std::sync::MutexGuard<'_, LoginThrottle> {
        // Un verrou empoisonné ne contient qu'un compteur : on continue avec son état.
        self.mcp_throttle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Jetons de l'utilisateur, révoqués compris, du plus récent au plus ancien.
    pub async fn access_tokens(&self, user: UserId) -> AppResult<Vec<AccessToken>> {
        Ok(self.access_tokens.list(user).await?)
    }

    /// Crée un jeton en lecture seule ; mot de passe et, si activé, code du second facteur exigés.
    pub async fn create_access_token(
        &self,
        user: UserId,
        name: &str,
        password: &str,
        code: Option<&str>,
        origin: &LoginOrigin,
    ) -> AppResult<CreatedAccessToken> {
        let name = AccessTokenName::parse(name)?;
        self.confirm_identity(user, password, code, origin).await?;
        let active = self
            .access_tokens
            .list(user)
            .await?
            .iter()
            .filter(|token| token.is_active())
            .count();
        if active >= MAX_ACTIVE_TOKENS {
            return Err(AppError::Conflict(TOO_MANY_TOKENS));
        }
        let secret = self.new_secret()?;
        let token = AccessToken {
            id: AccessTokenId::generate(),
            user,
            name,
            scope: TokenScope::Read,
            hint: secret.hint(),
            created_at: self.clock.now(),
            last_used_at: None,
            revoked_at: None,
        };
        self.access_tokens
            .insert(&token, &self.digest(&secret))
            .await?;
        self.prune_mcp_calls().await?;
        Ok(CreatedAccessToken { token, secret })
    }

    fn new_secret(&self) -> AppResult<AccessTokenSecret> {
        let bytes: [u8; SECRET_BYTES] =
            self.access_crypto
                .random_bytes(SECRET_BYTES)?
                .try_into()
                .map_err(|_| AppError::Internal("aléa de longueur inattendue".into()))?;
        Ok(AccessTokenSecret::from_random(&bytes))
    }

    fn digest(&self, secret: &AccessTokenSecret) -> SecretDigest {
        SecretDigest::from_bytes(&self.access_crypto.sha256(secret.as_str().as_bytes()))
    }

    /// Révoque un jeton ; mot de passe et, si activé, code du second facteur exigés.
    pub async fn revoke_access_token(
        &self,
        user: UserId,
        id: AccessTokenId,
        password: &str,
        code: Option<&str>,
        origin: &LoginOrigin,
    ) -> AppResult<AccessToken> {
        self.confirm_identity(user, password, code, origin).await?;
        let token = self
            .access_tokens
            .find(id)
            .await?
            .filter(|token| token.user == user)
            .ok_or(AppError::NotFound(UNKNOWN_TOKEN))?;
        let now = self.clock.now();
        if !token.is_active() || !self.access_tokens.revoke(id, user, now).await? {
            return Err(AppError::Conflict(ALREADY_REVOKED));
        }
        Ok(AccessToken {
            revoked_at: Some(now),
            ..token
        })
    }

    /// Identifie l'auteur d'un appel MCP à partir du secret présenté. Les échecs sont limités en fréquence
    /// par IP (compteur distinct de la connexion) ; un succès retient la date de dernière utilisation.
    pub async fn authenticate_access_token(
        &self,
        presented: Option<&str>,
        origin: &LoginOrigin,
    ) -> AppResult<AccessPrincipal> {
        let now = self.clock.now();
        self.mcp_throttle()
            .check(origin.throttle_key(), now)
            .map_err(|wait| AppError::TooManyAttempts(retry_after_seconds(wait)))?;
        let result = self.verify_access_token(presented).await;
        match &result {
            Ok(principal) => {
                self.mcp_throttle().record_success(origin.throttle_key());
                // Métadonnée seulement : un échec d'écriture ne refuse pas un jeton valide.
                if let Err(error) = self.access_tokens.touch(principal.token_id, now).await {
                    tracing::warn!("dernière utilisation du jeton non enregistrée : {error}");
                }
            }
            Err(AppError::Unauthorized(_)) => {
                self.mcp_throttle()
                    .record_failure(origin.throttle_key(), now);
            }
            Err(_) => {}
        }
        result
    }

    async fn verify_access_token(&self, presented: Option<&str>) -> AppResult<AccessPrincipal> {
        const REFUSED: AppError = AppError::Unauthorized(INVALID_ACCESS_TOKEN);
        let secret = presented
            .and_then(AccessTokenSecret::parse)
            .ok_or(REFUSED)?;
        let digest = self.digest(&secret);
        let (token, stored) = self
            .access_tokens
            .find_active_by_digest(&digest)
            .await?
            .ok_or(REFUSED)?;
        // La recherche par empreinte (index unique) est la vérification ; la comparaison en temps constant est
        // une défense en profondeur si l'adaptateur venait à renvoyer une autre ligne.
        if !constant_time_eq(stored.as_str().as_bytes(), digest.as_str().as_bytes()) {
            return Err(REFUSED);
        }
        // Utilisateur disparu (base restaurée) : le jeton ne vaut plus rien.
        if self.users.find_by_id(token.user).await?.is_none() {
            return Err(REFUSED);
        }
        Ok(AccessPrincipal {
            user: token.user,
            token_id: token.id,
            token_name: token.name.as_str().to_string(),
            scope: token.scope,
        })
    }

    /// Journalise l'appel d'un tool (sans ses paramètres ni son résultat).
    pub async fn record_mcp_call(
        &self,
        principal: &AccessPrincipal,
        tool: &str,
        success: bool,
    ) -> AppResult<()> {
        Ok(self
            .mcp_calls
            .record(&McpCall {
                at: self.clock.now(),
                token_id: principal.token_id,
                token_name: principal.token_name.clone(),
                tool: tool.to_string(),
                success,
            })
            .await?)
    }

    /// Derniers appels MCP, du plus récent au plus ancien.
    pub async fn mcp_calls(&self, limit: Option<i64>) -> AppResult<Vec<McpCall>> {
        let limit = limit.unwrap_or(DEFAULT_MCP_CALLS).clamp(1, MAX_MCP_CALLS);
        Ok(self.mcp_calls.recent(limit).await?)
    }

    /// Purge les appels MCP de plus de 90 jours ; renvoie le nombre d'appels supprimés.
    pub async fn prune_mcp_calls(&self) -> AppResult<u64> {
        let before = self.clock.now() - Duration::days(MCP_CALLS_RETENTION_DAYS);
        Ok(self.mcp_calls.prune_before(before).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::Session;
    use crate::ports::Clock;
    use crate::testing::TestApp;

    const PASSWORD: &str = "mot-de-passe-de-test";

    async fn configured(t: &TestApp) -> Session {
        t.app
            .auth
            .setup("a@b.test", PASSWORD, "A", None)
            .await
            .unwrap()
    }

    fn origin() -> LoginOrigin {
        LoginOrigin::new(Some("203.0.113.9"), Some("Client de test"))
    }

    async fn create(t: &TestApp, user: UserId, name: &str) -> CreatedAccessToken {
        t.app
            .auth
            .create_access_token(user, name, PASSWORD, None, &origin())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn created_token_authenticates_once_shown_and_keeps_only_its_digest() {
        let t = TestApp::empty().await;
        let user = configured(&t).await.user.id;
        let created = create(&t, user, " Claude Code ").await;

        assert_eq!(created.token.name.as_str(), "Claude Code");
        assert_eq!(created.token.scope, TokenScope::Read);
        assert_eq!(created.token.hint, created.secret.hint());
        let stored = t.store.state().access_tokens;
        assert_eq!(stored.len(), 1);
        assert_ne!(stored[0].1.as_str(), created.secret.as_str());

        let principal = t
            .app
            .auth
            .authenticate_access_token(Some(created.secret.as_str()), &origin())
            .await
            .unwrap();
        assert_eq!(principal.user, user);
        assert_eq!(principal.token_id, created.token.id);
        let listed = t.app.auth.access_tokens(user).await.unwrap();
        assert_eq!(listed[0].last_used_at, Some(t.clock.now()));
    }

    #[tokio::test]
    async fn wrong_password_refuses_creation_and_counts_as_a_failure() {
        let t = TestApp::empty().await;
        let user = configured(&t).await.user.id;
        // 5 échecs tolérés, le sixième déclenche le délai.
        for _ in 0..6 {
            let refused = t
                .app
                .auth
                .create_access_token(user, "Claude", "faux-mot-de-passe", None, &origin())
                .await;
            assert!(matches!(refused, Err(AppError::Forbidden(_))));
        }
        // L'essai suivant attend le délai, même avec le bon mot de passe.
        let throttled = t
            .app
            .auth
            .create_access_token(user, "Claude", PASSWORD, None, &origin())
            .await;
        assert!(matches!(throttled, Err(AppError::TooManyAttempts(_))));
        assert!(t.store.state().access_tokens.is_empty());
    }

    #[tokio::test]
    async fn invalid_name_is_refused_before_checking_the_password() {
        let t = TestApp::empty().await;
        let user = configured(&t).await.user.id;
        let before = t.hasher.verifications();
        let refused = t
            .app
            .auth
            .create_access_token(user, "  ", PASSWORD, None, &origin())
            .await;
        assert!(matches!(refused, Err(AppError::Domain(_))));
        assert_eq!(t.hasher.verifications(), before);
    }

    #[tokio::test]
    async fn at_most_ten_active_tokens() {
        let t = TestApp::empty().await;
        let user = configured(&t).await.user.id;
        let mut ids = Vec::new();
        for n in 0..MAX_ACTIVE_TOKENS {
            ids.push(create(&t, user, &format!("Jeton {n}")).await.token.id);
        }
        let refused = t
            .app
            .auth
            .create_access_token(user, "Un de trop", PASSWORD, None, &origin())
            .await;
        assert!(matches!(refused, Err(AppError::Conflict(_))));
        t.app
            .auth
            .revoke_access_token(user, ids[0], PASSWORD, None, &origin())
            .await
            .unwrap();
        create(&t, user, "Remplaçant").await;
    }

    #[tokio::test]
    async fn revoked_token_is_refused_and_cannot_be_revoked_twice() {
        let t = TestApp::empty().await;
        let user = configured(&t).await.user.id;
        let created = create(&t, user, "Claude").await;
        let revoked = t
            .app
            .auth
            .revoke_access_token(user, created.token.id, PASSWORD, None, &origin())
            .await
            .unwrap();
        assert_eq!(revoked.revoked_at, Some(t.clock.now()));

        let refused = t
            .app
            .auth
            .authenticate_access_token(Some(created.secret.as_str()), &origin())
            .await;
        assert!(matches!(refused, Err(AppError::Unauthorized(_))));
        let again = t
            .app
            .auth
            .revoke_access_token(user, created.token.id, PASSWORD, None, &origin())
            .await;
        assert!(matches!(again, Err(AppError::Conflict(_))));
        let unknown = t
            .app
            .auth
            .revoke_access_token(user, AccessTokenId::generate(), PASSWORD, None, &origin())
            .await;
        assert!(matches!(unknown, Err(AppError::NotFound(_))));
    }

    #[tokio::test]
    async fn revocation_requires_the_password() {
        let t = TestApp::empty().await;
        let user = configured(&t).await.user.id;
        let created = create(&t, user, "Claude").await;
        let refused = t
            .app
            .auth
            .revoke_access_token(user, created.token.id, "faux", None, &origin())
            .await;
        assert!(matches!(refused, Err(AppError::Forbidden(_))));
        assert!(t.app.auth.access_tokens(user).await.unwrap()[0].is_active());
    }

    #[tokio::test]
    async fn changing_or_resetting_the_password_revokes_every_token() {
        let t = TestApp::with_recovery_token("code-de-secours-fictif-0123").await;
        let user = configured(&t).await.user.id;
        let first = create(&t, user, "Portable").await;
        t.app
            .auth
            .change_password(user, PASSWORD, "nouveau-mot-de-passe")
            .await
            .unwrap();
        let refused = t
            .app
            .auth
            .authenticate_access_token(Some(first.secret.as_str()), &origin())
            .await;
        assert!(matches!(refused, Err(AppError::Unauthorized(_))));

        let second = t
            .app
            .auth
            .create_access_token(user, "Bureau", "nouveau-mot-de-passe", None, &origin())
            .await
            .unwrap();
        t.app
            .auth
            .reset_password(
                "a@b.test",
                Some("code-de-secours-fictif-0123"),
                "encore-un-mot-de-passe",
                &origin(),
            )
            .await
            .unwrap();
        let refused = t
            .app
            .auth
            .authenticate_access_token(Some(second.secret.as_str()), &origin())
            .await;
        assert!(matches!(refused, Err(AppError::Unauthorized(_))));
        assert!(
            t.app
                .auth
                .access_tokens(user)
                .await
                .unwrap()
                .iter()
                .all(|token| !token.is_active())
        );
    }

    #[tokio::test]
    async fn session_jwt_and_garbage_are_not_access_tokens() {
        let t = TestApp::empty().await;
        let session = configured(&t).await;
        for presented in [
            None,
            Some(""),
            Some(session.token.as_str()),
            Some("fm_pat_court"),
        ] {
            let refused = t
                .app
                .auth
                .authenticate_access_token(presented, &origin())
                .await;
            assert!(
                matches!(refused, Err(AppError::Unauthorized(_))),
                "{presented:?}"
            );
        }
    }

    #[tokio::test]
    async fn repeated_bad_tokens_are_throttled_per_ip_without_blocking_logins() {
        let t = TestApp::empty().await;
        let user = configured(&t).await.user.id;
        let created = create(&t, user, "Claude").await;
        for _ in 0..6 {
            let _ = t
                .app
                .auth
                .authenticate_access_token(Some("fm_pat_faux"), &origin())
                .await;
        }
        let throttled = t
            .app
            .auth
            .authenticate_access_token(Some(created.secret.as_str()), &origin())
            .await;
        assert!(matches!(throttled, Err(AppError::TooManyAttempts(_))));
        // Une autre IP passe ; la connexion n'est pas touchée par ce compteur.
        let other = LoginOrigin::new(Some("198.51.100.4"), None);
        t.app
            .auth
            .authenticate_access_token(Some(created.secret.as_str()), &other)
            .await
            .unwrap();
        t.app
            .auth
            .login("a@b.test", PASSWORD, &origin())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn token_of_a_vanished_user_is_refused() {
        let t = TestApp::empty().await;
        let user = configured(&t).await.user.id;
        let created = create(&t, user, "Claude").await;
        t.store.remove_users();
        let refused = t
            .app
            .auth
            .authenticate_access_token(Some(created.secret.as_str()), &origin())
            .await;
        assert!(matches!(refused, Err(AppError::Unauthorized(_))));
    }

    #[tokio::test]
    async fn calls_are_journaled_newest_first_and_pruned_after_ninety_days() {
        let t = TestApp::empty().await;
        let user = configured(&t).await.user.id;
        let created = create(&t, user, "Claude").await;
        let principal = t
            .app
            .auth
            .authenticate_access_token(Some(created.secret.as_str()), &origin())
            .await
            .unwrap();
        t.app
            .auth
            .record_mcp_call(&principal, "get_goals", true)
            .await
            .unwrap();
        t.clock.advance_days(91);
        t.app
            .auth
            .record_mcp_call(&principal, "get_overview", false)
            .await
            .unwrap();

        let calls = t.app.auth.mcp_calls(None).await.unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].tool, "get_overview");
        assert!(!calls[0].success);
        assert_eq!(calls[0].token_name, "Claude");

        assert_eq!(t.app.auth.prune_mcp_calls().await.unwrap(), 1);
        assert_eq!(t.app.auth.mcp_calls(Some(500)).await.unwrap().len(), 1);
    }
}
