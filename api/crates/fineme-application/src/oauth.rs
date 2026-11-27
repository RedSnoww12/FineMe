//! Serveur d'autorisation OAuth (connecteur claude.ai) : demande, consentement, échange et renouvellement des
//! jetons, autorisations révocables. Les règles de forme (client, redirection, PKCE, ressource) sont dans le domaine.

use chrono::{DateTime, Utc};
use fineme_domain::access_token::{SECRET_BYTES, SecretDigest, TokenScope};
use fineme_domain::ids::{OAuthGrantId, UserId};
use fineme_domain::login::{LoginAction, LoginOrigin};
use fineme_domain::oauth::{
    self, AuthorizationRequest, ClientId, ClientState, CodeChallenge, CodeVerifier,
    MAX_ACTIVE_GRANTS, OAuthGrant, OAuthSecret, ProtectedResource, PublicUrl, RedirectUri,
    SecretKind, append_query,
};
use fineme_domain::throttle::{LoginThrottle, retry_after_seconds};

use crate::auth::AuthService;
use crate::error::{AppError, AppResult};
use crate::ports::{
    AuthorizationCode, ClientMetadataFetcher, CodeUse, IssuedPair, OAuthRepository, PendingRequest,
};
use crate::secret::constant_time_eq;

const DISABLED: &str = "Le connecteur OAuth n'est pas activé sur ce serveur";
const UNKNOWN_CLIENT: &str =
    "Client OAuth non autorisé : son adresse n'est pas celle d'un client accepté";
const INVALID_REDIRECT: &str = "URI de redirection invalide";
const UNREADABLE_CLIENT: &str = "Document de métadonnées du client illisible";
const UNKNOWN_REQUEST: &str =
    "Demande d'autorisation introuvable ou expirée : relancez la connexion";
const TOO_MANY_GRANTS: &str =
    "Déjà 10 autorisations actives : révoquez-en une dans Paramètres > Accès IA";
const UNKNOWN_GRANT: &str = "Autorisation introuvable";
const ALREADY_REVOKED: &str = "Cette autorisation est déjà révoquée";
const NO_USER: &str = "Aucun compte n'est configuré";

/// Réglages du serveur d'autorisation ; absents = OAuth désactivé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthSettings {
    /// URL publique de l'API : l'`issuer`.
    pub issuer: PublicUrl,
    /// URL publique du front, qui porte l'écran de consentement.
    pub web: PublicUrl,
    /// Hôtes dont les documents de métadonnées client sont acceptés.
    pub client_hosts: Vec<String>,
}

impl OAuthSettings {
    pub fn resource(&self) -> ProtectedResource {
        ProtectedResource::mcp(&self.issuer)
    }
}

/// Paramètres bruts de `GET /oauth/authorize`.
#[derive(Debug, Clone, Default)]
pub struct AuthorizeParams {
    pub response_type: Option<String>,
    pub client_id: Option<String>,
    pub redirect_uri: Option<String>,
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<String>,
    pub state: Option<String>,
    pub scope: Option<String>,
    pub resource: Option<String>,
}

/// Ce que l'écran de consentement montre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsentView {
    pub client_name: String,
    pub redirect_host: String,
    pub scope: TokenScope,
    pub expires_in: i64,
}

/// Demande à `POST /oauth/token`.
#[derive(Clone)]
pub enum TokenRequest {
    AuthorizationCode {
        code: String,
        redirect_uri: String,
        client_id: String,
        code_verifier: String,
        resource: Option<String>,
    },
    Refresh {
        refresh_token: String,
        client_id: String,
        scope: Option<String>,
        resource: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedTokens {
    pub access_token: OAuthSecret,
    pub refresh_token: OAuthSecret,
    pub expires_in: i64,
    pub scope: TokenScope,
}

/// Erreur du point de jeton (RFC 6749 §5.2, RFC 8707).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenErrorCode {
    InvalidRequest,
    InvalidGrant,
    InvalidScope,
    InvalidTarget,
}

impl TokenErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::InvalidGrant => "invalid_grant",
            Self::InvalidScope => "invalid_scope",
            Self::InvalidTarget => "invalid_target",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TokenError {
    #[error("{description}")]
    Refused {
        code: TokenErrorCode,
        description: &'static str,
    },
    #[error(transparent)]
    App(#[from] AppError),
}

impl From<crate::ports::RepoError> for TokenError {
    fn from(error: crate::ports::RepoError) -> Self {
        Self::App(error.into())
    }
}

fn refused(code: TokenErrorCode, description: &'static str) -> TokenError {
    TokenError::Refused { code, description }
}

fn dead_grant() -> TokenError {
    refused(
        TokenErrorCode::InvalidGrant,
        "Code ou jeton invalide, expiré ou révoqué",
    )
}

/// Ports et état du serveur d'autorisation.
pub struct OAuthServer {
    pub(crate) settings: Option<OAuthSettings>,
    pub(crate) repo: std::sync::Arc<dyn OAuthRepository>,
    pub(crate) clients: std::sync::Arc<dyn ClientMetadataFetcher>,
    /// Demandes d'autorisation par source.
    pub(crate) authorize_throttle: std::sync::Mutex<LoginThrottle>,
    /// Échecs au point de jeton (compteur distinct).
    pub(crate) throttle: std::sync::Mutex<LoginThrottle>,
}

impl OAuthServer {
    /// Compte une demande d'autorisation de `source` ; 429 tant que son délai court.
    fn count_authorization(&self, source: &str, now: DateTime<Utc>) -> AppResult<()> {
        let mut throttle = self
            .authorize_throttle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        throttle
            .check(source, now)
            .map_err(|wait| AppError::TooManyAttempts(retry_after_seconds(wait)))?;
        throttle.record_failure(source, now);
        Ok(())
    }

    fn throttle(&self) -> std::sync::MutexGuard<'_, LoginThrottle> {
        // Un verrou empoisonné ne contient qu'un compteur : on continue avec son état.
        self.throttle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl AuthService {
    /// Réglages OAuth, ou 404 quand le connecteur n'est pas activé.
    pub fn oauth_settings(&self) -> AppResult<&OAuthSettings> {
        self.oauth
            .settings
            .as_ref()
            .ok_or(AppError::NotFound(DISABLED))
    }

    fn oauth_secret(&self, kind: SecretKind) -> AppResult<OAuthSecret> {
        let bytes: [u8; SECRET_BYTES] =
            self.access_crypto
                .random_bytes(SECRET_BYTES)?
                .try_into()
                .map_err(|_| AppError::Internal("aléa de longueur inattendue".into()))?;
        Ok(OAuthSecret::from_random(kind, &bytes))
    }

    fn secret_digest(&self, secret: &str) -> SecretDigest {
        SecretDigest::from_bytes(&self.access_crypto.sha256(secret.as_bytes()))
    }

    /// Empreinte d'un secret reçu, s'il a la forme attendue.
    fn presented_digest(&self, kind: SecretKind, value: &str) -> Option<SecretDigest> {
        OAuthSecret::parse(kind, value).map(|secret| self.secret_digest(secret.as_str()))
    }

    /// Valide une demande d'autorisation et renvoie l'URL de redirection : écran de consentement, ou retour au
    /// client avec une erreur. Un client ou une URI de redirection invalide ne redirige jamais (`Err`).
    /// Limitée en fréquence par IP.
    pub async fn authorize(
        &self,
        params: AuthorizeParams,
        origin: &LoginOrigin,
    ) -> AppResult<String> {
        let settings = self.oauth_settings()?;
        self.oauth
            .count_authorization(origin.throttle_key(), self.clock.now())?;
        let client_id = params
            .client_id
            .as_deref()
            .and_then(ClientId::parse)
            .filter(|id| {
                settings
                    .client_hosts
                    .iter()
                    .any(|host| host.eq_ignore_ascii_case(id.host()))
            })
            .ok_or_else(|| AppError::invalid(UNKNOWN_CLIENT))?;
        let redirect_uri = params
            .redirect_uri
            .as_deref()
            .and_then(RedirectUri::parse)
            .ok_or_else(|| AppError::invalid(INVALID_REDIRECT))?;
        let metadata = self
            .oauth
            .clients
            .fetch(&client_id)
            .await
            .map_err(|error| {
                tracing::warn!("métadonnées du client OAuth illisibles : {error}");
                AppError::invalid(UNREADABLE_CLIENT)
            })?;
        let client_name = metadata.check(&client_id, &redirect_uri)?;

        let state = params.state.as_deref().and_then(ClientState::parse);
        let unreadable_state = params.state.is_some() && state.is_none();
        let issuer = settings.issuer.as_str();
        let back = |error: &str| {
            let mut query = vec![("error", error)];
            if let Some(state) = &state {
                query.push(("state", state.as_str()));
            }
            query.push(("iss", issuer));
            redirect_uri.with_params(&query)
        };
        if params.response_type.as_deref() != Some("code") {
            return Ok(back("unsupported_response_type"));
        }
        if unreadable_state {
            return Ok(back("invalid_request"));
        }
        let challenge = params
            .code_challenge
            .as_deref()
            .and_then(CodeChallenge::parse)
            .filter(|_| params.code_challenge_method.as_deref() == Some("S256"));
        let Some(code_challenge) = challenge else {
            return Ok(back("invalid_request"));
        };
        if !params.scope.as_deref().is_none_or(is_read_scope) {
            return Ok(back("invalid_scope"));
        }
        let resource = settings.resource();
        if !params
            .resource
            .as_deref()
            .is_none_or(|requested| resource.accepts(requested))
        {
            return Ok(back("invalid_target"));
        }

        let secret = self.oauth_secret(SecretKind::Request)?;
        let now = self.clock.now();
        self.oauth
            .repo
            .save_request(
                &self.secret_digest(secret.as_str()),
                &PendingRequest {
                    request: AuthorizationRequest {
                        client_id,
                        client_name,
                        redirect_uri: redirect_uri.clone(),
                        code_challenge,
                        state: state.clone(),
                        scope: TokenScope::Read,
                        resource,
                    },
                    expires_at: now + oauth::request_lifetime(),
                },
                now,
            )
            .await?;
        Ok(append_query(
            &format!("{}/oauth/consent", settings.web.as_str()),
            &[("request", secret.as_str())],
        ))
    }

    async fn pending_request(&self, id: &str) -> AppResult<(SecretDigest, PendingRequest)> {
        self.oauth_settings()?;
        let unknown = || AppError::NotFound(UNKNOWN_REQUEST);
        let digest = self
            .presented_digest(SecretKind::Request, id)
            .ok_or_else(unknown)?;
        let pending = self
            .oauth
            .repo
            .find_request(&digest)
            .await?
            .filter(|pending| pending.expires_at > self.clock.now())
            .ok_or_else(unknown)?;
        Ok((digest, pending))
    }

    /// Détail d'une demande pour l'écran de consentement.
    pub async fn consent_view(&self, id: &str) -> AppResult<ConsentView> {
        let (_, pending) = self.pending_request(id).await?;
        Ok(ConsentView {
            client_name: pending.request.client_name.as_str().to_string(),
            redirect_host: pending.request.redirect_uri.host().to_string(),
            scope: pending.request.scope,
            expires_in: (pending.expires_at - self.clock.now()).num_seconds(),
        })
    }

    /// Autorise la demande après mot de passe et second facteur (limités et journalisés comme la connexion) ;
    /// renvoie la redirection portant le code.
    pub async fn approve_request(
        &self,
        id: &str,
        password: &str,
        code: Option<&str>,
        origin: &LoginOrigin,
    ) -> AppResult<String> {
        let (digest, pending) = self.pending_request(id).await?;
        self.check_throttle(origin)?;
        let owner = self
            .users
            .owner()
            .await?
            .ok_or(AppError::NotFound(NO_USER))?;
        let result = self.verify_identity(owner.user.id, password, code).await;
        self.conclude(LoginAction::Oauth, origin, &result).await?;
        let credentials = result?;
        let user = credentials.user.id;
        if self
            .active_grants_besides(user, &pending.request.client_id)
            .await?
            >= MAX_ACTIVE_GRANTS
        {
            return Err(AppError::Conflict(TOO_MANY_GRANTS));
        }
        // Une demande ne sert qu'une fois, même acceptée deux fois en même temps.
        if !self.oauth.repo.take_request(&digest).await? {
            return Err(AppError::NotFound(UNKNOWN_REQUEST));
        }
        let secret = self.oauth_secret(SecretKind::Code)?;
        let now = self.clock.now();
        let redirect_uri = pending.request.redirect_uri.clone();
        let state = pending.request.state.clone();
        self.oauth
            .repo
            .save_code(
                &self.secret_digest(secret.as_str()),
                &AuthorizationCode {
                    request: pending.request,
                    grant_id: OAuthGrantId::generate(),
                    user,
                    password_version: credentials.password_version,
                    expires_at: now + oauth::code_lifetime(),
                },
            )
            .await?;
        self.response_redirect(&redirect_uri, ("code", secret.as_str()), state.as_ref())
    }

    /// Refuse la demande et renvoie la redirection `access_denied`.
    pub async fn deny_request(&self, id: &str) -> AppResult<String> {
        let (digest, pending) = self.pending_request(id).await?;
        if !self.oauth.repo.take_request(&digest).await? {
            return Err(AppError::NotFound(UNKNOWN_REQUEST));
        }
        self.response_redirect(
            &pending.request.redirect_uri,
            ("error", "access_denied"),
            pending.request.state.as_ref(),
        )
    }

    fn response_redirect(
        &self,
        redirect_uri: &RedirectUri,
        outcome: (&str, &str),
        state: Option<&ClientState>,
    ) -> AppResult<String> {
        let mut query = vec![outcome];
        if let Some(state) = state {
            query.push(("state", state.as_str()));
        }
        query.push(("iss", self.oauth_settings()?.issuer.as_str()));
        Ok(redirect_uri.with_params(&query))
    }

    /// Autorisations actives, sans compter celles du même client (remplacées à l'échange).
    async fn active_grants_besides(&self, user: UserId, client: &ClientId) -> AppResult<usize> {
        Ok(self
            .oauth
            .repo
            .grants(user)
            .await?
            .iter()
            .filter(|grant| grant.is_active() && grant.client_id != *client)
            .count())
    }

    /// Point de jeton, limité en fréquence : seuls comptent les refus d'un secret mal formé. Un code ou un jeton
    /// bien formé mais expiré ou révoqué reçoit toujours `invalid_grant` : les clients hébergés partagent leurs
    /// adresses IP, et ce refus leur dit de se réauthentifier.
    pub async fn token(
        &self,
        request: TokenRequest,
        origin: &LoginOrigin,
    ) -> Result<IssuedTokens, TokenError> {
        self.oauth_settings()?;
        let now = self.clock.now();
        let well_formed = match &request {
            TokenRequest::AuthorizationCode { code, .. } => {
                OAuthSecret::parse(SecretKind::Code, code).is_some()
            }
            TokenRequest::Refresh { refresh_token, .. } => {
                OAuthSecret::parse(SecretKind::Refresh, refresh_token).is_some()
            }
        };
        let result = match request {
            TokenRequest::AuthorizationCode {
                code,
                redirect_uri,
                client_id,
                code_verifier,
                resource,
            } => {
                self.exchange_code(
                    &code,
                    &redirect_uri,
                    &client_id,
                    &code_verifier,
                    resource.as_deref(),
                )
                .await
            }
            TokenRequest::Refresh {
                refresh_token,
                client_id,
                scope,
                resource,
            } => {
                self.refresh_tokens(
                    &refresh_token,
                    &client_id,
                    scope.as_deref(),
                    resource.as_deref(),
                )
                .await
            }
        };
        let key = origin.throttle_key();
        match &result {
            Ok(_) => self.oauth.throttle().record_success(key),
            Err(TokenError::Refused { .. }) if !well_formed => {
                let mut throttle = self.oauth.throttle();
                throttle
                    .check(key, now)
                    .map_err(|wait| AppError::TooManyAttempts(retry_after_seconds(wait)))?;
                throttle.record_failure(key, now);
            }
            Err(_) => {}
        }
        result
    }

    fn new_pair(&self, now: DateTime<Utc>) -> AppResult<(OAuthSecret, OAuthSecret, IssuedPair)> {
        let access = self.oauth_secret(SecretKind::Access)?;
        let refresh = self.oauth_secret(SecretKind::Refresh)?;
        let pair = IssuedPair {
            access: self.secret_digest(access.as_str()),
            access_expires_at: now + oauth::access_lifetime(),
            refresh: self.secret_digest(refresh.as_str()),
            refresh_expires_at: now + oauth::refresh_lifetime(),
        };
        Ok((access, refresh, pair))
    }

    fn issued(access: OAuthSecret, refresh: OAuthSecret) -> IssuedTokens {
        IssuedTokens {
            access_token: access,
            refresh_token: refresh,
            expires_in: oauth::access_lifetime().num_seconds(),
            scope: TokenScope::Read,
        }
    }

    /// Vrai si le mot de passe de l'utilisateur n'a pas changé depuis `version`.
    async fn password_is_unchanged(&self, user: UserId, version: i64) -> AppResult<bool> {
        Ok(self
            .users
            .find_by_id(user)
            .await?
            .is_some_and(|credentials| credentials.password_version == version))
    }

    async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
        client_id: &str,
        code_verifier: &str,
        resource: Option<&str>,
    ) -> Result<IssuedTokens, TokenError> {
        let digest = self
            .presented_digest(SecretKind::Code, code)
            .ok_or_else(dead_grant)?;
        let now = self.clock.now();
        // Consommé dès la première présentation : un mauvais vérificateur ne permet pas de réessayer.
        let code = match self.oauth.repo.consume_code(&digest, now).await? {
            Some(CodeUse::First(code)) => *code,
            Some(CodeUse::Replayed(code)) => {
                // Un code périmé ou présenté par un autre client ne doit pas permettre de couper l'accès.
                if code.expires_at > now && code.request.client_id.as_str() == client_id {
                    self.oauth.repo.revoke_grant(code.grant_id, now).await?;
                }
                return Err(dead_grant());
            }
            None => return Err(dead_grant()),
        };
        let request = code.request;
        if code.expires_at <= now
            || request.client_id.as_str() != client_id
            || request.redirect_uri.as_str() != redirect_uri
        {
            return Err(dead_grant());
        }
        let verifier = CodeVerifier::parse(code_verifier)
            .ok_or_else(|| refused(TokenErrorCode::InvalidRequest, "code_verifier invalide"))?;
        let expected =
            CodeChallenge::from_digest(&self.access_crypto.sha256(verifier.as_str().as_bytes()));
        if !constant_time_eq(
            expected.as_str().as_bytes(),
            request.code_challenge.as_str().as_bytes(),
        ) {
            return Err(dead_grant());
        }
        if !resource.is_none_or(|requested| request.resource.accepts(requested)) {
            return Err(refused(TokenErrorCode::InvalidTarget, "Ressource inconnue"));
        }
        if !self
            .password_is_unchanged(code.user, code.password_version)
            .await?
            || self
                .active_grants_besides(code.user, &request.client_id)
                .await?
                >= MAX_ACTIVE_GRANTS
        {
            return Err(dead_grant());
        }
        let (access, refresh, pair) = self.new_pair(now)?;
        let grant = OAuthGrant {
            id: code.grant_id,
            user: code.user,
            client_id: request.client_id,
            client_name: request.client_name,
            redirect_uri: request.redirect_uri,
            scope: request.scope,
            resource: request.resource,
            password_version: code.password_version,
            created_at: now,
            last_used_at: None,
            revoked_at: None,
        };
        if !self.oauth.repo.create_grant(&grant, &pair).await? {
            return Err(dead_grant());
        }
        Ok(Self::issued(access, refresh))
    }

    /// Vrai si l'autorisation vaut toujours : active, pour cette ressource, et mot de passe inchangé depuis.
    async fn grant_is_live(&self, grant: &OAuthGrant) -> AppResult<bool> {
        Ok(grant.resource == self.oauth_settings()?.resource()
            && self
                .users
                .find_by_id(grant.user)
                .await?
                .is_some_and(|credentials| grant.is_usable(credentials.password_version)))
    }

    async fn refresh_tokens(
        &self,
        refresh_token: &str,
        client_id: &str,
        scope: Option<&str>,
        resource: Option<&str>,
    ) -> Result<IssuedTokens, TokenError> {
        let digest = self
            .presented_digest(SecretKind::Refresh, refresh_token)
            .ok_or_else(dead_grant)?;
        let now = self.clock.now();
        let record = self
            .oauth
            .repo
            .find_refresh(&digest)
            .await?
            .ok_or_else(dead_grant)?;
        if record.grant.client_id.as_str() != client_id {
            return Err(dead_grant());
        }
        if record.used {
            self.oauth.repo.revoke_grant(record.grant.id, now).await?;
            return Err(dead_grant());
        }
        if record.expires_at <= now || !self.grant_is_live(&record.grant).await? {
            return Err(dead_grant());
        }
        if !scope.is_none_or(is_read_scope) {
            return Err(refused(TokenErrorCode::InvalidScope, "Portée inconnue"));
        }
        if !resource.is_none_or(|requested| record.grant.resource.accepts(requested)) {
            return Err(refused(TokenErrorCode::InvalidTarget, "Ressource inconnue"));
        }
        let (access, refresh, pair) = self.new_pair(now)?;
        if !self
            .oauth
            .repo
            .rotate(&digest, record.grant.id, &pair)
            .await?
        {
            // Deux renouvellements simultanés avec le même jeton : traités comme un rejeu.
            self.oauth.repo.revoke_grant(record.grant.id, now).await?;
            return Err(dead_grant());
        }
        Ok(Self::issued(access, refresh))
    }

    /// Autorisation active désignée par un jeton d'accès non expiré, émis pour cette ressource.
    pub(crate) async fn grant_for_access_token(
        &self,
        presented: &str,
    ) -> AppResult<Option<OAuthGrant>> {
        if self.oauth.settings.is_none() {
            return Ok(None);
        }
        let Some(digest) = self.presented_digest(SecretKind::Access, presented) else {
            return Ok(None);
        };
        let Some(record) = self.oauth.repo.find_access(&digest).await? else {
            return Ok(None);
        };
        let valid = constant_time_eq(
            record.digest.as_str().as_bytes(),
            digest.as_str().as_bytes(),
        ) && record.expires_at > self.clock.now()
            && self.grant_is_live(&record.grant).await?;
        Ok(valid.then_some(record.grant))
    }

    /// Autorisations de l'utilisateur, révoquées comprises, de la plus récente à la plus ancienne.
    pub async fn oauth_grants(&self, user: UserId) -> AppResult<Vec<OAuthGrant>> {
        Ok(self.oauth.repo.grants(user).await?)
    }

    /// Révoque une autorisation ; mot de passe et, si activé, code du second facteur exigés.
    pub async fn revoke_oauth_grant(
        &self,
        user: UserId,
        id: OAuthGrantId,
        password: &str,
        code: Option<&str>,
        origin: &LoginOrigin,
    ) -> AppResult<OAuthGrant> {
        self.confirm_identity(user, password, code, origin).await?;
        let grant = self
            .oauth
            .repo
            .grants(user)
            .await?
            .into_iter()
            .find(|grant| grant.id == id)
            .ok_or(AppError::NotFound(UNKNOWN_GRANT))?;
        let now = self.clock.now();
        if !grant.is_active() || !self.oauth.repo.revoke_grant(id, now).await? {
            return Err(AppError::Conflict(ALREADY_REVOKED));
        }
        Ok(OAuthGrant {
            revoked_at: Some(now),
            ..grant
        })
    }

    /// Purge demandes, codes et jetons expirés ; renvoie le nombre de lignes supprimées.
    pub async fn prune_oauth(&self) -> AppResult<u64> {
        Ok(self.oauth.repo.prune_expired(self.clock.now()).await?)
    }
}

/// Seule portée : `read` (une liste répétant `read` est tolérée).
fn is_read_scope(scope: &str) -> bool {
    scope
        .split_ascii_whitespace()
        .all(|item| TokenScope::parse(item).is_some())
}

#[cfg(test)]
mod tests;
