use fineme_domain::oauth::CodeChallenge;

use super::*;
use crate::auth::Session;
use crate::ports::{AccessTokenCrypto, Clock, TotpSupport};
use crate::testing::{FakeAccessCrypto, TestApp};

const PASSWORD: &str = "mot-de-passe-de-test";
const CLIENT: &str = "https://claude.ai/oauth/client.json";
const CALLBACK: &str = "https://claude.ai/api/mcp/auth_callback";
const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
const RESOURCE: &str = "https://api.example.test/mcp";

fn origin() -> LoginOrigin {
    LoginOrigin::new(Some("203.0.113.9"), Some("Client de test"))
}

async fn configured() -> (TestApp, Session) {
    let t = TestApp::with_oauth().await;
    let session = t
        .app
        .auth
        .setup("a@b.test", PASSWORD, "A", None)
        .await
        .unwrap();
    t.clients.publish(CLIENT, "Claude", &[CALLBACK]);
    (t, session)
}

fn challenge() -> String {
    CodeChallenge::from_digest(&FakeAccessCrypto::default().sha256(VERIFIER.as_bytes()))
        .as_str()
        .to_string()
}

fn params() -> AuthorizeParams {
    AuthorizeParams {
        response_type: Some("code".into()),
        client_id: Some(CLIENT.into()),
        redirect_uri: Some(CALLBACK.into()),
        code_challenge: Some(challenge()),
        code_challenge_method: Some("S256".into()),
        state: Some("xyz".into()),
        scope: Some("read".into()),
        resource: Some(RESOURCE.into()),
    }
}

fn query_value(url: &str, key: &str) -> Option<String> {
    url.split_once('?')?
        .1
        .split('&')
        .find_map(|pair| pair.strip_prefix(&format!("{key}=")))
        .map(str::to_string)
}

async fn request_id(t: &TestApp) -> String {
    let url = t.app.auth.authorize(params(), &origin()).await.unwrap();
    assert!(url.starts_with("https://web.example.test/oauth/consent?request=fm_oar_"));
    query_value(&url, "request").unwrap()
}

async fn approved_code(t: &TestApp) -> String {
    let id = request_id(t).await;
    let url = t
        .app
        .auth
        .approve_request(&id, PASSWORD, None, &origin())
        .await
        .unwrap();
    assert!(
        url.starts_with(&format!("{CALLBACK}?code=fm_oac_")),
        "{url}"
    );
    assert!(
        url.ends_with("&state=xyz&iss=https%3A%2F%2Fapi.example.test"),
        "{url}"
    );
    query_value(&url, "code").unwrap()
}

fn exchange(code: &str) -> TokenRequest {
    TokenRequest::AuthorizationCode {
        code: code.into(),
        redirect_uri: CALLBACK.into(),
        client_id: CLIENT.into(),
        code_verifier: VERIFIER.into(),
        resource: Some(RESOURCE.into()),
    }
}

fn refresh(token: &OAuthSecret) -> TokenRequest {
    TokenRequest::Refresh {
        refresh_token: token.as_str().into(),
        client_id: CLIENT.into(),
        scope: None,
        resource: None,
    }
}

async fn connected(t: &TestApp) -> IssuedTokens {
    let code = approved_code(t).await;
    t.app.auth.token(exchange(&code), &origin()).await.unwrap()
}

async fn accepted(t: &TestApp, token: &OAuthSecret) -> bool {
    t.app
        .auth
        .authenticate_access_token(Some(token.as_str()), &origin())
        .await
        .is_ok()
}

fn refusal(result: Result<IssuedTokens, TokenError>) -> TokenErrorCode {
    match result {
        Err(TokenError::Refused { code, .. }) => code,
        other => panic!("refus attendu, obtenu {other:?}"),
    }
}

#[tokio::test]
async fn consent_then_exchange_gives_an_access_token_accepted_by_mcp() {
    let (t, session) = configured().await;
    let id = request_id(&t).await;
    let view = t.app.auth.consent_view(&id).await.unwrap();
    assert_eq!(view.client_name, "Claude");
    assert_eq!(view.redirect_host, "claude.ai");
    assert_eq!(view.expires_in, 600);

    let tokens = connected(&t).await;
    assert_eq!(tokens.expires_in, 3600);
    assert!(tokens.access_token.as_str().starts_with("fm_oat_"));
    assert!(tokens.refresh_token.as_str().starts_with("fm_ort_"));
    let principal = t
        .app
        .auth
        .authenticate_access_token(Some(tokens.access_token.as_str()), &origin())
        .await
        .unwrap();
    let grants = t.app.auth.oauth_grants(session.user.id).await.unwrap();
    assert_eq!(grants.len(), 1);
    assert_eq!(principal.token_id, grants[0].id.as_uuid());
    assert_eq!(principal.token_name, "Claude");
    assert_eq!(principal.user, session.user.id);
    assert_eq!(grants[0].last_used_at, Some(t.clock.now()));
    // Ni le secret ni le jeton de renouvellement ne sont conservés en clair.
    let stored = format!("{:?}", t.store.state().oauth_access);
    assert!(!stored.contains(tokens.access_token.as_str()));
    // La consultation du journal des connexions voit le consentement.
    let history = t.app.auth.login_history(None).await.unwrap();
    assert!(
        history
            .iter()
            .any(|e| e.action == LoginAction::Oauth && e.success)
    );
}

#[tokio::test]
async fn disabled_server_refuses_everything() {
    let t = TestApp::empty().await;
    assert!(matches!(
        t.app.auth.authorize(params(), &origin()).await,
        Err(AppError::NotFound(DISABLED))
    ));
    assert!(matches!(
        t.app.auth.token(exchange("x"), &origin()).await,
        Err(TokenError::App(AppError::NotFound(DISABLED)))
    ));
    let secret = OAuthSecret::from_random(SecretKind::Access, &[1; SECRET_BYTES]);
    assert!(!accepted(&t, &secret).await);
}

#[tokio::test]
async fn unknown_client_or_redirect_never_redirects() {
    let (t, _) = configured().await;
    t.clients
        .publish("https://evil.test/client.json", "Claude", &[CALLBACK]);
    for bad in [
        AuthorizeParams {
            client_id: Some("https://evil.test/client.json".into()),
            ..params()
        },
        AuthorizeParams {
            client_id: Some("https://claude.ai/absent.json".into()),
            ..params()
        },
        AuthorizeParams {
            client_id: None,
            ..params()
        },
        AuthorizeParams {
            redirect_uri: Some(format!("{CALLBACK}/autre")),
            ..params()
        },
        AuthorizeParams {
            redirect_uri: None,
            ..params()
        },
    ] {
        let result = t.app.auth.authorize(bad, &origin()).await;
        assert!(
            matches!(result, Err(AppError::Invalid(_) | AppError::Domain(_))),
            "{result:?}"
        );
    }
    assert!(t.store.state().oauth_requests.is_empty());
}

#[tokio::test]
async fn other_mistakes_return_to_the_client_with_an_error() {
    let (t, _) = configured().await;
    for (bad, error) in [
        (
            AuthorizeParams {
                response_type: Some("token".into()),
                ..params()
            },
            "unsupported_response_type",
        ),
        (
            AuthorizeParams {
                code_challenge_method: Some("plain".into()),
                ..params()
            },
            "invalid_request",
        ),
        (
            AuthorizeParams {
                code_challenge: None,
                ..params()
            },
            "invalid_request",
        ),
        (
            AuthorizeParams {
                scope: Some("read write".into()),
                ..params()
            },
            "invalid_scope",
        ),
        (
            AuthorizeParams {
                resource: Some("https://autre.example.test/mcp".into()),
                ..params()
            },
            "invalid_target",
        ),
    ] {
        let url = t.app.auth.authorize(bad, &origin()).await.unwrap();
        assert_eq!(
            url,
            format!("{CALLBACK}?error={error}&state=xyz&iss=https%3A%2F%2Fapi.example.test")
        );
    }
    for state in ["s".repeat(1025), "a\nb".to_string()] {
        let bad = AuthorizeParams {
            state: Some(state),
            ..params()
        };
        let url = t.app.auth.authorize(bad, &origin()).await.unwrap();
        assert_eq!(
            url,
            format!("{CALLBACK}?error=invalid_request&iss=https%3A%2F%2Fapi.example.test")
        );
    }
    assert!(t.store.state().oauth_requests.is_empty());
}

#[tokio::test]
async fn a_request_expires_and_serves_once() {
    let (t, _) = configured().await;
    let id = request_id(&t).await;
    t.clock.advance_seconds(600);
    assert!(matches!(
        t.app.auth.consent_view(&id).await,
        Err(AppError::NotFound(UNKNOWN_REQUEST))
    ));

    let id = request_id(&t).await;
    let url = t.app.auth.deny_request(&id).await.unwrap();
    assert!(url.starts_with(&format!("{CALLBACK}?error=access_denied&state=xyz")));
    assert!(matches!(
        t.app.auth.deny_request(&id).await,
        Err(AppError::NotFound(_))
    ));
    assert!(matches!(
        t.app
            .auth
            .approve_request(&id, PASSWORD, None, &origin())
            .await,
        Err(AppError::NotFound(_))
    ));
    assert!(matches!(
        t.app.auth.consent_view("fm_oar_invalide").await,
        Err(AppError::NotFound(_))
    ));
}

#[tokio::test]
async fn pending_requests_are_bounded() {
    let (t, _) = configured().await;
    let first = request_id(&t).await;
    for n in 0..crate::ports::MAX_PENDING_REQUESTS {
        // Une source différente à chaque fois : le limiteur par IP ne s'applique pas ici.
        let source = LoginOrigin::new(Some(&format!("198.51.100.{n}")), None);
        t.app.auth.authorize(params(), &source).await.unwrap();
    }
    assert_eq!(
        t.store.state().oauth_requests.len(),
        crate::ports::MAX_PENDING_REQUESTS
    );
    assert!(t.app.auth.consent_view(&first).await.is_err());
}

#[tokio::test]
async fn approval_requires_password_and_second_factor_and_is_throttled() {
    let (t, session) = configured().await;
    t.app.auth.start_totp(session.user.id).await.unwrap();
    let sealed = t.store.state().second_factor.pending_secret.unwrap();
    let secret = t.totp.open(&sealed).unwrap();
    let code = t.totp.code(&secret, t.clock.now(), 0);
    t.app
        .auth
        .enable_totp(session.user.id, &code)
        .await
        .unwrap();
    t.clock.advance_seconds(60);
    let id = request_id(&t).await;
    for (password, code) in [
        ("faux-mot-de-passe", None),
        (PASSWORD, None),
        (PASSWORD, Some("000000")),
    ] {
        let result = t
            .app
            .auth
            .approve_request(&id, password, code, &origin())
            .await;
        assert!(matches!(result, Err(AppError::Forbidden(_))), "{result:?}");
    }
    let code = t.totp.code(&secret, t.clock.now(), 0);
    assert!(
        t.app
            .auth
            .approve_request(&id, PASSWORD, Some(&code), &origin())
            .await
            .is_ok()
    );
    // Au-delà des échecs tolérés, la route refuse avant de vérifier.
    let id = request_id(&t).await;
    for _ in 0..6 {
        let _ = t
            .app
            .auth
            .approve_request(&id, "faux-mot-de-passe", None, &origin())
            .await;
    }
    assert!(matches!(
        t.app
            .auth
            .approve_request(&id, PASSWORD, None, &origin())
            .await,
        Err(AppError::TooManyAttempts(_))
    ));
}

#[tokio::test]
async fn code_is_bound_to_verifier_client_redirect_and_time() {
    let (t, _) = configured().await;
    let wrong = |field: usize, code: String| match exchange(&code) {
        TokenRequest::AuthorizationCode {
            code,
            mut redirect_uri,
            mut client_id,
            mut code_verifier,
            mut resource,
        } => {
            match field {
                0 => redirect_uri.push('x'),
                1 => client_id.push('x'),
                2 => code_verifier = "a".repeat(43),
                3 => code_verifier = "court".into(),
                _ => resource = Some("https://autre.example.test/mcp".into()),
            }
            TokenRequest::AuthorizationCode {
                code,
                redirect_uri,
                client_id,
                code_verifier,
                resource,
            }
        }
        other => other,
    };
    for (field, expected) in [
        (0, TokenErrorCode::InvalidGrant),
        (1, TokenErrorCode::InvalidGrant),
        (2, TokenErrorCode::InvalidGrant),
        (3, TokenErrorCode::InvalidRequest),
        (4, TokenErrorCode::InvalidTarget),
    ] {
        t.clock.advance_days(1);
        let code = approved_code(&t).await;
        let result = t
            .app
            .auth
            .token(wrong(field, code.clone()), &origin())
            .await;
        assert_eq!(refusal(result), expected, "champ {field}");
        // Le code est consommé par la première tentative.
        let again = t.app.auth.token(exchange(&code), &origin()).await;
        assert_eq!(refusal(again), TokenErrorCode::InvalidGrant);
    }
    t.clock.advance_days(1);
    let code = approved_code(&t).await;
    t.clock.advance_seconds(60);
    let late = t.app.auth.token(exchange(&code), &origin()).await;
    assert_eq!(refusal(late), TokenErrorCode::InvalidGrant);
    assert!(t.store.state().oauth_grants.is_empty());
}

#[tokio::test]
async fn replaying_a_code_revokes_the_grant() {
    let (t, _) = configured().await;
    let code = approved_code(&t).await;
    let tokens = t.app.auth.token(exchange(&code), &origin()).await.unwrap();
    assert!(accepted(&t, &tokens.access_token).await);
    let replay = t.app.auth.token(exchange(&code), &origin()).await;
    assert_eq!(refusal(replay), TokenErrorCode::InvalidGrant);
    assert!(!accepted(&t, &tokens.access_token).await);
}

#[tokio::test]
async fn refresh_rotates_and_replaying_it_revokes_everything() {
    let (t, _) = configured().await;
    let first = connected(&t).await;
    t.clock.advance_seconds(3600);
    assert!(!accepted(&t, &first.access_token).await, "expiré");
    let second = t
        .app
        .auth
        .token(refresh(&first.refresh_token), &origin())
        .await
        .unwrap();
    assert_ne!(second.refresh_token.as_str(), first.refresh_token.as_str());
    assert!(accepted(&t, &second.access_token).await);

    let replay = t
        .app
        .auth
        .token(refresh(&first.refresh_token), &origin())
        .await;
    assert_eq!(refusal(replay), TokenErrorCode::InvalidGrant);
    assert!(!accepted(&t, &second.access_token).await);
    let after = t
        .app
        .auth
        .token(refresh(&second.refresh_token), &origin())
        .await;
    assert_eq!(refusal(after), TokenErrorCode::InvalidGrant);
}

#[tokio::test]
async fn refresh_is_bound_to_its_client_scope_resource_and_lifetime() {
    let (t, _) = configured().await;
    let tokens = connected(&t).await;
    let with = |client: &str, scope: Option<&str>, resource: Option<&str>| TokenRequest::Refresh {
        refresh_token: tokens.refresh_token.as_str().into(),
        client_id: client.into(),
        scope: scope.map(str::to_string),
        resource: resource.map(str::to_string),
    };
    for (request, expected) in [
        (
            with("https://claude.ai/autre.json", None, None),
            TokenErrorCode::InvalidGrant,
        ),
        (
            with(CLIENT, Some("write"), None),
            TokenErrorCode::InvalidScope,
        ),
        (
            with(CLIENT, None, Some("https://autre.example.test/mcp")),
            TokenErrorCode::InvalidTarget,
        ),
    ] {
        let result = t.app.auth.token(request, &origin()).await;
        assert_eq!(refusal(result), expected);
    }
    t.clock.advance_days(30);
    let late = t
        .app
        .auth
        .token(refresh(&tokens.refresh_token), &origin())
        .await;
    assert_eq!(refusal(late), TokenErrorCode::InvalidGrant);
}

#[tokio::test]
async fn failed_token_requests_are_throttled_but_a_valid_one_passes() {
    let (t, _) = configured().await;
    let tokens = connected(&t).await;
    for _ in 0..5 {
        let result = t.app.auth.token(exchange("inconnu"), &origin()).await;
        assert_eq!(refusal(result), TokenErrorCode::InvalidGrant);
    }
    let blocked = async {
        let _ = t.app.auth.token(exchange("inconnu"), &origin()).await;
        t.app.auth.token(exchange("inconnu"), &origin()).await
    }
    .await;
    assert!(matches!(
        blocked,
        Err(TokenError::App(AppError::TooManyAttempts(_)))
    ));
    assert!(
        t.app
            .auth
            .token(refresh(&tokens.refresh_token), &origin())
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn changing_the_password_revokes_every_grant() {
    let (t, session) = configured().await;
    let tokens = connected(&t).await;
    let pending_code = approved_code(&t).await;
    t.app
        .auth
        .change_password(session.user.id, PASSWORD, "un-autre-mot-de-passe")
        .await
        .unwrap();
    assert!(!accepted(&t, &tokens.access_token).await);
    let refreshed = t
        .app
        .auth
        .token(refresh(&tokens.refresh_token), &origin())
        .await;
    assert_eq!(refusal(refreshed), TokenErrorCode::InvalidGrant);
    // Un code consenti avant le changement ne vaut plus rien.
    let exchanged = t.app.auth.token(exchange(&pending_code), &origin()).await;
    assert_eq!(refusal(exchanged), TokenErrorCode::InvalidGrant);
    let grants = t.app.auth.oauth_grants(session.user.id).await.unwrap();
    assert!(grants.iter().all(|g| !g.is_active()));
}

#[tokio::test]
async fn a_token_issued_for_another_resource_or_stale_password_is_refused() {
    let (t, _) = configured().await;
    let tokens = connected(&t).await;
    t.store.lock().oauth_grants[0].resource =
        ProtectedResource::parse("https://autre.example.test/mcp").unwrap();
    assert!(!accepted(&t, &tokens.access_token).await);
    t.store.lock().oauth_grants[0].resource = oauth_resource();
    assert!(accepted(&t, &tokens.access_token).await);
    // Défense en profondeur : même non marquée révoquée, une autorisation d'avant le changement est refusée.
    t.store.lock().users[0].password_version += 1;
    assert!(!accepted(&t, &tokens.access_token).await);
}

fn oauth_resource() -> ProtectedResource {
    crate::testing::oauth_settings().resource()
}

#[tokio::test]
async fn authorizing_a_client_again_replaces_its_grant_and_grants_are_capped() {
    let (t, session) = configured().await;
    let first = connected(&t).await;
    t.clock.advance_seconds(10);
    let second = connected(&t).await;
    assert!(!accepted(&t, &first.access_token).await);
    assert!(accepted(&t, &second.access_token).await);

    for n in 1..MAX_ACTIVE_GRANTS {
        let client = format!("https://claude.ai/oauth/client-{n}.json");
        t.clients.publish(&client, "Autre", &[CALLBACK]);
        let url = t
            .app
            .auth
            .authorize(
                AuthorizeParams {
                    client_id: Some(client),
                    ..params()
                },
                &origin(),
            )
            .await
            .unwrap();
        let id = query_value(&url, "request").unwrap();
        let code = t
            .app
            .auth
            .approve_request(&id, PASSWORD, None, &origin())
            .await
            .unwrap();
        let code = query_value(&code, "code").unwrap();
        let request = TokenRequest::AuthorizationCode {
            code,
            redirect_uri: CALLBACK.into(),
            client_id: format!("https://claude.ai/oauth/client-{n}.json"),
            code_verifier: VERIFIER.into(),
            resource: None,
        };
        t.app.auth.token(request, &origin()).await.unwrap();
    }
    let active = t.app.auth.oauth_grants(session.user.id).await.unwrap();
    assert_eq!(
        active.iter().filter(|g| g.is_active()).count(),
        MAX_ACTIVE_GRANTS
    );
    // Un onzième client est refusé ; le client déjà autorisé peut toujours se réautoriser.
    t.clients.publish(
        "https://claude.ai/oauth/onzieme.json",
        "Onzième",
        &[CALLBACK],
    );
    let url = t
        .app
        .auth
        .authorize(
            AuthorizeParams {
                client_id: Some("https://claude.ai/oauth/onzieme.json".into()),
                ..params()
            },
            &origin(),
        )
        .await
        .unwrap();
    let id = query_value(&url, "request").unwrap();
    assert!(matches!(
        t.app
            .auth
            .approve_request(&id, PASSWORD, None, &origin())
            .await,
        Err(AppError::Conflict(TOO_MANY_GRANTS))
    ));
    connected(&t).await;
}

#[tokio::test]
async fn revoking_a_grant_cuts_access_and_requires_identity() {
    let (t, session) = configured().await;
    let tokens = connected(&t).await;
    let user = session.user.id;
    let id = t.app.auth.oauth_grants(user).await.unwrap()[0].id;
    assert!(matches!(
        t.app
            .auth
            .revoke_oauth_grant(user, id, "faux-mot-de-passe", None, &origin())
            .await,
        Err(AppError::Forbidden(_))
    ));
    assert!(matches!(
        t.app
            .auth
            .revoke_oauth_grant(user, OAuthGrantId::generate(), PASSWORD, None, &origin())
            .await,
        Err(AppError::NotFound(UNKNOWN_GRANT))
    ));
    let revoked = t
        .app
        .auth
        .revoke_oauth_grant(user, id, PASSWORD, None, &origin())
        .await
        .unwrap();
    assert_eq!(revoked.revoked_at, Some(t.clock.now()));
    assert!(!accepted(&t, &tokens.access_token).await);
    let refreshed = t
        .app
        .auth
        .token(refresh(&tokens.refresh_token), &origin())
        .await;
    assert_eq!(refusal(refreshed), TokenErrorCode::InvalidGrant);
    assert!(matches!(
        t.app
            .auth
            .revoke_oauth_grant(user, id, PASSWORD, None, &origin())
            .await,
        Err(AppError::Conflict(ALREADY_REVOKED))
    ));
}

#[tokio::test]
async fn expired_requests_codes_and_tokens_are_pruned() {
    let (t, _) = configured().await;
    connected(&t).await;
    request_id(&t).await;
    t.clock.advance_days(31);
    assert!(t.app.auth.prune_oauth().await.unwrap() >= 4);
    let state = t.store.state();
    assert!(state.oauth_requests.is_empty() && state.oauth_codes.is_empty());
    assert!(state.oauth_access.is_empty() && state.oauth_refresh.is_empty());
    // L'autorisation reste visible dans Paramètres.
    assert_eq!(state.oauth_grants.len(), 1);
}

#[tokio::test]
async fn enabling_or_disabling_the_second_factor_keeps_grants() {
    let (t, session) = configured().await;
    let tokens = connected(&t).await;
    let user = session.user.id;
    t.app.auth.start_totp(user).await.unwrap();
    let sealed = t.store.state().second_factor.pending_secret.unwrap();
    let secret = t.totp.open(&sealed).unwrap();
    let code = t.totp.code(&secret, t.clock.now(), 0);
    t.app.auth.enable_totp(user, &code).await.unwrap();
    assert!(accepted(&t, &tokens.access_token).await);
    t.clock.advance_seconds(60);
    let code = t.totp.code(&secret, t.clock.now(), 0);
    t.app
        .auth
        .disable_totp(user, PASSWORD, &code, &origin())
        .await
        .unwrap();
    assert!(accepted(&t, &tokens.access_token).await);
    assert!(
        t.app
            .auth
            .token(refresh(&tokens.refresh_token), &origin())
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn a_code_replayed_while_being_exchanged_creates_no_grant() {
    let (t, _) = configured().await;
    let code = approved_code(&t).await;
    t.store.lock().oauth_codes[0].3 = true;
    let result = t.app.auth.token(exchange(&code), &origin()).await;
    assert_eq!(refusal(result), TokenErrorCode::InvalidGrant);
    assert!(t.store.state().oauth_grants.is_empty());
}

#[tokio::test]
async fn authorization_requests_are_limited_per_ip() {
    let (t, _) = configured().await;
    let mut allowed = 0;
    while t.app.auth.authorize(params(), &origin()).await.is_ok() {
        allowed += 1;
        assert!(allowed < 50);
    }
    assert_eq!(allowed, 21);
    assert!(matches!(
        t.app.auth.authorize(params(), &origin()).await,
        Err(AppError::TooManyAttempts(_))
    ));
    let other = LoginOrigin::new(Some("198.51.100.7"), None);
    assert!(t.app.auth.authorize(params(), &other).await.is_ok());
}

#[tokio::test]
async fn a_dead_but_well_formed_token_always_gets_invalid_grant() {
    let (t, _) = configured().await;
    let unknown = format!("fm_ort_{}", "a".repeat(43));
    for _ in 0..10 {
        let request = TokenRequest::Refresh {
            refresh_token: unknown.clone(),
            client_id: CLIENT.into(),
            scope: None,
            resource: None,
        };
        let result = t.app.auth.token(request, &origin()).await;
        assert_eq!(refusal(result), TokenErrorCode::InvalidGrant);
    }
}

#[tokio::test]
async fn replaying_an_expired_code_does_not_revoke() {
    let (t, _) = configured().await;
    let code = approved_code(&t).await;
    let tokens = t.app.auth.token(exchange(&code), &origin()).await.unwrap();
    t.clock.advance_seconds(61);
    let replay = t.app.auth.token(exchange(&code), &origin()).await;
    assert_eq!(refusal(replay), TokenErrorCode::InvalidGrant);
    assert!(accepted(&t, &tokens.access_token).await);
}
