//! Independent field and transformation contracts for stateless Responses text.
use crate::wire;
use morphiecore::{
    lowering::generation::{
        GenerationRepresentationContract as Contract, lower_request, lower_response,
    },
    protocol::{
        fidelity::FidelityRecords,
        openai::{
            DecodedRequest, Profile, chat, chat_envelope, envelope,
            events::{EventDecoder, EventEncoder},
            responses,
            sse::*,
        },
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
fn raw_envelopes_reject_duplicate_keys_before_task_decode() {
    let request = wire::request(false).to_string();
    assert_eq!(
        envelope::decode_request_bytes(request.as_bytes())
            .unwrap()
            .context
            .model,
        "fixture-model"
    );
    let duplicate = format!("{{\"model\":\"discarded\",{}", &request[1..]);
    assert!(envelope::decode_request_bytes(duplicate.as_bytes()).is_err());
    let nested = request.replace(
        "\"suite\":\"synthetic-local\"",
        "\"suite\":\"discarded\",\"suite\":\"synthetic-local\"",
    );
    assert_ne!(nested, request);
    assert!(envelope::decode_request_bytes(nested.as_bytes()).is_err());

    let response = wire::response(2).to_string();
    let d = envelope::decode_response_bytes(response.as_bytes()).unwrap();
    assert_eq!(d.fidelity.response_item_id(ItemId::new(1)), Some("answer"));
    let duplicate = response.replace(
        "\"id\":\"answer\"",
        "\"id\":\"discarded\",\"id\":\"answer\"",
    );
    assert_ne!(duplicate, response);
    assert!(envelope::decode_response_bytes(duplicate.as_bytes()).is_err());
    assert!(envelope::decode_response_bytes(format!("{response} null").as_bytes()).is_err());
    assert!(envelope::decode_request_bytes(format!("{request} null").as_bytes()).is_err());
}

#[test]
fn absent_text_container_cannot_hide_present_children() {
    let d = responses::decode_generation(&json!({"input":"hello"})).unwrap();
    for (format, verbosity) in [
        (
            Presence::Value(OutputConstraint::JsonObject),
            Presence::Absent,
        ),
        (Presence::Value(OutputConstraint::Text), Presence::Absent),
        (Presence::Null, Presence::Absent),
        (Presence::Absent, Presence::Value(Verbosity::Low)),
        (Presence::Absent, Presence::Null),
    ] {
        let mut settings = d.semantic.settings().clone();
        settings.text = TextOptions {
            presence: false,
            format,
            verbosity,
        };
        assert_eq!(settings.validate(), Err(GenerationError::InvalidControl));
        assert!(
            GenerationRequest::from_settings(d.semantic.items().to_vec(), settings.clone())
                .is_err()
        );
        assert!(d.semantic.clone().with_settings(settings.clone()).is_err());
        let mut response = envelope::decode_response(&wire::response(2)).unwrap();
        response.metadata.context.settings.as_mut().unwrap().text = settings.text;
        assert!(
            lower_response(
                &response.semantic,
                &response.fidelity,
                &response.metadata,
                Profile::Responses,
                contract()
            )
            .is_err()
        );
    }
}

#[test]
fn text_presence_and_final_requirements_match_independent_wire_expectations() {
    let mut d = responses::decode_generation(&json!({"input":"hello"})).unwrap();
    for (options, expected, structured) in [
        (
            TextOptions {
                presence: true,
                ..Default::default()
            },
            Some(json!({})),
            false,
        ),
        (
            TextOptions {
                presence: true,
                format: Presence::Null,
                verbosity: Presence::Null,
            },
            Some(json!({"format":null,"verbosity":null})),
            false,
        ),
        (
            TextOptions {
                presence: true,
                format: Presence::Value(OutputConstraint::Text),
                verbosity: Presence::Value(Verbosity::Medium),
            },
            Some(json!({"format":{"type":"text"},"verbosity":"medium"})),
            false,
        ),
        (
            TextOptions {
                presence: true,
                format: Presence::Value(OutputConstraint::JsonObject),
                verbosity: Presence::Absent,
            },
            Some(json!({"format":{"type":"json_object"}})),
            true,
        ),
        (TextOptions::default(), None, false),
    ] {
        let mut source = json!({"input":"hello"});
        if let Some(expected) = &expected {
            source["text"] = expected.clone();
        }
        assert_eq!(
            responses::decode_generation(&source)
                .unwrap()
                .semantic
                .text_options(),
            &options
        );
        let mut settings = d.semantic.settings().clone();
        settings.text = options;
        d.semantic = d.semantic.with_settings(settings).unwrap();
        assert_eq!(
            GenerationRequirements::derive(&d.semantic).structured_output,
            structured
        );
        assert_eq!(request_wire(&d).get("text"), expected.as_ref());
        let mut limited = contract();
        limited.semantics.structured_output = false;
        assert_eq!(
            lower_request(&d.semantic, &d.fidelity, Profile::Responses, limited).is_err(),
            structured
        );
    }
}

#[test]
fn response_format_shells_map_to_one_owner_with_independent_shapes() {
    let closed = json!({"type":"object","properties":{"zeta":{"type":"string"}},"required":["zeta"],"additionalProperties":false});
    let full = OutputConstraint::JsonSchema {
        name: text("answer"),
        description: Some(text("pick")),
        schema: closed.clone().into(),
        strict: Some(true),
    };
    let bare = OutputConstraint::JsonSchema {
        name: text("answer"),
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
            presence: format.is_some(),
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
}

#[test]
fn response_format_presence_defaults_and_rejections_are_deliberate() {
    let closed = json!({"type":"object","properties":{"zeta":{"type":"string"}},"required":["zeta"],"additionalProperties":false});
    let base = json!({"messages":[{"role":"user","content":"hello"}]});
    let decode = |format: Value| {
        let mut source = base.clone();
        source["response_format"] = format;
        chat::decode_generation(&source).unwrap()
    };
    // Missing and null inner description/strict are one state; an explicit false stays visible.
    let omitted =
        decode(json!({"type":"json_schema","json_schema":{"name":"answer","schema":closed}}));
    let nulled = decode(
        json!({"type":"json_schema","json_schema":{"name":"answer","schema":closed,"description":null,"strict":null}}),
    );
    assert_eq!(
        omitted.semantic.text_options(),
        nulled.semantic.text_options()
    );
    assert_eq!(chat_wire(&omitted), chat_wire(&nulled));
    assert!(!chat_wire(&omitted).to_string().contains("strict"));
    let explicit = decode(
        json!({"type":"json_schema","json_schema":{"name":"answer","schema":closed,"strict":false}}),
    );
    assert_ne!(
        omitted.semantic.text_options(),
        explicit.semantic.text_options()
    );
    assert_eq!(
        chat_wire(&explicit)["response_format"]["json_schema"]["strict"],
        json!(false)
    );
    // Explicit null and explicit text stay distinct from omission on the wire.
    let nulled = decode(Value::Null);
    assert_eq!(
        nulled.semantic.text_options(),
        &TextOptions {
            presence: true,
            format: Presence::Null,
            verbosity: Presence::Absent
        }
    );
    assert_eq!(
        chat_wire(&nulled).get("response_format"),
        Some(&Value::Null)
    );
    let explicit_text = decode(json!({"type":"text"}));
    assert_eq!(
        chat_wire(&explicit_text)["response_format"],
        json!({"type":"text"})
    );
    let absent = chat::decode_generation(&base).unwrap();
    assert!(chat_wire(&absent).get("response_format").is_none());
    // A missing or empty text container never becomes an explicit constraint.
    for source in [
        json!({"input":"hello","text":{}}),
        json!({"input":"hello","text":{"verbosity":"low"}}),
    ] {
        let r = responses::decode_generation(&source).unwrap();
        assert_eq!(r.semantic.text_options().format, Presence::Absent);
    }
    let empty = responses::decode_generation(&json!({"input":"hello","text":{}})).unwrap();
    assert!(chat_wire(&empty).get("response_format").is_none());
    for bad in [
        json!(5),
        json!(true),
        json!([]),
        json!({}),
        json!({"type":"unknown"}),
        json!({"type":"text","extra":1}),
        json!({"type":"json_object","schema":{}}),
        json!({"type":"json_schema"}),
        json!({"type":"json_schema","json_schema":null}),
        json!({"type":"json_schema","json_schema":"x"}),
        json!({"type":"json_schema","json_schema":{}}),
        json!({"type":"json_schema","json_schema":{"name":"answer"}}),
        json!({"type":"json_schema","json_schema":{"name":"answer","schema":null}}),
        json!({"type":"json_schema","json_schema":{"name":"answer","schema":5}}),
        json!({"type":"json_schema","json_schema":{"name":"answer","schema":{},"extra":1}}),
        json!({"type":"json_schema","json_schema":{"name":"bad name","schema":{}}}),
        json!({"type":"json_schema","json_schema":{"name":"","schema":{}}}),
        json!({"type":"json_schema","json_schema":{"name":"answer","schema":{},"strict":"yes"}}),
        json!({"type":"json_schema","json_schema":{"name":"answer","schema":{},"description":7}}),
    ] {
        let mut source = base.clone();
        source["response_format"] = bad;
        assert!(chat::decode_generation(&source).is_err());
    }
}

#[test]
fn complete_messages_require_headers_but_task_snapshots_keep_abbreviations() {
    let source = wire::response(2);
    for key in ["type", "id", "status", "role", "content"] {
        for replacement in [None, Some(Value::Null), Some(json!(false))] {
            let mut bad = source.clone();
            let item = bad["output"][0].as_object_mut().unwrap();
            if let Some(value) = replacement {
                item.insert(key.into(), value);
            } else {
                item.remove(key);
            }
            assert!(envelope::decode_response(&bad).is_err(), "{key}");
        }
    }
    for (key, value) in [("id", ""), ("status", "unknown"), ("role", "user")] {
        let mut bad = source.clone();
        bad["output"][0][key] = json!(value);
        assert!(envelope::decode_response(&bad).is_err(), "{key}");
    }
    for (status, lifecycle) in [
        ("completed", ItemLifecycle::Completed),
        ("incomplete", ItemLifecycle::Incomplete),
        ("in_progress", ItemLifecycle::InProgress),
    ] {
        let mut value = source.clone();
        value["status"] = json!("incomplete");
        value["incomplete_details"] = json!({"reason":"max_output_tokens"});
        value["output"][0]["status"] = json!(status);
        let d = envelope::decode_response(&value).unwrap();
        assert_eq!(d.semantic.items()[0].1.lifecycle(), Some(lifecycle));
        let encoded = envelope::encode_response(
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
        assert_eq!(encoded["output"][0]["id"], "answer");
        assert_eq!(encoded["output"][0]["status"], status);
    }
    let mut abbreviated = source;
    for key in ["id", "status"] {
        abbreviated["output"][0]
            .as_object_mut()
            .unwrap()
            .remove(key);
    }
    let decoded = responses::decode_response(&abbreviated).unwrap();
    let encoded = envelope::encode_response(
        &lower_response(
            &decoded.semantic,
            &decoded.fidelity,
            &decoded.metadata,
            Profile::Responses,
            contract(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(encoded["output"][0]["id"], "item_1");
    assert_eq!(encoded["output"][0]["status"], "completed");
    let input = responses::decode_generation(&json!({"input":[{"role":"user","content":"hello"}]}))
        .unwrap();
    assert_eq!(
        request_wire(&input)["input"],
        json!([{"type":"message","role":"user","content":[{"type":"input_text","text":"hello"}]}])
    );
}

#[test]
fn complete_response_snapshots_require_the_output_array() {
    let source = wire::response(2);
    for replacement in [None, Some(Value::Null)] {
        let mut bad = source.clone();
        if let Some(ref value) = replacement {
            bad["output"] = value.clone();
        } else {
            bad.as_object_mut().unwrap().remove("output");
        }
        assert!(envelope::decode_response(&bad).is_err(), "{replacement:?}");
        assert!(
            envelope::decode_response_bytes(bad.to_string().as_bytes()).is_err(),
            "{replacement:?}"
        );
    }
    let mut empty = source;
    empty["output"] = json!([]);
    envelope::decode_response(&empty).unwrap();
}

#[test]
fn instruction_status_is_checked_in_history_and_reported_echoes() {
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
fn annotation_payload_rejection_cannot_be_followed_by_success() {
    for annotation in [
        None,
        Some(Value::Null),
        Some(json!({})),
        Some(json!("invalid")),
    ] {
        let values = wire::events(2);
        let index = values
            .iter()
            .position(|v| v["type"] == "response.output_text.annotation.added")
            .unwrap();
        let mut decoder = EventDecoder::new(Profile::Responses);
        for value in &values[..index] {
            decoder.push(value).unwrap();
        }
        let mut bad = values[index].clone();
        if let Some(annotation) = annotation {
            bad["annotation"] = annotation;
        } else {
            bad.as_object_mut().unwrap().remove("annotation");
        }
        assert!(decoder.push(&bad).is_err(), "{bad}");
        for value in &values[index..] {
            assert!(decoder.push(value).is_err());
        }
        assert!(decoder.finish().is_err());
        assert!(decoder.materialize().is_err());
    }
}

#[test]
fn string_input_and_top_level_instructions_are_task_semantics() {
    let mut d = responses::decode_generation(&json!({"input":"hello","instructions":"Be precise"}))
        .unwrap();
    assert_eq!(
        d.semantic.instructions().value().unwrap().as_str(),
        "Be precise"
    );
    assert_eq!(d.semantic.items().len(), 1);
    let mut settings = d.semantic.settings().clone();
    settings.instructions = Presence::Value(text("New instruction"));
    d.semantic = d.semantic.with_settings(settings).unwrap();
    assert_eq!(request_wire(&d)["instructions"], "New instruction");
    let mut settings = d.semantic.settings().clone();
    settings.instructions = Presence::Absent;
    d.semantic = d.semantic.with_settings(settings).unwrap();
    assert!(request_wire(&d).get("instructions").is_none());
    let d = responses::decode_generation(&json!({"instructions":"Only instructions"})).unwrap();
    assert!(d.semantic.items().is_empty());
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
    c.input = "not JSON, not executed".into();
    c.call_id = text("edited-custom-call");
    let Item::CustomResult(r) = &mut items[1].1 else {
        panic!()
    };
    // Explicit synthetic report for the new call, never automatic result relinking.
    r.call_id = text("edited-custom-call");
    r.output = ToolOutput::Text("changed".into());
    d.semantic = d.semantic.with_items(items).unwrap();
    let out = request_wire(&d);
    assert_eq!(out["input"][0]["input"], "not JSON, not executed");
    assert!(out["input"][0].get("arguments").is_none());
    assert_eq!(out["input"][1]["output"], "changed");
    assert!(lower_request(&d.semantic, &d.fidelity, Profile::Chat, contract()).is_err());
    let mut mismatch = out;
    mismatch["input"][1]["type"] = json!("function_call_output");
    assert!(responses::decode_generation(&mismatch).is_err());
}
#[test]
fn annotated_output_and_probabilities_are_owned_by_the_current_text() {
    let original = wire::response(2);
    let mut d = envelope::decode_response(&original).unwrap();
    let mut items = d.semantic.items().to_vec();
    let Item::Message(m) = &mut items[0].1 else {
        panic!()
    };
    let ContentPart::Text(t) = &m.parts[0].content else {
        panic!()
    };
    assert_eq!(t.annotations().len(), 1);
    assert_eq!(
        t.logprobs().value().unwrap()[0].bytes.as_ref().unwrap(),
        b"{\"ok\":false}"
    );
    m.parts[0].content = ContentPart::Text(t.clone().replace_text(text("{\"ok\":true}")));
    d.semantic = d.semantic.with_items(items).unwrap();
    let out = envelope::encode_response(
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
    assert_eq!(
        out["output"][0]["content"][0],
        json!({"type":"output_text","text":"{\"ok\":true}","annotations":[]})
    );
    assert_eq!(out["created_at"], json!(1.25));
    assert_eq!(out["completed_at"], json!(2.5));
    // Presence-preserving admission (ADR 0008): absent reported facts stay
    // absent, while structural requirements stay strict.
    let minimal = json!({"id":"r","object":"response","created_at":1,"model":"m","status":"completed","output":[]});
    let decoded = envelope::decode_response(&minimal).unwrap();
    assert!(decoded.metadata.context.settings.is_none());
    assert!(
        envelope::decode_response(
            &json!({"id":"r","object":"response","created_at":1,"model":"m","status":"completed"})
        )
        .is_err(),
        "the structural output array is still required"
    );
}
#[test]
fn structured_output_and_reasoning_context_are_real_controls() {
    let mut d=responses::decode_generation(&json!({"input":[{"role":"user","content":"hello"}],"top_p":0.8,"top_logprobs":2,"text":{"format":{"type":"json_schema","name":"answer","schema":{"type":"object","properties":{},"required":[],"additionalProperties":false},"strict":true},"verbosity":"low"},"reasoning":{"effort":"max","context":"all_turns","mode":"pro"}})).unwrap();
    assert!(matches!(
        d.semantic.output(),
        OutputConstraint::JsonSchema {
            strict: Some(true),
            ..
        }
    ));
    assert_eq!(
        d.semantic.reasoning().context,
        Presence::Value(ReasoningContext::AllTurns)
    );
    assert!(GenerationRequirements::derive(&d.semantic).structured_output);
    let mut s = d.semantic.settings().clone();
    s.text.format = Presence::Value(OutputConstraint::JsonObject);
    s.text.verbosity = Presence::Value(Verbosity::High);
    s.reasoning = ReasoningRequest::absent();
    s.controls = GenerationControls::default();
    d.semantic = d.semantic.with_settings(s).unwrap();
    let out = request_wire(&d);
    assert_eq!(
        out["text"],
        json!({"format":{"type":"json_object"},"verbosity":"high"})
    );
    assert!(out.get("reasoning").is_none());
    assert!(out.get("top_p").is_none());
    assert!(out.get("top_logprobs").is_none());
}
#[test]
fn complete_envelope_separates_model_delivery_execution_and_task() {
    let mut request = wire::request(true);
    request["stream_options"] = json!({"include_obfuscation":false});
    let d = envelope::decode_request(&request).unwrap();
    assert_eq!(d.context.model, "fixture-model");
    assert!(d.context.delivery.streaming());
    assert_eq!(
        d.context.execution.metadata.value().unwrap()["suite"],
        "synthetic-local"
    );
    let out = envelope::encode_request(
        &lower_request(
            &d.task.semantic,
            &d.task.fidelity,
            Profile::Responses,
            contract(),
        )
        .unwrap(),
        &d.context,
    )
    .unwrap();
    assert_eq!(out["stream_options"], request["stream_options"]);
    assert_eq!(out["model"], "fixture-model");
    assert_eq!(out["store"], false);
    assert_eq!(out["text"], request["text"]);
    assert_eq!(out["input"][0]["content"][0]["text"], "hello 🧪");
    let mut no_store = request.clone();
    no_store.as_object_mut().unwrap().remove("store");
    let d = envelope::decode_request(&no_store).unwrap();
    assert_eq!(
        envelope::encode_request(
            &lower_request(
                &d.task.semantic,
                &d.task.fidelity,
                Profile::Responses,
                contract()
            )
            .unwrap(),
            &d.context
        )
        .unwrap()["store"],
        false
    );
    for (key, value) in [
        ("store", json!(true)),
        ("background", json!(true)),
        ("previous_response_id", json!("remote-state")),
        ("conversation", json!({"id":"remote"})),
        ("prompt", json!({"id":"template"})),
        ("context_management", json!([{"type":"compaction"}])),
        ("base_url", json!("https://example.test")),
    ] {
        let mut bad = request.clone();
        bad[key] = value;
        assert!(envelope::decode_request(&bad).is_err(), "{key}");
    }
}
#[test]
fn instruction_parts_and_cache_breakpoints_follow_surviving_owners() {
    let mut d=responses::decode_generation(&json!({"input":[{"type":"message","id":"instruction","role":"developer","content":[{"type":"input_text","text":"a","prompt_cache_breakpoint":{"mode":"explicit"}},{"type":"input_text","text":"b"}]},{"role":"assistant","content":[{"type":"input_text","text":"prefix","prompt_cache_breakpoint":{"mode":"explicit"}}]}]})).unwrap();
    let out = request_wire(&d);
    assert_eq!(
        out["input"][0]["content"][0]["prompt_cache_breakpoint"],
        json!({"mode":"explicit"})
    );
    assert_eq!(out["input"][1]["content"][0]["type"], "input_text");
    let mut items = d.semantic.items().to_vec();
    let Item::Instruction(i) = &mut items[0].1 else {
        panic!()
    };
    i.parts.remove(0);
    items.pop();
    d.semantic = d.semantic.with_items(items).unwrap();
    let out = request_wire(&d);
    assert_eq!(out["input"][0]["content"], "b");
    assert!(!out.to_string().contains("prompt_cache_breakpoint"));
    let mut response = wire::response(2);
    response["instructions"] = json!([{"type":"message","id":"instruction","role":"developer","content":[{"type":"input_text","text":"a"},{"type":"input_text","text":"b"}]}]);
    let d = envelope::decode_response(&response).unwrap();
    let encoded = envelope::encode_response(
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
    assert_eq!(encoded["instructions"], response["instructions"]);
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
fn reported_settings_nulls_follow_the_response_model_not_the_request_types() {
    let mut response = wire::response(2);
    response["text"] = Value::Null;
    response["reasoning"]["mode"] = Value::Null;
    let d = envelope::decode_response(&response).unwrap();
    let settings = d.metadata.context.settings.as_ref().unwrap();
    assert!(!settings.text.presence);
    assert_eq!(settings.reasoning.mode, Presence::Null);
    let target = lower_response(
        &d.semantic,
        &d.fidelity,
        &d.metadata,
        Profile::Responses,
        contract(),
    )
    .unwrap();
    let encoded = envelope::encode_response(&target).unwrap();
    assert!(encoded.get("text").is_none());
    assert_eq!(encoded["reasoning"]["mode"], Value::Null);
}
#[test]
fn static_file_path_is_not_an_annotation_added_event() {
    let mut response = wire::response(2);
    response["output"][0]["content"][0]["annotations"] =
        json!([{"type":"file_path","file_id":"synthetic-file","index":0}]);
    envelope::decode_response(&response).unwrap();
    let mut raw = wire::events(2);
    let added = raw
        .iter_mut()
        .find(|v| v["type"] == "response.output_text.annotation.added")
        .unwrap();
    added["annotation"] = json!({"type":"file_path","file_id":"synthetic-file","index":0});
    let mut decoder = EventDecoder::new(Profile::Responses);
    assert!(raw.iter().any(|v| decoder.push(v).is_err()));
    assert!(decoder.finish().is_err());

    let mut accepted = EventDecoder::new(Profile::Responses).with_replay_origin(origin());
    let mut events = vec![];
    for value in wire::events(2) {
        events.extend(accepted.push(&value).unwrap());
    }
    accepted.finish().unwrap();
    let mut encoder = EventEncoder::new(Profile::Responses, accepted.metadata().unwrap().clone())
        .unwrap()
        .with_contract(contract());
    let mut rejected = false;
    let mut sources = ResourceTable::default();
    for mut event in events {
        if let StreamEvent::ResourceDeclared { id, resource } = &mut event {
            resource.body =
                ResourceBody::Media(ResourceLocation::OpaqueReference(text("synthetic-file")));
            sources = sources.insert(*id, resource.clone()).unwrap();
        }
        if let StreamEvent::AnnotationAdded { annotation, .. } = &mut event {
            *annotation = Annotation::new(
                Citation {
                    kind: CitationKind::ArtifactReference,
                    claim: ClaimAnchor::Unreported,
                    source: annotation.value().source,
                    coordinates: SourceCoordinates::Unreported,
                    label: None,
                    reported_ordinal: Some(0),
                },
                &sources,
            )
            .unwrap();
        }
        if encoder.encode(&event, accepted.fidelity()).is_err() {
            rejected = true;
            break;
        }
    }
    assert!(rejected);
    assert!(encoder.finish().is_err());
}

#[test]
fn standard_profile_preserves_reported_cache_write_and_absence() {
    let mut source = wire::response(2);
    source["usage"]["input_tokens_details"]["cache_write_tokens"] = json!(2);
    let decoded = envelope::decode_response(&source).unwrap();
    assert_eq!(
        decoded.semantic.usage().unwrap().input_cache_write_tokens,
        Some(2)
    );
    let output = envelope::encode_response(
        &lower_response(
            &decoded.semantic,
            &decoded.fidelity,
            &decoded.metadata,
            Profile::Responses,
            contract(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(output["usage"], source["usage"]);
    source["usage"]["input_tokens_details"]
        .as_object_mut()
        .unwrap()
        .remove("cache_write_tokens");
    // The standard profile has no compatibility default; only the declared
    // DeepSeek adapter may normalize this absence at intake.
    let decoded = envelope::decode_response(&source).unwrap();
    assert_eq!(
        decoded.semantic.usage().unwrap().input_cache_write_tokens,
        None
    );
    let output = envelope::encode_response(
        &lower_response(
            &decoded.semantic,
            &decoded.fidelity,
            &decoded.metadata,
            Profile::Responses,
            contract(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(
        output["usage"]["input_tokens_details"]
            .get("cache_write_tokens")
            .is_none()
    );
}

#[test]
fn custom_annotations_and_logprobs_close_independent_sse_and_static_snapshots() {
    let mut decoder = EventDecoder::new(Profile::Responses).with_replay_origin(origin());
    let mut events = vec![];
    for v in wire::events(1) {
        events.extend(decoder.push(&v).unwrap());
    }
    decoder.finish().unwrap();
    let decoded = decoder.materialize().unwrap();
    assert!(events.iter().any(|e| matches!(
        e,
        StreamEvent::ItemStarted {
            kind: ItemKind::CustomCall { .. },
            ..
        }
    )));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, StreamEvent::AnnotationAdded { .. }))
    );
    let rendered = envelope::encode_response(
        &lower_response(
            &decoded.semantic,
            &decoded.fidelity,
            &decoded.metadata,
            Profile::Responses,
            contract(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(rendered["output"], wire::response(1)["output"]);
    assert_eq!(rendered["usage"], wire::response(1)["usage"]);
    let mut encoder = ResponsesSseEncoder::new(
        decoded.metadata,
        contract(),
        SseLimits::default(),
        Obfuscation::Seeded([7; 32]),
    )
    .unwrap();
    let mut bytes = vec![];
    for event in events {
        for frame in encoder.encode(&event, &decoded.fidelity).unwrap() {
            bytes.extend(frame);
        }
    }
    encoder.finish().unwrap();
    let mut round = ResponsesSseDecoder::new(
        200,
        "text/event-stream; charset=\"utf-8\"",
        SseLimits::default(),
        Some(origin()),
    )
    .unwrap();
    let mut seen = 0;
    for b in &bytes {
        let (used, events) = round.consume(std::slice::from_ref(b)).unwrap();
        assert_eq!(used, 1);
        seen += events.len();
    }
    round.finish().unwrap();
    assert!(seen > 10);
    let actual = round.materialize().unwrap();
    let out = envelope::encode_response(
        &lower_response(
            &actual.semantic,
            &actual.fidelity,
            &actual.metadata,
            Profile::Responses,
            contract(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(out["output"], wire::response(1)["output"]);
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
#[test]
fn absent_error_code_and_unspecified_incomplete_reason_are_not_success() {
    let mut d = EventDecoder::new(Profile::Responses);
    let events = d
        .push(&json!({"type":"error","message":"synthetic","code":null,"param":null}))
        .unwrap();
    assert!(
        matches!(&events[0],StreamEvent::Terminal{terminal:StreamTerminal::Error,details} if details.error.as_ref().unwrap().code.is_none())
    );
    d.finish().unwrap();
    assert!(d.materialize().is_err());
    let mut value = wire::response(2);
    value["status"] = json!("incomplete");
    value["incomplete_details"] = json!({"reason":null});
    let d = envelope::decode_response(&value).unwrap();
    assert_eq!(
        d.semantic.details().incomplete,
        Some(IncompleteReason::Unspecified)
    );
    assert!(
        lower_response(
            &d.semantic,
            &d.fidelity,
            &d.metadata,
            Profile::Chat,
            contract()
        )
        .is_err()
    );
}
#[test]
fn metadata_budget_and_ranges_fail_without_expanding_or_reusing_old_values() {
    let sources = ResourceTable::new(vec![(
        ResourceId::new(1),
        ResourceDeclaration {
            body: ResourceBody::Media(ResourceLocation::Url(text("https://example.test"))),
            conditions: Default::default(),
        },
    )])
    .unwrap();
    let ann = Annotation::new(
        Citation {
            kind: CitationKind::Citation,
            claim: ClaimAnchor::Range(TextRange {
                unit: TextUnit::UnicodeScalars,
                start: 0,
                end: 2,
            }),
            source: ResourceId::new(1),
            coordinates: SourceCoordinates::Unreported,
            label: Some(text("source")),
            reported_ordinal: None,
        },
        &sources,
    )
    .unwrap();
    assert!(TextContent::new(text("x"), vec![ann], Presence::Absent).is_err());
    let mut value = wire::response(2);
    value["output"][0]["content"][0]["logprobs"][0]["bytes"] = json!([256]);
    assert!(envelope::decode_response(&value).is_err());
    let mut metadata = wire::request(false);
    metadata["metadata"] = json!({"k":"a".repeat(513)});
    assert!(envelope::decode_request(&metadata).is_err());
    let mut hints = FidelityRecords::default();
    hints.record_cache_breakpoint(PartId::new(99)).unwrap();
    let d = responses::decode_generation(&json!({"input":"hello"})).unwrap();
    assert!(lower_request(&d.semantic, &hints, Profile::Chat, contract()).is_ok());
}
#[test]
fn output_text_annotations_are_required_and_empty_arrays_stay_visible() {
    assert!(
        responses::decode_generation(
            &json!({"input":[{"role":"assistant","content":[{"type":"output_text","text":"hi"}]}]})
        )
        .is_err()
    );
    assert!(responses::decode_generation(&json!({"input":[{"role":"assistant","content":[{"type":"output_text","text":"hi","annotations":null}]}]})).is_err());
    let decoded = responses::decode_generation(&json!({"input":[{"role":"assistant","content":[{"type":"output_text","text":"hi","annotations":[]}]}]})).unwrap();
    assert_eq!(
        request_wire(&decoded)["input"][0]["content"][0]["annotations"],
        json!([])
    );
    assert!(
        responses::decode_generation(
            &json!({"input":[{"role":"user","content":[{"type":"input_text","text":"hi"}]}]})
        )
        .is_ok()
    );
    let mut response = wire::response(2);
    response["output"][0]["content"][0]
        .as_object_mut()
        .unwrap()
        .remove("annotations");
    assert!(envelope::decode_response(&response).is_err());
    let mut decoder = EventDecoder::new(Profile::Responses);
    decoder.push(&json!({"type":"response.created","response":{"id":"r","object":"response","created_at":0,"model":"synthetic","status":"in_progress","output":[]}})).unwrap();
    decoder.push(&json!({"type":"response.output_item.added","output_index":0,"item":{"id":"m","type":"message","role":"assistant","status":"in_progress","content":[]}})).unwrap();
    assert!(decoder.push(&json!({"type":"response.content_part.added","output_index":0,"item_id":"m","content_index":0,"part":{"type":"output_text","text":""}})).is_err());
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
fn cache_affinity_hints_keep_presence_and_never_convert_retention_into_ttl() {
    let mut request = wire::request(false);
    request["prompt_cache_options"] = json!({"mode":"implicit","ttl":"30m","prewarm":false});
    request["user"] = json!("synthetic-user");
    let d = envelope::decode_request(&request).unwrap();
    assert_eq!(
        d.context
            .execution
            .cache
            .prompt_cache_options
            .value()
            .unwrap()
            .prewarm,
        Presence::Value(false)
    );
    assert_eq!(
        d.context.execution.identity.user.value().unwrap(),
        "synthetic-user"
    );
    let out = responses_round_trip(&request);
    assert_eq!(
        out["prompt_cache_options"],
        json!({"mode":"implicit","ttl":"30m","prewarm":false})
    );
    assert_eq!(out["user"], json!("synthetic-user"));
    assert_eq!(out["prompt_cache_key"], json!("synthetic-cache"));
    assert_eq!(out["prompt_cache_retention"], json!("in_memory"));
    // Omission is not an explicit default and retention never becomes options.ttl.
    let out = responses_round_trip(&wire::request(false));
    assert!(out.get("prompt_cache_options").is_none());
    assert!(out.get("user").is_none());
    let mut request = wire::request(false);
    request
        .as_object_mut()
        .unwrap()
        .remove("prompt_cache_retention");
    request["prompt_cache_options"] = json!({"mode":"explicit","ttl":"30m","prewarm":true});
    let out = responses_round_trip(&request);
    assert!(out.get("prompt_cache_retention").is_none());
    assert_eq!(
        out["prompt_cache_options"],
        json!({"mode":"explicit","ttl":"30m","prewarm":true})
    );
    // An omitted prewarm stays absent next to explicit defaults.
    let mut request = wire::request(false);
    request["prompt_cache_options"] = json!({"mode":"implicit","ttl":"30m"});
    let out = responses_round_trip(&request);
    assert_eq!(
        out["prompt_cache_options"],
        json!({"mode":"implicit","ttl":"30m"})
    );
    for options in [
        json!({"mode":"implicit","ttl":"30m","prewarm":null}),
        json!({"mode":"implicit","ttl":"30m","prewarm":"yes"}),
        json!({"mode":"implicit","ttl":"30m","prewarm":true,"extra":1}),
    ] {
        let mut bad = wire::request(false);
        bad["prompt_cache_options"] = options;
        assert!(envelope::decode_request(&bad).is_err());
    }
}
#[test]
fn reported_cache_context_is_an_independent_response_fact() {
    let mut response = wire::response(2);
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
    ] {
        assert!(out.get(key).is_none(), "{key}");
    }
}
#[test]
fn chat_cache_affinity_hints_round_trip_within_chat_scope() {
    let request = json!({"model":"fixture-model","messages":[{"role":"user","content":"x"}],"prompt_cache_key":"k","prompt_cache_options":{"mode":"implicit","ttl":"30m","prewarm":true},"prompt_cache_retention":"24h","safety_identifier":"synthetic-user","user":"synthetic-user"});
    let d = chat_envelope::decode_request(&request).unwrap();
    assert_eq!(d.context.cache.prompt_cache_key.value().unwrap(), "k");
    let out = chat_envelope::encode_request(
        &lower_request(
            &d.task.semantic,
            &d.task.fidelity,
            Profile::Chat,
            contract(),
        )
        .unwrap(),
        &d.context,
    )
    .unwrap();
    assert_eq!(out["prompt_cache_key"], json!("k"));
    assert_eq!(
        out["prompt_cache_options"],
        json!({"mode":"implicit","ttl":"30m","prewarm":true})
    );
    assert_eq!(out["prompt_cache_retention"], json!("24h"));
    assert_eq!(out["safety_identifier"], json!("synthetic-user"));
    assert_eq!(out["user"], json!("synthetic-user"));
    let minimal = json!({"model":"fixture-model","messages":[{"role":"user","content":"x"}]});
    let d = chat_envelope::decode_request(&minimal).unwrap();
    let out = chat_envelope::encode_request(
        &lower_request(
            &d.task.semantic,
            &d.task.fidelity,
            Profile::Chat,
            contract(),
        )
        .unwrap(),
        &d.context,
    )
    .unwrap();
    assert!(out.get("prompt_cache_key").is_none());
    assert!(out.get("prompt_cache_options").is_none());
    assert!(out.get("user").is_none());
    for options in [
        json!({"mode":"implicit","ttl":"30m","prewarm":null}),
        json!({"mode":"implicit","ttl":"30m","prewarm":1}),
    ] {
        let mut bad = minimal.clone();
        bad["prompt_cache_options"] = options;
        assert!(chat_envelope::decode_request(&bad).is_err());
    }
}
