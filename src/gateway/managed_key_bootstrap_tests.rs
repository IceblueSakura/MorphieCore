//! File activation and replay epoch rebinding, without inference traffic.
use super::*;
use crate::credential::{CredentialPool, CredentialRef};
fn setup() -> (tempfile::TempDir, CredentialManager) {
    let dir = crate::credential::test_support::private_directory();
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
fn modelbest_pool_binds_distinct_chat_targets_and_vision_is_not_text_admission() {
    let dir = crate::credential::test_support::private_directory();
    let manager = CredentialManager::new(dir.path(), vec![]).unwrap();
    manager.write_gateway_config_for_test(&serde_json::json!({
        "client_key":"synthetic-gateway-key-at-least-32-bytes",
        "models":["minicpm5-1b","minicpm5-2b","minicpm-v-4.6"]
    }));
    manager
        .add_api_key(
            "modelbest",
            "one",
            Secret::new("synthetic-modelbest-key".into()).unwrap(),
        )
        .unwrap();
    manager
        .set_pool(
            "modelbest",
            "modelbest-api-key",
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
    let boot = Bootstrap::from_directory(dir.path()).unwrap();
    assert_eq!(boot.gateway.state.entries.len(), 3);
    for (public, upstream, endpoint) in [
        ("minicpm5-1b", "MiniCPM5-1B", "modelbest-minicpm5-1b-chat"),
        ("minicpm5-2b", "MiniCPM5-2B", "modelbest-minicpm5-2b-chat"),
        (
            "minicpm-v-4.6",
            "MiniCPM-V-4.6",
            "modelbest-minicpm-v-4.6-chat",
        ),
    ] {
        let entry =
            &boot.gateway.state.entries[&(super::super::family(Profile::Chat), public.into())];
        assert_eq!(entry.public.canonical_model.as_str(), public);
        assert_eq!(
            entry.public.route.as_str(),
            format!("modelbest-{public}-generation")
        );
        let candidate = &entry.candidates[0];
        assert_eq!(candidate.endpoint.id.as_str(), endpoint);
        assert_eq!(candidate.endpoint.credential.as_str(), "modelbest-api-key");
        assert!(candidate.provider.responses.is_none());
        let body = serde_json::json!({"model":public,"messages":[{"role":"user","content":"hello"}],"max_completion_tokens":37});
        let (_, request) = super::super::admission::prepare(
            &boot.gateway.state,
            Profile::Chat,
            &serde_json::to_vec(&body).unwrap(),
        )
        .unwrap();
        let prepared = crate::execution::prepare(
            &candidate.endpoint,
            &candidate.provider,
            &candidate.secret.resolve().unwrap().0,
            &request,
        )
        .unwrap();
        assert_eq!(prepared.origin, "https://api.modelbest.cn");
        assert_eq!(prepared.path, "/v1/chat/completions");
        assert_eq!(
            prepared.auth_header,
            (
                "authorization".into(),
                "Bearer synthetic-modelbest-key".into()
            )
        );
        let wire: serde_json::Value = serde_json::from_slice(&prepared.body).unwrap();
        assert_eq!(wire["model"], upstream);
        assert_eq!(wire["max_tokens"], 37);
        assert!(wire.get("max_completion_tokens").is_none());
        let response_request = serde_json::json!({"model":public,"input":"hello"});
        assert_eq!(
            super::super::admission::prepare(
                &boot.gateway.state,
                Profile::Responses,
                &serde_json::to_vec(&response_request).unwrap()
            )
            .err()
            .unwrap()
            .status,
            404
        );
    }
    for (model, accepts) in [
        ("minicpm5-1b", false),
        ("minicpm5-2b", false),
        ("minicpm-v-4.6", true),
    ] {
        let body = serde_json::json!({"model":model,"messages":[{"role":"user","content":[{"type":"text","text":"describe"},{"type":"image_url","image_url":{"url":"data:image/png;base64,AQID"}}]}]});
        let decoded = super::super::admission::prepare(
            &boot.gateway.state,
            Profile::Chat,
            &serde_json::to_vec(&body).unwrap(),
        );
        assert_eq!(decoded.is_ok(), accepts);
        if let Ok((_, request)) = decoded {
            let entry =
                &boot.gateway.state.entries[&(super::super::family(Profile::Chat), model.into())];
            let candidate = &entry.candidates[0];
            let prepared = crate::execution::prepare(
                &candidate.endpoint,
                &candidate.provider,
                &candidate.secret.resolve().unwrap().0,
                &request,
            )
            .unwrap();
            let wire: serde_json::Value = serde_json::from_slice(&prepared.body).unwrap();
            assert_eq!(
                wire["messages"][0]["content"],
                body["messages"][0]["content"]
            );
        }
    }
}

#[test]
fn bootstrap_loads_selected_key_and_requires_reactivation_after_rotation() {
    let (dir, manager) = setup();
    let first = Bootstrap::from_directory(dir.path()).unwrap();
    let entry = &first.gateway.state.entries[&(
        super::super::family(Profile::Responses),
        "deepseek-flash".into(),
    )];
    let candidate = &entry.candidates[0];
    let (key, grant) = candidate.secret.resolve().unwrap();
    assert!(grant.is_none());
    let (_, request) = super::super::admission::prepare(
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
        super::super::family(Profile::Responses),
        "deepseek-flash".into(),
    )];
    assert_ne!(
        entry.downstream.replay_origin,
        next.downstream.replay_origin
    );
}
#[test]
fn file_pools_reject_unknown_bindings_repeated_members_and_invalid_attempt_limits() {
    for (path, value) in [
        (
            "/pools/deepseek-api-key/config/max_attempts",
            serde_json::json!(0),
        ),
        (
            "/pools/deepseek-api-key/config/members",
            serde_json::json!([{"kind":"api_key","alias":"one"},{"kind":"api_key","alias":"one"}]),
        ),
        ("/provider", serde_json::json!("another-provider")),
    ] {
        let (dir, _) = setup();
        let file = dir.path().join("deepseek.json");
        let mut value_file: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        *value_file.pointer_mut(path).unwrap() = value;
        std::fs::write(file, serde_json::to_vec(&value_file).unwrap()).unwrap();
        assert!(Bootstrap::from_directory(dir.path()).is_err());
    }
    let (dir, manager) = setup();
    manager
        .set_pool(
            "deepseek",
            "unknown",
            0,
            CredentialPool {
                members: vec![CredentialRef::ApiKey {
                    alias: "one".into(),
                }],
                fallback: true,
                max_attempts: 2,
            },
        )
        .unwrap();
    assert!(Bootstrap::from_directory(dir.path()).is_err());
}
#[test]
fn pool_candidates_have_distinct_ordinals_and_reject_opaque_replay_without_affinity() {
    let (dir, manager) = setup();
    manager
        .add_api_key(
            "deepseek",
            "two",
            Secret::new("synthetic-second".into()).unwrap(),
        )
        .unwrap();
    manager
        .set_pool(
            "deepseek",
            "deepseek-api-key",
            1,
            CredentialPool {
                members: vec![
                    CredentialRef::ApiKey {
                        alias: "two".into(),
                    },
                    CredentialRef::ApiKey {
                        alias: "one".into(),
                    },
                ],
                fallback: true,
                max_attempts: 2,
            },
        )
        .unwrap();
    let boot = Bootstrap::from_directory(dir.path()).unwrap();
    let entry = &boot.gateway.state.entries[&(
        super::super::family(Profile::Responses),
        "deepseek-flash".into(),
    )];
    let request = entry
        .client
        .decode_request(br#"{"model":"deepseek-flash","input":"test"}"#)
        .unwrap();
    let candidates = entry.eligible(&request).unwrap();
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].endpoint.id, candidates[1].endpoint.id);
    assert_eq!(
        candidates[0].secret.resolve().unwrap().0.expose(),
        "synthetic-second"
    );
    assert_eq!(
        candidates[1].secret.resolve().unwrap().0.expose(),
        "synthetic-managed-key"
    );
    assert!(entry.client.adaptation.scope.is_none());
    let request = entry.client.decode_request(br#"{"model":"deepseek-flash","input":"test","include":["reasoning.encrypted_content"]}"#).unwrap();
    assert!(entry.eligible(&request).is_err());
    manager.write_gateway_config_for_test(&serde_json::json!({"client_key":"synthetic-gateway-key-at-least-32-bytes","max_attempts":1}));
    let capped = Bootstrap::from_directory(dir.path()).unwrap();
    assert_eq!(
        capped.gateway.state.entries[&(
            super::super::family(Profile::Responses),
            "deepseek-flash".into()
        )]
            .policy
            .max_attempts,
        1
    );
    assert_eq!(
        capped.gateway.state.entries[&(
            super::super::family(Profile::Responses),
            "deepseek-flash".into()
        )]
            .candidates
            .len(),
        1
    );
}
