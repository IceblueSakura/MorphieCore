//! Explicit synthetic embedding activation cannot enlarge the default Generation surface.
use super::*;
use crate::credential::{CredentialPool, CredentialRef};
#[test]
fn embeddings_need_explicit_selection_and_a_single_nonfallback_key() {
    for binding in catalog::EMBEDDING_BINDINGS {
        let provider = (binding.provider)();
        let dir = crate::credential::test_support::private_directory();
        let manager = CredentialManager::new(dir.path(), vec![]).unwrap();
        manager
            .add_api_key(
                provider.id.as_str(),
                "one",
                Secret::new("synthetic-embedding-key".into()).unwrap(),
            )
            .unwrap();
        let mut status = manager
            .set_pool(
                provider.id.as_str(),
                binding.credential,
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
        manager.write_gateway_config_for_test(&serde_json::json!({
            "client_key":"synthetic-gateway-key-at-least-32-bytes","models":[binding.model]}));
        let boot = Bootstrap::from_directory(dir.path()).unwrap();
        assert!(boot.gateway.state.entries.is_empty());
        assert_eq!(boot.gateway.state.embeddings.len(), 1);
        assert!(boot.gateway.state.models.models.contains_key(binding.model));
        manager.write_gateway_config_for_test(&serde_json::json!({
            "client_key":"synthetic-gateway-key-at-least-32-bytes"}));
        let boot = Bootstrap::from_directory(dir.path()).unwrap();
        assert!(boot.gateway.state.embeddings.is_empty());
        assert!(!boot.gateway.state.entries.is_empty());
        manager.write_gateway_config_for_test(&serde_json::json!({
            "client_key":"synthetic-gateway-key-at-least-32-bytes","models":[binding.model]}));
        manager
            .add_api_key(
                provider.id.as_str(),
                "two",
                Secret::new("synthetic-second-key".into()).unwrap(),
            )
            .unwrap();
        for (aliases, fallback) in [(&["one", "two"][..], false), (&["one"][..], true)] {
            status = manager
                .set_pool(
                    provider.id.as_str(),
                    binding.credential,
                    status.revision,
                    CredentialPool {
                        members: aliases
                            .iter()
                            .map(|alias| CredentialRef::ApiKey {
                                alias: (*alias).into(),
                            })
                            .collect(),
                        fallback,
                        max_attempts: 1,
                    },
                )
                .unwrap();
            assert!(matches!(
                Bootstrap::from_directory(dir.path()),
                Err(StartupError::Credentials)
            ));
        }
    }
}
