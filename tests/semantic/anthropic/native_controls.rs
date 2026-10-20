use morphiecore::{
    adapter::Request,
    protocol::{
        CodecError, DecodedRequest,
        anthropic::{self, Dialect, Profile, SamplingPolicy, TargetCaps},
        fidelity::FidelityRecords,
        native::ReplayTarget,
    },
    semantic::{
        task::generation::*,
        value::{Presence, Text},
    },
};
use serde_json::{Value, json};
fn target() -> ReplayTarget {
    ReplayTarget::new("synthetic-native", "m").unwrap()
}
fn caps() -> TargetCaps {
    let mut caps = TargetCaps::new(Profile::AdaptiveTextTools, Dialect::Standard);
    caps.semantics = Profile::AdaptiveTextTools.representation_contract();
    caps
}
fn typed(budget: Option<u64>) -> Request {
    let mut settings = GenerationSettings::default();
    settings.controls.max_output_tokens = Some(4096);
    if let Some(budget) = budget {
        settings.reasoning = ReasoningRequest::present(None);
        settings.reasoning.activation = Presence::Value(ReasoningActivation::Enabled);
        settings.reasoning.mode = Presence::Value(ReasoningMode::Budgeted);
        settings.reasoning.budget_tokens = Some(budget);
    }
    let value = Text::new("go", "test", 8).unwrap();
    let semantic = GenerationRequest::from_settings(
        vec![(
            ItemId::new(1),
            Item::Message(Message {
                role: MessageRole::User,
                status: ItemLifecycle::Completed,
                phase: None,
                parts: vec![Part::new(
                    PartId::new(1),
                    ContentPart::Text(TextContent::new(value, vec![], Presence::Absent).unwrap()),
                )],
            }),
        )],
        settings,
    )
    .unwrap();
    Request::from_generation(
        DecodedRequest {
            semantic,
            fidelity: FidelityRecords::default(),
        },
        "m",
    )
    .unwrap()
}
#[test]
fn model_maximum_checks_inserted_default_without_replacing_explicit_caps() {
    use morphiecore::adapter::AdapterError;
    let mut request = typed(None);
    let mut settings = request.task.semantic.settings().clone();
    settings.controls.max_output_tokens = None;
    request.task.semantic = request.task.semantic.with_settings(settings).unwrap();
    let before = request.clone();
    let mut limited = caps();
    limited.model_limits.maximum_output_tokens = Some(31_999);
    assert!(matches!(limited.encode_request(&request, &target()),
        Err(AdapterError::Codec(CodecError::Unsupported(value)))
            if value == "target maximum output tokens"));
    limited.model_limits.maximum_output_tokens = Some(32_000);
    let projected = limited.encode_request(&request, &target()).unwrap();
    assert_eq!(projected.value["max_tokens"], 32_000);
    assert_eq!(
        projected
            .rules
            .iter()
            .filter(|rule| **rule == anthropic::RequestRule::DefaultMaxTokens)
            .count(),
        1
    );
    assert_eq!(request, before);
    limited.model_limits.maximum_output_tokens = Some(4096);
    let explicit = typed(None);
    assert_eq!(
        limited.encode_request(&explicit, &target()).unwrap().value["max_tokens"],
        4096
    );
    limited.model_limits.maximum_output_tokens = Some(4095);
    assert!(matches!(limited.encode_request(&explicit, &target()),
        Err(AdapterError::Codec(CodecError::Unsupported(value)))
            if value == "target maximum output tokens"));
}
#[test]
fn ordinary_budgets_preserve_numeric_intent_and_native_strict_relation() {
    let wire = json!({"model":"m","max_tokens":4096,"thinking":{"type":"enabled","budget_tokens":1024},
            "messages":[{"role":"user","content":"go"}]});
    let decoded = anthropic::client::decode_client_request(wire.to_string().as_bytes()).unwrap();
    assert_eq!(decoded.task.semantic.reasoning().budget_tokens, Some(1024));
    assert_eq!(
        decoded.task.semantic.reasoning().mode,
        Presence::Value(ReasoningMode::Budgeted)
    );
    let request = typed(Some(1024));
    let before = request.clone();
    let native = caps().encode_request(&request, &target()).unwrap();
    assert_eq!(
        native.value["thinking"],
        json!({"type":"enabled","budget_tokens":1024})
    );
    assert_eq!(native.value["max_tokens"], 4096);
    assert!(
        native
            .rules
            .iter()
            .all(|r| !matches!(r, anthropic::RequestRule::ReasoningPolicy(_)))
    );
    assert_eq!(request, before);
    for (budget, cap) in [
        (1023, 4096),
        (1024, 1024),
        (2048, 1024),
        (u64::MAX, u64::MAX),
    ] {
        let mut bad = wire.clone();
        bad["thinking"]["budget_tokens"] = json!(budget);
        bad["max_tokens"] = json!(cap);
        assert!(matches!(
            anthropic::client::decode_client_request(bad.to_string().as_bytes()),
            Err(CodecError::Invalid("thinking budget"))
        ));
        assert!(
            Profile::AdaptiveTextTools
                .decode_request(bad.to_string().as_bytes())
                .is_err()
        );
    }
    for budget in [1023, 4096] {
        assert!(
            caps()
                .encode_request(&typed(Some(budget)), &target())
                .is_err()
        );
    }
    let mut adaptive_only = caps();
    adaptive_only.manual_thinking = false;
    assert!(adaptive_only.encode_request(&request, &target()).is_err());
}
#[test]
fn sampling_is_native_unless_the_trusted_target_selects_fixed_policy() {
    let wire = json!({"model":"m","max_tokens":4096,"temperature":0.25,"top_p":0.75,"top_k":7,
            "messages":[{"role":"user","content":"go"}]});
    let decoded = anthropic::client::decode_client_request(wire.to_string().as_bytes()).unwrap();
    assert_eq!(decoded.task.semantic.controls().temperature(), Some(0.25));
    assert_eq!(decoded.task.semantic.controls().top_p(), Some(0.75));
    assert_eq!(decoded.task.semantic.controls().top_k, Some(7));
    let mut request = typed(None);
    let mut settings = request.task.semantic.settings().clone();
    settings.controls = settings
        .controls
        .with_temperature(0.25)
        .unwrap()
        .with_top_p(0.75)
        .unwrap();
    settings.controls.top_k = Some(7);
    request.task.semantic = request.task.semantic.with_settings(settings).unwrap();
    let source = request.clone();
    let native = caps().encode_request(&request, &target()).unwrap();
    assert_eq!(native.value["temperature"], 0.25);
    assert_eq!(native.value["top_p"], 0.75);
    assert_eq!(native.value["top_k"], 7);
    assert!(!native.rules.contains(&anthropic::RequestRule::OmitSampling));
    let mut fixed = caps();
    fixed.sampling = SamplingPolicy::Fixed;
    assert!(fixed.encode_request(&request, &target()).is_err());
    let mut settings = request.task.semantic.settings().clone();
    settings.controls.top_k = None;
    request.task.semantic = request.task.semantic.with_settings(settings).unwrap();
    let projected = fixed.encode_request(&request, &target()).unwrap();
    assert!(projected.value.get("temperature").is_none());
    assert!(projected.value.get("top_p").is_none());
    assert!(
        projected
            .rules
            .contains(&anthropic::RequestRule::OmitSampling)
    );
    assert_eq!(request.task.semantic.controls().temperature(), Some(0.25));
    assert_eq!(source.task.semantic.controls().top_k, Some(7));
    let endpoint = morphiecore::topology::catalog::MESSAGES_BINDINGS[0].endpoint();
    assert!(endpoint.encode_request_with_rules(&decoded).is_err());
}
#[test]
fn native_identity_keeps_null_empty_unicode_and_target_bounds_without_cross_projection() {
    for value in [Value::Null, json!(""), json!("用".repeat(512))] {
        let wire = json!({"model":"m","max_tokens":4096,"metadata":{"user_id":value},
                "messages":[{"role":"user","content":"go"}]});
        let decoded =
            anthropic::client::decode_client_request(wire.to_string().as_bytes()).unwrap();
        let expected = if value.is_null() {
            Presence::Null
        } else {
            Presence::Value(value.as_str().unwrap().into())
        };
        assert_eq!(decoded.context.identity.user, expected);
        let mut request = typed(None);
        request.context.identity.user = expected;
        let before = request.clone();
        let native = caps().encode_request(&request, &target()).unwrap();
        assert_eq!(native.value["metadata"], json!({"user_id":value}));
        assert!(
            !native
                .rules
                .iter()
                .any(|r| r.name() == "generation.omit-identity-hints.v1")
        );
        assert_eq!(request, before);
    }
    for metadata in [
        Value::Null,
        json!({"user_id":7}),
        json!({"user_id":"x".repeat(513)}),
    ] {
        let wire = json!({"model":"m","max_tokens":4096,"metadata":metadata,
                "messages":[{"role":"user","content":"go"}]});
        assert!(anthropic::client::decode_client_request(wire.to_string().as_bytes()).is_err());
    }
    let plain = caps().encode_request(&typed(None), &target()).unwrap();
    assert!(plain.value.get("metadata").is_none());
    let openai = morphiecore::protocol::openai::Adapter::new(
        morphiecore::protocol::openai::Profile::Chat,
        morphiecore::protocol::openai::Dialect::Standard,
        None,
    );
    let foreign = openai
        .decode_client_request(
            br#"{"model":"m","messages":[{"role":"user","content":"go"}],"user":"legacy"}"#,
        )
        .unwrap();
    let encoded = caps().encode_request(&foreign, &target()).unwrap();
    assert!(encoded.value.get("metadata").is_none());
    assert!(
        encoded
            .rules
            .iter()
            .any(|r| r.name() == "generation.omit-identity-hints.v1")
    );
}
