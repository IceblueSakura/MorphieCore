use morphiecore::{
    protocol::{
        CodecError, DecodedResponse,
        anthropic::messages::client::{
            decode_client_request, encode_client_response, sse::ClientEventEncoder,
        },
        openai::{Dialect, GenerationApi, GenerationCodec},
    },
    semantic::{task::generation::*, value::Presence},
};
use serde_json::json;

/// Beta carriers remain available to trusted native bindings, not stable clients.
fn native_request(body: &serde_json::Value) -> morphiecore::protocol::GenerationRequestEnvelope {
    use morphiecore::protocol::{
        GenerationRequestEnvelope, anthropic::messages::Profile, native::ReplayTarget,
    };
    assert!(decode_client_request(body.to_string().as_bytes()).is_err());
    let decoded = Profile::AdaptiveTextTools
        .decode_generation_request(
            body.to_string().as_bytes(),
            LocalScope::ROOT,
            &ReplayTarget::new("synthetic", "m").unwrap(),
        )
        .unwrap();
    GenerationRequestEnvelope::from_generation(decoded, "m").unwrap()
}

fn response() -> DecodedResponse {
    // Orthogonal content/usage fixtures carry an explicit, representable terminal.
    DecodedResponse {
        evidence: Default::default(),
        semantic: GenerationResponse::new(
            vec![(
                ItemId::new(1),
                Item::Message(Message {
                    participant: None,
                    role: MessageRole::Assistant,
                    status: ItemLifecycle::Completed,
                    phase: None,
                    parts: vec![Part::new(
                        PartId::new(1),
                        ContentPart::Text(
                            morphiecore::semantic::value::Text::new("answer", "fixture", 64)
                                .unwrap()
                                .into(),
                        ),
                    )],
                }),
            )],
            Outcome::Completed,
        )
        .unwrap()
        .with_progress(InteractionProgress::TurnFinished)
        .unwrap()
        .with_details(TerminalDetails {
            completion: Some(CompletionCause::Natural),
            ..Default::default()
        })
        .unwrap(),
        fidelity: Default::default(),
        metadata: morphiecore::protocol::ResponseMetadata {
            delivery: None,
            id: Some("r".into()),
            model: "synthetic".into(),
            storage: None,
            created: None,
            context: Default::default(),
            instruction_fidelity: Default::default(),
        },
    }
}
#[test]
fn unsigned_reasoning_delivery_rejects_without_changing_the_source() {
    use morphiecore::{adapter::ClientAdapter, semantic::value::Text};
    let mut source = response();
    let mut items = vec![(
        ItemId::new(2),
        Item::Reasoning(ReasoningItem {
            parts: vec![(
                PartId::new(10),
                ReasoningContent::Summary(Text::new("plan", "test", 64).unwrap()),
            )],
            status: ItemLifecycle::Completed,
            replay: None,
        }),
    )];
    items.extend(source.semantic.items().iter().cloned());
    source.semantic = source.semantic.with_items(items).unwrap();
    let original = source.clone();
    assert!(ClientAdapter::Messages.snapshot(source.clone()).is_err());
    assert_eq!(
        encode_client_response(&source, "m").unwrap_err(),
        CodecError::Unsupported(morphiecore::protocol::failure::CarrierReason::Replay)
    );
    assert_eq!(source, original);
}

fn terminal() -> StreamEvent {
    StreamEvent::Terminal {
        terminal: StreamTerminal::Completed,
        details: TerminalDetails {
            completion: Some(CompletionCause::Natural),
            ..Default::default()
        },
    }
}
#[test]
fn content_cache_null_defaults_do_not_erase_business_values_or_invent_boundaries() {
    use morphiecore::protocol::{anthropic::messages::Profile as Messages, native::ReplayTarget};
    let baseline = json!({"model":"m","max_tokens":32,
        "system":[{"type":"text","text":"system"}],
        "messages":[
            {"role":"user","content":[{"type":"text","text":"go"},
                {"type":"image","source":{"type":"base64","media_type":"image/png","data":"AQID"}},
                {"type":"document","source":{"type":"text","media_type":"text/plain","data":"notes"}}]},
            {"role":"assistant","content":[{"type":"tool_use","id":"c","name":"lookup",
                "input":{"cache_control":null,"is_error":null}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"c","is_error":false,"content":[
                {"type":"text","text":"result"},
                {"type":"image","source":{"type":"base64","media_type":"image/png","data":"AQID"}}]}]}]});
    let mut nullable = baseline.clone();
    nullable["system"][0]["cache_control"] = json!(null);
    for message in nullable["messages"].as_array_mut().unwrap() {
        for block in message["content"].as_array_mut().unwrap() {
            block["cache_control"] = json!(null);
            if block["type"] == "tool_result" {
                for part in block["content"].as_array_mut().unwrap() {
                    part["cache_control"] = json!(null);
                }
            }
        }
    }
    let decoded = decode_client_request(nullable.to_string().as_bytes()).unwrap();
    let expected = decode_client_request(baseline.to_string().as_bytes()).unwrap();
    assert_eq!(decoded.task.semantic, expected.task.semantic);
    assert!(
        decoded
            .task
            .semantic
            .items()
            .iter()
            .all(|(_, item)| !item.has_cache_breakpoints())
    );
    let profile = Messages::AdaptiveTextTools;
    let target = ReplayTarget::new("synthetic", "m").unwrap();
    let native = profile
        .decode_generation_request(nullable.to_string().as_bytes(), LocalScope::ROOT, &target)
        .unwrap();
    assert!(
        native
            .semantic
            .items()
            .iter()
            .all(|(_, item)| !item.has_cache_breakpoints())
    );
    for marker in [
        json!({"type":"ephemeral","ttl":null}),
        json!(false),
        json!({"type":"unknown"}),
    ] {
        let mut invalid = baseline.clone();
        invalid["messages"][0]["content"][0]["cache_control"] = marker;
        assert!(decode_client_request(invalid.to_string().as_bytes()).is_err());
        assert!(
            profile
                .decode_request(invalid.to_string().as_bytes())
                .is_err()
        );
    }
}

#[test]
fn nonnullable_top_level_controls_and_empty_ordinary_messages_reject_before_normalization() {
    use morphiecore::protocol::anthropic::messages::Profile as Messages;
    let profile = Messages::AdaptiveTextTools;
    let baseline =
        json!({"model":"m","max_tokens":4096,"messages":[{"role":"user","content":"go"}]});
    for (field, value) in [
        ("system", json!("system")),
        ("tools", json!([])),
        ("tool_choice", json!({"type":"auto"})),
        ("thinking", json!({"type":"adaptive"})),
        ("output_config", json!({"effort":"medium"})),
        ("stream", json!(false)),
    ] {
        let mut request = baseline.clone();
        request[field] = value;
        decode_client_request(request.to_string().as_bytes()).unwrap();
        profile
            .decode_request(request.to_string().as_bytes())
            .unwrap();
        request[field] = json!(null);
        assert_eq!(
            decode_client_request(request.to_string().as_bytes()).unwrap_err(),
            CodecError::Invalid(morphiecore::protocol::failure::ShapeReason::InvalidValue)
        );
        assert_eq!(
            profile
                .decode_request(request.to_string().as_bytes())
                .unwrap_err(),
            CodecError::Invalid(morphiecore::protocol::failure::ShapeReason::InvalidValue)
        );
    }
    for content in [json!([]), json!(""), json!([{"type":"text","text":""}])] {
        let mut request = baseline.clone();
        request["messages"] = json!([
            {"role":"user","content":"A"},
            {"role":"assistant","content":content},
            {"role":"user","content":"B"}]);
        assert_eq!(
            decode_client_request(request.to_string().as_bytes()).unwrap_err(),
            CodecError::Invalid(morphiecore::protocol::failure::ShapeReason::InvalidValue)
        );
        assert_eq!(
            profile
                .decode_request(request.to_string().as_bytes())
                .unwrap_err(),
            CodecError::Invalid(morphiecore::protocol::failure::ShapeReason::InvalidValue)
        );
    }
    decode_client_request(baseline.to_string().as_bytes()).unwrap();
}

#[test]
fn known_boolean_and_unresolved_configuration_nulls_cannot_disappear_at_intake() {
    use morphiecore::protocol::anthropic::messages::Profile as Messages;
    let base = json!({"model":"m","max_tokens":32,
        "messages":[{"role":"user","content":"go"},
            {"role":"assistant","content":[{"type":"tool_use","id":"c","name":"lookup","input":{}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"c","content":"done"}]}]});
    let profile = Messages::AdaptiveTextTools;
    for flag in [
        None,
        Some(json!(false)),
        Some(json!(true)),
        Some(json!(null)),
    ] {
        for error_flag in [false, true] {
            let mut body = base.clone();
            body["tool_choice"] = json!({"type":"auto"});
            if let Some(flag) = &flag {
                if error_flag {
                    body["messages"][2]["content"][0]["is_error"] = flag.clone();
                } else {
                    body["tool_choice"]["disable_parallel_tool_use"] = flag.clone();
                }
            }
            let decoded = decode_client_request(body.to_string().as_bytes());
            let native = profile.decode_request(body.to_string().as_bytes());
            if flag.as_ref().is_some_and(serde_json::Value::is_null) {
                assert_eq!(
                    decoded.unwrap_err(),
                    CodecError::Invalid(morphiecore::protocol::failure::ShapeReason::InvalidValue)
                );
                assert_eq!(
                    native.unwrap_err(),
                    CodecError::Invalid(morphiecore::protocol::failure::ShapeReason::InvalidValue)
                );
            } else {
                let decoded = decoded.unwrap();
                native.unwrap();
                if error_flag {
                    let Item::ToolResult(result) = &decoded.task.semantic.items()[2].1 else {
                        panic!("result")
                    };
                    assert_eq!(
                        result.is_error,
                        flag.as_ref().and_then(serde_json::Value::as_bool)
                    );
                } else {
                    assert_eq!(
                        decoded.task.semantic.settings().parallel_tool_calls,
                        flag.as_ref()
                            .and_then(serde_json::Value::as_bool)
                            .map(|v| !v)
                    );
                }
            }
        }
    }
    for field in ["effort", "format"] {
        let mut body = base.clone();
        body["output_config"] = json!({field:null});
        let expected =
            CodecError::Unsupported(morphiecore::protocol::failure::CarrierReason::Unavailable);
        assert_eq!(
            decode_client_request(body.to_string().as_bytes()).unwrap_err(),
            expected
        );
        assert_eq!(
            profile
                .decode_request(body.to_string().as_bytes())
                .unwrap_err(),
            expected
        );
    }
}

#[test]
fn leading_instructions_compose_once_and_effort_boundaries_split_same_role_history() {
    use morphiecore::protocol::{
        anthropic::messages::{self, Carriers},
        native::ReplayTarget,
    };
    let text =
        |s: &str| morphiecore::semantic::value::Text::allowing_empty(s, "fixture", 128).unwrap();
    let target = ReplayTarget::new("synthetic", "m").unwrap();
    let profile = messages::Profile::AdaptiveTextTools;
    let mut caps = messages::TargetContract::new(profile, messages::Dialect::Standard);
    caps.semantics = profile.representation_contract();
    caps.carriers = Carriers::all();
    for root in [None, Some(text("root"))] {
        let mut settings = GenerationSettings::default();
        settings.controls.max_output_tokens = Some(32);
        settings.instructions = root
            .clone()
            .map_or(Presence::Absent, |text| Presence::Value(text.into()));
        let mut items = vec![];
        for (index, content) in ["A", "B"].into_iter().enumerate() {
            items.push((
                ItemId::new(index as u64 + 1),
                Item::Instruction(Instruction {
                    participant: None,
                    status: None,
                    parts: vec![Part {
                        id: PartId::new(index as u64 + 1),
                        replay: None,
                        content: text(content),
                        cache: (index == 0)
                            .then_some(PrefixCacheBoundary::Ttl(PrefixCacheTtl::OneHour)),
                    }],
                }),
            ));
        }
        items.push((
            ItemId::new(3),
            Item::Message(Message {
                participant: None,
                role: MessageRole::User,
                status: ItemLifecycle::Completed,
                phase: None,
                parts: vec![Part::new(
                    PartId::new(3),
                    ContentPart::Text(text("go").into()),
                )],
            }),
        ));
        let semantic = GenerationRequest::from_settings(items, settings).unwrap();
        let request = morphiecore::protocol::GenerationRequestEnvelope::from_generation(
            morphiecore::protocol::DecodedRequest {
                evidence: Default::default(),
                semantic,
                fidelity: Default::default(),
            },
            "m",
        )
        .unwrap();
        let original = request.clone();
        let projection = caps.encode_request(&request, &target).unwrap();
        let mut expected = vec![];
        if root.is_some() {
            expected.push(json!({"type":"text","text":"root"}));
        }
        expected.extend([
            json!({"type":"text","text":"A","cache_control":{"type":"ephemeral","ttl":"1h"}}),
            json!({"type":"text","text":"B"}),
        ]);
        assert_eq!(projection.value["system"], json!(expected));
        assert_eq!(
            projection.value["messages"],
            json!([{"role":"user","content":[{"type":"text","text":"go"}]}])
        );
        assert!(projection.evidence.is_empty());
        assert_eq!(request, original);
    }
    let body = json!({"model":"m","max_tokens":32,"messages":[
        {"role":"user","content":"first"},
        {"role":"system","content":[],"output_config":{"effort":"low"}},
        {"role":"user","content":"second"}]});
    let request = native_request(&body);
    let original = request.clone();
    let projection = caps.encode_request(&request, &target).unwrap();
    assert_eq!(
        projection.value["messages"],
        json!([
        {"role":"user","content":[{"type":"text","text":"first"}]},
        {"role":"system","content":[],"output_config":{"effort":"low"}},
        {"role":"user","content":[{"type":"text","text":"second"}]}])
    );
    assert!(projection.evidence.is_empty());
    caps.carriers.message_effort = false;
    let flattened = caps.encode_request(&request, &target).unwrap();
    assert_eq!(flattened.value["output_config"], json!({"effort":"low"}));
    assert!(
        flattened
            .evidence
            .contains_rule(messages::RequestRule::FlattenMessageEffort.name())
    );
    assert_eq!(request, original);
    let body = json!({"model":"m","max_tokens":32,"messages":[
        {"role":"user","content":"A"},{"role":"user","content":"B"},{"role":"user","content":"C"}]});
    let mut partial = decode_client_request(body.to_string().as_bytes()).unwrap();
    partial.task.semantic = partial
        .task
        .semantic
        .clone()
        .with_message_envelopes(vec![
            MessageEnvelope::new(
                GroupId::new(LocalScope::ROOT, 7),
                MessageEnvelopeRole::User,
                vec![ItemId::new(1)],
            )
            .unwrap(),
            MessageEnvelope::new(
                GroupId::new(LocalScope::ROOT, 8),
                MessageEnvelopeRole::User,
                vec![ItemId::new(2)],
            )
            .unwrap(),
        ])
        .unwrap();
    let original = partial.clone();
    let encoded = caps.encode_request(&partial, &target).unwrap();
    assert_eq!(
        encoded.value["messages"],
        json!([
        {"role":"user","content":[{"type":"text","text":"A"}]},
        {"role":"user","content":[{"type":"text","text":"B"}]},
        {"role":"user","content":[{"type":"text","text":"C"}]}])
    );
    assert_eq!(partial, original);
    partial.task.semantic = partial
        .task
        .semantic
        .clone()
        .with_message_envelopes(vec![
            MessageEnvelope::new(
                GroupId::new(LocalScope::ROOT, 7),
                MessageEnvelopeRole::User,
                vec![ItemId::new(1), ItemId::new(3)],
            )
            .unwrap(),
        ])
        .unwrap();
    assert!(matches!(caps.encode_request(&partial, &target),
        Err(morphiecore::protocol::ConversionFailure { cause: morphiecore::protocol::ConversionCause::Codec(
            morphiecore::protocol::CodecError::Unsupported(reason)), .. })
            if matches!(reason,morphiecore::protocol::failure::CarrierReason::Unavailable)));
}

#[test]
fn nullable_tool_defaults_normalize_without_disabling_eager_input_or_inventing_cache() {
    use morphiecore::protocol::{anthropic::messages::Profile as Messages, native::ReplayTarget};
    let profile = Messages::AdaptiveTextTools;
    let target = ReplayTarget::new("synthetic", "m").unwrap();
    let text = |s: &str| morphiecore::semantic::value::Text::new(s, "fixture", 128).unwrap();
    for (eager, expected_eager, cache, expected_cache) in [
        (None, None, None, None),
        (Some(json!(null)), None, Some(json!(null)), None),
        (Some(json!(false)), Some(false), Some(json!(null)), None),
        (Some(json!(true)), Some(true), None, None),
        (
            Some(json!(null)),
            None,
            Some(json!({"type":"ephemeral"})),
            Some(PrefixCacheTtl::FiveMinutes),
        ),
    ] {
        let mut definition = json!({"name":"lookup","input_schema":{"type":"object"}});
        if let Some(eager) = eager {
            definition["eager_input_streaming"] = eager;
        }
        if let Some(cache) = cache {
            definition["cache_control"] = cache;
        }
        let mut later = definition.clone();
        later["name"] = json!("later");
        let body = json!({"model":"m","max_tokens":32,"tools":[definition],
            "messages":[{"role":"user","content":"go"},{"role":"system","content":[
                {"type":"tool_addition","tool":{"type":"tool_definition","definition":later}}]}]});
        let decoded = native_request(&body);
        let mut stable = body.clone();
        stable["messages"].as_array_mut().unwrap().truncate(1);
        let stable = decode_client_request(stable.to_string().as_bytes()).unwrap();
        assert_eq!(stable.task.semantic.tools(), decoded.task.semantic.tools());
        let native = profile
            .decode_generation_request(body.to_string().as_bytes(), LocalScope::ROOT, &target)
            .unwrap();
        for semantic in [&decoded.task.semantic, &native.semantic] {
            let ToolDefinition::Function(root) = &semantic.tools()[0] else {
                panic!("function");
            };
            let Item::ToolChange(ToolChange {
                action: ToolChangeAction::AddDefinition(later),
                ..
            }) = &semantic.items()[1].1
            else {
                panic!("tool addition");
            };
            for tool in [root, later.as_ref()] {
                assert_eq!(tool.eager_input, expected_eager);
                assert_eq!(tool.cache, expected_cache);
            }
        }

        // Independent semantic construction: null never becomes Some(false) or a TTL.
        let root = FunctionTool {
            name: text("lookup"),
            description: None,
            parameters: Some(json!({"type":"object"}).into()),
            input_examples: None,
            strict: FunctionStrictness::Omitted(StrictDefault::NonStrict),
            output_schema: None,
            dispatch: Default::default(),
            cache: expected_cache,
            eager_input: expected_eager,
        };
        let mut controls = GenerationControls::default();
        controls.max_output_tokens = Some(32);
        let request = GenerationRequest::new(
            vec![
                (
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
                ),
                (
                    ItemId::new(2),
                    Item::ToolChange(ToolChange {
                        action: ToolChangeAction::AddDefinition(Box::new(FunctionTool {
                            name: text("later"),
                            ..root.clone()
                        })),
                        cache: None,
                    }),
                ),
            ],
            controls,
        )
        .unwrap()
        .with_tools(vec![ToolDefinition::Function(root)], ToolChoice::Auto)
        .unwrap()
        .with_message_envelopes(vec![
            MessageEnvelope::new(
                GroupId::new(LocalScope::ROOT, 1),
                MessageEnvelopeRole::User,
                vec![ItemId::new(1)],
            )
            .unwrap(),
        ])
        .unwrap();
        let projection = profile
            .encode_generation_request(&request, &Default::default(), &target)
            .unwrap();
        assert!(projection.evidence.is_empty());
        let mut expected = json!({"name":"lookup","input_schema":{"type":"object"}});
        if let Some(eager) = expected_eager {
            expected["eager_input_streaming"] = json!(eager);
        }
        if expected_cache.is_some() {
            expected["cache_control"] = json!({"type":"ephemeral"});
        }
        assert_eq!(projection.value["tools"], json!([expected]));
        expected["name"] = json!("later");
        assert_eq!(
            projection.value["messages"][1]["content"],
            json!([{"type":"tool_addition","tool":{"type":"tool_definition","definition":expected}}])
        );
    }
    for inline in [false, true] {
        for (field, value) in [
            ("eager_input_streaming", json!("false")),
            ("eager_input_streaming", json!(0)),
            ("cache_control", json!(false)),
            ("description", json!(null)),
            ("strict", json!(null)),
            ("defer_loading", json!(null)),
            ("input_examples", json!(null)),
        ] {
            let mut definition = json!({"name":"lookup","input_schema":{"type":"object"}});
            definition[field] = value;
            let body = if inline {
                json!({"model":"m","max_tokens":32,"messages":[{"role":"user","content":"go"},
                    {"role":"system","content":[{"type":"tool_addition",
                        "tool":{"type":"tool_definition","definition":definition}}]}]})
            } else {
                json!({"model":"m","max_tokens":32,"tools":[definition],
                    "messages":[{"role":"user","content":"go"}]})
            };
            assert!(
                decode_client_request(body.to_string().as_bytes()).is_err(),
                "{field}"
            );
        }
    }
}
#[test]
fn messages_tool_examples_keep_order_presence_and_tool_change_owners() {
    use morphiecore::protocol::{anthropic::messages::Profile as Messages, native::ReplayTarget};
    let profile = Messages::AdaptiveTextTools;
    let examples = json!([{"n":9007199254740993_u64,"sequence":["b","a"]},{"n":"not-an-integer"}]);
    let schema = json!({"type":"object","properties":{"n":{"type":"integer"}}});
    let definition = json!({"name":"lookup","strict":false,"input_schema":schema,
        "input_examples":examples});
    let body = json!({"model":"m","max_tokens":32,"tools":[definition],
        "messages":[{"role":"user","content":[{"type":"text","text":"go"}]},
        {"role":"system","content":[{"type":"tool_addition","tool":{"type":"tool_definition",
            "definition":{"name":"later","strict":false,"input_schema":schema,"input_examples":[]}}}]}]});
    let decoded = native_request(&body);
    let ToolDefinition::Function(tool) = &decoded.task.semantic.tools()[0] else {
        panic!("function")
    };
    assert_eq!(tool.parameters.as_ref().unwrap().root(), &schema);
    assert_eq!(
        tool.input_examples
            .as_ref()
            .unwrap()
            .iter()
            .map(|v| v.value().clone())
            .collect::<Vec<_>>(),
        examples.as_array().unwrap().clone()
    );
    assert!(matches!(&decoded.task.semantic.items()[1].1,
        Item::ToolChange(ToolChange { action: ToolChangeAction::AddDefinition(tool), .. })
            if tool.input_examples == Some(vec![])));
    assert!(GenerationRequirements::derive(&decoded.task.semantic).tool_examples);
    assert_eq!(
        profile.semantic_contract().check(&decoded.task.semantic),
        Err(GenerationFeature::ToolExamples)
    );
    profile
        .representation_contract()
        .check(&decoded.task.semantic)
        .unwrap();

    // Independent IR-to-wire expectation; structural validity does not assert
    // that the second example satisfies the integer property in the schema.
    let text = |s: &str| morphiecore::semantic::value::Text::new(s, "fixture", 128).unwrap();
    let root = FunctionTool {
        name: text("lookup"),
        description: None,
        parameters: Some(schema.clone().into()),
        input_examples: Some(
            examples
                .as_array()
                .unwrap()
                .iter()
                .cloned()
                .map(|v| StructuredValue::new(v).unwrap())
                .collect(),
        ),
        strict: FunctionStrictness::Explicit(false),
        output_schema: None,
        dispatch: Default::default(),
        cache: None,
        eager_input: None,
    };
    let later = FunctionTool {
        name: text("later"),
        input_examples: Some(vec![]),
        ..root.clone()
    };
    let mut controls = GenerationControls::default();
    controls.max_output_tokens = Some(32);
    let request = GenerationRequest::new(
        vec![
            (
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
            ),
            (
                ItemId::new(2),
                Item::ToolChange(ToolChange {
                    action: ToolChangeAction::AddDefinition(Box::new(later)),
                    cache: None,
                }),
            ),
        ],
        controls,
    )
    .unwrap()
    .with_tools(
        vec![ToolDefinition::Function(root.clone())],
        ToolChoice::Auto,
    )
    .unwrap()
    .with_message_envelopes(vec![
        MessageEnvelope::new(
            GroupId::new(LocalScope::ROOT, 1),
            MessageEnvelopeRole::User,
            vec![ItemId::new(1)],
        )
        .unwrap(),
    ])
    .unwrap();
    let wire = profile
        .encode_generation_request(
            &request,
            &Default::default(),
            &ReplayTarget::new("synthetic", "m").unwrap(),
        )
        .unwrap()
        .value;
    assert_eq!(wire["tools"], body["tools"]);
    assert_eq!(wire["messages"], body["messages"]);
    let mut caps = morphiecore::protocol::anthropic::messages::TargetContract::new(
        profile,
        morphiecore::protocol::anthropic::messages::Dialect::Standard,
    );
    caps.semantics = profile.representation_contract();
    caps.carriers = morphiecore::protocol::anthropic::messages::Carriers::all();
    let target = ReplayTarget::new("synthetic", "m").unwrap();
    caps.encode_request(&decoded, &target).unwrap();
    caps.carriers.tool_changes = false;
    assert!(matches!(caps.encode_request(&decoded, &target),
        Err(morphiecore::protocol::ConversionFailure { cause: morphiecore::protocol::ConversionCause::Codec(CodecError::Unsupported(label)), .. })
            if matches!(label,morphiecore::protocol::failure::CarrierReason::Tools)));
    for (present, expected) in [(false, None), (true, Some(vec![]))] {
        let mut simple = body.clone();
        simple["messages"] = json!([{"role":"user","content":"go"}]);
        simple["tools"][0]
            .as_object_mut()
            .unwrap()
            .remove("input_examples");
        if present {
            simple["tools"][0]["input_examples"] = json!([]);
        }
        let decoded = decode_client_request(simple.to_string().as_bytes()).unwrap();
        let ToolDefinition::Function(tool) = &decoded.task.semantic.tools()[0] else {
            panic!("function")
        };
        assert_eq!(tool.input_examples, expected);
        let mut settings = request.settings().clone();
        settings.tools = Some(vec![ToolDefinition::Function(FunctionTool {
            input_examples: expected,
            ..root.clone()
        })]);
        let own = request.clone().with_settings(settings).unwrap();
        let wire = profile
            .encode_generation_request(
                &own,
                &Default::default(),
                &ReplayTarget::new("synthetic", "m").unwrap(),
            )
            .unwrap()
            .value;
        assert_eq!(wire["tools"][0].get("input_examples").is_some(), present);
    }
    // Neither root nor by-value definitions may evade the new admission bit.
    let mut only_inline = request.settings().clone();
    only_inline.tools = None;
    let only_inline = request.clone().with_settings(only_inline).unwrap();
    assert!(GenerationRequirements::derive(&only_inline).tool_examples);
    let bound = morphiecore::topology::catalog::MESSAGES_BINDINGS[0].endpoint();
    assert!(bound.representation.semantics().tool_examples);
    for profile in [GenerationApi::Chat, GenerationApi::Responses] {
        assert!(
            GenerationCodec::new(profile, Dialect::Standard, None)
                .encode_request(
                    &decoded,
                    "m",
                    &morphiecore::protocol::openai::TargetContract::full()
                )
                .is_err()
        );
    }
}

#[test]
fn messages_tool_examples_reject_nonobjects_and_account_for_resource_bounds() {
    use morphiecore::protocol::anthropic::messages::Profile as Messages;
    let body = json!({"model":"m","max_tokens":32,"tools":[
        {"name":"lookup","strict":false,"input_schema":{"type":"object"}}],
        "messages":[{"role":"user","content":"go"}]});
    for value in [
        json!(null),
        json!({}),
        json!([1]),
        json!([null]),
        json!([[]]),
    ] {
        let mut bad = body.clone();
        bad["tools"][0]["input_examples"] = value.clone();
        let mut inline = body.clone();
        inline.as_object_mut().unwrap().remove("tools");
        inline["messages"].as_array_mut().unwrap().push(json!({
            "role":"system","content":[{"type":"tool_addition","tool":{
                "type":"tool_definition","definition":bad["tools"][0]}}]
        }));
        for request in [bad, inline] {
            let bytes = request.to_string();
            let native_error = Messages::AdaptiveTextTools
                .decode_request(bytes.as_bytes())
                .err()
                .unwrap();
            assert!(decode_client_request(bytes.as_bytes()).is_err());
            if value.is_null() {
                assert_eq!(
                    native_error,
                    CodecError::Invalid(morphiecore::protocol::failure::ShapeReason::InvalidValue)
                );
            }
        }
    }
    let decoded = decode_client_request(body.to_string().as_bytes()).unwrap();
    let mut settings = decoded.task.semantic.settings().clone();
    let ToolDefinition::Function(tool) = &mut settings.tools.as_mut().unwrap()[0] else {
        panic!("function")
    };
    tool.input_examples = Some(vec![StructuredValue::new(json!(1)).unwrap()]);
    assert_eq!(
        decoded
            .task
            .semantic
            .clone()
            .with_settings(settings.clone())
            .unwrap_err(),
        GenerationError::InvalidToolDefinition
    );
    let ToolDefinition::Function(tool) = &mut settings.tools.as_mut().unwrap()[0] else {
        panic!("function")
    };
    tool.input_examples = Some(vec![
        StructuredValue::new(json!({})).unwrap();
        MAX_ITEMS + 1
    ]);
    assert_eq!(
        decoded.task.semantic.with_settings(settings).unwrap_err(),
        GenerationError::Limit
    );
}

#[test]
fn messages_tool_examples_share_root_and_inline_envelope_node_limits() {
    use morphiecore::protocol::anthropic::messages::{
        Block, CacheMarked, InputMessage, MessageContent, Profile as Messages, Role, ToolTarget,
    };
    let profile = Messages::AdaptiveTextTools;
    let body = json!({"model":"m","max_tokens":32,"tools":[
        {"name":"lookup","strict":false,"input_schema":{"type":"object"}}],
        "messages":[{"role":"user","content":"go"}]});
    let example = StructuredValue::new(json!({"a":vec![0; 8189]})).unwrap();
    for inline in [false, true] {
        for (count, valid) in [(127, true), (128, false)] {
            let mut request = profile.decode_request(body.to_string().as_bytes()).unwrap();
            let mut tool = request.tools.value().unwrap()[0].clone();
            tool.input_examples = Presence::Value(vec![example.clone(); count]);
            if inline {
                request.tools = Presence::Absent;
                request.messages.push(InputMessage {
                    role: Role::System,
                    content: MessageContent::Blocks(vec![CacheMarked {
                        content: Block::ToolAddition(ToolTarget::Definition(tool)),
                        cache: None,
                    }]),
                    effort: None,
                });
            } else {
                request.tools = Presence::Value(vec![tool]);
            }
            let encoded = profile.encode_request(&request);
            if valid {
                let wire = serde_json::to_vec(&encoded.unwrap()).unwrap();
                assert!(profile.decode_request(&wire).is_ok());
            } else {
                assert_eq!(encoded.err(), Some(CodecError::Limit));
            }
        }
    }
}

#[test]
fn messages_final_usage_requires_reported_counts_and_unambiguous_cache_categories() {
    let mut response = response();
    assert!(encode_client_response(&response, "m").is_err());
    let mut stream = ClientEventEncoder::new("r", "m");
    stream.encode(&StreamEvent::Started).unwrap();
    stream
        .encode(&StreamEvent::Progress(InteractionProgress::TurnFinished))
        .unwrap();
    assert!(stream.encode(&terminal()).is_err());
    assert!(stream.finish().is_err());

    let mut usage = Usage::operation(10, 3, 13);
    usage.cached_input_tokens = Some(2);
    usage.input_cache_write_tokens = Some(0);
    response.semantic = response.semantic.with_usage(Some(usage)).unwrap();
    let mut unknown = response.clone();
    unknown.semantic = unknown
        .semantic
        .with_details(TerminalDetails::default())
        .unwrap()
        .with_progress(InteractionProgress::Unreported)
        .unwrap();
    assert!(matches!(
        encode_client_response(&unknown, "m"),
        Err(morphiecore::protocol::CodecError::Unsupported(reason))
            if reason == morphiecore::protocol::failure::CarrierReason::Terminal
    ));
    let mut unknown_stream = ClientEventEncoder::new("r", "m");
    unknown_stream.encode(&StreamEvent::Usage(usage)).unwrap();
    assert!(matches!(
        unknown_stream.encode(&StreamEvent::Terminal {
            terminal: StreamTerminal::Completed,
            details: TerminalDetails::default(),
        }),
        Err(morphiecore::protocol::CodecError::Unsupported(reason))
            if reason == morphiecore::protocol::failure::CarrierReason::Terminal
    ));
    unknown.semantic = unknown
        .semantic
        .with_progress(InteractionProgress::NeedsContinuation)
        .unwrap();
    assert!(
        morphiecore::adapter::ClientAdapter::Messages
            .snapshot(unknown)
            .is_err()
    );
    let mut continuation_stream = ClientEventEncoder::new("r", "m");
    continuation_stream
        .encode(&StreamEvent::Usage(usage))
        .unwrap();
    continuation_stream
        .encode(&StreamEvent::Progress(
            InteractionProgress::NeedsContinuation,
        ))
        .unwrap();
    assert!(matches!(
        continuation_stream.encode(&terminal()),
        Err(morphiecore::protocol::CodecError::Unsupported(reason))
            if reason == morphiecore::protocol::failure::CarrierReason::Terminal
    ));
    assert_eq!(
        encode_client_response(&response, "m").unwrap()["usage"],
        json!({"input_tokens":8,"output_tokens":3,
                "cache_read_input_tokens":2,"cache_creation_input_tokens":0})
    );
    use morphiecore::semantic::context::ServiceClass;
    response
        .metadata
        .context
        .reports
        .latest_mut()
        .expect("report")
        .service_class = Presence::Value(ServiceClass::StandardCapacity);
    assert_eq!(
        encode_client_response(&response, "m").unwrap()["usage"]["service_tier"],
        "standard"
    );
    for class in [
        ServiceClass::StandardProcessing,
        ServiceClass::FlexibleProcessing,
        ServiceClass::PriorityProcessing,
    ] {
        let mut wrong_service = response.clone();
        wrong_service
            .metadata
            .context
            .reports
            .latest_mut()
            .expect("report")
            .service_class = Presence::Value(class);
        assert!(matches!(encode_client_response(&wrong_service, "m"),
            Err(morphiecore::protocol::CodecError::Unsupported(reason))
                if reason == morphiecore::protocol::failure::CarrierReason::Field(morphiecore::protocol::conversion::generation::Field::ServiceClass)));
    }
    for terminal_class in [
        Presence::Absent,
        Presence::Value(ServiceClass::PriorityCapacity),
    ] {
        let mut phased = response.clone();
        phased.metadata.context.reports =
            morphiecore::semantic::context::ResponseReports::Lifecycle {
                opening: Some(Box::new(morphiecore::semantic::context::ResponseReport {
                    settings: None,
                    service_class: Presence::Value(ServiceClass::StandardCapacity),
                })),
                terminal: Some(Box::new(morphiecore::semantic::context::ResponseReport {
                    settings: None,
                    service_class: terminal_class,
                })),
            };
        assert!(matches!(
            encode_client_response(&phased, "m"),
            Err(morphiecore::protocol::CodecError::Invalid(
                morphiecore::protocol::failure::ShapeReason::Metadata
            )) | Err(morphiecore::protocol::CodecError::Unsupported(_))
        ));
    }
    let mut late_service = ClientEventEncoder::new("r", "m");
    late_service.encode(&StreamEvent::Started).unwrap();
    assert!(
        late_service
            .update_service_class(Presence::Value(ServiceClass::StandardCapacity))
            .is_err()
    );
    assert!(late_service.encode(&terminal()).is_err());
    assert!(late_service.finish().is_err());
    let mut ambiguous = response.clone();
    ambiguous.semantic = ambiguous
        .semantic
        .clone()
        .with_details(TerminalDetails {
            completion: Some(CompletionCause::NaturalOrStopSequence),
            ..Default::default()
        })
        .unwrap();
    assert!(encode_client_response(&ambiguous, "m").is_err());
    let mut ambiguous_stream = ClientEventEncoder::new("r", "m");
    ambiguous_stream.encode(&StreamEvent::Usage(usage)).unwrap();
    ambiguous_stream
        .encode(&StreamEvent::Progress(InteractionProgress::TurnFinished))
        .unwrap();
    assert!(
        ambiguous_stream
            .encode(&StreamEvent::Terminal {
                terminal: StreamTerminal::Completed,
                details: ambiguous.semantic.details().clone(),
            })
            .is_err()
    );
    let original = response.clone();
    let (retained, wire) = morphiecore::adapter::ClientAdapter::Messages
        .snapshot(response.clone())
        .unwrap();
    assert_eq!(wire["usage"]["input_tokens"], 8);
    assert_eq!(wire["usage"]["service_tier"], "standard");
    assert_eq!(
        retained.metadata.context.service_class().clone(),
        response.metadata.context.service_class().clone()
    );
    let retained_usage = retained.semantic.usage().unwrap();
    assert_eq!(retained_usage.input_tokens, Some(8));
    assert_eq!(
        retained_usage.input_relation,
        InputTokenRelation::ExcludesCacheReadAndWrite
    );
    assert_eq!(retained_usage.total_tokens, None);
    assert_eq!(response, original);
    let mut stream = ClientEventEncoder::new("r", "m");
    stream
        .update_service_class(response.metadata.context.service_class().clone())
        .unwrap();
    let mut frames = stream.encode(&StreamEvent::Usage(usage)).unwrap();
    frames.extend(
        stream
            .encode(&StreamEvent::Progress(InteractionProgress::TurnFinished))
            .unwrap(),
    );
    frames.extend(stream.encode(&terminal()).unwrap());
    let values: Vec<serde_json::Value> = frames
        .iter()
        .map(|frame| {
            let data = std::str::from_utf8(frame)
                .unwrap()
                .lines()
                .find_map(|line| line.strip_prefix("data: "))
                .unwrap();
            serde_json::from_str(data).unwrap()
        })
        .collect();
    assert_eq!(values[0]["message"]["usage"]["service_tier"], "standard");
    assert!(values[1]["usage"].get("service_tier").is_none());
    assert!(frames.iter().any(|frame| {
        std::str::from_utf8(frame)
            .unwrap()
            .contains("\"input_tokens\":8")
    }));
    stream.finish().unwrap();

    // Inclusive cache categories may overlap; their sum cannot be subtracted.
    response
        .metadata
        .context
        .reports
        .latest_mut()
        .expect("report")
        .service_class = Presence::Absent;
    usage.input_cache_write_tokens = Some(1);
    response.semantic = response.semantic.with_usage(Some(usage)).unwrap();
    assert!(encode_client_response(&response, "m").is_err());

    // OpenAI and Google report no write category; it stays absent, not zero.
    usage.input_cache_write_tokens = None;
    response.semantic = response.semantic.with_usage(Some(usage)).unwrap();
    assert_eq!(
        encode_client_response(&response, "m").unwrap()["usage"],
        json!({"input_tokens":8,"output_tokens":3,"cache_read_input_tokens":2})
    );
    usage.reasoning_tokens = Some(1);
    response.semantic = response.semantic.with_usage(Some(usage)).unwrap();
    assert_eq!(
        encode_client_response(&response, "m").unwrap()["usage"]["output_tokens_details"],
        json!({"thinking_tokens":1})
    );
    usage.reasoning_tokens = None;
    // Without the read subset, input cannot be split.
    usage.cached_input_tokens = None;
    response.semantic = response.semantic.with_usage(Some(usage)).unwrap();
    assert!(encode_client_response(&response, "m").is_err());
}
#[test]
fn messages_rejects_audio_in_static_output_and_each_event_shape() {
    let mut response = response();
    let mut usage = Usage::operation(0, 3, 3);
    usage.input_relation = InputTokenRelation::ExcludesCacheReadAndWrite;
    response.semantic = response.semantic.with_usage(Some(usage)).unwrap();
    let mut audio = response.clone();
    let mut items = audio.semantic.items().to_vec();
    let Item::Message(message) = &mut items[0].1 else {
        panic!("message")
    };
    let text = |s: &str| morphiecore::semantic::value::Text::new(s, "synthetic", 128).unwrap();
    message.parts[0].content = ContentPart::Audio(
        GeneratedAudio::new(
            AudioReference::new(text("a"), Some(2000)).unwrap(),
            text("AQID"),
            text("answer"),
        )
        .unwrap(),
    );
    audio.semantic = audio.semantic.with_items(items).unwrap();
    assert!(encode_client_response(&audio, "m").is_err());
    for event in [
        StreamEvent::PartStarted {
            item: ItemId::new(1),
            part: PartId::new(1),
            kind: PartKind::Audio,
        },
        StreamEvent::AudioDelta {
            item: ItemId::new(1),
            part: PartId::new(1),
            update: AudioUpdate {
                data: Some(vec![1, 2, 3]),
                ..Default::default()
            },
        },
    ] {
        let mut stream = ClientEventEncoder::new("r", "m");
        stream.encode(&StreamEvent::Started).unwrap();
        stream
            .encode(&crate::events_support::start(
                1,
                ItemKind::Message { phase: None },
            ))
            .unwrap();
        assert_eq!(
            stream.encode(&event).unwrap_err(),
            CodecError::Unsupported(morphiecore::protocol::failure::CarrierReason::Unavailable),
            "{event:?}"
        );
        assert_eq!(
            stream.encode(&StreamEvent::Started).unwrap_err(),
            CodecError::Invalid(morphiecore::protocol::failure::ShapeReason::InvalidValue)
        );
        assert!(stream.finish().is_err());
    }
}

#[test]
fn messages_serializes_interleaved_owners_parts_and_delayed_signatures() {
    use crate::events_support::interleaved_messages;
    for thinking in [false, true] {
        let mut events = interleaved_messages(ItemReplay {
            value: ReplayValue::final_value(
                ReplayFormat::AnthropicMessagesThinking,
                crate::events_support::text("synthetic-signature"),
            ),
            origin: None,
        });
        if !thinking {
            events.retain(|event| {
                !matches!(event,
                StreamEvent::ItemStarted { item, .. } | StreamEvent::ItemFinished { item, .. }
                | StreamEvent::PartStarted { item, .. } | StreamEvent::Delta { item, .. }
                | StreamEvent::ValueFinished { item, .. } | StreamEvent::PartFinished { item, .. }
                if *item == ItemId::new(10))
            });
        }
        let mut state = StreamState::new();
        let mut encoder = ClientEventEncoder::new("r", "m");
        let mut wire = String::new();
        for event in &events {
            state = reduce(state, event.clone()).unwrap();
            let frames = encoder.encode(event).unwrap();
            let batch = frames
                .iter()
                .map(|f| std::str::from_utf8(f).unwrap())
                .collect::<String>();
            if matches!(event, StreamEvent::Delta { item, fragment, .. }
                if *item == ItemId::new(if thinking { 10 } else { 3 })
                    && fragment == if thinking { "Plan" } else { "A" })
            {
                assert!(
                    batch.contains(if thinking {
                        "thinking_delta"
                    } else {
                        "text_delta"
                    }),
                    "head content must publish before later owners or EOF"
                );
            }
            if thinking
                && matches!(event, StreamEvent::ItemFinished { item, .. } if *item != ItemId::new(10))
            {
                assert!(
                    batch.is_empty(),
                    "later completion cannot pass the signature barrier"
                );
            }
            wire.push_str(&batch);
        }
        encoder.finish().unwrap();
        let mut expected = vec![
            json!({"type":"text","text":"AC"}),
            json!({"type":"text","text":"B"}),
            json!({"type":"tool_use","id":"first","name":"lookup","input":{"n":1}}),
            json!({"type":"tool_use","id":"second","name":"lookup","input":{"n":2}}),
        ];
        if thinking {
            expected.insert(
                0,
                json!({"type":"thinking","thinking":"Plan","signature":"synthetic-signature"}),
            );
            assert_eq!(wire.matches("\"type\":\"signature_delta\"").count(), 1);
        }
        let streamed = crate::messages_stream::messages_stream_body(&wire);
        assert_eq!(streamed["content"], json!(expected));
        assert_eq!(streamed["stop_reason"], "tool_use");
        let mut source = response();
        source.semantic = materialize(&state).unwrap();
        let before = source.clone();
        let static_wire = encode_client_response(&source, "m").unwrap();
        assert_eq!(static_wire["content"], json!(expected));
        assert_eq!(source, before);
        if !thinking {
            use morphiecore::{
                adapter::ClientAdapter,
                execution::{Attempt, ResponseDelivery},
                protocol::{openai::obfuscation::Obfuscation, stream::SseLimits},
            };
            let limits = SseLimits::default();
            let chunk = |delta, finish| {
                json!({
                    "id":"scheduled","object":"chat.completion.chunk","created":1,"model":"synthetic",
                    "choices":[{"index":0,"delta":delta,"finish_reason":finish}]
                })
            };
            let frames = [
                chunk(
                    json!({"role":"assistant","content":"A"}),
                    serde_json::Value::Null,
                ),
                chunk(
                    json!({"tool_calls":[{"index":0,"id":"first","type":"function",
                    "function":{"name":"lookup","arguments":" \n{\"n\":"}}]}),
                    serde_json::Value::Null,
                ),
                chunk(
                    json!({"tool_calls":[{"index":1,"id":"second","type":"function",
                    "function":{"name":"lookup","arguments":"{\"n\":2}"}}]}),
                    serde_json::Value::Null,
                ),
                chunk(json!({"content":"B"}), serde_json::Value::Null),
                chunk(
                    json!({"tool_calls":[{"index":0,"function":{"arguments":"1}"}}]}),
                    serde_json::Value::Null,
                ),
                chunk(json!({}), json!("tool_calls")),
                json!({"id":"scheduled","object":"chat.completion.chunk","created":1,"model":"synthetic",
                    "choices":[],"usage":{"prompt_tokens":1,"completion_tokens":2,"total_tokens":3,
                        "prompt_tokens_details":{"cached_tokens":0}}}),
            ];
            let input = frames
                .iter()
                .map(|frame| format!("data: {frame}\n\n"))
                .collect::<String>()
                + "data: [DONE]\n\n";
            let mut attempt = Attempt::new(
                GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None),
                1 << 20,
                limits,
            );
            attempt.begin(200, "text/event-stream").unwrap();
            let mut delivery = ResponseDelivery::new(
                ClientAdapter::Messages,
                "m",
                limits,
                Default::default(),
                Obfuscation::Disabled,
            );
            let mut output = vec![];
            for fragment in input.as_bytes().chunks(13) {
                let mut rest = fragment;
                while !rest.is_empty() {
                    let (used, events) = attempt.push(rest).unwrap();
                    assert!(used > 0);
                    output.extend(
                        delivery
                            .encode_events(&attempt, &events)
                            .unwrap()
                            .into_bytes()
                            .into_iter()
                            .flatten(),
                    );
                    rest = &rest[used..];
                }
            }
            assert!(!String::from_utf8_lossy(&output).contains("message_stop"));
            attempt.finish().unwrap();
            output.extend(
                delivery
                    .finish_stream(&attempt)
                    .unwrap()
                    .into_bytes()
                    .into_iter()
                    .flatten(),
            );
            let delivered =
                crate::messages_stream::messages_stream_body(std::str::from_utf8(&output).unwrap());
            assert_eq!(
                delivered["content"],
                json!([
                    {"type":"text","text":"AB"},
                    {"type":"tool_use","id":"first","name":"lookup","input":{"n":1}},
                    {"type":"tool_use","id":"second","name":"lookup","input":{"n":2}},
                ])
            );
            assert_eq!(delivered["stop_reason"], "tool_use");
        }
    }
}

#[test]
fn messages_preserves_parts_and_rejects_unrepresentable_calls() {
    let mut response = response();
    let mut usage = Usage::operation(0, 3, 3);
    usage.input_relation = InputTokenRelation::ExcludesCacheReadAndWrite;
    response.semantic = response.semantic.with_usage(Some(usage)).unwrap();
    let text = |s: &str| morphiecore::semantic::value::Text::new(s, "synthetic", 128).unwrap();
    assert_eq!(
        encode_client_response(&response, "m").unwrap()["content"],
        json!([{"type":"text","text":"answer"}])
    );
    // An incomplete terminal closes the started text block by its wire index.
    let mut truncated = ClientEventEncoder::new("r", "m");
    let mut frames = vec![];
    for event in [
        StreamEvent::Started,
        StreamEvent::ItemStarted {
            item: ItemId::new(1),
            kind: ItemKind::Message { phase: None },
            replay: None,
        },
        StreamEvent::PartStarted {
            item: ItemId::new(1),
            part: PartId::new(1),
            kind: PartKind::Refusal,
        },
        StreamEvent::ItemStarted {
            item: ItemId::new(2),
            kind: ItemKind::Message { phase: None },
            replay: None,
        },
        StreamEvent::PartStarted {
            item: ItemId::new(2),
            part: PartId::new(2),
            kind: PartKind::Text,
        },
        StreamEvent::Delta {
            item: ItemId::new(2),
            part: PartId::new(2),
            fragment: "later".into(),
            logprobs: vec![],
        },
        StreamEvent::Delta {
            item: ItemId::new(1),
            part: PartId::new(1),
            fragment: "first".into(),
            logprobs: vec![],
        },
        StreamEvent::Usage(usage),
        StreamEvent::Terminal {
            terminal: StreamTerminal::Incomplete,
            details: TerminalDetails {
                incomplete: Some(IncompleteReason::MaxOutputTokens),
                ..Default::default()
            },
        },
    ] {
        frames.extend(truncated.encode(&event).unwrap());
    }
    truncated.finish().unwrap();
    let frames: String = frames
        .iter()
        .map(|frame| std::str::from_utf8(frame).unwrap())
        .collect();
    assert!(frames.contains(r#"{"type":"content_block_stop","index":0}"#));
    assert!(!frames.contains(r#""index":null"#));
    let folded = crate::messages_stream::messages_stream_body(&frames);
    assert_eq!(
        folded["content"],
        json!([
            {"type":"text","text":"first"},{"type":"text","text":"later"},
        ])
    );
    assert_eq!(folded["stop_reason"], "max_tokens");
    let mut split = response.clone();
    let mut items = split.semantic.items().to_vec();
    let Item::Message(message) = &mut items[0].1 else {
        panic!("message")
    };
    for (id, value) in [(2, ""), (3, "tail")] {
        message.parts.push(Part {
            replay: None,
            id: PartId::new(id),
            cache: None,
            content: ContentPart::Text(
                morphiecore::semantic::value::Text::allowing_empty(value, "test", 128)
                    .unwrap()
                    .into(),
            ),
        });
    }
    split.semantic = split.semantic.with_items(items).unwrap();
    let (retained, wire) = morphiecore::adapter::ClientAdapter::Messages
        .snapshot(split.clone())
        .unwrap();
    assert_eq!(retained.semantic.items(), split.semantic.items());
    assert_eq!(
        wire["content"],
        json!([
            {"type":"text","text":"answer"},{"type":"text","text":""},{"type":"text","text":"tail"}])
    );
    let mut stream = ClientEventEncoder::new("r", "m");
    let mut frames = stream
        .encode(&crate::events_support::start(
            1,
            ItemKind::Message { phase: None },
        ))
        .unwrap();
    for (id, value) in [(1, "answer"), (2, ""), (3, "tail")] {
        for event in crate::events_support::part(1, id, PartKind::Text, value) {
            frames.extend(stream.encode(&event).unwrap());
        }
    }
    frames.extend(
        stream
            .encode(&crate::events_support::close(1, ItemLifecycle::Completed))
            .unwrap(),
    );
    let blocks: Vec<serde_json::Value> = frames
        .iter()
        .filter_map(|frame| {
            std::str::from_utf8(frame)
                .unwrap()
                .lines()
                .find_map(|line| line.strip_prefix("data: "))
                .map(|data| serde_json::from_str(data).unwrap())
        })
        .filter(|value: &serde_json::Value| value["type"] != "message_start")
        .collect();
    assert_eq!(
        blocks,
        vec![
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"answer"}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_stop","index":1}),
            json!({"type":"content_block_start","index":2,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":2,"delta":{"type":"text_delta","text":"tail"}}),
            json!({"type":"content_block_stop","index":2}),
        ]
    );
    let mut call = ToolCall {
        replay: None,
        call_id: text("c"),
        cache: None,
        name: text("lookup"),
        arguments: ToolArguments::Raw("{}".into()),
        status: ItemLifecycle::Completed,
        context: Default::default(),
    };
    let mut unknown_call = response.clone();
    unknown_call.semantic = GenerationResponse::new(
        vec![(ItemId::new(1), Item::ToolCall(call.clone()))],
        Outcome::Completed,
    )
    .unwrap()
    .with_usage(Some(usage))
    .unwrap();
    let (retained, wire) = morphiecore::adapter::ClientAdapter::Messages
        .snapshot(unknown_call)
        .unwrap();
    assert_eq!(wire["stop_reason"], "tool_use");
    assert_eq!(
        retained.semantic.progress(),
        InteractionProgress::Unreported
    );
    let mut unknown_stream = ClientEventEncoder::new("r", "m");
    unknown_stream
        .encode(&crate::events_support::start(
            1,
            ItemKind::ToolCall {
                format: ArgumentFormat::Raw,
                call_id: call.call_id.clone(),
                name: call.name.clone(),
                context: call.context.clone(),
            },
        ))
        .unwrap();
    unknown_stream.encode(&StreamEvent::Usage(usage)).unwrap();
    let terminal_frames = unknown_stream
        .encode(&StreamEvent::Terminal {
            terminal: StreamTerminal::Completed,
            details: TerminalDetails::default(),
        })
        .unwrap();
    assert!(terminal_frames.iter().any(|frame| {
        std::str::from_utf8(frame)
            .unwrap()
            .contains(r#""stop_reason":"tool_use""#)
    }));
    let tool_response = |call: ToolCall| {
        let mut value = response.clone();
        value.semantic = GenerationResponse::from_resources(
            vec![(ItemId::new(1), Item::ToolCall(call))],
            Outcome::Completed,
            ResourceTable::default(),
        )
        .unwrap()
        .with_progress(InteractionProgress::AwaitingToolResults)
        .unwrap()
        .with_usage(Some(usage))
        .unwrap();
        value
    };
    assert_eq!(
        encode_client_response(&tool_response(call.clone()), "m").unwrap()["content"],
        json!([{"type":"tool_use","id":"c","name":"lookup","input":{}}])
    );
    for (id, name, valid) in [
        ("a".repeat(64), "b".repeat(128), true),
        ("a".repeat(65), "lookup".into(), false),
        ("bad.id".into(), "lookup".into(), false),
        ("c".into(), "bad.name".into(), false),
        ("c".into(), "工具".into(), false),
    ] {
        let mut candidate = call.clone();
        candidate.call_id = morphiecore::semantic::value::Text::new(id, "id", 256).unwrap();
        candidate.name = morphiecore::semantic::value::Text::new(name, "name", 128).unwrap();
        let source = tool_response(candidate.clone());
        let rendered = morphiecore::adapter::ClientAdapter::Messages.snapshot(source);
        assert_eq!(rendered.is_ok(), valid);
        let mut stream = ClientEventEncoder::new("r", "m");
        assert_eq!(
            stream
                .encode(&crate::events_support::start(
                    1,
                    ItemKind::ToolCall {
                        format: ArgumentFormat::Raw,
                        call_id: candidate.call_id.clone(),
                        name: candidate.name.clone(),
                        context: candidate.context.clone(),
                    }
                ))
                .is_ok(),
            valid
        );
        if valid {
            let (_, wire) = rendered.unwrap();
            let history = json!({"model":"m","max_tokens":32,"messages":[
                {"role":"user","content":"go"}, {"role":"assistant","content":wire["content"]},
                {"role":"user","content":[{"type":"tool_result","tool_use_id":candidate.call_id.as_str(),
                    "content":"found"}]}]});
            decode_client_request(history.to_string().as_bytes()).unwrap();
        } else {
            assert!(stream.encode(&terminal()).is_err());
            assert!(stream.finish().is_err());
        }
    }
    call.context.namespace = Some(text("private"));
    assert!(encode_client_response(&tool_response(call.clone()), "m").is_err());
    let mut stream = ClientEventEncoder::new("r", "m");
    assert!(
        stream
            .encode(&crate::events_support::start(
                1,
                ItemKind::ToolCall {
                    format: ArgumentFormat::Raw,
                    call_id: call.call_id.clone(),
                    name: call.name.clone(),
                    context: call.context.clone()
                }
            ))
            .is_err()
    );
    call.context = Default::default();
    call.arguments = ToolArguments::Raw(r#"{"x":1,"x":2}"#.into());
    assert!(encode_client_response(&tool_response(call.clone()), "m").is_err());
    call.arguments = ToolArguments::Raw("1".into());
    assert!(encode_client_response(&tool_response(call), "m").is_err());
}

#[test]
fn messages_rejects_citations_in_static_output_and_each_event_shape() {
    let mut response = response();
    let mut usage = Usage::operation(0, 3, 3);
    usage.input_relation = InputTokenRelation::ExcludesCacheReadAndWrite;
    response.semantic = response.semantic.with_usage(Some(usage)).unwrap();
    let resource = ResourceId::scoped(LocalScope::ROOT, 1);
    let resources = ResourceTable::default()
        .insert(
            resource,
            ResourceLocation::Url(
                morphiecore::semantic::value::Text::new(
                    "https://example.invalid/source",
                    "synthetic",
                    128,
                )
                .unwrap(),
            ),
        )
        .unwrap();
    let annotation = Annotation::new(
        Citation {
            kind: CitationKind::Citation,
            claim: ClaimAnchor::Whole,
            source: resource,
            coordinates: SourceCoordinates::Unreported,
            label: None,
            reported_ordinal: None,
        },
        &resources,
    )
    .unwrap();
    let mut items = response.semantic.items().to_vec();
    let Item::Message(message) = &mut items[0].1 else {
        panic!("message")
    };
    let ContentPart::Text(text) = &message.parts[0].content else {
        panic!("text")
    };
    message.parts[0].content = ContentPart::Text(
        TextContent::new(
            morphiecore::semantic::value::Text::new(text.as_str(), "synthetic", 128).unwrap(),
            vec![annotation.clone()],
            Presence::Absent,
        )
        .unwrap(),
    );
    response.semantic = response
        .semantic
        .with_resources(resources)
        .unwrap()
        .with_items(items)
        .unwrap();
    assert!(encode_client_response(&response, "m").is_err());
    for event in [
        StreamEvent::AnnotationAdded {
            item: ItemId::new(1),
            part: PartId::new(1),
            annotation: annotation.clone(),
        },
        StreamEvent::TextMetadata {
            item: ItemId::new(1),
            part: PartId::new(1),
            annotations: vec![annotation],
            logprobs: Presence::Absent,
        },
    ] {
        let mut stream = ClientEventEncoder::new("r", "m");
        stream.encode(&StreamEvent::Started).unwrap();
        stream
            .encode(&crate::events_support::start(
                1,
                ItemKind::Message { phase: None },
            ))
            .unwrap();
        stream
            .encode(&StreamEvent::PartStarted {
                item: ItemId::new(1),
                part: PartId::new(1),
                kind: PartKind::Text,
            })
            .unwrap();
        assert_eq!(
            stream.encode(&event).unwrap_err(),
            CodecError::Unsupported(morphiecore::protocol::failure::CarrierReason::Unavailable),
            "{event:?}"
        );
    }
}
/// pi 1.3.0's built-in Haiku request shape: per-message effort, block binding and
/// ordered tool changes keep exact carriers or their named rules.
#[test]
fn pi_haiku_profile_keeps_carriers_or_records_rules() {
    use morphiecore::protocol::{anthropic, native::ReplayTarget};
    let strict = json!({"type":"object","properties":{"path":{"type":"string"}},
            "required":["path"],"additionalProperties":false});
    let body = json!({"model":"m","max_tokens":64,
            "system":[{"type":"text","text":"You are pi."}],
            "thinking":{"type":"adaptive","display":"summarized",
                "block_binding":{"prefix_mismatch_behavior":"drop_block"}},
            "output_config":{"effort":"high"},
            "tools":[
                {"name":"read","input_schema":strict,"strict":true,"eager_input_streaming":true},
                {"name":"__pi_deferred_placeholder__","input_schema":{"type":"object"},
                    "defer_loading":true}],
            "messages":[
                {"role":"user","content":"read a"},
                {"role":"system","content":[],"output_config":{"effort":"low"}},
                {"role":"assistant","content":[
                    {"type":"tool_use","id":"t1","name":"read","input":{"path":"a"}}]},
                {"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"A"}]},
                {"role":"assistant","content":"done"},
                {"role":"user","content":"now write"},
                {"role":"system","content":[{"type":"text","text":"Write mode."},
                    {"type":"tool_removal","tool":{"type":"tool_reference","name":"read"}},
                    {"type":"tool_addition","tool":{"type":"tool_definition","definition":
                        {"name":"write","input_schema":strict,"strict":true}}}]},
                {"role":"system","content":[],"output_config":{"effort":"medium"}}]});
    let decoded = native_request(&body);
    let semantic = &decoded.task.semantic;
    assert_eq!(
        semantic.reasoning().replay_mismatch,
        Some(ReplayMismatch::Drop)
    );
    let effort = |level| {
        Item::ConfigurationUpdate(ConfigurationUpdate {
            activation: None,
            effort: Some(EffortControl {
                scope: EffortScope::Response,
                level,
            }),
        })
    };
    let items: Vec<_> = semantic.items().iter().map(|(_, item)| item).collect();
    assert_eq!(items.len(), 10);
    assert_eq!(items[1], &effort(EffortLevel::Low));
    assert!(matches!(items[6], Item::Instruction(i)
            if i.texts().map(|(_, t)| t).collect::<Vec<_>>() == ["Write mode."]));
    assert!(matches!(items[7], Item::ToolChange(ToolChange {
            action: ToolChangeAction::Remove(name), cache: None }) if name.as_str() == "read"));
    assert!(matches!(items[8], Item::ToolChange(ToolChange {
            action: ToolChangeAction::AddDefinition(tool), cache: None })
            if tool.name.as_str() == "write"
                && tool.strict == FunctionStrictness::Explicit(true)));
    assert_eq!(items[9], &effort(EffortLevel::Medium));

    let target = ReplayTarget::new("synthetic", "synthetic").unwrap();
    let mut caps = anthropic::messages::TargetContract::new(
        anthropic::messages::Profile::AdaptiveTextTools,
        anthropic::messages::Dialect::Standard,
    );
    caps.semantics = anthropic::messages::Profile::AdaptiveTextTools.representation_contract();
    caps.carriers = anthropic::messages::Carriers::all();
    let exact = caps.encode_request(&decoded, &target).unwrap();
    assert!(
        exact.evidence.is_empty(),
        "native arguments are already structured"
    );
    let wire = &exact.value;
    assert_eq!(wire["thinking"], body["thinking"]);
    assert_eq!(wire["tools"], body["tools"]);
    assert_eq!(wire["output_config"], json!({"effort":"high"}));
    // One system section shares a message; its effort rides on that message.
    let messages = wire["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 7);
    assert_eq!(
        messages[1],
        json!({"role":"system","content":[],"output_config":{"effort":"low"}})
    );
    assert_eq!(
        messages[6],
        json!({"role":"system","content":[{"type":"text","text":"Write mode."},
                body["messages"][6]["content"][1], body["messages"][6]["content"][2]],
                "output_config":{"effort":"medium"}})
    );

    // A retained historical level must satisfy the binding even when the reply's
    // level is allowed. Missing-carrier flattening is checked on its final value.
    let mut limited = caps.clone();
    limited.model_limits.allowed_effort = Some(vec![EffortLevel::High, EffortLevel::Medium].into());
    let before = decoded.clone();
    assert!(matches!(limited.encode_request(&decoded, &target),
        Err(morphiecore::protocol::ConversionFailure { cause: morphiecore::protocol::ConversionCause::Codec(
            morphiecore::protocol::CodecError::Unsupported(value)), .. }) if value == morphiecore::protocol::failure::CarrierReason::Unavailable));
    limited.carriers.message_effort = false;
    let flattened = limited.encode_request(&decoded, &target).unwrap();
    assert_eq!(flattened.value["output_config"], json!({"effort":"medium"}));
    assert!(
        flattened
            .evidence
            .contains_rule(anthropic::messages::RequestRule::FlattenMessageEffort.name())
    );
    assert_eq!(decoded, before);

    // Without carriers: tolerant binding is omitted, the reply's effort is the level in
    // effect for it, and the tool set is the final available one.
    caps.carriers = Default::default();
    let flat = caps.encode_request(&decoded, &target).unwrap();
    let rules: Vec<_> = flat.evidence.iter().map(|r| r.rule()).collect();
    assert_eq!(
        rules,
        [
            "messages.omit-block-binding.v1",
            "messages.flatten-message-effort.v1",
            "messages.collapse-tool-changes.v1"
        ]
    );
    let wire = &flat.value;
    assert_eq!(
        wire["thinking"],
        json!({"type":"adaptive","display":"summarized"})
    );
    assert_eq!(wire["output_config"], json!({"effort":"medium"}));
    assert_eq!(
        wire["tools"],
        json!([{"name":"write","input_schema":strict,"strict":true}])
    );
    let messages = wire["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 6);
    assert_eq!(
        messages[5],
        json!({"role":"system","content":[{"type":"text","text":"Write mode."}]})
    );
    // A change after a tool-result-only turn waits for new input.
    let mut pending = body.clone();
    pending["messages"] = json!([
            {"role":"user","content":"read a"},
            {"role":"assistant","content":[
                {"type":"tool_use","id":"t1","name":"read","input":{"path":"a"}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"A"}]},
            {"role":"system","content":[],"output_config":{"effort":"low"}}]);
    let decoded = native_request(&pending);
    let flat = caps.encode_request(&decoded, &target).unwrap();
    assert_eq!(flat.value["output_config"], json!({"effort":"high"}));
    // Explicit enforcement has no carrier to degrade to.
    let mut enforced = pending;
    enforced["thinking"]["block_binding"]["prefix_mismatch_behavior"] = json!("error");
    let decoded = native_request(&enforced);
    assert!(caps.encode_request(&decoded, &target).is_err());
    caps.carriers.block_binding = true;
    assert_eq!(
        caps.encode_request(&decoded, &target).unwrap().value["thinking"]["block_binding"],
        json!({"prefix_mismatch_behavior":"error"})
    );
    // Tool-change blocks are system content only, and removal names a reference.
    for (index, role, block, _error) in [
        (
            5,
            "user",
            json!({"type":"tool_removal","tool":{"type":"tool_reference","name":"read"}}),
            "Anthropic block role",
        ),
        (
            6,
            "system",
            json!({"type":"tool_removal","tool":{"type":"tool_definition",
                    "definition":{"name":"read","input_schema":{"type":"object"}}}}),
            "Anthropic tool removal",
        ),
    ] {
        let mut misplaced = body.clone();
        misplaced["messages"][index] =
            json!({"role":role,"content":[{"type":"text","text":"x"},block]});
        assert_eq!(
            morphiecore::protocol::anthropic::messages::Profile::AdaptiveTextTools
                .decode_request(misplaced.to_string().as_bytes())
                .unwrap_err(),
            CodecError::Invalid(morphiecore::protocol::failure::ShapeReason::InvalidValue)
        );
    }
    // Per-message effort belongs to system messages.
    let mut misplaced = body;
    misplaced["messages"][0]["output_config"] = json!({"effort":"low"});
    assert!(matches!(
        morphiecore::protocol::anthropic::messages::Profile::AdaptiveTextTools
            .decode_request(misplaced.to_string().as_bytes()),
        Err(CodecError::Invalid(
            morphiecore::protocol::failure::ShapeReason::InvalidValue
        ))
    ));
}
#[test]
fn protected_messages_controls_reject_at_their_owner_not_inside_business_data() {
    let p = morphiecore::protocol::anthropic::messages::Profile::AdaptiveTextTools;
    let business = json!({"task_budget":{"type":"tokens","total":20},"toolset_name":"business"});
    let body = json!({"model":"m","max_tokens":8,
        "tools":[{"name":"lookup","strict":false,"input_schema":{"type":"object","properties":{
            "task_budget":{"type":"object"},"toolset_name":{"type":"string"}}}}],
        "messages":[
            {"role":"user","content":"go"},
            {"role":"assistant","content":[{"type":"tool_use","id":"c","name":"lookup","input":business}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"c","content":"ok"}]}]});
    let source = decode_client_request(body.to_string().as_bytes()).unwrap();
    let arguments = business.to_string();
    assert!(source.task.semantic.items().iter().any(|(_, item)|
        matches!(item, Item::ToolCall(c) if c.arguments.as_raw() == Some(arguments.as_str()))));
    let native = p.decode_request(body.to_string().as_bytes()).unwrap();
    let encoded = p.encode_request(&native).unwrap();
    assert_eq!(encoded["messages"][1]["content"][0]["input"], business);
    assert_eq!(
        encoded["tools"][0]["input_schema"],
        body["tools"][0]["input_schema"]
    );

    for budget in [
        json!({"type":"tokens","total":20,"remaining":10}),
        json!({"type":"tokens","total":0,"remaining":null}),
        serde_json::Value::Null,
    ] {
        let mut active = body.clone();
        active["output_config"] = json!({"task_budget":budget});
        let bytes = active.to_string();
        let expected =
            CodecError::Unsupported(morphiecore::protocol::failure::CarrierReason::Unavailable);
        assert_eq!(
            decode_client_request(bytes.as_bytes()).unwrap_err(),
            expected
        );
        assert_eq!(p.decode_request(bytes.as_bytes()).unwrap_err(), expected);
    }
    for index in [1, 2] {
        for namespace in [json!("set"), json!(""), serde_json::Value::Null] {
            let mut active = body.clone();
            active["messages"][index]["content"][0]["toolset_name"] = namespace;
            let bytes = active.to_string();
            let expected =
                CodecError::Unsupported(morphiecore::protocol::failure::CarrierReason::Tools);
            assert_eq!(
                decode_client_request(bytes.as_bytes()).unwrap_err(),
                expected
            );
            assert_eq!(p.decode_request(bytes.as_bytes()).unwrap_err(), expected);
        }
    }
    let mut extended = body;
    extended["output_config"] = json!({"future_output_control":{"task_budget":20}});
    extended["messages"][1]["content"][0]["future_block_control"] = json!({"toolset_name":"x"});
    assert_eq!(
        decode_client_request(extended.to_string().as_bytes())
            .unwrap()
            .task
            .semantic,
        source.task.semantic
    );
}

#[test]
fn client_intake_carries_caps_and_rejects_active_fields_it_cannot_carry() {
    let body = json!({"model":"m","max_tokens":8,"stop_sequences":[],
            "messages":[{"role":"user","content":"go"}]});
    assert!(decode_client_request(body.to_string().as_bytes()).is_ok());
    let mut strict = body.clone();
    strict["tools"] = json!([{"name":"lookup","input_schema":{"type":"object"},"strict":true}]);
    let decoded = decode_client_request(strict.to_string().as_bytes()).unwrap();
    assert!(matches!(&decoded.task.semantic.tools()[0],
            ToolDefinition::Function(t) if t.strict == FunctionStrictness::Explicit(true)));
    use morphiecore::protocol::{anthropic, native::ReplayTarget};
    let target = ReplayTarget::new("synthetic", "synthetic").unwrap();
    let mut caps = anthropic::messages::TargetContract::new(
        anthropic::messages::Profile::AdaptiveTextTools,
        anthropic::messages::Dialect::Standard,
    );
    use morphiecore::semantic::context::ServicePolicy;
    let mut selected = body.clone();
    selected["service_tier"] = json!("standard_only");
    let selected = decode_client_request(selected.to_string().as_bytes()).unwrap();
    assert_eq!(
        selected.service_policy,
        Presence::Value(ServicePolicy::StandardCapacityOnly)
    );
    assert!(selected.check_context(false).is_err());
    assert_eq!(
        caps.encode_request(&selected, &target).unwrap().value["service_tier"],
        "standard_only"
    );
    let mut manual = decode_client_request(body.to_string().as_bytes()).unwrap();
    manual.service_policy = Presence::Value(ServicePolicy::PreferCommittedCapacity);
    let wire = caps.encode_request(&manual, &target).unwrap().value;
    assert_eq!(wire["service_tier"], "auto");
    assert!(
        anthropic::messages::Profile::AdaptiveTextTools
            .decode_generation_request(wire.to_string().as_bytes(), LocalScope::ROOT, &target)
            .is_err()
    );
    assert!(
        GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None)
            .encode_request(
                &manual,
                "synthetic",
                &morphiecore::protocol::openai::TargetContract::full()
            )
            .is_err()
    );
    manual.service_policy = Presence::Value(ServicePolicy::ProjectConfigured);
    assert!(caps.encode_request(&manual, &target).is_err());
    assert!(caps.encode_request(&decoded, &target).is_err());
    caps.semantics.strict_tools = true;
    assert!(caps.encode_request(&decoded, &target).is_err());
    strict["tools"][0]["input_schema"]["additionalProperties"] = json!(false);
    let eligible = decode_client_request(strict.to_string().as_bytes()).unwrap();
    let projected = caps.encode_request(&eligible, &target).unwrap();
    assert_eq!(
        projected.value["tools"],
        json!([
            {"name":"lookup","input_schema":{"type":"object","additionalProperties":false},"strict":true}
        ])
    );
    assert!(projected.evidence.is_empty());
    // pi's tool flags keep typed owners and reach the native wire unchanged.
    let mut flagged = strict.clone();
    flagged["tools"][0]["eager_input_streaming"] = json!(true);
    flagged["tools"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"later","input_schema":{"type":"object"},"defer_loading":true}));
    let decoded = decode_client_request(flagged.to_string().as_bytes()).unwrap();
    let tools: Vec<_> = decoded
        .task
        .semantic
        .tools()
        .iter()
        .map(|tool| match tool {
            ToolDefinition::Function(t) => (t.eager_input, t.dispatch.defer_loading),
            _ => panic!("function"),
        })
        .collect();
    assert_eq!(tools, [(Some(true), false), (None, true)]);
    let projected = caps.encode_request(&decoded, &target).unwrap();
    assert_eq!(projected.value["tools"], flagged["tools"]);
    flagged["tools"][1]["cache_control"] = json!({"type":"ephemeral"});
    assert!(decode_client_request(flagged.to_string().as_bytes()).is_err());
    for (key, value) in [
        ("allowed_callers", json!(["code_execution_20250825"])),
        ("input_examples", json!([1])),
    ] {
        let mut active = strict.clone();
        active["tools"][0][key] = value;
        assert!(
            decode_client_request(active.to_string().as_bytes()).is_err(),
            "{key}"
        );
    }
    // Mid-conversation system messages are ordered, text-only instructions.
    let mut system = body.clone();
    system["messages"] = json!([{"role":"user","content":"go"},
            {"role":"system","content":[{"type":"text","text":"Be brief."}]}]);
    let decoded = decode_client_request(system.to_string().as_bytes()).unwrap();
    assert!(
        matches!(&decoded.task.semantic.items()[1].1, Item::Instruction(i)
            if i.texts().map(|(_, t)| t).collect::<Vec<_>>() == ["Be brief."])
    );
    for messages in [
        json!([{"role":"system","content":"first"},{"role":"user","content":"go"}]),
        json!([{"role":"user","content":"go"},{"role":"system","content":"x"},
                {"role":"user","content":"again"}]),
        json!([{"role":"user","content":"go"},{"role":"system","content":[
                {"type":"image","source":{"type":"url","url":"https://example.invalid/a.png"}}]}]),
        json!([{"role":"user","content":"go"},{"role":"system","content":"x",
                "clear_at":"next_user_message"}]),
    ] {
        system["messages"] = messages;
        assert!(decode_client_request(system.to_string().as_bytes()).is_err());
    }
    let mut thinking = body.clone();
    thinking["messages"][0]["content"] =
        json!([{"type":"thinking","thinking":"plan","signature":"synthetic"}]);
    assert!(decode_client_request(thinking.to_string().as_bytes()).is_err());
    thinking["messages"][0]["role"] = json!("assistant");
    assert!(decode_client_request(thinking.to_string().as_bytes()).is_ok());
    let mut cached_content = body.clone();
    cached_content["messages"][0]["content"] = json!([
        {"type":"text","text":"go","cache_control":{"type":"ephemeral","ttl":"1h"}}
    ]);
    let cached = decode_client_request(cached_content.to_string().as_bytes()).unwrap();
    cached_content["messages"][0]["content"][0]["cache_control"]["future_cache_metadata"] =
        json!(true);
    let unknown = decode_client_request(cached_content.to_string().as_bytes()).unwrap();
    assert_eq!(unknown.task.semantic, cached.task.semantic);
    let mut calls = body.clone();
    calls["messages"] = json!([
            {"role":"assistant","content":[{"type":"tool_use","id":"c","name":"lookup","input":{},
                "caller":{"type":"direct"}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"c","content":"ok"}]}]);
    assert!(decode_client_request(calls.to_string().as_bytes()).is_ok());
    // `content` is optional on a tool result; absence is an empty result.
    let mut bare = calls.clone();
    bare["messages"][1]["content"] =
        json!([{"type":"tool_result","tool_use_id":"c","is_error":true}]);
    let bare = decode_client_request(bare.to_string().as_bytes()).unwrap();
    assert!(
        matches!(&bare.task.semantic.items()[1].1, Item::ToolResult(r)
            if matches!(&r.output, ToolOutput::Parts(parts) if parts.is_empty())
                && r.is_error == Some(true))
    );
    calls["messages"][0]["content"][0]["caller"] =
        json!({"type":"code_execution_20250825","tool_id":"program_1"});
    assert!(decode_client_request(calls.to_string().as_bytes()).is_err());
    for (key, value) in [
        ("stop_sequences", json!([7])),
        ("service_tier", json!("priority")),
        ("container", json!("container_1")),
        (
            "mcp_servers",
            json!([{"type":"url","url":"https://example.invalid","name":"x"}]),
        ),
        (
            "context_management",
            json!({"edits":[{"type":"clear_tool_uses_20250919"}]}),
        ),
        ("compaction", json!({})),
    ] {
        let mut active = body.clone();
        active[key] = value;
        assert!(
            decode_client_request(active.to_string().as_bytes()).is_err(),
            "{key}"
        );
    }

    // The client-facing Chat dialect accepts the deprecated cap spelling.
    let chat = GenerationCodec::new(GenerationApi::Chat, Dialect::MorphieCore, None);
    let decode = |body: &serde_json::Value| chat.decode_client_request(body.to_string().as_bytes());
    // Empty stop values request no stop control.
    let body = json!({"model":"m","max_tokens":7,"stop":"","functions":[],
            "messages":[{"role":"user","content":"go"}]});
    assert_eq!(
        decode(&body)
            .unwrap()
            .task
            .semantic
            .settings()
            .controls
            .max_output_tokens,
        Some(7)
    );
    let mut prediction = body.clone();
    prediction["prediction"] = json!({"type":"content","content":"synthetic predicted output"});
    let decoded = decode(&prediction).unwrap();
    assert_eq!(
        decoded
            .task
            .semantic
            .settings()
            .prediction
            .as_ref()
            .unwrap()[0]
            .as_str(),
        "synthetic predicted output"
    );
    assert!(decoded.task.evidence.is_empty());
    assert!(!format!("{:?}", decoded.task.fidelity).contains("synthetic predicted output"));
    let wire = chat
        .encode_request(
            &decoded,
            "m",
            &morphiecore::protocol::openai::TargetContract::full(),
        )
        .unwrap();
    assert_eq!(wire["prediction"], prediction["prediction"]);
    prediction["prediction"]["content"] = json!(7);
    assert!(decode(&prediction).is_err());
    let mut controlled = body.clone();
    controlled["stop"] = json!(["END", "DONE"]);
    controlled["seed"] = json!(0);
    controlled["presence_penalty"] = json!(0.25);
    controlled["frequency_penalty"] = json!(-0.5);
    let controlled = decode(&controlled).unwrap();
    assert_eq!(controlled.task.semantic.controls().seed, Some(0));
    assert_eq!(
        controlled.task.semantic.controls().presence_penalty(),
        Some(0.25)
    );
    assert_eq!(
        controlled.task.semantic.controls().frequency_penalty(),
        Some(-0.5)
    );
    assert_eq!(
        controlled
            .task
            .semantic
            .controls()
            .stop_sequences
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>(),
        ["END", "DONE"]
    );
    let wire = chat
        .encode_request(
            &controlled,
            "m",
            &morphiecore::protocol::openai::TargetContract::full(),
        )
        .unwrap();
    assert_eq!(wire["stop"], json!(["END", "DONE"]));
    assert_eq!(wire["seed"], 0);
    assert_eq!(wire["presence_penalty"], 0.25);
    assert_eq!(wire["frequency_penalty"], -0.5);
    for (key, value) in [
        ("max_completion_tokens", json!(7)),
        ("stop", json!([7])),
        (
            "functions",
            json!([{"name":"lookup","parameters":{"type":"object"}}]),
        ),
        ("function_call", json!("auto")),
        ("web_search_options", json!({})),
        ("seed", json!("0")),
        ("frequency_penalty", json!(3)),
        ("presence_penalty", json!(3)),
        ("logit_bias", json!({"1": 1})),
        // Stored completions need storage the gateway does not provide.
        ("store", json!(true)),
    ] {
        let mut active = body.clone();
        active[key] = value;
        assert!(decode(&active).is_err(), "{key}");
    }
    let mut stored = body.clone();
    stored["store"] = json!(false);
    assert!(decode(&stored).is_ok());
    // Official request names belong to the message, not a tool or fidelity record.
    let mut named = body.clone();
    named["messages"][0]["name"] = json!("alice");
    assert_eq!(
        decode(&named).unwrap().task.semantic.items()[0]
            .1
            .participant()
            .unwrap()
            .as_str(),
        "alice"
    );
    named["messages"][0]["name"] = serde_json::Value::Null;
    assert!(decode(&named).is_err());
    // Assistant history may mix text and refusal parts; both keep their kinds.
    let mut refused = body.clone();
    refused["messages"] = json!([{"role":"user","content":"go"},
            {"role":"assistant","content":[{"type":"text","text":"partial"},
                {"type":"refusal","refusal":"no"}]},
            {"role":"user","content":"again"}]);
    let refused = decode(&refused).unwrap();
    assert!(
        matches!(&refused.task.semantic.items()[1].1, Item::Message(m)
        if matches!(m.parts.as_slice(), [
            Part { content: ContentPart::Text(_), .. },
            Part { content: ContentPart::Refusal(_), .. }
        ]))
    );
    let wire = chat
        .encode_request(
            &refused,
            "m",
            &morphiecore::protocol::openai::TargetContract::full(),
        )
        .unwrap();
    assert_eq!(
        wire["messages"][1]["content"],
        json!([{"type":"text","text":"partial"},{"type":"refusal","refusal":"no"}])
    );
}
