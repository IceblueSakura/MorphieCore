//! Private Agent-key schema, admission identity and restart semantics.
use super::test_support::{Fixture, selected_models, service, write_private};
use super::*;
use axum::{body::Body, http::Request};
use serde_json::json;
use tower::ServiceExt;

const FIRST: &str = "synthetic-first-agent-secret-00000001";
const SECOND: &str = "synthetic-second-agent-secret-0000002";
const ROTATED: &str = "synthetic-rotated-agent-secret-000003";

fn fixture() -> Fixture {
    Fixture::new(
        service(),
        selected_models(&["deepseek-flash"], &["deepseek"]),
        &[("deepseek", "deepseek-api-key", "synthetic-upstream")],
    )
    .unwrap()
}

#[test]
fn auth_file_is_required_private_strict_and_versioned() {
    let fixture = fixture();
    let root = fixture.directory();
    let path = root.join("auth.json");
    let valid = json!({"version":1,"agents":[{"agent_id":"one","key":FIRST}]});
    for invalid in [
        json!({}),
        json!(null),
        json!({"one":FIRST}),
        json!({"agents":[]}),
        json!({"version":2,"agents":[]}),
        json!({"version":1,"agents":null}),
        json!({"version":1,"agents":[{"agent_id":"one","key":FIRST,"extra":true}]}),
        json!({"version":1,"agents":[{"agent_id":"one","key":FIRST,"name":null}]}),
        json!({"version":1,"agents":[{"agent_id":"one","key":FIRST,"desc":42}]}),
        json!({"version":1,"agents":[{"agent_id":"one","key":FIRST,"name":"x".repeat(257)}]}),
        json!({"version":1,"agents":[{"agent_id":"one","key":FIRST,"name":"bad\nname"}]}),
        json!({"version":1,"agents":[{"agent_id":"one","key":FIRST,"desc":"x".repeat(2049)}]}),
        json!({"version":1,"agents":[{"agent_id":"one","key":FIRST},{"agent_id":"one","key":SECOND}]}),
    ] {
        write_private(&path, &serde_json::to_vec(&invalid).unwrap());
        assert!(matches!(
            Bootstrap::from_directory(root),
            Err(StartupError::Configuration("auth"))
        ));
    }
    for agents in [
        json!([]),
        json!([{"agent_id":"one","key":"short"}]),
        json!([{"agent_id":"bad/id","key":FIRST}]),
        json!([{"agent_id":"one","key":FIRST},{"agent_id":"two","key":FIRST}]),
    ] {
        write_private(
            &path,
            &serde_json::to_vec(&json!({"version":1,"agents":agents})).unwrap(),
        );
        assert!(matches!(
            Bootstrap::from_directory(root),
            Err(StartupError::Credentials)
        ));
    }
    write_private(&path, br#"{"version":1,"version":1,"agents":[]}"#);
    assert!(matches!(
        Bootstrap::from_directory(root),
        Err(StartupError::Configuration("auth"))
    ));
    write_private(&path, &vec![b' '; 65537]);
    assert!(matches!(
        Bootstrap::from_directory(root),
        Err(StartupError::Configuration("auth"))
    ));
    std::fs::remove_file(&path).unwrap();
    assert!(matches!(
        Bootstrap::from_directory(root),
        Err(StartupError::Configuration("auth"))
    ));

    write_private(&path, &serde_json::to_vec(&valid).unwrap());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(
            Bootstrap::from_directory(root),
            Err(StartupError::Configuration("auth"))
        ));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    assert!(Bootstrap::from_directory(root).is_ok());
    write_private(
        &root.join("gateway.json"),
        &serde_json::to_vec(&json!({"client_key":FIRST})).unwrap(),
    );
    assert!(matches!(
        Bootstrap::from_directory(root),
        Err(StartupError::Configuration("gateway"))
    ));
}

#[tokio::test]
async fn configured_agents_keep_identity_across_rename_and_key_rotation() {
    let fixture = fixture();
    let root = fixture.directory();
    write_private(
        &root.join("gateway.json"),
        &serde_json::to_vec(&json!({"auth_file":"agent-keys.json"})).unwrap(),
    );
    let mut configuration = json!({"version":1,"agents":[
        {"agent_id":"pi","name":"Workstation Pi","key":FIRST,"desc":"Private operator note"},
        {"agent_id":"opencode","key":SECOND}
    ]});
    let write_auth = |configuration: &serde_json::Value| {
        write_private(
            &root.join("agent-keys.json"),
            &serde_json::to_vec(configuration).unwrap(),
        );
    };
    write_auth(&configuration);
    let first = Bootstrap::from_directory(root).unwrap();
    assert!(first.gateway.state.diagnostics.is_none());
    assert!(first.gateway.state.capture.is_none());
    let headers = |key: &str| {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-api-key", key.parse().unwrap());
        headers
    };
    for (header, key, expected) in [
        ("authorization", format!("Bearer {FIRST}"), "pi"),
        ("x-api-key", SECOND.into(), "opencode"),
    ] {
        let request = Request::builder()
            .uri("/v1/models")
            .header(header, key)
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            first
                .gateway
                .state
                .auth
                .authenticate(request.headers())
                .unwrap()
                .agent_id
                .as_ref(),
            expected
        );
        let response = first.gateway.router().oneshot(request).await.unwrap();
        assert_eq!(response.status(), 200);
        let body = axum::body::to_bytes(response.into_body(), 65536)
            .await
            .unwrap();
        let body = std::str::from_utf8(&body).unwrap();
        for private in [
            FIRST,
            SECOND,
            "\"pi\"",
            "\"opencode\"",
            "Workstation Pi",
            "Private operator note",
        ] {
            assert!(!body.contains(private));
        }
    }
    let original = first
        .gateway
        .state
        .auth
        .authenticate(&headers(FIRST))
        .unwrap();
    configuration["agents"].as_array_mut().unwrap().reverse();
    write_auth(&configuration);
    let reordered = Bootstrap::from_directory(root).unwrap();
    assert_eq!(
        reordered.gateway.state.auth.digest,
        first.gateway.state.auth.digest
    );
    configuration["agents"].as_array_mut().unwrap().reverse();
    configuration["agents"][0]["name"] = json!("Renamed Pi");
    configuration["agents"][0]["desc"] = json!("Updated note");
    write_auth(&configuration);
    let renamed = Bootstrap::from_directory(root).unwrap();
    assert_eq!(
        renamed.gateway.state.auth.digest,
        first.gateway.state.auth.digest
    );
    assert_eq!(
        renamed.gateway.state.auth.authenticate(&headers(FIRST)),
        Some(original.clone())
    );

    configuration["agents"][0]["key"] = json!(ROTATED);
    configuration["agents"].as_array_mut().unwrap().pop();
    write_auth(&configuration);
    assert!(
        first
            .gateway
            .state
            .auth
            .authenticate(&headers(FIRST))
            .is_some()
    );
    assert!(
        first
            .gateway
            .state
            .auth
            .authenticate(&headers(SECOND))
            .is_some()
    );
    assert!(
        first
            .gateway
            .state
            .auth
            .authenticate(&headers(ROTATED))
            .is_none()
    );
    let restarted = Bootstrap::from_directory(root).unwrap();
    assert_eq!(
        restarted.gateway.state.auth.authenticate(&headers(ROTATED)),
        Some(original)
    );
    for key in [FIRST, SECOND] {
        let response = restarted
            .gateway
            .router()
            .oneshot(
                Request::builder()
                    .uri("/v1/models")
                    .header("x-api-key", key)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 401);
    }
}
