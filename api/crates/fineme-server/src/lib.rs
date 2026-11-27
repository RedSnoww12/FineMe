//! Composition root : lit la configuration, câble les adaptateurs sur les ports et construit le routeur.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use axum::Router;
use fineme_application::auth::MIN_RECOVERY_TOKEN_LEN;
use fineme_application::daily::MIN_CRON_SECRET_LEN;
use fineme_application::oauth::OAuthSettings;
use fineme_application::ports::ClientMetadataFetcher;
use fineme_application::ports::PriceProvider;
use fineme_application::{App, Ports, Settings};
use fineme_domain::oauth::{ClientHost, PublicUrl};
use fineme_infra::access_token::AccessTokenDigest;
use fineme_infra::client_metadata::HttpClientMetadata;
use fineme_infra::clock::SystemClock;
use fineme_infra::market::{self, CoinGecko, Eodhd, Frankfurter, YahooFinance};
use fineme_infra::postgres::{self, PgStore};
use fineme_infra::security::{Argon2Hasher, JwtTokens};
use fineme_infra::totp::TotpCrypto;
use fineme_mcp::{DEFAULT_ALLOWED_HOSTS, McpSettings};
use sqlx::PgPool;

pub const DEFAULT_PORT: u16 = 8080;
/// Seul client OAuth accepté quand `OAUTH_CLIENT_HOSTS` est absent.
const DEFAULT_OAUTH_CLIENT_HOST: &str = "claude.ai";
/// Render place un proxy devant le service : sa dernière entrée de `X-Forwarded-For` est l'IP du client.
pub const DEFAULT_TRUSTED_PROXY_HOPS: usize = 1;

/// Configuration lue dans l'environnement (`DATABASE_URL`, `JWT_SECRET`, `CORS_ORIGINS`, `PORT`, `SETUP_TOKEN`,
/// `RECOVERY_TOKEN`, `CRON_SECRET`, `EODHD_API_KEY`, `TRUSTED_PROXY_HOPS`, `MCP_ALLOWED_HOSTS`,
/// `PUBLIC_API_URL`, `WEB_URL`, `OAUTH_CLIENT_HOSTS`).
#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    pub cors_origins: String,
    pub port: u16,
    /// Code exigé pour la première configuration (`SETUP_TOKEN`) ; `None` si absent ou vide.
    pub setup_token: Option<String>,
    /// Code de secours pour réinitialiser un mot de passe perdu (`RECOVERY_TOKEN`) ; `None` si absent ou vide.
    pub recovery_token: Option<String>,
    /// Secret de la tâche quotidienne (`CRON_SECRET`) ; `None` si absent ou vide.
    pub cron_secret: Option<String>,
    /// Clé du fournisseur de cours de repli EODHD (`EODHD_API_KEY`) ; `None` = pas de repli.
    pub eodhd_api_key: Option<String>,
    /// Proxys de confiance devant l'API (`TRUSTED_PROXY_HOPS`, défaut 1 : le proxy de Render) : l'IP du client
    /// est lue à cette position depuis la droite de `X-Forwarded-For` ; 0 = adresse TCP seulement.
    pub trusted_proxy_hops: usize,
    /// Hôtes acceptés dans l'en-tête `Host` de `/mcp` (`MCP_ALLOWED_HOSTS`, séparés par des virgules) ;
    /// défaut : boucle locale.
    pub mcp_allowed_hosts: Vec<String>,
    /// Serveur d'autorisation OAuth (`PUBLIC_API_URL`, `WEB_URL`, `OAUTH_CLIENT_HOSTS`) ; `None` = désactivé.
    pub oauth: Option<OAuthSettings>,
}

/// `PUBLIC_API_URL` et `WEB_URL` activent ensemble le connecteur OAuth.
fn oauth_settings(
    lookup: &impl Fn(&str) -> Option<String>,
) -> anyhow::Result<Option<OAuthSettings>> {
    let url = |key: &str| -> anyhow::Result<Option<PublicUrl>> {
        optional_code(lookup, key)
            .map(|value| PublicUrl::parse(&value).with_context(|| key.to_string()))
            .transpose()
    };
    let (issuer, web) = match (url("PUBLIC_API_URL")?, url("WEB_URL")?) {
        (Some(issuer), Some(web)) => (issuer, web),
        (None, None) => return Ok(None),
        _ => anyhow::bail!("PUBLIC_API_URL et WEB_URL doivent être définis ensemble"),
    };
    let hosts = optional_code(lookup, "OAUTH_CLIENT_HOSTS")
        .unwrap_or_else(|| DEFAULT_OAUTH_CLIENT_HOST.to_string());
    let client_hosts = hosts
        .split(',')
        .map(str::trim)
        .filter(|host| !host.is_empty())
        .map(|host| {
            ClientHost::parse(host)
                .with_context(|| format!("OAUTH_CLIENT_HOSTS : hôte invalide « {host} »"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    if client_hosts.is_empty() {
        anyhow::bail!("OAUTH_CLIENT_HOSTS ne contient aucun hôte");
    }
    Ok(Some(OAuthSettings {
        issuer,
        web,
        client_hosts,
    }))
}

/// Vrai si `/mcp` accepte l'hôte de l'URL publique (sinon claude.ai recevrait 403).
fn mcp_accepts(issuer: &PublicUrl, allowed_hosts: &[String]) -> bool {
    let authority = issuer
        .as_str()
        .split_once("://")
        .map_or("", |(_, authority)| authority);
    let host = authority
        .rsplit_once(':')
        .filter(|(_, port)| port.bytes().all(|b| b.is_ascii_digit()))
        .map_or(authority, |(host, _)| host);
    let host = host.trim_start_matches('[').trim_end_matches(']');
    allowed_hosts.iter().any(|allowed| {
        let bare = allowed.trim_start_matches('[').trim_end_matches(']');
        bare.eq_ignore_ascii_case(host) || allowed.eq_ignore_ascii_case(authority)
    })
}

/// Vrai si le front, qui porte l'écran de consentement, a le droit d'appeler l'API.
fn cors_accepts(web: &PublicUrl, cors_origins: &str) -> bool {
    cors_origins
        .split(',')
        .any(|origin| origin.trim().trim_end_matches('/') == web.as_str())
}

/// Liste séparée par des virgules ; vide ou absente = boucle locale seulement.
fn allowed_hosts(value: Option<String>) -> Vec<String> {
    let hosts: Vec<String> = value
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|host| !host.is_empty())
        .map(str::to_string)
        .collect();
    if hosts.is_empty() {
        DEFAULT_ALLOWED_HOSTS.map(str::to_string).to_vec()
    } else {
        hosts
    }
}

/// Code facultatif lu dans l'environnement : espaces retirés, vide = absent.
fn optional_code(lookup: &impl Fn(&str) -> Option<String>, key: &str) -> Option<String> {
    lookup(key)
        .map(|code| code.trim().to_string())
        .filter(|code| !code.is_empty())
}

/// Code facultatif protégeant une route appelable sans session : trop court, il se devinerait.
fn optional_secret(
    lookup: &impl Fn(&str) -> Option<String>,
    key: &str,
    min_len: usize,
) -> anyhow::Result<Option<String>> {
    let code = optional_code(lookup, key);
    if code
        .as_ref()
        .is_some_and(|code| code.chars().count() < min_len)
    {
        anyhow::bail!(
            "{key} doit contenir au moins {min_len} caractères (ex. `openssl rand -hex 24`)"
        );
    }
    Ok(code)
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> anyhow::Result<Self> {
        let recovery_token = optional_secret(&lookup, "RECOVERY_TOKEN", MIN_RECOVERY_TOKEN_LEN)?;
        let cron_secret = optional_secret(&lookup, "CRON_SECRET", MIN_CRON_SECRET_LEN)?;
        Ok(Self {
            database_url: lookup("DATABASE_URL").context("DATABASE_URL est requis")?,
            jwt_secret: lookup("JWT_SECRET")
                .filter(|secret| !secret.is_empty())
                .context("JWT_SECRET est requis")?,
            cors_origins: lookup("CORS_ORIGINS").unwrap_or_default(),
            port: lookup("PORT")
                .and_then(|port| port.parse().ok())
                .unwrap_or(DEFAULT_PORT),
            setup_token: optional_code(&lookup, "SETUP_TOKEN"),
            recovery_token,
            cron_secret,
            eodhd_api_key: optional_code(&lookup, "EODHD_API_KEY"),
            trusted_proxy_hops: lookup("TRUSTED_PROXY_HOPS")
                .and_then(|hops| hops.trim().parse().ok())
                .unwrap_or(DEFAULT_TRUSTED_PROXY_HOPS),
            mcp_allowed_hosts: allowed_hosts(lookup("MCP_ALLOWED_HOSTS")),
            oauth: oauth_settings(&lookup)?,
        })
    }
}

/// Câble les adaptateurs réels (Postgres, fournisseurs de cours, argon2, JWT) sur les cas d'usage.
/// Aucun agrégateur bancaire n'est branché : il suffira d'ajouter son adaptateur à `bank_providers`.
/// `eodhd_api_key` : clé du fournisseur de repli des titres, absent = pas de repli.
pub async fn build_app(
    pool: PgPool,
    jwt_secret: &str,
    settings: Settings,
    eodhd_api_key: Option<&str>,
) -> anyhow::Result<Arc<App>> {
    let clients = Arc::new(HttpClientMetadata::new().context("client des métadonnées OAuth")?);
    assemble(pool, jwt_secret, settings, eodhd_api_key, clients).await
}

async fn assemble(
    pool: PgPool,
    jwt_secret: &str,
    settings: Settings,
    eodhd_api_key: Option<&str>,
    client_metadata: Arc<dyn ClientMetadataFetcher>,
) -> anyhow::Result<Arc<App>> {
    let store = Arc::new(PgStore::new(pool));
    let http = market::http_client().context("client HTTP")?;
    // Ordre d'essai : un fournisseur suivant ne reçoit que ce que les précédents n'ont pas su coter.
    let mut price_providers: Vec<Arc<dyn PriceProvider>> = vec![
        Arc::new(CoinGecko::new(http.clone(), market::COINGECKO_URL)),
        Arc::new(YahooFinance::new(http.clone(), market::YAHOO_URL)),
    ];
    if let Some(key) = eodhd_api_key {
        price_providers.push(Arc::new(Eodhd::new(http.clone(), market::EODHD_URL, key)));
    }
    let ports = Ports {
        users: store.clone(),
        second_factor: store.clone(),
        login_journal: store.clone(),
        access_tokens: store.clone(),
        mcp_calls: store.clone(),
        oauth: store.clone(),
        client_metadata,
        portfolio: store.clone(),
        fx_rates: store.clone(),
        transactions: store.clone(),
        budget: store.clone(),
        operations: store.clone(),
        snapshots: store.clone(),
        benchmarks: store.clone(),
        goals: store.clone(),
        connections: store.clone(),
        alerts: store.clone(),
        imports: store.clone(),
        import_profiles: store,
        clock: Arc::new(SystemClock),
        hasher: Arc::new(Argon2Hasher),
        tokens: Arc::new(JwtTokens::new(jwt_secret)),
        // Secret TOTP chiffré au repos avec une clé dérivée de JWT_SECRET.
        totp: Arc::new(TotpCrypto::new(jwt_secret)),
        access_crypto: Arc::new(AccessTokenDigest),
        fx_provider: Arc::new(Frankfurter::new(http.clone(), market::FRANKFURTER_URL)),
        price_providers,
        bank_providers: Vec::new(),
    };
    Ok(Arc::new(
        App::build(ports, settings)
            .await
            .context("initialisation de l'application")?,
    ))
}

/// Routeur HTTP complet au-dessus d'une base déjà migrée : l'API sous `/api` et le serveur MCP sur `/mcp`.
pub async fn build_router(
    pool: PgPool,
    jwt_secret: &str,
    cors_origins: &str,
    trusted_proxy_hops: usize,
    settings: Settings,
    eodhd_api_key: Option<&str>,
    mcp_allowed_hosts: Vec<String>,
) -> anyhow::Result<Router> {
    let app = build_app(pool, jwt_secret, settings, eodhd_api_key).await?;
    Ok(routers(
        app,
        cors_origins,
        trusted_proxy_hops,
        mcp_allowed_hosts,
    ))
}

/// Comme [`build_router`], avec une autre lecture des métadonnées des clients OAuth (tests : client simulé).
pub async fn build_router_with_clients(
    pool: PgPool,
    jwt_secret: &str,
    cors_origins: &str,
    settings: Settings,
    mcp_allowed_hosts: Vec<String>,
    client_metadata: Arc<dyn ClientMetadataFetcher>,
) -> anyhow::Result<Router> {
    let app = assemble(pool, jwt_secret, settings, None, client_metadata).await?;
    Ok(routers(
        app,
        cors_origins,
        DEFAULT_TRUSTED_PROXY_HOPS,
        mcp_allowed_hosts,
    ))
}

fn routers(
    app: Arc<App>,
    cors_origins: &str,
    trusted_proxy_hops: usize,
    mcp_allowed_hosts: Vec<String>,
) -> Router {
    let mcp = fineme_mcp::router(
        app.clone(),
        McpSettings {
            allowed_hosts: mcp_allowed_hosts,
            trusted_proxy_hops,
        },
    );
    mcp.merge(fineme_http::router(app, cors_origins, trusted_proxy_hops))
}

/// Démarre le serveur : connexion, migrations, écoute sur `0.0.0.0:$PORT` jusqu'à Ctrl-C.
pub async fn run(config: Config) -> anyhow::Result<()> {
    if config.jwt_secret.len() < 32 {
        tracing::warn!(
            "JWT_SECRET est court : utilisez au moins 32 caractères aléatoires en production"
        );
    }
    if config.setup_token.is_none() {
        tracing::warn!(
            "SETUP_TOKEN non défini : la première configuration n'est pas protégée (à définir en production)"
        );
    }
    let pool = postgres::connect(&config.database_url).await?;
    postgres::migrate(&pool).await?;
    tracing::info!("migrations appliquées");

    if config.recovery_token.is_none() {
        tracing::info!(
            "RECOVERY_TOKEN non défini : la réinitialisation d'un mot de passe perdu est désactivée"
        );
    }
    if config.cron_secret.is_none() {
        tracing::info!(
            "CRON_SECRET non défini : la tâche quotidienne (cours du matin, instantané) est désactivée"
        );
    }
    if config.eodhd_api_key.is_none() {
        tracing::info!("EODHD_API_KEY non défini : pas de fournisseur de repli pour les titres");
    }
    if config
        .oauth
        .as_ref()
        .is_some_and(|oauth| !cors_accepts(&oauth.web, &config.cors_origins))
    {
        tracing::warn!(
            "CORS_ORIGINS ne contient pas WEB_URL : l'écran de consentement ne pourra pas appeler l'API"
        );
    }
    match &config.oauth {
        None => {
            tracing::info!(
                "PUBLIC_API_URL et WEB_URL non définis : le connecteur OAuth est désactivé"
            );
        }
        Some(oauth) if !mcp_accepts(&oauth.issuer, &config.mcp_allowed_hosts) => {
            tracing::warn!(
                "MCP_ALLOWED_HOSTS ne contient pas l'hôte de PUBLIC_API_URL : /mcp refusera le connecteur"
            );
        }
        Some(_) => {}
    }
    let settings = Settings {
        setup_token: config.setup_token.clone(),
        recovery_token: config.recovery_token.clone(),
        cron_secret: config.cron_secret.clone(),
        oauth: config.oauth.clone(),
    };
    let router = build_router(
        pool,
        &config.jwt_secret,
        &config.cors_origins,
        config.trusted_proxy_hops,
        settings,
        config.eodhd_api_key.as_deref(),
        config.mcp_allowed_hosts.clone(),
    )
    .await?;
    let address = SocketAddr::from(([0, 0, 0, 0], config.port));
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("écoute sur {address} impossible"))?;
    tracing::info!("FineMe API à l'écoute sur http://{address}");
    // L'adresse TCP du client sert de repli quand `X-Forwarded-For` est absent (usage local).
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        tokio::signal::ctrl_c().await.ok();
    })
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| {
            vars.iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| value.to_string())
        }
    }

    #[test]
    fn config_requires_database_and_secret() {
        assert!(Config::from_lookup(lookup(&[("JWT_SECRET", "s")])).is_err());
        assert!(Config::from_lookup(lookup(&[("DATABASE_URL", "postgres://x")])).is_err());
        assert!(
            Config::from_lookup(lookup(&[
                ("DATABASE_URL", "postgres://x"),
                ("JWT_SECRET", "")
            ]))
            .is_err()
        );
    }

    #[test]
    fn setup_token_is_optional_and_blank_means_unset() {
        let base = [("DATABASE_URL", "postgres://x"), ("JWT_SECRET", "secret")];
        assert_eq!(
            Config::from_lookup(lookup(&base)).unwrap().setup_token,
            None
        );
        let blank = [base[0], base[1], ("SETUP_TOKEN", "  ")];
        assert_eq!(
            Config::from_lookup(lookup(&blank)).unwrap().setup_token,
            None
        );
        let set = [base[0], base[1], ("SETUP_TOKEN", " code-fictif ")];
        assert_eq!(
            Config::from_lookup(lookup(&set))
                .unwrap()
                .setup_token
                .as_deref(),
            Some("code-fictif")
        );
    }

    #[test]
    fn recovery_token_is_optional_and_must_be_long_enough() {
        let base = [("DATABASE_URL", "postgres://x"), ("JWT_SECRET", "secret")];
        let read = |value: &'static str| {
            Config::from_lookup(lookup(&[base[0], base[1], ("RECOVERY_TOKEN", value)]))
        };
        assert_eq!(
            Config::from_lookup(lookup(&base)).unwrap().recovery_token,
            None
        );
        assert_eq!(read("   ").unwrap().recovery_token, None);
        assert!(read("trop-court-123").is_err());
        assert_eq!(
            read(" code-de-secours-fictif ")
                .unwrap()
                .recovery_token
                .as_deref(),
            Some("code-de-secours-fictif")
        );
    }

    #[test]
    fn cron_secret_must_be_long_enough_and_eodhd_key_is_optional() {
        let base = [("DATABASE_URL", "postgres://x"), ("JWT_SECRET", "secret")];
        let config = Config::from_lookup(lookup(&base)).unwrap();
        assert_eq!((config.cron_secret, config.eodhd_api_key), (None, None));
        let short = [base[0], base[1], ("CRON_SECRET", "court")];
        assert!(Config::from_lookup(lookup(&short)).is_err());
        let set = [
            base[0],
            base[1],
            ("CRON_SECRET", " secret-quotidien-fictif "),
            ("EODHD_API_KEY", " cle-fictive "),
        ];
        let config = Config::from_lookup(lookup(&set)).unwrap();
        assert_eq!(
            config.cron_secret.as_deref(),
            Some("secret-quotidien-fictif")
        );
        assert_eq!(config.eodhd_api_key.as_deref(), Some("cle-fictive"));
    }

    #[test]
    fn mcp_hosts_default_to_loopback_and_are_listed_by_commas() {
        let vars = [("DATABASE_URL", "postgres://x"), ("JWT_SECRET", "s")];
        let config = Config::from_lookup(lookup(&vars)).unwrap();
        assert_eq!(config.mcp_allowed_hosts, ["localhost", "127.0.0.1", "::1"]);
        let vars = [
            ("DATABASE_URL", "postgres://x"),
            ("JWT_SECRET", "s"),
            (
                "MCP_ALLOWED_HOSTS",
                " fineme-api.onrender.com , localhost:8080 ,",
            ),
        ];
        let config = Config::from_lookup(lookup(&vars)).unwrap();
        assert_eq!(
            config.mcp_allowed_hosts,
            ["fineme-api.onrender.com", "localhost:8080"]
        );
    }

    #[test]
    fn oauth_needs_both_urls_and_exact_client_hosts() {
        let load = |extra: &[(&str, &str)]| {
            let mut vars = vec![("DATABASE_URL", "postgres://x"), ("JWT_SECRET", "secret")];
            vars.extend_from_slice(extra);
            Config::from_lookup(lookup(&vars))
        };
        assert!(load(&[]).unwrap().oauth.is_none());
        let api = ("PUBLIC_API_URL", "https://api.example.test");
        let web = ("WEB_URL", "https://web.example.test");
        let oauth = load(&[api, web]).unwrap().oauth.unwrap();
        assert_eq!(oauth.issuer.as_str(), "https://api.example.test");
        assert_eq!(oauth.client_hosts[0].as_str(), "claude.ai");
        let hosts = ("OAUTH_CLIENT_HOSTS", "Claude.ai, example.test");
        let listed = load(&[api, web, hosts]).unwrap().oauth.unwrap();
        assert_eq!(
            listed
                .client_hosts
                .iter()
                .map(ClientHost::as_str)
                .collect::<Vec<_>>(),
            ["claude.ai", "example.test"]
        );
        for bad in [
            "*.claude.ai",
            "https://claude.ai",
            "claude.ai:443",
            "127.0.0.1",
            " , ",
        ] {
            let hosts = ("OAUTH_CLIENT_HOSTS", bad);
            assert!(load(&[api, web, hosts]).is_err(), "{bad}");
        }
        assert!(load(&[api]).is_err());
        assert!(load(&[web]).is_err());
        for bad in ["http://api.example.test", "https://api.example.test/"] {
            assert!(load(&[("PUBLIC_API_URL", bad), web]).is_err(), "{bad}");
        }
    }

    #[test]
    fn mcp_hosts_must_cover_the_public_url() {
        let issuer = PublicUrl::parse("https://api.example.test").unwrap();
        assert!(mcp_accepts(&issuer, &["api.example.test".into()]));
        assert!(!mcp_accepts(&issuer, &["localhost".into()]));
        let local = PublicUrl::parse("http://localhost:8080").unwrap();
        assert!(mcp_accepts(&local, &["localhost".into()]));
        assert!(mcp_accepts(&local, &["localhost:8080".into()]));
        assert!(!mcp_accepts(&local, &["localhost:9000".into()]));
        let ipv6 = PublicUrl::parse("http://[::1]:8080");
        if let Ok(ipv6) = ipv6 {
            assert!(mcp_accepts(&ipv6, &["::1".into()]));
            assert!(mcp_accepts(&ipv6, &["[::1]".into()]));
        }
    }

    #[test]
    fn cors_must_cover_the_web_url() {
        let web = PublicUrl::parse("https://web.example.test").unwrap();
        assert!(cors_accepts(
            &web,
            "http://localhost:5173, https://web.example.test/"
        ));
        assert!(!cors_accepts(&web, "https://autre.example.test"));
    }

    #[test]
    fn config_defaults_port_and_cors() {
        let vars = [("DATABASE_URL", "postgres://x"), ("JWT_SECRET", "secret")];
        let config = Config::from_lookup(lookup(&vars)).unwrap();
        assert_eq!(
            (config.port, config.cors_origins.as_str()),
            (DEFAULT_PORT, "")
        );
        assert_eq!(config.trusted_proxy_hops, DEFAULT_TRUSTED_PROXY_HOPS);
        let vars = [
            ("DATABASE_URL", "postgres://x"),
            ("JWT_SECRET", "s"),
            ("TRUSTED_PROXY_HOPS", " 0 "),
        ];
        assert_eq!(
            Config::from_lookup(lookup(&vars))
                .unwrap()
                .trusted_proxy_hops,
            0
        );
        let vars = [
            ("DATABASE_URL", "postgres://x"),
            ("JWT_SECRET", "s"),
            ("PORT", "9000"),
            ("CORS_ORIGINS", "http://a"),
        ];
        let config = Config::from_lookup(lookup(&vars)).unwrap();
        assert_eq!(
            (config.port, config.cors_origins.as_str()),
            (9000, "http://a")
        );
        let vars = [
            ("DATABASE_URL", "postgres://x"),
            ("JWT_SECRET", "s"),
            ("PORT", "pas-un-port"),
        ];
        assert_eq!(
            Config::from_lookup(lookup(&vars)).unwrap().port,
            DEFAULT_PORT
        );
    }
}
