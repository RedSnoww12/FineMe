use std::collections::HashMap;

use async_trait::async_trait;
use fineme_application::ports::{PriceProvider, ProviderError, Quote, QuoteRequest};
use fineme_domain::fx::Currency;
use fineme_domain::holding::HoldingKind;
use fineme_domain::ids::HoldingId;
use serde::Deserialize;

use super::get_json;

/// Cours des titres via Yahoo Finance : recherche par ISIN ou ticker, puis dernier cours.
/// Le cours est renvoyé dans la devise de cotation ; la conversion revient à l'appelant.
pub struct YahooFinance {
    http: reqwest::Client,
    base_url: String,
}

#[derive(Deserialize)]
struct Search {
    #[serde(default)]
    quotes: Vec<SearchQuote>,
}

#[derive(Deserialize)]
struct SearchQuote {
    symbol: Option<String>,
}

#[derive(Deserialize)]
struct Chart {
    chart: ChartBody,
}

#[derive(Deserialize)]
struct ChartBody {
    result: Option<Vec<ChartResult>>,
}

#[derive(Deserialize)]
struct ChartResult {
    meta: ChartMeta,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChartMeta {
    currency: Option<String>,
    regular_market_price: Option<f64>,
}

/// Yahoo cote certaines places en centièmes (« GBp », « ZAc ») : ramène à l'unité principale.
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

/// Extrait (cours, devise) d'une réponse `v8/finance/chart`.
fn parse_chart(chart: Chart) -> Option<Quote> {
    let meta = chart.chart.result?.into_iter().next()?.meta;
    let price = meta
        .regular_market_price
        .filter(|price| price.is_finite() && *price > 0.0)?;
    let (price, currency) = normalize_quote(price, meta.currency.as_deref().unwrap_or("EUR"));
    Some(Quote {
        price,
        currency: Currency::parse(&currency).ok()?,
    })
}

impl YahooFinance {
    pub fn new(http: reqwest::Client, base_url: impl Into<String>) -> Self {
        Self {
            http,
            base_url: base_url.into().trim_end_matches('/').to_string(),
        }
    }

    async fn chart(&self, ticker: &str) -> Result<Quote, ProviderError> {
        let url = format!("{}/v8/finance/chart/{ticker}", self.base_url);
        let chart: Chart =
            get_json(&self.http, &url, &[("interval", "1d"), ("range", "1d")]).await?;
        parse_chart(chart)
            .ok_or_else(|| ProviderError(format!("pas de cours pour {ticker} sur Yahoo Finance")))
    }

    async fn search(&self, query: &str) -> Result<String, ProviderError> {
        let url = format!("{}/v1/finance/search", self.base_url);
        let found: Search = get_json(
            &self.http,
            &url,
            &[("q", query), ("quotesCount", "5"), ("newsCount", "0")],
        )
        .await?;
        found
            .quotes
            .into_iter()
            .find_map(|quote| quote.symbol)
            .ok_or_else(|| ProviderError(format!("{query} introuvable sur Yahoo Finance")))
    }

    /// Recherche par ISIN, sinon ticker direct, puis recherche par ticker.
    async fn quote(
        &self,
        isin: Option<&str>,
        symbol: Option<&str>,
    ) -> Result<Quote, ProviderError> {
        let mut last_error = ProviderError("ni ISIN ni symbole".to_string());
        if let Some(isin) = isin {
            match self.search(isin).await {
                Ok(ticker) => match self.chart(&ticker).await {
                    Ok(quote) => return Ok(quote),
                    Err(error) => last_error = error,
                },
                Err(error) => last_error = error,
            }
        }
        if let Some(symbol) = symbol {
            match self.chart(symbol).await {
                Ok(quote) => return Ok(quote),
                Err(error) => last_error = error,
            }
            if let Ok(ticker) = self.search(symbol).await
                && !ticker.eq_ignore_ascii_case(symbol)
            {
                match self.chart(&ticker).await {
                    Ok(quote) => return Ok(quote),
                    Err(error) => last_error = error,
                }
            }
        }
        Err(last_error)
    }
}

#[async_trait]
impl PriceProvider for YahooFinance {
    fn handles(&self, kind: HoldingKind) -> bool {
        kind == HoldingKind::Security
    }

    async fn quotes(
        &self,
        requests: &[QuoteRequest],
    ) -> HashMap<HoldingId, Result<Quote, ProviderError>> {
        // Une seule résolution par couple (ISIN, symbole).
        let mut resolved: HashMap<(Option<&str>, Option<&str>), Result<Quote, ProviderError>> =
            HashMap::new();
        let mut results = HashMap::new();
        for request in requests {
            let key = (request.isin.as_deref(), request.symbol.as_deref());
            let outcome = match resolved.get(&key) {
                Some(outcome) => outcome.clone(),
                None => {
                    let outcome = self.quote(key.0, key.1).await;
                    resolved.insert(key, outcome.clone());
                    outcome
                }
            };
            results.insert(request.holding_id, outcome);
        }
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chart_and_rejects_empty_results() {
        let body = r#"{"chart":{"result":[{"meta":{"currency":"USD","symbol":"DEMO","regularMarketPrice":356.27},
            "timestamp":[1],"indicators":{"quote":[{"close":[356.2]}]}}],"error":null}}"#;
        let quote = parse_chart(serde_json::from_str(body).unwrap()).unwrap();
        assert_eq!((quote.price, quote.currency.as_str()), (356.27, "USD"));
        let empty = r#"{"chart":{"result":null,"error":{"code":"Not Found"}}}"#;
        assert!(parse_chart(serde_json::from_str(empty).unwrap()).is_none());
    }

    #[test]
    fn normalizes_pence_quotes() {
        assert_eq!(normalize_quote(1234.0, "GBp"), (12.34, "GBP".to_string()));
        assert_eq!(normalize_quote(1234.0, "GBX"), (12.34, "GBP".to_string()));
        assert_eq!(normalize_quote(50.0, "USD"), (50.0, "USD".to_string()));
    }
}
