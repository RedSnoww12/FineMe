use super::*;

const CLIENT: &str = "https://claude.ai/oauth/mcp-client.json";
const CALLBACK: &str = "https://claude.ai/api/mcp/auth_callback";

fn client_id() -> ClientId {
    ClientId::parse(CLIENT).unwrap()
}

fn redirect() -> RedirectUri {
    RedirectUri::parse(CALLBACK).unwrap()
}

fn issuer() -> PublicUrl {
    PublicUrl::parse("https://api.example.test").unwrap()
}

#[test]
fn secrets_are_prefixed_and_not_interchangeable() {
    let kinds = [
        SecretKind::Request,
        SecretKind::Code,
        SecretKind::Access,
        SecretKind::Refresh,
    ];
    for kind in kinds {
        let secret = OAuthSecret::from_random(kind, &[7; SECRET_BYTES]);
        assert!(secret.as_str().starts_with(kind.prefix()));
        assert_eq!(format!("{secret:?}"), "OAuthSecret(***)");
        for other in kinds {
            let parsed = OAuthSecret::parse(other, secret.as_str());
            assert_eq!(parsed.is_some(), other == kind);
        }
    }
    assert!(OAuthSecret::parse(SecretKind::Code, "fm_oac_court").is_none());
}

#[test]
fn client_id_is_an_https_url_with_a_path() {
    assert_eq!(client_id().host(), "claude.ai");
    assert_eq!(
        ClientId::parse("https://Claude.AI/a").unwrap().host(),
        "claude.ai"
    );
    for invalid in [
        "http://claude.ai/client.json",
        "https://claude.ai",
        "https://claude.ai/",
        "https://claude.ai:444/client.json",
        "https://user@claude.ai/client.json",
        "https://claude.ai@evil.test/client.json",
        "https://claude.ai/client.json#x",
        "https://claude.ai/client.json?x=1",
        "https://claude.ai/a%2Fclient.json",
        "https://claude.ai/a/..;/client.json",
        "https://claude.ai//client.json",
        "https://[claude.ai]/client.json",
        "https://[::1]/client.json",
        "https://127.0.0.1/client.json",
        "https://0x7f.1/client.json",
        "https://claude.ai/a/../client.json",
        "https://claude.ai/a/%2E%2E/client.json",
        "https://claude.ai\\@evil.test/client.json",
        "https://clàude.ai/client.json",
        "https:///client.json",
        "claude.ai/client.json",
    ] {
        assert_eq!(ClientId::parse(invalid), None, "{invalid}");
    }
}

#[test]
fn redirect_uri_is_https_or_loopback() {
    assert_eq!(redirect().host(), "claude.ai");
    let local = RedirectUri::parse("http://localhost:3118/callback").unwrap();
    assert_eq!(local.host(), "localhost:3118");
    assert_eq!(
        RedirectUri::parse("http://[::1]:8/cb").unwrap().host(),
        "[::1]:8"
    );
    assert!(RedirectUri::parse("http://127.0.0.1/cb").is_some());
    let long = format!("https://claude.ai/{}", "a".repeat(512 - 18));
    assert!(RedirectUri::parse(&long).is_some());
    assert!(RedirectUri::parse(&format!("{long}a")).is_none());
    for invalid in [
        "http://claude.ai/cb",
        "javascript:alert(1)",
        "myapp://callback",
        "https://claude.ai/cb#fragment",
        "https://a@claude.ai/cb",
        "https://claude.ai/c b",
        "https://claude.ai:99999/cb",
        "https://claude.ai:+80/cb",
        "https://claude.ai:080/cb",
        "https://claude.ai:0/cb",
        "http://[localhost]/cb",
        "http://[127.0.0.1]/cb",
        "https://::1/cb",
    ] {
        assert_eq!(RedirectUri::parse(invalid), None, "{invalid}");
    }
}

#[test]
fn response_parameters_are_encoded_and_appended() {
    assert_eq!(
        redirect().with_params(&[("code", "a_b-c"), ("state", "x y&z=é")]),
        format!("{CALLBACK}?code=a_b-c&state=x%20y%26z%3D%C3%A9")
    );
    let with_query = RedirectUri::parse("https://claude.ai/cb?k=v").unwrap();
    assert_eq!(
        with_query.with_params(&[("code", "1")]),
        "https://claude.ai/cb?k=v&code=1"
    );
}

#[test]
fn public_urls_are_bare_origins() {
    assert!(PublicUrl::parse("http://localhost:8080").is_ok());
    assert!(PublicUrl::parse("http://127.0.0.1:8080").is_ok());
    for invalid in [
        "https://api.example.test/",
        "https://api.example.test/api",
        "https://api.example.test?a=1",
        "http://api.example.test",
        "https://API.example.test",
        "api.example.test",
    ] {
        assert!(PublicUrl::parse(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn resource_accepts_only_the_mcp_endpoint() {
    let resource = ProtectedResource::mcp(&issuer());
    assert_eq!(resource.as_str(), "https://api.example.test/mcp");
    assert_eq!(
        ProtectedResource::parse(resource.as_str()),
        Some(resource.clone())
    );
    assert_eq!(ProtectedResource::parse("https://api.example.test"), None);
    for accepted in [
        "https://api.example.test/mcp",
        "https://api.example.test/mcp/",
        "HTTPS://API.example.test/mcp",
    ] {
        assert!(resource.accepts(accepted), "{accepted}");
    }
    for refused in [
        "https://api.example.test",
        "https://api.example.test/MCP",
        "https://api.example.test/mcp/x",
        "https://api.example.test.evil.test/mcp",
        "https://api.example.test/mcp#f",
        "https://api.example.test/mcp?x",
    ] {
        assert!(!resource.accepts(refused), "{refused}");
    }
}

#[test]
fn pkce_challenge_matches_the_rfc_7636_vector() {
    // RFC 7636, annexe B : empreinte SHA-256 du vérificateur d'exemple.
    let digest = [
        19, 211, 30, 150, 26, 26, 216, 236, 47, 22, 177, 12, 76, 152, 46, 8, 118, 168, 120, 173,
        109, 241, 68, 86, 110, 225, 137, 74, 203, 112, 249, 195,
    ];
    assert_eq!(
        CodeChallenge::from_digest(&digest).as_str(),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
    let verifier = CodeVerifier::parse("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk").unwrap();
    assert_eq!(format!("{verifier:?}"), "CodeVerifier(***)");
    assert!(CodeVerifier::parse(&"a".repeat(128)).is_some());
    assert!(CodeVerifier::parse(&"a".repeat(42)).is_none());
    assert!(CodeVerifier::parse(&"a".repeat(129)).is_none());
    assert!(CodeVerifier::parse(&format!("{}+", "a".repeat(43))).is_none());
    assert!(CodeChallenge::parse("court").is_none());
}

#[test]
fn state_and_client_name_are_bounded() {
    assert!(ClientState::parse("").is_some());
    assert!(ClientState::parse(&"s".repeat(1024)).is_some());
    assert!(ClientState::parse(&"s".repeat(1025)).is_none());
    assert!(ClientState::parse("a\nb").is_none());
    assert_eq!(ClientName::parse(" Claude ").unwrap().as_str(), "Claude");
    for invalid in [
        "",
        "  ",
        "a\u{202E}b",
        "a\u{200B}",
        "a\tb",
        "a\u{061C}b",
        "a\u{2028}b",
        "\u{2060}",
        "a\u{00AD}b",
        "a\u{FEFF}b",
        "a\u{E0041}",
    ] {
        assert_eq!(ClientName::parse(invalid), None, "{invalid:?}");
    }
    assert!(ClientName::parse(&"é".repeat(100)).is_some());
    assert!(ClientName::parse(&"é".repeat(101)).is_none());
}

#[test]
fn client_metadata_must_match_url_redirect_and_be_public() {
    let valid = ClientMetadata {
        client_id: CLIENT.into(),
        client_name: "Claude".into(),
        redirect_uris: vec![CALLBACK.into()],
        token_endpoint_auth_method: Some("none".into()),
    };
    let check = |m: &ClientMetadata| m.check(&client_id(), &redirect());
    assert_eq!(check(&valid).unwrap().as_str(), "Claude");
    let without_method = ClientMetadata {
        token_endpoint_auth_method: None,
        ..valid.clone()
    };
    assert!(check(&without_method).is_ok());
    for invalid in [
        ClientMetadata {
            client_id: format!("{CLIENT}x"),
            ..valid.clone()
        },
        ClientMetadata {
            redirect_uris: vec![format!("{CALLBACK}/x")],
            ..valid.clone()
        },
        ClientMetadata {
            token_endpoint_auth_method: Some("client_secret_post".into()),
            ..valid.clone()
        },
        ClientMetadata {
            client_name: " ".into(),
            ..valid.clone()
        },
    ] {
        assert!(check(&invalid).is_err(), "{invalid:?}");
    }
}

#[test]
fn redirect_must_stay_on_the_client_host_or_the_local_machine() {
    for (uri, belongs) in [
        (CALLBACK, true),
        ("https://claude.ai:8443/cb", true),
        ("http://localhost:3118/callback", true),
        ("http://[::1]:8/cb", true),
        ("https://evil.test/cb", false),
        ("https://claude.ai.evil.test/cb", false),
    ] {
        let redirect = RedirectUri::parse(uri).unwrap();
        assert_eq!(redirect.belongs_to(&client_id()), belongs, "{uri}");
    }
    let foreign = ClientMetadata {
        client_id: CLIENT.into(),
        client_name: "Claude".into(),
        redirect_uris: vec!["https://evil.test/cb".into()],
        token_endpoint_auth_method: None,
    };
    let target = RedirectUri::parse("https://evil.test/cb").unwrap();
    assert!(foreign.check(&client_id(), &target).is_err());
}

#[test]
fn grant_is_usable_only_when_active_at_the_current_password_version() {
    let grant = OAuthGrant {
        id: OAuthGrantId::generate(),
        user: UserId::generate(),
        client_id: client_id(),
        client_name: ClientName::parse("Claude").unwrap(),
        redirect_uri: redirect(),
        scope: TokenScope::Read,
        resource: ProtectedResource::mcp(&issuer()),
        password_version: 3,
        created_at: DateTime::UNIX_EPOCH,
        last_used_at: None,
        revoked_at: None,
    };
    assert!(grant.is_active() && grant.is_usable(3));
    assert!(!grant.is_usable(4));
    let revoked = OAuthGrant {
        revoked_at: Some(DateTime::UNIX_EPOCH),
        ..grant
    };
    assert!(!revoked.is_active() && !revoked.is_usable(3));
}

#[test]
fn client_hosts_are_exact_domain_names() {
    let host = ClientHost::parse("Claude.AI").unwrap();
    assert_eq!(host.as_str(), "claude.ai");
    assert!(host.serves(&ClientId::parse("https://claude.ai/client.json").unwrap()));
    assert!(!host.serves(&ClientId::parse("https://sub.claude.ai/client.json").unwrap()));
    for refused in [
        "",
        "*.claude.ai",
        "https://claude.ai",
        "claude.ai/chemin",
        "claude.ai:443",
        "claude.ai.",
        "127.0.0.1",
    ] {
        assert_eq!(ClientHost::parse(refused), None, "{refused}");
    }
}
