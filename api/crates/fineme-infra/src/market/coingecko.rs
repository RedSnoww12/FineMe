use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use fineme_application::ports::{PriceProvider, ProviderError, Quote, QuoteRequest};
use fineme_domain::holding::HoldingKind;
use fineme_domain::ids::HoldingId;
use serde::Deserialize;

use super::get_json;

/// Identifiants CoinGecko des cryptos courantes (évite une recherche par symbole).
const KNOWN_COINS: [(&str, &str); 12] = [
    ("BTC", "bitcoin"),
    ("ETH", "ethereum"),
    ("SOL", "solana"),
    ("BNB", "binancecoin"),
    ("XRP", "ripple"),
    ("ADA", "cardano"),
    ("DOGE", "dogecoin"),
    ("DOT", "polkadot"),
    ("LTC", "litecoin"),
    ("LINK", "chainlink"),
    ("USDT", "tether"),
    ("USDC", "usd-coin"),
];

/// Cours des cryptos via CoinGecko, par symbole, directement dans la devise de la position.
pub struct CoinGecko {
    http: reqwest::Client,
    base_url: String,
}

#[derive(Deserialize)]
struct CoinSearch {
    #[serde(default)]
    coins: Vec<Coin>,
}

#[derive(Deserialize)]
struct Coin {
    id: String,
    symbol: String,
    market_cap_rank: Option<u32>,
}

/// Parmi les résultats de recherche, la crypto du bon symbole la mieux classée.
fn pick_coin(coins: &[Coin], symbol: &str) -> Option<String> {
    coins
        .iter()
        .filter(|coin| coin.symbol.eq_ignore_ascii_case(symbol))
        .min_by_key(|coin| coin.market_cap_rank.unwrap_or(u32::MAX))
        .map(|coin| coin.id.clone())
}

impl CoinGecko {
    pub fn new(http: reqwest::Client, base_url: impl Into<String>) -> Self {
        Self {
            http,
            base_url: base_url.into().trim_end_matches('/').to_string(),
        }
    }

    async fn coin_id(&self, symbol: &str) -> Result<String, ProviderError> {
        if let Some((_, id)) = KNOWN_COINS
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(symbol))
        {
            return Ok(id.to_string());
        }
        let found: CoinSearch = get_json(
            &self.http,
            &format!("{}/search", self.base_url),
            &[("query", symbol)],
        )
        .await?;
        pick_coin(&found.coins, symbol)
            .ok_or_else(|| ProviderError(format!("crypto {symbol} introuvable sur CoinGecko")))
    }
}

#[async_trait]
impl PriceProvider for CoinGecko {
    fn handles(&self, kind: HoldingKind) -> bool {
        kind == HoldingKind::Crypto
    }

    async fn quotes(
        &self,
        requests: &[QuoteRequest],
    ) -> HashMap<HoldingId, Result<Quote, ProviderError>> {
        let mut results = HashMap::new();
        let mut ids: HashMap<&str, Result<String, ProviderError>> = HashMap::new();
        for request in requests {
            match request.symbol.as_deref() {
                None => {
                    results.insert(
                        request.holding_id,
                        Err(ProviderError("symbole manquant".to_string())),
                    );
                }
                Some(symbol) if !ids.contains_key(symbol) => {
                    ids.insert(symbol, self.coin_id(symbol).await);
                }
                Some(_) => {}
            }
        }
        let wanted: HashSet<&str> = ids.values().filter_map(|id| id.as_deref().ok()).collect();
        let currencies: HashSet<String> = requests
            .iter()
            .map(|r| r.currency.as_str().to_ascii_lowercase())
            .collect();
        let prices: Result<HashMap<String, HashMap<String, f64>>, ProviderError> =
            if wanted.is_empty() {
                Ok(HashMap::new())
            } else {
                let ids_param = wanted.into_iter().collect::<Vec<_>>().join(",");
                let currencies_param = currencies.into_iter().collect::<Vec<_>>().join(",");
                let url = format!("{}/simple/price", self.base_url);
                get_json(
                    &self.http,
                    &url,
                    &[("ids", &ids_param), ("vs_currencies", &currencies_param)],
                )
                .await
            };
        for request in requests {
            let Some(symbol) = request.symbol.as_deref() else {
                continue;
            };
            let outcome = match (&ids[symbol], &prices) {
                (Err(reason), _) | (_, Err(reason)) => Err(reason.clone()),
                (Ok(id), Ok(prices)) => prices
                    .get(id)
                    .and_then(|by_currency| {
                        by_currency.get(&request.currency.as_str().to_ascii_lowercase())
                    })
                    .map(|price| Quote {
                        price: *price,
                        currency: request.currency.clone(),
                    })
                    .ok_or_else(|| {
                        ProviderError(format!(
                            "pas de cours {} pour {symbol} sur CoinGecko",
                            request.currency
                        ))
                    }),
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
    fn picks_best_ranked_coin_with_exact_symbol() {
        let body = r#"{"coins":[{"id":"wrapped-demo","name":"x","symbol":"WDEMO","market_cap_rank":267},
            {"id":"fake-demo","name":"y","symbol":"DEMO","market_cap_rank":null},
            {"id":"demo-coin","name":"Demo","symbol":"DEMO","market_cap_rank":1}]}"#;
        let found: CoinSearch = serde_json::from_str(body).unwrap();
        assert_eq!(
            pick_coin(&found.coins, "demo").as_deref(),
            Some("demo-coin")
        );
        assert_eq!(pick_coin(&found.coins, "ZZZ"), None);
    }
}
