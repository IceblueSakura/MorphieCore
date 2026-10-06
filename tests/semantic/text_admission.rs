//! Admission regressions for the existing text subset, not the whole SDK union.
use crate::wire;
use morphiecore::{
    adapter::{Adapter, Dialect},
    protocol::openai::{
        Profile,
        events::EventDecoder,
        sse::{ResponsesSseDecoder, SseLimits, encode_frame},
    },
};
use serde_json::{Value, json};

#[test]
fn function_dispatch_nonnullable_flags_do_not_accept_null_as_an_inactive_default() {
    let adapter = Adapter::new(Profile::Responses, Dialect::Standard, None);
    for name in ["async", "defer_loading"] {
        for value in [Value::Null, json!(0), json!("false"), json!({})] {
            let mut tool =
                json!({"type":"function","name":"lookup","parameters":{},"strict":false});
            tool[name] = value;
            let request = json!({"model":"m","input":"lookup","tools":[tool]});
            assert!(
                adapter
                    .decode_request(request.to_string().as_bytes())
                    .is_err(),
                "{name}"
            );
        }
        for value in [None, Some(json!(false)), Some(json!(true))] {
            let mut tool =
                json!({"type":"function","name":"lookup","parameters":{},"strict":false});
            if let Some(value) = &value {
                tool[name] = value.clone();
            }
            let request = json!({"model":"m","input":"lookup","tools":[tool]});
            let decoded = adapter
                .decode_request(request.to_string().as_bytes())
                .unwrap();
            let encoded = adapter
                .encode_request(
                    &decoded,
                    "m",
                    &morphiecore::lowering::generation::GenerationRepresentationContract::full(),
                )
                .unwrap();
            assert_eq!(
                encoded["tools"][0].get(name),
                value.filter(|v| v == true).as_ref()
            );
        }
    }
}

#[test]
fn text_delta_and_done_require_probability_arrays_at_both_event_boundaries() {
    for kind in ["response.output_text.delta", "response.output_text.done"] {
        for invalid in [None, Some(Value::Null), Some(json!({}))] {
            let mut events = wire::events(2);
            let target = events.iter_mut().find(|v| v["type"] == kind).unwrap();
            if let Some(value) = invalid {
                target["logprobs"] = value;
            } else {
                target.as_object_mut().unwrap().shift_remove("logprobs");
            }
            let mut decoder = EventDecoder::new(Profile::Responses);
            let mut rejected = false;
            for event in &events {
                if decoder.push(event).is_err() {
                    rejected = true;
                    break;
                }
            }
            assert!(rejected, "missing/null/malformed required logprobs: {kind}");
            assert!(decoder.push(events.last().unwrap()).is_err());
            assert!(decoder.materialize().is_err());
            let mut decoder =
                ResponsesSseDecoder::new(200, "text/event-stream", SseLimits::default(), None)
                    .unwrap();
            let mut rejected = false;
            for event in &events {
                let frame = encode_frame(event, 1 << 20).unwrap();
                if decoder.consume(&frame).is_err() {
                    rejected = true;
                    break;
                }
            }
            assert!(rejected);
            assert!(decoder.finish().is_err());
        }
    }
}

#[test]
fn function_and_custom_event_payloads_keep_branch_specific_admission_after_fragmentation() {
    use morphiecore::protocol::openai::sse::SseError;
    fn feed(decoder: &mut ResponsesSseDecoder, frame: &[u8], size: usize) -> Result<(), SseError> {
        for chunk in frame.chunks(size) {
            let mut rest = chunk;
            while !rest.is_empty() {
                let (used, _) = decoder.consume(rest)?;
                assert!(used > 0);
                rest = &rest[used..];
            }
        }
        Ok(())
    }
    for (kind, payload, foreign) in [
        (
            "response.function_call_arguments.delta",
            "delta",
            "logprobs",
        ),
        (
            "response.function_call_arguments.done",
            "arguments",
            "input",
        ),
        ("response.custom_tool_call_input.delta", "delta", "logprobs"),
        ("response.custom_tool_call_input.done", "input", "arguments"),
    ] {
        let events = wire::events(1);
        let index = events.iter().position(|v| v["type"] == kind).unwrap();
        let original = &events[index];
        let mut invalid_events = Vec::new();
        for key in ["item_id", "output_index", payload] {
            for invalid in [None, Some(Value::Null), Some(json!(false))] {
                let mut event = original.clone();
                if let Some(value) = invalid {
                    event[key] = value;
                } else {
                    event.as_object_mut().unwrap().shift_remove(key);
                }
                invalid_events.push(event);
            }
        }
        let mut foreign_field = original.clone();
        foreign_field[foreign] = if foreign == "logprobs" {
            json!([])
        } else {
            json!("")
        };
        invalid_events.push(foreign_field);
        let mut wrong_kind = original.clone();
        wrong_kind["type"] = json!(kind.replace(
            if kind.contains("function_call_arguments") {
                "function_call_arguments"
            } else {
                "custom_tool_call_input"
            },
            if kind.contains("function_call_arguments") {
                "custom_tool_call_input"
            } else {
                "function_call_arguments"
            },
        ));
        invalid_events.push(wrong_kind);
        for invalid in invalid_events {
            let mut decoder = EventDecoder::new(Profile::Responses);
            for prefix in &events[..index] {
                decoder.push(prefix).unwrap();
            }
            assert!(decoder.push(&invalid).is_err(), "{kind}: {invalid}");
            assert!(decoder.push(original).is_err());
            assert!(decoder.finish().is_err());
            assert!(decoder.materialize().is_err());
            let frame = encode_frame(&invalid, 1 << 20).unwrap();
            for size in [1, 17, frame.len()] {
                let mut decoder =
                    ResponsesSseDecoder::new(200, "text/event-stream", SseLimits::default(), None)
                        .unwrap();
                for prefix in &events[..index] {
                    feed(&mut decoder, &encode_frame(prefix, 1 << 20).unwrap(), size).unwrap();
                }
                assert!(
                    feed(&mut decoder, &frame, size).is_err(),
                    "{kind}: {invalid}"
                );
                assert!(
                    feed(
                        &mut decoder,
                        &encode_frame(events.last().unwrap(), 1 << 20).unwrap(),
                        size
                    )
                    .is_err()
                );
                assert!(decoder.finish().is_err());
                assert!(decoder.materialize().is_err());
            }
        }
    }
}

#[test]
fn chat_nullable_continuation_fields_do_not_replace_call_identity_or_arguments() {
    let adapter = Adapter::new(Profile::Chat, Dialect::Standard, None);
    let chunk = |delta: Value, finish: Value| json!({"id":"c","object":"chat.completion.chunk","created":1,"model":"m","choices":[{"index":0,"delta":delta,"finish_reason":finish}]});
    let mut decoder = adapter.event_decoder();
    decoder.push(&chunk(json!({"tool_calls":[{"index":0,"id":"call","type":"function","function":{"name":"lookup","arguments":"{}"}}]}),Value::Null)).unwrap();
    for call in [
        json!({"index":0}),
        json!({"index":0,"function":null}),
        json!({"index":0,"id":null,"type":null,"function":{"name":null,"arguments":null}}),
    ] {
        assert!(
            decoder
                .push(&chunk(json!({"tool_calls":[call]}), Value::Null))
                .unwrap()
                .is_empty()
        );
    }
    decoder
        .push(&chunk(json!({}), json!("tool_calls")))
        .unwrap();
    decoder.done().unwrap();
    let decoded = decoder.materialize().unwrap();
    let morphiecore::semantic::task::generation::Item::ToolCall(call) =
        &decoded.semantic.items()[1].1
    else {
        panic!("call")
    };
    assert_eq!(call.arguments.as_raw(), Some("{}"));
    assert_eq!(call.call_id.as_str(), "call");
    let mut decoder = adapter.event_decoder();
    assert!(
        decoder
            .push(&chunk(
                json!({"tool_calls":[{"index":0,"function":null}]}),
                Value::Null
            ))
            .is_err()
    );
    assert!(decoder.done().is_err());
}

#[test]
fn role_specific_history_fields_cannot_disappear_during_decode() {
    let adapter = Adapter::new(Profile::Chat, Dialect::MorphieCore, None);
    for role in ["user", "system", "developer", "tool"] {
        let mut message = if role == "tool" {
            json!({"role":"tool","tool_call_id":"call","content":"result"})
        } else {
            json!({"role":role,"content":"hello"})
        };
        let request = |message: Value| {
            let mut messages = Vec::new();
            if role == "tool" {
                messages.push(json!({"role":"assistant","content":null,"tool_calls":[{"id":"call","type":"function","function":{"name":"lookup","arguments":"{}"}}]}));
            }
            messages.push(message);
            json!({"model":"m","messages":messages})
        };
        assert!(
            adapter
                .decode_request(request(message.clone()).to_string().as_bytes())
                .is_ok()
        );
        for (key, value) in [
            ("refusal", json!("must not disappear")),
            ("refusal", Value::Null),
            ("reasoning_content", Value::Null),
            ("reasoning_details", Value::Null),
        ] {
            message[key] = value;
            assert!(
                adapter
                    .decode_request(request(message.clone()).to_string().as_bytes())
                    .is_err(),
                "{role}: {key}"
            );
            message.as_object_mut().unwrap().shift_remove(key);
        }
    }
}

#[test]
fn nullable_reasoning_effort_and_optional_function_headers_follow_their_own_schema() {
    let adapter = Adapter::new(Profile::Chat, Dialect::Standard, None);
    let wire =
        json!({"model":"m","messages":[{"role":"user","content":"hi"}],"reasoning_effort":null});
    let decoded = adapter.decode_request(wire.to_string().as_bytes()).unwrap();
    let contract = morphiecore::lowering::generation::GenerationRepresentationContract::full();
    assert!(
        adapter
            .encode_request(&decoded, "m", &contract)
            .unwrap()
            .get("reasoning_effort")
            .unwrap()
            .is_null()
    );
    let wire = json!({"id":"r","object":"response","created_at":0,"model":"m","status":"completed","output":[{"id":null,"type":"function_call","call_id":"call","name":"lookup","arguments":"{}","status":null}]});
    let decoded = Adapter::new(Profile::Responses, Dialect::Standard, None)
        .decode_response(wire.to_string().as_bytes())
        .unwrap();
    assert!(
        decoded
            .fidelity
            .response_item_id(morphiecore::semantic::task::generation::ItemId::new(1))
            .is_none()
    );
    assert_eq!(
        decoded.semantic.continuation(),
        morphiecore::semantic::task::generation::Continuation::ToolResults(vec![
            morphiecore::semantic::task::generation::CallReference {
                item: morphiecore::semantic::task::generation::ItemId::new(1),
                call_id: "call",
            },
        ])
    );
}

#[test]
fn known_unrepresentable_event_fields_and_changed_bound_metadata_fail_early() {
    use morphiecore::{
        protocol::{
            fidelity::FidelityRecords,
            openai::{ResponseMetadata, events::EventEncoder},
        },
        semantic::{
            context::ServiceTier,
            task::generation::*,
            value::{Presence, Text},
        },
    };
    let metadata = ResponseMetadata {
        id: "c".into(),
        model: "m".into(),
        created: 1.into(),
        context: Default::default(),
        instruction_fidelity: Default::default(),
    };
    let fidelity = FidelityRecords::default();
    for kind in [
        ItemKind::Message {
            phase: Some(Phase::FinalAnswer),
        },
        ItemKind::ToolCall {
            format: ArgumentFormat::Raw,
            call_id: Text::new("call", "test", 128).unwrap(),
            name: Text::new("lookup", "test", 128).unwrap(),
            message: None,
            context: CallContext {
                namespace: Some(Text::new("scope", "test", 128).unwrap()),
                ..Default::default()
            },
        },
    ] {
        let mut encoder = EventEncoder::new(Profile::Chat, metadata.clone()).unwrap();
        encoder.encode(&StreamEvent::Started, &fidelity).unwrap();
        assert!(
            encoder
                .encode(
                    &StreamEvent::ItemStarted {
                        item: ItemId::new(1),
                        kind,
                        replay: None
                    },
                    &fidelity
                )
                .is_err()
        );
        assert!(encoder.finish().is_err());
    }
    let mut metadata = metadata;
    metadata.context.execution.service_tier = Presence::Value(ServiceTier::Default);
    let mut encoder = EventEncoder::new(Profile::Chat, metadata.clone()).unwrap();
    encoder.encode(&StreamEvent::Started, &fidelity).unwrap();
    metadata.context.execution.service_tier = Presence::Value(ServiceTier::Priority);
    assert!(encoder.update_metadata(metadata).is_err());
    assert!(encoder.finish().is_err());
}

#[test]
fn chat_reported_context_is_protocol_specific_and_never_guessed() {
    let adapter = Adapter::new(Profile::Chat, Dialect::Standard, None);
    let mut wire = json!({"id":"c","object":"chat.completion","created":1,"model":"m","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}],"metadata":{"trace":"reported"},"service_tier":"priority"});
    let mut decoded = adapter
        .decode_response(wire.to_string().as_bytes())
        .unwrap();
    assert_eq!(
        decoded.metadata.context.execution.metadata.value().unwrap()["trace"],
        "reported"
    );
    let contract = morphiecore::lowering::generation::GenerationRepresentationContract::full();
    assert_eq!(
        adapter.encode_response(&decoded, &contract).unwrap()["metadata"],
        wire["metadata"]
    );
    decoded.metadata.context.execution.metadata = morphiecore::semantic::value::Presence::Absent;
    assert!(
        adapter
            .encode_response(&decoded, &contract)
            .unwrap()
            .get("metadata")
            .is_none()
    );
    wire["service_tier"] = json!("ultrafast");
    assert!(
        adapter
            .decode_response(wire.to_string().as_bytes())
            .is_err()
    );
}
