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
fn native_identity_never_silently_becomes_openai_legacy_user() {
    use morphiecore::protocol::{
        anthropic::client::decode_client_request,
        openai::{Adapter, Dialect, Profile, TargetCaps},
    };
    use serde_json::{Value, json};
    for value in [
        Value::Null,
        json!(""),
        json!("x".repeat(256)),
        json!("x".repeat(257)),
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
            let (wire, rules) = adapter
                .encode_request_with_rules(&source, "m", &TargetCaps::full())
                .unwrap();
            assert!(wire.get("user").is_none());
            assert!(wire.get("safety_identifier").is_none());
            assert_eq!(
                rules
                    .iter()
                    .filter(|r| **r == "generation.omit-identity-hints.v1")
                    .count(),
                1
            );
        }
        assert_eq!(source, before);
    }
}
