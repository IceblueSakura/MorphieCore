//! Independent identity carriers, presence and loss evidence; no provider calls.
use morphiecore::{
    protocol::{
        GenerationRequestEnvelope as Request,
        anthropic::messages::{
            Dialect as MessagesDialect, Profile as MessagesProfile, TargetContract as MessagesCaps,
        },
        native::ReplayTarget,
        openai::{Dialect, GenerationApi, GenerationCodec, TargetContract},
    },
    topology::catalog::GOOGLE_BINDINGS,
};
use serde_json::{Value, json};

fn source(
    profile: GenerationApi,
    fields: Value,
) -> Result<Request, morphiecore::protocol::CodecError> {
    let mut wire = if profile == GenerationApi::Chat {
        json!({"model":"m","max_completion_tokens":4096,"messages":[{"role":"user","content":"go"}]})
    } else {
        json!({"model":"m","max_output_tokens":4096,"input":"go"})
    };
    wire.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    GenerationCodec::new(profile, Dialect::Standard, None)
        .decode_client_request(wire.to_string().as_bytes())
}
fn messages(
    request: &Request,
) -> morphiecore::protocol::conversion::generation::Conversion<serde_json::Value> {
    MessagesCaps::new(
        MessagesProfile::AdaptiveTextTools,
        MessagesDialect::Standard,
    )
    .encode_request(request, &ReplayTarget::new("synthetic", "m").unwrap())
    .unwrap()
}

#[test]
fn openai_preserves_independent_fields_and_checks_all_source_bounds() {
    for profile in [GenerationApi::Chat, GenerationApi::Responses] {
        let adapter = GenerationCodec::new(profile, Dialect::Standard, None);
        let fields = json!({"user":"legacy_a","safety_identifier":"usr_b","prompt_cache_key":"template_c",
            "metadata":{"user_id":"unrelated_metadata"}});
        let request = source(profile, fields.clone()).unwrap();
        let before = request.clone();
        let (wire, rules) = adapter
            .encode_request_with_rules(&request, "m", &TargetContract::full())
            .unwrap();
        for key in ["user", "safety_identifier", "prompt_cache_key", "metadata"] {
            assert_eq!(wire[key], fields[key]);
        }
        assert!(!rules.contains(&"generation.omit-identity-hints.v1"));
        assert!(!rules.contains(&"cache.omit-legacy-user-bucketing.v1"));
        assert_eq!(request, before);
        let other = if profile == GenerationApi::Chat {
            GenerationApi::Responses
        } else {
            GenerationApi::Chat
        };
        let (cross, rules) = GenerationCodec::new(other, Dialect::Standard, None)
            .encode_request_with_rules(&request, "m", &TargetContract::full())
            .unwrap();
        for key in ["user", "safety_identifier", "prompt_cache_key"] {
            assert_eq!(cross[key], fields[key]);
        }
        assert!(!rules.contains(&"generation.omit-identity-hints.v1"));
        assert!(!rules.contains(&"cache.omit-legacy-user-bucketing.v1"));
        for value in [Value::Null, json!("")] {
            let present = source(profile, json!({"user":value,"safety_identifier":value})).unwrap();
            let wire = adapter
                .encode_request(&present, "m", &TargetContract::full())
                .unwrap();
            assert_eq!(wire["user"], value);
            assert_eq!(wire["safety_identifier"], value);
        }
        let lone = source(profile, json!({"user":"legacy"})).unwrap();
        let wire = adapter
            .encode_request(&lone, "m", &TargetContract::full())
            .unwrap();
        assert_eq!(wire["user"], "legacy");
        assert!(wire.get("safety_identifier").is_none());
        assert!(wire.get("prompt_cache_key").is_none());
        for (field, maximum) in [("user", 256), ("safety_identifier", 128)] {
            let good = source(profile, json!({field:"用".repeat(maximum)})).unwrap();
            assert_eq!(
                adapter
                    .encode_request(&good, "m", &TargetContract::full())
                    .unwrap()[field],
                "用".repeat(maximum)
            );
            assert!(source(profile, json!({field:"用".repeat(maximum + 1)})).is_err());
            let mut invalid = good.clone();
            if field == "user" {
                invalid.context.identity.user = Some("x".repeat(maximum + 1));
            } else {
                invalid.context.identity.safety_identifier = Some("x".repeat(maximum + 1));
            }
            let mut without = TargetContract::full();
            without.dispositions.set_native(
                morphiecore::semantic::task::generation::Feature::Identity,
                false,
            );
            assert!(adapter.encode_request(&invalid, "m", &without).is_err());
        }
        // An invalid lower-priority native user must not hide behind a valid safety ID.
        let mut invalid = source(profile, json!({"safety_identifier":"valid"})).unwrap();
        invalid.context.identity.user = Some("x".repeat(257));
        assert!(
            MessagesCaps::new(
                MessagesProfile::AdaptiveTextTools,
                MessagesDialect::Standard
            )
            .encode_request(&invalid, &ReplayTarget::new("synthetic", "m").unwrap())
            .is_err()
        );
    }
    for bytes in [
        br#"{"model":"m","input":"go","user":"a","user":"b"}"#.as_slice(),
        br#"{"model":"m","input":"go","safety_identifier":null,"safety_identifier":"b"}"#
            .as_slice(),
    ] {
        assert!(
            GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None)
                .decode_client_request(bytes)
                .is_err()
        );
    }
    assert!(morphiecore::protocol::anthropic::messages::client::decode_client_request(
        br#"{"model":"m","max_tokens":32,"messages":[{"role":"user","content":"go"}],"metadata":{"user_id":"a","user_id":"b"}}"#
    ).is_err());
}

#[test]
fn single_carriers_preserve_priority_presence_and_separate_legacy_cache_loss() {
    for (fields, selected, lost_identity) in [
        (json!({}), None, false),
        (json!({"safety_identifier":null}), None, false),
        (
            json!({"safety_identifier":"usr_b"}),
            Some(json!("usr_b")),
            false,
        ),
        (json!({"user":"legacy_a"}), Some(json!("legacy_a")), false),
        (
            json!({"user":"legacy_a","safety_identifier":"usr_b"}),
            Some(json!("usr_b")),
            true,
        ),
        (
            json!({"user":"legacy_a","safety_identifier":null}),
            Some(json!("legacy_a")),
            false,
        ),
        (
            json!({"user":"legacy_a","safety_identifier":""}),
            Some(json!("")),
            true,
        ),
    ] {
        let request = source(GenerationApi::Responses, fields.clone()).unwrap();
        let before = request.clone();
        let projected = messages(&request);
        assert_eq!(
            projected
                .value
                .get("metadata")
                .and_then(|m| m.get("user_id")),
            selected.as_ref()
        );
        let rules: Vec<_> = projected.evidence.iter().map(|r| r.rule()).collect();
        assert_eq!(
            rules.contains(&"generation.omit-identity-hints.v1"),
            lost_identity
        );
        assert_eq!(
            rules.contains(&"cache.omit-legacy-user-bucketing.v1"),
            fields.get("user").is_some()
        );
        assert!(projected.value.get("prompt_cache_key").is_none());
        let (wire, rules) = GOOGLE_BINDINGS[0]
            .endpoint()
            .encode_request_with_rules(&request)
            .unwrap();
        let google_value = selected.as_ref().filter(|v| !v.is_null());
        assert_eq!(
            wire.get("labels").and_then(|m| m.get("safety_identifier")),
            google_value
        );
        assert_eq!(
            rules.contains(&"generation.omit-identity-hints.v1"),
            lost_identity || selected == Some(Value::Null)
        );
        assert_eq!(
            rules.contains(&"cache.omit-legacy-user-bucketing.v1"),
            fields.get("user").is_some()
        );
        assert_eq!(request, before);
    }
    let request = source(
        GenerationApi::Responses,
        json!({"user":"legacy_a","safety_identifier":"usr_b","prompt_cache_key":"template_c"}),
    )
    .unwrap();
    let projected = messages(&request);
    assert_eq!(projected.value["metadata"]["user_id"], "usr_b");
    let rules: Vec<_> = projected.evidence.iter().map(|r| r.rule()).collect();
    for rule in [
        "generation.omit-identity-hints.v1",
        "cache.omit-legacy-user-bucketing.v1",
        "generation.omit-cache-key.v1",
    ] {
        assert_eq!(rules.iter().filter(|r| **r == rule).count(), 1);
    }
    let evidence = format!("{:?}", projected.evidence);
    assert!(
        projected
            .evidence
            .iter()
            .any(|e| e.rule() == "cache.omit-legacy-user-bucketing.v1"
                && e.semantic_field()
                    == morphiecore::protocol::conversion::generation::Field::IdentityBucket
                && e.change() == morphiecore::protocol::conversion::generation::Change::Omit)
    );
    for private in ["usr_b", "legacy_a", "template_c"] {
        assert!(!evidence.contains(private));
    }
    // Interactions has no selected identity carrier; do not invent labels/metadata.
    let (wire, rules) = GOOGLE_BINDINGS[0]
        .interactions_endpoint()
        .encode_request_with_rules(&request)
        .unwrap();
    assert!(wire.get("labels").is_none());
    assert!(wire.get("metadata").is_none());
    assert!(rules.contains(&"generation.omit-identity-hints.v1"));
    assert!(rules.contains(&"cache.omit-legacy-user-bucketing.v1"));
    let native = morphiecore::protocol::anthropic::messages::client::decode_client_request(
        r#"{"model":"m","max_tokens":4096,"metadata":{"user_id":"native_用户"},"messages":[{"role":"user","content":"go"}]}"#.as_bytes()
    ).unwrap();
    let (wire, rules) = GOOGLE_BINDINGS[0]
        .endpoint()
        .encode_request_with_rules(&native)
        .unwrap();
    assert_eq!(wire["labels"], json!({"safety_identifier":"native_用户"}));
    assert!(!rules.contains(&"generation.omit-identity-hints.v1"));
    assert!(!rules.contains(&"cache.omit-legacy-user-bucketing.v1"));
}

#[test]
fn response_identity_bounds_are_consistent_in_json_and_sse_snapshots() {
    let adapter = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
    for length in [64, 65, 128, 129] {
        let mut wire = crate::wire::response(2);
        wire["safety_identifier"] = json!("用".repeat(length));
        let decoded = adapter.decode_response(wire.to_string().as_bytes());
        if length > 128 {
            assert!(decoded.is_err());
        } else {
            let decoded = decoded.unwrap();
            assert_eq!(
                adapter
                    .encode_response(&decoded, &TargetContract::full())
                    .unwrap()["safety_identifier"],
                wire["safety_identifier"]
            );
        }
        let mut decoder = adapter.event_decoder();
        let mut opening = wire.clone();
        opening["status"] = json!("in_progress");
        opening["output"] = json!([]);
        opening["usage"] = Value::Null;
        opening["completed_at"] = Value::Null;
        assert_eq!(
            decoder
                .push(&json!({"type":"response.created","response":opening}))
                .is_ok(),
            length <= 128
        );
    }
}
