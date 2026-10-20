//! Native Messages wire shapes and semantic mapping.
#[path = "anthropic/anthropic_media.rs"]
mod anthropic_media;
#[path = "anthropic/anthropic_output.rs"]
mod anthropic_output;
#[path = "anthropic/anthropic_profile.rs"]
mod anthropic_profile;
#[path = "anthropic/anthropic_semantics.rs"]
mod anthropic_semantics;
#[path = "anthropic/client_fidelity.rs"]
mod client_fidelity;
#[path = "anthropic/native_controls.rs"]
mod native_controls;
#[test]
fn native_identity_projects_to_openai_safety_not_legacy_cache_bucketing() {
    use morphiecore::protocol::{
        anthropic::messages::client::decode_client_request,
        openai::{Adapter, Dialect, Profile, TargetCaps},
    };
    use serde_json::{Value, json};
    for value in [
        Value::Null,
        json!(""),
        json!("x".repeat(128)),
        json!("x".repeat(129)),
        json!("x".repeat(512)),
    ] {
        let source = decode_client_request(
            json!({"model":"m","max_tokens":4096,"metadata":{"user_id":value},
            "messages":[{"role":"user","content":"go"}]})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
        let before = source.clone();
        for profile in [Profile::Chat, Profile::Responses] {
            let adapter = Adapter::new(profile, Dialect::Standard, None);
            let encoded = adapter.encode_request_with_rules(&source, "m", &TargetCaps::full());
            if value
                .as_str()
                .is_some_and(|value| value.chars().count() > 128)
            {
                assert!(encoded.is_err());
                continue;
            }
            let (wire, rules) = encoded.unwrap();
            assert!(wire.get("user").is_none());
            assert_eq!(wire["safety_identifier"], value);
            assert!(wire.get("prompt_cache_key").is_none());
            assert!(!rules.contains(&"generation.omit-identity-hints.v1"));
            assert!(!rules.contains(&"cache.omit-legacy-user-bucketing.v1"));
        }
        assert_eq!(source, before);
    }
}
