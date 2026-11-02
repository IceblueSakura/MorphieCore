//! Account admission through production construction, not a pool-shaped test adapter.
use super::test_support::{Fixture, service};
use axum::{
    Json, Router,
    http::{HeaderMap, StatusCode},
    routing::post,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

#[tokio::test]
async fn loader_binds_named_accounts_without_switching_after_dispatch() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let observed = calls.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}/custom/v1", listener.local_addr().unwrap());
    let upstream = tokio::spawn(async move {
        axum::serve(listener,Router::new().route("/custom/v1/responses",post(move |headers:HeaderMap,Json(body):Json<Value>| {
            let calls=observed.clone();
            async move {
                use axum::response::IntoResponse;
                assert_eq!(body["model"],"deepseek-flash");
                let first=headers["authorization"]=="Bearer synthetic-primary";
                calls.lock().unwrap().push(if first {"primary"}else{"backup"});
                if first {return StatusCode::SERVICE_UNAVAILABLE.into_response();}
                assert_eq!(headers["authorization"],"Bearer synthetic-backup");
                ([ ("content-type","text/event-stream") ],crate::test_responses_wire::response_stream(json!({
                    "object":"response","id":"synthetic-response","model":"deepseek-flash","created_at":1,
                    "status":"completed","output":[{"type":"message","id":"synthetic-message","role":"assistant","status":"completed",
                        "content":[{"type":"output_text","text":"healthy","annotations":[],"logprobs":[]}]}]
                }))).into_response()
            }
        }))).await.unwrap();
    });
    let accounts = json!({"version":1,"providers":{"deepseek":{"accounts":[
        {"name":"primary","enabled":true,"credential":{"kind":"api_key","alias":"primary"},"base_url":root},
        {"name":"backup","enabled":true,"credential":{"kind":"api_key","alias":"backup"},"base_url":root}
    ]}}});
    let boot = Fixture::with_accounts(
        service(),
        json!({"models":{"deepseek-flash":{"candidates":[{"binding":"deepseek-responses"}]}}}),
        &[
            ("deepseek", "primary", "synthetic-primary"),
            ("deepseek", "backup", "synthetic-backup"),
        ],
        accounts,
    )
    .unwrap();
    for expected in [StatusCode::SERVICE_UNAVAILABLE, StatusCode::OK] {
        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/v1/responses")
            .header(
                "authorization",
                "Bearer synthetic-gateway-client-token-0001",
            )
            .header("content-type", "application/json")
            .body(axum::body::Body::from(
                r#"{"model":"deepseek-flash","input":"go","store":false}"#,
            ))
            .unwrap();
        let response = boot.gateway.router().oneshot(request).await.unwrap();
        assert_eq!(response.status(), expected);
        let bytes = axum::body::to_bytes(response.into_body(), 65536)
            .await
            .unwrap();
        if expected == StatusCode::OK {
            let value: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(value["output"][0]["content"][0]["text"], "healthy");
        } else {
            assert_eq!(
                *calls.lock().unwrap(),
                ["primary"],
                "one dispatched target per request"
            );
        }
        let permit = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            boot.gateway.state.permits.acquire(),
        )
        .await
        .unwrap()
        .unwrap();
        drop(permit);
    }
    assert_eq!(*calls.lock().unwrap(), ["primary", "backup"]);
    boot.gateway.shutdown();
    upstream.abort();
    let _ = upstream.await;
}

#[test]
fn enabled_account_expansion_has_one_request_bound_independent_of_candidate_count() {
    let names: Vec<_> = (0..64).map(|n| format!("account-{n}")).collect();
    let keys: Vec<_> = names
        .iter()
        .map(|name| ("deepseek", name.as_str(), "synthetic-key"))
        .collect();
    let models =
        json!({"models":{"deepseek-flash":{"candidates":[{"binding":"deepseek-responses"}]}}});
    let single = Fixture::new(service(), models, &keys).unwrap();
    let entry = &single.gateway.state.entries[&(
        crate::protocol::ClientProtocol::Responses,
        "deepseek-flash".into(),
    )];
    assert_eq!(entry.candidates.len(), 64);
    assert!(
        Fixture::new(
            service(),
            json!({"models":{"deepseek-flash":{"candidates":[
                {"binding":"deepseek-responses"},{"binding":"deepseek-chat"}
            ]}}}),
            &keys
        )
        .is_err()
    );
}
#[tokio::test]
async fn read_only_validation_checks_observation_configuration_without_creating_sinks() {
    let boot = Fixture::new(
        service(),
        json!({"models":{"deepseek-flash":{"candidates":[{"binding":"deepseek-responses"}]}}}),
        &[("deepseek", "primary", "synthetic-primary")],
    )
    .unwrap();
    let root = boot.directory();
    let store = root.join("credentials");
    super::Bootstrap::validate(&root.join("gateway.json"), Some(&store), None).unwrap();
    let owned = b"{\"kind\":\"morphiecore_diagnostics\",\"version\":1,\"event\":\"file\"}\n{\"unfinished\":";
    super::test_support::write_private(&root.join("owned.jsonl"), owned);
    super::test_support::write_private(&root.join("foreign.jsonl"), b"unrelated synthetic file");
    super::test_support::write_private(
        &root.join("gateway.json"),
        &serde_json::to_vec(&json!({
            "version":1,"auth_file":"auth.json","diagnostics":"owned.jsonl"
        }))
        .unwrap(),
    );
    super::Bootstrap::validate(&root.join("gateway.json"), Some(&store), None).unwrap();
    assert_eq!(
        std::fs::read(root.join("owned.jsonl")).unwrap(),
        owned,
        "validation must not repair a partial tail"
    );
    assert!(!root.join("owned.jsonl.lock").exists());
    for invalid in [
        json!({"diagnostics":""}),
        json!({"diagnostics":"missing-directory/events.jsonl"}),
        json!({"diagnostics":"credentials"}),
        json!({"diagnostics":"foreign.jsonl"}),
        json!({"capture":{"path":"capture.jsonl","client_request":{"headers":true},"max_unit_bytes":0}}),
        json!({"capture":{"path":"foreign.jsonl","client_request":{"headers":true}}}),
        json!({"capture":{"path":"missing-directory/capture.jsonl","client_request":{"headers":true}}}),
    ] {
        let mut config = json!({"version":1,"auth_file":"auth.json","bind":"127.0.0.1:0"});
        config
            .as_object_mut()
            .unwrap()
            .extend(invalid.as_object().unwrap().clone());
        super::test_support::write_private(
            &root.join("gateway.json"),
            &serde_json::to_vec(&config).unwrap(),
        );
        assert!(
            super::Bootstrap::validate(&root.join("gateway.json"), Some(&store), None).is_err()
        );
        assert!(super::Bootstrap::from_directory(root).is_err());
    }
    assert!(!root.join("capture.jsonl").exists());
    assert!(!root.join("missing-directory").exists());
    assert_eq!(
        std::fs::read(root.join("foreign.jsonl")).unwrap(),
        b"unrelated synthetic file"
    );
}
#[test]
fn account_names_cannot_duplicate_one_security_target() {
    let mut accounts = json!({"version":1,"providers":{"deepseek":{"accounts":[
        {"name":"primary","enabled":true,"credential":{"kind":"api_key","alias":"shared"}},
        {"name":"another-name","enabled":true,"credential":{"kind":"api_key","alias":"shared"}}
    ]}}});
    let models =
        json!({"models":{"deepseek-flash":{"candidates":[{"binding":"deepseek-responses"}]}}});
    let keys = [("deepseek", "shared", "synthetic-key")];
    assert!(matches!(
        Fixture::with_accounts(service(), models.clone(), &keys, accounts.clone()),
        Err(super::StartupError::Binding)
    ));
    accounts["providers"]["deepseek"]["accounts"][1]["base_url"] =
        json!("https://relay.invalid/custom/v1");
    let distinct = Fixture::with_accounts(service(), models, &keys, accounts).unwrap();
    let entry = &distinct.gateway.state.entries[&(
        crate::protocol::ClientProtocol::Responses,
        "deepseek-flash".into(),
    )];
    assert_eq!(entry.candidates.len(), 2);
    assert_ne!(
        entry.candidates[0].cache_scope,
        entry.candidates[1].cache_scope
    );
}
