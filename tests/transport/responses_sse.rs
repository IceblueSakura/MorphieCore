//! Offline HTTP/SSE framing acceptance for the stateless Responses text profile.
use crate::wire;

use morphiecore::{
    lowering::generation::GenerationRepresentationContract,
    protocol::{
        fidelity::FidelityRecords,
        openai::{
            Profile,
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
fn timestamp_underflow_is_rejected_in_snapshots_and_poisons_fragmented_streams() {
    for (kind, field) in [
        ("response.created", "created_at"),
        ("response.in_progress", "completed_at"),
        ("response.completed", "completed_at"),
    ] {
        let mut events = wire::events(2);
        let index = if kind == "response.in_progress" {
            events.insert(
                1,
                json!({"type":kind,"sequence_number":1,"response":events[0]["response"].clone()}),
            );
            for (n, event) in events.iter_mut().enumerate() {
                event["sequence_number"] = json!(n);
            }
            1
        } else {
            events.iter().position(|v| v["type"] == kind).unwrap()
        };
        events[index]["response"][field] = serde_json::from_str("-1e-9999").unwrap();
        let bad = encode_frame(&events[index], SseLimits::default().max_event_bytes).unwrap();
        for fragment in [1, 7, bad.len()] {
            let mut d = decoder(SseLimits::default());
            for event in &events[..index] {
                let frame = encode_frame(event, SseLimits::default().max_event_bytes).unwrap();
                consume_all(&mut d, &frame, fragment);
            }
            let mut rejected = false;
            for chunk in bad.chunks(fragment) {
                match d.consume(chunk) {
                    Ok((used, parsed)) => {
                        assert_eq!(used, chunk.len());
                        assert!(
                            !parsed
                                .iter()
                                .any(|e| matches!(e, StreamEvent::Terminal { .. }))
                        );
                    }
                    Err(_) => {
                        rejected = true;
                        break;
                    }
                }
            }
            assert!(rejected, "{kind} {field}");
            assert!(d.consume(&wire_events()).is_err());
            assert!(d.finish().is_err());
            assert!(d.materialize().is_err());
        }
    }
}

#[test]
fn timestamp_zero_fractional_and_underflow_positive_values_survive_fragmentation() {
    for source in [
        "-0.0",
        "-0e-9999",
        "1.25",
        "1e-9999",
        "9007199254740993.125",
    ] {
        let expected: serde_json::Number = source.parse().unwrap();
        let mut events = wire::events(2);
        for event in &mut events {
            if let Some(response) = event.get_mut("response") {
                response["created_at"] = Value::Number(expected.clone());
                if response["completed_at"].is_number() {
                    response["completed_at"] = Value::Number(expected.clone());
                }
            }
        }
        let frames: Vec<u8> = events
            .iter()
            .flat_map(|v| encode_frame(v, SseLimits::default().max_event_bytes).unwrap())
            .collect();
        for fragment in [1, frames.len()] {
            let mut d = decoder(SseLimits::default());
            consume_all(&mut d, &frames, fragment);
            d.finish().unwrap();
            let metadata = d.materialize().unwrap().metadata;
            assert_eq!(metadata.created.as_str(), expected.as_str());
            assert_eq!(
                metadata.context.completed_at.value().unwrap().as_str(),
                expected.as_str()
            );
        }
    }
}

#[test]
fn strict_json_rejection_and_poisoning_survive_fragmentation() {
    use morphiecore::protocol::openai::{CodecError, sse::SseError};
    let values = wire::events(2);
    let first = encode_frame(&values[0], SseLimits::default().max_event_bytes).unwrap();
    let item = values[1].to_string();
    let duplicate = item.replace(
        "\"id\":\"answer\"",
        "\"id\":\"discarded\",\"id\":\"answer\"",
    );
    assert_ne!(duplicate, item);
    for (payload, expected) in [
        (duplicate, CodecError::Invalid("JSON")),
        (
            format!("{{\"type\":\"response.output_item.added\",{}", &item[1..]),
            CodecError::Invalid("JSON"),
        ),
        (format!("{item} null"), CodecError::Invalid("JSON")),
        (
            format!("{}0{}", "[".repeat(65), "]".repeat(65)),
            CodecError::Limit,
        ),
    ] {
        let frame = format!("event: response.output_item.added\ndata: {payload}\n\n");
        for fragment in [1, frame.len()] {
            let mut d = decoder(SseLimits::default());
            consume_all(&mut d, &first, fragment);
            let mut rejected = false;
            for chunk in frame.as_bytes().chunks(fragment) {
                match d.consume(chunk) {
                    Ok((used, _)) => assert_eq!(used, chunk.len()),
                    Err(error) => {
                        assert!(matches!(error, SseError::Codec(actual) if actual == expected));
                        rejected = true;
                        break;
                    }
                }
            }
            assert!(rejected);
            assert!(d.consume(&wire_events()).is_err());
            assert!(d.finish().is_err());
            assert!(d.materialize().is_err());
        }
    }
}

#[test]
fn sse_reasoning_snapshots_cannot_drop_required_identity() {
    for kind in [
        "response.output_item.added",
        "response.output_item.done",
        "response.completed",
    ] {
        let mut values = wire::events(1);
        let index = values
            .iter()
            .position(|v| {
                v["type"] == kind
                    && if kind == "response.completed" {
                        v["response"]["output"][0]["type"] == "reasoning"
                    } else {
                        v["item"]["type"] == "reasoning"
                    }
            })
            .unwrap();
        let item = if kind == "response.completed" {
            &mut values[index]["response"]["output"][0]
        } else {
            &mut values[index]["item"]
        }
        .as_object_mut()
        .unwrap();
        item.remove("id");
        let mut d = decoder(SseLimits::default());
        for value in &values[..index] {
            let frame = encode_frame(value, SseLimits::default().max_event_bytes).unwrap();
            consume_all(&mut d, &frame, 1);
        }
        let frame = encode_frame(&values[index], SseLimits::default().max_event_bytes).unwrap();
        assert!(d.consume(&frame).is_err(), "{kind}");
        assert!(d.finish().is_err());
    }
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
fn complete_sse_events_cannot_omit_required_sequence_number() {
    for single in [false, true] {
        let mut values = wire::events(2);
        for (i, value) in values.iter_mut().enumerate() {
            if !single || i == 3 {
                value.as_object_mut().unwrap().remove("sequence_number");
            }
        }
        let mut d = decoder(SseLimits::default());
        let mut rejected = None;
        for (i, value) in values.iter().enumerate() {
            let frame = encode_frame(value, SseLimits::default().max_event_bytes).unwrap();
            match d.consume(&frame) {
                Ok((used, _)) => {
                    assert_eq!(used, frame.len());
                    assert!(rejected.is_none(), "event {i} accepted after rejection");
                }
                Err(_) if rejected.is_none() => rejected = Some(i),
                Err(_) => {}
            }
        }
        assert_eq!(rejected, Some(if single { 3 } else { 0 }), "{single}");
        assert!(d.consume(&wire_events()).is_err());
        assert!(d.finish().is_err());
        assert!(d.materialize().is_err());
    }
}

#[test]
fn lifecycle_response_snapshots_cannot_omit_the_output_array() {
    for kind in [
        "response.queued",
        "response.created",
        "response.in_progress",
    ] {
        let mut values = wire::events(2);
        let mut snapshot = values[0]["response"].clone();
        let index = match kind {
            "response.queued" => {
                snapshot["status"] = json!("queued");
                values.insert(0, json!({"type": kind, "response": snapshot}));
                0
            }
            "response.created" => 0,
            _ => {
                values.insert(1, json!({"type": kind, "response": snapshot}));
                1
            }
        };
        for (i, value) in values.iter_mut().enumerate() {
            value["sequence_number"] = json!(i);
        }
        let bytes: Vec<u8> = values
            .iter()
            .flat_map(|v| encode_frame(v, SseLimits::default().max_event_bytes).unwrap())
            .collect();
        // An explicit empty output array keeps the lifecycle snapshot valid.
        let mut accepted = decoder(SseLimits::default());
        consume_all(&mut accepted, &bytes, 5);
        accepted.finish().unwrap();
        accepted.materialize().unwrap();

        values[index]["response"]
            .as_object_mut()
            .unwrap()
            .remove("output");
        let mut d = decoder(SseLimits::default());
        let mut rejected = None;
        for (i, value) in values.iter().enumerate() {
            let frame = encode_frame(value, SseLimits::default().max_event_bytes).unwrap();
            match d.consume(&frame) {
                Ok((used, _)) => {
                    assert_eq!(used, frame.len());
                    assert!(
                        rejected.is_none(),
                        "{kind} event {i} accepted after rejection"
                    );
                }
                Err(_) if rejected.is_none() => rejected = Some(i),
                Err(_) => {}
            }
        }
        assert_eq!(rejected, Some(index), "{kind}");
        assert!(d.consume(&wire_events()).is_err());
        assert!(d.finish().is_err());
        assert!(d.materialize().is_err());
    }
}

#[test]
fn fragmented_utf8_and_bare_cr_preserve_the_same_terminal_and_output() {
    let lf = wire_events();
    let mut fragmented = decoder(SseLimits::default());
    let count = consume_all(&mut fragmented, &lf, 1);
    fragmented.finish().unwrap();
    let expected = fragmented.materialize().unwrap();
    assert!(count > 5);

    let mut transformed = b"\xef\xbb\xbf: synthetic comment\r\r".to_vec();
    transformed.extend_from_slice(&lf);
    let transformed = transformed
        .into_iter()
        .flat_map(|b| {
            if b == b'\n' {
                b"\r\n".to_vec()
            } else {
                vec![b]
            }
        })
        .collect::<Vec<_>>();
    let mut crlf = decoder(SseLimits::default());
    assert_eq!(consume_all(&mut crlf, &transformed, 3), count);
    crlf.finish().unwrap();
    assert_eq!(crlf.materialize().unwrap().semantic, expected.semantic);
    let mut bare_cr = decoder(SseLimits::default());
    let bare = lf
        .iter()
        .map(|b| if *b == b'\n' { b'\r' } else { *b })
        .collect::<Vec<_>>();
    assert_eq!(consume_all(&mut bare_cr, &bare, 1), count);
    bare_cr.finish().unwrap();
    assert_eq!(bare_cr.materialize().unwrap().semantic, expected.semantic);
}

#[test]
fn malformed_http_event_identity_eof_and_post_terminal_fail_closed() {
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
            GenerationRepresentationContract::full(),
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
        GenerationRepresentationContract::full(),
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
fn multiline_data_is_joined_and_an_invalid_json_record_poisons_the_stream() {
    let first: Value = wire::events(2)[0].clone();
    let raw = json!({"type": "response.created", "response": first["response"], "sequence_number": first["sequence_number"]})
        .to_string();
    let split = raw.find("\"response\":").unwrap() + "\"response\":".len();
    let valid = format!(
        "event: response.created\ndata: {}\ndata: {}\n\n",
        &raw[..split],
        &raw[split..]
    );
    let mut accepted = decoder(SseLimits::default());
    assert_eq!(consume_all(&mut accepted, valid.as_bytes(), 1), 1);
    assert!(accepted.finish().is_err()); // A valid prefix is not a complete response.

    // Split inside a known string token, independent of object serialization order.
    let split = raw.find("response.created").unwrap() + "response.".len();
    let input = format!(
        "event: response.created\ndata: {}\ndata: {}\n\n",
        &raw[..split],
        &raw[split..]
    );
    let mut d = decoder(SseLimits::default());
    // Newline in a JSON token must fail; framing must not silently merge data lines.
    assert!(d.consume(input.as_bytes()).is_err());
    assert!(d.consume(wire_events().as_slice()).is_err());
}

#[test]
fn empty_terminal_summary_requires_real_closed_items_and_exact_nonempty_snapshots() {
    use morphiecore::{
        adapter::{Adapter, Dialect},
        execution::Attempt,
        protocol::openai::{Profile, sse::SseLimits},
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
