//! Clients HTTP des fournisseurs de cours et de change. Les URL de base sont injectées
//! (tests contre un serveur simulé).

mod coingecko;
mod frankfurter;
mod yahoo;

use std::time::Duration;

pub use coingecko::CoinGecko;
use fineme_application::ports::ProviderError;
pub use frankfurter::Frankfurter;
pub use yahoo::YahooFinance;

pub const COINGECKO_URL: &str = "https://api.coingecko.com/api/v3";
pub const YAHOO_URL: &str = "https://query1.finance.yahoo.com";
pub const FRANKFURTER_URL: &str = "https://api.frankfurter.app";
/// Yahoo refuse les clients sans User-Agent de navigateur.
const USER_AGENT: &str = "Mozilla/5.0";

/// Client HTTP partagé, avec délais courts : un fournisseur lent ne doit pas bloquer la requête.
pub fn http_client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .build()
}

async fn get_json<T: serde::de::DeserializeOwned>(
    http: &reqwest::Client,
    url: &str,
    query: &[(&str, &str)],
) -> Result<T, ProviderError> {
    let url = reqwest::Url::parse_with_params(url, query)
        .map_err(|e| ProviderError(format!("URL invalide : {e}")))?;
    let response = http
        .get(url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|e| ProviderError(format!("service injoignable : {}", e.without_url())))?;
    if !response.status().is_success() {
        return Err(ProviderError(format!(
            "réponse HTTP {}",
            response.status().as_u16()
        )));
    }
    response
        .json::<T>()
        .await
        .map_err(|_| ProviderError("réponse illisible".to_string()))
}
