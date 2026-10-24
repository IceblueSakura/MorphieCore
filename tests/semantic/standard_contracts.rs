//! Source-derived requirements; ignored probes remain unmet, not expected-failure passes.
//! Sources and scope: docs/refs/2026-10-23-stable-protocol-contracts.md.
use morphiecore::protocol::{
    anthropic::messages::client::decode_client_request,
    openai::{Adapter, Dialect, Profile, TargetCaps},
};
use morphiecore::semantic::{task::generation::*, value::Text};
use serde_json::{Value, json};

fn adapter(profile: Profile) -> Adapter {
    Adapter::new(profile, Dialect::Standard, None)
}
fn messages() -> Value {
    json!({"model":"public", "max_tokens":32, "messages":[{"role":"user","content":"Hello"}]})
}

#[test]
fn public_chat_profiles_keep_controls_and_returned_reasoning_explicit() {
    let standard = adapter(Profile::Chat);
    let readable = Adapter::new(Profile::Chat, Dialect::ChatReasoning, None);
    let base = json!({"model":"public","messages":[{"role":"user","content":"go"}],
        "reasoning_effort":"low"});
    for codec in [&standard, &readable] {
        for field in ["enable_thinking", "thinking_budget", "top_k"] {
            for value in [Value::Null, json!(false), json!(32)] {
                let mut body = base.clone();
                body[field] = value;
                assert!(matches!(
                    codec.decode_client_request(body.to_string().as_bytes()),
                    Err(morphiecore::protocol::CodecError::Unsupported(
                        morphiecore::protocol::failure::CarrierReason::UnknownField
                    ))
                ));
            }
        }
        let mut body = base.clone();
        body["unknown_extension"] = json!({"reasoning_content":"not history"});
        // Tool schema properties are not protocol controls.
        body["tools"] = json!([{"type":"function","function":{"name":"inspect",
            "parameters":{"type":"object","properties":{"enable_thinking":{"type":"boolean"}}},
            "strict":false}}]);
        codec
            .decode_client_request(body.to_string().as_bytes())
            .unwrap();
    }
    let mut body = base.clone();
    body["preserve_thinking"] = json!(false);
    assert!(
        standard
            .decode_client_request(body.to_string().as_bytes())
            .is_err()
    );
    let decoded = readable
        .decode_client_request(body.to_string().as_bytes())
        .unwrap();
    assert_eq!(
        decoded.task.semantic.reasoning().read_history,
        morphiecore::semantic::value::Presence::Value(false)
    );
    body["messages"] = json!([
        {"role":"assistant","content":"answer","reasoning_content":"Plan 🧪"},
        {"role":"user","content":"next"}]);
    let decoded = readable
        .decode_client_request(body.to_string().as_bytes())
        .unwrap();
    let [(_, Item::Reasoning(reasoning)), ..] = decoded.task.semantic.items() else {
        panic!("missing reasoning owner");
    };
    assert_eq!(reasoning.parts.len(), 1);
    assert!(
        matches!(&reasoning.parts[0].1, ReasoningContent::Text(text) if text.as_str() == "Plan 🧪")
    );
    assert!(reasoning.replay.is_none());
    for field in ["reasoning", "reasoning_details"] {
        body["messages"][0][field] = Value::Null;
        assert!(
            readable
                .decode_client_request(body.to_string().as_bytes())
                .is_err()
        );
        body["messages"][0].as_object_mut().unwrap().remove(field);
    }
    body.as_object_mut().unwrap().remove("preserve_thinking");
    assert!(
        standard
            .decode_client_request(body.to_string().as_bytes())
            .is_err()
    );
}

#[test]
fn prediction_survives_same_protocol_projection() {
    let adapter = adapter(Profile::Chat);
    let wire = json!({"model":"public", "messages":[{"role":"user","content":"Edit"}],
        "prediction":{"type":"content","content":"hello"}});
    let request = adapter
        .decode_client_request(wire.to_string().as_bytes())
        .unwrap();
    assert_eq!(
        request
            .task
            .semantic
            .settings()
            .prediction
            .as_ref()
            .unwrap()[0]
            .as_str(),
        "hello"
    );
    assert!(request.task.evidence.is_empty());
    let original = request.clone();
    let output = adapter
        .encode_request(&request, "public", &TargetCaps::full())
        .unwrap();
    assert_eq!(output["prediction"], wire["prediction"]);
    let mut restricted = TargetCaps::full();
    restricted.prediction = false;
    let (omitted, rules) = adapter
        .encode_request_with_rules(&request, "public", &restricted)
        .unwrap();
    assert!(omitted.get("prediction").is_none());
    assert!(rules.contains(&"generation.omit-prediction.v1"));
    let (omitted, rules) = self::adapter(Profile::Responses)
        .encode_request_with_rules(&request, "public", &TargetCaps::full())
        .unwrap();
    assert!(omitted.get("prediction").is_none());
    assert!(rules.contains(&"generation.omit-prediction.v1"));
    assert_eq!(request, original);
    let mut settings = request.task.semantic.settings().clone();
    settings.prediction = Some(vec![
        morphiecore::semantic::value::Text::allowing_empty("A", "test", 8).unwrap(),
        morphiecore::semantic::value::Text::allowing_empty("B", "test", 8).unwrap(),
    ]);
    let mut edited = request.clone();
    edited.task.semantic = edited
        .task
        .semantic
        .with_settings(settings.clone())
        .unwrap();
    let (wire, rules) = adapter
        .encode_request_with_rules(&edited, "public", &TargetCaps::full())
        .unwrap();
    assert_eq!(
        wire["prediction"],
        json!({"type":"content","content":[
        {"type":"text","text":"A"},{"type":"text","text":"B"}]})
    );
    assert!(!rules.contains(&"generation.omit-prediction.v1"));
    settings.prediction.as_mut().unwrap().resize(
        morphiecore::semantic::task::generation::MAX_ITEMS + 1,
        morphiecore::semantic::value::Text::allowing_empty("", "test", 8).unwrap(),
    );
    assert!(
        request
            .task
            .semantic
            .clone()
            .with_settings(settings)
            .is_err()
    );
}

#[test]
fn named_chat_participant_survives_same_protocol_projection() {
    let adapter = adapter(Profile::Chat);
    let wire = json!({"model":"public", "messages":[
        {"role":"system","name":"policy","content":"Be precise"},
        {"role":"developer","name":"author","content":[{"type":"text","text":"First"},{"type":"text","text":" second"}]},
        {"role":"user","name":"","content":"Plan"},
        {"role":"assistant","name":"planner","content":null,"tool_calls":[
            {"id":"call-1","type":"function","function":{"name":"lookup","arguments":"{}"}}]},
        {"role":"tool","tool_call_id":"call-1","content":"found"}
    ]});
    let request = adapter
        .decode_client_request(wire.to_string().as_bytes())
        .expect("official participant name");
    assert_eq!(
        request
            .task
            .semantic
            .items()
            .iter()
            .map(|(_, i)| i.participant().map(Text::as_str))
            .collect::<Vec<_>>(),
        [
            Some("policy"),
            Some("author"),
            Some(""),
            Some("planner"),
            None,
            None
        ]
    );
    assert!(matches!(&request.task.semantic.items()[4].1,
        Item::ToolCall(c) if c.name.as_str()=="lookup" && c.call_id.as_str()=="call-1"));
    assert_eq!(
        request.task.semantic.message_envelopes()[0].members(),
        &[
            request.task.semantic.items()[3].0,
            request.task.semantic.items()[4].0
        ]
    );
    let before = request.clone();
    let output = adapter
        .encode_request(&request, "public", &TargetCaps::full())
        .unwrap();
    assert_eq!(output["messages"], wire["messages"]);
    assert!(
        self::adapter(Profile::Responses)
            .encode_request(&request, "public", &TargetCaps::full())
            .is_err()
    );
    assert_eq!(request, before);
    for name in [Value::Null, json!(0), json!(false), json!([]), json!({})] {
        let mut bad = wire.clone();
        bad["messages"][0]["name"] = name;
        assert_eq!(
            adapter
                .decode_client_request(bad.to_string().as_bytes())
                .unwrap_err(),
            morphiecore::protocol::CodecError::Invalid(
                morphiecore::protocol::failure::ShapeReason::InvalidValue
            )
        );
    }
}

#[test]
fn independently_owned_participants_encode_exactly_or_reject_without_a_carrier() {
    use morphiecore::protocol::{
        anthropic::messages::Profile as Messages,
        fidelity::FidelityRecords,
        google::{generate_content, interactions},
        native::ReplayTarget,
        openai::{chat, lowering::lower_request},
    };
    let text = |s| Text::allowing_empty(s, "synthetic", MAX_TEXT_BYTES).unwrap();
    let mut controls = GenerationControls::default();
    controls.max_output_tokens = Some(32);
    let semantic = GenerationRequest::new(
        vec![
            (
                ItemId::new(1),
                Item::Instruction(Instruction {
                    participant: Some(text("policy")),
                    authority: InstructionAuthority::System,
                    parts: vec![Part::new(PartId::new(1), text("exact"))],
                    status: None,
                }),
            ),
            (
                ItemId::new(2),
                Item::Message(Message {
                    participant: Some(text("alice")),
                    role: MessageRole::User,
                    parts: vec![Part::new(
                        PartId::new(2),
                        ContentPart::Text(text("hello").into()),
                    )],
                    status: ItemLifecycle::Completed,
                    phase: None,
                }),
            ),
        ],
        controls,
    )
    .unwrap();
    let before = semantic.clone();
    let fidelity = FidelityRecords::default();
    let target = lower_request(&semantic, &fidelity, Profile::Chat, TargetCaps::full()).unwrap();
    assert_eq!(
        chat::encode_generation(&target).unwrap()["messages"],
        json!([
            {"role":"system","name":"policy","content":"exact"},{"role":"user","name":"alice","content":"hello"}
        ])
    );
    // Test both instruction and ordinary-message paths without an earlier
    // unsupported field or envelope masking participant rejection.
    for owner in [ItemId::new(1), ItemId::new(2)] {
        let named = GenerationRequest::new(
            semantic
                .items()
                .iter()
                .cloned()
                .map(|(id, mut i)| {
                    if id != owner {
                        match &mut i {
                            Item::Instruction(i) => i.participant = None,
                            Item::Message(m) => m.participant = None,
                            _ => {}
                        }
                    }
                    (id, i)
                })
                .collect(),
            semantic.controls().clone(),
        )
        .unwrap()
        .with_message_envelopes(vec![
            MessageEnvelope::new(
                GroupId::new(LocalScope::ROOT, 1),
                MessageEnvelopeRole::User,
                vec![ItemId::new(2)],
            )
            .unwrap(),
        ])
        .unwrap();
        let native = ReplayTarget::new("synthetic", "model").unwrap();
        let Item::Message(mut response_message) = semantic.items()[1].1.clone() else {
            unreachable!()
        };
        response_message.role = MessageRole::Assistant;
        assert_eq!(
            GenerationResponse::new(
                vec![(ItemId::new(1), Item::Message(response_message))],
                Outcome::Completed
            ),
            Err(GenerationError::InvalidResponse)
        );
        assert!(lower_request(&named, &fidelity, Profile::Responses, TargetCaps::full()).is_err());
        assert!(
            Messages::AdaptiveTextTools
                .encode_generation_request(&named, &fidelity, &native)
                .is_err()
        );
        assert!(
            generate_content::encode_generation_request(&named, &fidelity, &native, None).is_err()
        );
        assert!(interactions::encode_generation_request(&named, &fidelity, &native).is_err());
        let unnamed_items = named
            .items()
            .iter()
            .cloned()
            .map(|(id, mut i)| {
                match &mut i {
                    Item::Instruction(i) => i.participant = None,
                    Item::Message(m) => m.participant = None,
                    _ => unreachable!(),
                }
                (id, i)
            })
            .collect();
        let unnamed = GenerationRequest::new(unnamed_items, semantic.controls().clone())
            .unwrap()
            .with_message_envelopes(named.message_envelopes().to_vec())
            .unwrap();
        assert!(
            Messages::AdaptiveTextTools
                .encode_generation_request(&unnamed, &fidelity, &native)
                .is_ok()
        );
        assert!(
            generate_content::encode_generation_request(&unnamed, &fidelity, &native, None).is_ok()
        );
        assert!(interactions::encode_generation_request(&unnamed, &fidelity, &native).is_ok());
    }
    assert_eq!(semantic, before);
}

#[test]
fn responses_omitted_strict_can_reach_native_best_effort_fallback() {
    let adapter = adapter(Profile::Responses);
    let mut wire = json!({"model":"public","input":"Choose", "tools":[{"type":"function","name":"choose",
        "strict":false,"parameters":{"type":"object","properties":{"x":{"oneOf":[{"type":"string"},{"type":"integer"}]}}}}]});
    let explicit_false = adapter
        .decode_client_request(wire.to_string().as_bytes())
        .unwrap();
    adapter
        .encode_request(&explicit_false, "public", &TargetCaps::full())
        .expect("ordinary non-strict schema");
    wire["tools"][0]["strict"] = json!(true);
    let explicit_true = adapter
        .decode_client_request(wire.to_string().as_bytes())
        .unwrap();
    assert!(
        adapter
            .encode_request(&explicit_true, "public", &TargetCaps::full())
            .is_err(),
        "explicit guarantee must not weaken"
    );
    wire["tools"][0].as_object_mut().unwrap().remove("strict");
    let omitted = adapter
        .decode_client_request(wire.to_string().as_bytes())
        .unwrap();
    let output = adapter
        .encode_request(&omitted, "public", &TargetCaps::full())
        .expect("official normalization-with-fallback policy");
    assert_eq!(
        output["tools"][0]["parameters"],
        wire["tools"][0]["parameters"]
    );
    assert!(output["tools"][0].get("strict").is_none());
}

#[test]
fn messages_rejects_removed_sampling_even_at_defaults() {
    use morphiecore::protocol::{CodecError, anthropic::messages::Profile as Messages};
    let baseline = messages();
    decode_client_request(baseline.to_string().as_bytes()).unwrap();
    for field in ["temperature", "top_p", "top_k"] {
        for value in [Value::Null, json!(1), json!(0.5), json!("invalid")] {
            let mut wire = baseline.clone();
            wire[field] = value;
            let bytes = wire.to_string();
            let expected = CodecError::Unsupported(
                morphiecore::protocol::failure::CarrierReason::UnknownField,
            );
            assert_eq!(
                decode_client_request(bytes.as_bytes()).unwrap_err(),
                expected
            );
            assert_eq!(
                Messages::AdaptiveTextTools
                    .decode_request(bytes.as_bytes())
                    .unwrap_err(),
                expected
            );
        }
    }
}

#[test]
fn messages_accepts_direct_only_function_tools() {
    let mut wire = messages();
    wire["tools"] = json!([{"name":"read","input_schema":{"type":"object","properties":{}}}]);
    let omitted = decode_client_request(wire.to_string().as_bytes()).unwrap();
    wire["tools"][0]["allowed_callers"] = json!(["direct"]);
    wire["tools"][0]["type"] = json!("custom");
    let direct = decode_client_request(wire.to_string().as_bytes())
        .expect("direct is not hosted tool execution");
    assert_eq!(direct.task.semantic, omitted.task.semantic);
    let native = morphiecore::protocol::native::ReplayTarget::new("synthetic", "model").unwrap();
    let encoded = morphiecore::protocol::anthropic::messages::TargetCaps::new(
        morphiecore::protocol::anthropic::messages::Profile::AdaptiveTextTools,
        morphiecore::protocol::anthropic::messages::Dialect::Standard,
    )
    .encode_request(&direct, &native)
    .unwrap();
    assert_eq!(
        encoded.value["tools"],
        json!([{"name":"read","input_schema":{"type":"object","properties":{}}}])
    );
    assert!(encoded.evidence.is_empty());
    // Both root tools and by-value additions use the same validated reader.
    for inline in [false, true] {
        let mut body = wire.clone();
        if inline {
            let tool = body.as_object_mut().unwrap().remove("tools").unwrap()[0].clone();
            body["messages"]
                .as_array_mut()
                .unwrap()
                .push(json!({"role":"system","content":[
                {"type":"tool_addition","tool":{"type":"tool_definition","definition":tool}}]}));
        }
        decode_client_request(body.to_string().as_bytes()).unwrap();
        for value in [
            Value::Null,
            json!("direct"),
            json!([]),
            json!(["direct", "direct"]),
            json!([7]),
            json!(["code_execution_20250825"]),
            json!(["code_execution_20260120"]),
            json!(["code_execution_20260521"]),
            json!(["direct", "code_execution_20260521"]),
            json!(["future"]),
        ] {
            let mut bad = body.clone();
            let tool = if inline {
                &mut bad["messages"][1]["content"][0]["tool"]["definition"]
            } else {
                &mut bad["tools"][0]
            };
            tool["allowed_callers"] = value;
            assert_eq!(
                decode_client_request(bad.to_string().as_bytes()).unwrap_err(),
                morphiecore::protocol::CodecError::Unsupported(
                    morphiecore::protocol::failure::CarrierReason::Tools
                )
            );
        }
        let mut bad = body.clone();
        let tool = if inline {
            &mut bad["messages"][1]["content"][0]["tool"]["definition"]
        } else {
            &mut bad["tools"][0]
        };
        tool["type"] = json!("code_execution_20260521");
        assert_eq!(
            decode_client_request(bad.to_string().as_bytes()).unwrap_err(),
            morphiecore::protocol::CodecError::Unsupported(
                morphiecore::protocol::failure::CarrierReason::Tools
            )
        );
    }
}
