//! Lecture des métadonnées client contre un serveur HTTP simulé ; données fictives.

use fineme_application::ports::ClientMetadataFetcher;
use fineme_domain::oauth::ClientId;
use fineme_infra::client_metadata::HttpClientMetadata;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const CLIENT: &str = "https://claude.ai/oauth/client.json";

async fn serve(response: ResponseTemplate) -> (MockServer, HttpClientMetadata) {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/oauth/client.json"))
        .respond_with(response)
        .mount(&server)
        .await;
    let fetcher = HttpClientMetadata::served_by(&server.uri()).unwrap();
    (server, fetcher)
}

fn client() -> ClientId {
    ClientId::parse(CLIENT).unwrap()
}

fn document() -> serde_json::Value {
    json!({
        "client_id": CLIENT,
        "client_name": "Claude",
        "redirect_uris": ["https://claude.ai/api/mcp/auth_callback"],
        "token_endpoint_auth_method": "none",
        "grant_types": ["authorization_code"],
    })
}

#[tokio::test]
async fn reads_the_document() {
    let (_server, fetcher) = serve(ResponseTemplate::new(200).set_body_json(document())).await;
    let metadata = fetcher.fetch(&client()).await.unwrap();
    assert_eq!(metadata.client_id, CLIENT);
    assert_eq!(metadata.client_name, "Claude");
    assert_eq!(metadata.redirect_uris.len(), 1);
    assert_eq!(metadata.token_endpoint_auth_method.as_deref(), Some("none"));
}

#[tokio::test]
async fn refuses_errors_redirects_other_types_and_large_documents() {
    let mut large = document();
    large["padding"] = json!("x".repeat(20_000));
    for response in [
        ResponseTemplate::new(404),
        ResponseTemplate::new(302).insert_header("location", "https://evil.test/c.json"),
        ResponseTemplate::new(200).set_body_raw(document().to_string(), "text/html"),
        ResponseTemplate::new(200).set_body_raw("pas du json", "application/json"),
        ResponseTemplate::new(200).set_body_json(large),
    ] {
        let (_server, fetcher) = serve(response).await;
        assert!(fetcher.fetch(&client()).await.is_err());
    }
}

#[tokio::test]
async fn mixed_case_hosts_are_not_fetched() {
    let (server, fetcher) = serve(ResponseTemplate::new(200).set_body_json(document())).await;
    let mixed = ClientId::parse("https://Claude.ai/oauth/client.json").unwrap();
    assert!(fetcher.fetch(&mixed).await.is_err());
    assert!(server.received_requests().await.unwrap().is_empty());
}
