use std::collections::HashMap;

use async_trait::async_trait;
use fineme_application::ports::{PriceProvider, ProviderError, Quote, QuoteRequest};
use fineme_domain::fx::Currency;
use fineme_domain::holding::HoldingKind;
use fineme_domain::ids::HoldingId;
use serde::Deserialize;
use serde_json::Value;

use super::{get_json, normalize_quote};

/// Correspondance entre le suffixe d'un ticker au format Yahoo (« CW8.PA ») et le code de place EODHD.
/// Sert seulement à préférer la bonne cotation quand un titre est coté sur plusieurs places.
const YAHOO_TO_EODHD_EXCHANGE: [(&str, &str); 12] = [
    ("PA", "PA"),
    ("AS", "AS"),
    ("BR", "BR"),
    ("LS", "LS"),
    ("MI", "MI"),
    ("MC", "MC"),
    ("SW", "SW"),
    ("VI", "VI"),
    ("TO", "TO"),
    ("F", "F"),
    ("DE", "XETRA"),
    ("L", "LSE"),
];

/// Cours des titres via EODHD (eodhd.com), fournisseur de repli quand Yahoo Finance échoue.
/// Recherche par ISIN ou ticker ; le dernier cours de clôture est renvoyé dans la devise de cotation.
/// Une clé d'API est nécessaire (offre gratuite limitée en nombre d'appels par jour) : une recherche par titre,
/// plus un appel de cours si la recherche ne donne pas la dernière clôture.
pub struct Eodhd {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
}

/// Cotation renvoyée par l'API de recherche.
#[derive(Debug, Clone, Deserialize)]
struct Listing {
    #[serde(rename = "Code")]
    code: String,
    #[serde(rename = "Exchange")]
    exchange: String,
    #[serde(rename = "Currency")]
    currency: Option<String>,
    #[serde(rename = "ISIN")]
    isin: Option<String>,
    #[serde(rename = "isPrimary", default)]
    is_primary: bool,
    /// Nombre, ou chaîne (« NA ») quand la clôture est inconnue.
    #[serde(rename = "previousClose")]
    previous_close: Option<Value>,
}

impl Listing {
    fn ticker(&self) -> String {
        format!("{}.{}", self.code, self.exchange)
    }
}

/// Ce que l'on cherche : un ISIN exact ou un code de ticker, de préférence sur une place et dans une devise.
struct Wanted<'a> {
    isin: Option<&'a str>,
    code: Option<&'a str>,
    exchange: Option<&'a str>,
    currency: &'a str,
}

/// Cours positif et fini, donné en nombre ou en chaîne numérique.
fn positive_number(value: &Value) -> Option<f64> {
    let number = match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }?;
    (number.is_finite() && number > 0.0).then_some(number)
}

/// Ticker au format Yahoo (« CW8.PA ») : code (« CW8 ») et place EODHD si le suffixe est connu.
/// Sans suffixe, le titre est supposé américain.
fn split_ticker(symbol: &str) -> (&str, Option<&'static str>) {
    match symbol.rsplit_once('.') {
        Some((code, suffix)) => {
            let exchange = YAHOO_TO_EODHD_EXCHANGE
                .iter()
                .find(|(yahoo, _)| yahoo.eq_ignore_ascii_case(suffix))
                .map(|(_, eodhd)| *eodhd);
            (code, exchange)
        }
        None => (symbol, Some("US")),
    }
}

/// Parmi les résultats de recherche, la cotation qui correspond (même ISIN ou même code), en préférant
/// la place demandée, puis la devise de la position, puis la cotation principale.
fn pick_listing<'a>(listings: &'a [Listing], wanted: &Wanted) -> Option<&'a Listing> {
    let same = |a: &str, b: Option<&str>| b.is_some_and(|b| a.eq_ignore_ascii_case(b));
    listings
        .iter()
        .filter(|listing| match (wanted.isin, wanted.code) {
            (Some(isin), _) => same(isin, listing.isin.as_deref()),
            (None, Some(code)) => listing.code.eq_ignore_ascii_case(code),
            (None, None) => false,
        })
        .min_by_key(|listing| {
            let currency = listing
                .currency
                .as_deref()
                .map(|currency| normalize_quote(1.0, currency).1);
            (
                wanted.exchange.is_some() && !same(&listing.exchange, wanted.exchange),
                currency.as_deref() != Some(wanted.currency),
                !listing.is_primary,
            )
        })
}

impl Eodhd {
    pub fn new(
        http: reqwest::Client,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
        Self {
            http,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
        }
    }

    /// URL `base/<section>/<valeur>`, la valeur étant encodée comme segment de chemin.
    fn url(&self, section: &str, value: &str) -> Result<String, ProviderError> {
        let mut url = reqwest::Url::parse(&self.base_url)
            .map_err(|e| ProviderError(format!("URL invalide : {e}")))?;
        url.path_segments_mut()
            .map_err(|_| ProviderError("URL invalide".to_string()))?
            .push(section)
            .push(value);
        Ok(url.into())
    }

    async fn search(&self, query: &str, wanted: &Wanted<'_>) -> Result<Listing, ProviderError> {
        let listings: Vec<Listing> = get_json(
            &self.http,
            &self.url("search", query)?,
            &[
                ("api_token", &self.api_key),
                ("fmt", "json"),
                ("limit", "10"),
            ],
        )
        .await?;
        pick_listing(&listings, wanted)
            .cloned()
            .ok_or_else(|| ProviderError(format!("{query} introuvable sur EODHD")))
    }

    async fn last_close(&self, ticker: &str) -> Result<f64, ProviderError> {
        let close: Value = get_json(
            &self.http,
            &self.url("eod", ticker)?,
            &[
                ("api_token", &self.api_key),
                ("fmt", "json"),
                ("filter", "last_close"),
            ],
        )
        .await?;
        positive_number(&close)
            .ok_or_else(|| ProviderError(format!("pas de cours pour {ticker} sur EODHD")))
    }

    async fn price(&self, listing: &Listing) -> Result<Quote, ProviderError> {
        let ticker = listing.ticker();
        let currency = listing
            .currency
            .as_deref()
            .ok_or_else(|| ProviderError(format!("devise inconnue pour {ticker} sur EODHD")))?;
        let close = match listing.previous_close.as_ref().and_then(positive_number) {
            Some(close) => close,
            None => self.last_close(&ticker).await?,
        };
        let (price, currency) = normalize_quote(close, currency);
        let currency = Currency::parse(&currency)
            .map_err(|_| ProviderError(format!("devise {currency} non reconnue pour {ticker}")))?;
        Ok(Quote { price, currency })
    }

    /// Recherche par ISIN, sinon par code du ticker, puis dernier cours de la cotation retenue.
    async fn quote(&self, request: &QuoteRequest) -> Result<Quote, ProviderError> {
        let (code, exchange) = match request.symbol.as_deref() {
            Some(symbol) => {
                let (code, exchange) = split_ticker(symbol);
                (Some(code), exchange)
            }
            None => (None, None),
        };
        let currency = request.currency.as_str();
        let mut last_error = ProviderError("ni ISIN ni symbole".to_string());
        let searches = [
            request.isin.as_deref().map(|isin| {
                (
                    isin,
                    Wanted {
                        isin: Some(isin),
                        code: None,
                        exchange,
                        currency,
                    },
                )
            }),
            code.map(|code| {
                (
                    code,
                    Wanted {
                        isin: None,
                        code: Some(code),
                        exchange,
                        currency,
                    },
                )
            }),
        ];
        for (query, wanted) in searches.into_iter().flatten() {
            match self.search(query, &wanted).await {
                Ok(listing) => match self.price(&listing).await {
                    Ok(quote) => return Ok(quote),
                    Err(error) => last_error = error,
                },
                Err(error) => last_error = error,
            }
        }
        Err(last_error)
    }
}

#[async_trait]
impl PriceProvider for Eodhd {
    fn handles(&self, kind: HoldingKind) -> bool {
        kind == HoldingKind::Security
    }

    async fn quotes(
        &self,
        requests: &[QuoteRequest],
    ) -> HashMap<HoldingId, Result<Quote, ProviderError>> {
        // Une seule résolution par titre : l'offre gratuite limite le nombre d'appels.
        let mut resolved: HashMap<
            (Option<&str>, Option<&str>, &str),
            Result<Quote, ProviderError>,
        > = HashMap::new();
        let mut results = HashMap::new();
        for request in requests {
            let key = (
                request.isin.as_deref(),
                request.symbol.as_deref(),
                request.currency.as_str(),
            );
            let outcome = match resolved.get(&key) {
                Some(outcome) => outcome.clone(),
                None => {
                    let outcome = self.quote(request).await;
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

    fn listings() -> Vec<Listing> {
        let body = r#"[
            {"Code":"DEMO","Exchange":"XETRA","Name":"ETF Démo","Type":"ETF","Country":"Germany","Currency":"EUR",
             "ISIN":"LU0000000001","isPrimary":false,"previousClose":10.5,"previousCloseDate":"2026-01-02"},
            {"Code":"DEMO","Exchange":"LSE","Name":"ETF Démo","Type":"ETF","Country":"UK","Currency":"GBX",
             "ISIN":"LU0000000001","isPrimary":false,"previousClose":"NA"},
            {"Code":"DEMO","Exchange":"PA","Name":"ETF Démo","Type":"ETF","Country":"France","Currency":"EUR",
             "ISIN":"LU0000000001","isPrimary":true,"previousClose":10.4},
            {"Code":"AUTRE","Exchange":"US","Name":"Autre","Type":"Common Stock","Country":"USA","Currency":"USD",
             "ISIN":"US0000000002","previousClose":3}
        ]"#;
        serde_json::from_str(body).unwrap()
    }

    fn wanted<'a>(
        isin: Option<&'a str>,
        code: Option<&'a str>,
        exchange: Option<&'a str>,
        currency: &'a str,
    ) -> Wanted<'a> {
        Wanted {
            isin,
            code,
            exchange,
            currency,
        }
    }

    #[test]
    fn picks_the_requested_exchange_then_currency_then_primary_listing() {
        let all = listings();
        let pick = |w: Wanted| pick_listing(&all, &w).map(Listing::ticker);
        assert_eq!(
            pick(wanted(Some("lu0000000001"), None, Some("XETRA"), "EUR")).as_deref(),
            Some("DEMO.XETRA")
        );
        assert_eq!(
            pick(wanted(Some("LU0000000001"), None, None, "GBP")).as_deref(),
            Some("DEMO.LSE")
        );
        assert_eq!(
            pick(wanted(Some("LU0000000001"), None, None, "EUR")).as_deref(),
            Some("DEMO.PA")
        );
        assert_eq!(
            pick(wanted(None, Some("autre"), Some("US"), "EUR")).as_deref(),
            Some("AUTRE.US")
        );
        assert_eq!(pick(wanted(Some("FR0000000009"), None, None, "EUR")), None);
        assert_eq!(pick(wanted(None, Some("ZZZ"), None, "EUR")), None);
    }

    #[test]
    fn splits_yahoo_tickers_into_code_and_exchange() {
        assert_eq!(split_ticker("CW8.PA"), ("CW8", Some("PA")));
        assert_eq!(split_ticker("DEMO.DE"), ("DEMO", Some("XETRA")));
        assert_eq!(split_ticker("DEMO.L"), ("DEMO", Some("LSE")));
        assert_eq!(split_ticker("DEMO.XX"), ("DEMO", None));
        assert_eq!(split_ticker("AAPL"), ("AAPL", Some("US")));
    }

    #[test]
    fn reads_prices_given_as_numbers_or_strings() {
        assert_eq!(positive_number(&serde_json::json!(12.5)), Some(12.5));
        assert_eq!(positive_number(&serde_json::json!(" 12.5 ")), Some(12.5));
        assert_eq!(positive_number(&serde_json::json!("NA")), None);
        assert_eq!(positive_number(&serde_json::json!(0)), None);
        assert_eq!(positive_number(&Value::Null), None);
    }
}
