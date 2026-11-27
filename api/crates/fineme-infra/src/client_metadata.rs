//! Lecture du document de métadonnées d'un client OAuth (Client ID Metadata Document). L'URL vient d'une requête
//! publique : hôte revérifié par l'analyseur du client HTTP, adresses privées refusées, aucune redirection ni
//! proxy, délai et taille bornés.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use fineme_application::ports::{ClientMetadataFetcher, ProviderError};
use fineme_domain::oauth::{ClientId, ClientMetadata};
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::{Url, header, redirect};
use serde::Deserialize;

const TIMEOUT: Duration = Duration::from_secs(5);
const MAX_BYTES: usize = 16 * 1024;

#[derive(Deserialize)]
struct Document {
    client_id: String,
    #[serde(default)]
    client_name: String,
    #[serde(default)]
    redirect_uris: Vec<String>,
    token_endpoint_auth_method: Option<String>,
}

/// Vrai pour une adresse routable sur Internet.
fn is_public(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => is_public_v6(v6),
    }
}

fn is_public_v4(address: Ipv4Addr) -> bool {
    let [a, b, c, _] = address.octets();
    !(address.is_private()
        || address.is_loopback()
        || address.is_link_local()
        || address.is_multicast()
        || address.is_documentation()
        || a == 0
        || a >= 240
        || (a == 100 && (64..128).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 198 && (b == 18 || b == 19)))
}

/// Seul l'unicast global (`2000::/3`), hors documentation, Teredo et 6to4 (qui encapsulent de l'IPv4).
fn is_public_v6(address: Ipv6Addr) -> bool {
    if let Some(v4) = address.to_ipv4_mapped() {
        return is_public_v4(v4);
    }
    let [first, second, ..] = address.segments();
    (first & 0xe000) == 0x2000
        && first != 0x2002
        && !(first == 0x2001 && (second == 0 || second == 0x0db8))
}

/// Résolution DNS qui ne rend que des adresses publiques (protection contre le SSRF par le DNS).
struct PublicOnly;

impl Resolve for PublicOnly {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(async move {
            let resolved: Vec<SocketAddr> = tokio::net::lookup_host((name.as_str(), 0))
                .await?
                .filter(|socket| is_public(socket.ip()))
                .collect();
            if resolved.is_empty() {
                return Err("aucune adresse publique pour cet hôte".into());
            }
            Ok(Box::new(resolved.into_iter()) as Addrs)
        })
    }
}

pub struct HttpClientMetadata {
    http: reqwest::Client,
    /// Serveur qui remplace `https://<hôte>` (tests contre un serveur simulé).
    base_url: Option<String>,
}

fn builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .timeout(TIMEOUT)
        .redirect(redirect::Policy::none())
        .no_proxy()
        .user_agent("FineMe")
}

impl HttpClientMetadata {
    pub fn new() -> Result<Self, reqwest::Error> {
        Ok(Self {
            http: builder()
                .https_only(true)
                .dns_resolver(Arc::new(PublicOnly))
                .build()?,
            base_url: None,
        })
    }

    /// Pour les tests seulement : tous les hôtes sont servis par `base_url`, sans les protections réseau.
    pub fn served_by(base_url: &str) -> Result<Self, reqwest::Error> {
        Ok(Self {
            http: builder().build()?,
            base_url: Some(base_url.trim_end_matches('/').to_string()),
        })
    }

    /// URL à lire, après une seconde analyse qui doit voir le même hôte que le domaine.
    fn url(&self, client_id: &ClientId) -> Result<Url, ProviderError> {
        let refused = || ProviderError("adresse du client refusée".into());
        let url = Url::parse(client_id.as_str()).map_err(|_| refused())?;
        if url.domain() != Some(client_id.host())
            || url.port().is_some()
            || url.as_str() != client_id.as_str()
        {
            return Err(refused());
        }
        match &self.base_url {
            Some(base) => Url::parse(&format!("{base}{}", url.path())).map_err(|_| refused()),
            None => Ok(url),
        }
    }
}

#[async_trait]
impl ClientMetadataFetcher for HttpClientMetadata {
    async fn fetch(&self, client_id: &ClientId) -> Result<ClientMetadata, ProviderError> {
        let unreadable = |reason: &str| ProviderError(format!("document du client : {reason}"));
        let mut response = self
            .http
            .get(self.url(client_id)?)
            .header(header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|e| unreadable(&e.without_url().to_string()))?;
        if response.status() != reqwest::StatusCode::OK {
            return Err(unreadable(&format!("HTTP {}", response.status().as_u16())));
        }
        let json = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.split(';').next().unwrap_or_default().trim())
            .is_some_and(|mime| mime == "application/json" || mime.ends_with("+json"));
        if !json {
            return Err(unreadable("ce n'est pas du JSON"));
        }
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| unreadable("lecture interrompue"))?
        {
            if body.len() + chunk.len() > MAX_BYTES {
                return Err(unreadable("document trop volumineux"));
            }
            body.extend_from_slice(&chunk);
        }
        let document: Document =
            serde_json::from_slice(&body).map_err(|_| unreadable("JSON invalide"))?;
        Ok(ClientMetadata {
            client_id: document.client_id,
            client_name: document.client_name,
            redirect_uris: document.redirect_uris,
            token_endpoint_auth_method: document.token_endpoint_auth_method,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_public_addresses_are_reachable() {
        for public in ["93.184.215.14", "160.79.104.1", "2606:4700::1111"] {
            assert!(is_public(public.parse().unwrap()), "{public}");
        }
        for private in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "198.18.0.1",
            "224.0.0.1",
            "255.255.255.255",
            "::1",
            "::",
            "fe80::1",
            "fd00::1",
            "::ffff:127.0.0.1",
            "64:ff9b::7f00:1",
            "2002:7f00:1::",
            "2001:db8::1",
        ] {
            assert!(!is_public(private.parse().unwrap()), "{private}");
        }
    }

    #[tokio::test]
    async fn private_hosts_are_not_resolved() {
        let name: Name = "localhost".parse().unwrap();
        assert!(PublicOnly.resolve(name).await.is_err());
    }
}
