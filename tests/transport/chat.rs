//! Chat byte boundaries, explicit DONE lifecycle and independent static/event closure.
use crate::chat_wire as wire;
use morphiecore::{
    lowering::generation::{
        GenerationRepresentationContract as Contract, lower_request, lower_response,
    },
    protocol::{
        fidelity::FidelityRecords,
        openai::{
            Profile, chat, chat_envelope as envelope,
            chat_sse::{ChatSseDecoder, ChatSseEncoder},
            responses,
            sse::{Obfuscation, SseLimits},
        },
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
fn fragmented_chat_stream_matches_static_and_done_is_not_finish_reason() {
    for turn in [1, 2] {
        let expected =
            envelope::decode_response_bytes(&serde_json::to_vec(&wire::response(turn)).unwrap())
                .unwrap();
        for size in [1, 4096] {
            let mut d = decoder();
            for v in wire::events(turn) {
                consume(&mut d, &frame(&v), size);
            }
            assert!(d.materialize().is_err());
            let events = consume(&mut d, b"data: [DONE]\n\n", size);
            assert!(matches!(
                events.last(),
                Some(StreamEvent::Terminal {
                    terminal: StreamTerminal::Completed,
                    ..
                })
            ));
            d.finish().unwrap();
            assert_eq!(d.materialize().unwrap().semantic, expected.semantic);
        }
    }
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
fn content_filter_stream_keeps_partial_output_usage_and_requires_done() {
    let mut values = wire::events(2);
    values[3]["choices"][0]["finish_reason"] = json!("content_filter");
    let mut d = decoder();
    let mut events = vec![];
    for v in &values {
        events.extend(consume(&mut d, &frame(v), 1));
    }
    assert!(d.materialize().is_err());
    events.extend(consume(&mut d, b"data: [DONE]\n\n", 1));
    d.finish().unwrap();
    let r = d.materialize().unwrap();
    assert_eq!(r.semantic.outcome(), Outcome::Incomplete);
    assert_eq!(
        r.semantic.details().incomplete,
        Some(IncompleteReason::ContentFilter)
    );
    assert_eq!(r.semantic.usage().unwrap().total_tokens, Some(5));
    let mut static_wire = wire::response(2);
    static_wire["choices"][0]["finish_reason"] = json!("content_filter");
    assert_eq!(
        r.semantic,
        envelope::decode_response(&static_wire).unwrap().semantic
    );
    let mut encoder = ChatSseEncoder::new(
        r.metadata.clone(),
        Contract::full(),
        SseLimits::default(),
        options(true),
        Obfuscation::Disabled,
    )
    .unwrap();
    let mut encoded = vec![];
    for event in events {
        for frame in encoder.encode(&event, &r.fidelity).unwrap() {
            encoded.extend(frame);
        }
    }
    encoder.finish().unwrap();
    let text = std::str::from_utf8(&encoded).unwrap();
    assert!(text.contains("\"finish_reason\":\"content_filter\""));
    assert!(!text.contains("\"finish_reason\":\"length\""));
    assert_eq!(text.matches("data: [DONE]").count(), 1);
    let mut missing = decoder();
    for v in &values {
        consume(&mut missing, &frame(v), 4096);
    }
    assert!(missing.finish().is_err());
    assert!(missing.consume(b"data: [DONE]\n\n").is_err());
}

#[test]
fn malformed_late_or_unfinished_chat_streams_cannot_recover() {
    for bad in [
        b"data: [DONE]\n\n".to_vec(),
        b"data: {\"a\":1,\"a\":2}\n\n".to_vec(),
        b"data: invalid\n\n".to_vec(),
        b"event: response.created\ndata: {}\n\n".to_vec(),
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
    for (status, content) in [
        (201, "text/event-stream"),
        (200, "application/json"),
        (200, "text/event-stream; charset=latin1"),
    ] {
        assert!(ChatSseDecoder::new(status, content, SseLimits::default()).is_err());
    }
}

#[test]
fn chat_encoder_uses_final_events_and_respects_usage_delivery() {
    let mut d = decoder();
    let mut events = vec![];
    for v in wire::events(2) {
        events.extend(consume(&mut d, &frame(&v), 4096));
    }
    events.extend(consume(&mut d, b"data: [DONE]\n\n", 1));
    d.finish().unwrap();
    let r = d.materialize().unwrap();
    for e in &mut events {
        if let StreamEvent::Delta { fragment, .. } = e
            && fragment == "old "
        {
            *fragment = "new ".into();
        }
    }
    for include_usage in [false, true] {
        let mut encoder = ChatSseEncoder::new(
            r.metadata.clone(),
            Contract::full(),
            SseLimits::default(),
            options(include_usage),
            Obfuscation::Disabled,
        )
        .unwrap();
        let mut bytes = vec![];
        for e in &events {
            for f in encoder.encode(e, &FidelityRecords::default()).unwrap() {
                bytes.extend(f);
            }
        }
        encoder.finish().unwrap();
        let raw = std::str::from_utf8(&bytes).unwrap();
        assert_eq!(raw.matches("data: [DONE]\n\n").count(), 1);
        assert!(!raw.contains("old "));
        assert!(raw.contains("new "));
        let mut round = decoder();
        consume(&mut round, &bytes, 1);
        round.finish().unwrap();
        assert_eq!(
            round.materialize().unwrap().semantic.usage().is_some(),
            include_usage
        );
        assert!(encoder.encode(&StreamEvent::Started, &r.fidelity).is_err());
    }
}

#[test]
fn chat_encoder_padding_limits_and_missing_usage_do_not_emit_done() {
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
    let mut encoder = ChatSseEncoder::new(
        r.metadata,
        Contract::full(),
        SseLimits::default(),
        options(true),
        Obfuscation::Disabled,
    )
    .unwrap();
    for event in &events {
        if matches!(event, StreamEvent::Usage(_)) {
            continue;
        }
        if matches!(event, StreamEvent::Terminal { .. }) {
            assert!(encoder.encode(event, &r.fidelity).is_err());
        } else {
            for frame in encoder.encode(event, &r.fidelity).unwrap() {
                assert_ne!(frame.as_ref(), b"data: [DONE]\n\n");
            }
        }
    }
    assert!(encoder.finish().is_err());
}

#[test]
fn chat_budgets_and_padding_are_independent_of_chunks() {
    let first = frame(&wire::events(2)[0]);
    for size in [1, first.len()] {
        let mut d = ChatSseDecoder::new(
            200,
            "text/event-stream",
            SseLimits {
                max_wire_bytes: first.len() - 1,
                ..Default::default()
            },
        )
        .unwrap();
        let mut rejected = false;
        for chunk in first.chunks(size) {
            if d.consume(chunk).is_err() {
                rejected = true;
                break;
            }
        }
        assert!(rejected);
        assert!(d.finish().is_err());
    }
    let mut padded = wire::events(2)[0].clone();
    padded["obfuscation"] = json!("🧪");
    let mut d = ChatSseDecoder::new(
        200,
        "text/event-stream",
        SseLimits {
            max_obfuscation_bytes: 3,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(d.consume(&frame(&padded)).is_err());
    assert!(d.finish().is_err());
    let mut d = decoder();
    consume(&mut d, &frame(&padded), 1);
    let mut d = ChatSseDecoder::new(
        200,
        "text/event-stream",
        SseLimits {
            max_events: 1,
            ..Default::default()
        },
    )
    .unwrap();
    consume(&mut d, &first, 1);
    assert!(d.consume(&frame(&wire::events(2)[1])).is_err());
}

#[test]
fn response_format_byte_entry_projects_both_ways_and_stays_a_request_fact() {
    let schema = json!({"type":"object","properties":{"zeta":{"type":"string"}},"required":["zeta"],"additionalProperties":false});
    let source = json!({"model":"fixture-model","messages":[{"role":"user","content":"hello"}],"response_format":{"type":"json_schema","json_schema":{"name":"answer","strict":true,"schema":schema}}});
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
    assert_eq!(out["response_format"], source["response_format"]);
    assert_eq!(
        out["response_format"]["json_schema"]["schema"]["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["zeta"]
    );
    // The nested Chat shell projects to the flat Responses shell independently.
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
        projected["text"]["format"],
        json!({"type":"json_schema","name":"answer","strict":true,"schema":schema})
    );
    // And the flat shell projects back to the nested shell with its own wire shape.
    let r = responses::decode_generation(&json!({"input":"hello","text":{"format":{"type":"json_schema","name":"answer","schema":{"type":"object"}}}})).unwrap();
    let back = chat::encode_generation(
        &lower_request(&r.semantic, &r.fidelity, Profile::Chat, Contract::full()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        back["response_format"],
        json!({"type":"json_schema","json_schema":{"name":"answer","schema":{"type":"object"}}})
    );
    // Duplicate keys inside the shell die at raw-byte admission.
    assert!(
        envelope::decode_request_bytes(
            br#"{"model":"m","messages":[{"role":"user","content":"x"}],"response_format":{"type":"json_schema","json_schema":{"name":"a","schema":{},"schema":{}}}}"#
        )
        .is_err()
    );
    // A request constraint is never echoed into the response or its chunks.
    let response =
        envelope::decode_response_bytes(&serde_json::to_vec(&wire::response(2)).unwrap()).unwrap();
    let out = envelope::encode_response(
        &lower_response(
            &response.semantic,
            &response.fidelity,
            &response.metadata,
            Profile::Chat,
            Contract::full(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(!out.to_string().contains("response_format"));
    for value in wire::events(2) {
        assert!(!value.to_string().contains("response_format"));
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
