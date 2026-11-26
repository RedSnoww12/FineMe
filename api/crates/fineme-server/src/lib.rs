//! Composition root : lit la configuration, câble les adaptateurs sur les ports et construit le routeur.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use axum::Router;
use fineme_application::auth::MIN_RECOVERY_TOKEN_LEN;
use fineme_application::daily::MIN_CRON_SECRET_LEN;
use fineme_application::ports::PriceProvider;
use fineme_application::{App, Ports, Settings};
use fineme_infra::access_token::AccessTokenDigest;
use fineme_infra::clock::SystemClock;
use fineme_infra::market::{self, CoinGecko, Eodhd, Frankfurter, YahooFinance};
use fineme_infra::postgres::{self, PgStore};
use fineme_infra::security::{Argon2Hasher, JwtTokens};
use fineme_infra::totp::TotpCrypto;
use sqlx::PgPool;

pub const DEFAULT_PORT: u16 = 8080;
/// Render place un proxy devant le service : sa dernière entrée de `X-Forwarded-For` est l'IP du client.
pub const DEFAULT_TRUSTED_PROXY_HOPS: usize = 1;

/// Configuration lue dans l'environnement (`DATABASE_URL`, `JWT_SECRET`, `CORS_ORIGINS`, `PORT`, `SETUP_TOKEN`,
/// `RECOVERY_TOKEN`, `CRON_SECRET`, `EODHD_API_KEY`, `TRUSTED_PROXY_HOPS`).
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

/// Routeur HTTP complet au-dessus d'une base déjà migrée.
pub async fn build_router(
    pool: PgPool,
    jwt_secret: &str,
    cors_origins: &str,
    trusted_proxy_hops: usize,
    settings: Settings,
    eodhd_api_key: Option<&str>,
) -> anyhow::Result<Router> {
    Ok(fineme_http::router(
        build_app(pool, jwt_secret, settings, eodhd_api_key).await?,
        cors_origins,
        trusted_proxy_hops,
    ))
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
    let settings = Settings {
        setup_token: config.setup_token.clone(),
        recovery_token: config.recovery_token.clone(),
        cron_secret: config.cron_secret.clone(),
    };
    let router = build_router(
        pool,
        &config.jwt_secret,
        &config.cors_origins,
        config.trusted_proxy_hops,
        settings,
        config.eodhd_api_key.as_deref(),
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
