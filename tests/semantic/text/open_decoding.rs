//! Upstream responses ignore unknown keys while known keys stay strict; requests stay closed.
use crate::{chat_wire, wire};
use morphiecore::protocol::openai::{Dialect, GenerationApi, GenerationCodec};
use serde_json::{Value, json};

fn adapter(profile: GenerationApi) -> GenerationCodec {
    GenerationCodec::new(profile, Dialect::Standard, None)
}
fn future() -> Value {
    json!({"synthetic":["future",1,null]})
}
fn add(value: &mut Value, pointer: &str) {
    value
        .pointer_mut(pointer)
        .and_then(Value::as_object_mut)
        .unwrap_or_else(|| panic!("{pointer}"))
        .insert("future_field".into(), future());
}

#[test]
fn responses_static_response_ignores_unknown_keys_at_every_level() {
    let a = adapter(GenerationApi::Responses);
    let plain = wire::response(1);
    let mut extended = plain.clone();
    extended["usage"]["prompt_cache_hit_tokens"] = json!({"future":true});
    extended["usage"]["prompt_cache_miss_tokens"] = json!(-1);
    extended["usage"]["input_tokens_details"]["image_tokens"] = json!("unknown extension");
    extended["usage"]["input_tokens_details"]["text_tokens"] = json!(-1);
    for pointer in [
        "",
        "/usage",
        "/usage/input_tokens_details",
        "/usage/output_tokens_details",
        "/output/0",
        "/output/0/summary/0",
        "/output/2",
        "/output/3",
        "/output/3/content/0",
        "/output/3/content/0/annotations/0",
    ] {
        add(&mut extended, pointer);
    }
    assert_eq!(
        a.decode_response(extended.to_string().as_bytes()).unwrap(),
        a.decode_response(plain.to_string().as_bytes()).unwrap()
    );
}

#[test]
fn chat_static_response_ignores_unknown_keys_at_every_level() {
    let a = adapter(GenerationApi::Chat);
    for turn in [1, 2] {
        let plain = chat_wire::response(turn);
        let mut extended = plain.clone();
        for pointer in ["", "/usage", "/choices/0", "/choices/0/message"] {
            add(&mut extended, pointer);
        }
        if turn == 1 {
            add(&mut extended, "/choices/0/message/tool_calls/0");
            add(&mut extended, "/choices/0/message/tool_calls/0/function");
        }
        assert_eq!(
            a.decode_response(extended.to_string().as_bytes()).unwrap(),
            a.decode_response(plain.to_string().as_bytes()).unwrap()
        );
    }
}

fn stream(
    a: &GenerationCodec,
    chat: bool,
    frames: &[Value],
) -> Vec<morphiecore::semantic::task::generation::StreamEvent> {
    let mut decoder = a.event_decoder();
    let mut events = vec![];
    for frame in frames {
        events.extend(decoder.push(frame).unwrap());
    }
    if chat {
        // Chat closes on the `[DONE]` sentinel, not on the last chunk.
        events.extend(decoder.done().unwrap());
    }
    decoder.finish().unwrap();
    events
}

#[test]
fn responses_events_ignore_unknown_keys() {
    let a = adapter(GenerationApi::Responses);
    let plain = wire::events(1);
    let mut extended = plain.clone();
    for frame in &mut extended {
        add(frame, "");
        for key in ["item", "part", "response"] {
            if frame.get(key).is_some_and(Value::is_object) {
                add(frame, &format!("/{key}"));
            }
        }
    }
    assert_eq!(stream(&a, false, &extended), stream(&a, false, &plain));
}

#[test]
fn chat_chunks_ignore_unknown_keys() {
    let a = adapter(GenerationApi::Chat);
    for turn in [1, 2] {
        let plain = chat_wire::events(turn);
        let mut extended = plain.clone();
        for frame in &mut extended {
            add(frame, "");
            if frame["choices"].as_array().is_some_and(|c| !c.is_empty()) {
                add(frame, "/choices/0");
                add(frame, "/choices/0/delta");
            }
        }
        assert_eq!(stream(&a, true, &extended), stream(&a, true, &plain));
    }
}

#[test]
fn known_upstream_fields_stay_strict() {
    let a = adapter(GenerationApi::Responses);
    for (pointer, value) in [
        ("/usage/input_tokens", json!("3")),
        ("/status", json!("synthetic_status")),
        ("/output/3/content/0/text", json!(7)),
    ] {
        let mut body = wire::response(1);
        *body.pointer_mut(pointer).unwrap() = value;
        assert!(
            a.decode_response(body.to_string().as_bytes()).is_err(),
            "{pointer}"
        );
    }
    let mut body = wire::response(1);
    body["output"][1]["type"] = json!("synthetic_future_item");
    assert!(a.decode_response(body.to_string().as_bytes()).is_err());
}

#[test]
fn client_text_history_shorthand_does_not_relax_annotations() {
    use morphiecore::{
        protocol::{
            CodecError,
            fidelity::FidelityRecords,
            openai::{TargetContract, lowering::lower_request},
        },
        semantic::{task::generation::*, value::Text},
    };
    let a = adapter(GenerationApi::Responses);
    let mut history = json!({"model":"m","input":[{"role":"assistant","content":[{"type":"output_text","text":"hello"}]}]});
    let decoded = a
        .decode_client_request(history.to_string().as_bytes())
        .unwrap();
    let expected = GenerationRequest::new(
        vec![(
            ItemId::new(1),
            Item::Message(Message {
                participant: None,
                role: MessageRole::Assistant,
                phase: None,
                status: ItemLifecycle::Completed,
                parts: vec![Part::new(
                    PartId::new(1),
                    ContentPart::Text(Text::new("hello", "test", 32).unwrap().into()),
                )],
            }),
        )],
        GenerationControls::default(),
    )
    .unwrap();
    assert_eq!(decoded.task.semantic, expected);
    assert_eq!(
        decoded.task.evidence.rule_names(),
        vec!["responses.default-history-annotations.v1"]
    );
    // Independently constructed IR emits the official array, not the shorthand.
    let fidelity = FidelityRecords::default();
    let target = lower_request(
        &expected,
        &fidelity,
        GenerationApi::Responses,
        TargetContract::full(),
    )
    .unwrap();
    let encoded = morphiecore::protocol::openai::responses::encode_generation(&target).unwrap();
    assert_eq!(
        encoded["input"][0]["content"][0],
        json!({"type":"output_text","text":"hello","annotations":[]})
    );
    assert!(matches!(
        a.decode_request(history.to_string().as_bytes()),
        Err(CodecError::Invalid(
            morphiecore::protocol::failure::ShapeReason::InvalidValue
        ))
    ));
    for invalid in [
        Value::Null,
        json!({}),
        json!([{"type":"unregistered_annotation"}]),
    ] {
        history["input"][0]["content"][0]["annotations"] = invalid;
        assert!(
            a.decode_client_request(history.to_string().as_bytes())
                .is_err()
        );
    }
    history["input"][0]["content"][0]["annotations"] = json!([{
        "type":"url_citation","start_index":0,"end_index":5,"url":"https://example.test/","title":"Example"}]);
    let cited = a
        .decode_client_request(history.to_string().as_bytes())
        .unwrap();
    assert!(cited.task.evidence.is_empty());
    assert_eq!(
        a.encode_request(&cited, "m", &TargetContract::full())
            .unwrap()["input"][0]["content"][0]["annotations"],
        history["input"][0]["content"][0]["annotations"]
    );
    let mut upstream = wire::response(1);
    upstream["output"][3]["content"][0]
        .as_object_mut()
        .unwrap()
        .remove("annotations");
    assert!(matches!(
        a.decode_response(upstream.to_string().as_bytes()),
        Err(CodecError::Invalid(
            morphiecore::protocol::failure::ShapeReason::InvalidValue
        ))
    ));
}

#[test]
fn downstream_requests_still_reject_unmodeled_fields() {
    let mut body = wire::request(false);
    body["future_control"] = json!(true);
    assert!(
        adapter(GenerationApi::Responses)
            .decode_request(body.to_string().as_bytes())
            .is_err()
    );
    let body =
        json!({"model":"m","messages":[{"role":"user","content":"hi"}],"future_control":true});
    assert!(
        adapter(GenerationApi::Chat)
            .decode_request(body.to_string().as_bytes())
            .is_err()
    );
}
