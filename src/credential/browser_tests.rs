//! Independent callback and xAI browser boundaries. EC fixtures were generated
//! locally with OpenSSL, contain no user/upstream key, and are public test assets.
use super::callback::{Callback, CallbackDecision, classify};
use super::test_support::{Authority, Directory, Step, step};
use super::*;
use axum::http::StatusCode;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD as B64};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    net::SocketAddr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::{Instant, timeout},
};

const CLIENT: &str = "synthetic-browser-client";
const SCOPES: &str = "openid profile email offline_access grok-cli:access api:access";
fn keys() -> Value {
    json!({"keys":[serde_json::from_str::<Value>(include_str!("../../tests/fixtures/auth/grok-test-jwk.json")).unwrap()]})
}
fn claims(nonce: &str) -> Value {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    json!({"iss":"https://auth.x.ai","aud":CLIENT,"sub":"person-a","exp":timestamp+3600,"nonce":nonce})
}
fn sign(header: Value, claims: Value) -> String {
    let random = ring::rand::SystemRandom::new();
    let pair = ring::signature::EcdsaKeyPair::from_pkcs8(
        &ring::signature::ECDSA_P256_SHA256_FIXED_SIGNING,
        include_bytes!("../../tests/fixtures/auth/grok-test-p256.pk8"),
        &random,
    )
    .unwrap();
    let message = format!(
        "{}.{}",
        B64.encode(serde_json::to_vec(&header).unwrap()),
        B64.encode(serde_json::to_vec(&claims).unwrap())
    );
    format!(
        "{message}.{}",
        B64.encode(pair.sign(&random, message.as_bytes()).unwrap().as_ref())
    )
}
fn token(nonce: &str) -> Value {
    json!({"access_token":"synthetic-browser-access","refresh_token":"synthetic-browser-refresh",
        "id_token":sign(json!({"alg":"ES256","kid":"synthetic-grok-key"}),claims(nonce)),
        "expires_in":3600,"token_type":"Bearer","scope":SCOPES})
}
fn browser_steps(subject: &str) -> Vec<Step> {
    vec![
        step(
            "/oauth2/token",
            vec![
                ("grant_type", "authorization_code"),
                ("client_id", CLIENT),
                ("code", "synthetic-browser-code"),
            ],
            token("placeholder"),
        ),
        step("/.well-known/jwks.json", vec![], keys()),
        step(
            "/oauth2/userinfo",
            vec![("authorization", "Bearer synthetic-browser-access")],
            json!({"sub":subject}),
        ),
    ]
}
pub(super) async fn request(address: SocketAddr, target: &str) -> String {
    let mut stream = TcpStream::connect(address).await.unwrap();
    let request = format!("GET {target} HTTP/1.1\r\nHost: {address}\r\n\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    String::from_utf8(bytes).unwrap()
}
async fn run_browser(
    pool: &CredentialManager,
    authority: &Authority,
    wrong_nonce: bool,
) -> Result<AccountStatus, CredentialError> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let mut login = Box::pin(pool.login_grok_browser("personal", CLIENT, 0, |prompt| {
        sender
            .send((
                prompt.authorization_url.clone(),
                prompt.redirect_uri.clone(),
            ))
            .unwrap();
    }));
    let (authorization, redirect) = tokio::select! {
        _ = &mut login => panic!("browser login must wait for its callback"),
        result = receiver => result.unwrap(),
    };
    let url = url::Url::parse(&authorization).unwrap();
    assert_eq!(url.origin().ascii_serialization(), "https://auth.x.ai");
    assert_eq!(url.path(), "/oauth2/authorize");
    let fields: BTreeMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(fields["client_id"], CLIENT);
    assert_eq!(fields["scope"], SCOPES);
    assert_eq!(fields["response_type"], "code");
    assert_eq!(fields["referrer"], "grok-build");
    assert_eq!(fields["code_challenge_method"], "S256");
    assert_eq!(fields["redirect_uri"], redirect);
    assert_ne!(fields["nonce"], fields["state"]);
    assert!(!fields.contains_key("code_verifier"));
    let actual_nonce = if wrong_nonce {
        "unrelated-nonce"
    } else {
        fields["nonce"].as_str()
    };
    authority.steps.lock().unwrap().front_mut().unwrap().body = token(actual_nonce);
    let redirect = url::Url::parse(&redirect).unwrap();
    let address = SocketAddr::from(([127, 0, 0, 1], redirect.port().unwrap()));
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    query
        .append_pair("state", &fields["state"])
        .append_pair("code", "synthetic-browser-code");
    let target = format!("/callback?{}", query.finish());
    let (result, response) = timeout(Duration::from_secs(5), async {
        tokio::join!(&mut login, request(address, &target))
    })
    .await
    .unwrap();
    assert!(response.starts_with("HTTP/1.1 200"));
    assert!(response.contains("Check the CLI"));
    assert!(!response.contains("synthetic-browser-code"));
    {
        let calls = authority.forms.lock().unwrap();
        let exchange = calls
            .iter()
            .find(|call| call.path == "/oauth2/token")
            .unwrap();
        assert_eq!(exchange.fields["redirect_uri"], fields["redirect_uri"]);
        assert_eq!(exchange.fields.len(), 5);
        let verifier = &exchange.fields["code_verifier"];
        assert_eq!(
            B64.encode(ring::digest::digest(&ring::digest::SHA256, verifier.as_bytes()).as_ref()),
            fields["code_challenge"]
        );
        assert!(!authorization.contains(verifier));
    }
    // The callback socket is released even before remote validation finishes.
    let listener = TcpListener::bind(address).await.unwrap();
    drop(listener);
    result
}

#[test]
fn callback_state_errors_and_duplicate_fields_do_not_consume_transaction() {
    let address = "127.0.0.1:5555".parse().unwrap();
    for query in [
        "code=synthetic-code",
        "state=wrong&code=synthetic-code",
        "state=wrong&error=access_denied",
        "state=expected&state=expected&code=synthetic-code",
        "state=expected&code=one&code=two",
        "state=expected&%73tate=expected&code=synthetic-code",
    ] {
        let request = format!("GET /callback?{query} HTTP/1.1\r\nHost: 127.0.0.1:5555\r\n\r\n");
        assert!(matches!(
            classify(request.as_bytes(), address, "expected", "/callback"),
            CallbackDecision::Reject(_)
        ));
    }
    let request = b"GET /callback?state=expected&code=synthetic-code HTTP/1.1\r\nHost: 127.0.0.1:5555\r\n\r\n";
    match classify(request, address, "expected", "/callback") {
        CallbackDecision::Code(code) => assert_eq!(code.expose(), "synthetic-code"),
        _ => panic!("valid callback must yield code"),
    }
    let error = b"GET /callback?state=expected&error=access_denied HTTP/1.1\r\nHost: 127.0.0.1:5555\r\n\r\n";
    assert!(matches!(
        classify(error, address, "expected", "/callback"),
        CallbackDecision::Error(CredentialError::Denied)
    ));
}

#[test]
fn callback_host_method_path_and_body_are_not_generic_ingress() {
    let address = "127.0.0.1:5555".parse().unwrap();
    for request in [
        "POST /callback?state=expected&code=x HTTP/1.1\r\nHost: 127.0.0.1:5555\r\n\r\n",
        "GET /other?state=expected&code=x HTTP/1.1\r\nHost: 127.0.0.1:5555\r\n\r\n",
        "GET /callback?state=expected&code=x HTTP/1.1\r\nHost: localhost:5555\r\n\r\n",
        "GET /callback?state=expected&code=x HTTP/1.1\r\nHost: 127.0.0.1:5555\r\nHost: 127.0.0.1:5555\r\n\r\n",
        "GET /callback?state=expected&code=x HTTP/1.1\r\nHost: 127.0.0.1:5555\r\nContent-Length: 1\r\n\r\nx",
        "GET /callback?state=expected&code=x HTTP/1.1\r\nHost: 127.0.0.1:5555\r\nTransfer-Encoding: chunked\r\n\r\n",
    ] {
        assert!(matches!(
            classify(request.as_bytes(), address, "expected", "/callback"),
            CallbackDecision::Reject(_)
        ));
    }
    assert!(matches!(
        classify(&vec![b'x'; 8193], address, "expected", "/callback"),
        CallbackDecision::Reject(431)
    ));
}

#[tokio::test]
async fn unrelated_callback_can_be_followed_by_valid_callback_without_reflection() {
    let mut callback = Callback::bind(0, "/callback").await.unwrap();
    let address = callback.address();
    let (code, _) = timeout(Duration::from_secs(5), async {
        tokio::join!(
            callback.wait("expected", Instant::now() + Duration::from_secs(5)),
            async {
                let response = request(
                    address,
                    "/callback?state=wrong&error=access_denied&error_description=private-detail",
                )
                .await;
                assert!(response.starts_with("HTTP/1.1 400"));
                assert!(!response.contains("private-detail"));
                request(address, "/callback?state=expected&code=synthetic-code").await;
            }
        )
    })
    .await
    .unwrap();
    assert_eq!(code.unwrap().expose(), "synthetic-code");
}

#[tokio::test]
async fn callback_timeout_releases_literal_loopback_listener() {
    let mut callback = Callback::bind(0, "/callback").await.unwrap();
    let address = callback.address();
    assert_eq!(
        callback.redirect_uri(),
        format!("http://127.0.0.1:{}/callback", address.port())
    );
    assert_eq!(
        callback.wait("expected", Instant::now()).await.unwrap_err(),
        CredentialError::Expired
    );
    drop(callback);
    let listener = TcpListener::bind(address).await.unwrap();
    drop(listener);
}

#[test]
fn es256_nonce_issuer_audience_and_key_policy_are_independent_of_siwc() {
    let good = sign(
        json!({"alg":"ES256","kid":"synthetic-grok-key"}),
        claims("expected"),
    );
    let identity = grok::verify_identity(&good, &keys(), CLIENT, "expected").unwrap();
    assert_eq!(identity.subject, "person-a");
    assert_eq!(identity.scope, None);
    assert!(siwc::verify_identity(&good, &keys(), CLIENT, None).is_err());
    assert!(grok::verify_identity(&good, &keys(), CLIENT, "wrong").is_err());
    for field in ["nonce", "iss", "aud", "exp"] {
        let mut bad = claims("expected");
        bad[field] = if field == "exp" {
            json!(1)
        } else {
            json!("unrelated")
        };
        let token = sign(json!({"alg":"ES256","kid":"synthetic-grok-key"}), bad);
        assert!(grok::verify_identity(&token, &keys(), CLIENT, "expected").is_err());
    }
    for header in [
        json!({"alg":"RS256","kid":"synthetic-grok-key"}),
        json!({"alg":"ES256","kid":"wrong"}),
    ] {
        assert!(
            grok::verify_identity(
                &sign(header, claims("expected")),
                &keys(),
                CLIENT,
                "expected"
            )
            .is_err()
        );
    }
    let mut forged = good.as_bytes().to_vec();
    let index = good.rfind('.').unwrap() + 8;
    forged[index] = if forged[index] == b'A' { b'B' } else { b'A' };
    assert!(
        grok::verify_identity(
            std::str::from_utf8(&forged).unwrap(),
            &keys(),
            CLIENT,
            "expected"
        )
        .is_err()
    );
    let mut bad_curve = keys();
    bad_curve["keys"][0]["crv"] = json!("P-384");
    assert!(grok::verify_identity(&good, &bad_curve, CLIENT, "expected").is_err());
    assert_eq!(
        oauth::challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk")
            .unwrap()
            .as_str(),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
}

#[tokio::test]
async fn browser_exchange_verifies_signed_nonce_and_userinfo_before_publish() {
    let authority = Authority::new(browser_steps("person-a")).await;
    let dir = Directory::new();
    let pool = authority.pool(&dir);
    let status = run_browser(&pool, &authority, false).await.unwrap();
    assert_eq!(status.state, AccountState::Active);
    let loaded = pool
        .store
        .transaction()
        .unwrap()
        .load("grok", "personal")
        .unwrap();
    let account = loaded.account.unwrap();
    let credential = account.credential.as_ref().unwrap();
    assert!(credential.id_token.is_none()); // Nonce-bound transient token is not persisted.
    authority.done();
}

#[tokio::test]
async fn bad_nonce_or_mixed_subject_keeps_previous_browser_session() {
    for bad_nonce in [true, false] {
        let mut steps = browser_steps("person-a");
        if bad_nonce {
            steps.truncate(2);
        } else {
            steps[2].body = json!({"sub":"different-person"});
        }
        let authority = Authority::new(steps).await;
        let dir = Directory::new();
        let pool = authority.pool(&dir);
        let mut account = model::Account::new("grok", "personal", CLIENT);
        account.identity = Some(VerifiedIdentity {
            subject: "person-a".into(),
            scope: None,
        });
        account
            .replace_credential(Some(Credential {
                access: Secret::new("old-access".into()).unwrap(),
                refresh: Some(Secret::new("old-refresh".into()).unwrap()),
                scopes: Some(SCOPES.into()),
                expires_at: Some(4000000000),
                id_token: None,
            }))
            .unwrap();
        account.state = AccountState::Active;
        pool.store.transaction().unwrap().publish(&account).unwrap();
        let error = run_browser(&pool, &authority, bad_nonce).await.unwrap_err();
        assert_eq!(
            error,
            if bad_nonce {
                CredentialError::Protocol
            } else {
                CredentialError::IdentityMismatch
            }
        );
        let account = pool
            .store
            .transaction()
            .unwrap()
            .load("grok", "personal")
            .unwrap()
            .account
            .unwrap();
        assert_eq!(
            account.credential.as_ref().unwrap().access.expose(),
            "old-access"
        );
        authority.done();
    }
}

#[tokio::test]
async fn cancelling_browser_drops_idle_callback_socket_and_pending_ticket() {
    let authority = Authority::new(vec![]).await;
    let dir = Directory::new();
    let pool = authority.pool(&dir);
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let mut login = Box::pin(pool.login_grok_browser("personal", CLIENT, 0, |prompt| {
        sender.send(prompt.redirect_uri.clone()).unwrap();
    }));
    let redirect = tokio::select! { _=&mut login=>panic!("must await callback"), value=receiver=>value.unwrap() };
    let url = url::Url::parse(&redirect).unwrap();
    let address = SocketAddr::from(([127, 0, 0, 1], url.port().unwrap()));
    let mut stream = TcpStream::connect(address).await.unwrap();
    stream.write_all(b"GET /callback").await.unwrap();
    // Exercise the pending read without arbitrary sleeps or an independent server task.
    assert!(
        timeout(Duration::from_millis(20), &mut login)
            .await
            .is_err()
    );
    drop(login);
    let mut byte = [0; 1];
    let read = timeout(Duration::from_secs(1), stream.read(&mut byte))
        .await
        .unwrap();
    assert!(read.is_err() || read.unwrap() == 0);
    assert!(!pool.list(Some("grok")).unwrap()[0].login_pending);
    let listener = TcpListener::bind(address).await.unwrap();
    drop(listener);
    authority.done();
}

#[tokio::test]
async fn device_unavailable_zero_interval_and_identity_deadline_are_closed() {
    let mut missing = step("/oauth2/device/code", vec![], json!({}));
    missing.status = StatusCode::NOT_FOUND;
    let authority = Authority::new(vec![missing]).await;
    let dir = Directory::new();
    let pool = authority.pool(&dir);
    assert_eq!(
        pool.login_grok("personal", CLIENT, |_| {})
            .await
            .unwrap_err(),
        CredentialError::DeviceUnavailable
    );
    authority.done();

    let mut steps = super::tests::login_steps("person-a");
    steps[0].body["interval"] = json!(0);
    let authority = Authority::new(steps).await;
    let dir = Directory::new();
    let pool = authority.pool(&dir);
    super::tests::login(&pool, "personal").await;
    {
        let arrivals = authority.arrivals.lock().unwrap();
        assert!(arrivals[1].duration_since(arrivals[0]).as_secs() >= 1);
    }
    authority.done();

    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    let mut steps = super::tests::login_steps("person-a");
    steps[0].body["expires_in"] = json!(2);
    steps[2].gate = Some(gate.clone());
    let authority = Authority::new(steps).await;
    let dir = Directory::new();
    let pool = authority.pool(&dir);
    assert_eq!(
        pool.login_grok("personal", "synthetic-client", |_| {})
            .await
            .unwrap_err(),
        CredentialError::Expired
    );
    gate.notify_one();
    assert_eq!(
        pool.list(Some("grok")).unwrap()[0].state,
        AccountState::SignedOut
    );
    authority.done();
}

#[tokio::test]
async fn shared_browser_mechanism_rejects_overrides_and_never_falls_back_from_an_occupied_port() {
    use super::browser::{BrowserGrant, BrowserProfile};
    let occupied = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = occupied.local_addr().unwrap().port();
    let profile = || BrowserProfile {
        authorize: "https://authority.invalid/authorize",
        client: "synthetic-client",
        scopes: "openid",
        callback_path: "/auth/callback",
        extra: &[],
        timeout: Duration::from_secs(1),
    };
    assert!(matches!(
        BrowserGrant::begin(profile(), port).await,
        Err(CredentialError::Callback)
    ));
    let mut invalid = profile();
    invalid.extra = &[("state", "injected")];
    assert!(matches!(
        BrowserGrant::begin(invalid, 0).await,
        Err(CredentialError::Protocol)
    ));
}
