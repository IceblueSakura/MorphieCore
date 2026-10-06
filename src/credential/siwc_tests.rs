//! SIWC login availability is independent of legacy product clients.
use super::browser_tests::request;
use super::test_support::{Authority, Directory, read_account, step};
use super::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD as B64};
use serde_json::{Value, json};
use std::{collections::BTreeMap, net::SocketAddr, sync::Mutex, time::Duration};
use tokio::time::timeout;

const CLIENT: &str = "oaiapp_synthetic";
const SCOPES: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
fn keys() -> Value {
    json!({"keys":[serde_json::from_str::<Value>(include_str!("../../tests/fixtures/auth/codex-test-jwk.json")).unwrap()]})
}
fn claims(nonce: &str) -> Value {
    json!({"iss":"https://auth.openai.com","aud":CLIENT,"sub":"person-a",
        "exp":model::now().unwrap()+3600,"nonce":nonce})
}
fn sign(header: Value, claims: Value) -> String {
    let pair = ring::signature::RsaKeyPair::from_pkcs8(include_bytes!(
        "../../tests/fixtures/auth/codex-test-rsa.pk8"
    ))
    .unwrap();
    let message = format!(
        "{}.{}",
        B64.encode(serde_json::to_vec(&header).unwrap()),
        B64.encode(serde_json::to_vec(&claims).unwrap())
    );
    let mut signature = vec![0; pair.public().modulus_len()];
    pair.sign(
        &ring::signature::RSA_PKCS1_SHA256,
        &ring::rand::SystemRandom::new(),
        message.as_bytes(),
        &mut signature,
    )
    .unwrap();
    format!("{message}.{}", B64.encode(signature))
}
fn token(nonce: &str, scopes: &str) -> Value {
    json!({"access_token":"siwc-access","refresh_token":"siwc-refresh",
        "id_token":sign(json!({"alg":"RS256","kid":"synthetic-codex-key"}),claims(nonce)),
        "scope":scopes,"token_type":"Bearer","expires_in":3600})
}
fn options() -> LoginOptions {
    LoginOptions {
        method: LoginMethod::Browser,
        ..Default::default()
    }
}
async fn login(
    manager: &CredentialManager,
    authority: &Authority,
    initial: bool,
    scopes: &str,
    callback_client: Option<&str>,
    bad_nonce: bool,
) -> Result<AccountStatus, CredentialError> {
    let (send, recv) = tokio::sync::oneshot::channel();
    let send = Mutex::new(Some(send));
    let mut login = Box::pin(manager.login("siwc", "personal", options(), |p| {
        let LoginPrompt::Browser(p) = p else {
            panic!("browser only")
        };
        send.lock()
            .unwrap()
            .take()
            .unwrap()
            .send(p.clone())
            .unwrap();
    }));
    let prompt = timeout(Duration::from_secs(3), async {
        tokio::select! { v = recv => v.unwrap(), r = &mut login => panic!("early login {r:?}") }
    })
    .await
    .unwrap();
    let url = url::Url::parse(&prompt.authorization_url).unwrap();
    assert_eq!(
        url.origin().ascii_serialization(),
        "https://auth.openai.com"
    );
    assert_eq!(url.path(), "/api/accounts/authorize");
    let fields: BTreeMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(
        fields["client_id"],
        if initial {
            "dynamic_agent_client"
        } else {
            CLIENT
        }
    );
    assert_eq!(
        fields.get("agent_name_hint").map(String::as_str),
        initial.then_some("MorphieCore")
    );
    assert_eq!(fields["resource"], "https://api.openai.com/v1");
    assert_eq!(fields["scope"], SCOPES);
    assert_eq!(fields["response_type"], "code");
    assert_eq!(fields["code_challenge_method"], "S256");
    assert!(store::valid_host_id(&fields["ext_agent_host_id"]));
    assert_ne!(fields["state"], fields["nonce"]);
    assert_eq!(fields["state"].len(), 43);
    assert_eq!(fields["nonce"].len(), 43);
    assert!(!fields.contains_key("originator") && !fields.contains_key("id_token_hint"));
    if let Some(next) = authority.steps.lock().unwrap().front_mut()
        && next.status == axum::http::StatusCode::OK
    {
        next.body = token(if bad_nonce { "wrong" } else { &fields["nonce"] }, scopes);
    }
    let redirect = url::Url::parse(&prompt.redirect_uri).unwrap();
    assert_eq!(redirect.scheme(), "http");
    assert_eq!(redirect.host_str(), Some("127.0.0.1"));
    assert_eq!(redirect.path(), "/auth/callback");
    let address = SocketAddr::from(([127, 0, 0, 1], redirect.port().unwrap()));
    let target = format!(
        "/auth/callback?state={}&code=synthetic-code{}",
        fields["state"],
        callback_client
            .map(|c| format!("&client_id={c}"))
            .unwrap_or_default()
    );
    let callbacks = async {
        let rejected = request(address, "/auth/callback?state=wrong&error=access_denied").await;
        assert!(rejected.starts_with("HTTP/1.1 400"));
        request(address, &target).await
    };
    let (result, response) = timeout(Duration::from_secs(5), async {
        tokio::join!(&mut login, callbacks)
    })
    .await
    .unwrap();
    assert!(response.contains("Callback received") && !response.contains("synthetic-code"));
    if let Some(call) = authority.forms.lock().unwrap().last() {
        assert_eq!(call.fields.len(), 6);
        assert_eq!(call.fields["client_id"], CLIENT);
        assert_eq!(call.fields["resource"], "https://api.openai.com/v1");
        assert_eq!(call.fields["redirect_uri"], prompt.redirect_uri);
        assert_eq!(
            B64.encode(
                ring::digest::digest(
                    &ring::digest::SHA256,
                    call.fields["code_verifier"].as_bytes()
                )
                .as_ref()
            ),
            fields["code_challenge"]
        );
    }
    result
}
fn login_steps() -> Vec<test_support::Step> {
    vec![
        step(
            "/api/accounts/oauth/token",
            vec![("grant_type", "authorization_code"), ("client_id", CLIENT)],
            json!({}),
        ),
        step("/.well-known/jwks.json", vec![], keys()),
    ]
}

#[test]
fn builtin_siwc_is_a_dynamic_browser_profile() {
    assert_eq!(siwc::APPLICATION_NAME, "MorphieCore");
    assert_eq!(
        siwc::APPLICATION_DESCRIPTION,
        "分布式Agent Runtime的测试性路由网关"
    );
    let drivers = builtin_drivers(None).unwrap();
    assert!(
        !drivers
            .iter()
            .any(|d| matches!(d.profile(), "codex" | "chatgpt"))
    );
    let driver = drivers
        .iter()
        .find(|d| d.profile() == "siwc")
        .expect("SIWC profile must be registered");
    assert!(driver.login_client(&LoginOptions::default()).is_err());
    assert_eq!(
        driver
            .login_client(&LoginOptions {
                method: LoginMethod::Browser,
                client_id: None,
                callback_port: Some(0),
                consent: false,
            })
            .unwrap(),
        "dynamic_agent_client"
    );
}

#[tokio::test]
async fn registration_returning_login_and_identity_only_access_are_separate() {
    let authority = Authority::new(login_steps()).await;
    let dir = Directory::new();
    let manager = authority.pool(&dir);
    login(
        &manager,
        &authority,
        true,
        "openid profile email",
        Some(CLIENT),
        false,
    )
    .await
    .unwrap();
    let binding = manager.bind_access("siwc", "personal").unwrap();
    assert_eq!(binding.borrow().unwrap_err(), CredentialError::Permission);
    let before = read_account(&dir.path, "siwc", "personal");
    assert_eq!(before["client_id"], CLIENT);
    assert!(before["identity"]["scope"].is_null());
    let host = manager
        .store
        .transaction()
        .unwrap()
        .host_id("siwc")
        .unwrap();
    authority.steps.lock().unwrap().extend(login_steps());
    login(&manager, &authority, false, SCOPES, None, false)
        .await
        .unwrap();
    assert_eq!(binding.borrow().unwrap().access.expose(), "siwc-access");
    assert_eq!(
        manager
            .store
            .transaction()
            .unwrap()
            .host_id("siwc")
            .unwrap(),
        host
    );
    authority.steps.lock().unwrap().extend(login_steps());
    assert_eq!(
        login(&manager, &authority, false, SCOPES, Some(CLIENT), true)
            .await
            .unwrap_err(),
        CredentialError::Protocol
    );
    assert_eq!(binding.borrow().unwrap().access.expose(), "siwc-access");
    authority.done();
}

#[tokio::test]
async fn invalid_grant_retains_issued_client_and_never_repeats_dynamic_registration() {
    let mut step = step(
        "/api/accounts/oauth/token",
        vec![("client_id", CLIENT)],
        json!({"error":"invalid_grant"}),
    );
    step.status = axum::http::StatusCode::BAD_REQUEST;
    let authority = Authority::new(vec![step]).await;
    let dir = Directory::new();
    let manager = authority.pool(&dir);
    assert_eq!(
        login(&manager, &authority, true, SCOPES, Some(CLIENT), false)
            .await
            .unwrap_err(),
        CredentialError::InvalidGrant
    );
    assert_eq!(
        read_account(&dir.path, "siwc", "personal")["client_id"],
        CLIENT
    );
    authority.steps.lock().unwrap().extend(login_steps());
    login(&manager, &authority, false, SCOPES, None, false)
        .await
        .unwrap();
    authority.done();
}

#[tokio::test]
async fn rotated_material_survives_jwks_failure_and_recovers_without_token_reuse() {
    let authority = Authority::new(login_steps()).await;
    let dir = Directory::new();
    let manager = authority.pool(&dir);
    login(&manager, &authority, true, SCOPES, Some(CLIENT), false)
        .await
        .unwrap();
    let binding = manager.bind_access("siwc", "personal").unwrap();
    let mut replacement = token("", SCOPES);
    replacement["access_token"] = json!("new-access");
    replacement["refresh_token"] = json!("replacement-refresh");
    let mut failure = step(
        "/.well-known/jwks.json",
        vec![],
        json!({"error":"unavailable"}),
    );
    failure.status = axum::http::StatusCode::SERVICE_UNAVAILABLE;
    authority.steps.lock().unwrap().extend([
        step(
            "/api/accounts/oauth/token",
            vec![
                ("grant_type", "refresh_token"),
                ("client_id", CLIENT),
                ("refresh_token", "siwc-refresh"),
                ("resource", "https://api.openai.com/v1"),
            ],
            replacement,
        ),
        failure,
    ]);
    assert!(manager.refresh("siwc", "personal").await.is_err());
    let state = manager.list(Some("siwc")).unwrap().remove(0);
    assert!(state.renewal_pending);
    assert!(binding.borrow().is_err());
    let stored = read_account(&dir.path, "siwc", "personal");
    assert!(stored["credential"].is_null());
    assert_eq!(
        stored["pending_renewal"]["credential"]["refresh"],
        "replacement-refresh"
    );
    authority
        .steps
        .lock()
        .unwrap()
        .push_back(step("/.well-known/jwks.json", vec![], keys()));
    let reopened = authority.pool(&dir);
    let state = reopened.refresh("siwc", "personal").await.unwrap();
    assert!(!state.renewal_pending);
    assert_eq!(binding.borrow().unwrap().access.expose(), "new-access");
    assert!(
        !authority
            .forms
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .fields
            .contains_key("scope")
    );
    authority.done();
}

#[test]
fn rsa_identity_is_bound_to_issuer_client_nonce_and_algorithm_not_product_metadata() {
    let header = json!({"alg":"RS256","kid":"synthetic-codex-key"});
    let good = sign(header.clone(), claims("nonce"));
    assert_eq!(
        siwc::verify_identity(&good, &keys(), CLIENT, Some("nonce"))
            .unwrap()
            .subject,
        "person-a"
    );
    for (field, value) in [
        ("iss", json!("https://untrusted.invalid")),
        ("aud", json!("other-client")),
        ("nonce", json!("wrong")),
        ("sub", json!("")),
        ("exp", json!(1)),
    ] {
        let mut c = claims("nonce");
        c[field] = value;
        assert!(
            siwc::verify_identity(&sign(header.clone(), c), &keys(), CLIENT, Some("nonce"))
                .is_err()
        );
    }
    for h in [
        json!({"alg":"none","kid":"synthetic-codex-key"}),
        json!({"alg":"HS256","kid":"synthetic-codex-key"}),
        json!({"alg":"RS256","kid":"missing"}),
        json!({"alg":"RS256","kid":"synthetic-codex-key","crit":["unknown"]}),
    ] {
        assert!(
            siwc::verify_identity(&sign(h, claims("nonce")), &keys(), CLIENT, Some("nonce"))
                .is_err()
        );
    }
    let mut duplicate = keys();
    duplicate["keys"]
        .as_array_mut()
        .unwrap()
        .push(keys()["keys"][0].clone());
    assert!(siwc::verify_identity(&good, &duplicate, CLIENT, Some("nonce")).is_err());
}

#[tokio::test]
async fn callback_must_issue_a_client_and_cannot_replace_an_existing_registration() {
    for client in [None, Some("dynamic_agent_client")] {
        let authority = Authority::new(vec![]).await;
        let dir = Directory::new();
        let manager = authority.pool(&dir);
        assert!(
            login(&manager, &authority, true, SCOPES, client, false)
                .await
                .is_err()
        );
        assert!(manager.bind_access("siwc", "personal").is_err());
        assert!(authority.forms.lock().unwrap().is_empty());
    }
    let authority = Authority::new(login_steps()).await;
    let dir = Directory::new();
    let manager = authority.pool(&dir);
    login(&manager, &authority, true, SCOPES, Some(CLIENT), false)
        .await
        .unwrap();
    // The helper checks the last exchange against this attempt; there must be none.
    authority.forms.lock().unwrap().clear();
    assert_eq!(
        login(
            &manager,
            &authority,
            false,
            SCOPES,
            Some("oaiapp_other"),
            false
        )
        .await
        .unwrap_err(),
        CredentialError::IdentityMismatch
    );
    assert_eq!(
        read_account(&dir.path, "siwc", "personal")["client_id"],
        CLIENT
    );
    assert_eq!(
        manager
            .bind_access("siwc", "personal")
            .unwrap()
            .borrow()
            .unwrap()
            .access
            .expose(),
        "siwc-access"
    );
    authority.done();
}

#[tokio::test]
async fn renewal_omission_inherits_identity_but_scope_loss_disables_plan_usage() {
    let authority = Authority::new(login_steps()).await;
    let dir = Directory::new();
    let manager = authority.pool(&dir);
    login(&manager, &authority, true, SCOPES, Some(CLIENT), false)
        .await
        .unwrap();
    authority.steps.lock().unwrap().push_back(step(
        "/api/accounts/oauth/token",
        vec![("refresh_token", "siwc-refresh")],
        json!({"access_token":"identity-only-access","refresh_token":"replacement-refresh",
            "expires_in":3600,"token_type":"Bearer","scope":"openid profile email offline_access"}),
    ));
    let status = manager.refresh("siwc", "personal").await.unwrap();
    assert_eq!(status.plan_usage_enabled, Some(false));
    assert!(!status.renewal_pending);
    assert_eq!(
        manager
            .bind_access("siwc", "personal")
            .unwrap()
            .borrow()
            .unwrap_err(),
        CredentialError::Permission
    );
    authority.done();
}

#[tokio::test]
async fn logout_discovers_only_the_pinned_revoke_target_and_keeps_registration() {
    let authority = Authority::new(login_steps()).await;
    let dir = Directory::new();
    let manager = authority.pool(&dir);
    login(&manager, &authority, true, SCOPES, Some(CLIENT), false)
        .await
        .unwrap();
    let host = manager
        .store
        .transaction()
        .unwrap()
        .host_id("siwc")
        .unwrap();
    authority.steps.lock().unwrap().extend([
        step(
            "/.well-known/openid-configuration",
            vec![],
            json!({"issuer":"https://auth.openai.com",
            "revocation_endpoint":"https://auth.openai.com/api/accounts/oauth/revoke"}),
        ),
        step(
            "/api/accounts/oauth/revoke",
            vec![
                ("client_id", CLIENT),
                ("token", "siwc-refresh"),
                ("token_type_hint", "refresh_token"),
            ],
            json!({}),
        ),
    ]);
    assert_eq!(
        manager.logout("siwc", "personal", true).await.unwrap(),
        LogoutOutcome::Revoked
    );
    let record = read_account(&dir.path, "siwc", "personal");
    assert_eq!(record["client_id"], CLIENT);
    assert!(record["credential"].is_null() && record["pending_renewal"].is_null());
    assert_eq!(
        manager
            .store
            .transaction()
            .unwrap()
            .host_id("siwc")
            .unwrap(),
        host
    );
    assert_eq!(
        authority.forms.lock().unwrap().last().unwrap().fields.len(),
        3
    );
    authority.steps.lock().unwrap().extend(login_steps());
    login(&manager, &authority, false, SCOPES, None, false)
        .await
        .unwrap();
    authority.steps.lock().unwrap().push_back(step("/.well-known/openid-configuration", vec![],
        json!({"issuer":"https://auth.openai.com","revocation_endpoint":"https://untrusted.invalid/revoke"})));
    assert_eq!(
        manager.logout("siwc", "personal", true).await.unwrap(),
        LogoutOutcome::RevocationUnconfirmed
    );
    assert!(
        manager
            .bind_access("siwc", "personal")
            .unwrap()
            .borrow()
            .is_err()
    );
    authority.done();
}

#[tokio::test]
async fn uncertain_refresh_never_reuses_the_consumed_refresh_token() {
    let authority = Authority::new(login_steps()).await;
    let dir = Directory::new();
    let manager = authority.pool(&dir);
    login(&manager, &authority, true, SCOPES, Some(CLIENT), false)
        .await
        .unwrap();
    let mut failure = step(
        "/api/accounts/oauth/token",
        vec![("refresh_token", "siwc-refresh")],
        json!({"error":"refresh_token_reused","error_description":"private"}),
    );
    failure.status = axum::http::StatusCode::BAD_REQUEST;
    authority.steps.lock().unwrap().push_back(failure);
    assert_eq!(
        manager.refresh("siwc", "personal").await.unwrap_err(),
        CredentialError::GrantReused
    );
    assert_eq!(
        manager.refresh("siwc", "personal").await.unwrap_err(),
        CredentialError::LoginRequired
    );
    assert!(read_account(&dir.path, "siwc", "personal")["credential"].is_null());
    authority.done();
}

#[tokio::test]
async fn cancellation_during_pending_verification_is_recoverable_after_restart() {
    use std::sync::Arc;
    let authority = Authority::new(login_steps()).await;
    let dir = Directory::new();
    let manager = authority.pool(&dir);
    login(&manager, &authority, true, SCOPES, Some(CLIENT), false)
        .await
        .unwrap();
    let gate = Arc::new(tokio::sync::Notify::new());
    let mut jwks = step("/.well-known/jwks.json", vec![], keys());
    jwks.gate = Some(gate.clone());
    authority.steps.lock().unwrap().extend([
        step("/api/accounts/oauth/token", vec![], token("", SCOPES)),
        jwks,
    ]);
    let mut renewal = Box::pin(manager.refresh("siwc", "personal"));
    timeout(Duration::from_secs(3), async {
        loop {
            tokio::select! {
                r = &mut renewal => panic!("must wait at JWKS: {r:?}"),
                _ = authority.entered.notified() => {
                    if authority.steps.lock().unwrap().is_empty() { break; }
                }
            }
        }
    })
    .await
    .unwrap();
    drop(renewal);
    assert!(manager.list(Some("siwc")).unwrap()[0].renewal_pending);
    gate.notify_one();
    authority
        .steps
        .lock()
        .unwrap()
        .push_back(step("/.well-known/jwks.json", vec![], keys()));
    assert!(
        !authority
            .pool(&dir)
            .refresh("siwc", "personal")
            .await
            .unwrap()
            .renewal_pending
    );
    authority.done();
}

#[test]
fn siwc_never_forms_a_rotation_pool_or_reuses_product_metadata() {
    let reference = CredentialRef::OAuth {
        profile: "siwc".into(),
        alias: "personal".into(),
    };
    for (members, fallback, max_attempts, accepted) in [
        (vec![reference.clone()], false, 1, true),
        (vec![reference.clone()], true, 1, false),
        (vec![reference.clone()], false, 2, false),
        (
            vec![
                reference,
                CredentialRef::OAuth {
                    profile: "siwc".into(),
                    alias: "other".into(),
                },
            ],
            false,
            1,
            false,
        ),
    ] {
        assert_eq!(
            CredentialPool {
                members,
                fallback,
                max_attempts
            }
            .validate()
            .is_ok(),
            accepted
        );
    }
    let grant = AccessGrant {
        access: Secret::new("synthetic".into()).unwrap(),
        identity: VerifiedIdentity {
            subject: "subject".into(),
            scope: None,
        },
    };
    assert_eq!(
        crate::provider::subscription::headers(
            crate::provider::AuthScheme::OAuthBearer("siwc"),
            &grant
        )
        .unwrap(),
        vec![("user-agent".into(), siwc::USER_AGENT.into())]
    );
}
#[tokio::test]
async fn changed_subject_in_replacement_remains_quarantined_until_explicit_logout() {
    let authority = Authority::new(login_steps()).await;
    let dir = Directory::new();
    let manager = authority.pool(&dir);
    login(&manager, &authority, true, SCOPES, Some(CLIENT), false)
        .await
        .unwrap();
    let mut replacement = token("", SCOPES);
    let mut claim = claims("");
    claim["sub"] = json!("other-person");
    replacement["id_token"] = json!(sign(
        json!({"alg":"RS256","kid":"synthetic-codex-key"}),
        claim
    ));
    authority.steps.lock().unwrap().extend([
        step("/api/accounts/oauth/token", vec![], replacement),
        step("/.well-known/jwks.json", vec![], keys()),
    ]);
    assert_eq!(
        manager.refresh("siwc", "personal").await.unwrap_err(),
        CredentialError::IdentityMismatch
    );
    assert!(manager.list(Some("siwc")).unwrap()[0].renewal_pending);
    assert!(
        manager
            .bind_access("siwc", "personal")
            .unwrap()
            .borrow()
            .is_err()
    );
    assert_eq!(
        manager.logout("siwc", "personal", false).await.unwrap(),
        LogoutOutcome::LocalOnly
    );
    let stored = read_account(&dir.path, "siwc", "personal");
    assert!(stored["credential"].is_null() && stored["pending_renewal"].is_null());
    assert_eq!(stored["identity"]["subject"], "person-a");
    authority.done();
}
#[test]
fn rsa_key_policy_signature_and_multiple_audiences_do_not_use_header_authority() {
    let header = json!({"alg":"RS256","kid":"synthetic-codex-key"});
    let good = sign(header.clone(), claims("nonce"));
    for (field, value) in [
        ("use", json!("enc")),
        ("key_ops", json!(["sign"])),
        ("alg", json!("ES256")),
    ] {
        let mut keys = keys();
        keys["keys"][0][field] = value;
        assert!(siwc::verify_identity(&good, &keys, CLIENT, Some("nonce")).is_err());
    }
    let mut forged = good.as_bytes().to_vec();
    let i = good.rfind('.').unwrap() + 8;
    forged[i] = if forged[i] == b'A' { b'B' } else { b'A' };
    assert!(
        siwc::verify_identity(
            std::str::from_utf8(&forged).unwrap(),
            &keys(),
            CLIENT,
            Some("nonce")
        )
        .is_err()
    );
    let mut claim = claims("nonce");
    claim["aud"] = json!([CLIENT, "other-client"]);
    assert!(
        siwc::verify_identity(
            &sign(header.clone(), claim.clone()),
            &keys(),
            CLIENT,
            Some("nonce")
        )
        .is_err()
    );
    claim["azp"] = json!(CLIENT);
    let mut header = header;
    header["jku"] = json!("https://untrusted.invalid/keys");
    header["x5u"] = json!("https://untrusted.invalid/cert");
    header["jwk"] = json!({"kty":"oct","k":"c3ludGhldGlj"});
    assert!(siwc::verify_identity(&sign(header, claim), &keys(), CLIENT, Some("nonce")).is_ok());
}
