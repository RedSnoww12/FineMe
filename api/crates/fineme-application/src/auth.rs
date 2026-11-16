use std::sync::Arc;

use fineme_domain::ids::{MemberId, UserId};
use fineme_domain::member::{Member, User, default_color};

use crate::error::{AppError, AppResult};
use crate::ports::{Clock, Credentials, PasswordHasher, TokenService, UserRepository};

pub const MIN_PASSWORD_LEN: usize = 10;
const INVALID_CREDENTIALS: &str = "E-mail ou mot de passe incorrect";
const INVALID_SESSION: &str = "Session invalide ou expirée";
const INVALID_SETUP_TOKEN: &str = "Code de configuration invalide ou manquant";

/// Comparaison en temps constant : la durée ne dépend pas de la position du premier octet différent.
fn constant_time_eq(expected: &[u8], provided: &[u8]) -> bool {
    let mut difference = expected.len() ^ provided.len();
    for index in 0..expected.len().max(provided.len()) {
        let (a, b) = (
            expected.get(index).copied().unwrap_or(0),
            provided.get(index).copied().unwrap_or(0),
        );
        difference |= usize::from(a ^ b);
    }
    difference == 0
}

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub token: String,
    pub user: User,
}

pub struct AuthService {
    pub(crate) users: Arc<dyn UserRepository>,
    pub(crate) hasher: Arc<dyn PasswordHasher>,
    pub(crate) tokens: Arc<dyn TokenService>,
    pub(crate) clock: Arc<dyn Clock>,
    /// Hash vérifié quand l'e-mail est inconnu, pour un temps de réponse identique.
    pub(crate) dummy_hash: String,
    /// Code exigé pour la première configuration (`None` = aucune protection, usage local).
    pub(crate) setup_token: Option<String>,
}

impl AuthService {
    /// Vrai tant qu'aucun utilisateur n'existe.
    pub async fn setup_required(&self) -> AppResult<bool> {
        Ok(!self.users.exists_any().await?)
    }

    /// Vrai si la première configuration exige un code (`SETUP_TOKEN` défini côté serveur).
    pub fn setup_token_required(&self) -> bool {
        self.setup_token.is_some()
    }

    /// Refuse la configuration si un code est exigé et que celui fourni est absent ou différent.
    fn check_setup_token(&self, provided: Option<&str>) -> AppResult<()> {
        match &self.setup_token {
            None => Ok(()),
            Some(expected)
                if constant_time_eq(
                    expected.as_bytes(),
                    provided.unwrap_or("").trim().as_bytes(),
                ) =>
            {
                Ok(())
            }
            Some(_) => Err(AppError::Forbidden(INVALID_SETUP_TOKEN)),
        }
    }

    /// Première configuration : crée l'utilisateur unique et le membre principal du foyer.
    /// Le code de configuration est vérifié avant toute autre chose : rien n'est créé sans lui.
    pub async fn setup(
        &self,
        email: &str,
        password: &str,
        name: &str,
        setup_token: Option<&str>,
    ) -> AppResult<Session> {
        self.check_setup_token(setup_token)?;
        let email = email.trim().to_lowercase();
        let name = name.trim().to_string();
        if !email.contains('@') || email.len() < 3 {
            return Err(AppError::invalid("Adresse e-mail invalide"));
        }
        if name.is_empty() {
            return Err(AppError::invalid("Le nom est requis"));
        }
        if password.chars().count() < MIN_PASSWORD_LEN {
            return Err(AppError::invalid(
                "Le mot de passe doit contenir au moins 10 caractères",
            ));
        }
        const ALREADY: AppError = AppError::Conflict("Un utilisateur existe déjà");
        if self.users.exists_any().await? {
            return Err(ALREADY);
        }
        let user = User {
            id: UserId::generate(),
            email,
            name: name.clone(),
        };
        let credentials = Credentials {
            user: user.clone(),
            password_hash: self.hasher.hash(password).await?,
        };
        let primary = Member {
            id: MemberId::generate(),
            name,
            color: default_color(0).to_string(),
            is_primary: true,
        };
        if !self.users.create_first(&credentials, &primary).await? {
            return Err(ALREADY);
        }
        Ok(Session {
            token: self.tokens.issue(user.id, self.clock.now())?,
            user,
        })
    }

    /// Connexion : même message et même coût que l'e-mail soit inconnu ou le mot de passe faux.
    pub async fn login(&self, email: &str, password: &str) -> AppResult<Session> {
        let found = self
            .users
            .find_by_email(&email.trim().to_lowercase())
            .await?;
        let hash = found
            .as_ref()
            .map_or(self.dummy_hash.as_str(), |c| c.password_hash.as_str());
        let valid = self.hasher.verify(password, hash).await;
        match found {
            Some(credentials) if valid => Ok(Session {
                token: self.tokens.issue(credentials.user.id, self.clock.now())?,
                user: credentials.user,
            }),
            _ => Err(AppError::Unauthorized(INVALID_CREDENTIALS)),
        }
    }

    /// Identifie l'utilisateur d'un jeton `Bearer`.
    pub fn authenticate(&self, token: &str) -> AppResult<UserId> {
        self.tokens
            .verify(token, self.clock.now())
            .ok_or(AppError::Unauthorized(INVALID_SESSION))
    }

    pub async fn me(&self, id: UserId) -> AppResult<User> {
        self.users
            .find_by_id(id)
            .await?
            .ok_or(AppError::Unauthorized(INVALID_SESSION))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestApp;

    const PASSWORD: &str = "mot-de-passe-de-test";

    #[tokio::test]
    async fn setup_creates_user_primary_member_and_session() {
        let t = TestApp::empty().await;
        assert!(t.app.auth.setup_required().await.unwrap());
        let session = t
            .app
            .auth
            .setup(" Camille@Exemple.test ", PASSWORD, " Camille ", None)
            .await
            .unwrap();
        assert_eq!(session.user.email, "camille@exemple.test");
        assert_eq!(session.user.name, "Camille");
        assert!(!t.app.auth.setup_required().await.unwrap());
        let members = t.app.members.list().await.unwrap();
        assert_eq!(members.len(), 1);
        assert!(members[0].is_primary);
        assert_eq!(members[0].name, "Camille");
        assert_eq!(
            t.app.auth.authenticate(&session.token).unwrap(),
            session.user.id
        );
    }

    #[tokio::test]
    async fn setup_validates_inputs() {
        let t = TestApp::empty().await;
        assert!(matches!(
            t.app.auth.setup("pas-un-email", PASSWORD, "A", None).await,
            Err(AppError::Invalid(_))
        ));
        assert!(matches!(
            t.app.auth.setup("a@b.test", PASSWORD, "  ", None).await,
            Err(AppError::Invalid(_))
        ));
        assert!(matches!(
            t.app.auth.setup("a@b.test", "court", "A", None).await,
            Err(AppError::Invalid(_))
        ));
        assert!(t.app.auth.setup_required().await.unwrap());
    }

    #[tokio::test]
    async fn setup_is_refused_once_a_user_exists() {
        let t = TestApp::empty().await;
        t.app
            .auth
            .setup("a@b.test", PASSWORD, "A", None)
            .await
            .unwrap();
        assert!(matches!(
            t.app.auth.setup("c@d.test", PASSWORD, "C", None).await,
            Err(AppError::Conflict(_))
        ));
    }

    #[tokio::test]
    async fn password_is_stored_hashed() {
        let t = TestApp::empty().await;
        t.app
            .auth
            .setup("a@b.test", PASSWORD, "A", None)
            .await
            .unwrap();
        let hash = t.store.state().users[0].password_hash.clone();
        assert_ne!(hash, PASSWORD);
        assert!(hash.contains("hash"));
    }

    #[tokio::test]
    async fn login_succeeds_with_right_password_whatever_the_email_case() {
        let t = TestApp::empty().await;
        t.app
            .auth
            .setup("a@b.test", PASSWORD, "A", None)
            .await
            .unwrap();
        let session = t.app.auth.login("A@B.test", PASSWORD).await.unwrap();
        assert_eq!(
            t.app.auth.me(session.user.id).await.unwrap().email,
            "a@b.test"
        );
    }

    #[tokio::test]
    async fn login_failures_are_indistinguishable() {
        let t = TestApp::empty().await;
        t.app
            .auth
            .setup("a@b.test", PASSWORD, "A", None)
            .await
            .unwrap();
        let wrong_password = t
            .app
            .auth
            .login("a@b.test", "mauvais-mot-de-passe")
            .await
            .unwrap_err();
        let unknown_email = t
            .app
            .auth
            .login("inconnu@b.test", PASSWORD)
            .await
            .unwrap_err();
        assert!(matches!(wrong_password, AppError::Unauthorized(_)));
        assert_eq!(wrong_password.to_string(), unknown_email.to_string());
        // Un hash est vérifié dans les deux cas (coût constant).
        assert_eq!(t.hasher.verifications(), 2);
    }

    #[tokio::test]
    async fn invalid_or_expired_tokens_are_rejected() {
        let t = TestApp::empty().await;
        let session = t
            .app
            .auth
            .setup("a@b.test", PASSWORD, "A", None)
            .await
            .unwrap();
        assert!(matches!(
            t.app.auth.authenticate("n'importe-quoi"),
            Err(AppError::Unauthorized(_))
        ));
        t.clock.advance_days(31);
        assert!(matches!(
            t.app.auth.authenticate(&session.token),
            Err(AppError::Unauthorized(_))
        ));
    }

    #[tokio::test]
    async fn me_fails_for_unknown_user() {
        let t = TestApp::empty().await;
        assert!(matches!(
            t.app.auth.me(UserId::generate()).await,
            Err(AppError::Unauthorized(_))
        ));
    }

    const CODE: &str = "code-de-configuration-fictif";

    #[tokio::test]
    async fn without_configured_setup_token_any_or_no_token_is_accepted() {
        let t = TestApp::empty().await;
        assert!(!t.app.auth.setup_token_required());
        t.app
            .auth
            .setup("a@b.test", PASSWORD, "A", Some("peu importe"))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn correct_setup_token_allows_setup() {
        let t = TestApp::with_setup_token(CODE).await;
        assert!(t.app.auth.setup_token_required());
        let session = t
            .app
            .auth
            .setup("a@b.test", PASSWORD, "A", Some(&format!(" {CODE}\n")))
            .await
            .unwrap();
        assert_eq!(session.user.email, "a@b.test");
        assert!(!t.app.auth.setup_required().await.unwrap());
    }

    #[tokio::test]
    async fn wrong_or_missing_setup_token_is_forbidden_and_creates_nothing() {
        let t = TestApp::with_setup_token(CODE).await;
        for provided in [
            None,
            Some(""),
            Some("mauvais-code"),
            Some("code-de-configuration-fictiF"),
            Some("code"),
        ] {
            let error = t
                .app
                .auth
                .setup("a@b.test", PASSWORD, "A", provided)
                .await
                .unwrap_err();
            assert!(matches!(error, AppError::Forbidden(_)), "{provided:?}");
            assert_eq!(
                error.to_string(),
                "Code de configuration invalide ou manquant"
            );
        }
        // Le code est vérifié avant la validation des champs : aucune information sans lui.
        assert!(matches!(
            t.app.auth.setup("pas-un-email", "court", "", None).await,
            Err(AppError::Forbidden(_))
        ));
        let state = t.store.state();
        assert!(state.users.is_empty() && state.members.is_empty());
        assert!(t.app.auth.setup_required().await.unwrap());
    }

    #[tokio::test]
    async fn existing_user_still_conflicts_with_a_valid_token_and_stays_forbidden_without() {
        let t = TestApp::with_setup_token(CODE).await;
        t.app
            .auth
            .setup("a@b.test", PASSWORD, "A", Some(CODE))
            .await
            .unwrap();
        assert!(matches!(
            t.app
                .auth
                .setup("c@d.test", PASSWORD, "C", Some(CODE))
                .await,
            Err(AppError::Conflict(_))
        ));
        assert!(matches!(
            t.app.auth.setup("c@d.test", PASSWORD, "C", None).await,
            Err(AppError::Forbidden(_))
        ));
        assert_eq!(t.store.state().users.len(), 1);
    }

    #[test]
    fn constant_time_comparison_is_exact() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(constant_time_eq(b"", b""));
        for other in [&b"abd"[..], b"ab", b"abcd", b"", b"ABC"] {
            assert!(!constant_time_eq(b"abc", other));
        }
    }
}
