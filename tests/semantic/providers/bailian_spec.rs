use crate::events_support::text;
use morphiecore::{
    protocol::{
        CodecError, ConversionFailure, DecodedRequest, GenerationRequestEnvelope as Request,
        fidelity::FidelityRecords,
        openai::{Dialect, GenerationApi, GenerationCodec, TargetContract},
    },
    semantic::{task::generation::*, value::Presence},
    topology::{GenerationProtocol, catalog::API_KEY_BINDINGS},
};
use serde_json::{Value, json};
fn client(profile: GenerationApi) -> GenerationCodec {
    GenerationCodec::new(profile, Dialect::MorphieCore, None)
}
fn native(profile: GenerationApi) -> GenerationCodec {
    GenerationCodec::new(profile, Dialect::Bailian, None)
}
fn chat() -> Value {
    json!({"model":"qwen3.8-flash","messages":[{"role":"user","content":"go"}]})
}
fn endpoint(prefix: &str, profile: GenerationProtocol) -> morphiecore::topology::Endpoint {
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
            evidence: Default::default(),
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
    reasoning.activation = Some(ReasoningActivation::Enabled);
    reasoning.mode = Presence::Value(ReasoningMode::Budgeted);
    reasoning.budget_tokens = Some(32);
    reasoning.read_history = Some(false);
    let expected = request(GenerationSettings {
        controls,
        reasoning,
        ..Default::default()
    });
    for adapter in [native(GenerationApi::Chat), client(GenerationApi::Chat)] {
        let decoded = adapter
            .decode_client_request(wire.to_string().as_bytes())
            .unwrap();
        assert_eq!(decoded.task.semantic, expected.task.semantic);
    }
    let before = expected.clone();
    assert_eq!(
        native(GenerationApi::Chat)
            .encode_request(&expected, "qwen3.8-flash", &TargetContract::full())
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
        let target = endpoint(prefix, GenerationProtocol::OpenAiChat);
        let (actual, rules) = target.encode_request_with_rules(&input).unwrap();
        let mut expected = wire.clone();
        expected["model"] = json!(target.upstream_model);
        expected.as_object_mut().unwrap().remove("top_k");
        assert_eq!(actual, expected);
        assert!(rules.is_empty());
    }
    let empty = client(GenerationApi::Chat)
        .decode_client_request(chat().to_string().as_bytes())
        .unwrap();
    let (out, rules) = endpoint("dashscope-flash", GenerationProtocol::OpenAiChat)
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
            client(GenerationApi::Chat)
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
            client(GenerationApi::Chat)
                .decode_client_request(wire.to_string().as_bytes())
                .is_err()
        );
    }
    let mut wire = chat();
    wire["future_extension"] = json!({"ignored":true});
    assert!(
        client(GenerationApi::Chat)
            .decode_client_request(wire.to_string().as_bytes())
            .is_ok()
    );
    wire = chat();
    wire["messages"][0]["content"] =
        json!([{"type":"text","text":"go","cache_control":{"type":"ephemeral"}}]);
    assert!(
        client(GenerationApi::Chat)
            .decode_client_request(wire.to_string().as_bytes())
            .is_err()
    );
    wire = chat();
    wire["messages"] = json!([{"role":"assistant","content":"prefix","partial":true}]);
    assert!(
        client(GenerationApi::Chat)
            .decode_client_request(wire.to_string().as_bytes())
            .is_err()
    );
}
#[test]
fn unsupported_advisory_controls_omit_without_weakening_native_constraints() {
    let target = endpoint("dashscope-flash", GenerationProtocol::OpenAiChat);
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
        let input = client(GenerationApi::Chat)
            .decode_client_request(wire.to_string().as_bytes())
            .unwrap();
        let before = input.clone();
        let (wire, rules) = target.encode_request_with_rules(&input).unwrap();
        assert!(wire.get(field).is_none(), "{field}");
        assert!(!rules.is_empty(), "{field}");
        assert_eq!(input, before);
    }
    let mut off = chat();
    off["enable_thinking"] = json!(false);
    off["temperature"] = json!(0.0);
    let input = client(GenerationApi::Chat)
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
        let target = endpoint(prefix, GenerationProtocol::OpenAiChat);
        let mut budget = chat();
        budget["thinking_budget"] = json!(32);
        let input = client(GenerationApi::Chat)
            .decode_client_request(budget.to_string().as_bytes())
            .unwrap();
        let (wire, rules) = target.encode_request_with_rules(&input).unwrap();
        assert!(wire.get("thinking_budget").is_none());
        assert_eq!(wire["reasoning_effort"], "low");
        assert!(rules.contains(&"generation.budget-to-effort.v1"));
        let mut cap = chat();
        cap["max_completion_tokens"] = json!(64);
        let input = client(GenerationApi::Chat)
            .decode_client_request(cap.to_string().as_bytes())
            .unwrap();
        let (wire, _) = target.encode_request_with_rules(&input).unwrap();
        assert_eq!(wire["max_completion_tokens"], 64);
        assert!(wire.get("max_tokens").is_none());
    }
    let mut legacy = chat();
    legacy["max_tokens"] = json!(64);
    assert!(
        native(GenerationApi::Chat)
            .decode_request(legacy.to_string().as_bytes())
            .is_err()
    );
    legacy["enable_thinking"] = json!(false);
    assert!(
        native(GenerationApi::Chat)
            .decode_request(legacy.to_string().as_bytes())
            .is_ok()
    );
    legacy = chat();
    legacy["model"] = json!("glm-5.3");
    legacy["max_tokens"] = json!(64);
    assert_eq!(
        native(GenerationApi::Chat)
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
            let request = client(GenerationApi::Chat)
                .decode_client_request(wire.to_string().as_bytes())
                .unwrap();
            let (wire, rules) = endpoint(prefix, GenerationProtocol::OpenAiChat)
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
    let native = native(GenerationApi::Chat)
        .decode_request(wire.to_string().as_bytes())
        .unwrap();
    assert_eq!(
        native
            .task
            .semantic
            .settings()
            .effort
            .as_ref()
            .unwrap()
            .level,
        EffortLevel::XHigh
    );
    for value in ["none", "medium", "minimal"] {
        wire["reasoning_effort"] = json!(value);
        let input = client(GenerationApi::Chat)
            .decode_client_request(wire.to_string().as_bytes())
            .unwrap();
        let output = endpoint("dashscope-glm", GenerationProtocol::OpenAiChat)
            .encode_request_with_rules(&input);
        if value == "none" {
            assert!(output.is_err());
        } else {
            let (wire, rules) = output.unwrap();
            assert_eq!(
                wire["reasoning_effort"],
                if value == "minimal" { "low" } else { "high" }
            );
            assert!(wire.get("thinking_budget").is_none());
            assert!(!rules.is_empty());
        }
    }
}
#[test]
fn model_binding_and_local_delivery_prefs_cannot_be_bypassed_by_direct_native_adapters() {
    let mut wire = chat();
    wire["reasoning_effort"] = json!("medium");
    let input = client(GenerationApi::Chat)
        .decode_client_request(wire.to_string().as_bytes())
        .unwrap();
    let output = native(GenerationApi::Chat)
        .encode_request(&input, "glm-5.3", &TargetContract::full())
        .unwrap();
    assert_eq!(output["reasoning_effort"], "high");
    let mut caps = TargetContract::full();
    caps.adaptation = native(GenerationApi::Chat).adaptation;
    let lowered = morphiecore::protocol::openai::lowering::lower_request(
        &input.task.semantic,
        &input.task.fidelity,
        GenerationApi::Chat,
        caps,
    )
    .unwrap();
    let context = morphiecore::protocol::openai::chat::envelope::RequestContext {
        model: "glm-5.3".into(),
        ..Default::default()
    };
    assert!(
        matches!(morphiecore::protocol::openai::chat::envelope::encode_request(&lowered,&context),
            Err(CodecError::Unsupported(field)) if field == morphiecore::protocol::failure::CarrierReason::Unavailable)
    );
    wire["reasoning_effort"] = json!("high");
    let input = client(GenerationApi::Chat)
        .decode_client_request(wire.to_string().as_bytes())
        .unwrap();
    let (out, rules) = native(GenerationApi::Chat)
        .encode_request_with_rules(&input, "qwen3.8-flash", &TargetContract::full())
        .unwrap();
    assert_eq!(out["reasoning_effort"], "xhigh");
    assert_eq!(rules, ["bailian.qwen3.8-flash.effort.v1"]);
    wire = chat();
    wire["model"] = json!("kimi-k3.1-pro");
    wire["reasoning_effort"] = json!("none");
    for result in [
        native(GenerationApi::Chat).decode_request(wire.to_string().as_bytes()),
        native(GenerationApi::Chat).decode_client_request(wire.to_string().as_bytes()),
    ] {
        result.expect("Kimi K3.1 supports explicit thinking off");
    }
    let input = client(GenerationApi::Chat)
        .decode_client_request(wire.to_string().as_bytes())
        .unwrap();
    let (out, _) = endpoint("dashscope-kimi", GenerationProtocol::OpenAiChat)
        .encode_request_with_rules(&input)
        .unwrap();
    assert_eq!(out["enable_thinking"], false);
    assert!(out.get("reasoning_effort").is_none());
    wire = chat();
    wire["stream"] = json!(true);
    wire["stream_options"] = json!({"include_usage":true,"include_obfuscation":false});
    let input = client(GenerationApi::Chat)
        .decode_client_request(wire.to_string().as_bytes())
        .unwrap();
    let before = input.clone();
    let (out, _) = endpoint("dashscope-flash", GenerationProtocol::OpenAiChat)
        .encode_request_with_rules(&input)
        .unwrap();
    assert_eq!(out["stream_options"], json!({"include_usage":true}));
    assert_eq!(input, before);
    assert!(
        native(GenerationApi::Chat)
            .decode_client_request(wire.to_string().as_bytes())
            .is_err()
    );
    let wire = json!({"model":"qwen3.8-flash","input":[
            {"type":"configuration_update","reasoning":{"effort":"low"}},{"role":"user","content":"go"}]});
    let input = client(GenerationApi::Responses)
        .decode_client_request(wire.to_string().as_bytes())
        .unwrap();
    let before = input.clone();
    let (projected, rules) = endpoint("dashscope-flash", GenerationProtocol::OpenAiResponses)
        .encode_request_with_rules(&input)
        .unwrap();
    assert_eq!(projected["reasoning"]["effort"], "low");
    assert!(
        projected["input"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["type"] != "configuration_update")
    );
    assert!(rules.contains(&"generation.collapse-configuration.v1"));
    assert_eq!(input, before);
    assert!(
        native(GenerationApi::Responses)
            .decode_client_request(wire.to_string().as_bytes())
            .is_err()
    );
    let wire = json!({"model":"glm-5.3","input":"go","reasoning":{"effort":"medium"}});
    let native = native(GenerationApi::Responses)
        .decode_request(wire.to_string().as_bytes())
        .unwrap();
    assert_eq!(
        native
            .task
            .semantic
            .settings()
            .effort
            .as_ref()
            .unwrap()
            .level,
        EffortLevel::High
    );
    let input = client(GenerationApi::Responses)
        .decode_client_request(wire.to_string().as_bytes())
        .unwrap();
    let (wire, rules) = endpoint("dashscope-glm", GenerationProtocol::OpenAiResponses)
        .encode_request_with_rules(&input)
        .unwrap();
    assert_eq!(wire["reasoning"]["effort"], "high");
    assert_eq!(rules, ["bailian.glm-5.3.responses.effort.v1"]);
}
#[test]
fn deepseek_bindings_keep_bailian_identity_controls_and_strictness_boundary() {
    for (api, protocol) in [
        (GenerationApi::Chat, GenerationProtocol::OpenAiChat),
        (
            GenerationApi::Responses,
            GenerationProtocol::OpenAiResponses,
        ),
    ] {
        let target = endpoint("dashscope-deepseek", protocol);
        for effort in ["low", "high", "max", "none"] {
            let mut wire = match api {
                GenerationApi::Chat => json!({
                    "model":"deepseek-v4.1-flash-1017",
                    "messages":[{"role":"user","content":"go"}],
                    "reasoning_effort":effort,"max_completion_tokens":64,
                    "tools":[{"type":"function","function":{"name":"lookup",
                        "parameters":{"type":"object"},"strict":false}}]
                }),
                GenerationApi::Responses => json!({
                    "model":"deepseek-v4.1-flash-1017",
                    "input":"go","reasoning":{"effort":effort},"max_output_tokens":64,
                    "tools":[{"type":"function","name":"lookup",
                        "parameters":{"type":"object"},"strict":false}]
                }),
            };
            let input = client(api)
                .decode_client_request(wire.to_string().as_bytes())
                .unwrap();
            let before = input.clone();
            let (prepared, rules) = target.encode_request_with_rules(&input).unwrap();
            assert_eq!(prepared["model"], "deepseek-v4.1-flash-1017");
            if effort == "none" {
                assert_eq!(prepared["enable_thinking"], false);
            }
            if api == GenerationApi::Chat {
                assert_eq!(prepared["max_completion_tokens"], 64);
                assert!(prepared.get("max_tokens").is_none());
                if effort != "none" {
                    assert_eq!(prepared["reasoning_effort"], effort);
                }
                wire["tools"][0]["function"]["strict"] = json!(true);
            } else {
                assert_eq!(prepared["max_output_tokens"], 64);
                if effort != "none" {
                    assert_eq!(prepared["reasoning"]["effort"], effort);
                    assert!(rules.contains(&"generation.reply-only-output-ceiling.v1"));
                }
                wire["tools"][0]["strict"] = json!(true);
            }
            assert_eq!(input, before);
            let strict = client(api)
                .decode_client_request(wire.to_string().as_bytes())
                .unwrap();
            let (prepared, rules) = target.encode_request_with_rules(&strict).unwrap();
            let tool = if api == GenerationApi::Chat {
                &prepared["tools"][0]["function"]
            } else {
                &prepared["tools"][0]
            };
            assert!(tool.get("strict").is_none());
            assert!(rules.contains(&"bailian.relax-function-strictness.v1"));
        }
    }
}
fn response_request() -> Value {
    json!({"model":"qwen3.8-flash","input":"go","max_output_tokens":64,
            "tools":[{"type":"function","name":"lookup","description":"lookup value","parameters":{"type":"object"}}]})
}
#[test]
fn bailian_unverified_strictness_is_relaxed_without_changing_schemas_or_source() {
    let schema = json!({"type":"object","properties":{"key":{"type":"string","minLength":5}},
        "required":["key"],"additionalProperties":false});
    for inline in [false, true] {
        for strict in [Some(true), Some(false), None] {
            let mut function = json!({"type":"function","name":"lookup","description":"Look up a value",
                "parameters":schema});
            if let Some(strict) = strict {
                function["strict"] = json!(strict);
            }
            let mut body = json!({"model":"public","input":"go","tools":[function]});
            if inline {
                body.as_object_mut().unwrap().remove("tools");
                body["input"] = json!([
                    {"type":"additional_tools","role":"developer","tools":[function]},
                    {"role":"user","content":"go"}
                ]);
            }
            let source = client(GenerationApi::Responses)
                .decode_client_request(body.to_string().as_bytes())
                .unwrap();
            let before = source.clone();
            for prefix in ["dashscope-flash", "dashscope-glm", "tokenplan-max"] {
                for protocol in [
                    GenerationProtocol::OpenAiChat,
                    GenerationProtocol::OpenAiResponses,
                ] {
                    let target = endpoint(prefix, protocol);
                    let (wire, rules) = target.encode_request_with_rules(&source).unwrap();
                    let tools = wire
                        .get("tools")
                        .and_then(Value::as_array)
                        .or_else(|| {
                            wire["input"]
                                .as_array()
                                .and_then(|items| {
                                    items.iter().find(|item| item["type"] == "additional_tools")
                                })
                                .and_then(|batch| batch["tools"].as_array())
                        })
                        .unwrap();
                    let actual = if protocol == GenerationProtocol::OpenAiChat {
                        &tools[0]["function"]
                    } else {
                        &tools[0]
                    };
                    assert_eq!(actual["parameters"], schema);
                    if inline && protocol == GenerationProtocol::OpenAiResponses {
                        // Standard retained batches spell their non-strict default explicitly.
                        assert_eq!(actual["strict"], false);
                    } else {
                        assert!(actual.get("strict").is_none());
                    }
                    assert_eq!(
                        rules.contains(&"bailian.relax-function-strictness.v1"),
                        strict != Some(false)
                    );
                    let morphiecore::lowering::target::TargetContract::OpenAi(contract) =
                        &target.representation
                    else {
                        panic!("OpenAI contract")
                    };
                    let profile = if protocol == GenerationProtocol::OpenAiChat {
                        GenerationApi::Chat
                    } else {
                        GenerationApi::Responses
                    };
                    let lowered = morphiecore::protocol::openai::lowering::lower_request(
                        &source.task.semantic,
                        &source.task.fidelity,
                        profile,
                        (**contract).clone(),
                    )
                    .unwrap();
                    let direct = if profile == GenerationApi::Chat {
                        morphiecore::protocol::openai::chat::encode_generation(&lowered).unwrap()
                    } else {
                        morphiecore::protocol::openai::responses::encode_generation(&lowered)
                            .unwrap()
                    };
                    assert_eq!(direct.get("tools"), wire.get("tools"));
                    assert_eq!(source, before);
                }
            }
            let standard = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None)
                .encode_request(&source, "public", &TargetContract::full())
                .unwrap();
            let expected = if inline {
                &standard["input"][0]["tools"][0]
            } else {
                &standard["tools"][0]
            };
            assert_eq!(expected.get("strict"), function.get("strict"));
        }
    }
    // A permissive direct contract or an unknown future model cannot opt in.
    let source = client(GenerationApi::Responses)
        .decode_client_request(
            br#"{"model":"unqualified","input":"go","tools":[{"type":"function","name":"lookup",
        "description":"lookup","strict":true,"parameters":{"type":"object"}}]}"#,
        )
        .unwrap();
    for api in [GenerationApi::Chat, GenerationApi::Responses] {
        let (wire, rules) = native(api)
            .encode_request_with_rules(&source, "unqualified", &TargetContract::full())
            .unwrap();
        assert!(!wire.to_string().contains("\"strict\""));
        assert!(rules.contains(&"bailian.relax-function-strictness.v1"));
    }
}

#[test]
fn responses_non_strict_defaults_and_single_function_selection_have_native_oracles() {
    let mut reasoning = ReasoningRequest::absent();
    reasoning.presence = ReasoningPresence::Present;
    reasoning.activation = Some(ReasoningActivation::Disabled);
    let native_request = native(GenerationApi::Responses)
        .decode_request(response_request().to_string().as_bytes())
        .unwrap();
    assert!(
        matches!(&native_request.task.semantic.tools()[0],ToolDefinition::Function(t)
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
        reasoning,
        ..Default::default()
    };
    let input = request(settings);
    let target = endpoint("dashscope-flash", GenerationProtocol::OpenAiResponses);
    let (wire, rules) = target.encode_request_with_rules(&input).unwrap();
    let mut expected = response_request();
    expected["enable_thinking"] = json!(false);
    expected["store"] = json!(false);
    expected["stream"] = json!(true);
    expected["input"] =
        json!([{"type":"message","role":"user","content":[{"type":"input_text","text":"go"}]}]);
    expected["tool_choice"] = json!({"type":"allowed_tools","mode":"required","tools":[{"type":"function","name":"lookup"}]});
    assert_eq!(wire, expected);
    assert!(rules.is_empty());
    let mut source = response_request();
    source["tools"][0]["strict"] = json!(false);
    source["enable_thinking"] = json!(true);
    let input = client(GenerationApi::Responses)
        .decode_client_request(source.to_string().as_bytes())
        .unwrap();
    let (wire, _) = target.encode_request_with_rules(&input).unwrap();
    assert_eq!(wire["enable_thinking"], true);
    assert!(wire.get("reasoning").is_none());
    assert!(wire["tools"][0].get("strict").is_none());
    let source = client(GenerationApi::Responses)
        .decode_client_request(response_request().to_string().as_bytes())
        .unwrap();
    let before = source.clone();
    let (wire, rules) = target.encode_request_with_rules(&source).unwrap();
    assert_eq!(wire["tools"][0]["parameters"], json!({"type":"object"}));
    assert!(wire["tools"][0].get("strict").is_none());
    assert_eq!(rules, ["bailian.relax-function-strictness.v1"]);
    let lowered = morphiecore::protocol::openai::lowering::lower_request(
        &source.task.semantic,
        &source.task.fidelity,
        GenerationApi::Responses,
        native(GenerationApi::Responses).contract(&TargetContract::full()),
    )
    .unwrap();
    let direct = morphiecore::protocol::openai::responses::encode_generation(&lowered).unwrap();
    assert_eq!(direct["tools"], wire["tools"]);
    assert_eq!(source, before);
}
#[test]
fn responses_forced_subset_filters_inventory_without_rewriting_history_or_schema() {
    use morphiecore::protocol::conversion::generation::RepresentationError;
    let selected = json!({"type":"function","name":"lookup","description":"lookup value",
        "strict":false,"parameters":{"type":"object","properties":{"code":{"type":"integer","enum":[7]}},
        "required":["code"],"additionalProperties":false}});
    let decoy = json!({"type":"function","name":"decoy","description":"previous operation",
        "strict":false,"parameters":{"type":"object"}});
    let target = endpoint("dashscope-flash", GenerationProtocol::OpenAiResponses);
    let morphiecore::lowering::target::TargetContract::OpenAi(contract) = &target.representation
    else {
        panic!("OpenAI contract")
    };
    for inline in [false, true] {
        for choice in [
            json!({"type":"function","name":"lookup"}),
            json!({"type":"allowed_tools","mode":"required","tools":[{"type":"function","name":"lookup"}]}),
        ] {
            let mut body = json!({"model":"public","reasoning":{"effort":"none"},
                "tools":[selected,decoy],"tool_choice":choice,"parallel_tool_calls":false,
                "input":[{"role":"user","content":"previous"},
                    {"type":"function_call","id":"old-item","call_id":"old-call",
                     "name":"decoy","arguments":"{ \"old\": 1 }"},
                    {"type":"function_call_output","call_id":"old-call","output":"previous result"},
                    {"role":"user","content":"next"}]});
            if inline {
                let tools = body.as_object_mut().unwrap().remove("tools").unwrap();
                body["input"].as_array_mut().unwrap().insert(
                    0,
                    json!({"type":"additional_tools","role":"developer","tools":tools}),
                );
            }
            let source = client(GenerationApi::Responses)
                .decode_client_request(body.to_string().as_bytes())
                .unwrap();
            let before = source.clone();
            let (wire, rules) = target.encode_request_with_rules(&source).unwrap();
            let mut expected = selected.clone();
            expected.as_object_mut().unwrap().remove("strict");
            assert_eq!(wire["tools"], json!([expected]));
            assert_eq!(
                wire["tool_choice"],
                json!({"type":"allowed_tools","mode":"required","tools":[{"type":"function","name":"lookup"}]})
            );
            assert_eq!(wire["parallel_tool_calls"], false);
            assert_eq!(wire["enable_thinking"], false);
            assert!(rules.contains(&"generation.filter-allowed-inventory.v1"));
            let history = wire["input"].as_array().unwrap();
            assert!(
                !history
                    .iter()
                    .any(|item| item["type"] == "additional_tools")
            );
            let call = history
                .iter()
                .find(|item| item["type"] == "function_call")
                .unwrap();
            assert_eq!(call["call_id"], "old-call");
            assert_eq!(call["name"], "decoy");
            assert_eq!(call["arguments"], "{ \"old\": 1 }");
            assert!(
                history
                    .iter()
                    .any(|item| item["type"] == "function_call_output"
                        && item["call_id"] == "old-call"
                        && item["output"] == "previous result")
            );
            let lowered = morphiecore::protocol::openai::lowering::lower_request(
                &source.task.semantic,
                &source.task.fidelity,
                GenerationApi::Responses,
                (**contract).clone(),
            )
            .unwrap();
            let direct =
                morphiecore::protocol::openai::responses::encode_generation(&lowered).unwrap();
            assert_eq!(direct["tools"], wire["tools"]);
            assert_eq!(direct["tool_choice"], wire["tool_choice"]);
            assert_eq!(source, before);

            // Inventory reduction cannot supply a missing thinking-mode guarantee.
            body["reasoning"] = json!({"effort":"low"});
            let thinking = client(GenerationApi::Responses)
                .decode_client_request(body.to_string().as_bytes())
                .unwrap();
            body["reasoning"] = json!({"effort":"none"});
            body["tool_choice"] = json!("required");
            let unrestricted = client(GenerationApi::Responses)
                .decode_client_request(body.to_string().as_bytes())
                .unwrap();
            for rejected in [thinking, unrestricted] {
                assert!(matches!(
                    target.encode_request(&rejected),
                    Err(ConversionFailure {
                        cause: morphiecore::protocol::ConversionCause::Representation(
                            RepresentationError::Admission(Feature::ToolChoice)
                        ),
                        ..
                    })
                ));
            }
        }
    }
}

#[test]
fn responses_serial_auto_tools_keep_the_native_control_and_source() {
    let mut wire = response_request();
    wire.as_object_mut().unwrap().remove("max_output_tokens");
    wire["parallel_tool_calls"] = json!(false);
    wire["tool_choice"] = json!("auto");
    let native_request = native(GenerationApi::Responses)
        .decode_request(wire.to_string().as_bytes())
        .unwrap();
    assert_eq!(
        native_request.task.semantic.settings().parallel_tool_calls,
        Some(false)
    );
    wire["tools"][0]["strict"] = json!(false);
    let source = client(GenerationApi::Responses)
        .decode_client_request(wire.to_string().as_bytes())
        .unwrap();
    for (prefix, model) in [
        ("dashscope-flash", "qwen3.8-flash"),
        ("dashscope-glm", "glm-5.3"),
    ] {
        let before = source.clone();
        let (encoded, rules) = endpoint(prefix, GenerationProtocol::OpenAiResponses)
            .encode_request_with_rules(&source)
            .unwrap();
        let mut expected = wire.clone();
        expected["model"] = json!(model);
        expected["store"] = json!(false);
        expected["stream"] = json!(true);
        expected["input"] =
            json!([{"type":"message","role":"user","content":[{"type":"input_text","text":"go"}]}]);
        expected["tools"][0]
            .as_object_mut()
            .unwrap()
            .remove("strict");
        assert_eq!(encoded, expected);
        assert!(rules.is_empty());
        assert_eq!(source, before);
    }
    wire["model"] = json!("kimi-k3.1-pro");
    assert_eq!(
        native(GenerationApi::Responses)
            .decode_request(wire.to_string().as_bytes())
            .unwrap()
            .task
            .semantic
            .parallel_tool_calls(),
        Some(false)
    );
}

#[test]
fn responses_thinking_profiles_reject_unenforced_forced_selection() {
    use morphiecore::protocol::conversion::generation::RepresentationError;
    for choice in [
        json!("required"),
        json!({"type":"allowed_tools","mode":"required","tools":[{"type":"function","name":"lookup"}]}),
        json!({"type":"function","name":"lookup"}),
    ] {
        for (prefix, model) in [
            ("dashscope-flash", "qwen3.8-flash"),
            ("dashscope-glm", "glm-5.3"),
        ] {
            let mut wire = response_request();
            wire.as_object_mut().unwrap().remove("max_output_tokens");
            wire["model"] = json!(model);
            wire["tools"][0]["strict"] = json!(false);
            native(GenerationApi::Responses)
                .decode_request(wire.to_string().as_bytes())
                .expect("the unforced native profile is otherwise expressible");
            wire["tool_choice"] = choice.clone();
            let source = client(GenerationApi::Responses)
                .decode_client_request(wire.to_string().as_bytes())
                .unwrap();
            assert!(matches!(
                endpoint(prefix, GenerationProtocol::OpenAiResponses)
                    .encode_request_with_rules(&source),
                Err(ConversionFailure {
                    cause: morphiecore::protocol::ConversionCause::Representation(
                        RepresentationError::Admission(Feature::ToolChoice)
                    ),
                    ..
                })
            ));
            assert!(
                native(GenerationApi::Responses)
                    .decode_request(wire.to_string().as_bytes())
                    .is_err()
            );
            let morphiecore::lowering::target::TargetContract::OpenAi(contract) =
                endpoint(prefix, GenerationProtocol::OpenAiResponses).representation
            else {
                panic!("OpenAI contract")
            };
            assert!(
                morphiecore::protocol::openai::lowering::lower_request(
                    &source.task.semantic,
                    &source.task.fidelity,
                    GenerationApi::Responses,
                    *contract,
                )
                .is_err(),
                "model-bound direct lowering must keep the forcing restriction"
            );
        }
    }
}

#[test]
fn responses_history_keeps_summary_and_transfers_uncarried_readable_reasoning() {
    let target = endpoint("dashscope-flash", GenerationProtocol::OpenAiResponses);
    let summary = json!({"type":"reasoning","id":"reason-0",
        "summary":[{"type":"summary_text","text":"plan"}]});
    let mut body = json!({"model":"qwen3.8-flash","input":[
        {"role":"user","content":"first"}, summary,
        {"type":"message","id":"message-kept","status":"completed","role":"assistant",
         "content":[{"type":"output_text","text":"answer","annotations":[]}]},
        {"role":"user","content":"next"}
    ]});
    let input = client(GenerationApi::Responses)
        .decode_client_request(body.to_string().as_bytes())
        .unwrap();
    let (wire, rules) = target.encode_request_with_rules(&input).unwrap();
    assert_eq!(wire["input"][1], body["input"][1]);
    assert!(rules.is_empty());

    body["input"][1]["content"] =
        json!([{"type":"reasoning_text","text":"uncarried full reasoning"}]);
    let input = client(GenerationApi::Responses)
        .decode_client_request(body.to_string().as_bytes())
        .unwrap();
    let original = input.clone();
    let (wire, rules) = target.encode_request_with_rules(&input).unwrap();
    assert_eq!(
        wire["input"],
        json!([
            {"type":"message","role":"user","content":[{"type":"input_text","text":"first"}]},
        {"type":"message","id":"message-kept","status":"completed","role":"assistant",
         "content":[{"type":"output_text","text":"answer","annotations":[]}]},
            {"type":"message","role":"user","content":[{"type":"input_text","text":"next"}]}
        ])
    );
    assert_eq!(rules, ["responses.transfer-visible-history.v1"]);
    assert_eq!(input, original);
    let delegated = endpoint("aliyun-tokenplan-cn", GenerationProtocol::OpenAiResponses)
        .encode_request_with_rules(&input)
        .unwrap();
    assert_eq!(delegated.0["input"], wire["input"]);
    assert_eq!(delegated.1, ["responses.transfer-visible-history.v1"]);
    let mut explicit = input.clone();
    let mut settings = explicit.task.semantic.settings().clone();
    settings.reasoning.presence = ReasoningPresence::Present;
    settings.reasoning.read_history = Some(true);
    explicit.task.semantic = explicit.task.semantic.with_settings(settings).unwrap();
    assert!(matches!(
        target.encode_request(&explicit),
        Err(ConversionFailure {
            cause: morphiecore::protocol::ConversionCause::Codec(CodecError::Unsupported(
                morphiecore::protocol::failure::CarrierReason::Reasoning
            )),
            ..
        })
    ));
    assert_eq!(input, original);
}

#[test]
fn responses_presentation_projects_without_changing_computation_or_source() {
    let target = endpoint("dashscope-flash", GenerationProtocol::OpenAiResponses);
    let mut body = json!({"model":"qwen3.8-flash","input":"go","max_output_tokens":64,
        "reasoning":{"effort":"low","summary":"auto"},
        "include":["reasoning.encrypted_content"]});
    let input = client(GenerationApi::Responses)
        .decode_client_request(body.to_string().as_bytes())
        .unwrap();
    let original = input.clone();
    let (wire, rules) = target.encode_request_with_rules(&input).unwrap();
    assert_eq!(wire["reasoning"], json!({"effort":"low"}));
    assert!(wire.get("enable_thinking").is_none());
    assert!(wire.get("include").is_none());
    assert_eq!(rules, ["generation.omit-encrypted-reasoning-include.v1"]);
    assert_eq!(input, original);
    let standard = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None)
        .encode_request(&input, "m", &TargetContract::full())
        .unwrap();
    assert_eq!(standard["reasoning"], body["reasoning"]);
    assert_eq!(standard["include"], body["include"]);
    let mut default_text = body.clone();
    default_text["text"] = json!({"format":{"type":"text"}});
    let default_text = client(GenerationApi::Responses)
        .decode_client_request(default_text.to_string().as_bytes())
        .unwrap();
    let before = default_text.clone();
    let (wire, _) = target.encode_request_with_rules(&default_text).unwrap();
    assert!(wire.get("text").is_none());
    assert_eq!(default_text, before);
    let mut explicit = input.clone();
    let mut settings = explicit.task.semantic.settings().clone();
    settings.reasoning.read_history = Some(true);
    explicit.task.semantic = explicit.task.semantic.with_settings(settings).unwrap();
    assert!(matches!(
        target.encode_request(&explicit),
        Err(ConversionFailure {
            cause: morphiecore::protocol::ConversionCause::Representation(
                morphiecore::protocol::conversion::generation::RepresentationError::Admission(
                    morphiecore::semantic::task::generation::Feature::ReasoningHistoryRead
                )
            ),
            ..
        })
    ));

    body["reasoning"]["summary"] = json!("concise");
    let input = client(GenerationApi::Responses)
        .decode_client_request(body.to_string().as_bytes())
        .unwrap();
    let original = input.clone();
    let (wire, rules) = target.encode_request_with_rules(&input).unwrap();
    assert_eq!(wire["reasoning"], json!({"effort":"low"}));
    assert_eq!(
        rules,
        [
            "generation.omit-encrypted-reasoning-include.v1",
            "generation.omit-reasoning-summary.v1"
        ]
    );
    assert_eq!(input, original);

    body["reasoning"] = json!({"effort":"none"});
    let input = client(GenerationApi::Responses)
        .decode_client_request(body.to_string().as_bytes())
        .unwrap();
    let original = input.clone();
    let (wire, rules) = target.encode_request_with_rules(&input).unwrap();
    assert_eq!(wire["enable_thinking"], false);
    assert!(wire.get("include").is_none());
    assert!(
        rules.is_empty(),
        "thinking-off has no new reasoning to encrypt"
    );
    assert_eq!(input, original);
}

#[test]
fn responses_ceiling_scope_projects_usefully_without_changing_source() {
    let codec = native(GenerationApi::Responses);
    assert!(
        codec
            .decode_request(br#"{"model":"glm-5.3","input":"go","max_output_tokens":64}"#)
            .is_err(),
        "native reply-only input must not become an unevidenced total-output ceiling"
    );
    for (model, disabled, cap, expected_cap, expected_rule) in [
        (
            "glm-5.3",
            false,
            64,
            Some(64),
            Some("generation.reply-only-output-ceiling.v1"),
        ),
        (
            "glm-5.3",
            false,
            15,
            None,
            Some("generation.omit-max-output-tokens.v1"),
        ),
        ("qwen3.8-flash", false, 64, Some(64), None),
        ("deepseek-v4.1-flash-1017", true, 64, Some(64), None),
    ] {
        let mut body = json!({"model":"public","input":"go","max_output_tokens":cap});
        if disabled {
            body["reasoning"] = json!({"effort":"none"});
        }
        let input = client(GenerationApi::Responses)
            .decode_client_request(body.to_string().as_bytes())
            .unwrap();
        let before = input.clone();
        let (wire, rules) = codec
            .encode_request_with_rules(&input, model, &TargetContract::full())
            .unwrap();
        assert_eq!(
            wire.get("max_output_tokens").and_then(Value::as_u64),
            expected_cap
        );
        assert_eq!(
            rules.contains(&"generation.reply-only-output-ceiling.v1"),
            expected_rule == Some("generation.reply-only-output-ceiling.v1")
        );
        if let Some(rule) = expected_rule {
            assert!(rules.contains(&rule), "{rules:?}");
        }
        assert_eq!(input, before);
    }
}

#[test]
fn responses_output_constraints_reach_preparation_without_losing_the_schema() {
    let schema = json!({"type":"object","properties":{"ok":{"type":"boolean"}},
        "required":["ok"],"additionalProperties":false});
    for format in [
        json!({"type":"json_object"}),
        json!({"type":"json_schema","name":"answer","strict":true,"schema":schema}),
    ] {
        let source = json!({"model":"public-model","input":"Return JSON.",
            "max_output_tokens":64,"text":{"format":format}});
        let input = client(GenerationApi::Responses)
            .decode_client_request(source.to_string().as_bytes())
            .unwrap();
        let before = input.clone();
        let target = endpoint("dashscope-flash", GenerationProtocol::OpenAiResponses);
        let (wire, rules) = target.encode_request_with_rules(&input).unwrap();
        assert_eq!(wire["text"], json!({"format":format}));
        assert!(rules.is_empty());
        assert_eq!(input, before);
    }
}

#[test]
fn responses_unlisted_controls_project_without_changing_source() {
    let target = endpoint("dashscope-flash", GenerationProtocol::OpenAiResponses);
    for (patch, field) in [
        (json!({"reasoning":{"summary":null}}), "reasoning"),
        (json!({"truncation":"auto"}), "truncation"),
        (json!({"max_output_tokens":15}), "max_output_tokens"),
    ] {
        let mut wire = response_request();
        wire["tools"][0]["strict"] = json!(false);
        wire.as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        let input = client(GenerationApi::Responses)
            .decode_client_request(wire.to_string().as_bytes())
            .unwrap();
        let before = input.clone();
        let (wire, _) = target.encode_request_with_rules(&input).unwrap();
        assert!(wire.get(field).is_none(), "{field}");
        assert_eq!(input, before);
    }
}

#[test]
fn responses_associate_grouped_results_by_call_id_without_reordering() {
    let target = endpoint("dashscope-flash", GenerationProtocol::OpenAiResponses);
    let calls = [
        json!({"id":"fc_a","type":"function_call","name":"lookup","call_id":"a","arguments":"{\"key\":\"A\"}"}),
        json!({"id":"fc_b","type":"function_call","name":"lookup","call_id":"b","arguments":"{\"key\":\"B\"}"}),
    ];
    let results = [
        json!({"type":"function_call_output","call_id":"a","output":[
            {"type":"input_text","text":"value-"},{"type":"input_text","text":""},{"type":"input_text","text":"A"}]}),
        json!({"type":"function_call_output","call_id":"b","output":"value-B"}),
    ];
    for history in [
        json!([calls[0], calls[1], results[0], results[1]]),
        json!([calls[0], results[0], calls[1], results[1]]),
    ] {
        let wire = json!({"model":"qwen3.8-flash","input":history});
        for codec in [
            GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None),
            native(GenerationApi::Responses),
        ] {
            let input = codec.decode_request(wire.to_string().as_bytes()).unwrap();
            let result = input
                .task
                .semantic
                .items()
                .iter()
                .find_map(|(_, item)| match item {
                    Item::ToolResult(result)
                        if result.call_id.as_ref().is_some_and(|id| id.as_str() == "a") =>
                    {
                        Some(result)
                    }
                    _ => None,
                })
                .unwrap();
            let ToolOutput::Parts(parts) = &result.output else {
                panic!("text result parts must retain their boundaries");
            };
            let texts: Vec<_> = parts
                .iter()
                .map(|part| match &part.content {
                    InputPart::Text(text) => text.as_str(),
                    _ => panic!("text-only feedback"),
                })
                .collect();
            assert_eq!(texts, ["value-", "", "A"]);
            let before = input.clone();
            let (encoded, rules) = target.encode_request_with_rules(&input).unwrap();
            assert_eq!(encoded["input"], history);
            assert!(rules.is_empty());
            let lowered = morphiecore::protocol::openai::lowering::lower_request(
                &input.task.semantic,
                &input.task.fidelity,
                GenerationApi::Responses,
                native(GenerationApi::Responses).contract(&TargetContract::full()),
            )
            .unwrap();
            let direct =
                morphiecore::protocol::openai::responses::encode_generation(&lowered).unwrap();
            assert_eq!(direct["input"], history);
            assert_eq!(input, before);
        }
    }
    for history in [
        json!([results[0], calls[0]]),
        json!([calls[0], {"id":"fc_duplicate","type":"function_call","name":"lookup","call_id":"a","arguments":"{}"}, results[0]]),
        json!([calls[0], results[0], results[0]]),
        json!([calls[0], results[1]]),
    ] {
        let wire = json!({"model":"qwen3.8-flash","input":history});
        let rejected =
            client(GenerationApi::Responses).decode_client_request(wire.to_string().as_bytes());
        assert!(
            matches!(rejected, Err(CodecError::Semantic(_))),
            "{history}: {rejected:?}"
        );
    }
}
#[test]
fn qwen4_native_output_caps_include_thinking_without_changing_older_chat_caps() {
    for profile in [GenerationApi::Chat, GenerationApi::Responses] {
        let wire = if profile == GenerationApi::Chat {
            json!({"model":"qwen4-max-preview","messages":[{"role":"user","content":"go"}],
                "reasoning_effort":"medium","max_tokens":512})
        } else {
            json!({"model":"qwen4-max-preview","input":[{"role":"user","content":"go"}],
                "reasoning":{"effort":"medium"},"max_output_tokens":512})
        };
        let source = native(profile)
            .decode_request(wire.to_string().as_bytes())
            .unwrap();
        assert_eq!(source.task.semantic.controls().max_output_tokens, Some(512));
        let protocol = if profile == GenerationApi::Chat {
            GenerationProtocol::OpenAiChat
        } else {
            GenerationProtocol::OpenAiResponses
        };
        let (projected, rules) = endpoint("tokenplan-qwen4", protocol)
            .encode_request_with_rules(&source)
            .unwrap();
        let cap = if profile == GenerationApi::Chat {
            "max_completion_tokens"
        } else {
            "max_output_tokens"
        };
        assert_eq!(projected[cap], 512);
        assert!(!rules.iter().any(|rule| rule.contains("reply-only")));
        assert_eq!(
            native(profile)
                .decode_request(projected.to_string().as_bytes())
                .unwrap()
                .task
                .semantic
                .controls()
                .max_output_tokens,
            Some(512)
        );
    }
    let old = json!({"model":"qwen3.8-max","messages":[{"role":"user","content":"go"}],
        "reasoning_effort":"medium","max_tokens":512});
    assert!(
        native(GenerationApi::Chat)
            .decode_request(old.to_string().as_bytes())
            .is_err()
    );

    let wire = json!({"model":"kimi-k3.1-pro","messages":[{"role":"user","content":"go"}],
        "reasoning_effort":"medium"});
    let source = client(GenerationApi::Chat)
        .decode_client_request(wire.to_string().as_bytes())
        .unwrap();
    let (projected, _) = endpoint("dashscope-kimi", GenerationProtocol::OpenAiChat)
        .encode_request_with_rules(&source)
        .unwrap();
    assert_eq!(projected["reasoning_effort"], "medium");
}
#[test]
fn kimi_fixed_sampling_is_projected_without_mutating_source_or_ignoring_thinking_off() {
    for effort in ["none", "medium"] {
        for (temperature, top_p, penalty, omitted) in
            [(0.5, 0.8, 0.2, true), (1.0, 0.95, 0.0, false)]
        {
            let wire = json!({"model":"kimi-k3.1-pro","messages":[{"role":"user","content":"go"}],
                "reasoning_effort":effort,"temperature":temperature,"top_p":top_p,"presence_penalty":penalty});
            assert_eq!(
                native(GenerationApi::Chat)
                    .decode_request(wire.to_string().as_bytes())
                    .is_err(),
                omitted
            );
            let request = client(GenerationApi::Chat)
                .decode_client_request(wire.to_string().as_bytes())
                .unwrap();
            let original = request.clone();
            let (projected, rules) = endpoint("dashscope-kimi", GenerationProtocol::OpenAiChat)
                .encode_request_with_rules(&request)
                .unwrap();
            for key in ["temperature", "top_p", "presence_penalty"] {
                if omitted {
                    assert!(projected.get(key).is_none(), "{key}");
                } else {
                    assert!(projected.get(key).is_none_or(|value| value == &wire[key]));
                }
            }
            for rule in ["generation.omit-temperature.v1", "generation.omit-top-p.v1"] {
                assert_eq!(rules.contains(&rule), omitted);
            }
            if omitted {
                assert!(rules.contains(&"generation.omit-presence-penalty.v1"));
            }
            assert_eq!(request, original);
        }
    }
}
