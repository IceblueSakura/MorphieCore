use crate::events_support::text;
use morphiecore::{
    adapter::{AdapterError, Request},
    protocol::{
        CodecError, DecodedRequest,
        fidelity::FidelityRecords,
        openai::{Adapter, Dialect, Profile, TargetCaps},
    },
    semantic::{task::generation::*, value::Presence},
    topology::{ProtocolProfile, catalog::API_KEY_BINDINGS},
};
use serde_json::{Value, json};
fn client(profile: Profile) -> Adapter {
    Adapter::new(profile, Dialect::MorphieCore, None)
}
fn native(profile: Profile) -> Adapter {
    Adapter::new(profile, Dialect::Bailian, None)
}
fn chat() -> Value {
    json!({"model":"qwen3.8-flash","messages":[{"role":"user","content":"go"}]})
}
fn endpoint(prefix: &str, profile: ProtocolProfile) -> morphiecore::topology::Endpoint {
    API_KEY_BINDINGS
        .iter()
        .find(|b| b.endpoint_prefix == prefix)
        .unwrap()
        .endpoint(profile)
        .unwrap()
}
fn request(settings: GenerationSettings) -> Request {
    let semantic = GenerationRequest::from_settings(
        vec![(
            ItemId::new(1),
            Item::Message(Message {
                participant: None,
                role: MessageRole::User,
                status: ItemLifecycle::Completed,
                phase: None,
                parts: vec![Part::new(
                    PartId::new(1),
                    ContentPart::Text(text("go").into()),
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
        "public-model",
    )
    .unwrap()
}
#[test]
fn native_chat_controls_have_independent_input_and_output_oracles_without_defaults() {
    let wire = json!({"model":"qwen3.8-flash","messages":[{"role":"user","content":"go"}],
            "max_completion_tokens":64,"temperature":0.6,"top_k":40,
            "enable_thinking":true,"thinking_budget":32,"preserve_thinking":false});
    let mut controls = GenerationControls::default();
    controls.max_output_tokens = Some(64);
    controls.top_k = Some(40);
    controls = controls.with_temperature(0.6).unwrap();
    let mut reasoning = ReasoningRequest::present(None);
    reasoning.activation = Presence::Value(ReasoningActivation::Enabled);
    reasoning.mode = Presence::Value(ReasoningMode::Budgeted);
    reasoning.budget_tokens = Some(32);
    reasoning.read_history = Presence::Value(false);
    let expected = request(GenerationSettings {
        controls,
        reasoning,
        ..Default::default()
    });
    for adapter in [native(Profile::Chat), client(Profile::Chat)] {
        let decoded = adapter
            .decode_client_request(wire.to_string().as_bytes())
            .unwrap();
        assert_eq!(decoded.task.semantic, expected.task.semantic);
    }
    let before = expected.clone();
    assert_eq!(
        native(Profile::Chat)
            .encode_request(&expected, "qwen3.8-flash", &TargetCaps::full())
            .unwrap(),
        wire
    );
    assert_eq!(expected, before);
    // top_k's codec carrier does not widen the catalog's public admission.
    let mut settings = expected.task.semantic.settings().clone();
    settings.controls.top_k = None;
    let input = request(settings);
    for prefix in [
        "dashscope-flash",
        "aliyun-dashscope-cn",
        "tokenplan-max",
        "aliyun-tokenplan-cn",
    ] {
        let target = endpoint(prefix, ProtocolProfile::OpenAiChat);
        let (actual, rules) = target.encode_request_with_rules(&input).unwrap();
        let mut expected = wire.clone();
        expected["model"] = json!(target.upstream_model);
        expected.as_object_mut().unwrap().remove("top_k");
        assert_eq!(actual, expected);
        assert!(rules.is_empty());
    }
    let empty = client(Profile::Chat)
        .decode_client_request(chat().to_string().as_bytes())
        .unwrap();
    let (out, rules) = endpoint("dashscope-flash", ProtocolProfile::OpenAiChat)
        .encode_request_with_rules(&empty)
        .unwrap();
    for field in [
        "enable_thinking",
        "thinking_budget",
        "preserve_thinking",
        "reasoning_effort",
        "top_k",
    ] {
        assert!(out.get(field).is_none(), "{field}");
    }
    assert!(rules.is_empty());
}
#[test]
fn native_control_types_conflicts_and_unowned_fields_never_silently_disappear() {
    for (field, value) in [
        ("enable_thinking", Value::Null),
        ("enable_thinking", json!("true")),
        ("thinking_budget", json!(0)),
        ("thinking_budget", json!(1.5)),
        ("top_k", Value::Null),
        ("top_k", json!(0)),
        ("top_k", json!(101)),
        ("enable_search", json!(true)),
        ("extra_body", json!({"enable_thinking":true})),
        ("parameters", json!({"max_tokens":64})),
        ("enable_code_interpreter", json!(true)),
        ("repetition_penalty", json!(1.0)),
        ("tool_stream", json!(true)),
    ] {
        let mut wire = chat();
        wire[field] = value;
        assert!(
            client(Profile::Chat)
                .decode_client_request(wire.to_string().as_bytes())
                .is_err(),
            "{field}"
        );
    }
    for controls in [
        json!({"enable_thinking":true,"reasoning_effort":"none"}),
        json!({"enable_thinking":false,"reasoning_effort":"low"}),
        json!({"thinking_budget":32,"reasoning_effort":"low"}),
        json!({"thinking_budget":65,"max_completion_tokens":64}),
    ] {
        let mut wire = chat();
        wire.as_object_mut()
            .unwrap()
            .extend(controls.as_object().unwrap().clone());
        assert!(
            client(Profile::Chat)
                .decode_client_request(wire.to_string().as_bytes())
                .is_err()
        );
    }
    let mut wire = chat();
    wire["future_extension"] = json!({"ignored":true});
    assert!(
        client(Profile::Chat)
            .decode_client_request(wire.to_string().as_bytes())
            .is_ok()
    );
    wire = chat();
    wire["messages"][0]["content"] =
        json!([{"type":"text","text":"go","cache_control":{"type":"ephemeral"}}]);
    assert!(
        client(Profile::Chat)
            .decode_client_request(wire.to_string().as_bytes())
            .is_err()
    );
    wire = chat();
    wire["messages"] = json!([{"role":"assistant","content":"prefix","partial":true}]);
    assert!(
        client(Profile::Chat)
            .decode_client_request(wire.to_string().as_bytes())
            .is_err()
    );
}
#[test]
fn output_caps_sampling_and_model_specific_ignored_controls_fail_closed() {
    let target = endpoint("dashscope-flash", ProtocolProfile::OpenAiChat);
    for (field, value) in [
        ("temperature", json!(2)),
        ("temperature", json!(0.5)),
        ("top_p", json!(0)),
        ("seed", json!(-1)),
        ("seed", json!(2147483648u64)),
        ("frequency_penalty", json!(0.5)),
    ] {
        let mut wire = chat();
        wire[field] = value;
        let input = client(Profile::Chat)
            .decode_client_request(wire.to_string().as_bytes())
            .unwrap();
        assert!(target.encode_request_with_rules(&input).is_err(), "{field}");
    }
    let mut off = chat();
    off["enable_thinking"] = json!(false);
    off["temperature"] = json!(0.0);
    let input = client(Profile::Chat)
        .decode_client_request(off.to_string().as_bytes())
        .unwrap();
    let (wire, rules) = target.encode_request_with_rules(&input).unwrap();
    assert_eq!(wire["enable_thinking"], false);
    assert_eq!(wire["temperature"], 0.0);
    assert!(rules.is_empty());
    for prefix in [
        "dashscope-glm",
        "dashscope-glm-prime",
        "tokenplan-glm",
        "dashscope-kimi",
    ] {
        let target = endpoint(prefix, ProtocolProfile::OpenAiChat);
        let mut budget = chat();
        budget["thinking_budget"] = json!(32);
        let input = client(Profile::Chat)
            .decode_client_request(budget.to_string().as_bytes())
            .unwrap();
        assert!(
            matches!(target.encode_request_with_rules(&input),Err(AdapterError::Codec(CodecError::Unsupported(field))) if field=="Bailian thinking_budget carrier")
        );
        let mut cap = chat();
        cap["max_completion_tokens"] = json!(64);
        let input = client(Profile::Chat)
            .decode_client_request(cap.to_string().as_bytes())
            .unwrap();
        let (wire, _) = target.encode_request_with_rules(&input).unwrap();
        assert_eq!(wire["max_completion_tokens"], 64);
        assert!(wire.get("max_tokens").is_none());
    }
    let mut legacy = chat();
    legacy["max_tokens"] = json!(64);
    assert!(
        native(Profile::Chat)
            .decode_request(legacy.to_string().as_bytes())
            .is_err()
    );
    legacy["enable_thinking"] = json!(false);
    assert!(
        native(Profile::Chat)
            .decode_request(legacy.to_string().as_bytes())
            .is_ok()
    );
    legacy = chat();
    legacy["model"] = json!("glm-5.3");
    legacy["max_tokens"] = json!(64);
    assert_eq!(
        native(Profile::Chat)
            .decode_request(legacy.to_string().as_bytes())
            .unwrap()
            .task
            .semantic
            .controls()
            .max_output_tokens,
        Some(64)
    );
}
#[test]
fn qualitative_aliases_are_model_bound_and_never_budget_presets() {
    for (input, output) in [
        ("minimal", "low"),
        ("low", "low"),
        ("medium", "medium"),
        ("high", "xhigh"),
        ("xhigh", "xhigh"),
        ("max", "xhigh"),
    ] {
        for prefix in ["dashscope-flash", "aliyun-tokenplan-cn"] {
            let mut wire = chat();
            wire["reasoning_effort"] = json!(input);
            let request = client(Profile::Chat)
                .decode_client_request(wire.to_string().as_bytes())
                .unwrap();
            let (wire, rules) = endpoint(prefix, ProtocolProfile::OpenAiChat)
                .encode_request_with_rules(&request)
                .unwrap();
            assert_eq!(wire["reasoning_effort"], output);
            assert!(wire.get("thinking_budget").is_none());
            assert!(wire.get("enable_thinking").is_none());
            assert_eq!(rules.is_empty(), input == output);
        }
    }
    let mut wire = chat();
    wire["reasoning_effort"] = json!("high");
    let native = native(Profile::Chat)
        .decode_request(wire.to_string().as_bytes())
        .unwrap();
    assert_eq!(
        native
            .task
            .semantic
            .settings()
            .effort
            .value()
            .unwrap()
            .level,
        EffortLevel::XHigh
    );
    for value in ["none", "medium", "minimal"] {
        wire["reasoning_effort"] = json!(value);
        let input = client(Profile::Chat)
            .decode_client_request(wire.to_string().as_bytes())
            .unwrap();
        assert!(
            endpoint("dashscope-glm", ProtocolProfile::OpenAiChat)
                .encode_request_with_rules(&input)
                .is_err()
        );
    }
}
#[test]
fn model_binding_and_local_delivery_prefs_cannot_be_bypassed_by_direct_native_adapters() {
    let mut wire = chat();
    wire["reasoning_effort"] = json!("medium");
    let input = client(Profile::Chat)
        .decode_client_request(wire.to_string().as_bytes())
        .unwrap();
    assert!(
        native(Profile::Chat)
            .encode_request(&input, "glm-5.3", &TargetCaps::full())
            .is_err()
    );
    let mut caps = TargetCaps::full();
    caps.adaptation = native(Profile::Chat).adaptation;
    let lowered = morphiecore::protocol::openai::lowering::lower_request(
        &input.task.semantic,
        &input.task.fidelity,
        Profile::Chat,
        caps,
    )
    .unwrap();
    let context = morphiecore::protocol::openai::chat::envelope::RequestContext {
        model: "glm-5.3".into(),
        ..Default::default()
    };
    assert!(
        matches!(morphiecore::protocol::openai::chat::envelope::encode_request(&lowered,&context),
            Err(CodecError::Unsupported(field)) if field=="Bailian native effort level")
    );
    wire["reasoning_effort"] = json!("high");
    let input = client(Profile::Chat)
        .decode_client_request(wire.to_string().as_bytes())
        .unwrap();
    let (out, rules) = native(Profile::Chat)
        .encode_request_with_rules(&input, "qwen3.8-flash", &TargetCaps::full())
        .unwrap();
    assert_eq!(out["reasoning_effort"], "xhigh");
    assert_eq!(rules, ["bailian.qwen3.8-flash.effort.v1"]);
    wire = chat();
    wire["model"] = json!("kimi-k3");
    wire["reasoning_effort"] = json!("none");
    for result in [
        native(Profile::Chat).decode_request(wire.to_string().as_bytes()),
        native(Profile::Chat).decode_client_request(wire.to_string().as_bytes()),
    ] {
        assert!(result.is_err());
    }
    let input = client(Profile::Chat)
        .decode_client_request(wire.to_string().as_bytes())
        .unwrap();
    let (out, _) = endpoint("dashscope-kimi", ProtocolProfile::OpenAiChat)
        .encode_request_with_rules(&input)
        .unwrap();
    assert_eq!(out["enable_thinking"], false);
    assert!(out.get("reasoning_effort").is_none());
    wire = chat();
    wire["stream"] = json!(true);
    wire["stream_options"] = json!({"include_usage":true,"include_obfuscation":false});
    let input = client(Profile::Chat)
        .decode_client_request(wire.to_string().as_bytes())
        .unwrap();
    let before = input.clone();
    let (out, _) = endpoint("dashscope-flash", ProtocolProfile::OpenAiChat)
        .encode_request_with_rules(&input)
        .unwrap();
    assert_eq!(out["stream_options"], json!({"include_usage":true}));
    assert_eq!(input, before);
    assert!(
        native(Profile::Chat)
            .decode_client_request(wire.to_string().as_bytes())
            .is_err()
    );
    let wire = json!({"model":"qwen3.8-flash","input":[
            {"type":"configuration_update","reasoning":{"effort":"low"}},{"role":"user","content":"go"}]});
    let input = client(Profile::Responses)
        .decode_client_request(wire.to_string().as_bytes())
        .unwrap();
    assert!(
        matches!(endpoint("dashscope-flash",ProtocolProfile::OpenAiResponses).encode_request_with_rules(&input),
            Err(AdapterError::Codec(CodecError::Unsupported(field))) if field=="Bailian history item")
    );
    assert!(
        native(Profile::Responses)
            .decode_client_request(wire.to_string().as_bytes())
            .is_err()
    );
    let wire = json!({"model":"glm-5.3","input":"go","reasoning":{"effort":"medium"}});
    let native = native(Profile::Responses)
        .decode_request(wire.to_string().as_bytes())
        .unwrap();
    assert_eq!(
        native
            .task
            .semantic
            .settings()
            .effort
            .value()
            .unwrap()
            .level,
        EffortLevel::High
    );
    let input = client(Profile::Responses)
        .decode_client_request(wire.to_string().as_bytes())
        .unwrap();
    let (wire, rules) = endpoint("tokenplan-glm", ProtocolProfile::OpenAiResponses)
        .encode_request_with_rules(&input)
        .unwrap();
    assert_eq!(wire["reasoning"]["effort"], "high");
    assert_eq!(rules, ["bailian.glm-5.3.responses.effort.v1"]);
}
fn response_request() -> Value {
    json!({"model":"qwen3.8-flash","input":"go","max_output_tokens":64,
            "tools":[{"type":"function","name":"lookup","description":"lookup value","parameters":{"type":"object"}}]})
}
#[test]
fn responses_non_strict_defaults_and_single_function_selection_have_native_oracles() {
    let native = native(Profile::Responses)
        .decode_request(response_request().to_string().as_bytes())
        .unwrap();
    assert!(
        matches!(&native.task.semantic.tools()[0],ToolDefinition::Function(t)
            if t.strict==FunctionStrictness::Omitted(StrictDefault::NonStrict))
    );
    let mut controls = GenerationControls::default();
    controls.max_output_tokens = Some(64);
    let settings = GenerationSettings {
        controls,
        tools: Some(vec![ToolDefinition::Function(FunctionTool {
            name: text("lookup"),
            description: Some("lookup value".into()),
            input_examples: None,
            parameters: Some(json!({"type":"object"}).into()),
            strict: FunctionStrictness::Explicit(false),
            output_schema: None,
            dispatch: Default::default(),
            cache: None,
            eager_input: None,
        })]),
        tool_choice: Some(ToolChoice::Specific(text("lookup"))),
        ..Default::default()
    };
    let input = request(settings);
    let target = endpoint("dashscope-flash", ProtocolProfile::OpenAiResponses);
    let (wire, rules) = target.encode_request_with_rules(&input).unwrap();
    let mut expected = response_request();
    expected["store"] = json!(false);
    expected["input"] =
        json!([{"type":"message","role":"user","content":[{"type":"input_text","text":"go"}]}]);
    expected["tool_choice"] = json!({"type":"allowed_tools","mode":"required","tools":[{"type":"function","name":"lookup"}]});
    assert_eq!(wire, expected);
    assert!(rules.is_empty());
    let mut source = response_request();
    source["tools"][0]["strict"] = json!(false);
    source["enable_thinking"] = json!(true);
    let input = client(Profile::Responses)
        .decode_client_request(source.to_string().as_bytes())
        .unwrap();
    let (wire, _) = target.encode_request_with_rules(&input).unwrap();
    assert_eq!(wire["enable_thinking"], true);
    assert!(wire.get("reasoning").is_none());
    assert!(wire["tools"][0].get("strict").is_none());
}
#[test]
fn responses_unlisted_guarantees_and_non_adjacent_results_reject_without_reordering() {
    let target = endpoint("dashscope-flash", ProtocolProfile::OpenAiResponses);
    for patch in [
        json!({"text":{"format":{"type":"json_object"}}}),
        json!({"reasoning":{"summary":"concise"}}),
        json!({"parallel_tool_calls":false}),
        json!({"truncation":"auto"}),
        json!({"include":["reasoning.encrypted_content"]}),
        json!({"max_output_tokens":15}),
    ] {
        let mut wire = response_request();
        wire["tools"][0]["strict"] = json!(false);
        wire.as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        let input = client(Profile::Responses)
            .decode_client_request(wire.to_string().as_bytes())
            .unwrap();
        assert!(target.encode_request_with_rules(&input).is_err(), "{patch}");
    }
    let strict = client(Profile::Responses)
        .decode_client_request(response_request().to_string().as_bytes())
        .unwrap();
    assert!(target.encode_request_with_rules(&strict).is_err());
    let make_call =
        |id: &str| json!({"type":"function_call","name":"lookup","call_id":id,"arguments":"{}"});
    let make_result =
        |id: &str| json!({"type":"function_call_output","call_id":id,"output":"done"});
    let mut wire = json!({"model":"qwen3.8-flash","input":[make_call("a"),make_call("b"),make_result("a"),make_result("b")]});
    let input = client(Profile::Responses)
        .decode_client_request(wire.to_string().as_bytes())
        .unwrap();
    let before = input.clone();
    assert!(
        matches!(target.encode_request_with_rules(&input),Err(AdapterError::Codec(CodecError::Unsupported(field))) if field=="Bailian function output adjacency")
    );
    assert_eq!(input, before);
    wire["input"] = json!([
        make_call("a"),
        make_result("a"),
        make_call("b"),
        make_result("b")
    ]);
    let input = client(Profile::Responses)
        .decode_client_request(wire.to_string().as_bytes())
        .unwrap();
    assert_eq!(
        target.encode_request_with_rules(&input).unwrap().0["input"],
        wire["input"]
    );
}
