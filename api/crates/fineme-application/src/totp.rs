//! Second facteur d'authentification (TOTP) : activation, seconde étape de connexion, codes de secours,
//! désactivation. Les règles (pas, fenêtre, anti-rejeu, format des codes) sont dans le domaine.

use fineme_domain::ids::UserId;
use fineme_domain::login::{LoginAction, LoginOrigin};
use fineme_domain::member::User;
use fineme_domain::totp::{
    self, RECOVERY_CODE_COUNT, RECOVERY_CODE_LEN, SECRET_BYTES, TotpCode, TotpSecret,
};

use crate::auth::{AuthService, Session};
use crate::error::{AppError, AppResult};
use crate::ports::{Credentials, SecondFactorRecord, TokenSubject};

/// Émetteur affiché dans l'application d'authentification.
pub const TOTP_ISSUER: &str = "FineMe";

const INVALID_CHALLENGE: &str = "Étape de connexion expirée : reconnectez-vous";
const WRONG_LOGIN_CODE: &str = "Code de vérification incorrect ou déjà utilisé";
const WRONG_CODE: &str = "Code de vérification incorrect";
const WRONG_PASSWORD: &str = "Mot de passe incorrect";
const ALREADY_ENABLED: &str = "Le second facteur est déjà activé";
const NOT_ENABLED: &str = "Le second facteur n'est pas activé";
const NO_PENDING: &str = "Aucune activation en cours : générez d'abord un QR code";
const UNREADABLE_SECRET: &str = "Le secret du second facteur est illisible (JWT_SECRET a-t-il changé ?) : utilisez un code de secours";

/// Issue de la première étape de connexion.
#[derive(Debug, Clone, PartialEq)]
pub enum LoginOutcome {
    /// Pas de second facteur : la session est ouverte.
    Session(Session),
    /// Second facteur activé : le code est attendu avec ce jeton de défi.
    Challenge { token: String, expires_in: i64 },
}

#[cfg(test)]
impl LoginOutcome {
    pub(crate) fn into_session(self) -> Session {
        match self {
            Self::Session(session) => session,
            Self::Challenge { .. } => panic!("défi inattendu"),
        }
    }
}

/// Secret en attente, à scanner puis confirmer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TotpEnrollment {
    pub secret: String,
    pub otpauth_uri: String,
    pub qr_svg: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TotpStatus {
    pub enabled: bool,
    pub recovery_codes_remaining: usize,
}

/// Session ouverte à l'activation, et codes de secours à montrer une seule fois.
#[derive(Debug, Clone, PartialEq)]
pub struct TotpActivation {
    pub session: Session,
    pub recovery_codes: Vec<String>,
}

/// Quels codes une vérification accepte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Accepted {
    TotpOnly,
    TotpOrRecovery,
}

impl AuthService {
    pub async fn totp_status(&self, user: UserId) -> AppResult<TotpStatus> {
        let record = self.second_factor.second_factor(user).await?;
        Ok(TotpStatus {
            enabled: record.secret.is_some(),
            recovery_codes_remaining: if record.secret.is_some() {
                record.recovery_codes.len()
            } else {
                0
            },
        })
    }

    /// Vrai si l'utilisateur a activé le second facteur.
    pub(crate) async fn totp_enabled(&self, user: UserId) -> AppResult<bool> {
        Ok(self
            .second_factor
            .second_factor(user)
            .await?
            .secret
            .is_some())
    }

    /// Défi de seconde étape pour un mot de passe juste.
    pub(crate) fn challenge(&self, credentials: &Credentials) -> AppResult<LoginOutcome> {
        let subject = TokenSubject {
            user: credentials.user.id,
            session_version: credentials.session_version,
        };
        Ok(LoginOutcome::Challenge {
            token: self.tokens.issue_challenge(subject, self.clock.now())?,
            expires_in: self.tokens.challenge_seconds(),
        })
    }

    /// Génère un nouveau secret en attente (remplace le précédent) et son QR code.
    pub async fn start_totp(&self, user: UserId) -> AppResult<TotpEnrollment> {
        let credentials = self.credentials(user).await?;
        if self.totp_enabled(user).await? {
            return Err(AppError::Conflict(ALREADY_ENABLED));
        }
        let secret = TotpSecret::new(self.totp.random_bytes(SECRET_BYTES)?)
            .ok_or_else(|| AppError::Internal("secret TOTP trop court".into()))?;
        self.second_factor
            .save_pending_secret(user, &self.totp.seal(secret.as_bytes())?)
            .await?;
        let otpauth_uri = secret.otpauth_uri(TOTP_ISSUER, &credentials.user.email);
        Ok(TotpEnrollment {
            secret: secret.to_base32(),
            qr_svg: self.totp.qr_svg(&otpauth_uri)?,
            otpauth_uri,
        })
    }

    /// Confirme le secret en attente par un premier code : active le second facteur, délivre les codes de
    /// secours et ferme les autres sessions.
    pub async fn enable_totp(&self, user: UserId, code: &str) -> AppResult<TotpActivation> {
        let credentials = self.credentials(user).await?;
        let record = self.second_factor.second_factor(user).await?;
        if record.secret.is_some() {
            return Err(AppError::Conflict(ALREADY_ENABLED));
        }
        let sealed = record
            .pending_secret
            .ok_or(AppError::Conflict(NO_PENDING))?;
        let secret = self.open_secret(&sealed)?;
        let step = TotpCode::parse(code)
            .and_then(|code| self.matching_step(&secret, code, None))
            .ok_or(AppError::Forbidden(WRONG_CODE))?;
        let (codes, hashes) = self.new_recovery_codes().await?;
        let session_version = self
            .second_factor
            .enable_second_factor(user, &sealed, step, &hashes)
            .await?
            .ok_or(AppError::Unauthorized(crate::auth::INVALID_SESSION))?;
        Ok(TotpActivation {
            session: self.open_session(Credentials {
                session_version,
                ..credentials
            })?,
            recovery_codes: codes,
        })
    }

    /// Seconde étape de connexion : jeton de défi et code TOTP (ou de secours). Limitée et journalisée.
    pub async fn login_totp(
        &self,
        challenge: &str,
        code: &str,
        origin: &LoginOrigin,
    ) -> AppResult<Session> {
        self.check_throttle(origin)?;
        let result = self.verify_login_totp(challenge, code).await;
        self.conclude(LoginAction::Totp, origin, &result).await?;
        result
    }

    async fn verify_login_totp(&self, challenge: &str, code: &str) -> AppResult<Session> {
        const EXPIRED: AppError = AppError::Unauthorized(INVALID_CHALLENGE);
        let subject = self
            .tokens
            .verify_challenge(challenge, self.clock.now())
            .ok_or(EXPIRED)?;
        let credentials = match self.users.find_by_id(subject.user).await? {
            Some(credentials) if credentials.session_version == subject.session_version => {
                credentials
            }
            _ => return Err(EXPIRED),
        };
        if !self
            .verify_code(subject.user, code, Accepted::TotpOrRecovery)
            .await?
        {
            return Err(AppError::Unauthorized(WRONG_LOGIN_CODE));
        }
        self.open_session(credentials)
    }

    /// Désactive le second facteur : mot de passe et code (TOTP ou de secours) exigés. Limitée en fréquence.
    pub async fn disable_totp(
        &self,
        user: UserId,
        password: &str,
        code: &str,
        origin: &LoginOrigin,
    ) -> AppResult<Session> {
        self.check_throttle(origin)?;
        let result = self.verify_disable(user, password, code).await;
        self.count_failure(origin, &result);
        result
    }

    async fn verify_disable(&self, user: UserId, password: &str, code: &str) -> AppResult<Session> {
        let credentials = self.credentials(user).await?;
        if !self.totp_enabled(user).await? {
            return Err(AppError::Conflict(NOT_ENABLED));
        }
        let password_ok = self
            .hasher
            .verify(password, &credentials.password_hash)
            .await;
        if !password_ok {
            return Err(AppError::Forbidden(WRONG_PASSWORD));
        }
        if !self
            .verify_code(user, code, Accepted::TotpOrRecovery)
            .await?
        {
            return Err(AppError::Forbidden(WRONG_CODE));
        }
        let session_version = self
            .second_factor
            .disable_second_factor(user)
            .await?
            .ok_or(AppError::Unauthorized(crate::auth::INVALID_SESSION))?;
        self.open_session(Credentials {
            session_version,
            ..credentials
        })
    }

    /// Remplace les codes de secours ; un code TOTP est exigé. Limitée en fréquence.
    pub async fn regenerate_recovery_codes(
        &self,
        user: UserId,
        code: &str,
        origin: &LoginOrigin,
    ) -> AppResult<Vec<String>> {
        self.check_throttle(origin)?;
        let result = self.verify_regenerate(user, code).await;
        self.count_failure(origin, &result);
        result
    }

    async fn verify_regenerate(&self, user: UserId, code: &str) -> AppResult<Vec<String>> {
        if !self.totp_enabled(user).await? {
            return Err(AppError::Conflict(NOT_ENABLED));
        }
        if !self.verify_code(user, code, Accepted::TotpOnly).await? {
            return Err(AppError::Forbidden(WRONG_CODE));
        }
        let (codes, hashes) = self.new_recovery_codes().await?;
        self.second_factor
            .replace_recovery_codes(user, &hashes)
            .await?;
        Ok(codes)
    }

    /// Un mauvais code sur une route protégée compte comme un échec pour le limiteur (sans journal).
    fn count_failure<T>(&self, origin: &LoginOrigin, result: &AppResult<T>) {
        if matches!(result, Err(AppError::Forbidden(_))) {
            self.throttle()
                .record_failure(origin.throttle_key(), self.clock.now());
        }
    }

    fn open_secret(&self, sealed: &str) -> AppResult<TotpSecret> {
        self.totp
            .open(sealed)
            .ok()
            .and_then(TotpSecret::new)
            .ok_or(AppError::Conflict(UNREADABLE_SECRET))
    }

    fn matching_step(
        &self,
        secret: &TotpSecret,
        code: TotpCode,
        last_used: Option<i64>,
    ) -> Option<i64> {
        totp::matching_step(code, self.clock.now(), last_used, |step| {
            totp::truncate(
                &self
                    .totp
                    .hmac_sha1(secret.as_bytes(), &totp::counter_message(step)),
            )
        })
    }

    /// Vérifie un code TOTP (puis le consigne : il ne resservira pas) ou, si permis, un code de secours (consommé).
    async fn verify_code(&self, user: UserId, code: &str, accepted: Accepted) -> AppResult<bool> {
        let record = self.second_factor.second_factor(user).await?;
        if let Some(totp_code) = TotpCode::parse(code) {
            return self.verify_totp(user, &record, totp_code).await;
        }
        match (accepted, totp::normalize_recovery_code(code)) {
            (Accepted::TotpOrRecovery, Some(recovery)) => {
                self.verify_recovery(user, &record, &recovery).await
            }
            _ => Ok(false),
        }
    }

    async fn verify_totp(
        &self,
        user: UserId,
        record: &SecondFactorRecord,
        code: TotpCode,
    ) -> AppResult<bool> {
        let Some(sealed) = &record.secret else {
            return Ok(false);
        };
        let secret = match self.open_secret(sealed) {
            Ok(secret) => secret,
            // Secret illisible : seul un code de secours peut encore servir.
            Err(_) => return Ok(false),
        };
        match self.matching_step(&secret, code, record.last_step) {
            // Écriture conditionnelle : deux requêtes simultanées avec le même code n'aboutissent qu'une fois.
            Some(step) => Ok(self.second_factor.use_totp_step(user, step).await?),
            None => Ok(false),
        }
    }

    async fn verify_recovery(
        &self,
        user: UserId,
        record: &SecondFactorRecord,
        code: &str,
    ) -> AppResult<bool> {
        for stored in &record.recovery_codes {
            if self.hasher.verify(code, &stored.hash).await {
                return Ok(self
                    .second_factor
                    .use_recovery_code(user, stored.id)
                    .await?);
            }
        }
        Ok(false)
    }

    /// Codes de secours en clair (à montrer) et leurs hachés (à stocker).
    async fn new_recovery_codes(&self) -> AppResult<(Vec<String>, Vec<String>)> {
        let bytes = self
            .totp
            .random_bytes(RECOVERY_CODE_COUNT * RECOVERY_CODE_LEN)?;
        let mut codes = Vec::with_capacity(RECOVERY_CODE_COUNT);
        let mut hashes = Vec::with_capacity(RECOVERY_CODE_COUNT);
        for chunk in bytes.as_chunks::<RECOVERY_CODE_LEN>().0 {
            let code = totp::recovery_code_from_bytes(chunk);
            hashes.push(self.hasher.hash(&code).await?);
            codes.push(code);
        }
        if codes.len() != RECOVERY_CODE_COUNT {
            return Err(AppError::Internal("aléa insuffisant".into()));
        }
        Ok((codes, hashes))
    }

    /// Utilisé par la réinitialisation : le code serveur `RECOVERY_TOKEN` désactive le second facteur.
    pub(crate) async fn drop_second_factor(&self, user: &User) -> AppResult<()> {
        self.second_factor.disable_second_factor(user.id).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{Clock, TotpSupport};
    use crate::testing::TestApp;

    const PASSWORD: &str = "mot-de-passe-de-test";
    const RECOVERY: &str = "code-de-secours-fictif-0123";

    async fn configured(t: &TestApp) -> Session {
        t.app
            .auth
            .setup("a@b.test", PASSWORD, "A", None)
            .await
            .unwrap()
    }

    /// Secret en clair (en attente ou actif) lu dans le stockage factice.
    fn secret(t: &TestApp) -> Vec<u8> {
        let record = t.store.state().second_factor;
        let sealed = record.secret.or(record.pending_secret).unwrap();
        t.totp.open(&sealed).unwrap()
    }

    /// Code du pas courant décalé de `offset` pas.
    fn code(t: &TestApp, offset: i64) -> String {
        t.totp.code(&secret(t), t.clock.now(), offset)
    }

    /// Code à 6 chiffres qui n'est celui d'aucun pas toléré.
    fn wrong(t: &TestApp) -> String {
        let valid: Vec<String> = (-1..=1).map(|offset| code(t, offset)).collect();
        (0..10)
            .map(|n| format!("{n}{n}{n}{n}{n}{n}"))
            .find(|candidate| !valid.contains(candidate))
            .unwrap()
    }

    /// Utilisateur configuré avec le second facteur activé.
    async fn enrolled(t: &TestApp) -> TotpActivation {
        let session = configured(t).await;
        t.app.auth.start_totp(session.user.id).await.unwrap();
        let activation = t
            .app
            .auth
            .enable_totp(session.user.id, &code(t, 0))
            .await
            .unwrap();
        // Deux pas plus tard : le code d'activation et ses voisins sont hors fenêtre.
        t.clock.advance_seconds(60);
        activation
    }

    async fn challenge(t: &TestApp, origin: &LoginOrigin) -> String {
        match t
            .app
            .auth
            .login("a@b.test", PASSWORD, origin)
            .await
            .unwrap()
        {
            LoginOutcome::Challenge { token, expires_in } => {
                assert_eq!(expires_in, 300);
                token
            }
            LoginOutcome::Session(_) => panic!("défi attendu"),
        }
    }

    fn from_ip(ip: &str) -> LoginOrigin {
        LoginOrigin::new(Some(ip), Some("Navigateur de test"))
    }

    #[tokio::test]
    async fn enrollment_gives_a_base32_secret_and_an_otpauth_qr_code() {
        let t = TestApp::empty().await;
        let session = configured(&t).await;
        let enrollment = t.app.auth.start_totp(session.user.id).await.unwrap();
        let secret = TotpSecret::new(secret(&t)).unwrap();
        assert_eq!(enrollment.secret, secret.to_base32());
        assert_eq!(
            enrollment.otpauth_uri,
            secret.otpauth_uri("FineMe", "a@b.test")
        );
        assert_eq!(
            enrollment.qr_svg,
            format!("<svg>{}</svg>", enrollment.otpauth_uri)
        );
        // Rien n'est activé tant que le premier code n'est pas confirmé.
        assert!(
            !t.app
                .auth
                .totp_status(session.user.id)
                .await
                .unwrap()
                .enabled
        );
        assert!(t.store.state().second_factor.secret.is_none());
    }

    #[tokio::test]
    async fn activation_requires_a_valid_first_code_and_closes_other_sessions() {
        let t = TestApp::empty().await;
        let session = configured(&t).await;
        let id = session.user.id;
        assert!(matches!(
            t.app.auth.enable_totp(id, "123456").await,
            Err(AppError::Conflict(_))
        ));
        t.app.auth.start_totp(id).await.unwrap();
        for bad in [wrong(&t), "abc".into(), String::new()] {
            assert!(matches!(
                t.app.auth.enable_totp(id, &bad).await,
                Err(AppError::Forbidden(_))
            ));
        }
        let activation = t.app.auth.enable_totp(id, &code(&t, 0)).await.unwrap();
        assert_eq!(activation.recovery_codes.len(), 10);
        assert!(
            t.app
                .auth
                .authenticate(&activation.session.token)
                .await
                .is_ok()
        );
        assert!(t.app.auth.authenticate(&session.token).await.is_err());
        assert_eq!(
            t.app.auth.totp_status(id).await.unwrap(),
            TotpStatus {
                enabled: true,
                recovery_codes_remaining: 10
            }
        );
        // Codes de secours stockés hachés seulement.
        let stored = t.store.state().second_factor.recovery_codes;
        assert!(
            stored
                .iter()
                .all(|c| !activation.recovery_codes.contains(&c.hash))
        );
        assert!(matches!(
            t.app.auth.start_totp(id).await,
            Err(AppError::Conflict(_))
        ));
    }

    #[tokio::test]
    async fn login_with_second_factor_takes_two_steps_and_refuses_replay() {
        let t = TestApp::empty().await;
        enrolled(&t).await;
        let origin = from_ip("192.0.2.4");
        let token = challenge(&t, &origin).await;
        // Le mot de passe juste n'est pas journalisé : seule la seconde étape l'est.
        assert!(t.store.state().login_events.is_empty());
        // Le défi ne vaut pas session.
        assert!(t.app.auth.authenticate(&token).await.is_err());

        let current = code(&t, 0);
        let session = t
            .app
            .auth
            .login_totp(&token, &current, &origin)
            .await
            .unwrap();
        assert!(t.app.auth.authenticate(&session.token).await.is_ok());
        // Rejeu du même code, même dans sa fenêtre : refusé.
        assert!(matches!(
            t.app.auth.login_totp(&token, &current, &origin).await,
            Err(AppError::Unauthorized(_))
        ));
        // Un code d'un pas antérieur, encore dans la tolérance, est aussi refusé.
        assert!(matches!(
            t.app.auth.login_totp(&token, &code(&t, -1), &origin).await,
            Err(AppError::Unauthorized(_))
        ));
        // Même instant pour les trois tentatives : l'ordre du journal n'est pas défini, on compare triés.
        let mut journal: Vec<_> = t
            .app
            .auth
            .login_history(None)
            .await
            .unwrap()
            .iter()
            .map(|e| (e.action.as_str(), e.success))
            .collect();
        journal.sort();
        assert_eq!(
            journal,
            vec![("totp", false), ("totp", false), ("totp", true)]
        );
    }

    #[tokio::test]
    async fn one_step_of_clock_skew_is_tolerated() {
        let t = TestApp::empty().await;
        enrolled(&t).await;
        let origin = from_ip("192.0.2.4");
        let token = challenge(&t, &origin).await;
        assert!(
            t.app
                .auth
                .login_totp(&token, &code(&t, -2), &origin)
                .await
                .is_err()
        );
        assert!(
            t.app
                .auth
                .login_totp(&token, &code(&t, 2), &origin)
                .await
                .is_err()
        );
        t.app
            .auth
            .login_totp(&token, &code(&t, -1), &origin)
            .await
            .unwrap();
        t.app
            .auth
            .login_totp(&token, &code(&t, 1), &origin)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn challenge_expires_and_session_tokens_are_not_challenges() {
        let t = TestApp::empty().await;
        let activation = enrolled(&t).await;
        let origin = from_ip("192.0.2.4");
        assert!(matches!(
            t.app
                .auth
                .login_totp(&activation.session.token, &code(&t, 0), &origin)
                .await,
            Err(AppError::Unauthorized(_))
        ));
        let token = challenge(&t, &origin).await;
        t.clock.advance_seconds(301);
        assert!(matches!(
            t.app.auth.login_totp(&token, &code(&t, 0), &origin).await,
            Err(AppError::Unauthorized(_))
        ));
    }

    #[tokio::test]
    async fn recovery_codes_work_once_and_can_be_regenerated() {
        let t = TestApp::empty().await;
        let activation = enrolled(&t).await;
        let id = activation.session.user.id;
        let origin = from_ip("192.0.2.4");
        let first = activation.recovery_codes[0]
            .to_uppercase()
            .replace('-', " ");
        let token = challenge(&t, &origin).await;
        t.app
            .auth
            .login_totp(&token, &first, &origin)
            .await
            .unwrap();
        assert!(
            t.app
                .auth
                .login_totp(&token, &first, &origin)
                .await
                .is_err()
        );
        assert_eq!(
            t.app
                .auth
                .totp_status(id)
                .await
                .unwrap()
                .recovery_codes_remaining,
            9
        );

        // Régénération : un code TOTP est exigé (pas un code de secours).
        assert!(matches!(
            t.app
                .auth
                .regenerate_recovery_codes(id, &activation.recovery_codes[1], &origin)
                .await,
            Err(AppError::Forbidden(_))
        ));
        let renewed = t
            .app
            .auth
            .regenerate_recovery_codes(id, &code(&t, 0), &origin)
            .await
            .unwrap();
        assert_eq!(renewed.len(), 10);
        assert!(
            renewed
                .iter()
                .all(|c| !activation.recovery_codes.contains(c))
        );
        assert!(
            t.app
                .auth
                .login_totp(&token, &activation.recovery_codes[1], &origin)
                .await
                .is_err()
        );
        t.app
            .auth
            .login_totp(&token, &renewed[0], &origin)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn wrong_codes_are_throttled_and_a_right_password_does_not_reset_the_counter() {
        let t = TestApp::empty().await;
        enrolled(&t).await;
        let attacker = from_ip("203.0.113.9");
        let token = challenge(&t, &attacker).await;
        let bad = wrong(&t);
        for _ in 0..5 {
            assert!(matches!(
                t.app.auth.login_totp(&token, &bad, &attacker).await,
                Err(AppError::Unauthorized(_))
            ));
            // Un nouveau défi (mot de passe juste) n'efface pas les échecs du code.
            challenge(&t, &attacker).await;
        }
        assert!(
            t.app
                .auth
                .login_totp(&token, &bad, &attacker)
                .await
                .is_err()
        );
        assert!(matches!(
            t.app.auth.login_totp(&token, &code(&t, 0), &attacker).await,
            Err(AppError::TooManyAttempts(30))
        ));
    }

    #[tokio::test]
    async fn disabling_requires_password_and_code_and_closes_sessions() {
        let t = TestApp::empty().await;
        let activation = enrolled(&t).await;
        let id = activation.session.user.id;
        let origin = from_ip("192.0.2.4");
        assert!(matches!(
            t.app
                .auth
                .disable_totp(id, "mauvais-mot-de-passe", &code(&t, 0), &origin)
                .await,
            Err(AppError::Forbidden(_))
        ));
        assert!(matches!(
            t.app
                .auth
                .disable_totp(id, PASSWORD, "abcde-fgh89", &origin)
                .await,
            Err(AppError::Forbidden(_))
        ));
        let session = t
            .app
            .auth
            .disable_totp(id, PASSWORD, &code(&t, 0), &origin)
            .await
            .unwrap();
        assert!(t.app.auth.authenticate(&session.token).await.is_ok());
        assert!(
            t.app
                .auth
                .authenticate(&activation.session.token)
                .await
                .is_err()
        );
        assert_eq!(t.store.state().second_factor, SecondFactorRecord::default());
        assert!(matches!(
            t.app.auth.login("a@b.test", PASSWORD, &origin).await,
            Ok(LoginOutcome::Session(_))
        ));
        assert!(matches!(
            t.app
                .auth
                .disable_totp(id, PASSWORD, "123456", &origin)
                .await,
            Err(AppError::Conflict(_))
        ));
    }

    #[tokio::test]
    async fn reset_with_the_server_recovery_token_disables_the_second_factor() {
        let t = TestApp::with_recovery_token(RECOVERY).await;
        enrolled(&t).await;
        t.app
            .auth
            .reset_password(
                "a@b.test",
                Some(RECOVERY),
                "nouveau-mot-de-passe",
                &LoginOrigin::default(),
            )
            .await
            .unwrap();
        assert_eq!(t.store.state().second_factor, SecondFactorRecord::default());
        assert!(matches!(
            t.app
                .auth
                .login("a@b.test", "nouveau-mot-de-passe", &LoginOrigin::default())
                .await,
            Ok(LoginOutcome::Session(_))
        ));
    }
}
