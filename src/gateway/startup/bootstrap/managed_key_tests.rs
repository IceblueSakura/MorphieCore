//! File activation and replay epoch rebinding, without inference traffic.
use super::*;
use crate::credential::{CredentialPool, CredentialRef};
fn setup() -> (tempfile::TempDir, CredentialManager) {
    let dir = crate::credential::tests::support::private_directory();
    let manager = CredentialManager::new(dir.path(), vec![]).unwrap();
    manager.write_gateway_config_for_test(
        &serde_json::json!({"client_key":"synthetic-gateway-key-at-least-32-bytes"}),
    );
    manager
        .add_api_key(
            "deepseek",
            "one",
            Secret::new("synthetic-managed-key".into()).unwrap(),
        )
        .unwrap();
    manager
        .set_pool(
            "deepseek",
            "deepseek-api-key",
            0,
            CredentialPool {
                members: vec![CredentialRef::ApiKey {
                    alias: "one".into(),
                }],
                fallback: false,
                max_attempts: 1,
            },
        )
        .unwrap();
    (dir, manager)
}

#[test]
fn bootstrap_loads_selected_key_and_requires_reactivation_after_rotation() {
    let (dir, manager) = setup();
    let first = Bootstrap::from_directory(dir.path()).unwrap();
    let entry = &first.gateway.state.entries[&(
        crate::gateway::family(Profile::Responses),
        "deepseek-flash".into(),
    )];
    let candidate = &entry.candidates[0];
    let (key, grant) = candidate.secret.resolve().unwrap();
    assert!(grant.is_none());
    let (_, request) = crate::gateway::ingress::admission::prepare(
        &first.gateway.state,
        Profile::Responses,
        br#"{"model":"deepseek-flash","input":"synthetic request"}"#,
    )
    .unwrap();
    let prepared =
        crate::execution::prepare(&candidate.endpoint, &candidate.provider, &key, &request)
            .unwrap();
    assert_eq!(prepared.auth_header.1, "Bearer synthetic-managed-key");
    manager
        .replace_api_key(
            "deepseek",
            "one",
            1,
            Secret::new("synthetic-replacement".into()).unwrap(),
        )
        .unwrap();
    assert!(candidate.secret.resolve().is_err());
    let next = Bootstrap::from_directory(dir.path()).unwrap();
    let next = &next.gateway.state.entries[&(
        crate::gateway::family(Profile::Responses),
        "deepseek-flash".into(),
    )];
    assert_eq!(
        entry.client.as_openai().unwrap().1.replay_origin,
        next.client.as_openai().unwrap().1.replay_origin
    );
    assert_ne!(candidate.cache_scope, next.candidates[0].cache_scope);
    assert_eq!(
        candidate
            .endpoint
            .representation
            .openai()
            .unwrap()
            .adaptation
            .scope,
        next.candidates[0]
            .endpoint
            .representation
            .openai()
            .unwrap()
            .adaptation
            .scope
    );
}
