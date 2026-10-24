//! Chat byte boundaries, explicit DONE lifecycle and independent static/event closure.
use crate::chat_wire as wire;
use morphiecore::{
    protocol::{
        openai::{
            Profile, TargetCaps as Contract,
            chat::{
                envelope,
                sse::{ChatSseDecoder, ChatSseEncoder},
            },
            lowering::{lower_request, lower_response},
            obfuscation::Obfuscation,
            responses,
        },
        stream::SseLimits,
    },
    semantic::{context::StreamOptions, task::generation::*, value::Presence},
};
use serde_json::{Value, json};
fn frame(value: &Value) -> Vec<u8> {
    format!("data: {value}\n\n").into_bytes()
}
fn decoder() -> ChatSseDecoder {
    ChatSseDecoder::new(
        200,
        "text/event-stream; charset=utf-8",
        SseLimits::default(),
    )
    .unwrap()
}
fn consume(d: &mut ChatSseDecoder, bytes: &[u8], size: usize) -> Vec<StreamEvent> {
    let mut events = vec![];
    for chunk in bytes.chunks(size) {
        let mut rest = chunk;
        while !rest.is_empty() {
            let (n, e) = d.consume(rest).unwrap();
            assert!(n > 0 && n <= rest.len());
            rest = &rest[n..];
            events.extend(e);
        }
    }
    events
}
fn options(usage: bool) -> StreamOptions {
    StreamOptions {
        include_usage: Presence::Value(usage),
        include_obfuscation: Presence::Value(false),
    }
}

#[test]
fn decoded_padding_has_an_aggregate_utf8_budget_and_poisoned_rejection() {
    use morphiecore::protocol::stream::SseError;
    let mut chunks = wire::events(2);
    for chunk in chunks.iter_mut().take(2) {
        chunk["obfuscation"] = json!("🧪");
    }
    let bytes: Vec<_> = chunks
        .iter()
        .flat_map(frame)
        .chain(b"data: [DONE]\n\n".iter().copied())
        .collect();
    for size in [1, bytes.len()] {
        let mut exact = ChatSseDecoder::new(
            200,
            "text/event-stream",
            SseLimits {
                max_obfuscation_bytes: 8,
                ..Default::default()
            },
        )
        .unwrap();
        consume(&mut exact, &bytes, size);
        exact.finish().unwrap();
        let mut limited = ChatSseDecoder::new(
            200,
            "text/event-stream",
            SseLimits {
                max_obfuscation_bytes: 7,
                ..Default::default()
            },
        )
        .unwrap();
        let mut failure = None;
        'chunks: for chunk in bytes.chunks(size) {
            let mut rest = chunk;
            while !rest.is_empty() {
                match limited.consume(rest) {
                    Ok((used, _)) => {
                        assert!(used > 0);
                        rest = &rest[used..];
                    }
                    Err(error) => {
                        failure = Some(error);
                        break 'chunks;
                    }
                }
            }
        }
        assert!(matches!(failure, Some(SseError::Limit)));
        assert!(matches!(limited.consume(&bytes), Err(SseError::Closed)));
        assert!(matches!(limited.finish(), Err(SseError::Closed)));
    }
}

#[test]
fn chat_rejects_uncarried_cache_boundaries_before_client_field_filtering() {
    use morphiecore::protocol::{
        CodecError,
        openai::{Adapter, Dialect},
    };
    // All text-array owners and standard media use the same cache-boundary guard.
    for dialect in [Dialect::Standard, Dialect::Bailian] {
        let adapter = Adapter::new(Profile::Chat, dialect, None);
        for (role, part) in [
            ("system", json!({"type":"text","text":"instruction"})),
            ("developer", json!({"type":"text","text":"instruction"})),
            ("assistant", json!({"type":"text","text":"answer"})),
            ("tool", json!({"type":"text","text":"result"})),
            ("user", json!({"type":"text","text":"input"})),
            (
                "user",
                json!({"type":"image_url","image_url":{"url":"https://example.test/a.png"}}),
            ),
            (
                "user",
                json!({"type":"input_audio","input_audio":{"data":"AQID","format":"wav"}}),
            ),
            (
                "user",
                json!({"type":"file","file":{"file_data":"data:application/pdf;base64,AQID","filename":"a.pdf"}}),
            ),
        ] {
            let mut message = json!({"role":role,"content":[part]});
            let mut messages = vec![];
            if role == "tool" {
                messages.push(json!({"role":"assistant","tool_calls":[
                    {"id":"c","type":"function","function":{"name":"read","arguments":"{}"}}
                ]}));
                message["tool_call_id"] = json!("c");
            }
            messages.push(message);
            let mut source = json!({"model":"synthetic","messages":messages});
            let baseline = adapter.decode_client_request(source.to_string().as_bytes());
            if dialect == Dialect::Bailian && role == "developer" {
                assert!(
                    matches!(baseline,Err(CodecError::Unsupported(field)) if field=="Bailian Chat controls")
                );
            } else {
                baseline.unwrap();
            }
            let last = source["messages"].as_array().unwrap().len() - 1;
            for marker in [json!({"mode":"explicit"}), Value::Null] {
                source["messages"][last]["content"][0]["prompt_cache_breakpoint"] = marker;
                for result in [
                    adapter.decode_client_request(source.to_string().as_bytes()),
                    adapter.decode_request(source.to_string().as_bytes()),
                ] {
                    assert!(
                        matches!(
                            result,
                            Err(CodecError::Unsupported(field))
                                if field == "Chat prompt_cache_breakpoint"
                        ),
                        "{dialect:?} {role}"
                    );
                }
            }
        }
    }
}

#[test]
fn chat_static_and_streamed_moderation_reports_cannot_disappear() {
    use morphiecore::protocol::{
        CodecError,
        openai::{Adapter, Dialect},
        stream::SseError,
    };
    let report =
        json!({"input":{"type":"error","code":"moderation_unavailable","message":"synthetic"}});
    for dialect in [Dialect::Standard, Dialect::Bailian] {
        let adapter = Adapter::new(Profile::Chat, dialect, None);
        let mut source = wire::response(2);
        source["unknown_extension"] = json!({"ignored":true});
        source["moderation"] = Value::Null;
        adapter
            .decode_response(source.to_string().as_bytes())
            .unwrap();
        source["moderation"] = report.clone();
        assert!(matches!(
            adapter.decode_response(source.to_string().as_bytes()),
            Err(CodecError::Unsupported(field)) if field == "active moderation report"
        ));

        let opening = json!({"id":"synthetic-response","object":"chat.completion.chunk","created":1,
            "model":"synthetic-model","choices":[{"index":0,"delta":{"role":"assistant","content":"answer"},
            "finish_reason":null}],"unknown_extension":true,"moderation":null});
        let finish = json!({"id":"synthetic-response","object":"chat.completion.chunk","created":1,
            "model":"synthetic-model","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]});
        let mut tail = opening.clone();
        tail["choices"] = json!([]);
        tail["usage"] = json!({"prompt_tokens":1,"completion_tokens":1,"total_tokens":2});
        let new_decoder = || {
            ChatSseDecoder::with_decoder(
                200,
                "text/event-stream",
                SseLimits::default(),
                adapter.event_decoder(),
            )
            .unwrap()
        };
        let mut control = new_decoder();
        for chunk in [&opening, &finish, &tail] {
            assert!(
                consume(&mut control, &frame(chunk), 1)
                    .iter()
                    .all(|event| !matches!(event, StreamEvent::Terminal { .. }))
            );
        }
        consume(&mut control, b"data: [DONE]\n\n", 1);
        control.finish().unwrap();
        assert_eq!(
            control.materialize().unwrap().semantic.outcome(),
            Outcome::Completed
        );
        // Exercise both the first chunk and a usage tail after the finish.
        for late in [false, true] {
            let mut d = new_decoder();
            let mut moderated = opening.clone();
            if late {
                for chunk in [&opening, &finish] {
                    assert!(
                        consume(&mut d, &frame(chunk), 1)
                            .iter()
                            .all(|event| !matches!(event, StreamEvent::Terminal { .. }))
                    );
                }
                moderated = tail.clone();
            }
            moderated["moderation"] = report.clone();
            assert!(matches!(
                d.consume(&frame(&moderated)),
                Err(SseError::Codec(CodecError::Unsupported(field)))
                    if field == "active moderation report"
            ));
            assert!(matches!(
                d.consume(b"data: [DONE]\n\n"),
                Err(SseError::Closed)
            ));
            assert!(matches!(d.finish(), Err(SseError::Closed)));
            assert!(matches!(d.materialize(), Err(SseError::Closed)));
        }
    }
}

#[test]
fn complete_chat_envelope_separates_delivery_and_rejects_unsupported_admission() {
    let source = json!({"model":"fixture-model","messages":[{"role":"user","content":"hello"}],"top_p":0.8,"n":1,"stream":true,"stream_options":{"include_usage":true,"include_obfuscation":false}});
    let d = envelope::decode_request_bytes(&serde_json::to_vec(&source).unwrap()).unwrap();
    let out = envelope::encode_request(
        &lower_request(
            &d.task.semantic,
            &d.task.fidelity,
            Profile::Chat,
            Contract::full(),
        )
        .unwrap(),
        &d.context,
    )
    .unwrap();
    assert_eq!(out, source);
    let client = morphiecore::protocol::openai::Adapter::new(
        Profile::Chat,
        morphiecore::protocol::openai::Dialect::Standard,
        None,
    );
    client
        .decode_client_request(source.to_string().as_bytes())
        .unwrap();
    let mut moderated = source.clone();
    moderated["moderation"] = json!({
        "model":"omni-moderation-latest",
        "policy":{"input":{"mode":"block"},"output":{"mode":"block"}}
    });
    assert!(matches!(
        client.decode_client_request(moderated.to_string().as_bytes()),
        Err(morphiecore::protocol::CodecError::Unsupported(reason))
            if reason == "Chat moderation"
    ));
    let projected = responses::encode_generation(
        &lower_request(
            &d.task.semantic,
            &d.task.fidelity,
            Profile::Responses,
            Contract::full(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        projected,
        json!({"input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"hello"}]}],"top_p":0.8})
    );
    for (key, value) in [
        ("model", Value::Null),
        ("n", json!(2)),
        ("n", json!(0)),
        ("top_p", json!(1.1)),
        ("stream", json!(false)),
        ("stream_options", json!({"include_usage":null})),
        ("response_format", json!({"type":"json_schema"})),
        ("base_url", json!("https://example.test")),
    ] {
        let mut bad = source.clone();
        bad[key] = value;
        assert!(
            envelope::decode_request_bytes(&serde_json::to_vec(&bad).unwrap()).is_err(),
            "{key}"
        );
    }
    assert!(
        envelope::decode_request_bytes(
            br#"{"model":"a","model":"b","messages":[{"role":"user","content":"x"}]}"#
        )
        .is_err()
    );
    for (key, value) in [
        ("created", json!(1.5)),
        ("id", Value::Null),
        ("choices", json!([])),
    ] {
        let mut bad = wire::response(2);
        bad[key] = value;
        assert!(envelope::decode_response_bytes(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    let mut bad = wire::response(2);
    bad["choices"][0]["message"]["role"] = json!("user");
    assert!(envelope::decode_response_bytes(&serde_json::to_vec(&bad).unwrap()).is_err());
}

#[test]
fn chat_refusal_empty_and_length_have_independent_static_event_meaning() {
    for (field, text, reason) in [
        ("content", "", "stop"),
        ("refusal", "no", "stop"),
        ("content", "partial", "length"),
    ] {
        let mut values = wire::events(2);
        values[1]["choices"][0]["delta"] = json!({field:text});
        values.remove(2);
        values[2]["choices"][0]["finish_reason"] = json!(reason);
        let mut d = decoder();
        for v in values {
            consume(&mut d, &frame(&v), 1);
        }
        consume(&mut d, b"data: [DONE]\n\n", 1);
        d.finish().unwrap();
        let r = d.materialize().unwrap();
        let out = envelope::encode_response(
            &lower_response(
                &r.semantic,
                &r.fidelity,
                &r.metadata,
                Profile::Chat,
                Contract::full(),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(out["choices"][0]["message"][field], text);
        assert_eq!(out["choices"][0]["finish_reason"], reason);
    }
}

#[test]
fn malformed_late_or_unfinished_chat_streams_cannot_recover() {
    for bad in [
        b"data: [DONE]\n\n".to_vec(),
        b"data: {\"a\":1,\"a\":2}\n\n".to_vec(),
        b"data: invalid\n\n".to_vec(),
        b"event: response.created\ndata: {}\n\n".to_vec(),
        frame(
            &json!({"id":"r","object":"chat.completion.chunk","created":1,"model":"m",
            "choices":[{"index":0,"delta":{"function_call":{"name":"lookup","arguments":"{}"}},
                "finish_reason":null}]}),
        ),
    ] {
        let mut d = decoder();
        assert!(d.consume(&bad).is_err());
        assert!(d.consume(&frame(&wire::events(2)[0])).is_err());
        assert!(d.finish().is_err());
        assert!(d.materialize().is_err());
    }
    let values = wire::events(2);
    let mut late = decoder();
    for value in &values[..values.len() - 1] {
        consume(&mut late, &frame(value), 4096);
    }
    assert!(late.consume(&frame(&values[1])).is_err());
    assert!(late.consume(b"data: [DONE]\n\n").is_err());
    assert!(late.materialize().is_err());
    for tail in [b"".as_slice(), b"data: [DONE]\n"] {
        let mut d = decoder();
        for v in wire::events(2) {
            consume(&mut d, &frame(&v), 4096);
        }
        consume(&mut d, tail, 1);
        assert!(d.finish().is_err());
        assert!(d.materialize().is_err());
    }
    let mut d = decoder();
    for v in wire::events(2) {
        consume(&mut d, &frame(&v), 4096);
    }
    consume(&mut d, b"data: [DONE]\n\n", 1);
    assert!(d.consume(b"data: [DONE]\n\n").is_err());
    assert!(d.finish().is_err());
    let mut d = decoder();
    let function = wire::events(1);
    for chunk in &function[..2] {
        consume(&mut d, &frame(chunk), 4096);
    }
    let mut mixed = function[2].clone();
    mixed["choices"][0]["delta"]["tool_calls"][0]["custom"] = json!({"input":"SELECT 1"});
    assert!(d.consume(&frame(&mixed)).is_err());
    assert!(d.materialize().is_err());
    for (status, content) in [
        (201, "text/event-stream"),
        (200, "application/json"),
        (200, "text/event-stream; charset=latin1"),
    ] {
        assert!(ChatSseDecoder::new(status, content, SseLimits::default()).is_err());
    }
}

#[test]
fn chat_encoder_padding_limits_do_not_emit_done() {
    let mut d = decoder();
    let mut events = vec![];
    for v in wire::events(2) {
        events.extend(consume(&mut d, &frame(&v), 4096));
    }
    events.extend(consume(&mut d, b"data: [DONE]\n\n", 4096));
    d.finish().unwrap();
    let r = d.materialize().unwrap();
    let padded = StreamOptions {
        include_usage: Presence::Value(true),
        include_obfuscation: Presence::Value(true),
    };
    assert!(
        ChatSseEncoder::new(
            r.metadata.clone(),
            Contract::full(),
            SseLimits::default(),
            padded.clone(),
            Obfuscation::Disabled
        )
        .is_err()
    );
    let mut encoder = ChatSseEncoder::new(
        r.metadata.clone(),
        Contract::full(),
        SseLimits::default(),
        padded.clone(),
        Obfuscation::Seeded([3; 32]),
    )
    .unwrap();
    let mut bytes = vec![];
    for event in &events {
        for frame in encoder.encode(event, &r.fidelity).unwrap() {
            bytes.extend(frame);
        }
    }
    encoder.finish().unwrap();
    assert!(
        std::str::from_utf8(&bytes)
            .unwrap()
            .contains("\"obfuscation\":\"")
    );
    let mut decoded = decoder();
    consume(&mut decoded, &bytes, 1);
    decoded.finish().unwrap();
    assert_eq!(decoded.materialize().unwrap().semantic, r.semantic);
    for limits in [
        SseLimits {
            max_obfuscation_bytes: 0,
            ..Default::default()
        },
        SseLimits {
            max_event_bytes: 1,
            ..Default::default()
        },
        SseLimits {
            max_wire_bytes: 1,
            ..Default::default()
        },
    ] {
        let mut encoder = ChatSseEncoder::new(
            r.metadata.clone(),
            Contract::full(),
            limits,
            padded.clone(),
            Obfuscation::Seeded([3; 32]),
        )
        .unwrap();
        assert!(encoder.encode(&StreamEvent::Started, &r.fidelity).is_err());
        assert!(encoder.finish().is_err());
        assert!(encoder.encode(events.last().unwrap(), &r.fidelity).is_err());
    }
}

#[test]
fn invalid_chat_usage_cannot_fall_back_to_null_or_recover_at_done() {
    for usage in [
        json!({"prompt_tokens":1,"completion_tokens":2,"total_tokens":4}),
        json!({"prompt_tokens":-1,"completion_tokens":2,"total_tokens":1}),
        json!({"prompt_tokens":"1","completion_tokens":2,"total_tokens":3}),
    ] {
        let mut values = wire::events(2);
        values.last_mut().unwrap()["usage"] = usage;
        let mut d = decoder();
        for value in &values[..values.len() - 1] {
            consume(&mut d, &frame(value), 1);
        }
        assert!(d.consume(&frame(values.last().unwrap())).is_err());
        assert!(d.consume(b"data: [DONE]\n\n").is_err());
        assert!(d.finish().is_err());
        assert!(d.materialize().is_err());
    }
}

#[test]
fn probability_bytes_and_empty_reports_survive_fragmentation_without_a_fake_done() {
    for (key, empty) in [
        ("content", false),
        ("refusal", false),
        ("content", true),
        ("refusal", true),
    ] {
        let probabilities = if empty {
            json!([])
        } else {
            json!([{"token":"🧪","logprob":-0.125,"bytes":[240,159,167,170],"top_logprobs":[{"token":"x","logprob":-2.25,"bytes":null}]}])
        };
        let mut source = wire::response(2);
        source.as_object_mut().unwrap().shift_remove("usage");
        source["choices"][0]["message"] = if key == "content" {
            json!({"role":"assistant","content":"🧪"})
        } else {
            json!({"role":"assistant","content":null,"refusal":"🧪"})
        };
        source["choices"][0]["logprobs"] = json!({key:probabilities.clone()});
        let expected = envelope::decode_response_bytes(source.to_string().as_bytes()).unwrap();
        let payload = |delta: Value, probs: Value, finish: Value| json!({"id":"chat-local","object":"chat.completion.chunk","created":1,"model":"fixture-model","choices":[{"index":0,"delta":delta,"logprobs":probs,"finish_reason":finish}]});
        for size in [1, 7, 4096] {
            let mut decoder = decoder();
            let mut events = consume(
                &mut decoder,
                &frame(&payload(
                    json!({key:"🧪"}),
                    json!({key:probabilities.clone()}),
                    Value::Null,
                )),
                size,
            );
            events.extend(consume(
                &mut decoder,
                &frame(&payload(json!({}), Value::Null, json!("stop"))),
                size,
            ));
            assert!(decoder.materialize().is_err());
            events.extend(consume(&mut decoder, b"data: [DONE]\n\n", size));
            decoder.finish().unwrap();
            assert_eq!(decoder.materialize().unwrap().semantic, expected.semantic);
            let mut encoder = ChatSseEncoder::new(
                expected.metadata.clone(),
                Contract::full(),
                SseLimits::default(),
                options(false),
                Obfuscation::Disabled,
            )
            .unwrap();
            let mut bytes = Vec::new();
            for event in &events {
                for frame in encoder.encode(event, &expected.fidelity).unwrap() {
                    bytes.extend_from_slice(&frame)
                }
            }
            encoder.finish().unwrap();
            let text = String::from_utf8(bytes.clone()).unwrap();
            assert_eq!(text.matches("data: [DONE]").count(), 1);
            let reported: Vec<_> = text
                .split("\n\n")
                .filter_map(|frame| frame.strip_prefix("data: "))
                .filter(|s| *s != "[DONE]")
                .filter_map(|s| serde_json::from_str::<Value>(s).ok())
                .filter_map(|v| v["choices"][0]["logprobs"][key].as_array().cloned())
                .collect();
            assert_eq!(reported, vec![probabilities.as_array().unwrap().clone()]);
            let mut replay = self::decoder();
            consume(&mut replay, &bytes, size);
            replay.finish().unwrap();
            assert_eq!(replay.materialize().unwrap().semantic, expected.semantic);
        }
        let mut bad = decoder();
        let mut invalid = frame(&payload(
            json!({key:"🧪"}),
            json!({key:[{"token":"🧪","logprob":-1}]}),
            Value::Null,
        ));
        invalid.extend_from_slice(b"data: [DONE]\n\n");
        assert!(bad.consume(&invalid).is_err());
        assert!(bad.finish().is_err());
        assert!(bad.materialize().is_err());
    }
}
#[test]
fn dashscope_reasoning_cannot_resume_after_text_or_call_publication() {
    use morphiecore::protocol::{
        CodecError,
        openai::{Adapter, Dialect},
        stream::SseError,
    };
    let adapter = Adapter::new(Profile::Chat, Dialect::Bailian, None);
    let chunk = |delta: Value| {
        json!({"id":"r","object":"chat.completion.chunk","created":1,"model":"m",
        "choices":[{"index":0,"delta":delta,"finish_reason":null}]})
    };
    for visible in [
        json!({"content":"answer"}),
        json!({"tool_calls":[
        {"index":0,"id":"c","type":"function","function":{"name":"lookup","arguments":""}}]}),
    ] {
        let mut d = ChatSseDecoder::with_decoder(
            200,
            "text/event-stream",
            SseLimits::default(),
            adapter.event_decoder(),
        )
        .unwrap();
        consume(
            &mut d,
            &frame(&chunk(json!({"reasoning_content":"before"}))),
            1,
        );
        consume(&mut d, &frame(&chunk(visible)), 1);
        assert!(
            matches!(d.consume(&frame(&chunk(json!({"reasoning_content":"late"})))),
            Err(SseError::Codec(CodecError::Unsupported(field))) if field=="late Chat reasoning owner")
        );
        assert!(matches!(
            d.consume(b"data: [DONE]\n\n"),
            Err(SseError::Closed)
        ));
        assert!(d.finish().is_err());
    }
}
