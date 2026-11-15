//! Composition root : lit la configuration, câble les adaptateurs sur les ports et construit le routeur.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use axum::Router;
use fineme_application::{App, Ports};
use fineme_infra::clock::SystemClock;
use fineme_infra::market::{self, CoinGecko, Frankfurter, YahooFinance};
use fineme_infra::postgres::{self, PgStore};
use fineme_infra::security::{Argon2Hasher, JwtTokens};
use sqlx::PgPool;

pub const DEFAULT_PORT: u16 = 8080;

/// Configuration lue dans l'environnement (`DATABASE_URL`, `JWT_SECRET`, `CORS_ORIGINS`, `PORT`).
#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    pub cors_origins: String,
    pub port: u16,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> anyhow::Result<Self> {
        Ok(Self {
            database_url: lookup("DATABASE_URL").context("DATABASE_URL est requis")?,
            jwt_secret: lookup("JWT_SECRET")
                .filter(|secret| !secret.is_empty())
                .context("JWT_SECRET est requis")?,
            cors_origins: lookup("CORS_ORIGINS").unwrap_or_default(),
            port: lookup("PORT")
                .and_then(|port| port.parse().ok())
                .unwrap_or(DEFAULT_PORT),
        })
    }
}

/// Câble les adaptateurs réels (Postgres, fournisseurs de cours, argon2, JWT) sur les cas d'usage.
/// Aucun agrégateur bancaire n'est branché : il suffira d'ajouter son adaptateur à `bank_providers`.
pub async fn build_app(pool: PgPool, jwt_secret: &str) -> anyhow::Result<Arc<App>> {
    let store = Arc::new(PgStore::new(pool));
    let http = market::http_client().context("client HTTP")?;
    let ports = Ports {
        users: store.clone(),
        portfolio: store.clone(),
        fx_rates: store.clone(),
        transactions: store.clone(),
        snapshots: store.clone(),
        goals: store.clone(),
        connections: store.clone(),
        imports: store,
        clock: Arc::new(SystemClock),
        hasher: Arc::new(Argon2Hasher),
        tokens: Arc::new(JwtTokens::new(jwt_secret)),
        fx_provider: Arc::new(Frankfurter::new(http.clone(), market::FRANKFURTER_URL)),
        price_providers: vec![
            Arc::new(CoinGecko::new(http.clone(), market::COINGECKO_URL)),
            Arc::new(YahooFinance::new(http, market::YAHOO_URL)),
        ],
        bank_providers: Vec::new(),
    };
    Ok(Arc::new(
        App::build(ports)
            .await
            .context("initialisation de l'application")?,
    ))
}

/// Routeur HTTP complet au-dessus d'une base déjà migrée.
pub async fn build_router(
    pool: PgPool,
    jwt_secret: &str,
    cors_origins: &str,
) -> anyhow::Result<Router> {
    Ok(fineme_http::router(
        build_app(pool, jwt_secret).await?,
        cors_origins,
    ))
}

/// Démarre le serveur : connexion, migrations, écoute sur `0.0.0.0:$PORT` jusqu'à Ctrl-C.
pub async fn run(config: Config) -> anyhow::Result<()> {
    if config.jwt_secret.len() < 32 {
        tracing::warn!(
            "JWT_SECRET est court : utilisez au moins 32 caractères aléatoires en production"
        );
    }
    let pool = postgres::connect(&config.database_url).await?;
    postgres::migrate(&pool).await?;
    tracing::info!("migrations appliquées");

    let router = build_router(pool, &config.jwt_secret, &config.cors_origins).await?;
    let address = SocketAddr::from(([0, 0, 0, 0], config.port));
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("écoute sur {address} impossible"))?;
    tracing::info!("FineMe API à l'écoute sur http://{address}");
    axum::serve(listener, router)
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
    fn config_defaults_port_and_cors() {
        let vars = [("DATABASE_URL", "postgres://x"), ("JWT_SECRET", "secret")];
        let config = Config::from_lookup(lookup(&vars)).unwrap();
        assert_eq!(
            (config.port, config.cors_origins.as_str()),
            (DEFAULT_PORT, "")
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
