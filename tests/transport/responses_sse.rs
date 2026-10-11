//! Offline HTTP/SSE framing acceptance for the stateless Responses text profile.
use crate::wire;

use morphiecore::{
    protocol::{
        fidelity::FidelityRecords,
        openai::{
            Profile, TargetCaps,
            events::EventDecoder,
            sse::{Obfuscation, ResponsesSseDecoder, ResponsesSseEncoder, SseLimits, encode_frame},
        },
    },
    semantic::{task::generation::StreamEvent, value::ReplayOrigin},
};
use serde_json::{Value, json};

fn origin() -> ReplayOrigin {
    ReplayOrigin::new("synthetic-loopback").unwrap()
}

fn decoder(limits: SseLimits) -> ResponsesSseDecoder {
    ResponsesSseDecoder::new(
        200,
        "text/event-stream; charset=utf-8",
        limits,
        Some(origin()),
    )
    .unwrap()
}

fn wire_events() -> Vec<u8> {
    wire::events(2)
        .iter()
        .flat_map(|v| encode_frame(v, SseLimits::default().max_event_bytes).unwrap())
        .collect()
}

fn consume_all(d: &mut ResponsesSseDecoder, bytes: &[u8], chunk_len: usize) -> usize {
    let mut events = 0;
    for chunk in bytes.chunks(chunk_len) {
        let mut remaining = chunk;
        while !remaining.is_empty() {
            let (used, parsed) = d.consume(remaining).unwrap();
            assert!(used > 0 && used <= remaining.len());
            events += parsed.len();
            remaining = &remaining[used..];
        }
    }
    events
}

#[test]
fn opaque_replay_cannot_be_promoted_by_missing_done_conflicting_terminal_or_eof() {
    for failure in ["missing_item_done", "conflicting_token", "missing_terminal"] {
        let mut values = wire::events(1);
        match failure {
            "missing_item_done" => values.retain(|v| {
                !(v["type"] == "response.output_item.done" && v["item"]["type"] == "reasoning")
            }),
            "conflicting_token" => {
                values.last_mut().unwrap()["response"]["output"][0]["encrypted_content"] =
                    json!("different-synthetic-token")
            }
            _ => {
                values.pop();
            }
        }
        let bytes: Vec<_> = values
            .iter_mut()
            .enumerate()
            .flat_map(|(n, v)| {
                v["sequence_number"] = json!(n);
                encode_frame(v, SseLimits::default().max_event_bytes).unwrap()
            })
            .collect();
        for size in [1, 17, bytes.len()] {
            let mut d = decoder(SseLimits::default());
            let mut rejected = false;
            'input: for chunk in bytes.chunks(size) {
                let mut rest = chunk;
                while !rest.is_empty() {
                    match d.consume(rest) {
                        Ok((used, events)) => {
                            assert!(used > 0);
                            assert!(
                                !events
                                    .iter()
                                    .any(|event| matches!(event, StreamEvent::Terminal { .. }))
                            );
                            rest = &rest[used..];
                        }
                        Err(error) => {
                            if failure == "conflicting_token" {
                                assert!(matches!(
                                    error,
                                    morphiecore::protocol::openai::sse::SseError::Codec(
                                        morphiecore::protocol::CodecError::Invalid(
                                            "terminal snapshot replay changed"
                                        )
                                    )
                                ));
                            }
                            rejected = true;
                            break 'input;
                        }
                    }
                }
            }
            assert!(d.finish().is_err(), "{failure}");
            assert!(d.materialize().is_err(), "{failure}");
            assert!(
                d.consume(&wire_events()).is_err(),
                "rejected streams cannot resume"
            );
            assert_eq!(rejected, failure != "missing_terminal");
        }
    }
}

#[test]
fn tool_value_and_item_closure_cannot_be_repaired_by_a_terminal_snapshot() {
    for (kind, item_kind) in [
        ("response.function_call_arguments.done", ""),
        ("response.custom_tool_call_input.done", ""),
        ("response.output_item.done", "function_call"),
        ("response.output_item.done", "custom_tool_call"),
    ] {
        let mut events = wire::events(1);
        let index = events
            .iter()
            .position(|e| {
                e["type"] == kind && (item_kind.is_empty() || e["item"]["type"] == item_kind)
            })
            .unwrap();
        events.remove(index);
        let bytes: Vec<_> = events
            .iter_mut()
            .enumerate()
            .flat_map(|(n, event)| {
                event["sequence_number"] = json!(n);
                encode_frame(event, SseLimits::default().max_event_bytes).unwrap()
            })
            .collect();
        for size in [1, 17, bytes.len()] {
            let mut d = decoder(SseLimits::default());
            let mut rejected = false;
            'input: for chunk in bytes.chunks(size) {
                let mut rest = chunk;
                while !rest.is_empty() {
                    match d.consume(rest) {
                        Ok((used, _)) => {
                            assert!(used > 0);
                            rest = &rest[used..];
                        }
                        Err(_) => {
                            rejected = true;
                            break 'input;
                        }
                    }
                }
            }
            assert!(rejected, "{kind}/{item_kind}");
            assert!(d.finish().is_err());
            assert!(d.materialize().is_err());
            assert!(d.consume(&wire_events()).is_err());
        }
    }
}

#[test]
fn complete_sse_message_snapshots_cannot_default_missing_identity_or_status() {
    for kind in [
        "response.output_item.added",
        "response.output_item.done",
        "response.completed",
    ] {
        for key in ["id", "status"] {
            for replacement in [None, Some(Value::Null), Some(json!(false))] {
                let mut values = wire::events(2);
                let index = values.iter().position(|v| v["type"] == kind).unwrap();
                let item = if kind == "response.completed" {
                    &mut values[index]["response"]["output"][0]
                } else {
                    &mut values[index]["item"]
                }
                .as_object_mut()
                .unwrap();
                if let Some(value) = replacement {
                    item.insert(key.into(), value);
                } else {
                    item.remove(key);
                }
                let mut d = decoder(SseLimits::default());
                for value in &values[..index] {
                    let frame = encode_frame(value, SseLimits::default().max_event_bytes).unwrap();
                    consume_all(&mut d, &frame, 1);
                }
                let frame =
                    encode_frame(&values[index], SseLimits::default().max_event_bytes).unwrap();
                assert!(d.consume(&frame).is_err(), "{kind} {key}");
                let terminal = encode_frame(
                    wire::events(2).last().unwrap(),
                    SseLimits::default().max_event_bytes,
                )
                .unwrap();
                assert!(d.consume(&terminal).is_err());
                assert!(d.finish().is_err());
                assert!(d.materialize().is_err());
            }
        }
    }
}

#[test]
fn malformed_http_event_identity_eof_and_post_terminal_fail_closed() {
    let initial = encode_frame(&wire::events(2)[0], SseLimits::default().max_event_bytes).unwrap();
    let future =
        b"event: response.future\ndata: {\"type\":\"response.future\",\"sequence_number\":1}\n\n";
    let mut unknown = decoder(SseLimits::default());
    consume_all(&mut unknown, &initial, 1);
    assert!(matches!(
        unknown.consume(future),
        Err(morphiecore::protocol::openai::sse::SseError::Codec(
            morphiecore::protocol::openai::CodecError::Unsupported(_)
        ))
    ));
    assert!(unknown.consume(&wire_events()).is_err());
    assert!(unknown.finish().is_err());
    assert!(unknown.materialize().is_err());
    for (status, content_type) in [
        (201, "text/event-stream"),
        (200, "application/json"),
        (200, "text/event-stream; charset=iso-8859-1"),
        (200, "text/event-stream; charset=utf-8; charset=iso-8859-1"),
    ] {
        assert!(
            ResponsesSseDecoder::new(status, content_type, SseLimits::default(), None).is_err()
        );
    }
    let mut mismatch = decoder(SseLimits::default());
    assert!(
        mismatch
            .consume(b"event: response.completed\ndata: {\"type\":\"response.created\"}\n\n")
            .is_err()
    );
    assert!(mismatch.consume(b"").is_err());
    assert!(mismatch.finish().is_err());

    let bytes = wire_events();
    let last_delimiter = bytes.len() - 1;
    let mut missing_delimiter = decoder(SseLimits::default());
    consume_all(&mut missing_delimiter, &bytes[..last_delimiter], 23);
    assert!(missing_delimiter.finish().is_err());
    assert!(missing_delimiter.materialize().is_err());
    let mut no_terminal = decoder(SseLimits::default());
    let terminal = encode_frame(
        wire::events(2).last().unwrap(),
        SseLimits::default().max_event_bytes,
    )
    .unwrap();
    consume_all(
        &mut no_terminal,
        &bytes[..bytes.len() - terminal.len()],
        128,
    );
    assert!(no_terminal.finish().is_err());
    assert!(no_terminal.materialize().is_err());
    let mut duplicate = decoder(SseLimits::default());
    consume_all(&mut duplicate, &bytes, 4096);
    assert!(duplicate.consume(&terminal).is_err());
    assert!(duplicate.finish().is_err());
}

#[test]
fn byte_event_and_wire_limits_cannot_be_evaded_by_fragmentation() {
    let bytes = wire_events();
    let first = encode_frame(&wire::events(2)[0], SseLimits::default().max_event_bytes).unwrap();
    for fragment in [1, bytes.len()] {
        let mut limited = decoder(SseLimits {
            max_event_bytes: first.len() - 1,
            ..SseLimits::default()
        });
        let mut failed = false;
        for chunk in bytes.chunks(fragment) {
            let mut remaining = chunk;
            while !remaining.is_empty() {
                match limited.consume(remaining) {
                    Ok((n, _)) => remaining = &remaining[n..],
                    Err(_) => {
                        failed = true;
                        break;
                    }
                }
            }
            if failed {
                break;
            }
        }
        assert!(failed);
        assert!(limited.finish().is_err());
    }
    let mut wire_limited = decoder(SseLimits {
        max_wire_bytes: bytes.len() - 1,
        ..SseLimits::default()
    });
    let mut cursor = 0;
    let mut failed = false;
    while cursor < bytes.len() {
        match wire_limited.consume(&bytes[cursor..]) {
            Ok((n, _)) => cursor += n,
            Err(_) => {
                failed = true;
                break;
            }
        }
    }
    assert!(failed);
    assert!(wire_limited.finish().is_err());
    // The caller must revisit the unconsumed prefix; a single consume never swallows all events.
    let mut cursor = 0;
    let mut event_limited = decoder(SseLimits {
        max_events: 1,
        ..SseLimits::default()
    });
    while cursor < bytes.len() {
        match event_limited.consume(&bytes[cursor..]) {
            Ok((n, _)) => cursor += n,
            Err(_) => break,
        }
    }
    assert!(cursor < bytes.len());
    assert!(event_limited.finish().is_err());
}

#[test]
fn padding_has_an_independent_cumulative_utf8_budget() {
    let mut values = wire::events(1);
    let mut deltas = 0;
    for value in &mut values {
        value.as_object_mut().unwrap().remove("obfuscation");
        if value["type"].as_str().unwrap().ends_with(".delta") && deltas < 2 {
            value["obfuscation"] = json!("🧪");
            deltas += 1;
        }
    }
    assert_eq!(deltas, 2);
    let bytes: Vec<_> = values
        .iter()
        .flat_map(|v| encode_frame(v, SseLimits::default().max_event_bytes).unwrap())
        .collect();
    for fragment in [1, bytes.len()] {
        let mut limited = decoder(SseLimits {
            max_obfuscation_bytes: 4,
            ..SseLimits::default()
        });
        let mut rejected = false;
        for chunk in bytes.chunks(fragment) {
            let mut rest = chunk;
            while !rest.is_empty() {
                match limited.consume(rest) {
                    Ok((n, _)) => rest = &rest[n..],
                    Err(error) => {
                        assert!(matches!(
                            error,
                            morphiecore::protocol::openai::sse::SseError::Limit
                        ));
                        rejected = true;
                        break;
                    }
                }
            }
            if rejected {
                break;
            }
        }
        assert!(rejected);
        assert!(limited.consume(b"").is_err());
        assert!(limited.finish().is_err());
        assert!(limited.materialize().is_err());
        let mut exact = decoder(SseLimits {
            max_obfuscation_bytes: 8,
            ..SseLimits::default()
        });
        consume_all(&mut exact, &bytes, fragment);
        exact.finish().unwrap();
        assert_eq!(exact.materialize().unwrap().semantic.items().len(), 4);
    }
}

#[test]
fn padding_encoder_respects_zero_budget_and_disabled_switch() {
    let mut decoded = EventDecoder::new(Profile::Responses).with_replay_origin(origin());
    let mut events = vec![];
    for payload in wire::events(2) {
        events.extend(decoded.push(&payload).unwrap());
    }
    decoded.finish().unwrap();
    for enabled in [false, true] {
        let padding = if enabled {
            Obfuscation::Seeded([1; 32])
        } else {
            Obfuscation::Disabled
        };
        let mut encoder = ResponsesSseEncoder::new(
            decoded.metadata().unwrap().clone(),
            TargetCaps::full(),
            SseLimits {
                max_obfuscation_bytes: 0,
                ..SseLimits::default()
            },
            padding,
        )
        .unwrap();
        let mut rejected = false;
        for event in &events {
            match encoder.encode(event, decoded.fidelity()) {
                Ok(frames) => {
                    if !enabled {
                        for frame in frames {
                            assert!(
                                !std::str::from_utf8(&frame)
                                    .unwrap()
                                    .contains("\"obfuscation\"")
                            );
                        }
                    }
                }
                Err(error) => {
                    assert!(matches!(
                        error,
                        morphiecore::protocol::openai::sse::SseError::Limit
                    ));
                    rejected = true;
                    break;
                }
            }
        }
        assert_eq!(rejected, enabled);
        assert_eq!(encoder.finish().is_err(), enabled);
        if enabled {
            assert!(
                encoder
                    .encode(&StreamEvent::Started, decoded.fidelity())
                    .is_err()
            );
        }
    }
}

#[test]
fn rejected_metadata_update_poisoned_stream_cannot_emit_success() {
    let mut decoder = EventDecoder::new(Profile::Responses).with_replay_origin(origin());
    decoder.push(&wire::events(2)[0]).unwrap();
    let metadata = decoder.metadata().unwrap().clone();
    let mut encoder = ResponsesSseEncoder::new(
        metadata.clone(),
        TargetCaps::full(),
        SseLimits::default(),
        Obfuscation::Disabled,
    )
    .unwrap();
    let mut changed = metadata;
    changed.id = "different-response".into();
    assert!(encoder.update_metadata(changed).is_err());
    assert!(
        encoder
            .encode(&StreamEvent::Started, &FidelityRecords::default())
            .is_err()
    );
    assert!(encoder.finish().is_err());
}

#[test]
fn empty_terminal_summary_requires_real_closed_items_and_exact_nonempty_snapshots() {
    use morphiecore::{
        execution::Attempt,
        protocol::openai::{Adapter, Dialect, Profile, sse::SseLimits},
        semantic::value::ReplayOrigin,
    };
    let adapter = |dialect, scope| {
        Adapter::new(
            Profile::Responses,
            dialect,
            Some(ReplayOrigin::new(scope).unwrap()),
        )
    };
    let events = crate::wire::events(2);
    for (dialect, finished, output, accepted) in [
        (Dialect::Codex, true, json!([]), true),
        (Dialect::Standard, true, json!([]), false),
        (Dialect::Codex, false, json!([]), false),
        (Dialect::Codex, true, Value::Null, false),
        (
            Dialect::Codex,
            true,
            json!([{"type":"message","id":"other","role":"assistant","status":"completed","content":[]}]),
            false,
        ),
    ] {
        let mut selected = events.clone();
        if !finished {
            selected.retain(|e| e["type"] != "response.output_item.done");
        }
        selected.last_mut().unwrap()["response"]["output"] = output;
        let mut attempt = Attempt::new(adapter(dialect, "one"), 1 << 20, SseLimits::default());
        attempt.begin(200, "text/event-stream").unwrap();
        let mut valid = true;
        for event in selected {
            let bytes = format!(
                "event: {}\ndata: {event}\n\n",
                event["type"].as_str().unwrap()
            );
            if attempt.push(bytes.as_bytes()).is_err() {
                valid = false;
                break;
            }
        }
        valid = valid && attempt.finish().is_ok();
        assert_eq!(valid, accepted, "{dialect:?} finished={finished}");
    }
}
