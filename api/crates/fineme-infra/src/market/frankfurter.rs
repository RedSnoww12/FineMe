use std::collections::HashMap;

use async_trait::async_trait;
use fineme_application::ports::{FxProvider, ProviderError};
use serde::Deserialize;

use super::get_json;

/// Taux de change de la BCE via frankfurter.app (base EUR).
pub struct Frankfurter {
    http: reqwest::Client,
    base_url: String,
}

impl Frankfurter {
    pub fn new(http: reqwest::Client, base_url: impl Into<String>) -> Self {
        Self {
            http,
            base_url: base_url.into(),
        }
    }
}

#[derive(Deserialize)]
struct Latest {
    rates: HashMap<String, f64>,
}

#[async_trait]
impl FxProvider for Frankfurter {
    async fn eur_rates(&self) -> Result<Vec<(String, f64)>, ProviderError> {
        let url = format!("{}/latest", self.base_url.trim_end_matches('/'));
        let latest: Latest = get_json(&self.http, &url, &[("from", "EUR")]).await?;
        Ok(latest.rates.into_iter().collect())
    }
}
