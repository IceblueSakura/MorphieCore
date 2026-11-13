//! Independent synthetic oracles for scoped provider boundary differences.
use morphiecore::{
    protocol::openai::{Dialect, GenerationApi, GenerationCodec, TargetContract as Contract},
    semantic::value::ReplayOrigin,
};
use serde_json::{Value, json};
fn adapter(dialect: Dialect) -> GenerationCodec {
    GenerationCodec::new(
        GenerationApi::Chat,
        dialect,
        Some(ReplayOrigin::new("synthetic-account").unwrap()),
    )
}
fn body() -> Value {
    json!({"id":"c1","object":"chat.completion","model":"m","created":1,
        "choices":[{"index":0,"message":{"role":"assistant","content":"pong","reasoning_content":"think"},"finish_reason":"stop"}],
        "usage":{"prompt_tokens":4,"completion_tokens":2,"total_tokens":6}})
}

#[test]
fn deepseek_chat_preserves_the_native_cap_and_standard_thinking_off_control() {
    use morphiecore::topology::{GenerationProtocol, catalog::API_KEY_BINDINGS};
    let source = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None)
        .decode_client_request(
            br#"{"model":"public","messages":[{"role":"user","content":"go"}],
                "max_tokens":1,"reasoning_effort":"none"}"#,
        )
        .unwrap();
    let before = source.clone();
    let endpoint = API_KEY_BINDINGS
        .iter()
        .find(|b| b.endpoint_prefix == "deepseek")
        .unwrap()
        .endpoint(GenerationProtocol::OpenAiChat)
        .unwrap();
    let (wire, rules) = endpoint.encode_request_with_rules(&source).unwrap();
    assert_eq!(wire["max_tokens"], 1);
    assert!(wire.get("max_completion_tokens").is_none());
    assert_eq!(wire["reasoning_effort"], "none");
    assert!(wire.get("thinking").is_none());
    assert!(rules.is_empty());
    assert_eq!(source, before);
    let standard = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None)
        .encode_request(&source, "m", &Contract::full())
        .unwrap();
    assert_eq!(standard["max_completion_tokens"], 1);
}

#[test]
fn go_standard_effort_controls_survive_model_scoped_projection() {
    use morphiecore::{
        semantic::context::ConversationContext,
        topology::{GenerationProtocol, catalog::API_KEY_BINDINGS},
    };
    for (prefix, protocol, levels, supports_off) in [
        (
            "opencode-go",
            GenerationProtocol::OpenAiChat,
            vec![("high", "high"), ("medium", "high")],
            true,
        ),
        (
            "opencode-go-minimax",
            GenerationProtocol::OpenAiChat,
            vec![("low", "low"), ("max", "max")],
            false,
        ),
    ] {
        let endpoint = API_KEY_BINDINGS
            .iter()
            .find(|b| b.endpoint_prefix == prefix)
            .unwrap()
            .endpoint(protocol)
            .unwrap();
        for api in [GenerationApi::Chat, GenerationApi::Responses] {
            for (input, expected) in std::iter::once((None, None))
                .chain(supports_off.then_some((Some("none"), Some("none"))))
                .chain(levels.iter().map(|&(a, b)| (Some(a), Some(b))))
            {
                let mut body = match api {
                    GenerationApi::Chat => {
                        json!({"model":"public","messages":[{"role":"user","content":"go"}]})
                    }
                    GenerationApi::Responses => json!({"model":"public","input":"go"}),
                };
                if let Some(effort) = input {
                    match api {
                        GenerationApi::Chat => body["reasoning_effort"] = json!(effort),
                        GenerationApi::Responses => body["reasoning"] = json!({"effort":effort}),
                    }
                }
                let mut source = GenerationCodec::new(api, Dialect::Standard, None)
                    .decode_client_request(&serde_json::to_vec(&body).unwrap())
                    .unwrap();
                source.conversation =
                    Some(ConversationContext::independent_request("synthetic-go").unwrap());
                let before = source.clone();
                let (wire, rules) = endpoint.encode_request_with_rules(&source).unwrap();
                let actual = if protocol == GenerationProtocol::OpenAiChat {
                    wire.get("reasoning_effort")
                } else {
                    wire.get("reasoning").and_then(|r| r.get("effort"))
                };
                assert_eq!(actual, expected.map(|s| json!(s)).as_ref());
                assert_eq!(
                    rules.contains(&"generation.project-effort.v1"),
                    input != expected
                );
                assert_eq!(source, before);
            }
        }
    }
}

#[test]
fn deepseek_chat_rejects_schema_and_thinking_forcing_without_restricting_responses() {
    use morphiecore::{
        protocol::{ConversionCause, conversion::generation::RepresentationError},
        semantic::task::generation::Feature,
        topology::{GenerationProtocol, catalog::API_KEY_BINDINGS},
    };
    let client = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None);
    let binding = API_KEY_BINDINGS
        .iter()
        .find(|b| b.endpoint_prefix == "deepseek")
        .unwrap();
    let responses = binding
        .endpoint(GenerationProtocol::OpenAiResponses)
        .unwrap();
    for prefix in ["deepseek", "deepseek"] {
        let chat = API_KEY_BINDINGS
            .iter()
            .find(|b| b.endpoint_prefix == prefix)
            .unwrap()
            .endpoint(GenerationProtocol::OpenAiChat)
            .unwrap();
        let schema = json!({"type":"json_schema","json_schema":{"name":"record","strict":true,
            "schema":{"type":"object","properties":{"ok":{"type":"boolean"}},
                "required":["ok"],"additionalProperties":false}}});
        for format in [
            json!({"type":"text"}),
            json!({"type":"json_object"}),
            schema,
        ] {
            let body = json!({"model":"public","messages":[{"role":"user","content":"Return JSON."}],
                "response_format":format});
            let source = client
                .decode_request(&serde_json::to_vec(&body).unwrap())
                .unwrap();
            let before = source.clone();
            let result = chat.encode_request(&source);
            if format["type"] == "json_schema" {
                assert!(matches!(
                    result.unwrap_err().cause,
                    ConversionCause::Representation(RepresentationError::Admission(
                        Feature::StructuredOutput
                    ))
                ));
                assert_eq!(
                    responses.encode_request(&source).unwrap()["text"]["format"]["schema"],
                    format["json_schema"]["schema"]
                );
            } else {
                assert_eq!(result.unwrap()["response_format"], format);
            }
            assert_eq!(source, before);
        }
        for choice in [
            json!("required"),
            json!({"type":"allowed_tools","allowed_tools":{"mode":"required","tools":[{"type":"function","function":{"name":"lookup"}}]}}),
        ] {
            for effort in [None, Some("high"), Some("none")] {
                let mut body = json!({"model":"public","messages":[{"role":"user","content":"go"}],
                    "tools":[{"type":"function","function":{"name":"lookup","strict":false,
                        "parameters":{"type":"object","properties":{}}}}],
                    "tool_choice":choice});
                if let Some(effort) = effort {
                    body["reasoning_effort"] = json!(effort);
                }
                let source = client
                    .decode_request(&serde_json::to_vec(&body).unwrap())
                    .unwrap();
                let before = source.clone();
                let result = chat.encode_request(&source);
                if effort == Some("none") {
                    let wire = result.unwrap();
                    assert_eq!(wire["tool_choice"], "required");
                    assert_eq!(wire["reasoning_effort"], "none");
                } else {
                    assert!(matches!(
                        result.unwrap_err().cause,
                        ConversionCause::Representation(RepresentationError::Admission(
                            Feature::ToolChoice
                        ))
                    ));
                }
                if choice == "required" {
                    assert_eq!(
                        responses.encode_request(&source).unwrap()["tool_choice"],
                        "required"
                    );
                }
                assert_eq!(source, before);
            }
        }
        // A full optional set is equivalent to auto; a proper subset must not
        // discard unselected declarations merely to fit the native enum.
        for full in [true, false] {
            let mut body = json!({"model":"public","messages":[{"role":"user","content":"go"}],
                "reasoning_effort":"none",
                "tools":[{"type":"function","function":{"name":"lookup","strict":false,
                    "parameters":{"type":"object","properties":{}}}}],
                "tool_choice":{"type":"allowed_tools","allowed_tools":{"mode":"auto",
                    "tools":[{"type":"function","function":{"name":"lookup"}}]}}});
            if !full {
                body["tools"].as_array_mut().unwrap().push(json!({"type":"function",
                    "function":{"name":"other","strict":false,"parameters":{"type":"object","properties":{}}}}));
            }
            let source = client
                .decode_request(&serde_json::to_vec(&body).unwrap())
                .unwrap();
            let before = source.clone();
            let result = chat.encode_request(&source);
            if full {
                let wire = result.unwrap();
                assert_eq!(wire["tool_choice"], "auto");
                assert_eq!(wire["tools"], body["tools"]);
            } else {
                assert!(matches!(
                    result.unwrap_err().cause,
                    ConversionCause::Representation(RepresentationError::Admission(
                        Feature::ToolChoice
                    ))
                ));
            }
            assert_eq!(source, before);
        }
    }
}

#[test]
fn cross_protocol_assistant_history_mints_responses_message_ids() {
    let codec = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
    let bodies = [
        morphiecore::adapter::ClientAdapter::openai(
            GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None),
            Contract::full(),
        ),
        morphiecore::adapter::ClientAdapter::Messages,
    ];
    for client in bodies {
        let request = client
            .decode_client_request(
                br#"{
            "model":"public","max_tokens":64,"messages":[
                {"role":"user","content":"go"},
                {"role":"assistant","content":"answer"},
                {"role":"user","content":"next"}]}"#,
            )
            .unwrap();
        let before = request.clone();
        let wire = codec
            .encode_request(&request, "m", &Contract::full())
            .unwrap();
        let message = &wire["input"][1];
        assert_eq!(message["type"], "message");
        assert_eq!(message["role"], "assistant");
        assert!(message["id"].as_str().unwrap().starts_with("msg_"));
        assert_eq!(message["content"][0]["text"], "answer");
        assert_eq!(request, before);
    }
}

#[test]
fn generated_message_id_collisions_are_checked_only_on_responses_wire() {
    let responses = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
    let request = responses
        .decode_client_request(
            br#"{"model":"m","input":[
        {"type":"message","id":"msg_3","role":"assistant","status":"completed",
            "content":[{"type":"output_text","text":"A","annotations":[]}]},
        {"role":"user","content":"next"},
        {"role":"assistant","content":"B"},
        {"role":"user","content":"continue"}]}"#,
        )
        .unwrap();
    let before = request.clone();
    let chat = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None)
        .encode_request(&request, "m", &Contract::full())
        .unwrap();
    assert_eq!(chat["messages"][0]["content"], "A");
    assert_eq!(chat["messages"][2]["content"], "B");
    assert!(
        responses
            .encode_request(&request, "m", &Contract::full())
            .is_err()
    );
    assert_eq!(request, before);
}

#[test]
fn gpt_strict_namespaced_tools_reach_native_subscription_and_router_writers() {
    use morphiecore::topology::{
        GenerationProtocol,
        catalog::{API_KEY_BINDINGS, SUBSCRIPTION_BINDINGS},
    };
    let source = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None)
        .decode_client_request(br#"{"model":"public","input":"go","tools":[{
            "type":"namespace","name":"ops","description":"operations","tools":[{"type":"function","name":"lookup","strict":true,
            "parameters":{"type":"object","properties":{},"required":[],"additionalProperties":false}}]}]}"#)
        .unwrap();
    let before = source.clone();
    let endpoints = API_KEY_BINDINGS
        .iter()
        .filter(|b| {
            matches!(
                b.canonical_model,
                "gpt-6-astra" | "gpt-6.1-sol" | "gpt-6-luna" | "gpt-6.1-terra"
            )
        })
        .map(|b| b.endpoint(GenerationProtocol::OpenAiResponses).unwrap())
        .chain(
            SUBSCRIPTION_BINDINGS
                .iter()
                .filter(|b| {
                    matches!(
                        b.model,
                        "gpt-6-astra" | "gpt-6.1-sol" | "gpt-6-luna" | "gpt-6.1-terra"
                    )
                })
                .map(|b| b.endpoint()),
        );
    let inline = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None)
        .decode_client_request(br#"{"model":"public","input":[
            {"type":"additional_tools","role":"developer","tools":[{"type":"function","name":"lookup","strict":true,
                "parameters":{"type":"object","properties":{},"required":[],"additionalProperties":false}}]},
            {"role":"user","content":"go"}]}"#).unwrap();
    let inline_before = inline.clone();
    for endpoint in endpoints {
        let (wire, _) = endpoint.encode_request_with_rules(&source).unwrap();
        assert_eq!(wire["tools"][0]["tools"][0]["strict"], true);
        assert_eq!(
            wire["tools"][0]["tools"][0]["parameters"]["additionalProperties"],
            false
        );
        let (wire, _) = endpoint.encode_request_with_rules(&inline).unwrap();
        assert_eq!(wire["input"][0]["tools"][0]["strict"], true);
    }
    assert_eq!(source, before);
    assert_eq!(inline, inline_before);
}

#[test]
fn siwc_flat_tools_keep_names_schemas_choices_and_call_history() {
    use morphiecore::topology::catalog::SUBSCRIPTION_BINDINGS;
    let client = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
    let tools = json!([
        {"type":"function","name":"lookup","strict":true,
            "parameters":{"type":"object","properties":{"value":{"type":"string"}},
                "required":["value"],"additionalProperties":false}},
        {"type":"custom","name":"query","description":"Query","format":{"type":"text"}}
    ]);
    for choice in [
        json!("auto"),
        json!("none"),
        json!("required"),
        json!({"type":"function","name":"lookup"}),
        json!({"type":"allowed_tools","mode":"required","tools":[{"type":"function","name":"lookup"}]}),
    ] {
        let body = json!({"model":"public","tools":tools,"tool_choice":choice,
            "parallel_tool_calls":false,"input":[
                {"role":"user","content":"go"},
                {"type":"function_call","id":"item-1","call_id":"call-1","name":"lookup",
                    "arguments":"{ \"value\": \"unchanged\" }","status":"completed"},
                {"type":"function_call_output","call_id":"call-1","output":"result"},
                {"role":"user","content":"next"}]});
        let source = client
            .decode_client_request(body.to_string().as_bytes())
            .unwrap();
        let before = source.clone();
        for binding in SUBSCRIPTION_BINDINGS
            .iter()
            .filter(|b| b.profile == "openai")
        {
            let wire = binding.endpoint().encode_request(&source).unwrap();
            assert!(
                wire.get("tools")
                    .is_none_or(|t| t.as_array().unwrap().is_empty())
            );
            assert_eq!(
                wire["input"][0],
                json!({"type":"additional_tools","role":"developer","tools":tools})
            );
            assert_eq!(wire["input"][2]["call_id"], "call-1");
            assert_eq!(wire["input"][2]["name"], "lookup");
            assert_eq!(
                wire["input"][2]["arguments"],
                "{ \"value\": \"unchanged\" }"
            );
            assert_eq!(wire["input"][3]["call_id"], "call-1");
            assert_eq!(wire["input"][3]["output"], "result");
            assert_eq!(wire["tool_choice"], choice);
            assert_eq!(wire["parallel_tool_calls"], false);
        }
        let wire = client
            .encode_request(&source, "m", &Contract::full())
            .unwrap();
        assert_eq!(wire["tools"], tools);
        assert_eq!(source, before);
    }
}

#[test]
fn native_binding_reasoning_controls_match_model_activation_and_effort() {
    use morphiecore::topology::catalog::SUBSCRIPTION_BINDINGS;
    let client = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
    for binding in SUBSCRIPTION_BINDINGS {
        let endpoint = binding.endpoint();
        let off = client
            .decode_client_request(
                br#"{"model":"public","input":"go","reasoning":{"effort":"none"}}"#,
            )
            .unwrap();
        let result = endpoint.encode_request(&off);
        assert_eq!(
            result.is_ok(),
            matches!(binding.model, "gpt-6-luna" | "gpt-6.1-terra"),
            "{}",
            binding.model
        );
        use morphiecore::semantic::task::generation::{
            ConfigurationBoundary, ConfigurationUpdate, Item, ItemId, ReasoningActivation,
        };
        let mut ordered = client
            .decode_client_request(br#"{"model":"public","input":"go"}"#)
            .unwrap();
        let mut items = ordered.task.semantic.items().to_vec();
        items.insert(
            0,
            (
                ItemId::new(99),
                Item::ConfigurationUpdate(ConfigurationUpdate {
                    boundary: ConfigurationBoundary::FollowingResponse,
                    activation: Some(ReasoningActivation::Disabled),
                    effort: None,
                }),
            ),
        );
        ordered.task.semantic = ordered.task.semantic.with_items(items).unwrap();
        assert_eq!(
            endpoint.encode_request(&ordered).is_ok(),
            matches!(binding.model, "gpt-6-luna" | "gpt-6.1-terra")
        );
        for (effort, expected) in [
            ("minimal", "low"),
            ("high", "high"),
            (
                "max",
                if binding.model == "grok-4.7" {
                    "xhigh"
                } else {
                    "max"
                },
            ),
        ] {
            let source = client
                .decode_client_request(
                    json!({"model":"public","input":"go","reasoning":{"effort":effort}})
                        .to_string()
                        .as_bytes(),
                )
                .unwrap();
            let wire = endpoint.encode_request(&source).unwrap();
            assert_eq!(wire["reasoning"]["effort"], expected, "{}", binding.model);
        }
    }
}

#[test]
fn application_instruction_carriers_are_dialect_local_and_never_become_user() {
    use morphiecore::{
        execution::attempt::prepare_with_rules,
        provider::SecretMaterial,
        semantic::task::generation::{Item, MessageRole},
        topology::{GenerationProtocol, catalog::API_KEY_BINDINGS},
    };
    let standard = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
    let source = standard.decode_client_request(br#"{"model":"public","instructions":"R","input":[
        {"role":"system","content":"A"},{"role":"user","content":"U"},{"role":"developer","content":"B"}]}"#).unwrap();
    let original = source.clone();
    let binding = API_KEY_BINDINGS
        .iter()
        .find(|b| b.endpoint_prefix == "deepseek")
        .unwrap();
    for (profile, protocol) in [
        (GenerationApi::Chat, GenerationProtocol::OpenAiChat),
        (
            GenerationApi::Responses,
            GenerationProtocol::OpenAiResponses,
        ),
    ] {
        let endpoint = binding.endpoint(protocol).unwrap();
        let (wire, rules) = endpoint.encode_request_with_rules(&source).unwrap();
        let (prepared, prepared_rules) = prepare_with_rules(
            &endpoint,
            &(binding.provider)(),
            &SecretMaterial::new("synthetic-key").unwrap(),
            &source,
        )
        .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&prepared.body).unwrap(),
            wire
        );
        assert_eq!(prepared_rules, rules);
        let expected = if profile == GenerationApi::Chat {
            json!([{"role":"system","content":"R"},{"role":"system","content":"A"},
                {"role":"user","content":"U"},{"role":"system","content":"B"}])
        } else {
            assert_eq!(wire["instructions"], "R");
            json!([{"role":"system","content":"A"},
                {"type":"message","role":"user","content":[{"type":"input_text","text":"U"}]},
                {"role":"system","content":"B"}])
        };
        assert_eq!(
            wire[if profile == GenerationApi::Chat {
                "messages"
            } else {
                "input"
            }],
            expected
        );
    }
    let bailian = adapter(Dialect::Bailian)
        .encode_request(&source, "qwen3.8-flash", &Contract::full())
        .unwrap();
    assert_eq!(bailian["messages"][0]["role"], "system");
    assert_eq!(bailian["messages"][1]["role"], "system");
    assert_eq!(bailian["messages"][2]["role"], "user");
    assert_eq!(bailian["messages"][3]["role"], "system");
    assert_eq!(source.task, original.task);
    let canonical = standard
        .encode_request(&source, "m", &Contract::full())
        .unwrap();
    assert_eq!(canonical["input"][0]["role"], "developer");
    assert_eq!(canonical["input"][2]["role"], "developer");
    // A trusted native source dialect interprets effective meaning, while a
    // public standard source remains developer regardless of requested model.
    let native = GenerationCodec::new(GenerationApi::Responses, Dialect::DeepSeek, None);
    let wire =
        br#"{"model":"deepseek-flash","input":[{"role":"developer","content":"unprivileged"}]}"#;
    let decoded = native.decode_request(wire).unwrap();
    assert!(
        matches!(&decoded.task.semantic.items()[0].1, Item::Message(m) if m.role==MessageRole::User)
    );
    let decoded = standard.decode_client_request(wire).unwrap();
    assert!(matches!(
        &decoded.task.semantic.items()[0].1,
        Item::Instruction(_)
    ));
    // The native dialect explicitly allows images for its effective-user
    // developer spelling; that does not widen standard instruction content.
    let image = br#"{"model":"deepseek-flash","input":[{"role":"developer","content":[
        {"type":"input_image","image_url":"https://example.invalid/a.png"}]}]}"#;
    let decoded = native.decode_request(image).unwrap();
    assert!(
        matches!(&decoded.task.semantic.items()[0].1, Item::Message(m) if m.role==MessageRole::User)
    );
    assert!(standard.decode_client_request(image).is_err());
    assert!(
        native
            .decode_request(
                br#"{"model":"m","input":[{"role":"developer","content":[
        {"type":"input_file","file_url":"https://example.invalid/a.pdf"}]}]}"#
            )
            .is_err()
    );
}

#[test]
fn binding_parallel_omission_preserves_serial_guarantees_and_source() {
    use morphiecore::{
        semantic::task::generation::Feature,
        topology::{GenerationProtocol, catalog::API_KEY_BINDINGS},
    };
    let client = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
    for prefix in [
        "openrouter",
        "opencode-go-mimo-pro",
        "opencode-go-mimo-flash",
    ] {
        let binding = API_KEY_BINDINGS
            .iter()
            .find(|b| b.endpoint_prefix == prefix)
            .unwrap();
        for &protocol in binding.protocols {
            if !matches!(
                protocol,
                GenerationProtocol::OpenAiChat | GenerationProtocol::OpenAiResponses
            ) {
                continue;
            }
            let endpoint = binding.endpoint(protocol).unwrap();
            assert!(
                !endpoint
                    .representation
                    .dispositions()
                    .native(Feature::ParallelTools)
            );
            for parallel in [true, false] {
                let mut request = client
                    .decode_client_request(
                        json!({"model":binding.model,"input":"go","parallel_tool_calls":parallel})
                            .to_string()
                            .as_bytes(),
                    )
                    .unwrap();
                request.conversation = Some(
                    morphiecore::semantic::context::ConversationContext::conversation(
                        "synthetic-parallel-conversation",
                    )
                    .unwrap(),
                );
                let original = request.clone();
                let (wire, rules) = endpoint
                    .encode_request_with_rules(&request)
                    .unwrap_or_else(|error| panic!("{prefix} {protocol:?} {parallel}: {error:?}"));
                if parallel {
                    assert!(
                        wire.get("parallel_tool_calls").is_none(),
                        "{prefix} {protocol:?}"
                    );
                    assert_eq!(rules, ["generation.omit-parallel-tools.v1"]);
                } else {
                    assert_eq!(wire["parallel_tool_calls"], false);
                    assert!(rules.is_empty());
                }
                assert_eq!(request, original);
                let native = client
                    .encode_request(&request, "m", &Contract::full())
                    .unwrap();
                assert_eq!(native["parallel_tool_calls"], parallel);
            }
        }
    }
}

#[test]
fn deepseek_never_forwards_parallel_control_or_weakens_serial_intent() {
    use morphiecore::{
        execution::attempt::prepare_with_rules,
        provider::SecretMaterial,
        topology::{GenerationProtocol, catalog::API_KEY_BINDINGS},
    };
    let binding = API_KEY_BINDINGS
        .iter()
        .find(|b| b.endpoint_prefix == "deepseek")
        .unwrap();
    for (profile, target) in [
        (GenerationApi::Chat, GenerationProtocol::OpenAiChat),
        (
            GenerationApi::Responses,
            GenerationProtocol::OpenAiResponses,
        ),
    ] {
        let client = GenerationCodec::new(profile, Dialect::Standard, None);
        let endpoint = binding.endpoint(target).unwrap();
        for parallel in [None, Some(true), Some(false)] {
            let mut wire = match profile {
                GenerationApi::Chat => {
                    json!({"model":binding.model,"messages":[{"role":"user","content":"go"}]})
                }
                GenerationApi::Responses => json!({"model":binding.model,"input":"go"}),
            };
            if let Some(value) = parallel {
                wire["parallel_tool_calls"] = json!(value);
            }
            let request = client
                .decode_client_request(wire.to_string().as_bytes())
                .unwrap();
            let original = request.clone();
            let encoded = endpoint.encode_request_with_rules(&request);
            let prepared = prepare_with_rules(
                &endpoint,
                &(binding.provider)(),
                &SecretMaterial::new("synthetic-deepseek-key").unwrap(),
                &request,
            );
            if parallel == Some(false) {
                assert!(matches!(
                    encoded,
                    Err(morphiecore::protocol::ConversionFailure {
                        cause: morphiecore::protocol::ConversionCause::Representation(
                            morphiecore::protocol::conversion::generation::RepresentationError::Tools
                        ),
                        ..
                    })
                ));
                assert!(prepared.is_err());
            } else {
                let (value, rules) = encoded.unwrap();
                assert!(value.get("parallel_tool_calls").is_none());
                assert!(rules.is_empty());
                let (prepared, prepared_rules) = prepared.unwrap();
                assert_eq!(
                    serde_json::from_slice::<Value>(&prepared.body).unwrap(),
                    value
                );
                assert_eq!(prepared_rules, rules);
            }
            assert_eq!(request.task, original.task);
            let standard = client
                .encode_request(&request, "m", &Contract::full())
                .unwrap();
            assert_eq!(
                standard.get("parallel_tool_calls"),
                parallel.as_ref().map(|v| if *v {
                    &Value::Bool(true)
                } else {
                    &Value::Bool(false)
                })
            );
        }
    }
    // Messages' inverse flag must reach the same candidate-local refusal.
    let request = morphiecore::protocol::anthropic::messages::client::decode_client_request(
        br#"{"model":"deepseek-flash","max_tokens":32,"tool_choice":{"type":"auto","disable_parallel_tool_use":true},"messages":[{"role":"user","content":"go"}]}"#
    ).unwrap();
    assert_eq!(
        request.task.semantic.settings().parallel_tool_calls,
        Some(false)
    );
    for target in [
        GenerationProtocol::OpenAiChat,
        GenerationProtocol::OpenAiResponses,
    ] {
        assert!(
            binding
                .endpoint(target)
                .unwrap()
                .encode_request_with_rules(&request)
                .is_err()
        );
    }
}

#[test]
fn zhipu_effort_lands_on_its_three_levels_by_a_named_rule() {
    use morphiecore::topology::{GenerationProtocol, catalog::API_KEY_BINDINGS};
    let endpoint = API_KEY_BINDINGS
        .iter()
        .find(|binding| binding.endpoint_prefix == "zhipu")
        .unwrap()
        .endpoint(GenerationProtocol::OpenAiChat)
        .unwrap();
    // glm-5.3 always thinks and takes only low, high or max (error 1210 otherwise).
    for (effort, sent) in [("medium", "high"), ("minimal", "low"), ("high", "high")] {
        let body = json!({"model":"m","messages":[{"role":"user","content":"q"}],"reasoning_effort":effort});
        let client = GenerationCodec::new(GenerationApi::Chat, Dialect::MorphieCore, None);
        let request = client.decode_request(body.to_string().as_bytes()).unwrap();
        let (wire, rules) = endpoint.encode_request_with_rules(&request).unwrap();
        assert_eq!(wire["reasoning_effort"], sent, "{effort}");
        assert_eq!(rules.contains(&"zhipu.glm-5.3.effort.v1"), effort != "high");
    }
    use morphiecore::semantic::task::generation::{
        ConfigurationBoundary, ConfigurationUpdate, Item, ItemId, ReasoningActivation,
    };
    for update in [false, true] {
        let client = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None);
        let mut body = json!({"model":"m","messages":[{"role":"user","content":"q"}]});
        if !update {
            body["reasoning_effort"] = json!("none");
        }
        let mut request = client.decode_request(body.to_string().as_bytes()).unwrap();
        if update {
            let mut items = request.task.semantic.items().to_vec();
            items.insert(
                0,
                (
                    ItemId::new(99),
                    Item::ConfigurationUpdate(ConfigurationUpdate {
                        boundary: ConfigurationBoundary::FollowingResponse,
                        activation: Some(ReasoningActivation::Disabled),
                        effort: None,
                    }),
                ),
            );
            request.task.semantic = request.task.semantic.with_items(items).unwrap();
        }
        let error = endpoint.encode_request_with_rules(&request).unwrap_err();
        assert!(matches!(
            error.cause,
            morphiecore::protocol::ConversionCause::Representation(
                morphiecore::protocol::conversion::generation::RepresentationError::Admission(
                    morphiecore::semantic::task::generation::Feature::ReasoningActivation
                )
            )
        ));
    }
}
/// Messages-only tool and replay controls on a Chat target: eager input is omitted
/// under a named rule; explicit block-binding enforcement rejects.
#[test]
fn messages_only_controls_leave_chat_targets_by_named_rules() {
    use morphiecore::{
        protocol::anthropic::messages::client::decode_client_request,
        topology::{GenerationProtocol, catalog::API_KEY_BINDINGS},
    };
    let endpoint = API_KEY_BINDINGS
        .iter()
        .find(|binding| binding.endpoint_prefix == "dashscope-flash")
        .unwrap()
        .endpoint(GenerationProtocol::OpenAiChat)
        .unwrap();
    let mut body = json!({"model":"m","max_tokens":64,
            "tools":[{"name":"read","description":"Read a file","input_schema":{"type":"object"},"eager_input_streaming":true}],
            "messages":[{"role":"user","content":"q"}]});
    let request = decode_client_request(body.to_string().as_bytes()).unwrap();
    let (wire, rules) = endpoint.encode_request_with_rules(&request).unwrap();
    assert_eq!(rules, ["generation.omit-eager-tool-input.v1"]);
    assert_eq!(wire["tools"][0]["function"]["name"], "read");
    assert!(!wire.to_string().contains("eager"));
    // Explicit replay enforcement has no Chat carrier.
    body["thinking"] = json!({"type":"adaptive",
            "block_binding":{"prefix_mismatch_behavior":"error"}});
    assert!(decode_client_request(body.to_string().as_bytes()).is_err());
    let native = morphiecore::protocol::anthropic::messages::Profile::AdaptiveTextTools
        .decode_generation_request(
            body.to_string().as_bytes(),
            morphiecore::semantic::task::generation::LocalScope::ROOT,
            &morphiecore::protocol::native::ReplayTarget::new("synthetic", "m").unwrap(),
        )
        .unwrap();
    let request =
        morphiecore::protocol::GenerationRequestEnvelope::from_generation(native, "m").unwrap();
    assert!(matches!(
        endpoint.encode_request_with_rules(&request),
        Err(morphiecore::protocol::ConversionFailure {
            cause: morphiecore::protocol::ConversionCause::Representation(
                morphiecore::protocol::conversion::generation::RepresentationError::Admission(
                    morphiecore::semantic::task::generation::Feature::ReplayMismatch
                )
            ),
            ..
        })
    ));
    let mut tolerant = request.clone();
    let mut settings = tolerant.task.semantic.settings().clone();
    settings.reasoning.replay_mismatch =
        Some(morphiecore::semantic::task::generation::ReplayMismatch::Drop);
    tolerant.task.semantic = tolerant
        .task
        .semantic
        .with_settings(settings)
        .unwrap()
        .with_message_envelopes(vec![])
        .unwrap();
    let before = tolerant.clone();
    let (wire, rules) = endpoint.encode_request_with_rules(&tolerant).unwrap();
    assert!(!wire.to_string().contains("block_binding"));
    assert!(rules.contains(&"messages.omit-block-binding.v1"));
    assert_eq!(tolerant, before);
}
#[test]
fn unversioned_readable_views_are_closed_and_never_restore_deleted_reasoning() {
    use morphiecore::semantic::{
        task::generation::{Item, ReasoningContent},
        value::Text,
    };
    let standard = adapter(Dialect::Standard);
    let mut unknown = body();
    unknown["choices"][0]["message"]["reasoning_content"] = json!({"not":"text"});
    unknown["choices"][0]["message"]["reasoning"] = json!(7);
    unknown["choices"][0]["message"]["reasoning_details"] = json!({"not":"details"});
    assert!(
        standard
            .decode_response(unknown.to_string().as_bytes())
            .unwrap()
            .semantic
            .items()
            .iter()
            .all(|(_, item)| !matches!(item, Item::Reasoning(_)))
    );
    let client = adapter(Dialect::MorphieCore);
    let alias = client.decode_client_request(
            br#"{"model":"m","messages":[{"role":"assistant","content":"answer","reasoning":"think","reasoning_content":""}]}"#
        ).unwrap();
    assert!(
        matches!(&alias.task.semantic.items()[0].1, Item::Reasoning(r)
            if matches!(&r.parts[0].1, ReasoningContent::Text(t) if t.as_str() == "think"))
    );
    assert!(client.decode_client_request(
            br#"{"model":"m","messages":[{"role":"assistant","content":"answer","reasoning":"think","reasoning_content":"different"}]}"#
        ).is_err());
    let provider = adapter(Dialect::OpenCodeGo);
    let details = json!([{"type":"reasoning.text","text":"think","format":"unknown","index":0}]);
    let mut source = body();
    let message = source["choices"][0]["message"].as_object_mut().unwrap();
    message.insert("reasoning_content".into(), json!(""));
    message.insert("reasoning".into(), json!("think"));
    message.insert("reasoning_details".into(), details.clone());
    source["usage"]["prompt_tokens_details"] =
        json!({"audio_tokens":0,"cached_tokens":0,"cache_write_tokens":0});
    source["usage"]["completion_tokens_details"] = json!({"audio_tokens":0,"reasoning_tokens":1});
    let decoded = provider
        .decode_response(source.to_string().as_bytes())
        .unwrap();
    let client = adapter(Dialect::MorphieCore);
    let wire = client.encode_response(&decoded, &Contract::full()).unwrap();
    assert_eq!(wire["choices"][0]["message"]["reasoning_content"], "think");
    assert_eq!(
        wire["usage"]["completion_tokens_details"]["reasoning_tokens"],
        1
    );
    assert_eq!(
        wire["usage"]["completion_tokens_details"]["audio_tokens"],
        0
    );
    // OpenRouter sends the same view for non-OpenAI models (`z-ai/glm-5.3`).
    assert!(
        adapter(Dialect::OpenRouter)
            .decode_response(source.to_string().as_bytes())
            .is_ok()
    );
    let unrelated = adapter(Dialect::DeepSeek)
        .decode_response(source.to_string().as_bytes())
        .unwrap();
    assert!(
        unrelated
            .semantic
            .items()
            .iter()
            .all(|(_, item)| !matches!(item, Item::Reasoning(_)))
    );
    for (path, value) in [
        ("/choices/0/message/reasoning", json!("disagree")),
        (
            "/choices/0/message/reasoning_details/0/type",
            json!("reasoning.encrypted"),
        ),
        (
            "/choices/0/message/reasoning_details/0/format",
            json!("arbitrary"),
        ),
        ("/choices/0/message/reasoning_details/0/index", json!(1)),
        ("/usage/completion_tokens_details/audio_tokens", json!(-1)),
        (
            "/usage/prompt_tokens_details/audio_tokens",
            json!("unknown"),
        ),
    ] {
        let mut invalid = source.clone();
        *invalid.pointer_mut(path).unwrap() = value;
        assert!(
            provider
                .decode_response(invalid.to_string().as_bytes())
                .is_err()
        );
    }
    let chunk = |delta: Value, finish: Value| json!({"id":"c1","object":"chat.completion.chunk","model":"m","created":1,"choices":[{"index":0,"delta":delta,"finish_reason":finish}]});
    let mut standard_stream = standard.event_decoder();
    standard_stream
        .push(&chunk(
            json!({"role":"assistant","content":"answer",
            "reasoning_content":{},"reasoning":7,"reasoning_details":false}),
            json!("stop"),
        ))
        .unwrap();
    standard_stream.done().unwrap();
    assert!(
        standard_stream
            .materialize()
            .unwrap()
            .semantic
            .items()
            .iter()
            .all(|(_, item)| !matches!(item, Item::Reasoning(_)))
    );
    let mut stream = provider.event_decoder();
    stream
            .push(&chunk(
                json!({"role":"assistant","reasoning":null,"reasoning_content":"","reasoning_details":details}),
                Value::Null,
            ))
            .unwrap();
    stream
        .push(&chunk(json!({"content":"pong"}), Value::Null))
        .unwrap();
    stream.push(&chunk(json!({}), json!("stop"))).unwrap();
    stream.push(&json!({"id":"c1","object":"chat.completion.chunk","model":"m","created":1,"choices":[],"usage":source["usage"]})).unwrap();
    stream.done().unwrap();
    assert_eq!(stream.materialize().unwrap().semantic, decoded.semantic);
    let history = json!({"model":"hy4","messages":[source["choices"][0]["message"],{"role":"user","content":"next"}],"max_tokens":37});
    let mut request = provider
        .decode_request(history.to_string().as_bytes())
        .unwrap();
    request.conversation = Some(
        morphiecore::semantic::context::ConversationContext::conversation("synthetic-conversation")
            .unwrap(),
    );
    let mut items = request.task.semantic.items().to_vec();
    let Item::Reasoning(reason) = &mut items[0].1 else {
        panic!("reasoning owner")
    };
    reason.parts[0].1 = ReasoningContent::Text(Text::new("replacement", "reasoning", 256).unwrap());
    request.task.semantic = request.task.semantic.clone().with_items(items).unwrap();
    let changed = provider
        .encode_request(&request, "hy4", &Contract::full())
        .unwrap();
    assert_eq!(changed["messages"][0]["reasoning"], "replacement");
    assert!(changed["messages"][0].get("reasoning_details").is_none());
    let projected = provider
        .decode_request(changed.to_string().as_bytes())
        .unwrap();
    assert_eq!(projected.task.semantic, request.task.semantic);
    request.task.semantic = request
        .task
        .semantic
        .clone()
        .retain_items(|_, i| !matches!(i, Item::Reasoning(_)))
        .unwrap();
    let deleted = provider
        .encode_request(&request, "hy4", &Contract::full())
        .unwrap();
    assert!(deleted["messages"][0].get("reasoning").is_none());
    assert!(deleted["messages"][0].get("reasoning_details").is_none());
    assert_eq!(deleted["messages"][0]["content"], "pong");
}

#[test]
fn queued_creation_requires_real_progress_and_complete_opening() {
    let provider = GenerationCodec::new(GenerationApi::Responses, Dialect::Bailian, None);
    let source = crate::wire::events(2);
    let mut queued = source[0].clone();
    queued["response"]["status"] = json!("queued");
    let mut standard =
        GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None).event_decoder();
    assert_eq!(
        standard.push(&queued).unwrap(),
        [morphiecore::semantic::task::generation::StreamEvent::Queued]
    );
    assert!(standard.finish().is_err());
    for next in [
        queued.clone(),
        source[0].clone(),
        source[1].clone(),
        source.last().unwrap().clone(),
    ] {
        let mut decoder = provider.event_decoder();
        decoder.push(&queued).unwrap();
        let mut next = next;
        next["sequence_number"] = json!(1);
        assert!(decoder.push(&next).is_err());
        assert!(decoder.finish().is_err());
        assert!(decoder.materialize().is_err());
    }
    let mut missing = queued.clone();
    missing["response"]
        .as_object_mut()
        .unwrap()
        .shift_remove("output");
    assert!(provider.event_decoder().push(&missing).is_err());
    // A plain queued event is not proof of a created event under the alias rule.
    queued["type"] = json!("response.queued");
    let mut decoder = provider.event_decoder();
    decoder.push(&queued).unwrap();
    let mut progress = source[0].clone();
    progress["type"] = json!("response.in_progress");
    progress["sequence_number"] = json!(1);
    assert!(decoder.push(&progress).is_err());
}

fn summary_alias_fixture() -> (Value, Vec<Value>) {
    let item = json!({"id":"r","type":"reasoning","status":null,"content":null,
        "summary":[{"type":"summary_text","text":"brief"}]});
    let mut expected = crate::wire::response(2);
    expected["output"]
        .as_array_mut()
        .unwrap()
        .insert(0, item.clone());
    let base = crate::wire::events(2);
    let mut events = vec![
        base[0].clone(),
        json!({"type":"response.output_item.added","output_index":0,"item":{"id":"r","type":"reasoning","status":null,"summary":[],"content":null}}),
        json!({"type":"response.reasoning_text.delta","output_index":0,"item_id":"r","content_index":0,"delta":"brief"}),
        json!({"type":"response.reasoning_text.done","output_index":0,"item_id":"r","content_index":0,"text":"brief"}),
        json!({"type":"response.output_item.done","output_index":0,"item":item}),
    ];
    for mut event in base.into_iter().skip(1) {
        if event.get("output_index").is_some() {
            event["output_index"] = json!(1);
        }
        if event["type"] == "response.completed" {
            event["response"] = expected.clone();
        }
        events.push(event);
    }
    for (n, event) in events.iter_mut().enumerate() {
        event["sequence_number"] = json!(n);
    }
    (expected, events)
}

#[test]
fn reasoning_event_alias_rejects_incomplete_or_conflicting_summary_snapshots() {
    let provider = GenerationCodec::new(GenerationApi::Responses, Dialect::Bailian, None);
    let (_, source) = summary_alias_fixture();
    let mut missing_done = source.clone();
    missing_done.remove(3);
    let mut wrong_done = source.clone();
    wrong_done[3]["text"] = json!("changed");
    let mut wrong_snapshot = source.clone();
    wrong_snapshot[4]["item"]["summary"][0]["text"] = json!("changed");
    let mut wrong_domain = source.clone();
    wrong_domain[2]["summary_index"] = json!(0);
    for invalid in [missing_done, wrong_done, wrong_snapshot, wrong_domain] {
        let mut decoder = provider.event_decoder();
        assert!(invalid.iter().any(|event| decoder.push(event).is_err()));
        assert!(decoder.finish().is_err());
        assert!(decoder.materialize().is_err());
    }
}

#[test]
fn reasoning_event_alias_cannot_reclassify_content_snapshots_as_summary() {
    use morphiecore::protocol::{CodecError, failure::ShapeReason};
    use morphiecore::semantic::task::generation::Item;
    let (_, mut events) = summary_alias_fixture();
    let content = json!([{"type":"reasoning_text","text":"brief"}]);
    events[4]["item"]["summary"] = json!([]);
    events[4]["item"]["content"] = content.clone();
    let terminal = events.last_mut().unwrap();
    terminal["response"]["output"][0]["summary"] = json!([]);
    terminal["response"]["output"][0]["content"] = content;

    let provider = GenerationCodec::new(GenerationApi::Responses, Dialect::Bailian, None);
    let mut decoder = provider.event_decoder();
    for event in &events[..4] {
        decoder.push(event).unwrap();
    }
    assert!(matches!(
        decoder.push(&events[4]),
        Err(CodecError::Invalid(ShapeReason::SnapshotSummary))
    ));
    assert!(decoder.materialize().is_err());

    let endpoint = morphiecore::topology::catalog::API_KEY_BINDINGS
        .iter()
        .find(|binding| binding.endpoint_prefix == "dashscope-deepseek")
        .unwrap()
        .endpoint(morphiecore::topology::GenerationProtocol::OpenAiResponses)
        .unwrap();
    let morphiecore::adapter::UpstreamAdapter::OpenAi(content_provider) =
        endpoint.adapter().unwrap()
    else {
        panic!("Responses adapter")
    };
    let mut decoder = content_provider.event_decoder();
    for event in events {
        decoder.push(&event).unwrap();
    }
    decoder.finish().unwrap();
    let response = decoder.materialize().unwrap();
    let Item::Reasoning(reasoning) = &response.semantic.items()[0].1 else {
        panic!("reasoning owner")
    };
    assert!(matches!(
        &reasoning.parts[0].1,
        morphiecore::semantic::task::generation::ReasoningContent::Text(text)
            if text.as_str() == "brief"
    ));
    let endpoint = morphiecore::topology::catalog::API_KEY_BINDINGS
        .iter()
        .find(|binding| binding.endpoint_prefix == "dashscope-flash")
        .unwrap()
        .endpoint(morphiecore::topology::GenerationProtocol::OpenAiResponses)
        .unwrap();
    let morphiecore::adapter::UpstreamAdapter::OpenAi(summary_provider) =
        endpoint.adapter().unwrap()
    else {
        panic!("Responses adapter")
    };
    let (_, events) = summary_alias_fixture();
    let mut decoder = summary_provider.event_decoder();
    for event in events {
        decoder.push(&event).unwrap();
    }
    decoder.finish().unwrap();
    let response = decoder.materialize().unwrap();
    let Item::Reasoning(reasoning) = &response.semantic.items()[0].1 else {
        panic!("reasoning owner")
    };
    assert!(matches!(
        &reasoning.parts[0].1,
        morphiecore::semantic::task::generation::ReasoningContent::Summary(text)
            if text.as_str() == "brief"
    ));
}

#[test]
fn inactive_response_state_placeholders_do_not_enable_remote_state() {
    let provider = GenerationCodec::new(GenerationApi::Responses, Dialect::Zhipu, None);
    let mut value = crate::wire::response(2);
    value["previous_response_id"] = json!("");
    value["conversation_id"] = json!("");
    let decoded = provider
        .decode_response(value.to_string().as_bytes())
        .unwrap();
    assert_eq!(decoded.semantic.usage().unwrap().total_tokens, Some(8));
    let encoded = provider
        .encode_response(&decoded, &Contract::full())
        .unwrap();
    assert_eq!(encoded["previous_response_id"], Value::Null);
    assert!(encoded.get("conversation_id").is_none());
    for dialect in [Dialect::Standard, Dialect::MorphieCore, Dialect::Bailian] {
        assert!(
            GenerationCodec::new(GenerationApi::Responses, dialect, None)
                .decode_response(value.to_string().as_bytes())
                .is_err()
        );
    }
    assert!(
        provider
            .decode_request(br#"{"model":"m","input":"hi","previous_response_id":""}"#)
            .is_err()
    );
    for field in ["previous_response_id", "conversation_id", "conversation"] {
        for invalid in [json!("synthetic-state"), json!({}), json!(0), json!(false)] {
            let mut bad = value.clone();
            bad[field] = invalid;
            assert!(
                provider
                    .decode_response(bad.to_string().as_bytes())
                    .is_err()
            );
        }
    }
    for conflict in [false, true] {
        use morphiecore::protocol::{
            openai::responses::sse::ResponsesSseDecoder, stream::SseLimits,
        };
        let mut decoder = ResponsesSseDecoder::with_decoder(
            200,
            "text/event-stream",
            SseLimits::default(),
            provider.event_decoder(),
        )
        .unwrap();
        for mut event in crate::wire::events(2) {
            let terminal = event["type"] == "response.completed";
            if let Some(snapshot) = event.get_mut("response") {
                snapshot["previous_response_id"] = json!("");
                snapshot["conversation_id"] = if terminal && conflict {
                    json!("synthetic-state")
                } else {
                    Value::Null
                };
            }
            let frame = format!("data: {event}\n\n");
            let result = decoder.consume(frame.as_bytes());
            if terminal && conflict {
                assert!(result.is_err());
            } else {
                assert_eq!(result.unwrap().0, frame.len());
            }
        }
        if conflict {
            assert!(decoder.finish().is_err());
            assert!(decoder.materialize().is_err());
        } else {
            decoder.finish().unwrap();
            assert_eq!(decoder.materialize().unwrap().semantic, decoded.semantic);
        }
    }
}

fn terminal_reasoning_fixture() -> (Value, Vec<Value>) {
    let (mut snapshot, source) = summary_alias_fixture();
    snapshot["output"][0] = json!({"id":"r","type":"reasoning","status":"completed","summary":[],"content":[{"text":"brief"}]});
    let mut events = vec![];
    for mut event in source {
        if event["type"] == "response.reasoning_text.done"
            || (event["type"] == "response.output_item.done" && event["output_index"] == 0)
        {
            continue;
        }
        if event["type"] == "response.output_item.added" && event["output_index"] == 0 {
            event["item"] = json!({"id":"r","type":"reasoning","status":"in_progress","summary":[],"content":[]});
        }
        if event["type"] == "response.completed" {
            event["response"] = snapshot.clone();
        }
        event["sequence_number"] = json!(events.len());
        events.push(event);
    }
    (snapshot, events)
}

#[test]
fn terminal_reasoning_snapshot_cannot_recover_missing_text_identity_or_other_lifecycles() {
    let provider = GenerationCodec::new(GenerationApi::Responses, Dialect::Nvidia, None);
    let (_, source) = terminal_reasoning_fixture();
    for (path, value) in [
        ("/response/output/0/id", json!("other")),
        ("/response/output/0/status", json!("in_progress")),
        ("/response/output/0/status", Value::Null),
        ("/response/output/0/content/0/text", json!("changed")),
        (
            "/response/output/0/content/0",
            json!({"text":"brief","unknown":true}),
        ),
    ] {
        let mut invalid = source.clone();
        *invalid.last_mut().unwrap().pointer_mut(path).unwrap() = value;
        let mut decoder = provider.event_decoder();
        assert!(invalid.iter().any(|e| decoder.push(e).is_err()));
        assert!(decoder.push(source.last().unwrap()).is_err());
        assert!(decoder.finish().is_err());
        assert!(decoder.materialize().is_err());
    }
    for removed_type in [
        "response.reasoning_text.delta",
        "response.output_item.done",
        "response.completed",
    ] {
        let mut decoder = provider.event_decoder();
        let failed = source
            .iter()
            .filter(|e| e["type"] != removed_type)
            .any(|e| decoder.push(e).is_err());
        assert!(failed || decoder.finish().is_err());
        assert!(decoder.materialize().is_err());
    }
}

fn billing_view(value: &mut Value) {
    value["billing"] = Value::Null;
    value["frequency_penalty"] = json!(0.25);
    if value.get("usage").is_some_and(Value::is_object) {
        value["usage"]["x_details"] = json!([{
            "x_billing_type":"response_api", "input_tokens":3,"output_tokens":5,"total_tokens":8,
            "prompt_tokens_details":{"cached_tokens":1,"cache_write_tokens":0},
            "output_tokens_details":{"reasoning_tokens":2}
        }]);
    }
}

#[test]
fn response_billing_view_events_match_static_and_conflicts_poison_terminal() {
    let provider = GenerationCodec::new(
        GenerationApi::Responses,
        Dialect::Bailian,
        Some(ReplayOrigin::new("a").unwrap()),
    );
    let mut value = crate::wire::response(2);
    billing_view(&mut value);
    let expected = provider
        .decode_response(value.to_string().as_bytes())
        .unwrap();
    for conflict in [false, true] {
        let mut decoder = provider.event_decoder();
        for mut event in crate::wire::events(2) {
            let terminal = event["type"] == "response.completed";
            if let Some(response) = event.get_mut("response") {
                billing_view(response);
                if terminal && conflict {
                    response["usage"]["x_details"][0]["total_tokens"] = json!(99);
                }
            }
            let result = decoder.push(&event);
            if terminal && conflict {
                assert!(result.is_err());
            } else {
                result.unwrap();
            }
        }
        if conflict {
            assert!(decoder.finish().is_err());
            assert!(decoder.materialize().is_err());
        } else {
            decoder.finish().unwrap();
            let actual = decoder.materialize().unwrap();
            assert_eq!(actual.semantic, expected.semantic);
            let projection = provider
                .encode_response(&actual, &Contract::full())
                .unwrap();
            assert_eq!(projection["frequency_penalty"], 0.25);
            assert!(projection["usage"].get("x_details").is_none());
        }
    }
}

#[test]
fn request_diagnostics_are_source_bound_not_public_facts() {
    let provider = adapter(Dialect::Zhipu);
    let mut value = body();
    value["request_id"] = json!("synthetic-request");
    let decoded = provider
        .decode_response(value.to_string().as_bytes())
        .unwrap();
    assert_eq!(
        provider
            .encode_response(&decoded, &Contract::full())
            .unwrap()["request_id"],
        "synthetic-request"
    );
    let other = GenerationCodec::new(
        GenerationApi::Chat,
        Dialect::Zhipu,
        Some(ReplayOrigin::new("other").unwrap()),
    );
    assert!(
        other
            .encode_response(&decoded, &Contract::full())
            .unwrap()
            .get("request_id")
            .is_none()
    );
    assert!(
        adapter(Dialect::MorphieCore)
            .encode_response(&decoded, &Contract::full())
            .unwrap()
            .get("request_id")
            .is_none()
    );
    let mut edited = decoded.clone();
    let items = edited
        .semantic
        .items()
        .iter()
        .filter(|(_, i)| {
            !matches!(
                i,
                morphiecore::semantic::task::generation::Item::Reasoning(_)
            )
        })
        .cloned()
        .collect();
    edited.semantic = edited.semantic.with_items(items).unwrap();
    assert!(
        provider
            .encode_response(&edited, &Contract::full())
            .unwrap()
            .get("request_id")
            .is_none()
    );
    value["request_id"] = json!({"not":"a string"});
    assert!(
        provider
            .decode_response(value.to_string().as_bytes())
            .is_err()
    );
}
#[test]
fn bailian_created_drift_is_chat_only_and_preserves_identity() {
    assert!(
        !GenerationCodec::new(GenerationApi::Responses, Dialect::Bailian, None)
            .adaptation
            .rules
            .chunk_created_drift
    );
    let first = json!({"id":"r","object":"chat.completion.chunk","model":"synthetic","created":1,
        "choices":[{"index":0,"delta":{"role":"assistant","content":"pong"},"finish_reason":null}]});
    let last = json!({"id":"r","object":"chat.completion.chunk","model":"synthetic","created":2,
        "choices":[{"index":0,"delta":{},"finish_reason":"stop"}]});
    for (dialect, admitted) in [(Dialect::Standard, false), (Dialect::Bailian, true)] {
        let provider = adapter(dialect);
        let mut decoder = provider.event_decoder();
        decoder.push(&first).unwrap();
        assert_eq!(decoder.push(&last).is_ok(), admitted);
        if admitted {
            decoder.done().unwrap();
            let decoded = decoder.materialize().unwrap();
            assert_eq!(
                provider
                    .encode_response(&decoded, &Contract::full())
                    .unwrap()["created"],
                1
            );
        } else {
            assert!(decoder.finish().is_err());
        }
    }
    for (key, old, changed) in [
        ("id", json!("r"), json!("other")),
        ("model", json!("synthetic"), json!("other")),
        ("created", json!(1), json!(-1)),
        ("system_fingerprint", json!("a"), json!("b")),
        ("service_tier", json!("priority"), json!("default")),
    ] {
        let mut a = first.clone();
        let mut b = last.clone();
        a[key] = old;
        b[key] = changed;
        let mut decoder = adapter(Dialect::Bailian).event_decoder();
        decoder.push(&a).unwrap();
        assert!(decoder.push(&b).is_err(), "{key}");
        assert!(decoder.done().is_err());
    }
}

#[test]
fn bailian_schema_alias_is_response_only_and_rejects_ambiguity() {
    use morphiecore::semantic::task::generation::OutputConstraint;
    let provider = GenerationCodec::new(
        GenerationApi::Responses,
        Dialect::Bailian,
        Some(ReplayOrigin::new("synthetic-account").unwrap()),
    );
    let mut source = crate::wire::response(2);
    source["text"] = json!({"format":{
        "type":"json_schema","name":"synthetic","strict":false,"schema_":{"type":"object"}
    }});
    source["frequency_penalty"] = json!(0.0);
    source["presence_penalty"] = Value::Null;
    for key in ["schema_", "schema"] {
        let mut reported = source.clone();
        reported["text"]["format"]
            .as_object_mut()
            .unwrap()
            .remove("schema_");
        reported["text"]["format"][key] = json!({"type":"object"});
        let decoded = provider
            .decode_response(reported.to_string().as_bytes())
            .unwrap();
        let Some(OutputConstraint::JsonSchema {
            name,
            schema,
            strict,
            ..
        }) = &decoded.metadata.context.settings().unwrap().text.format
        else {
            panic!("missing reported schema");
        };
        assert_eq!(name.as_ref().unwrap().as_str(), "synthetic");
        assert_eq!(schema.root(), &json!({"type":"object"}));
        assert_eq!(*strict, Some(false));
        let rendered = provider
            .encode_response(&decoded, &Contract::full())
            .unwrap();
        assert_eq!(
            rendered["output"][0]["content"][0]["text"],
            "{\"ok\":false}"
        );
        assert_eq!(
            rendered["text"]["format"]["schema"],
            json!({"type":"object"})
        );
        assert!(rendered["text"]["format"].get("schema_").is_none());
        assert_eq!(rendered["frequency_penalty"], 0.0);
        assert!(rendered["presence_penalty"].is_null());
        assert!(
            GenerationCodec::new(GenerationApi::Responses, Dialect::Bailian, None)
                .decode_response(reported.to_string().as_bytes())
                .is_err()
        );
    }
    for dialect in [Dialect::Standard, Dialect::MorphieCore] {
        assert!(
            GenerationCodec::new(GenerationApi::Responses, dialect, None)
                .decode_response(source.to_string().as_bytes())
                .is_err()
        );
    }
    assert!(
        provider
            .decode_request(
                json!({
                    "model":"synthetic","input":"hello","text":source["text"]
                })
                .to_string()
                .as_bytes()
            )
            .is_err()
    );
    for value in [
        Value::Null,
        json!({"type":"object"}),
        json!({"type":"string"}),
    ] {
        let mut ambiguous = source.clone();
        ambiguous["text"]["format"]["schema"] = value;
        assert!(
            provider
                .decode_response(ambiguous.to_string().as_bytes())
                .is_err()
        );
        let mut stream = provider.event_decoder();
        ambiguous["status"] = json!("in_progress");
        ambiguous["output"] = json!([]);
        assert!(
            stream
                .push(&json!({
                    "type":"response.created","sequence_number":0,"response":ambiguous
                }))
                .is_err()
        );
    }
    for (key, value) in [
        ("schema_", Value::Null),
        ("schema_", json!(42)),
        ("type", json!("json_object")),
    ] {
        let mut invalid = source.clone();
        invalid["text"]["format"][key] = value;
        assert!(
            provider
                .decode_response(invalid.to_string().as_bytes())
                .is_err()
        );
    }
}
