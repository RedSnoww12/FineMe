//! Clients HTTP des fournisseurs de cours et de change. Les URL de base sont injectées
//! (tests contre un serveur simulé).

mod coingecko;
mod eodhd;
mod frankfurter;
mod yahoo;

use std::time::Duration;

pub use coingecko::CoinGecko;
pub use eodhd::Eodhd;
use fineme_application::ports::ProviderError;
pub use frankfurter::Frankfurter;
pub use yahoo::YahooFinance;

pub const COINGECKO_URL: &str = "https://api.coingecko.com/api/v3";
pub const YAHOO_URL: &str = "https://query1.finance.yahoo.com";
pub const FRANKFURTER_URL: &str = "https://api.frankfurter.app";
pub const EODHD_URL: &str = "https://eodhd.com/api";
/// Yahoo refuse les clients sans User-Agent de navigateur.
const USER_AGENT: &str = "Mozilla/5.0";

/// Client HTTP partagé, avec délais courts : un fournisseur lent ne doit pas bloquer la requête.
pub fn http_client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .build()
}

/// Certaines places cotent en centièmes (« GBp », « ZAc » chez Yahoo, « GBX » chez EODHD) :
/// ramène le cours à l'unité principale de la devise.
fn normalize_quote(price: f64, currency: &str) -> (f64, String) {
    let upper = currency.to_ascii_uppercase();
    if currency.len() == 3 && currency != upper {
        (price / 100.0, upper)
    } else if upper == "GBX" {
        (price / 100.0, "GBP".to_string())
    } else {
        (price, upper)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_pence_quotes() {
        assert_eq!(normalize_quote(1234.0, "GBp"), (12.34, "GBP".to_string()));
        assert_eq!(normalize_quote(1234.0, "GBX"), (12.34, "GBP".to_string()));
        assert_eq!(normalize_quote(50.0, "USD"), (50.0, "USD".to_string()));
    }
}
