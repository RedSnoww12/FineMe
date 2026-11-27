//! Règles du serveur d'autorisation OAuth (connecteur claude.ai) : client identifié par un document de métadonnées,
//! URI de redirection exactes, PKCE S256, secrets opaques et autorisations révocables.

use chrono::{DateTime, Duration, Utc};

use crate::DomainError;
use crate::access_token::{SECRET_BYTES, TokenScope, base64url, is_base64url};
use crate::ids::{OAuthGrantId, UserId};

/// Longueur de l'encodage base64url sans remplissage de 32 octets.
const ENCODED_LEN: usize = 43;
const MAX_URL_CHARS: usize = 512;
const MAX_STATE_CHARS: usize = 1024;
const MAX_CLIENT_NAME_CHARS: usize = 100;
const MIN_VERIFIER_CHARS: usize = 43;
const MAX_VERIFIER_CHARS: usize = 128;

/// Nombre maximal d'autorisations actives à la fois.
pub const MAX_ACTIVE_GRANTS: usize = 10;

/// Durée de vie d'une demande en attente de consentement.
pub fn request_lifetime() -> Duration {
    Duration::minutes(10)
}

/// Durée de vie d'un code d'autorisation.
pub fn code_lifetime() -> Duration {
    Duration::seconds(60)
}

/// Durée de vie d'un jeton d'accès.
pub fn access_lifetime() -> Duration {
    Duration::hours(1)
}

/// Durée de vie d'un jeton de renouvellement, comptée depuis la dernière rotation.
pub fn refresh_lifetime() -> Duration {
    Duration::days(30)
}

/// Nature d'un secret opaque, reconnaissable à son préfixe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecretKind {
    /// Demande d'autorisation en attente de consentement.
    Request,
    /// Code d'autorisation, à usage unique.
    Code,
    Access,
    Refresh,
}

impl SecretKind {
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Request => "fm_oar_",
            Self::Code => "fm_oac_",
            Self::Access => "fm_oat_",
            Self::Refresh => "fm_ort_",
        }
    }
}

/// Secret opaque : préfixe puis 32 octets en base64url. Ne se journalise pas.
#[derive(Clone, PartialEq, Eq)]
pub struct OAuthSecret(String);

impl std::fmt::Debug for OAuthSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OAuthSecret(***)")
    }
}

impl OAuthSecret {
    pub fn from_random(kind: SecretKind, bytes: &[u8; SECRET_BYTES]) -> Self {
        Self(format!("{}{}", kind.prefix(), base64url(bytes)))
    }

    /// Secret reçu d'un client : `None` si sa forme ne peut pas être celle d'un secret de cette nature.
    pub fn parse(kind: SecretKind, value: &str) -> Option<Self> {
        let encoded = value.strip_prefix(kind.prefix())?;
        (encoded.len() == ENCODED_LEN && is_base64url(encoded)).then(|| Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Schéma, hôte et reste d'une URL absolue simple : sans identifiants, sans fragment, sans espaces.
struct UrlParts<'a> {
    scheme: &'a str,
    host: &'a str,
    port: Option<&'a str>,
    /// Chemin et requête, vide ou commençant par `/` ou `?`.
    rest: &'a str,
}

fn split_url(value: &str) -> Option<UrlParts<'_>> {
    let forbidden = |c: char| c.is_whitespace() || c.is_control() || c == '\\' || c == '#';
    if value.len() > MAX_URL_CHARS || !value.is_ascii() || value.contains(forbidden) {
        return None;
    }
    let (scheme, after) = value.split_once("://")?;
    let end = after.find(['/', '?']).unwrap_or(after.len());
    let (authority, rest) = after.split_at(end);
    if authority.contains('@') {
        return None;
    }
    // Les crochets n'entourent qu'une adresse IPv6 ; ils sont conservés dans `host`.
    let host_end = if authority.starts_with('[') {
        let end = authority.find(']')? + 1;
        authority[1..end - 1].parse::<std::net::Ipv6Addr>().ok()?;
        end
    } else {
        let end = authority.find(':').unwrap_or(authority.len());
        let name = &authority[..end];
        let valid = !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'));
        valid.then_some(end)?
    };
    let (host, tail) = authority.split_at(host_end);
    let port = match tail {
        "" => None,
        _ => Some(tail.strip_prefix(':')?),
    };
    let valid_port = port.is_none_or(|p| {
        !p.starts_with('0') && p.bytes().all(|b| b.is_ascii_digit()) && p.parse::<u16>().is_ok()
    });
    valid_port.then_some(UrlParts {
        scheme,
        host,
        port,
        rest,
    })
}

fn is_loopback(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "[::1]")
}

/// Vrai pour un nom de domaine, faux pour une adresse IP écrite sous une forme ou une autre.
fn is_domain_name(host: &str) -> bool {
    let last = host
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    last.starts_with(|c: char| c.is_ascii_alphabetic())
}

/// `https`, ou `http` sur la machine locale seulement.
fn is_secure(parts: &UrlParts<'_>) -> bool {
    parts.scheme == "https" || (parts.scheme == "http" && is_loopback(parts.host))
}

/// URL publique d'un service (API ou front) : origine seule, sans chemin ni `/` final.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicUrl(String);

impl PublicUrl {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        split_url(value)
            .filter(|parts| is_secure(parts) && parts.rest.is_empty())
            .filter(|_| value == value.to_ascii_lowercase())
            .map(|_| Self(value.to_string()))
            .ok_or(DomainError::Rule(
                "URL publique invalide : https (ou http en local), sans chemin ni « / » final",
            ))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Identifiant d'un client OAuth : l'URL `https` de son document de métadonnées (CIMD).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientId {
    url: String,
    host: String,
}

impl ClientId {
    pub fn parse(value: &str) -> Option<Self> {
        let parts = split_url(value)?;
        // Chemin sans encodage, paramètre ni requête : aucun serveur ne peut l'interpréter autrement.
        let path = parts.rest;
        let plain = path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-' | b'/'));
        let dot_segment = path.split('/').any(|s| s == "." || s == "..");
        let valid = parts.scheme == "https"
            && parts.port.is_none()
            && is_domain_name(parts.host)
            && path.len() > 1
            && plain
            && !path.contains("//")
            && !dot_segment;
        valid.then(|| Self {
            url: value.to_string(),
            host: parts.host.to_ascii_lowercase(),
        })
    }

    pub fn as_str(&self) -> &str {
        &self.url
    }

    /// Hôte en minuscules, comparé à la liste des hôtes autorisés.
    pub fn host(&self) -> &str {
        &self.host
    }
}

/// URI de redirection : `https`, ou `http` sur la machine locale. Comparée à l'identique.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedirectUri {
    url: String,
    host: String,
}

impl RedirectUri {
    pub fn parse(value: &str) -> Option<Self> {
        let parts = split_url(value)?;
        is_secure(&parts).then(|| Self {
            url: value.to_string(),
            host: match parts.port {
                Some(port) => format!("{}:{port}", parts.host.to_ascii_lowercase()),
                None => parts.host.to_ascii_lowercase(),
            },
        })
    }

    pub fn as_str(&self) -> &str {
        &self.url
    }

    /// Hôte (et port) montré à l'utilisateur sur l'écran de consentement.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Vrai si la redirection reste chez le client : même hôte que son identifiant, ou la machine locale.
    pub fn belongs_to(&self, client_id: &ClientId) -> bool {
        let host = match self.host.rsplit_once(':') {
            Some((host, port)) if port.bytes().all(|b| b.is_ascii_digit()) => host,
            _ => &self.host,
        };
        host == client_id.host() || is_loopback(host)
    }

    /// L'URI complétée de paramètres de réponse.
    pub fn with_params(&self, params: &[(&str, &str)]) -> String {
        append_query(&self.url, params)
    }
}

fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Ajoute des paramètres encodés à une URL, qui peut déjà avoir une requête.
pub fn append_query(url: &str, params: &[(&str, &str)]) -> String {
    let mut out = url.to_string();
    let mut separator = if url.contains('?') { '&' } else { '?' };
    for (key, value) in params {
        out.push(separator);
        out.push_str(&percent_encode(key));
        out.push('=');
        out.push_str(&percent_encode(value));
        separator = '&';
    }
    out
}

/// Ressource protégée (le serveur MCP) : seule audience pour laquelle un jeton est émis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectedResource(String);

impl ProtectedResource {
    /// Serveur MCP d'une API publiée à `issuer`.
    pub fn mcp(issuer: &PublicUrl) -> Self {
        Self(format!("{}/mcp", issuer.as_str()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Vrai si `requested` désigne cette ressource : casse du schéma et de l'hôte ignorée, `/` final toléré.
    pub fn accepts(&self, requested: &str) -> bool {
        let Some(parts) = split_url(requested) else {
            return false;
        };
        let rest = parts.rest.strip_suffix('/').unwrap_or(parts.rest);
        let origin_end = requested.len() - parts.rest.len();
        let origin = requested[..origin_end].to_ascii_lowercase();
        format!("{origin}{rest}") == self.0
    }
}

/// Défi PKCE `S256` : base64url de l'empreinte SHA-256 du vérificateur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeChallenge(String);

impl CodeChallenge {
    pub fn parse(value: &str) -> Option<Self> {
        (value.len() == ENCODED_LEN && is_base64url(value)).then(|| Self(value.to_string()))
    }

    /// Défi correspondant à l'empreinte d'un vérificateur (calculée par un port).
    pub fn from_digest(digest: &[u8; 32]) -> Self {
        Self(base64url(digest))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Vérificateur PKCE : 43 à 128 caractères non réservés (RFC 7636 §4.1). Secret : ne se journalise pas.
#[derive(Clone)]
pub struct CodeVerifier(String);

impl std::fmt::Debug for CodeVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CodeVerifier(***)")
    }
}

impl CodeVerifier {
    pub fn parse(value: &str) -> Option<Self> {
        let valid = (MIN_VERIFIER_CHARS..=MAX_VERIFIER_CHARS).contains(&value.len())
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~'));
        valid.then(|| Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Paramètre `state` du client, recopié tel quel dans la réponse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientState(String);

impl ClientState {
    pub fn parse(value: &str) -> Option<Self> {
        (value.len() <= MAX_STATE_CHARS && !value.chars().any(char::is_control))
            .then(|| Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Caractères de contrôle, invisibles ou de direction du texte : ils permettraient de maquiller un nom à l'écran
/// de consentement.
fn is_disguising(c: char) -> bool {
    c.is_control()
        || matches!(c,
            '\u{00AD}' | '\u{034F}' | '\u{061C}' | '\u{115F}' | '\u{1160}' | '\u{17B4}' | '\u{17B5}'
            | '\u{180B}'..='\u{180F}' | '\u{200B}'..='\u{200F}' | '\u{2028}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}' | '\u{3164}' | '\u{E000}'..='\u{F8FF}' | '\u{FE00}'..='\u{FE0F}'
            | '\u{FEFF}' | '\u{FFA0}' | '\u{FFF0}'..='\u{FFFB}' | '\u{E0000}'..='\u{E0FFF}')
}

/// Nom affiché d'un client : 1 à 100 caractères, sans caractères de contrôle ni de mise en forme du texte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientName(String);

impl ClientName {
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        let length = value.chars().count();
        (length > 0 && length <= MAX_CLIENT_NAME_CHARS && !value.contains(is_disguising))
            .then(|| Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Document de métadonnées d'un client, tel que lu à son URL.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClientMetadata {
    pub client_id: String,
    pub client_name: String,
    pub redirect_uris: Vec<String>,
    pub token_endpoint_auth_method: Option<String>,
}

impl ClientMetadata {
    /// Vérifie le document lu à `client_id` pour la redirection demandée ; renvoie le nom à afficher.
    pub fn check(
        &self,
        client_id: &ClientId,
        redirect_uri: &RedirectUri,
    ) -> Result<ClientName, DomainError> {
        if self.client_id != client_id.as_str() {
            return Err(DomainError::Rule(
                "Le document du client ne porte pas son propre identifiant",
            ));
        }
        if !self
            .redirect_uris
            .iter()
            .any(|u| u == redirect_uri.as_str())
        {
            return Err(DomainError::Rule(
                "URI de redirection non déclarée par le client",
            ));
        }
        if !redirect_uri.belongs_to(client_id) {
            return Err(DomainError::Rule(
                "URI de redirection étrangère à l'hôte du client",
            ));
        }
        if self
            .token_endpoint_auth_method
            .as_deref()
            .is_some_and(|method| method != "none")
        {
            return Err(DomainError::Rule(
                "Seuls les clients publics (sans secret) sont acceptés",
            ));
        }
        ClientName::parse(&self.client_name)
            .ok_or(DomainError::Rule("Nom de client absent ou invalide"))
    }
}

/// Demande d'autorisation validée, en attente du consentement puis de l'échange du code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationRequest {
    pub client_id: ClientId,
    pub client_name: ClientName,
    pub redirect_uri: RedirectUri,
    pub code_challenge: CodeChallenge,
    pub state: Option<ClientState>,
    pub scope: TokenScope,
    pub resource: ProtectedResource,
}

/// Autorisation donnée à un client : ce que Paramètres montre et permet de révoquer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthGrant {
    pub id: OAuthGrantId,
    pub user: UserId,
    pub client_id: ClientId,
    pub client_name: ClientName,
    pub redirect_uri: RedirectUri,
    pub scope: TokenScope,
    pub resource: ProtectedResource,
    /// Version de session à l'autorisation : un changement de mot de passe l'invalide.
    pub session_version: i64,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl OAuthGrant {
    /// Non révoquée (elle compte dans le maximum d'autorisations).
    pub fn is_active(&self) -> bool {
        self.revoked_at.is_none()
    }

    /// Vrai si elle ouvre encore l'accès : non révoquée, et mot de passe inchangé depuis qu'elle a été donnée.
    pub fn is_usable(&self, current_session_version: i64) -> bool {
        self.is_active() && self.session_version == current_session_version
    }
}

#[cfg(test)]
mod tests;
