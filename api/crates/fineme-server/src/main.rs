use fineme_server::Config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "fineme=info,fineme_server=info,fineme_http=info,fineme_application=info,fineme_infra=info,tower_http=info,sqlx=warn".into()),
        )
        .init();
    fineme_server::run(Config::from_env()?).await
}
