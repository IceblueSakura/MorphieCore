//! Source-derived contracts with independent directional expectations.
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
fn legacy_and_current_instruction_roles_have_one_ir_meaning_and_canonical_output() {
    // Pinned OpenAI Chat types call both roles developer-provided instructions.
    // The selected current-standard profile canonicalizes mixed roles in place.
    for profile in [Profile::Chat, Profile::Responses] {
        let source_items = json!([
            {"role":"system","content":"A"},
            {"role":"user","content":"U"},
            {"role":"developer","content":"B"},
        ]);
        let wire = match profile {
            Profile::Chat => json!({"model":"public","messages":source_items}),
            Profile::Responses => json!({"model":"public","input":source_items}),
        };
        let client = adapter(profile);
        let source = client
            .decode_client_request(wire.to_string().as_bytes())
            .unwrap();
        let text = |s| Text::new(s, "test", 32).unwrap();
        let expected = vec![
            (
                ItemId::new(1),
                Item::Instruction(Instruction {
                    participant: None,
                    parts: vec![Part::new(PartId::new(1), text("A"))],
                    status: None,
                }),
            ),
            (
                ItemId::new(2),
                Item::Message(Message {
                    role: MessageRole::User,
                    participant: None,
                    parts: vec![Part::new(
                        PartId::new(2),
                        ContentPart::Text(text("U").into()),
                    )],
                    status: ItemLifecycle::Completed,
                    phase: None,
                }),
            ),
            (
                ItemId::new(3),
                Item::Instruction(Instruction {
                    participant: None,
                    parts: vec![Part::new(PartId::new(3), text("B"))],
                    status: None,
                }),
            ),
        ];
        assert_eq!(source.task.semantic.items(), expected);
        for target in [Profile::Chat, Profile::Responses] {
            let value = adapter(target)
                .encode_request(&source, "m", &TargetCaps::full())
                .unwrap();
            let items = &value[if target == Profile::Chat {
                "messages"
            } else {
                "input"
            }];
            assert_eq!(items[0]["role"], "developer");
            assert_eq!(items[1]["role"], "user");
            assert_eq!(items[2]["role"], "developer");
            assert_eq!(items[0]["content"], "A");
            assert_eq!(items[2]["content"], "B");
        }
        for invalid in [
            json!(null),
            json!(false),
            json!([{"type":"image_url","image_url":{"url":"https://example.invalid/a.png"}}]),
        ] {
            let mut bad = wire.clone();
            bad[if profile == Profile::Chat {
                "messages"
            } else {
                "input"
            }][0]["content"] = invalid;
            assert!(
                client
                    .decode_client_request(bad.to_string().as_bytes())
                    .is_err()
            );
        }
    }
}

#[test]
fn messages_root_forms_share_request_scope_without_becoming_history() {
    use morphiecore::semantic::value::Presence;
    let mut source = messages();
    source["system"] = json!("R");
    let string = decode_client_request(source.to_string().as_bytes()).unwrap();
    source["system"] = json!([{"type":"text","text":"R"}]);
    let block = decode_client_request(source.to_string().as_bytes()).unwrap();
    assert_eq!(string.task.semantic, block.task.semantic);
    assert_eq!(block.task.semantic.items().len(), 1);
    source["system"] = json!([]);
    let empty = decode_client_request(source.to_string().as_bytes()).unwrap();
    source["system"] = json!("");
    assert_eq!(
        empty.task.semantic,
        decode_client_request(source.to_string().as_bytes())
            .unwrap()
            .task
            .semantic
    );
    assert_eq!(
        block.task.semantic.instructions(),
        &Presence::Value(RootInstructions::from(Text::new("R", "test", 32).unwrap()))
    );
    for request in [&string, &block] {
        let chat = adapter(Profile::Chat)
            .encode_request(request, "m", &TargetCaps::full())
            .unwrap();
        assert_eq!(
            chat["messages"][0],
            json!({"role":"developer","content":"R"})
        );
        let response = adapter(Profile::Responses)
            .encode_request(request, "m", &TargetCaps::full())
            .unwrap();
        assert_eq!(response["instructions"], "R");
        assert_eq!(response["input"].as_array().unwrap().len(), 1);
    }
    source["system"] = json!([{"type":"text","text":"R"},{"type":"text","text":"S"}]);
    let multipart = decode_client_request(source.to_string().as_bytes()).unwrap();
    let chat = adapter(Profile::Chat)
        .encode_request(&multipart, "m", &TargetCaps::full())
        .unwrap();
    assert_eq!(
        chat["messages"][0],
        json!({"role":"developer","content":[
        {"type":"text","text":"R"},{"type":"text","text":"S"}]})
    );
    let response = adapter(Profile::Responses)
        .encode_request(&multipart, "m", &TargetCaps::full())
        .unwrap();
    assert!(response.get("instructions").is_none());
    assert_eq!(
        response["input"][0],
        json!({"role":"developer","content":[
        {"type":"input_text","text":"R"},{"type":"input_text","text":"S"}]})
    );
    assert_eq!(multipart.task.semantic.items().len(), 1);
    assert_eq!(
        multipart
            .task
            .semantic
            .instructions()
            .value()
            .unwrap()
            .parts
            .len(),
        2
    );
    source["system"][0]["cache_control"] = json!({"type":"ephemeral","ttl":"1h"});
    let cached = decode_client_request(source.to_string().as_bytes()).unwrap();
    assert!(GenerationRequirements::derive(&cached.task.semantic).explicit_cache);
    assert!(
        adapter(Profile::Chat)
            .encode_request(&cached, "m", &TargetCaps::full())
            .is_err()
    );
    assert!(
        adapter(Profile::Responses)
            .encode_request(&cached, "m", &TargetCaps::full())
            .is_err()
    );
    let mut boundary = block.clone();
    let mut settings = boundary.task.semantic.settings().clone();
    let Presence::Value(root) = &mut settings.instructions else {
        unreachable!()
    };
    root.parts[0].cache = Some(PrefixCacheBoundary::RequestLifetime);
    boundary.task.semantic = boundary.task.semantic.with_settings(settings).unwrap();
    let response = adapter(Profile::Responses)
        .encode_request(&boundary, "m", &TargetCaps::full())
        .unwrap();
    assert!(response.get("instructions").is_none());
    assert_eq!(
        response["input"][0],
        json!({"role":"developer","content":[{
        "type":"input_text","text":"R","prompt_cache_breakpoint":{"mode":"explicit"}}]})
    );
    assert_eq!(boundary.task.semantic.items().len(), 1);
}

#[test]
fn managed_continuation_replaces_root_but_retains_ordered_application_instructions() {
    let client = adapter(Profile::Responses);
    let output = client.decode_response(br#"{"id":"r","object":"response","created_at":1,
        "model":"public","status":"completed","output":[{"type":"message","id":"a","role":"assistant",
        "status":"completed","content":[{"type":"output_text","text":"ok","annotations":[]}]}]}"#).unwrap();
    let sources = [
        client
            .decode_client_request(
                br#"{"model":"public","instructions":"R",
            "input":[{"role":"system","content":"P"},{"role":"user","content":"go"}]}"#,
            )
            .unwrap(),
        decode_client_request(
            br#"{"model":"public","max_tokens":32,"system":[{"type":"text","text":"R"}],
            "messages":[{"role":"user","content":"go"},{"role":"system","content":"P"}]}"#,
        )
        .unwrap(),
    ];
    for source in sources {
        for root in [None, Some(json!(null)), Some(json!("")), Some(json!("new"))] {
            let mut next = json!({"model":"public","input":"again"});
            if let Some(value) = &root {
                next["instructions"] = value.clone();
            }
            let combined = client
                .decode_client_history(next.to_string().as_bytes(), Some((&source.task, &output)))
                .unwrap();
            let value = client
                .encode_request(&combined, "m", &TargetCaps::full())
                .unwrap();
            assert_eq!(value.get("instructions"), root.as_ref());
            let instructions: Vec<_> = value["input"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|item| item["role"] == "developer")
                .collect();
            assert_eq!(instructions.len(), 1);
            assert_eq!(instructions[0]["content"], "P");
            assert_eq!(combined.task.semantic.items().len(), 4);
        }
    }
    let text = |s| Text::allowing_empty(s, "test", MAX_TEXT_BYTES).unwrap();
    for parts in [
        vec![],
        vec![
            InstructionText {
                text: text(""),
                cache: None
            };
            MAX_ITEMS + 1
        ],
        vec![
            InstructionText {
                text: text(&"x".repeat(MAX_TEXT_BYTES)),
                cache: None
            };
            MAX_TOTAL_BYTES / MAX_TEXT_BYTES + 1
        ],
    ] {
        assert!(
            GenerationSettings {
                instructions: morphiecore::semantic::value::Presence::Value(RootInstructions {
                    parts
                }),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }
}

#[test]
fn responses_stream_options_follow_directional_field_policy() {
    let adapter = adapter(Profile::Responses);
    let mut wire = json!({"model":"public","input":"go","stream":true,
        "stream_options":{"include_obfuscation":false}});
    let original = adapter
        .decode_client_request(wire.to_string().as_bytes())
        .unwrap();
    wire["stream_options"]["future_option"] = json!({"nested":true});
    let decoded = adapter
        .decode_client_request(wire.to_string().as_bytes())
        .unwrap();
    assert_eq!(decoded.task, original.task);
    assert_eq!(decoded.delivery, original.delivery);
    assert!(adapter.decode_request(wire.to_string().as_bytes()).is_err());
    for invalid in [json!(1), json!("false"), json!([])] {
        wire["stream_options"]["include_obfuscation"] = invalid;
        assert!(
            adapter
                .decode_client_request(wire.to_string().as_bytes())
                .is_err()
        );
    }
    wire["stream_options"] = json!({"include_obfuscation":null});
    assert!(
        adapter
            .decode_client_request(wire.to_string().as_bytes())
            .is_err()
    );
    wire["stream_options"] = json!(false);
    assert!(
        adapter
            .decode_client_request(wire.to_string().as_bytes())
            .is_err()
    );
}

#[test]
fn chat_function_contract_distinguishes_omission_null_and_target_name_grammar() {
    let chat = adapter(Profile::Chat);
    let responses = adapter(Profile::Responses);
    let base = json!({"model":"public","messages":[{"role":"user","content":"go","name":"participant with spaces"}],
        "tools":[{"type":"function","function":{"name":"lookup","strict":null}}]});
    chat.decode_client_request(base.to_string().as_bytes())
        .unwrap();
    for field in ["description", "parameters"] {
        let mut bad = base.clone();
        bad["tools"][0]["function"][field] = Value::Null;
        assert!(
            chat.decode_client_request(bad.to_string().as_bytes())
                .is_err(),
            "{field}"
        );
        assert!(
            chat.decode_request(bad.to_string().as_bytes()).is_err(),
            "{field}"
        );
    }
    let mut custom = base.clone();
    custom["tools"] = json!([{"type":"custom","custom":{"name":"lookup","description":null}}]);
    assert!(
        chat.decode_client_request(custom.to_string().as_bytes())
            .is_err()
    );
    let nullable = json!({"model":"public","input":"go","tools":[{
        "type":"function","name":"lookup","description":null,"parameters":null,"strict":false}]});
    responses
        .decode_client_request(nullable.to_string().as_bytes())
        .unwrap();
    for name in [
        "bad name".to_owned(),
        "a".repeat(65),
        "工具".to_owned(),
        "a".repeat(64),
    ] {
        let valid = name == "a".repeat(64);
        let mut wire = base.clone();
        wire["tools"][0]["function"]["name"] = json!(name);
        assert_eq!(
            chat.decode_client_request(wire.to_string().as_bytes())
                .is_ok(),
            valid
        );
        // Build the target input independently from the broader Responses carrier.
        let mut native = nullable.clone();
        native["tools"][0]["name"] = json!(name);
        let request = responses
            .decode_client_request(native.to_string().as_bytes())
            .unwrap();
        assert_eq!(
            chat.encode_request(&request, "public", &TargetCaps::full())
                .is_ok(),
            valid
        );
    }
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
    let mut expected_messages = wire["messages"].clone();
    expected_messages[0]["role"] = json!("developer");
    assert_eq!(output["messages"], expected_messages);
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
            {"role":"developer","name":"policy","content":"exact"},{"role":"user","name":"alice","content":"hello"}
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
