use std::sync::{Arc, Mutex, MutexGuard};

use chrono::Duration;
use fineme_domain::ids::{MemberId, UserId};
use fineme_domain::login::{LoginAction, LoginEvent, LoginOrigin};
use fineme_domain::member::{Member, User, default_color};
use fineme_domain::throttle::{LoginThrottle, retry_after_seconds};

use crate::error::{AppError, AppResult};
use crate::ports::{
    Clock, Credentials, LoginJournal, PasswordHasher, SecondFactorRepository, TokenService,
    TokenSubject, TotpSupport, UserRepository,
};
use crate::secret::code_matches;
use crate::totp::LoginOutcome;

/// Nombre de tentatives renvoyées par défaut par le journal des connexions, et plafond.
pub const DEFAULT_LOGIN_HISTORY: i64 = 50;
pub const MAX_LOGIN_HISTORY: i64 = 200;
/// Durée de conservation du journal des connexions.
const LOGIN_HISTORY_RETENTION_DAYS: i64 = 90;

pub const MIN_PASSWORD_LEN: usize = 10;
/// Longueur minimale du code de secours : la route de réinitialisation est publique.
pub const MIN_RECOVERY_TOKEN_LEN: usize = 16;
const INVALID_CREDENTIALS: &str = "E-mail ou mot de passe incorrect";
pub(crate) const INVALID_SESSION: &str = "Session invalide ou expirée";
const INVALID_SETUP_TOKEN: &str = "Code de configuration invalide ou manquant";
const INVALID_RECOVERY_TOKEN: &str = "Code de secours invalide ou manquant";
const RECOVERY_DISABLED: &str =
    "La récupération du mot de passe n'est pas activée sur ce serveur (RECOVERY_TOKEN)";
const WRONG_CURRENT_PASSWORD: &str = "Mot de passe actuel incorrect";
const UNKNOWN_EMAIL: &str = "Aucun compte ne correspond à cette adresse e-mail";

fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

fn check_password_strength(password: &str) -> AppResult<()> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(AppError::invalid(
            "Le mot de passe doit contenir au moins 10 caractères",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub token: String,
    pub user: User,
}

pub struct AuthService {
    pub(crate) users: Arc<dyn UserRepository>,
    pub(crate) second_factor: Arc<dyn SecondFactorRepository>,
    pub(crate) totp: Arc<dyn TotpSupport>,
    pub(crate) journal: Arc<dyn LoginJournal>,
    /// Échecs récents par IP et globalement (en mémoire : une seule instance de l'API).
    pub(crate) throttle: Mutex<LoginThrottle>,
    pub(crate) hasher: Arc<dyn PasswordHasher>,
    pub(crate) tokens: Arc<dyn TokenService>,
    pub(crate) clock: Arc<dyn Clock>,
    /// Hash vérifié quand l'e-mail est inconnu, pour un temps de réponse identique.
    pub(crate) dummy_hash: String,
    /// Code exigé pour la première configuration (`None` = aucune protection, usage local).
    pub(crate) setup_token: Option<String>,
    /// Code de secours pour réinitialiser un mot de passe perdu (`None` = récupération désactivée).
    pub(crate) recovery_token: Option<String>,
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

    /// Vrai si un mot de passe perdu peut être réinitialisé (`RECOVERY_TOKEN` défini côté serveur).
    pub fn recovery_enabled(&self) -> bool {
        self.recovery_token.is_some()
    }

    /// Refuse la configuration si un code est exigé et que celui fourni est absent ou différent.
    fn check_setup_token(&self, provided: Option<&str>) -> AppResult<()> {
        match &self.setup_token {
            Some(expected) if !code_matches(expected, provided) => {
                Err(AppError::Forbidden(INVALID_SETUP_TOKEN))
            }
            _ => Ok(()),
        }
    }

    pub(crate) fn open_session(&self, credentials: Credentials) -> AppResult<Session> {
        let subject = TokenSubject {
            user: credentials.user.id,
            session_version: credentials.session_version,
        };
        Ok(Session {
            token: self.tokens.issue(subject, self.clock.now())?,
            user: credentials.user,
        })
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
        let email = normalize_email(email);
        let name = name.trim().to_string();
        if !email.contains('@') || email.len() < 3 {
            return Err(AppError::invalid("Adresse e-mail invalide"));
        }
        if name.is_empty() {
            return Err(AppError::invalid("Le nom est requis"));
        }
        check_password_strength(password)?;
        const ALREADY: AppError = AppError::Conflict("Un utilisateur existe déjà");
        if self.users.exists_any().await? {
            return Err(ALREADY);
        }
        let credentials = Credentials {
            user: User {
                id: UserId::generate(),
                email,
                name: name.clone(),
            },
            password_hash: self.hasher.hash(password).await?,
            session_version: 0,
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
        self.open_session(credentials)
    }

    pub(crate) fn throttle(&self) -> MutexGuard<'_, LoginThrottle> {
        // Un verrou empoisonné ne contient qu'un compteur : on continue avec son état.
        self.throttle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Refuse (429) une tentative tant que le délai imposé à cette IP, ou à tout le serveur, court.
    pub(crate) fn check_throttle(&self, origin: &LoginOrigin) -> AppResult<()> {
        self.throttle()
            .check(origin.throttle_key(), self.clock.now())
            .map_err(|wait| AppError::TooManyAttempts(retry_after_seconds(wait)))
    }

    /// Met à jour le limiteur et le journal selon l'issue d'une tentative. Seuls les refus d'identité
    /// (mauvais mot de passe, code ou e-mail) comptent comme échecs ; une saisie invalide ne compte pas.
    pub(crate) async fn conclude<T>(
        &self,
        action: LoginAction,
        origin: &LoginOrigin,
        result: &AppResult<T>,
    ) -> AppResult<()> {
        let now = self.clock.now();
        let success = match result {
            Ok(_) => true,
            Err(AppError::Unauthorized(_) | AppError::Forbidden(_) | AppError::NotFound(_)) => {
                false
            }
            Err(_) => return Ok(()),
        };
        {
            let mut throttle = self.throttle();
            if success {
                throttle.record_success(origin.throttle_key());
            } else {
                throttle.record_failure(origin.throttle_key(), now);
            }
        }
        self.journal
            .record(&LoginEvent {
                at: now,
                action,
                success,
                origin: origin.clone(),
            })
            .await?;
        if success {
            self.journal
                .prune_before(now - Duration::days(LOGIN_HISTORY_RETENTION_DAYS))
                .await?;
        }
        Ok(())
    }

    /// Connexion limitée en fréquence et journalisée : même message et même coût que l'e-mail soit
    /// inconnu ou le mot de passe faux. Avec le second facteur activé, un mot de passe juste donne un défi :
    /// ni journalisé ni compté comme réussite (le compteur de l'IP n'est remis à zéro qu'à la seconde étape).
    pub async fn login(
        &self,
        email: &str,
        password: &str,
        origin: &LoginOrigin,
    ) -> AppResult<LoginOutcome> {
        self.check_throttle(origin)?;
        let result = self.verify_login(email, password).await;
        if !matches!(result, Ok(LoginOutcome::Challenge { .. })) {
            self.conclude(LoginAction::Login, origin, &result).await?;
        }
        result
    }

    async fn verify_login(&self, email: &str, password: &str) -> AppResult<LoginOutcome> {
        let found = self.users.find_by_email(&normalize_email(email)).await?;
        let hash = found
            .as_ref()
            .map_or(self.dummy_hash.as_str(), |c| c.password_hash.as_str());
        let valid = self.hasher.verify(password, hash).await;
        match found {
            Some(credentials) if valid => {
                if self.totp_enabled(credentials.user.id).await? {
                    self.challenge(&credentials)
                } else {
                    self.open_session(credentials).map(LoginOutcome::Session)
                }
            }
            _ => Err(AppError::Unauthorized(INVALID_CREDENTIALS)),
        }
    }

    /// Identifie l'utilisateur d'un jeton `Bearer`. Un jeton émis avant le dernier changement de mot de passe
    /// (version de session dépassée) ou pour un utilisateur disparu est refusé.
    pub async fn authenticate(&self, token: &str) -> AppResult<UserId> {
        const REFUSED: AppError = AppError::Unauthorized(INVALID_SESSION);
        let subject = self.tokens.verify(token, self.clock.now()).ok_or(REFUSED)?;
        match self.users.find_by_id(subject.user).await? {
            Some(credentials) if credentials.session_version == subject.session_version => {
                Ok(subject.user)
            }
            _ => Err(REFUSED),
        }
    }

    pub async fn me(&self, id: UserId) -> AppResult<User> {
        Ok(self.credentials(id).await?.user)
    }

    /// Renouvellement glissant : un nouveau jeton, à la version de session courante, pour l'utilisateur
    /// déjà authentifié.
    pub async fn refresh(&self, id: UserId) -> AppResult<Session> {
        let credentials = self.credentials(id).await?;
        self.open_session(credentials)
    }

    /// Dernières tentatives de connexion et de réinitialisation, de la plus récente à la plus ancienne.
    pub async fn login_history(&self, limit: Option<i64>) -> AppResult<Vec<LoginEvent>> {
        let limit = limit
            .unwrap_or(DEFAULT_LOGIN_HISTORY)
            .clamp(1, MAX_LOGIN_HISTORY);
        Ok(self.journal.recent(limit).await?)
    }

    pub(crate) async fn credentials(&self, id: UserId) -> AppResult<Credentials> {
        self.users
            .find_by_id(id)
            .await?
            .ok_or(AppError::Unauthorized(INVALID_SESSION))
    }

    /// Change le mot de passe de l'utilisateur connecté, qui doit fournir l'actuel.
    /// Toutes les autres sessions sont fermées ; la session renvoyée remplace celle de l'appareil courant.
    pub async fn change_password(
        &self,
        id: UserId,
        current_password: &str,
        new_password: &str,
    ) -> AppResult<Session> {
        let credentials = self.credentials(id).await?;
        if !self
            .hasher
            .verify(current_password, &credentials.password_hash)
            .await
        {
            return Err(AppError::Forbidden(WRONG_CURRENT_PASSWORD));
        }
        check_password_strength(new_password)?;
        if new_password == current_password {
            return Err(AppError::invalid(
                "Le nouveau mot de passe doit être différent de l'actuel",
            ));
        }
        self.replace_password(credentials.user, new_password).await
    }

    /// Réinitialise un mot de passe perdu avec le code de secours défini côté serveur.
    /// Le code est vérifié avant tout le reste : sans lui, la route ne révèle rien (ni les e-mails connus).
    /// Limitée en fréquence et journalisée comme la connexion.
    pub async fn reset_password(
        &self,
        email: &str,
        recovery_token: Option<&str>,
        new_password: &str,
        origin: &LoginOrigin,
    ) -> AppResult<Session> {
        self.check_throttle(origin)?;
        let result = self.verify_reset(email, recovery_token, new_password).await;
        self.conclude(LoginAction::Reset, origin, &result).await?;
        result
    }

    async fn verify_reset(
        &self,
        email: &str,
        recovery_token: Option<&str>,
        new_password: &str,
    ) -> AppResult<Session> {
        let expected = self
            .recovery_token
            .as_deref()
            .ok_or(AppError::NotConfigured(RECOVERY_DISABLED))?;
        if !code_matches(expected, recovery_token) {
            return Err(AppError::Forbidden(INVALID_RECOVERY_TOKEN));
        }
        check_password_strength(new_password)?;
        let credentials = self
            .users
            .find_by_email(&normalize_email(email))
            .await?
            .ok_or(AppError::NotFound(UNKNOWN_EMAIL))?;
        // Le code serveur est le dernier recours (téléphone perdu) : il désactive aussi le second facteur.
        self.drop_second_factor(&credentials.user).await?;
        self.replace_password(credentials.user, new_password).await
    }

    /// Enregistre le nouveau hash et ouvre une session à la nouvelle version (les précédentes deviennent invalides).
    async fn replace_password(&self, user: User, new_password: &str) -> AppResult<Session> {
        let password_hash = self.hasher.hash(new_password).await?;
        let session_version = self
            .users
            .update_password(user.id, &password_hash)
            .await?
            .ok_or(AppError::Unauthorized(INVALID_SESSION))?;
        self.open_session(Credentials {
            user,
            password_hash,
            session_version,
        })
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
            t.app.auth.authenticate(&session.token).await.unwrap(),
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
        let session = t
            .app
            .auth
            .login("A@B.test", PASSWORD, &LoginOrigin::default())
            .await
            .unwrap()
            .into_session();
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
            .login("a@b.test", "mauvais-mot-de-passe", &LoginOrigin::default())
            .await
            .unwrap_err();
        let unknown_email = t
            .app
            .auth
            .login("inconnu@b.test", PASSWORD, &LoginOrigin::default())
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
            t.app.auth.authenticate("n'importe-quoi").await,
            Err(AppError::Unauthorized(_))
        ));
        t.clock.advance_days(31);
        assert!(matches!(
            t.app.auth.authenticate(&session.token).await,
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

    // ------------------------------------------------------------ changement de mot de passe

    const NEW_PASSWORD: &str = "nouveau-mot-de-passe";

    /// Application configurée (a@b.test / PASSWORD) et sa session initiale.
    async fn configured(t: &TestApp) -> Session {
        t.app
            .auth
            .setup("a@b.test", PASSWORD, "A", None)
            .await
            .unwrap()
    }

    async fn is_valid(t: &TestApp, token: &str) -> bool {
        t.app.auth.authenticate(token).await.is_ok()
    }

    #[tokio::test]
    async fn changing_password_closes_every_other_session() {
        let t = TestApp::empty().await;
        let first = configured(&t).await;
        let second = t
            .app
            .auth
            .login("a@b.test", PASSWORD, &LoginOrigin::default())
            .await
            .unwrap()
            .into_session();

        let renewed = t
            .app
            .auth
            .change_password(first.user.id, PASSWORD, NEW_PASSWORD)
            .await
            .unwrap();

        assert_eq!(renewed.user, first.user);
        assert!(is_valid(&t, &renewed.token).await);
        assert!(!is_valid(&t, &first.token).await);
        assert!(!is_valid(&t, &second.token).await);
        assert!(
            t.app
                .auth
                .login("a@b.test", PASSWORD, &LoginOrigin::default())
                .await
                .is_err()
        );
        let fresh = t
            .app
            .auth
            .login("a@b.test", NEW_PASSWORD, &LoginOrigin::default())
            .await
            .unwrap()
            .into_session();
        assert!(is_valid(&t, &fresh.token).await);
        assert_eq!(
            t.store.state().users[0].password_hash,
            "hash(nouveau-mot-de-passe)"
        );
    }

    #[tokio::test]
    async fn changing_password_requires_the_current_one() {
        let t = TestApp::empty().await;
        let session = configured(&t).await;
        let error = t
            .app
            .auth
            .change_password(session.user.id, "mauvais-mot-de-passe", NEW_PASSWORD)
            .await
            .unwrap_err();
        // 403 et non 401 : une saisie erronée ne doit pas déconnecter l'utilisateur.
        assert!(matches!(error, AppError::Forbidden(_)));
        assert_eq!(error.to_string(), "Mot de passe actuel incorrect");
        assert!(is_valid(&t, &session.token).await);
        assert_eq!(t.store.state().users[0].session_version, 0);
    }

    #[tokio::test]
    async fn new_password_must_be_long_enough_and_different() {
        let t = TestApp::empty().await;
        let session = configured(&t).await;
        for new_password in ["court", PASSWORD] {
            assert!(matches!(
                t.app
                    .auth
                    .change_password(session.user.id, PASSWORD, new_password)
                    .await,
                Err(AppError::Invalid(_))
            ));
        }
        assert!(is_valid(&t, &session.token).await);
        assert_eq!(
            t.store.state().users[0].password_hash,
            "hash(mot-de-passe-de-test)"
        );
    }

    #[tokio::test]
    async fn sessions_of_a_vanished_user_are_rejected() {
        let t = TestApp::empty().await;
        let session = configured(&t).await;
        t.store.remove_users();
        assert!(!is_valid(&t, &session.token).await);
        assert!(matches!(
            t.app
                .auth
                .change_password(session.user.id, PASSWORD, NEW_PASSWORD)
                .await,
            Err(AppError::Unauthorized(_))
        ));
    }

    // ------------------------------------------------------------ réinitialisation par code de secours

    const RECOVERY: &str = "code-de-secours-fictif-0123";

    #[tokio::test]
    async fn reset_is_unavailable_without_recovery_token() {
        let t = TestApp::empty().await;
        configured(&t).await;
        assert!(!t.app.auth.recovery_enabled());
        assert!(matches!(
            t.app
                .auth
                .reset_password(
                    "a@b.test",
                    Some(RECOVERY),
                    NEW_PASSWORD,
                    &LoginOrigin::default()
                )
                .await,
            Err(AppError::NotConfigured(_))
        ));
    }

    #[tokio::test]
    async fn reset_with_recovery_token_replaces_password_and_closes_sessions() {
        let t = TestApp::with_recovery_token(&format!("  {RECOVERY} ")).await;
        assert!(t.app.auth.recovery_enabled());
        let before = configured(&t).await;

        let session = t
            .app
            .auth
            .reset_password(
                " A@B.test ",
                Some(&format!("{RECOVERY}\n")),
                NEW_PASSWORD,
                &LoginOrigin::default(),
            )
            .await
            .unwrap();

        assert_eq!(session.user, before.user);
        assert!(is_valid(&t, &session.token).await);
        assert!(!is_valid(&t, &before.token).await);
        assert!(
            t.app
                .auth
                .login("a@b.test", PASSWORD, &LoginOrigin::default())
                .await
                .is_err()
        );
        t.app
            .auth
            .login("a@b.test", NEW_PASSWORD, &LoginOrigin::default())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn reset_checks_the_recovery_token_before_anything_else() {
        let t = TestApp::with_recovery_token(RECOVERY).await;
        let session = configured(&t).await;
        for provided in [
            None,
            Some(""),
            Some("mauvais-code"),
            Some("code-de-secours-fictif-012"),
        ] {
            for (email, password) in [("a@b.test", NEW_PASSWORD), ("inconnu@b.test", "court")] {
                // Un jour entre chaque essai : ce test porte sur l'ordre des vérifications, pas sur le limiteur.
                t.clock.advance_days(1);
                let error = t
                    .app
                    .auth
                    .reset_password(email, provided, password, &LoginOrigin::default())
                    .await
                    .unwrap_err();
                assert!(matches!(error, AppError::Forbidden(_)), "{provided:?}");
                assert_eq!(error.to_string(), "Code de secours invalide ou manquant");
            }
        }
        assert!(is_valid(&t, &session.token).await);
        assert_eq!(t.store.state().users[0].session_version, 0);
    }

    #[tokio::test]
    async fn reset_validates_password_then_email() {
        let t = TestApp::with_recovery_token(RECOVERY).await;
        configured(&t).await;
        assert!(matches!(
            t.app
                .auth
                .reset_password("a@b.test", Some(RECOVERY), "court", &LoginOrigin::default())
                .await,
            Err(AppError::Invalid(_))
        ));
        assert!(matches!(
            t.app
                .auth
                .reset_password(
                    "inconnu@b.test",
                    Some(RECOVERY),
                    NEW_PASSWORD,
                    &LoginOrigin::default()
                )
                .await,
            Err(AppError::NotFound(_))
        ));
        assert_eq!(t.store.state().users[0].session_version, 0);
    }

    // ------------------------------------------------------------ limitation et journal

    fn from_ip(ip: &str) -> LoginOrigin {
        LoginOrigin::new(Some(ip), Some("Navigateur de test"))
    }

    async fn fail_login(t: &TestApp, origin: &LoginOrigin) -> AppError {
        t.app
            .auth
            .login("a@b.test", "mauvais-mot-de-passe", origin)
            .await
            .unwrap_err()
    }

    #[tokio::test]
    async fn repeated_failures_from_one_ip_are_throttled_with_growing_delay() {
        let t = TestApp::empty().await;
        configured(&t).await;
        let attacker = from_ip("203.0.113.9");
        for _ in 0..5 {
            assert!(matches!(
                fail_login(&t, &attacker).await,
                AppError::Unauthorized(_)
            ));
        }
        // 6e échec : encore vérifié, mais il déclenche un délai de 30 s.
        assert!(matches!(
            fail_login(&t, &attacker).await,
            AppError::Unauthorized(_)
        ));
        let verifications = t.hasher.verifications();
        let blocked = t
            .app
            .auth
            .login("a@b.test", PASSWORD, &attacker)
            .await
            .unwrap_err();
        assert!(matches!(blocked, AppError::TooManyAttempts(30)));
        // Pendant le délai, le mot de passe n'est même pas vérifié.
        assert_eq!(t.hasher.verifications(), verifications);
        // Une autre IP peut toujours se connecter.
        t.app
            .auth
            .login("a@b.test", PASSWORD, &from_ip("198.51.100.1"))
            .await
            .unwrap();

        t.clock.advance_seconds(30);
        assert!(matches!(
            fail_login(&t, &attacker).await,
            AppError::Unauthorized(_)
        ));
        assert!(matches!(
            fail_login(&t, &attacker).await,
            AppError::TooManyAttempts(60)
        ));
    }

    #[tokio::test]
    async fn a_successful_login_clears_the_ip_counter() {
        let t = TestApp::empty().await;
        configured(&t).await;
        let me = from_ip("192.0.2.4");
        for _ in 0..5 {
            fail_login(&t, &me).await;
        }
        t.app.auth.login("a@b.test", PASSWORD, &me).await.unwrap();
        for _ in 0..5 {
            assert!(matches!(
                fail_login(&t, &me).await,
                AppError::Unauthorized(_)
            ));
        }
    }

    #[tokio::test]
    async fn invalid_input_does_not_count_as_a_failure() {
        let t = TestApp::with_recovery_token(RECOVERY).await;
        configured(&t).await;
        let me = from_ip("192.0.2.4");
        for _ in 0..10 {
            assert!(matches!(
                t.app
                    .auth
                    .reset_password("a@b.test", Some(RECOVERY), "court", &me)
                    .await,
                Err(AppError::Invalid(_))
            ));
        }
        assert!(t.store.state().login_events.is_empty());
    }

    #[tokio::test]
    async fn reset_shares_the_limit_with_login() {
        let t = TestApp::with_recovery_token(RECOVERY).await;
        configured(&t).await;
        let attacker = from_ip("203.0.113.9");
        for _ in 0..6 {
            fail_login(&t, &attacker).await;
        }
        assert!(matches!(
            t.app
                .auth
                .reset_password("a@b.test", Some(RECOVERY), NEW_PASSWORD, &attacker)
                .await,
            Err(AppError::TooManyAttempts(_))
        ));
    }

    #[tokio::test]
    async fn attempts_are_journaled_newest_first() {
        let t = TestApp::with_recovery_token(RECOVERY).await;
        configured(&t).await;
        let origin = from_ip("192.0.2.4");
        fail_login(&t, &origin).await;
        t.clock.advance_seconds(5);
        t.app
            .auth
            .login("a@b.test", PASSWORD, &origin)
            .await
            .unwrap();
        t.clock.advance_seconds(5);
        t.app
            .auth
            .reset_password("a@b.test", Some("mauvais"), NEW_PASSWORD, &origin)
            .await
            .unwrap_err();

        let history = t.app.auth.login_history(None).await.unwrap();
        let summary: Vec<_> = history.iter().map(|e| (e.action, e.success)).collect();
        assert_eq!(
            summary,
            vec![
                (LoginAction::Reset, false),
                (LoginAction::Login, true),
                (LoginAction::Login, false),
            ]
        );
        assert_eq!(history[0].origin, origin);
        assert_eq!(history[0].at, t.clock.now());
        assert_eq!(t.app.auth.login_history(Some(1)).await.unwrap().len(), 1);
        assert_eq!(t.app.auth.login_history(Some(0)).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_successful_login_prunes_old_journal_entries() {
        let t = TestApp::empty().await;
        configured(&t).await;
        fail_login(&t, &from_ip("192.0.2.4")).await;
        t.clock.advance_days(91);
        t.app
            .auth
            .login("a@b.test", PASSWORD, &from_ip("192.0.2.4"))
            .await
            .unwrap();
        let history = t.app.auth.login_history(None).await.unwrap();
        assert_eq!(history.len(), 1);
        assert!(history[0].success);
    }

    #[tokio::test]
    async fn refresh_issues_a_new_token_at_the_current_session_version() {
        let t = TestApp::empty().await;
        let session = configured(&t).await;
        t.clock.advance_days(1);
        let renewed = t.app.auth.refresh(session.user.id).await.unwrap();
        assert_ne!(renewed.token, session.token);
        assert!(is_valid(&t, &renewed.token).await);
        t.store.remove_users();
        assert!(matches!(
            t.app.auth.refresh(session.user.id).await,
            Err(AppError::Unauthorized(_))
        ));
    }
}
