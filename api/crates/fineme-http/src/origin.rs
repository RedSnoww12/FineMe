//! Origine d'une requête (IP du client, user-agent) pour le limiteur de tentatives et le journal des connexions.
//!
//! Derrière un proxy (Render), la connexion TCP vient du proxy : l'IP du client est dans `X-Forwarded-For`.
//! Chaque proxy y **ajoute à droite** l'adresse qu'il a vue ; tout ce qui est à gauche peut avoir été écrit
//! par le client lui-même. On lit donc l'entrée située à `trusted_proxy_hops` positions depuis la droite
//! (1 = la dernière, ajoutée par le proxy de Render), jamais la première. Sans l'en-tête, ou avec moins
//! d'entrées que de proxys attendus, on retombe sur l'adresse de la connexion TCP.

use std::net::{IpAddr, SocketAddr};

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{HeaderMap, header};
use fineme_domain::login::LoginOrigin;

/// Nombre de proxys de confiance devant l'API (`TRUSTED_PROXY_HOPS`) ; 0 = ignorer `X-Forwarded-For`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrustedProxyHops(pub usize);

/// IP du client selon `X-Forwarded-For` et le nombre de proxys de confiance, sinon l'adresse TCP.
pub fn client_ip(
    headers: &HeaderMap,
    hops: TrustedProxyHops,
    peer: Option<IpAddr>,
) -> Option<String> {
    let forwarded = (hops.0 > 0)
        .then(|| headers.get_all("x-forwarded-for"))
        .into_iter()
        .flatten()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .collect::<Vec<_>>();
    forwarded
        .len()
        .checked_sub(hops.0)
        .and_then(|index| forwarded.get(index))
        .and_then(|entry| entry.parse::<IpAddr>().ok())
        .or(peer)
        .map(|ip| ip.to_string())
}

/// Extracteur : origine de la requête, construite selon la politique déposée par le routeur.
pub struct ClientOrigin(pub LoginOrigin);

impl<S: Send + Sync> FromRequestParts<S> for ClientOrigin {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let hops = parts
            .extensions
            .get::<TrustedProxyHops>()
            .copied()
            .unwrap_or(TrustedProxyHops(0));
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(address)| address.ip());
        let ip = client_ip(&parts.headers, hops, peer);
        let user_agent = parts
            .headers
            .get(header::USER_AGENT)
            .and_then(|value| value.to_str().ok());
        Ok(Self(LoginOrigin::new(ip.as_deref(), user_agent)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(values: &[&str]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for value in values {
            map.append("x-forwarded-for", HeaderValue::from_str(value).unwrap());
        }
        map
    }

    const PEER: Option<IpAddr> = Some(IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 1)));

    #[test]
    fn the_rightmost_entry_added_by_the_trusted_proxy_wins() {
        // Le client a tenté d'usurper une adresse en tête ; Render a ajouté la vraie à droite.
        let h = headers(&["1.2.3.4, 203.0.113.7"]);
        assert_eq!(
            client_ip(&h, TrustedProxyHops(1), PEER).as_deref(),
            Some("203.0.113.7")
        );
        assert_eq!(
            client_ip(&h, TrustedProxyHops(2), PEER).as_deref(),
            Some("1.2.3.4")
        );
        // Plusieurs en-têtes se lisent comme une seule liste.
        let h = headers(&["1.2.3.4", "198.51.100.2"]);
        assert_eq!(
            client_ip(&h, TrustedProxyHops(1), PEER).as_deref(),
            Some("198.51.100.2")
        );
    }

    #[test]
    fn falls_back_to_the_tcp_peer() {
        let h = headers(&["203.0.113.7"]);
        assert_eq!(
            client_ip(&h, TrustedProxyHops(0), PEER).as_deref(),
            Some("10.0.0.1")
        );
        assert_eq!(
            client_ip(&h, TrustedProxyHops(2), PEER).as_deref(),
            Some("10.0.0.1")
        );
        assert_eq!(
            client_ip(&headers(&["pas-une-ip"]), TrustedProxyHops(1), PEER).as_deref(),
            Some("10.0.0.1")
        );
        assert_eq!(
            client_ip(&HeaderMap::new(), TrustedProxyHops(1), None),
            None
        );
    }

    #[test]
    fn ipv6_is_supported() {
        let h = headers(&["2001:db8::1"]);
        assert_eq!(
            client_ip(&h, TrustedProxyHops(1), None).as_deref(),
            Some("2001:db8::1")
        );
    }
}
