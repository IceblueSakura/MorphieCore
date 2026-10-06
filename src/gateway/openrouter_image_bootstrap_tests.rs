//! Synthetic private-file activation, without inference or account discovery.
use super::*;
use crate::credential::{CredentialPool, CredentialRef};
#[test]
fn openrouter_images_need_explicit_model_selection_and_a_single_nonfallback_source() {
    let dir = crate::credential::test_support::private_directory();
    let manager = CredentialManager::new(dir.path(), vec![]).unwrap();
    manager
        .add_api_key(
            "openrouter",
            "one",
            Secret::new("synthetic-openrouter-key".into()).unwrap(),
        )
        .unwrap();
    let status = manager
        .set_pool(
            "openrouter",
            "openrouter-api-key",
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
    manager.write_gateway_config_for_test(&serde_json::json!({"client_key":"synthetic-gateway-key-at-least-32-bytes","models":["gpt-image-2.5-flare"]}));
    let boot = Bootstrap::from_directory(dir.path()).unwrap();
    assert!(boot.gateway.state.entries.is_empty());
    assert!(
        boot.gateway
            .state
            .images
            .contains_key("gpt-image-2.5-flare")
    );
    manager.write_gateway_config_for_test(
        &serde_json::json!({"client_key":"synthetic-gateway-key-at-least-32-bytes"}),
    );
    let boot = Bootstrap::from_directory(dir.path()).unwrap();
    assert!(
        boot.gateway.state.images.is_empty(),
        "new media cannot enlarge default activation"
    );
    assert!(!boot.gateway.state.entries.is_empty());
    manager.write_gateway_config_for_test(&serde_json::json!({"client_key":"synthetic-gateway-key-at-least-32-bytes","models":["gpt-image-2.5-flare"]}));
    manager
        .set_pool(
            "openrouter",
            "openrouter-api-key",
            status.revision,
            CredentialPool {
                members: vec![CredentialRef::ApiKey {
                    alias: "one".into(),
                }],
                fallback: true,
                max_attempts: 1,
            },
        )
        .unwrap();
    assert!(Bootstrap::from_directory(dir.path()).is_err());
}
