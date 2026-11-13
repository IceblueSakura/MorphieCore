//! File activation and replay epoch rebinding, without inference traffic.
use super::{
    test_support::{selected_models, write_accounts, write_configuration},
    *,
};
use crate::protocol::ClientProtocol;
fn setup() -> (tempfile::TempDir, CredentialManager) {
    let dir = crate::test_files::private_directory();
    let manager = CredentialManager::new(dir.path().join("credentials"), vec![]).unwrap();
    write_configuration(
        dir.path(),
        serde_json::json!({"auth_file":"auth.json"}),
        selected_models(&["deepseek-v4.1-flash-1017"], &["deepseek"]),
    );
    manager
        .add_api_key(
            "deepseek",
            "one",
            Secret::new("synthetic-managed-key".into()).unwrap(),
        )
        .unwrap();
    write_accounts(
        dir.path(),
        &serde_json::json!({"version":1,"providers":{"deepseek":{"accounts":[{
            "name":"primary","enabled":true,"credential":{"alias":"one"}
        }]}}}),
    );
    (dir, manager)
}

#[test]
fn bootstrap_loads_selected_key_and_requires_reactivation_after_rotation() {
    let (dir, manager) = setup();
    let first = Bootstrap::from_directory(dir.path()).unwrap();
    let entry = &first.gateway.state.entries[&(
        crate::protocol::ClientProtocol::Responses,
        "deepseek-v4.1-flash-1017".into(),
    )];
    let candidate = &entry.candidates[0];
    let (key, grant) = candidate.secret.resolve().unwrap();
    assert!(grant.is_none());
    let (_, request) = crate::gateway::ingress::admission::prepare(
        &first.gateway.state,
        ClientProtocol::Responses,
        br#"{"model":"deepseek-v4.1-flash-1017","input":"synthetic request"}"#,
        &crate::execution::report::Report::default(),
    )
    .unwrap();
    let prepared =
        crate::execution::prepare(&candidate.endpoint, &candidate.provider, &key, &request)
            .unwrap();
    assert_eq!(prepared.auth_header.1, "Bearer synthetic-managed-key");
    let sealed = crate::protocol::replay_envelope::seal(
        &candidate.endpoint.replay_origin(),
        crate::semantic::task::generation::ReplayFormat::ResponsesCompaction,
        "synthetic-old-key-compaction",
    )
    .unwrap();
    let mut replay=entry.client.as_openai().unwrap().0.decode_request(
        &serde_json::to_vec(&serde_json::json!({
            "model":"deepseek-v4.1-flash-1017","input":[{"type":"compaction","id":"cmp","encrypted_content":sealed.as_str()}]
        })).unwrap()
    ).unwrap();
    crate::execution::replay::open_from_client(&mut replay).unwrap();
    // Enable only the synthetic carrier, independently of catalog admission.
    let mut replay_endpoint = candidate.endpoint.clone();
    replay_endpoint
        .representation
        .openai_mut()
        .unwrap()
        .dispositions
        .set_native(crate::semantic::task::generation::Feature::Compaction, true);
    assert_eq!(
        replay_endpoint.encode_request(&replay).unwrap()["input"][0]["encrypted_content"],
        "synthetic-old-key-compaction"
    );
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
        crate::protocol::ClientProtocol::Responses,
        "deepseek-v4.1-flash-1017".into(),
    )];
    assert_eq!(
        entry.client.as_openai().unwrap().1.replay_origin,
        next.client.as_openai().unwrap().1.replay_origin
    );
    assert_ne!(candidate.cache_scope(), next.candidates[0].cache_scope());
    let mut replacement = next.candidates[0].endpoint.clone();
    replacement
        .representation
        .openai_mut()
        .unwrap()
        .dispositions
        .set_native(crate::semantic::task::generation::Feature::Compaction, true);
    assert!(
        replacement.encode_request(&replay).is_err(),
        "a fresh key binding cannot forward the former key's opaque state"
    );
    assert_ne!(
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
