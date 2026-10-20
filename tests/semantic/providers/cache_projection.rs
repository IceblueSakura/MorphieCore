//! Cache affinity is Provider functionality, not gateway sessions or route selection.
use morphiecore::protocol::openai::{Adapter, Dialect, Profile, TargetCaps as Contract};
use serde_json::{Value, json};

#[test]
fn extended_retention_omission_is_explicit_value_specific_and_target_local() {
    use morphiecore::{
        protocol::cache::CacheProjection,
        semantic::{context::CacheRetention, value::Presence},
    };
    assert!(!CacheProjection::all().omit_day_retention);
    let mut approved = Contract::full();
    approved.cache.omit_day_retention = true;
    for profile in [Profile::Chat, Profile::Responses] {
        let body = match profile {
            Profile::Chat => {
                json!({"model":"fixture","messages":[{"role":"user","content":"go"}],"prompt_cache_retention":"24h"})
            }
            Profile::Responses => {
                json!({"model":"fixture","input":"go","prompt_cache_retention":"24h"})
            }
        };
        let source = client(profile)
            .decode_request(body.to_string().as_bytes())
            .unwrap();
        assert_eq!(
            source.context.cache.prompt_cache_retention,
            Presence::Value(CacheRetention::Day)
        );
        let before = source.clone();
        let target = Adapter::new(profile, Dialect::OpenRouter, None);
        assert!(
            target
                .encode_request(&source, "target", &Contract::full())
                .is_err()
        );
        let (wire, rules) = target
            .encode_request_with_rules(&source, "target", &approved)
            .unwrap();
        assert!(wire.get("prompt_cache_retention").is_none());
        assert_eq!(
            rules
                .iter()
                .filter(|r| **r == "cache.omit-day-retention.v1")
                .count(),
            1
        );
        let (wire, rules) = client(profile)
            .encode_request_with_rules(&source, "target", &approved)
            .unwrap();
        assert_eq!(wire["prompt_cache_retention"], "24h");
        assert!(!rules.contains(&"cache.omit-day-retention.v1"));
        assert_eq!(source, before);
        for protected in [Presence::Value(CacheRetention::InMemory), Presence::Null] {
            let mut source = source.clone();
            source.context.cache.prompt_cache_retention = protected;
            assert!(target.encode_request(&source, "target", &approved).is_err());
        }
    }
    let mut policy = CacheProjection::all();
    let selected = CacheProjection {
        omit_day_retention: true,
        ..Default::default()
    };
    policy.intersect(selected);
    policy.intersect(CacheProjection::default());
    assert!(policy.omit_day_retention);
    assert!(!policy.retention);
    let mut reverse = selected;
    reverse.intersect(CacheProjection::all());
    assert_eq!(policy, reverse);
    // A failed projection does not erase a control before rejecting another one.
    let mut context = morphiecore::semantic::context::ExecutionHints::default();
    context.cache.prompt_cache_retention = Presence::Value(CacheRetention::Day);
    context.cache.prompt_cache_options =
        Presence::Value(morphiecore::semantic::context::CacheOptions {
            mode: Presence::Value(morphiecore::semantic::context::CacheMode::Explicit),
            ..Default::default()
        });
    let before = context.clone();
    assert!(selected.project(&mut context).is_err());
    assert_eq!(context, before);
}
fn client(p: Profile) -> Adapter {
    Adapter::new(p, Dialect::MorphieCore, None)
}
fn request(p: Profile) -> Value {
    match p {
        Profile::Responses => {
            json!({"model":"public","input":[{"role":"user","content":[{"type":"input_text","text":"stable prefix"},{"type":"input_image","image_url":"https://example.invalid/image","detail":"auto"}]}]})
        }
        Profile::Chat => {
            json!({"model":"public","messages":[{"role":"user","content":[{"type":"text","text":"stable prefix"},{"type":"image_url","image_url":{"url":"https://example.invalid/image","detail":"auto"}}]}]})
        }
    }
}

#[test]
fn scoped_session_and_standard_cache_key_remain_independent_on_both_wires() {
    let mut empty = request(Profile::Chat);
    empty["prompt_cache_options"] = json!({});
    let decoded = client(Profile::Chat)
        .decode_request(empty.to_string().as_bytes())
        .unwrap();
    let saved = decoded.context.clone();
    let (without_options, rules) = Adapter::new(Profile::Chat, Dialect::DeepSeek, None)
        .encode_request_with_rules(&decoded, "upstream", &Contract::full())
        .unwrap();
    assert!(without_options.get("prompt_cache_options").is_none());
    assert!(rules.contains(&"cache.omit-inactive-options.v1"));
    assert_eq!(decoded.context, saved);
    empty["prompt_cache_options"] = Value::Null;
    assert!(
        client(Profile::Chat)
            .decode_client_request(empty.to_string().as_bytes())
            .is_err()
    );
    let mut invalid = decoded.clone();
    invalid.context.cache.prompt_cache_options = morphiecore::semantic::value::Presence::Null;
    assert!(
        Adapter::new(Profile::Chat, Dialect::DeepSeek, None)
            .encode_request(&invalid, "upstream", &Contract::full())
            .is_err()
    );
    let mut advisory = decoded.clone();
    advisory.context.cache.prompt_cache_key =
        morphiecore::semantic::value::Presence::Value("synthetic".into());
    let (wire, rules) = Adapter::new(Profile::Chat, Dialect::DeepSeek, None)
        .encode_request_with_rules(&advisory, "upstream", &Contract::full())
        .unwrap();
    assert!(wire.get("prompt_cache_key").is_none());
    assert!(wire.get("prompt_cache_retention").is_none());
    assert!(rules.contains(&"cache.omit-key.v1"));
    advisory.context.cache.prompt_cache_retention = morphiecore::semantic::value::Presence::Value(
        morphiecore::semantic::context::CacheRetention::InMemory,
    );
    let saved = advisory.clone();
    assert!(
        Adapter::new(Profile::Chat, Dialect::DeepSeek, None)
            .encode_request(&advisory, "upstream", &Contract::full())
            .is_err()
    );
    let (wire, rules) = Adapter::new(Profile::Chat, Dialect::Standard, None)
        .encode_request_with_rules(&advisory, "upstream", &Contract::full())
        .unwrap();
    assert_eq!(wire["prompt_cache_retention"], "in_memory");
    assert!(rules.is_empty());
    assert_eq!(advisory, saved);
    // Identity hints only attribute abuse upstream: a target without their carrier
    // omits them under a named rule, and a target with it keeps them.
    let mut identified = decoded.clone();
    identified.context.identity.safety_identifier =
        morphiecore::semantic::value::Presence::Value("synthetic-user".into());
    let mut without = Contract::full();
    without.identity_hints = false;
    let (wire, rules) = Adapter::new(Profile::Chat, Dialect::Standard, None)
        .encode_request_with_rules(&identified, "upstream", &without)
        .unwrap();
    assert!(wire.get("safety_identifier").is_none());
    assert!(rules.contains(&"generation.omit-identity-hints.v1"));
    let (wire, rules) = Adapter::new(Profile::Chat, Dialect::Standard, None)
        .encode_request_with_rules(&identified, "upstream", &Contract::full())
        .unwrap();
    assert_eq!(wire["safety_identifier"], "synthetic-user");
    assert!(!rules.contains(&"generation.omit-identity-hints.v1"));
    use morphiecore::protocol::anthropic::messages::{
        Dialect as MessagesDialect, Profile as MessagesProfile, RequestRule, TargetCaps,
    };
    let text_only = client(Profile::Chat)
        .decode_request(
            json!({"model":"public","max_completion_tokens":4096,"messages":[{"role":"user","content":"hello"}],
                    "safety_identifier":"synthetic-user"})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
    let native = TargetCaps::new(
        MessagesProfile::AdaptiveTextTools,
        MessagesDialect::Standard,
    )
    .encode_request(
        &text_only,
        &morphiecore::protocol::native::ReplayTarget::new("synthetic", "model").unwrap(),
    )
    .unwrap();
    assert_eq!(native.value["metadata"]["user_id"], "synthetic-user");
    assert!(!native.rules.contains(&RequestRule::Hint(
        morphiecore::protocol::cache::HintRule::OmitIdentityHints
    )));
    empty["prompt_cache_options"] = json!({"mode":"explicit"});
    let active = client(Profile::Chat)
        .decode_request(empty.to_string().as_bytes())
        .unwrap();
    assert!(
        Adapter::new(Profile::Chat, Dialect::DeepSeek, None)
            .encode_request(&active, "upstream", &Contract::full())
            .is_err()
    );
    empty["prompt_cache_options"] = json!({"prewarm":true});
    assert!(
        client(Profile::Chat)
            .decode_client_request(empty.to_string().as_bytes())
            .is_err()
    );
    for p in [Profile::Chat, Profile::Responses] {
        let mut wire = request(p);
        wire["prompt_cache_key"] = json!("key-a");
        wire["session_id"] = json!("session-b");
        let req = client(p)
            .decode_request(&serde_json::to_vec(&wire).unwrap())
            .unwrap();
        let target = Adapter::new(p, Dialect::OpenRouter, None);
        let out = target
            .encode_request(&req, "upstream", &Contract::full())
            .unwrap();
        assert_eq!(out["session_id"], "session-b");
        assert_eq!(out["prompt_cache_key"], "key-a");
        let mut edited = req.clone();
        edited.cache_session = None;
        edited.context.cache.prompt_cache_key = morphiecore::semantic::value::Presence::Absent;
        let erased = target
            .encode_request(&edited, "upstream", &Contract::full())
            .unwrap();
        assert!(erased.get("session_id").is_none());
        assert!(erased.get("prompt_cache_key").is_none());
        assert!(
            Adapter::new(p, Dialect::Standard, None)
                .decode_request(&serde_json::to_vec(&wire).unwrap())
                .is_err()
        );
        assert!(
            Adapter::new(p, Dialect::DeepSeek, None)
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
