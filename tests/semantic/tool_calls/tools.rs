//! Independent oracles for the function-tool migration, without providers or production routing.
use crate::events_support::{apply, close, created, encode, envelope, metadata, start, terminal};
use morphiecore::{
    protocol::{
        conversion::generation::RepresentationError,
        fidelity::FidelityRecords,
        openai::{
            DecodedRequest, GenerationApi, TargetContract as Contract, chat, events,
            lowering::{lower_request, lower_response},
            responses,
        },
    },
    semantic::{task::generation::*, value::Text},
};
use serde_json::{Value, json};

fn text(s: &str) -> Text {
    Text::new(s, "fixture", 1024).unwrap()
}
// Official Responses ToolParam/AdditionalTools unions and ordered-additions guide:
// docs/refs/2026-10-27-client-ir-boundaries.md#tool-contract-qualification-2026-11-04
fn definition_batch() -> Value {
    json!({"type":"additional_tools","id":"batch-a","role":"developer","tools":[
        {"type":"function","name":"read","strict":true,"parameters":{
            "type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}},
        {"type":"custom","name":"note","format":{"type":"text"}},
        {"type":"namespace","name":"ops","description":"Scoped tools","tools":[
            {"type":"function","name":"read","strict":false,"parameters":{"type":"object"}}]}
    ]})
}
#[test]
fn responses_batches_keep_membership_identity_order_and_shared_admission() {
    let batch = definition_batch();
    let source = json!({"input":[
        {"role":"user","content":"read"},
        batch,
        {"type":"additional_tools","id":"empty-batch","role":"developer","tools":[]}
    ],"tool_choice":{"type":"allowed_tools","mode":"required","tools":[
        {"type":"function","name":"read"},{"type":"custom","name":"note"}
    ]},"parallel_tool_calls":false});
    let decoded = responses::decode_generation(&source).unwrap();
    assert_eq!(decoded.semantic.items().len(), 3);
    let (owner, Item::ToolBatch(batch)) = &decoded.semantic.items()[1] else {
        panic!("one batch owner")
    };
    assert_eq!(batch.tools.len(), 3);
    assert_eq!(decoded.fidelity.response_item_id(*owner), Some("batch-a"));
    assert_eq!(decoded.semantic.function_tools().count(), 2);
    assert!(
        decoded
            .semantic
            .features()
            .contains_key(&Feature::CustomTools)
    );
    let encoded = request_wire(&decoded, GenerationApi::Responses);
    assert_eq!(encoded["input"][1], source["input"][1]);
    assert_eq!(encoded["input"][2], source["input"][2]);
    assert_eq!(encoded["tool_choice"], source["tool_choice"]);
    assert_eq!(encoded["parallel_tool_calls"], false);
    assert!(encoded.get("tools").is_none());
    for feature in [Feature::Tools, Feature::StrictTools, Feature::CustomTools] {
        let mut contract = Contract::full();
        contract.dispositions.set_native(feature, false);
        assert!(
            lower_request(
                &decoded.semantic,
                &decoded.fidelity,
                GenerationApi::Responses,
                contract
            )
            .is_err()
        );
    }
    let mut invalid_strict = source.clone();
    invalid_strict["input"][1]["tools"][0]["parameters"]
        .as_object_mut()
        .unwrap()
        .remove("required");
    let invalid_strict = responses::decode_generation(&invalid_strict).unwrap();
    assert!(
        lower_request(
            &invalid_strict.semantic,
            &invalid_strict.fidelity,
            GenerationApi::Responses,
            Contract::full()
        )
        .is_err()
    );
    assert_eq!(
        request_wire(&decoded, GenerationApi::Responses)["input"][1],
        source["input"][1]
    );
    let mut items = decoded.semantic.items().to_vec();
    let Item::ToolBatch(batch) = &mut items[1].1 else {
        panic!("batch")
    };
    let ToolDefinition::Namespace(group) = &mut batch.tools[2] else {
        panic!("namespace")
    };
    let ToolDefinition::Function(tool) = &mut group.tools[0] else {
        panic!("function")
    };
    tool.cache = Some(PrefixCacheTtl::FiveMinutes);
    tool.eager_input = Some(true);
    tool.strict = FunctionStrictness::Explicit(true);
    tool.parameters = Some(
        json!({"type":"object","properties":{},"required":[],"additionalProperties":false}).into(),
    );
    let source = decoded.semantic.clone().with_items(items).unwrap();
    let before = source.clone();
    let projected = lower_request(
        &source,
        &decoded.fidelity,
        GenerationApi::Responses,
        Contract::full(),
    )
    .unwrap();
    assert!(
        projected
            .semantic()
            .function_tools()
            .all(|tool| tool.cache.is_none() && tool.eager_input.is_none())
    );
    assert!(projected.projection().iter().any(|effect| effect.owner()
        == morphiecore::protocol::conversion::generation::Owner::item(source.items()[1].0)));
    let encoded = responses::encode_generation(&projected).unwrap();
    assert_eq!(encoded["input"][1]["tools"][2]["tools"][0]["strict"], true);
    assert_eq!(
        encoded["input"][1]["tools"][2]["tools"][0]["parameters"],
        json!({"type":"object","properties":{},"required":[],"additionalProperties":false})
    );
    assert_eq!(source, before);
}
#[test]
fn responses_message_dialects_preserve_inline_and_retained_tool_batches() {
    use morphiecore::protocol::openai::{Dialect, GenerationCodec};
    let batch = json!({"type":"additional_tools","id":"input-batch","role":"developer","tools":[
        {"type":"function","name":"read","strict":false,"parameters":{"type":"object"}}
    ]});
    let mut returned = batch.clone();
    returned["id"] = json!("returned-batch");
    returned["tools"][0]["name"] = json!("write");
    let source = json!({"model":"synthetic","input":[
        {"role":"developer","content":"Use the tools."},
        batch,
        {"role":"user","content":"read"}
    ]});
    let standard = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
    let request = standard
        .decode_request(&serde_json::to_vec(&source).unwrap())
        .unwrap();
    let output = responses::decode_response(&envelope("completed", json!([returned]))).unwrap();
    let continuation = standard
        .decode_client_history(
            &serde_json::to_vec(&json!({"model":"synthetic","input":"write"})).unwrap(),
            Some((&request.task, &output)),
        )
        .unwrap();
    for dialect in [Dialect::Zhipu, Dialect::DeepSeek] {
        let codec = GenerationCodec::new(GenerationApi::Responses, dialect, None);
        for (request, retained) in [(&request, false), (&continuation, true)] {
            let wire = codec
                .encode_request(request, "synthetic", &Contract::full())
                .unwrap();
            assert_eq!(wire["input"][1], batch, "{dialect:?}");
            assert_eq!(wire["input"][0]["content"], "Use the tools.");
            assert_eq!(
                wire["input"][0]["role"],
                if dialect == Dialect::DeepSeek {
                    "system"
                } else {
                    "developer"
                }
            );
            if dialect == Dialect::Zhipu {
                assert_eq!(wire["input"][0]["type"], "message");
            }
            if retained {
                assert_eq!(wire["input"][3], returned, "{dialect:?}");
                assert_eq!(wire["input"][4]["content"][0]["text"], "write");
            }
        }
        let decoded = codec
            .decode_request(&serde_json::to_vec(&source).unwrap())
            .unwrap();
        assert!(matches!(
            &decoded.task.semantic.items()[1].1,
            Item::ToolBatch(batch) if batch.role == ToolBatchRole::Developer
        ));
        if dialect == Dialect::DeepSeek {
            assert!(matches!(
                &decoded.task.semantic.items()[0].1,
                Item::Message(message) if message.role == MessageRole::User
            ));
        } else {
            assert!(matches!(
                &decoded.task.semantic.items()[0].1,
                Item::Instruction(_)
            ));
        }
    }
}
#[test]
fn native_batches_preserve_old_arguments_without_choosing_a_schema_winner() {
    let batch = |key: &str| {
        json!({"type":"additional_tools","role":"developer","tools":[
            {"type":"function","name":"read","strict":false,"parameters":{
                "type":"object","properties":{key:{"type":"integer"}},"required":[key]}}
        ]})
    };
    let source = json!({"input":[batch("old"),
        {"type":"function_call","id":"historical-call","call_id":"call","name":"read","arguments":"{ \"old\":9007199254740993 }"},
        {"type":"function_call_output","call_id":"call","output":"old-result"},
        batch("new")
    ],"tool_choice":{"type":"function","name":"read"}});
    let decoded = responses::decode_generation(&source).unwrap();
    let encoded = request_wire(&decoded, GenerationApi::Responses);
    assert_eq!(encoded["input"], source["input"]);
}
#[test]
fn responses_batch_presence_bounds_and_choice_follow_the_input_contract() {
    let batch = definition_batch();
    for id in [None, Some(Value::Null), Some(json!("batch-a"))] {
        let mut batch = batch.clone();
        if let Some(id) = &id {
            batch["id"] = id.clone();
        } else {
            batch.as_object_mut().unwrap().remove("id");
        }
        let decoded = responses::decode_generation(&json!({"input":[batch]})).unwrap();
        let encoded = request_wire(&decoded, GenerationApi::Responses);
        assert_eq!(
            encoded["input"][0]["id"].as_str(),
            id.as_ref().and_then(Value::as_str)
        );
    }
    for field in ["tools", "role"] {
        for value in [None, Some(Value::Null), Some(json!(17))] {
            let mut batch = batch.clone();
            if let Some(value) = value {
                batch[field] = value;
            } else {
                batch.as_object_mut().unwrap().remove(field);
            }
            assert!(responses::decode_generation(&json!({"input":[batch]})).is_err());
        }
    }
    let mut too_many = batch.clone();
    too_many["tools"] = (0..=MAX_TOOLS)
        .map(|i| json!({"type":"function","name":format!("f{i}")}))
        .collect();
    assert!(responses::decode_generation(&json!({"input":[too_many]})).is_err());
    assert!(responses::decode_generation(&json!({"input":[batch.clone(),batch]})).is_err());
    let empty = json!({"input":[{"type":"additional_tools","role":"developer","tools":[]}]});
    assert_eq!(
        responses::decode_generation(&empty)
            .unwrap()
            .semantic
            .items()
            .len(),
        1
    );
    let mut required = empty;
    required["tool_choice"] = json!("required");
    assert!(responses::decode_generation(&required).is_err());
}
#[test]
fn responses_returned_batches_keep_snapshot_roles_and_replay_authority() {
    use morphiecore::protocol::openai::{Dialect, GenerationCodec};
    for id in [None, Some(Value::Null)] {
        let mut batch = definition_batch();
        if let Some(id) = id {
            batch["id"] = id;
        } else {
            batch.as_object_mut().unwrap().remove("id");
        }
        assert!(responses::decode_response(&envelope("completed", json!([batch]))).is_err());
    }
    let input =
        responses::decode_generation(&json!({"input":[{"role":"user","content":"go"}]})).unwrap();
    let codec = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
    for role in ["developer", "system", "unknown"] {
        let mut batch = definition_batch();
        batch["role"] = json!(role);
        let response = envelope("completed", json!([batch]));
        let static_value = responses::decode_response(&response).unwrap();
        let representation = lower_response(
            &static_value.semantic,
            &static_value.fidelity,
            &static_value.metadata,
            GenerationApi::Responses,
            Contract::full(),
        )
        .unwrap();
        assert_eq!(
            responses::encode_response(&representation).unwrap()["output"],
            response["output"]
        );
        let frames = crate::wire::response_events(response.clone());
        let mut decoder = events::EventDecoder::new(GenerationApi::Responses);
        let mut semantic_events = Vec::new();
        for frame in &frames {
            semantic_events.extend(decoder.push(frame).unwrap());
        }
        decoder.finish().unwrap();
        let streamed = decoder.materialize().unwrap();
        assert_eq!(streamed.semantic.items(), static_value.semantic.items());
        let rendered = encode(
            &semantic_events,
            GenerationApi::Responses,
            &streamed.fidelity,
        );
        assert_eq!(
            rendered.last().unwrap()["response"]["output"],
            response["output"]
        );
        let continuation = json!({"model":"synthetic","input":[{"role":"user","content":"next"}]});
        let managed = codec.decode_client_history(
            &serde_json::to_vec(&continuation).unwrap(),
            Some((&input, &static_value)),
        );
        assert_eq!(managed.is_ok(), role == "developer");
        let manual = responses::decode_generation(&json!({"input":response["output"]}));
        assert_eq!(manual.is_ok(), role == "developer");
        let mut missing_done = events::EventDecoder::new(GenerationApi::Responses);
        for frame in frames.iter().take(2) {
            missing_done.push(frame).unwrap();
        }
        assert!(missing_done.finish().is_err());
        let mut changed = events::EventDecoder::new(GenerationApi::Responses);
        changed.push(&frames[0]).unwrap();
        changed.push(&frames[1]).unwrap();
        let mut done = frames[2].clone();
        done["item"]["id"] = json!("different-batch");
        assert!(changed.push(&done).is_err());
    }
}
#[test]
fn returned_batch_custom_carrier_is_checked_at_open_and_final_snapshot() {
    let mut batch = definition_batch();
    batch["tools"] = json!([batch["tools"][2].clone()]);
    batch["tools"][0]["tools"] = json!([{"type":"custom","name":"note","format":{"type":"text"}}]);
    let decoded = responses::decode_response(&envelope("completed", json!([batch]))).unwrap();
    let mut contract = Contract::full();
    contract
        .dispositions
        .set_native(Feature::CustomTools, false);
    assert!(
        lower_response(
            &decoded.semantic,
            &decoded.fidelity,
            &decoded.metadata,
            GenerationApi::Responses,
            contract.clone()
        )
        .is_err()
    );
    let Item::ToolBatch(batch) = &decoded.semantic.items()[0].1 else {
        panic!("batch")
    };
    for opening in [true, false] {
        let mut encoder = events::EventEncoder::new(GenerationApi::Responses, metadata())
            .unwrap()
            .with_contract(contract.clone());
        encoder
            .encode(&StreamEvent::Started, &decoded.fidelity)
            .unwrap();
        let start = start(
            1,
            ItemKind::ToolBatch(if opening {
                batch.clone()
            } else {
                ToolBatch {
                    tools: vec![],
                    role: ToolBatchRole::Developer,
                }
            }),
        );
        let result = encoder.encode(&start, &decoded.fidelity);
        if opening {
            assert!(result.is_err());
        } else {
            result.unwrap();
            assert!(
                encoder
                    .encode(
                        &StreamEvent::ToolBatchSnapshot {
                            item: ItemId::new(1),
                            batch: batch.clone()
                        },
                        &decoded.fidelity
                    )
                    .is_err()
            );
        }
    }
}
#[test]
fn incomplete_terminal_cannot_close_an_unfinished_declaration_batch() {
    let mut batch = definition_batch();
    batch["tools"] = json!([]);
    let mut response = envelope("incomplete", json!([batch]));
    response["incomplete_details"] = json!({"reason":"max_output_tokens"});
    let frames = crate::wire::response_events(response);
    let mut decoder = events::EventDecoder::new(GenerationApi::Responses);
    for frame in &frames {
        decoder.push(frame).unwrap();
    }
    decoder.finish().unwrap();
    let mut missing_done = events::EventDecoder::new(GenerationApi::Responses);
    for frame in frames.iter().take(2) {
        missing_done.push(frame).unwrap();
    }
    assert!(missing_done.push(frames.last().unwrap()).is_err());
    assert!(missing_done.materialize().is_err());
    let prefix = [
        StreamEvent::Started,
        start(
            1,
            ItemKind::ToolBatch(ToolBatch {
                tools: vec![],
                role: ToolBatchRole::Developer,
            }),
        ),
    ];
    let state = apply(&prefix).unwrap();
    assert!(reduce(state, close(1, ItemLifecycle::Incomplete)).is_err());
}
fn chat_history() -> Value {
    json!({"messages":[
        {"role":"user","content":"weather?"},
        {"role":"assistant","content":"Checking.","tool_calls":[
            {"id":"call_a","type":"function","function":{"name":"weather","arguments":"{ \"city\": \"Paris\" }"}},
            {"id":"call_b","type":"function","function":{"name":"weather","arguments":"{\"city\":\"Rome\"}"}}
        ]},
        {"role":"tool","tool_call_id":"call_b","content":""},
        {"role":"tool","tool_call_id":"call_a","content":"sunny"}
    ],"tools":[{"type":"function","function":{"name":"weather","description":"","parameters":{"type":"object","properties":{"city":{"type":"string"}}},"strict":false}}],
    "tool_choice":{"type":"function","function":{"name":"weather"}},"parallel_tool_calls":false})
}
fn request_wire(d: &DecodedRequest, profile: GenerationApi) -> Value {
    let t = lower_request(&d.semantic, &d.fidelity, profile, Contract::full()).unwrap();
    match profile {
        GenerationApi::Chat => chat::encode_generation(&t).unwrap(),
        GenerationApi::Responses => responses::encode_generation(&t).unwrap(),
    }
}
#[test]
fn reported_tool_choices_validate_shape_without_current_request_inventory() {
    let reference = |name: &str| ToolReference {
        kind: ToolKind::Function,
        name: text(name),
        namespace: None,
    };
    let choices = [
        ToolChoice::Allowed {
            required: false,
            tools: vec![],
        },
        ToolChoice::Allowed {
            required: false,
            tools: vec![reference("read"), reference("read")],
        },
        ToolChoice::Allowed {
            required: false,
            tools: (0..=MAX_TOOLS)
                .map(|i| reference(&format!("f{i}")))
                .collect(),
        },
        ToolChoice::Qualified(reference("read")),
    ];
    for choice in choices {
        let settings = GenerationSettings {
            tool_choice: Some(choice),
            ..Default::default()
        };
        assert!(settings.validate().is_err());
    }
}

#[test]
fn tool_defaults_keep_non_strict_and_conditional_projection_distinct() {
    let source = json!({"messages":[{"role":"user","content":"go"}],
            "tools":[{"type":"function","function":{"name":"lookup","parameters":{
                "type":"object","properties":{"key":{"type":"string"}}}}}]});
    let decoded = chat::decode_generation(&source).unwrap();
    assert!(
        matches!(&decoded.semantic.tools()[0], ToolDefinition::Function(t)
            if t.strict == FunctionStrictness::Omitted(StrictDefault::NonStrict))
    );
    let before = decoded.semantic.clone();
    assert_eq!(
        request_wire(&decoded, GenerationApi::Responses)["tools"],
        json!([{
            "type":"function","name":"lookup","strict":false,
            "parameters":{"type":"object","properties":{"key":{"type":"string"}}}
        }])
    );
    assert_eq!(decoded.semantic, before);
    assert_eq!(
        request_wire(&decoded, GenerationApi::Chat)["tools"],
        source["tools"]
    );
    let conditional = responses::decode_generation(&json!({"input":"go","tools":[{
        "type":"function","name":"lookup",
        "parameters":{"type":"object","properties":{"key":{"type":"string"}}}
    }]}))
    .unwrap();
    let lowered = lower_request(
        &conditional.semantic,
        &conditional.fidelity,
        GenerationApi::Chat,
        Contract::full(),
    )
    .unwrap();
    assert_eq!(lowered.projection().len(), 1);
    assert_eq!(
        chat::encode_generation(&lowered).unwrap()["tools"][0]["function"]["strict"],
        false
    );
}
fn expected_responses_history() -> Value {
    json!({"input":[
        {"type":"message","role":"user","content":[{"type":"input_text","text":"weather?"}]},
        {"id":"item_2","type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"Checking.","annotations":[]}]},
        {"type":"function_call","call_id":"call_a","name":"weather","arguments":"{ \"city\": \"Paris\" }"},
        {"type":"function_call","call_id":"call_b","name":"weather","arguments":"{\"city\":\"Rome\"}"},
        {"type":"function_call_output","call_id":"call_b","output":""},
        {"type":"function_call_output","call_id":"call_a","output":"sunny"}
    ],"tools":[{"type":"function","name":"weather","description":"","parameters":{"type":"object","properties":{"city":{"type":"string"}}},"strict":false}],
    "tool_choice":{"type":"function","name":"weather"},"parallel_tool_calls":false})
}
#[test]
fn chat_tool_text_arrays_preserve_parts_and_project_only_final_ir() {
    for parts in [
        json!([]),
        json!([{"type":"text","text":""}]),
        json!([{"type":"text","text":"sunny"},{"type":"text","text":""}]),
    ] {
        let mut c = chat_history();
        c["messages"][3]["content"] = parts.clone();
        let mut r = expected_responses_history();
        r["input"][5]["output"] = json!(
            parts
                .as_array()
                .unwrap()
                .iter()
                .map(|p| json!({"type":"input_text","text":p["text"]}))
                .collect::<Vec<_>>()
        );
        let mut decoded = chat::decode_generation(&c).unwrap();
        let Item::ToolResult(result) = &decoded.semantic.items()[5].1 else {
            panic!("result");
        };
        let ToolOutput::Parts(values) = &result.output else {
            panic!("parts");
        };
        assert_eq!(result.required_call_id().unwrap(), "call_a");
        assert_eq!(values.len(), parts.as_array().unwrap().len());
        // The same result facts can be reported independently in a response.
        assert!(
            GenerationResponse::new(
                vec![decoded.semantic.items()[5].clone()],
                Outcome::Completed
            )
            .is_ok()
        );
        if !values.is_empty() {
            let mut cached = r.clone();
            cached["input"][5]["output"][0]["prompt_cache_breakpoint"] = json!({"mode":"explicit"});
            let cached = responses::decode_generation(&cached).unwrap();
            let projected = lower_request(
                &cached.semantic,
                &cached.fidelity,
                GenerationApi::Chat,
                Contract::full(),
            )
            .unwrap();
            assert!(!projected.semantic().items()[5].1.has_cache_breakpoints());
        }
        assert_eq!(request_wire(&decoded, GenerationApi::Chat), c);
        assert_eq!(request_wire(&decoded, GenerationApi::Responses), r);
        let independent = responses::decode_generation(&r).unwrap();
        assert_eq!(request_wire(&independent, GenerationApi::Responses), r);
        // Responses call items have no Chat carrier-message association.
        let projected = request_wire(
            &responses::decode_generation(&r).unwrap(),
            GenerationApi::Chat,
        );
        let result = projected["messages"].as_array().unwrap().last().unwrap();
        assert_eq!(result["tool_call_id"], "call_a");
        assert_eq!(result["content"], parts);
        let mut items = decoded.semantic.items().to_vec();
        let Item::ToolResult(result) = &mut items[5].1 else {
            unreachable!()
        };
        result.output = ToolOutput::Parts(vec![
            Part::new(PartId::new(900), InputPart::Text(text("replacement"))),
            Part::new(PartId::new(901), InputPart::Text(text("inserted"))),
        ]);
        decoded.semantic = decoded.semantic.with_items(items).unwrap();
        assert_eq!(
            request_wire(&decoded, GenerationApi::Chat)["messages"][3]["content"],
            json!([{"type":"text","text":"replacement"},{"type":"text","text":"inserted"}])
        );
        let mut items = decoded.semantic.items().to_vec();
        let Item::ToolResult(result) = &mut items[5].1 else {
            unreachable!()
        };
        result.output = ToolOutput::Parts(vec![]);
        decoded.semantic = decoded.semantic.with_items(items).unwrap();
        assert_eq!(
            request_wire(&decoded, GenerationApi::Chat)["messages"][3]["content"],
            json!([])
        );
    }
    for bad in [
        Value::Null,
        json!([{"type":"image_url","image_url":{"url":"https://example.test/a"}}]),
        json!([{"type":"text","text":1}]),
        json!([{"type":"text","text":"x","extra":true}]),
    ] {
        let mut c = chat_history();
        c["messages"][3]["content"] = bad;
        assert!(chat::decode_generation(&c).is_err());
    }
}

#[test]
fn chat_allowed_functions_use_their_own_shell_and_keep_selection_authoritative() {
    for mode in ["auto", "required"] {
        let mut c = chat_history();
        c["messages"] = json!([{"role":"user","content":"weather?"}]);
        c["tool_choice"] = json!({"type":"allowed_tools","allowed_tools":{"mode":mode,"tools":[{"type":"function","function":{"name":"weather"}}]}});
        let mut r = expected_responses_history();
        r["input"].as_array_mut().unwrap().truncate(1);
        r["tool_choice"] = json!({"type":"allowed_tools","mode":mode,"tools":[{"type":"function","name":"weather"}]});
        let mut d = chat::decode_generation(&c).unwrap();
        assert_eq!(
            d.semantic.tool_choice(),
            Some(&ToolChoice::Allowed {
                required: mode == "required",
                tools: vec![ToolReference {
                    kind: ToolKind::Function,
                    name: text("weather"),
                    namespace: None,
                }]
            })
        );
        assert_eq!(request_wire(&d, GenerationApi::Chat), c);
        assert_eq!(request_wire(&d, GenerationApi::Responses), r);
        assert_eq!(
            request_wire(
                &responses::decode_generation(&r).unwrap(),
                GenerationApi::Chat
            )["tool_choice"],
            c["tool_choice"]
        );
        let mut s = d.semantic.settings().clone();
        s.tool_choice = Some(ToolChoice::None);
        d.semantic = d.semantic.with_settings(s).unwrap();
        assert_eq!(request_wire(&d, GenerationApi::Chat)["tool_choice"], "none");
        let mut s = d.semantic.settings().clone();
        s.tool_choice = None;
        d.semantic = d.semantic.with_settings(s).unwrap();
        assert!(
            request_wire(&d, GenerationApi::Chat)
                .get("tool_choice")
                .is_none()
        );
        for refs in [
            json!([]),
            json!([{"type":"function","function":{"name":"absent"}}]),
            json!([{"type":"function","function":{"name":"weather"}},{"type":"function","function":{"name":"weather"}}]),
            json!([{"type":"custom","name":"weather"}]),
        ] {
            let mut bad = c.clone();
            bad["tool_choice"]["allowed_tools"]["tools"] = refs;
            assert!(chat::decode_generation(&bad).is_err());
        }
    }
}

#[test]
fn bounded_values_fidelity_collisions_and_codec_profile_mismatch_fail() {
    let mut wire = chat_history();
    wire["messages"][1]["tool_calls"][0]["function"]["arguments"] =
        json!("x".repeat(MAX_TEXT_BYTES + 1));
    assert!(chat::decode_generation(&wire).is_err());
    let mut f = FidelityRecords::default();
    f.record_response_item_id(ItemId::new(1), "same").unwrap();
    assert!(f.record_response_item_id(ItemId::new(2), "same").is_err());
    let d = chat::decode_generation(&chat_history()).unwrap();
    let t = lower_request(
        &d.semantic,
        &d.fidelity,
        GenerationApi::Chat,
        Contract::full(),
    )
    .unwrap();
    assert!(responses::encode_generation(&t).is_err());
}

#[test]
fn semantic_construction_cannot_bypass_identifier_or_control_validation() {
    let empty = Text::allowing_empty("", "fixture", 1).unwrap();
    let call = Item::ToolCall(ToolCall {
        replay: None,
        call_id: empty.clone(),
        cache: None,
        name: text("f"),
        arguments: "{}".into(),
        status: ItemLifecycle::Completed,
        context: CallContext::default(),
    });
    assert!(
        GenerationRequest::new(vec![(ItemId::new(1), call)], GenerationControls::default())
            .is_err()
    );
    let d = chat::decode_generation(&chat_history()).unwrap();
    let tool = ToolDefinition::Function(FunctionTool {
        name: empty,
        output_schema: None,
        description: None,
        parameters: None,
        strict: FunctionStrictness::Explicit(false),
        dispatch: ToolDispatch::default(),
        cache: None,
        input_examples: None,
        eager_input: None,
    });
    assert!(
        d.semantic
            .clone()
            .with_tools(vec![tool], ToolChoice::Auto)
            .is_err()
    );
    let mut controls = GenerationControls::default();
    controls.max_output_tokens = Some(0);
    assert!(GenerationRequest::new(d.semantic.items().to_vec(), controls).is_err());
    let mut settings = d.semantic.settings().clone();
    let ToolDefinition::Function(tool) = &mut settings.tools.as_mut().unwrap()[0] else {
        panic!("function tool")
    };
    tool.dispatch.allowed_callers = Some(vec![]);
    assert!(matches!(
        d.semantic.with_settings(settings),
        Err(GenerationError::InvalidToolDefinition)
    ));
}
#[test]
fn active_tool_dispatch_and_instruction_lifecycle_round_trip_without_chat_projection() {
    use morphiecore::protocol::openai::{Dialect, GenerationCodec};
    let adapter = GenerationCodec::new(GenerationApi::Chat, Dialect::MorphieCore, None);
    let decode = |value: &Value| adapter.decode_client_request(value.to_string().as_bytes());
    let mut ignored = chat_history();
    ignored["model"] = json!("m");
    let original = decode(&ignored).unwrap();
    ignored["tools"][0]["function"]["output_schema"] = json!({"type":"string"});
    ignored["messages"][1]["tool_calls"][0]["status"] = json!("in_progress");
    assert_eq!(decode(&ignored).unwrap().task, original.task);
    for key in ["async", "defer_loading", "allowed_callers"] {
        let mut active = ignored.clone();
        active["tools"][0]["function"][key] = if key == "allowed_callers" {
            json!(["programmatic"])
        } else {
            json!(true)
        };
        assert_eq!(decode(&active).unwrap().task, original.task, "{key}");
    }
    let mut legacy = ignored.clone();
    legacy["messages"][1]["function_call"] = json!({"name":"lookup","arguments":"{}"});
    assert!(decode(&legacy).is_err());
    let mut caller = ignored.clone();
    caller["messages"][1]["tool_calls"][0]["caller"] = json!({"type":"program","caller_id":"p"});
    assert_eq!(decode(&caller).unwrap().task, original.task);
    let mut audio = ignored;
    audio["messages"][0]["audio"] = json!({"id":"a"});
    assert!(decode(&audio).is_err());
    let source = json!({"input":[
        {"type":"message","id":"ins","role":"developer","status":"in_progress","content":[{"type":"input_text","text":"hold"}]},
        {"type":"program","id":"p","call_id":"prog","code":"return lookup();","fingerprint":"fp"},
        {"type":"function_call","call_id":"c","name":"lookup","arguments":"{}","namespace":"pkg","async":true,"caller":{"type":"program","caller_id":"prog"}},
        {"type":"function_call_output","call_id":"c","output":"ok","caller":{"type":"program","caller_id":"prog"}},
        {"type":"program_output","id":"po","call_id":"prog","result":"ok","status":"completed"}
    ],"tools":[{"type":"function","name":"lookup","async":true,"defer_loading":true,"allowed_callers":["programmatic"]}]});
    let decoded = responses::decode_generation(&source).unwrap();
    let mut orphan = source.clone();
    orphan["input"].as_array_mut().unwrap().remove(1);
    assert!(matches!(
        responses::decode_generation(&orphan),
        Err(morphiecore::protocol::CodecError::Semantic(
            GenerationError::InvalidDependency
        ))
    ));
    let mut mismatch = source.clone();
    mismatch["input"][3]["caller"]["caller_id"] = json!("different-program");
    assert!(matches!(
        responses::decode_generation(&mismatch),
        Err(morphiecore::protocol::CodecError::Semantic(
            GenerationError::InvalidToolResult
        ))
    ));
    let Item::Instruction(instruction) = &decoded.semantic.items()[0].1 else {
        panic!("instruction")
    };
    assert_eq!(instruction.status, Some(ItemLifecycle::InProgress));
    let Item::ToolCall(call) = &decoded.semantic.items()[2].1 else {
        panic!("call")
    };
    assert_eq!(
        call.context.namespace.as_ref().map(|v| v.as_str()),
        Some("pkg")
    );
    assert!(call.context.async_call);
    assert!(matches!(
        call.context.caller,
        Some(CallOrigin::Program { .. })
    ));
    let ToolDefinition::Function(tool) = &decoded.semantic.tools()[0] else {
        panic!("tool")
    };
    assert!(tool.dispatch.async_call && tool.dispatch.defer_loading);
    assert_eq!(
        tool.dispatch.allowed_callers,
        Some(vec![CallerMode::Programmatic])
    );
    let encoded = request_wire(&decoded, GenerationApi::Responses);
    assert_eq!(encoded["input"][0]["status"], json!("in_progress"));
    assert_eq!(encoded["input"][0]["id"], json!("ins"));
    // An input instruction message has no id field; none is minted for it.
    let mut unnamed = source.clone();
    unnamed["input"][0].as_object_mut().unwrap().remove("id");
    let unnamed = request_wire(
        &responses::decode_generation(&unnamed).unwrap(),
        GenerationApi::Responses,
    );
    assert_eq!(unnamed["input"][0]["status"], json!("in_progress"));
    assert!(unnamed["input"][0].get("id").is_none());
    // Assistant `input_text` is the SDK's spelling; the API accepts only output text.
    let assistant = responses::decode_generation(&json!({"input":[
            {"role":"user","content":"hi"},
            {"role":"assistant","content":[{"type":"input_text","text":"hello"}]},
            {"role":"user","content":"again"}]}))
    .unwrap();
    let assistant = request_wire(&assistant, GenerationApi::Responses);
    assert_eq!(
        assistant["input"][1]["content"][0]["type"],
        json!("output_text")
    );
    assert_eq!(assistant["input"][1]["content"][0]["text"], json!("hello"));
    assert_eq!(encoded["input"][2]["namespace"], json!("pkg"));
    assert_eq!(encoded["input"][2]["caller"]["type"], json!("program"));
    assert_eq!(encoded["input"][3]["caller"], encoded["input"][2]["caller"]);
    assert_eq!(encoded["tools"][0]["defer_loading"], json!(true));
    assert!(
        lower_request(
            &decoded.semantic,
            &decoded.fidelity,
            GenerationApi::Chat,
            Contract::full()
        )
        .is_err()
    );
    let completed =
        responses::decode_generation(&json!({"input":[{"role":"developer","content":"done"}]}))
            .unwrap();
    let Item::Instruction(instruction) = &completed.semantic.items()[0].1 else {
        panic!("complete instruction")
    };
    assert_eq!(instruction.status, None);
    assert!(
        request_wire(&completed, GenerationApi::Responses)["input"][0]
            .get("status")
            .is_none()
    );
}
fn program_history() -> Value {
    json!({"input":[
        {"type":"message","role":"user","content":[{"type":"input_text","text":"run"}]},
        {"type":"program","id":"pg_1","call_id":"call_p","code":"const x = 1; return x;","fingerprint":"fp-Ω-1"},
        {"type":"program_output","id":"pg_2","call_id":"call_p","result":"1","status":"completed"}
    ]})
}
fn program_items() -> Vec<(ItemId, Item)> {
    vec![
        (
            ItemId::new(1),
            Item::Program(Program {
                call_id: text("call_p"),
                code: "const x = 1; return x;".into(),
                fingerprint: "fp-Ω-1".into(),
            }),
        ),
        (
            ItemId::new(2),
            Item::ProgramOutput(ProgramOutput {
                call_id: text("call_p"),
                result: "1".into(),
                status: ItemLifecycle::Completed,
            }),
        ),
    ]
}
#[test]
fn program_output_requires_terminal_status_and_complete_opaque_fields() {
    for (index, key) in [(1, "fingerprint"), (1, "id"), (2, "status"), (2, "result")] {
        let mut wire = program_history();
        wire["input"][index].as_object_mut().unwrap().remove(key);
        assert!(responses::decode_generation(&wire).is_err());
    }
    for status in ["in_progress", "failed", "queued"] {
        let mut wire = program_history();
        wire["input"][2]["status"] = json!(status);
        assert!(responses::decode_generation(&wire).is_err());
    }
    let mut wire = program_history();
    wire["input"][1]["fingerprint"] = Value::Null;
    assert!(responses::decode_generation(&wire).is_err());
    let mut items = program_items();
    if let Item::ProgramOutput(o) = &mut items[1].1 {
        o.status = ItemLifecycle::InProgress;
    }
    assert!(matches!(
        GenerationRequest::new(items, GenerationControls::default()),
        Err(GenerationError::InvalidProgramOutput)
    ));
}
#[test]
fn program_items_have_no_chat_projection() {
    let d = responses::decode_generation(&program_history()).unwrap();
    assert_eq!(
        lower_request(
            &d.semantic,
            &d.fidelity,
            GenerationApi::Chat,
            Contract::full()
        )
        .err(),
        Some(RepresentationError::Tools.into())
    );
    let r = GenerationResponse::new(
        vec![(ItemId::new(1), program_items()[0].1.clone())],
        Outcome::Completed,
    )
    .unwrap();
    assert!(matches!(
        lower_response(
            &r,
            &FidelityRecords::default(),
            &metadata(),
            GenerationApi::Chat,
            Contract::full()
        )
        .map_err(|error| error.cause),
        Err(morphiecore::protocol::ConversionCause::Representation(
            RepresentationError::Tools
        ))
    ));
    let mut e = events::EventEncoder::new(GenerationApi::Chat, metadata()).unwrap();
    assert!(
        e.encode(&start(1, program_kind()), &FidelityRecords::default())
            .is_err()
    );
}
fn program_kind() -> ItemKind {
    ItemKind::Program {
        call_id: text("call_p"),
        code: "c()".into(),
        fingerprint: "fp-1".into(),
    }
}
#[test]
fn program_items_are_atomic_event_items_with_owner_bound_snapshots() {
    let mut e = vec![StreamEvent::Started, start(1, program_kind())];
    e.push(close(1, ItemLifecycle::Completed));
    e.push(terminal(StreamTerminal::Completed));
    let wire = encode(&e, GenerationApi::Responses, &FidelityRecords::default());
    let added: Vec<_> = wire
        .iter()
        .filter(|v| v["type"] == "response.output_item.added")
        .collect();
    assert_eq!(
        added[0]["item"],
        json!({"id":"item_1","type":"program","call_id":"call_p","code":"c()","fingerprint":"fp-1"})
    );
    let done: Vec<_> = wire
        .iter()
        .filter(|v| v["type"] == "response.output_item.done")
        .collect();
    assert_eq!(done[0]["item"].get("status"), None);
    assert_eq!(
        wire.last().unwrap()["response"]["output"][0]["type"],
        json!("program")
    );
    assert!(apply(&e).is_ok());
    let mut dup = e.clone();
    dup.insert(
        2,
        start(
            3,
            ItemKind::Program {
                call_id: text("call_p"),
                code: "x".into(),
                fingerprint: "fp-2".into(),
            },
        ),
    );
    assert!(apply(&dup).is_err());
    // A terminal-only atomic opener waits for observed status, not a fabricated
    // in-progress value. Missing closure still fails.
    let mut encoder = events::EventEncoder::new(GenerationApi::Responses, metadata()).unwrap();
    let fidelity = FidelityRecords::default();
    encoder.encode(&StreamEvent::Started, &fidelity).unwrap();
    assert!(
        encoder
            .encode(
                &start(
                    1,
                    ItemKind::ProgramOutput {
                        call_id: text("call_p"),
                        result: "1".into(),
                    }
                ),
                &fidelity
            )
            .unwrap()
            .is_empty()
    );
    assert!(encoder.finish().is_err());
    let wire = encoder
        .encode(&close(1, ItemLifecycle::Completed), &fidelity)
        .unwrap();
    assert_eq!(wire.len(), 2);
    assert_eq!(wire[0]["type"], "response.output_item.added");
    assert_eq!(wire[0]["item"]["status"], "completed");
    assert_eq!(wire[0]["item"], wire[1]["item"]);
}
#[test]
fn program_output_waits_for_status_without_reordering_following_items() {
    for status in [ItemLifecycle::Completed, ItemLifecycle::Incomplete] {
        let fidelity = FidelityRecords::default();
        let mut encoder = events::EventEncoder::new(GenerationApi::Responses, metadata()).unwrap();
        let mut wire = encoder.encode(&StreamEvent::Started, &fidelity).unwrap();
        let mut held = vec![
            start(
                1,
                ItemKind::ProgramOutput {
                    call_id: text("c"),
                    result: "value".into(),
                },
            ),
            start(2, ItemKind::Message { phase: None }),
        ];
        held.extend(crate::events_support::part(2, 2, PartKind::Text, "after"));
        held.push(close(2, ItemLifecycle::Completed));
        for event in held {
            assert!(encoder.encode(&event, &fidelity).unwrap().is_empty());
        }
        wire.extend(encoder.encode(&close(1, status), &fidelity).unwrap());
        wire.extend(
            encoder
                .encode(
                    &terminal(if status == ItemLifecycle::Completed {
                        StreamTerminal::Completed
                    } else {
                        StreamTerminal::Incomplete
                    }),
                    &fidelity,
                )
                .unwrap(),
        );
        encoder.finish().unwrap();
        let added: Vec<_> = wire
            .iter()
            .filter(|e| e["type"] == "response.output_item.added")
            .collect();
        assert_eq!(added[0]["output_index"], 0);
        assert_eq!(added[1]["output_index"], 1);
        assert_eq!(
            added[0]["item"],
            json!({"type":"program_output","id":"item_1",
            "call_id":"c","result":"value","status":if status==ItemLifecycle::Completed {"completed"} else {"incomplete"}})
        );
        assert_eq!(
            wire.last().unwrap()["response"]["output"][0],
            added[0]["item"]
        );
        let mut decoder = events::EventDecoder::new(GenerationApi::Responses);
        for (index, frame) in wire.iter().enumerate() {
            assert_eq!(frame["sequence_number"], index);
            decoder.push(frame).unwrap();
        }
        assert_eq!(
            decoder.materialize().unwrap().semantic.items()[0]
                .1
                .lifecycle(),
            Some(status)
        );
    }
}
#[test]
fn program_event_stream_decodes_and_changed_or_unfinished_snapshots_fail() {
    let mut d = events::EventDecoder::new(GenerationApi::Responses);
    d.push(&created()).unwrap();
    d.push(&json!({"type":"response.output_item.added","output_index":0,"item":{"id":"pg_1","type":"program","call_id":"call_p","code":"c()","fingerprint":"fp-1"}})).unwrap();
    d.push(&json!({"type":"response.output_item.done","output_index":0,"item":{"id":"pg_1","type":"program","call_id":"call_p","code":"c()","fingerprint":"fp-1"}})).unwrap();
    d.push(&json!({"type":"response.output_item.added","output_index":1,"item":{"id":"pg_2","type":"program_output","call_id":"call_p","result":"1","status":"completed"}})).unwrap();
    d.push(&json!({"type":"response.output_item.done","output_index":1,"item":{"id":"pg_2","type":"program_output","call_id":"call_p","result":"1","status":"completed"}})).unwrap();
    let out = json!([
        {"id":"pg_1","type":"program","call_id":"call_p","code":"c()","fingerprint":"fp-1"},
        {"id":"pg_2","type":"program_output","call_id":"call_p","result":"1","status":"completed"}
    ]);
    d.push(&json!({"type":"response.completed","response":envelope("completed",out)}))
        .unwrap();
    let r = d.materialize().unwrap().semantic;
    assert_eq!(r.outcome(), Outcome::Completed);
    assert_eq!(r.continuation(), Continuation::Unreported);
    assert_eq!(r.items()[1].1, program_items()[1].1);
    let mut d = events::EventDecoder::new(GenerationApi::Responses);
    d.push(&created()).unwrap();
    d.push(&json!({"type":"response.output_item.added","output_index":0,"item":{"id":"pg_1","type":"program","call_id":"call_p","code":"c()","fingerprint":"fp-1"}})).unwrap();
    assert!(d
        .push(&json!({"type":"response.output_item.done","output_index":0,"item":{"id":"pg_1","type":"program","call_id":"call_p","code":"c()","fingerprint":"fp-OTHER"}}))
        .is_err());
    let mut d = events::EventDecoder::new(GenerationApi::Responses);
    d.push(&created()).unwrap();
    d.push(&json!({"type":"response.output_item.added","output_index":0,"item":{"id":"pg_2","type":"program_output","call_id":"call_p","result":"1","status":"completed"}})).unwrap();
    assert!(d
        .push(&json!({"type":"response.output_item.done","output_index":0,"item":{"id":"pg_2","type":"program_output","call_id":"call_p","result":"1"}}))
        .is_err());
    let mut d = events::EventDecoder::new(GenerationApi::Responses);
    d.push(&created()).unwrap();
    assert!(d
        .push(&json!({"type":"response.output_item.added","output_index":0,"item":{"id":"pg_2","type":"program_output","call_id":"call_p","result":"1"}}))
        .is_err());
}
