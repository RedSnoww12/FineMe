//! Clients de cours et de change contre un serveur HTTP simulé (wiremock).
//! Les corps de réponse reprennent la forme réelle des API, avec des valeurs fictives.

use std::collections::HashMap;

use fineme_application::ports::{FxProvider, PriceProvider, ProviderError, Quote, QuoteRequest};
use fineme_domain::fx::Currency;
use fineme_domain::holding::HoldingKind;
use fineme_domain::ids::HoldingId;
use fineme_infra::market::{CoinGecko, Frankfurter, YahooFinance, http_client};
use serde_json::json;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn request(isin: Option<&str>, symbol: Option<&str>, currency: &str) -> QuoteRequest {
    QuoteRequest {
        holding_id: HoldingId::generate(),
        isin: isin.map(str::to_string),
        symbol: symbol.map(str::to_string),
        currency: Currency::parse(currency).unwrap(),
    }
}

fn price(
    results: &HashMap<HoldingId, Result<Quote, ProviderError>>,
    request: &QuoteRequest,
) -> (f64, String) {
    let quote = results[&request.holding_id].clone().unwrap();
    (quote.price, quote.currency.as_str().to_string())
}

fn reason(
    results: &HashMap<HoldingId, Result<Quote, ProviderError>>,
    request: &QuoteRequest,
) -> String {
    results[&request.holding_id]
        .clone()
        .unwrap_err()
        .to_string()
}

fn chart_body(currency: &str, market_price: f64) -> serde_json::Value {
    json!({ "chart": { "result": [{
        "meta": { "currency": currency, "symbol": "DEMO", "exchangeName": "PAR", "regularMarketPrice": market_price,
                  "chartPreviousClose": market_price - 1.0, "dataGranularity": "1d", "range": "1d" },
        "timestamp": [1794580200],
        "indicators": { "quote": [{ "close": [market_price], "open": [market_price], "volume": [1000] }] }
    }], "error": null } })
}

// ---------------------------------------------------------------- Frankfurter

#[tokio::test]
async fn frankfurter_returns_eur_based_rates() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/latest"))
        .and(query_param("from", "EUR"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "amount": 1.0, "base": "EUR", "date": "2026-01-02", "rates": { "USD": 1.1052, "GBP": 0.83559, "JPY": 176.83 }
        })))
        .expect(1)
        .mount(&server)
        .await;
    let mut rates = Frankfurter::new(http_client().unwrap(), server.uri())
        .eur_rates()
        .await
        .unwrap();
    rates.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        rates,
        vec![
            ("GBP".to_string(), 0.83559),
            ("JPY".to_string(), 176.83),
            ("USD".to_string(), 1.1052)
        ]
    );
}

#[tokio::test]
async fn frankfurter_failures_are_reported_not_panicked() {
    let server = MockServer::start().await;
    Mock::given(path("/latest"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    let client = Frankfurter::new(http_client().unwrap(), format!("{}/", server.uri()));
    assert_eq!(
        client.eur_rates().await.unwrap_err().to_string(),
        "réponse HTTP 503"
    );
    Mock::given(path("/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>"))
        .mount(&server)
        .await;
    assert_eq!(
        client.eur_rates().await.unwrap_err().to_string(),
        "réponse illisible"
    );

    let unreachable = Frankfurter::new(http_client().unwrap(), "http://127.0.0.1:1");
    assert!(
        unreachable
            .eur_rates()
            .await
            .unwrap_err()
            .to_string()
            .starts_with("service injoignable")
    );
}

// ---------------------------------------------------------------- CoinGecko

#[tokio::test]
async fn coingecko_prices_known_symbols_in_the_holding_currency_without_searching() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/simple/price"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "bitcoin": { "eur": 58683.0, "usd": 64850.5 }, "ethereum": { "eur": 1647.52, "usd": 1820.1 }
        })))
        .expect(1)
        .mount(&server)
        .await;
    let provider = CoinGecko::new(http_client().unwrap(), server.uri());
    assert!(provider.handles(HoldingKind::Crypto) && !provider.handles(HoldingKind::Security));
    let (btc, eth, btc_again) = (
        request(None, Some("BTC"), "EUR"),
        request(None, Some("eth"), "USD"),
        request(None, Some("BTC"), "EUR"),
    );
    let results = provider
        .quotes(&[btc.clone(), eth.clone(), btc_again.clone()])
        .await;
    assert_eq!(price(&results, &btc), (58683.0, "EUR".to_string()));
    assert_eq!(price(&results, &eth), (1820.1, "USD".to_string()));
    assert_eq!(price(&results, &btc_again), (58683.0, "EUR".to_string()));
}

#[tokio::test]
async fn coingecko_searches_unknown_symbols_and_reports_per_holding_failures() {
    let server = MockServer::start().await;
    Mock::given(path("/search"))
        .and(query_param("query", "DMO"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "coins": [
            { "id": "wrapped-demo", "name": "Wrapped Demo", "api_symbol": "wrapped-demo", "symbol": "WDMO", "market_cap_rank": 300 },
            { "id": "demo-clone", "name": "Demo Clone", "api_symbol": "demo-clone", "symbol": "DMO", "market_cap_rank": null },
            { "id": "demo-coin", "name": "Demo Coin", "api_symbol": "demo-coin", "symbol": "DMO", "market_cap_rank": 42 }
        ], "exchanges": [], "categories": [] })))
        .mount(&server)
        .await;
    Mock::given(path("/search"))
        .and(query_param("query", "NOPE"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "coins": [] })))
        .mount(&server)
        .await;
    Mock::given(path("/simple/price"))
        .and(query_param("ids", "demo-coin"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "demo-coin": { "eur": 3.5 } })),
        )
        .mount(&server)
        .await;
    let provider = CoinGecko::new(http_client().unwrap(), server.uri());
    let (found, unknown, no_symbol, no_currency) = (
        request(None, Some("DMO"), "EUR"),
        request(None, Some("NOPE"), "EUR"),
        request(None, None, "EUR"),
        request(None, Some("DMO"), "CHF"),
    );
    let results = provider
        .quotes(&[
            found.clone(),
            unknown.clone(),
            no_symbol.clone(),
            no_currency.clone(),
        ])
        .await;
    assert_eq!(price(&results, &found), (3.5, "EUR".to_string()));
    assert_eq!(
        reason(&results, &unknown),
        "crypto NOPE introuvable sur CoinGecko"
    );
    assert_eq!(reason(&results, &no_symbol), "symbole manquant");
    assert_eq!(
        reason(&results, &no_currency),
        "pas de cours CHF pour DMO sur CoinGecko"
    );
}

#[tokio::test]
async fn coingecko_rate_limit_fails_every_holding_with_the_same_reason() {
    let server = MockServer::start().await;
    Mock::given(path("/simple/price"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&server)
        .await;
    let provider = CoinGecko::new(http_client().unwrap(), server.uri());
    let (btc, sol) = (
        request(None, Some("BTC"), "EUR"),
        request(None, Some("SOL"), "EUR"),
    );
    let results = provider.quotes(&[btc.clone(), sol.clone()]).await;
    assert_eq!(reason(&results, &btc), "réponse HTTP 429");
    assert_eq!(reason(&results, &sol), "réponse HTTP 429");
    assert!(provider.quotes(&[]).await.is_empty());
}

// ---------------------------------------------------------------- Yahoo Finance

#[tokio::test]
async fn yahoo_resolves_an_isin_then_reads_the_chart_once_per_security() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/finance/search"))
        .and(query_param("q", "FR0000000001"))
        .and(header("user-agent", "Mozilla/5.0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "explains": [], "count": 1,
            "quotes": [{ "exchange": "PAR", "shortname": "ETF Démo", "quoteType": "ETF", "symbol": "DEMO.PA",
                         "index": "quotes", "score": 20022.0, "isYahooFinance": true }],
            "news": [], "nav": []
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/v8/finance/chart/DEMO.PA"))
        .and(query_param("interval", "1d"))
        .respond_with(ResponseTemplate::new(200).set_body_json(chart_body("EUR", 34.6255)))
        .expect(1)
        .mount(&server)
        .await;
    let provider = YahooFinance::new(http_client().unwrap(), server.uri());
    assert!(provider.handles(HoldingKind::Security) && !provider.handles(HoldingKind::Crypto));
    let (first, same) = (
        request(Some("FR0000000001"), None, "EUR"),
        request(Some("FR0000000001"), None, "EUR"),
    );
    let results = provider.quotes(&[first.clone(), same.clone()]).await;
    assert_eq!(price(&results, &first), (34.6255, "EUR".to_string()));
    assert_eq!(price(&results, &same), (34.6255, "EUR".to_string()));
}

#[tokio::test]
async fn yahoo_reads_a_ticker_directly_and_normalizes_pence() {
    let server = MockServer::start().await;
    Mock::given(path("/v8/finance/chart/DEMO"))
        .respond_with(ResponseTemplate::new(200).set_body_json(chart_body("USD", 356.27)))
        .mount(&server)
        .await;
    Mock::given(path("/v8/finance/chart/LSE.L"))
        .respond_with(ResponseTemplate::new(200).set_body_json(chart_body("GBp", 1234.0)))
        .mount(&server)
        .await;
    let provider = YahooFinance::new(http_client().unwrap(), server.uri());
    let (us, uk) = (
        request(None, Some("DEMO"), "EUR"),
        request(None, Some("LSE.L"), "GBP"),
    );
    let results = provider.quotes(&[us.clone(), uk.clone()]).await;
    // Le cours reste dans la devise de cotation : la conversion revient au cas d'usage.
    assert_eq!(price(&results, &us), (356.27, "USD".to_string()));
    assert_eq!(price(&results, &uk), (12.34, "GBP".to_string()));
}

#[tokio::test]
async fn yahoo_falls_back_from_isin_to_ticker_and_from_ticker_to_search() {
    let server = MockServer::start().await;
    // ISIN inconnu → repli sur le ticker ; ticker sans cours direct → recherche → autre ticker.
    Mock::given(path("/v1/finance/search"))
        .and(query_param("q", "FR0000000002"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "quotes": [] })))
        .mount(&server)
        .await;
    Mock::given(path("/v8/finance/chart/ABC"))
        .respond_with(ResponseTemplate::new(404).set_body_json(
            json!({ "chart": { "result": null, "error": { "code": "Not Found" } } }),
        ))
        .mount(&server)
        .await;
    Mock::given(path("/v1/finance/search"))
        .and(query_param("q", "ABC"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                json!({ "quotes": [{ "quoteType": "NEWS" }, { "symbol": "ABC.PA" }] }),
            ),
        )
        .mount(&server)
        .await;
    Mock::given(path("/v8/finance/chart/ABC.PA"))
        .respond_with(ResponseTemplate::new(200).set_body_json(chart_body("EUR", 12.5)))
        .mount(&server)
        .await;
    let provider = YahooFinance::new(http_client().unwrap(), server.uri());
    let holding = request(Some("FR0000000002"), Some("ABC"), "EUR");
    assert_eq!(
        price(
            &provider.quotes(std::slice::from_ref(&holding)).await,
            &holding
        ),
        (12.5, "EUR".to_string())
    );
}

#[tokio::test]
async fn yahoo_reports_why_a_security_has_no_price() {
    let server = MockServer::start().await;
    Mock::given(path("/v1/finance/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "quotes": [] })))
        .mount(&server)
        .await;
    Mock::given(path("/v8/finance/chart/VIDE"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({ "chart": { "result": [{ "meta": { "currency": "EUR" } }], "error": null } }),
        ))
        .mount(&server)
        .await;
    Mock::given(path("/v8/finance/chart/LIMITE"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&server)
        .await;
    let provider = YahooFinance::new(http_client().unwrap(), server.uri());
    let (nothing, unknown_isin, no_price, throttled) = (
        request(None, None, "EUR"),
        request(Some("FR0000000003"), None, "EUR"),
        request(None, Some("VIDE"), "EUR"),
        request(None, Some("LIMITE"), "EUR"),
    );
    let results = provider
        .quotes(&[
            nothing.clone(),
            unknown_isin.clone(),
            no_price.clone(),
            throttled.clone(),
        ])
        .await;
    assert_eq!(reason(&results, &nothing), "ni ISIN ni symbole");
    assert_eq!(
        reason(&results, &unknown_isin),
        "FR0000000003 introuvable sur Yahoo Finance"
    );
    assert_eq!(
        reason(&results, &no_price),
        "pas de cours pour VIDE sur Yahoo Finance"
    );
    assert_eq!(reason(&results, &throttled), "réponse HTTP 429");
}
