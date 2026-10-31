//! Cache affinity is Provider functionality, not gateway sessions or route selection.
use morphiecore::protocol::openai::{
    Dialect, GenerationApi, GenerationCodec, TargetContract as Contract,
};
use serde_json::{Value, json};

#[test]
fn retention_omission_is_target_local_and_preserves_explicit_storage_guarantees() {
    use morphiecore::semantic::context::{CacheMode, CacheOptions, CacheRetention};
    use morphiecore::semantic::value::Presence;
    for profile in [GenerationApi::Chat, GenerationApi::Responses] {
        let mut body = request(profile);
        body["prompt_cache_retention"] = json!("24h");
        let source = client(profile)
            .decode_request(body.to_string().as_bytes())
            .unwrap();
        let before = source.clone();
        let target = GenerationCodec::new(profile, Dialect::OpenRouter, None);
        let (wire, rules) = target
            .encode_request_with_rules(&source, "target", &Contract::full())
            .unwrap();
        assert!(wire.get("prompt_cache_retention").is_none());
        assert_eq!(
            rules
                .iter()
                .filter(|r| **r == "generation.omit-cache-retention.v1")
                .count(),
            1
        );
        let (wire, rules) = client(profile)
            .encode_request_with_rules(&source, "target", &Contract::full())
            .unwrap();
        assert_eq!(wire["prompt_cache_retention"], "24h");
        assert!(!rules.contains(&"generation.omit-cache-retention.v1"));
        assert_eq!(source, before);
        let mut protected = source.clone();
        protected.context.cache.prompt_cache_retention = Some(CacheRetention::InMemory);
        let before = protected.clone();
        assert!(
            target
                .encode_request(&protected, "target", &Contract::full())
                .is_err()
        );
        assert_eq!(protected, before);
        let mut explicit = source;
        explicit.context.cache.prompt_cache_options = Presence::Value(CacheOptions {
            mode: Some(CacheMode::Explicit),
            ..Default::default()
        });
        let before = explicit.clone();
        assert!(
            target
                .encode_request(&explicit, "target", &Contract::full())
                .is_err()
        );
        assert_eq!(explicit, before);
    }
}
fn client(p: GenerationApi) -> GenerationCodec {
    GenerationCodec::new(p, Dialect::MorphieCore, None)
}
fn request(p: GenerationApi) -> Value {
    match p {
        GenerationApi::Responses => {
            json!({"model":"public","input":[{"role":"user","content":[{"type":"input_text","text":"stable prefix"},{"type":"input_image","image_url":"https://example.invalid/image","detail":"auto"}]}]})
        }
        GenerationApi::Chat => {
            json!({"model":"public","messages":[{"role":"user","content":[{"type":"text","text":"stable prefix"},{"type":"image_url","image_url":{"url":"https://example.invalid/image","detail":"auto"}}]}]})
        }
    }
}

#[test]
fn scoped_session_and_standard_cache_key_remain_independent_on_both_wires() {
    let mut empty = request(GenerationApi::Chat);
    empty["prompt_cache_options"] = json!({});
    let decoded = client(GenerationApi::Chat)
        .decode_request(empty.to_string().as_bytes())
        .unwrap();
    let saved = decoded.context.clone();
    let (without_options, rules) =
        GenerationCodec::new(GenerationApi::Chat, Dialect::DeepSeek, None)
            .encode_request_with_rules(&decoded, "upstream", &Contract::full())
            .unwrap();
    assert!(without_options.get("prompt_cache_options").is_none());
    assert!(rules.is_empty());
    assert_eq!(decoded.context, saved);
    empty["prompt_cache_options"] = Value::Null;
    assert!(
        client(GenerationApi::Chat)
            .decode_client_request(empty.to_string().as_bytes())
            .is_err()
    );
    let mut invalid = decoded.clone();
    invalid.context.cache.prompt_cache_options = morphiecore::semantic::value::Presence::Null;
    assert!(
        GenerationCodec::new(GenerationApi::Chat, Dialect::DeepSeek, None)
            .encode_request(&invalid, "upstream", &Contract::full())
            .is_err()
    );
    let mut advisory = decoded.clone();
    advisory.context.cache.prompt_cache_key = Some("synthetic".into());
    let (wire, rules) = GenerationCodec::new(GenerationApi::Chat, Dialect::DeepSeek, None)
        .encode_request_with_rules(&advisory, "upstream", &Contract::full())
        .unwrap();
    assert!(wire.get("prompt_cache_key").is_none());
    assert!(wire.get("prompt_cache_retention").is_none());
    assert!(rules.contains(&"generation.omit-cache-key.v1"));
    advisory.context.cache.prompt_cache_retention =
        Some(morphiecore::semantic::context::CacheRetention::InMemory);
    let saved = advisory.clone();
    assert!(
        GenerationCodec::new(GenerationApi::Chat, Dialect::DeepSeek, None)
            .encode_request(&advisory, "upstream", &Contract::full())
            .is_err()
    );
    let (wire, rules) = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None)
        .encode_request_with_rules(&advisory, "upstream", &Contract::full())
        .unwrap();
    assert_eq!(wire["prompt_cache_retention"], "in_memory");
    assert!(rules.is_empty());
    assert_eq!(advisory, saved);
    // Identity hints only attribute abuse upstream: a target without their carrier
    // omits them under a named rule, and a target with it keeps them.
    let mut identified = decoded.clone();
    identified.context.identity.safety_identifier = Some("synthetic-user".into());
    let mut without = Contract::full();
    without.dispositions.set_native(
        morphiecore::semantic::task::generation::Feature::Identity,
        false,
    );
    let (wire, rules) = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None)
        .encode_request_with_rules(&identified, "upstream", &without)
        .unwrap();
    assert!(wire.get("safety_identifier").is_none());
    assert!(rules.contains(&"generation.omit-identity-hints.v1"));
    let (wire, rules) = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None)
        .encode_request_with_rules(&identified, "upstream", &Contract::full())
        .unwrap();
    assert_eq!(wire["safety_identifier"], "synthetic-user");
    assert!(!rules.contains(&"generation.omit-identity-hints.v1"));
    use morphiecore::protocol::anthropic::messages::{
        Dialect as MessagesDialect, Profile as MessagesProfile, TargetContract,
    };
    let text_only = client(GenerationApi::Chat)
        .decode_request(
            json!({"model":"public","max_completion_tokens":4096,"messages":[{"role":"user","content":"hello"}],
                    "safety_identifier":"synthetic-user"})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
    let native = TargetContract::new(
        MessagesProfile::AdaptiveTextTools,
        MessagesDialect::Standard,
    )
    .encode_request(
        &text_only,
        &morphiecore::protocol::native::ReplayTarget::new("synthetic", "model").unwrap(),
    )
    .unwrap();
    assert_eq!(native.value["metadata"]["user_id"], "synthetic-user");
    assert!(
        !native
            .evidence
            .contains_rule("generation.omit-identity-hints.v1")
    );
    empty["prompt_cache_options"] = json!({"mode":"explicit"});
    let active = client(GenerationApi::Chat)
        .decode_request(empty.to_string().as_bytes())
        .unwrap();
    assert!(
        GenerationCodec::new(GenerationApi::Chat, Dialect::DeepSeek, None)
            .encode_request(&active, "upstream", &Contract::full())
            .is_err()
    );
    empty["prompt_cache_options"] = json!({});
    let original = client(GenerationApi::Chat)
        .decode_client_request(empty.to_string().as_bytes())
        .unwrap();
    empty["prompt_cache_options"] = json!({"prewarm":true,"comparison_response_id":"ignored"});
    let decoded = client(GenerationApi::Chat)
        .decode_client_request(empty.to_string().as_bytes())
        .unwrap();
    assert_eq!(decoded.task, original.task);
    assert_eq!(decoded.context, original.context);
    let out = client(GenerationApi::Chat)
        .encode_request(&decoded, "upstream", &Contract::full())
        .unwrap();
    assert_eq!(out["prompt_cache_options"], json!({}));
    for p in [GenerationApi::Chat, GenerationApi::Responses] {
        let mut wire = request(p);
        wire["prompt_cache_key"] = json!("key-a");
        wire["session_id"] = json!("session-b");
        let req = client(p)
            .decode_request(&serde_json::to_vec(&wire).unwrap())
            .unwrap();
        let target = GenerationCodec::new(p, Dialect::OpenRouter, None);
        let out = target
            .encode_request(&req, "upstream", &Contract::full())
            .unwrap();
        assert_eq!(out["session_id"], "session-b");
        assert_eq!(out["prompt_cache_key"], "key-a");
        let mut edited = req.clone();
        edited.cache_session = None;
        edited.context.cache.prompt_cache_key = None;
        let erased = target
            .encode_request(&edited, "upstream", &Contract::full())
            .unwrap();
        assert!(erased.get("session_id").is_none());
        assert!(erased.get("prompt_cache_key").is_none());
        assert!(
            GenerationCodec::new(p, Dialect::Standard, None)
                .decode_request(&serde_json::to_vec(&wire).unwrap())
                .is_err()
        );
        assert!(
            GenerationCodec::new(p, Dialect::DeepSeek, None)
                .encode_request(&req, "upstream", &Contract::full())
                .is_err()
        );
        for invalid in [
            Value::Null,
            json!(""),
            json!("x".repeat(257)),
            json!({"token":"synthetic"}),
        ] {
            wire["session_id"] = invalid;
            assert!(
                client(p)
                    .decode_request(&serde_json::to_vec(&wire).unwrap())
                    .is_err()
            );
        }
    }
}
