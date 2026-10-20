//! Independent field and transformation contracts for stateless Responses text.
use crate::wire;
use morphiecore::{
    protocol::openai::{
        DecodedRequest, Profile, TargetCaps as Contract, chat,
        events::EventDecoder,
        lowering::{lower_request, lower_response},
        responses::{self, envelope},
    },
    semantic::{
        task::generation::*,
        value::{Presence, ReplayOrigin, Text},
    },
};
use serde_json::{Value, json};
fn origin() -> ReplayOrigin {
    ReplayOrigin::new("synthetic-loopback").unwrap()
}
fn contract() -> Contract {
    let mut c = Contract::full();
    c.replay_origin = Some(origin());
    c
}
fn text(s: &str) -> Text {
    Text::allowing_empty(s, "synthetic", MAX_TEXT_BYTES).unwrap()
}
fn request_wire(d: &DecodedRequest) -> Value {
    responses::encode_generation(
        &lower_request(&d.semantic, &d.fidelity, Profile::Responses, contract()).unwrap(),
    )
    .unwrap()
}
fn chat_wire(d: &DecodedRequest) -> Value {
    chat::encode_generation(
        &lower_request(&d.semantic, &d.fidelity, Profile::Chat, contract()).unwrap(),
    )
    .unwrap()
}

#[test]
fn audit_protected_response_details_and_access_policy_are_not_ignored() {
    use morphiecore::protocol::{
        CodecError,
        openai::{Adapter, Dialect},
    };
    let adapter = Adapter::new(Profile::Responses, Dialect::Standard, None);
    let mut body = json!({"id":"r","object":"response","created_at":1,"model":"m",
            "status":"failed","output":[],"error":{"code":"misalignment_policy_violation","message":"blocked"}});
    assert_eq!(
        adapter
            .decode_response(body.to_string().as_bytes())
            .unwrap()
            .semantic
            .outcome(),
        Outcome::Failed
    );
    for details in [
        json!({"error_type":"potentially_unintended_data_access","steer":{"message":"Use approved data only"}}),
        json!(7),
    ] {
        body["error"]["misalignment"] = details;
        assert!(
            matches!(adapter.decode_response(body.to_string().as_bytes()),
                Err(CodecError::Unsupported(field)) if field == "Responses misalignment details")
        );
    }
    let body = json!({"model":"m","input":"go","access_programs":{"cyber":"standard"}});
    assert!(
        matches!(adapter.decode_client_request(body.to_string().as_bytes()),
            Err(CodecError::Unsupported(field)) if field == "Responses access_programs")
    );
    let mut reported = wire::response(2);
    reported["access_programs"] = json!({"cyber":"standard"});
    assert!(
        matches!(adapter.decode_response(reported.to_string().as_bytes()),
            Err(CodecError::Unsupported(field)) if field == "Responses access_programs")
    );
    for late in [false, true] {
        let mut values = wire::events(2);
        let index = if late { values.len() - 1 } else { 0 };
        values[index]["response"]["access_programs"] = json!({"cyber":"standard"});
        let mut decoder = EventDecoder::new(Profile::Responses);
        let mut rejected = false;
        for value in &values {
            match decoder.push(value) {
                Ok(events) => assert!(
                    !events
                        .iter()
                        .any(|e| matches!(e, StreamEvent::Terminal { .. }))
                ),
                Err(CodecError::Unsupported(field)) if field == "Responses access_programs" => {
                    rejected = true;
                    break;
                }
                Err(error) => panic!("wrong rejection: {error}"),
            }
        }
        assert!(rejected);
    }
    let body = json!({"model":"m","input":"go","unregistered_future_field":true});
    assert!(
        adapter
            .decode_client_request(body.to_string().as_bytes())
            .is_ok()
    );
}

#[test]
fn response_format_shells_map_to_one_owner_with_independent_shapes() {
    let closed = json!({"type":"object","properties":{"zeta":{"type":"string"}},"required":["zeta"],"additionalProperties":false});
    let full = OutputConstraint::JsonSchema {
        name: Some(text("answer")),
        description: Some(text("pick")),
        schema: closed.clone().into(),
        strict: Some(true),
    };
    let bare = OutputConstraint::JsonSchema {
        name: Some(text("answer")),
        description: None,
        schema: closed.clone().into(),
        strict: None,
    };
    for (format, chat_format, responses_text, structured) in [
        (None, None, None, false),
        (
            Some(Presence::Null),
            Some(Value::Null),
            Some(json!({"format":null})),
            false,
        ),
        (
            Some(Presence::Value(OutputConstraint::Text)),
            Some(json!({"type":"text"})),
            Some(json!({"format":{"type":"text"}})),
            false,
        ),
        (
            Some(Presence::Value(OutputConstraint::JsonObject)),
            Some(json!({"type":"json_object"})),
            Some(json!({"format":{"type":"json_object"}})),
            true,
        ),
        (
            Some(Presence::Value(full.clone())),
            Some(
                json!({"type":"json_schema","json_schema":{"name":"answer","description":"pick","schema":closed,"strict":true}}),
            ),
            Some(
                json!({"format":{"type":"json_schema","name":"answer","description":"pick","schema":closed,"strict":true}}),
            ),
            true,
        ),
        (
            Some(Presence::Value(bare.clone())),
            Some(json!({"type":"json_schema","json_schema":{"name":"answer","schema":closed}})),
            Some(json!({"format":{"type":"json_schema","name":"answer","schema":closed}})),
            true,
        ),
    ] {
        let options = TextOptions {
            presence: if format.is_some() {
                Presence::Value(())
            } else {
                Presence::Absent
            },
            format: format.unwrap_or(Presence::Absent),
            verbosity: Presence::Absent,
        };
        let mut source = json!({"messages":[{"role":"user","content":"hello"}]});
        if let Some(value) = &chat_format {
            source["response_format"] = value.clone();
        }
        let d = chat::decode_generation(&source).unwrap();
        assert_eq!(d.semantic.text_options(), &options);
        assert_eq!(chat_wire(&d).get("response_format"), chat_format.as_ref());
        let mut source = json!({"input":"hello"});
        if let Some(value) = &responses_text {
            source["text"] = value.clone();
        }
        let r = responses::decode_generation(&source).unwrap();
        assert_eq!(r.semantic.text_options(), &options);
        assert_eq!(request_wire(&r).get("text"), responses_text.as_ref());
        assert_eq!(
            GenerationRequirements::derive(&d.semantic).structured_output,
            structured
        );
        let mut limited = contract();
        limited.semantics.structured_output = false;
        for profile in [Profile::Chat, Profile::Responses] {
            assert_eq!(
                lower_request(&d.semantic, &d.fidelity, profile, limited.clone()).is_err(),
                structured
            );
        }
    }
    // Standard carriers are independent of a binding's model eligibility.
    let source = json!({"messages":[{"role":"user","content":"hello"}],"verbosity":"low"});
    let d = chat::decode_generation(&source).unwrap();
    assert_eq!(
        d.semantic.text_options(),
        &TextOptions {
            presence: Presence::Value(()),
            format: Presence::Absent,
            verbosity: Presence::Value(Verbosity::Low),
        }
    );
    assert_eq!(request_wire(&d)["text"], json!({"verbosity":"low"}));
    assert_eq!(chat_wire(&d)["verbosity"], "low");
    let mut limited = contract();
    limited.semantics.verbosity = false;
    assert!(lower_request(&d.semantic, &d.fidelity, Profile::Chat, limited).is_err());
    let mut invalid = source;
    invalid["verbosity"] = json!("loud");
    assert!(chat::decode_generation(&invalid).is_err());
}

#[test]
fn instruction_status_is_checked_in_history_and_reported_echoes() {
    let mut response = wire::response(2);
    response["instructions"] = json!([{"role":"system","content":[
            {"type":"input_image","image_url":"https://example.test/image.png"}]}]);
    assert_eq!(
        envelope::decode_response(&response).unwrap_err(),
        morphiecore::protocol::CodecError::Unsupported("instruction content".into())
    );
    for role in ["system", "developer"] {
        for status in [
            None,
            Some(json!("completed")),
            Some(Value::Null),
            Some(json!(false)),
            Some(json!("unknown")),
            Some(json!("in_progress")),
            Some(json!("incomplete")),
        ] {
            let lifecycle = matches!(&status, Some(value) if value == &json!("in_progress") || value == &json!("incomplete"));
            let accepted = status.is_none() || status == Some(json!("completed")) || lifecycle;
            let mut item = json!({"type":"message","role":role,"content":[{"type":"input_text","text":"Be precise"}]});
            if let Some(status) = status.clone() {
                item["status"] = status;
            }
            let decoded = responses::decode_generation(&json!({"input":[item.clone()]}));
            assert_eq!(decoded.is_ok(), accepted, "{item}");
            if let Ok(d) = decoded {
                let encoded = request_wire(&d)["input"][0].clone();
                if lifecycle {
                    assert_eq!(encoded["status"], item["status"]);
                    assert_eq!(encoded["type"], json!("message"));
                } else {
                    assert!(encoded.get("status").is_none());
                    assert_eq!(encoded["content"], json!("Be precise"));
                }
            }
            let mut response = wire::response(2);
            response["instructions"] = json!([item]);
            let decoded = envelope::decode_response(&response);
            assert_eq!(decoded.is_ok(), accepted);
            if let Ok(d) = decoded {
                let output = envelope::encode_response(
                    &lower_response(
                        &d.semantic,
                        &d.fidelity,
                        &d.metadata,
                        Profile::Responses,
                        contract(),
                    )
                    .unwrap(),
                )
                .unwrap();
                let encoded = &output["instructions"][0];
                if lifecycle {
                    assert_eq!(encoded["status"], item["status"]);
                } else {
                    assert!(encoded.get("status").is_none());
                    assert_eq!(encoded["content"], json!("Be precise"));
                }
            }
        }
    }
}

#[test]
fn custom_tools_and_text_array_results_are_not_function_arguments() {
    let mut d=responses::decode_generation(&json!({"input":[{"type":"custom_tool_call","call_id":"c","name":"sql","input":"SELECT 1"},{"type":"custom_tool_call_output","call_id":"c","output":[{"type":"input_text","text":"1"},{"type":"input_text","text":""}]}],"tools":[{"type":"custom","name":"sql","format":{"type":"grammar","syntax":"regex","definition":"SELECT [0-9]+"}}],"tool_choice":{"type":"custom","name":"sql"}})).unwrap();
    assert!(matches!(&d.semantic.items()[0].1,Item::CustomCall(c) if c.input=="SELECT 1"));
    assert!(GenerationRequirements::derive(&d.semantic).custom_tools);
    let mut items = d.semantic.items().to_vec();
    items[0].0 = ItemId::new(99);
    items[1].0 = ItemId::new(100);
    let Item::CustomCall(c) = &mut items[0].1 else {
        panic!()
    };
    c.input = "  not JSON,\nnot executed  ".into();
    c.call_id = text("edited-custom-call");
    let Item::CustomResult(r) = &mut items[1].1 else {
        panic!()
    };
    // Explicit synthetic report for the new call, never automatic result relinking.
    r.call_id = Some(text("edited-custom-call"));
    r.output = ToolOutput::Text("changed".into());
    d.semantic = d.semantic.with_items(items).unwrap();
    let out = request_wire(&d);
    assert_eq!(out["input"][0]["input"], "  not JSON,\nnot executed  ");
    assert!(out["input"][0].get("arguments").is_none());
    assert_eq!(out["input"][1]["output"], "changed");
    let chat = lower_request(&d.semantic, &d.fidelity, Profile::Chat, contract()).unwrap();
    let encoded = morphiecore::protocol::openai::chat::encode_generation(&chat).unwrap();
    assert_eq!(
        encoded["messages"][0]["tool_calls"],
        json!([
            {"id":"edited-custom-call","type":"custom","custom":{"name":"sql","input":"  not JSON,\nnot executed  "}}
        ])
    );
    assert_eq!(
        encoded["messages"][1],
        json!({"role":"tool","tool_call_id":"edited-custom-call","content":"changed"})
    );
    let chat_wire = json!({
        "messages":[
            {"role":"assistant","content":null,"tool_calls":[
                {"id":"f","type":"function","function":{"name":"sql","arguments":"{}"}},
                {"id":"c","type":"custom","custom":{"name":"sql","input":"SELECT 1"}}]},
            {"role":"tool","tool_call_id":"f","content":"function result"},
            {"role":"tool","tool_call_id":"c","content":[{"type":"text","text":"1"},{"type":"text","text":""}]}],
        "tools":[{"type":"function","function":{"name":"sql","strict":false}},
            {"type":"custom","custom":{"name":"sql","format":{"type":"grammar",
                "grammar":{"syntax":"regex","definition":"SELECT [0-9]+"}}}}],
        "tool_choice":{"type":"custom","custom":{"name":"sql"}}
    });
    let decoded = morphiecore::protocol::openai::chat::decode_generation(&chat_wire).unwrap();
    assert!(
        matches!(&decoded.semantic.items()[1].1,Item::ToolCall(call) if call.call_id.as_str()=="f")
    );
    assert!(
        matches!(&decoded.semantic.items()[2].1,Item::CustomCall(call) if call.call_id.as_str()=="c" && call.input=="SELECT 1")
    );
    assert!(matches!(
        &decoded.semantic.items()[3].1,
        Item::ToolResult(_)
    ));
    assert!(
        matches!(&decoded.semantic.items()[4].1,Item::CustomResult(result)
            if matches!(&result.output,ToolOutput::Parts(parts) if parts.len()==2 && parts[1].content.as_text()==Some("")))
    );
    assert!(
        matches!(&decoded.semantic.tools()[1],ToolDefinition::Custom(tool)
            if matches!(&tool.format,Some(CustomFormat::Grammar{syntax:GrammarSyntax::Regex,definition}) if definition.as_str()=="SELECT [0-9]+"))
    );
    let target = lower_request(
        &decoded.semantic,
        &decoded.fidelity,
        Profile::Chat,
        contract(),
    )
    .unwrap();
    assert_eq!(
        morphiecore::protocol::openai::chat::encode_generation(&target).unwrap(),
        chat_wire
    );
    let mut denied = contract();
    denied.semantics.custom_tools = false;
    assert!(lower_request(&decoded.semantic, &decoded.fidelity, Profile::Chat, denied).is_err());
    let mut malformed = chat_wire.clone();
    malformed["messages"][0]["tool_calls"][1]["function"] = json!({"name":"sql","arguments":"{}"});
    assert!(morphiecore::protocol::openai::chat::decode_generation(&malformed).is_err());
    let mut unsupported = chat_wire.clone();
    unsupported["tools"][1]["custom"]["defer_loading"] = json!(true);
    assert!(morphiecore::protocol::openai::chat::decode_generation(&unsupported).is_err());
    let mut allowed = chat_wire.clone();
    allowed["tool_choice"] = json!({"type":"allowed_tools","allowed_tools":{"mode":"required",
            "tools":[{"type":"custom","custom":{"name":"sql"}}]}});
    let decoded = morphiecore::protocol::openai::chat::decode_generation(&allowed).unwrap();
    let target = lower_request(
        &decoded.semantic,
        &decoded.fidelity,
        Profile::Chat,
        contract(),
    )
    .unwrap();
    assert_eq!(
        morphiecore::protocol::openai::chat::encode_generation(&target).unwrap()["tool_choice"],
        allowed["tool_choice"]
    );
    let response_wire = json!({"id":"r","object":"chat.completion","created":1,"model":"m",
            "choices":[{"index":0,"message":{"role":"assistant","content":null,
                "tool_calls":[{"id":"c","type":"custom","custom":{"name":"sql","input":"SELECT 1"}}]},
                "finish_reason":"tool_calls"}]});
    let response = morphiecore::protocol::openai::chat::decode_response(&response_wire).unwrap();
    assert!(
        matches!(&response.semantic.items()[1].1,Item::CustomCall(call) if call.input=="SELECT 1")
    );
    let target = lower_response(
        &response.semantic,
        &response.fidelity,
        &response.metadata,
        Profile::Chat,
        contract(),
    )
    .unwrap();
    assert_eq!(
        morphiecore::protocol::openai::chat::encode_response(&target).unwrap()["choices"][0]["message"],
        response_wire["choices"][0]["message"]
    );
    let mut stream =
        morphiecore::protocol::openai::events::EventEncoder::new(Profile::Chat, response.metadata)
            .unwrap();
    stream
        .encode(&StreamEvent::Started, &Default::default())
        .unwrap();
    assert!(
        stream
            .encode(
                &crate::events_support::start(
                    1,
                    ItemKind::CustomCall {
                        call_id: text("c"),
                        name: text("sql"),
                        context: Default::default()
                    }
                ),
                &Default::default()
            )
            .is_err()
    );
    let adapter = morphiecore::protocol::openai::Adapter::new(
        Profile::Chat,
        morphiecore::protocol::openai::Dialect::Standard,
        None,
    );
    let mut streaming = chat_wire.clone();
    streaming["model"] = json!("m");
    streaming["stream"] = json!(true);
    let request = adapter
        .decode_request(streaming.to_string().as_bytes())
        .unwrap();
    assert!(
        adapter
            .encode_request_with_rules(&request, "m", &contract())
            .is_err()
    );
    let mut forced = adapter.clone();
    forced.adaptation.rules.responses_forced_stream = true;
    let mut json_request = request.clone();
    json_request.delivery.stream = Presence::Value(false);
    assert!(
        forced
            .encode_request_with_rules(&json_request, "m", &contract())
            .unwrap_err()
            .to_string()
            .contains("Chat custom tool streaming")
    );
    streaming["tool_choice"] = json!("none");
    let disabled = adapter
        .decode_request(streaming.to_string().as_bytes())
        .unwrap();
    assert!(
        adapter
            .encode_request_with_rules(&disabled, "m", &contract())
            .is_ok()
    );
    streaming["tools"] = json!([]);
    let history = adapter
        .decode_request(streaming.to_string().as_bytes())
        .unwrap();
    assert!(
        adapter
            .encode_request_with_rules(&history, "m", &contract())
            .is_ok()
    );
    let mut mismatch = out;
    mismatch["input"][1]["type"] = json!("function_call_output");
    assert!(responses::decode_generation(&mismatch).is_err());
}
#[test]
fn reported_text_container_presence_is_not_request_intent() {
    use morphiecore::{
        protocol::{
            CodecError,
            fidelity::FidelityRecords,
            openai::{Adapter, Dialect, ResponseMetadata, events::EventEncoder},
        },
        semantic::context::{ResponseContext, ResponseReport, ResponseReports},
    };
    let adapter = Adapter::new(Profile::Responses, Dialect::Standard, None);
    for (wire_text, options) in [
        (None, TextOptions::default()),
        (
            Some(Value::Null),
            TextOptions {
                presence: Presence::Null,
                ..Default::default()
            },
        ),
        (
            Some(json!({})),
            TextOptions {
                presence: Presence::Value(()),
                ..Default::default()
            },
        ),
        (
            Some(json!({"format":{"type":"text"},"verbosity":"medium"})),
            TextOptions {
                presence: Presence::Value(()),
                format: Presence::Value(OutputConstraint::Text),
                verbosity: Presence::Value(Verbosity::Medium),
            },
        ),
    ] {
        // Another reported setting prevents whole-settings absence from hiding
        // a lost text-container observation.
        let mut source = json!({"id":"r","object":"response","created_at":1,"model":"m",
            "status":"completed","output":[],"temperature":0.5});
        if let Some(value) = &wire_text {
            source["text"] = value.clone();
        }
        let settings = GenerationSettings {
            controls: GenerationControls::default().with_temperature(0.5).unwrap(),
            text: options,
            ..Default::default()
        };
        let decoded = adapter
            .decode_response(source.to_string().as_bytes())
            .unwrap();
        assert_eq!(decoded.metadata.context.settings(), Some(&settings));

        // Independently constructed IR supplies both JSON and SSE expectations.
        let metadata = ResponseMetadata {
            id: Some("r".into()),
            model: "m".into(),
            created: Some(1.into()),
            context: ResponseContext {
                reports: ResponseReports::single(ResponseReport {
                    settings: Some(settings),
                    ..Default::default()
                }),
                ..Default::default()
            },
            storage: None,
            instruction_fidelity: Default::default(),
        };
        let response = GenerationResponse::new(vec![], Outcome::Completed).unwrap();
        let fidelity = FidelityRecords::default();
        let projected = lower_response(
            &response,
            &fidelity,
            &metadata,
            Profile::Responses,
            contract(),
        )
        .unwrap();
        let output = envelope::encode_response(&projected).unwrap();
        assert_eq!(output.get("text"), wire_text.as_ref());
        assert!(
            projected
                .projection()
                .iter()
                .all(|stage| stage.loss.is_none())
        );
        let mut encoder = EventEncoder::new(Profile::Responses, metadata).unwrap();
        for event in [
            StreamEvent::Started,
            StreamEvent::Terminal {
                terminal: StreamTerminal::Completed,
                details: Default::default(),
            },
        ] {
            let frames = encoder.encode(&event, &fidelity).unwrap();
            assert!(!frames.is_empty());
            for frame in frames {
                assert_eq!(frame["response"].get("text"), wire_text.as_ref());
            }
        }
        encoder.finish().unwrap();
    }
    let request = json!({"model":"m","input":"go","text":null});
    assert!(matches!(
        adapter.decode_client_request(request.to_string().as_bytes()),
        Err(CodecError::Invalid("text"))
    ));
    assert!(matches!(
        responses::decode_generation(&json!({"input":"go","text":null})),
        Err(CodecError::Invalid("text"))
    ));
    let request = responses::decode_generation(&json!({"input":"go"}))
        .unwrap()
        .semantic;
    let mut settings = request.settings().clone();
    settings.text.presence = Presence::Null;
    settings.validate().unwrap();
    assert!(matches!(
        request.with_settings(settings),
        Err(GenerationError::InvalidControl)
    ));
    for presence in [Presence::Absent, Presence::Null] {
        let mut settings = GenerationSettings::default();
        settings.text.presence = presence;
        settings.text.format = Presence::Null;
        assert!(matches!(
            settings.validate(),
            Err(GenerationError::InvalidControl)
        ));
        settings.text.format = Presence::Absent;
        settings.text.verbosity = Presence::Value(Verbosity::Low);
        assert!(matches!(
            settings.validate(),
            Err(GenerationError::InvalidControl)
        ));
    }
}

#[test]
fn null_empty_false_and_default_controls_are_deliberate_not_unknown_passthrough() {
    let d=responses::decode_generation(&json!({"input":"","instructions":null,"reasoning":{"effort":null,"summary":null},"tools":[],"tool_choice":"none","parallel_tool_calls":false,"text":{"format":{"type":"text"},"verbosity":null}})).unwrap();
    let out = request_wire(&d);
    assert_eq!(out["instructions"], Value::Null);
    assert_eq!(out["reasoning"], json!({"effort":null,"summary":null}));
    assert_eq!(out["parallel_tool_calls"], false);
    assert_eq!(out["tools"], json!([]));
    for bad in [
        json!({"top_logprobs":21}),
        json!({"top_p":1.1}),
        json!({"temperature":-1}),
        json!({"text":{"format":{"type":"json_schema","name":"bad name","schema":{}}}}),
        json!({"tools":[{"type":"custom","name":"x","format":{"type":"grammar","syntax":"unknown","definition":"x"}}]}),
        json!({"tools":[{"type":"function","name":"x","async":"yes"}]}),
        json!({"reasoning":{"unknown":true}}),
        json!({"text":null}),
        json!({"tools":null}),
        json!({"tool_choice":null}),
        json!({"reasoning":{"mode":null}}),
    ] {
        let mut value = json!({"input":"hello"});
        value
            .as_object_mut()
            .unwrap()
            .extend(bad.as_object().unwrap().clone());
        assert!(responses::decode_generation(&value).is_err());
    }
}

#[test]
fn probabilities_annotations_and_snapshots_cannot_rewrite_observed_values() {
    for field in ["probability", "annotation", "terminal"] {
        let mut values = wire::events(1);
        match field {
            "probability" => {
                let v = values
                    .iter_mut()
                    .find(|v| v["type"] == "response.content_part.done")
                    .unwrap();
                v["part"]["logprobs"][0]["logprob"] = json!(-0.75);
            }
            "annotation" => {
                let v = values
                    .iter_mut()
                    .find(|v| v["type"] == "response.output_text.annotation.added")
                    .unwrap();
                v["annotation_index"] = json!(1);
            }
            _ => {
                values.last_mut().unwrap()["response"]["output"][3]["content"][0]["text"] =
                    json!("resurrected");
            }
        }
        let mut d = EventDecoder::new(Profile::Responses).with_replay_origin(origin());
        assert!(values.iter().any(|v| d.push(v).is_err()), "{field}");
        assert!(d.finish().is_err());
    }
}
fn responses_round_trip(request: &Value) -> Value {
    let d = envelope::decode_request(request).unwrap();
    envelope::encode_request(
        &lower_request(
            &d.task.semantic,
            &d.task.fidelity,
            Profile::Responses,
            contract(),
        )
        .unwrap(),
        &d.context,
    )
    .unwrap()
}
fn response_round_trip(response: &Value) -> Value {
    let d = envelope::decode_response(response).unwrap();
    envelope::encode_response(
        &lower_response(
            &d.semantic,
            &d.fidelity,
            &d.metadata,
            Profile::Responses,
            contract(),
        )
        .unwrap(),
    )
    .unwrap()
}
#[test]
fn reported_cache_context_is_an_independent_response_fact() {
    let mut response = wire::response(2);
    response["prompt_cache_options"] = json!({"ttl":"30m","future":true});
    let decoded = responses::decode_response(&response).unwrap();
    assert_eq!(
        serde_json::to_value(
            &decoded
                .metadata
                .context
                .execution
                .cache
                .prompt_cache_options
        )
        .unwrap(),
        json!({"ttl":"30m"})
    );
    assert_eq!(
        response_round_trip(&response)["prompt_cache_options"],
        json!({"ttl":"30m"})
    );
    let mut request = wire::request(false);
    request["prompt_cache_options"] = json!({});
    assert_eq!(
        responses_round_trip(&request)["prompt_cache_options"],
        json!({})
    );
    request["prompt_cache_options"] = json!({"mode":null});
    assert!(responses::envelope::decode_request(&request).is_err());
    request["prompt_cache_options"] = Value::Null;
    assert!(responses::envelope::decode_request(&request).is_err());
    response["prompt_cache_options"] = Value::Null;
    assert_eq!(
        response_round_trip(&response)["prompt_cache_options"],
        Value::Null
    );
    response["prompt_cache_options"] = json!({"mode":"explicit","ttl":"30m","prewarm":true});
    response["prompt_cache_diagnostics"] = json!({"type":"cache_miss","reason":"input_changed","cache_missed_tokens":3,"comparison_reusable_tokens":2});
    let out = response_round_trip(&response);
    assert_eq!(
        out["prompt_cache_options"],
        json!({"mode":"explicit","ttl":"30m","prewarm":true})
    );
    assert_eq!(
        out["prompt_cache_diagnostics"],
        response["prompt_cache_diagnostics"]
    );
    // The effective tier is a reported fact and stays independent of the request hint.
    assert_eq!(out["service_tier"], json!("default"));
    assert_eq!(
        responses_round_trip(&wire::request(false))["service_tier"],
        json!("auto")
    );
    use morphiecore::semantic::context::{ServiceClass, ServicePolicy};
    use morphiecore::semantic::value::Presence;
    let mut requested = envelope::decode_request(&wire::request(false)).unwrap();
    assert_eq!(
        requested.context.service_policy,
        Presence::Value(ServicePolicy::ProjectConfigured)
    );
    assert_eq!(
        decoded.metadata.context.service_class().clone(),
        Presence::Value(ServiceClass::StandardProcessing)
    );
    requested.context.service_policy = Presence::Value(ServicePolicy::PriorityProcessing);
    let target = lower_request(
        &requested.task.semantic,
        &requested.task.fidelity,
        Profile::Responses,
        morphiecore::protocol::openai::TargetCaps::full(),
    )
    .unwrap();
    assert_eq!(
        envelope::encode_request(&target, &requested.context).unwrap()["service_tier"],
        "priority"
    );
    requested.context.service_policy = Presence::Value(ServicePolicy::PreferCommittedCapacity);
    assert!(envelope::encode_request(&target, &requested.context).is_err());
    let mut invalid = wire::response(2);
    invalid["service_tier"] = json!("standard");
    assert!(envelope::decode_response(&invalid).is_err());
    let mut reported = decoded.clone();
    reported
        .metadata
        .context
        .reports
        .latest_mut()
        .expect("report")
        .service_class = Presence::Value(ServiceClass::PriorityProcessing);
    let target = lower_response(
        &reported.semantic,
        &reported.fidelity,
        &reported.metadata,
        Profile::Responses,
        morphiecore::protocol::openai::TargetCaps::full(),
    )
    .unwrap();
    assert_eq!(
        envelope::encode_response(&target).unwrap()["service_tier"],
        "priority"
    );
    // Nothing is invented for absent echoes.
    let mut bare = wire::response(2);
    for key in [
        "prompt_cache_key",
        "prompt_cache_retention",
        "prompt_cache_options",
        "prompt_cache_diagnostics",
        "safety_identifier",
        "user",
        "metadata",
        "service_tier",
    ] {
        bare.as_object_mut().unwrap().remove(key);
    }
    let out = response_round_trip(&bare);
    for key in [
        "prompt_cache_key",
        "prompt_cache_retention",
        "prompt_cache_options",
        "prompt_cache_diagnostics",
        "safety_identifier",
        "user",
        "metadata",
        "service_tier",
    ] {
        assert!(out.get(key).is_none(), "{key}");
    }
}
