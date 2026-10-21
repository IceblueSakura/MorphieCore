//! Offline HTTP/SSE framing acceptance for the stateless Responses text profile.
use crate::wire;

use morphiecore::{
    protocol::{
        fidelity::FidelityRecords,
        openai::{
            Profile, TargetCaps,
            events::EventDecoder,
            obfuscation::Obfuscation,
            responses::sse::{ResponsesSseDecoder, ResponsesSseEncoder, encode_frame},
        },
        stream::SseLimits,
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
fn unresolved_file_citation_positions_reject_events_and_final_part_snapshots() {
    use morphiecore::protocol::{CodecError, stream::SseError};
    let annotation =
        json!({"type":"file_citation","file_id":"synthetic-file","filename":"report","index":2});
    for event_type in [
        "response.output_text.annotation.added",
        "response.content_part.done",
    ] {
        let mut values = wire::events(2);
        let value = values.iter_mut().find(|v| v["type"] == event_type).unwrap();
        if event_type == "response.output_text.annotation.added" {
            value["annotation"] = annotation.clone();
        } else {
            value["part"]["annotations"] = json!([annotation.clone()]);
        }
        let mut decoder = decoder(SseLimits::default());
        let mut rejected = false;
        for value in &values {
            let bytes = encode_frame(value, SseLimits::default().max_event_bytes).unwrap();
            let mut rest = bytes.as_ref();
            while !rest.is_empty() {
                match decoder.consume(rest) {
                    Ok((n, events)) => {
                        assert!(
                            !events
                                .iter()
                                .any(|v| matches!(v, StreamEvent::Terminal { .. }))
                        );
                        rest = &rest[n..];
                    }
                    Err(error) => {
                        assert!(
                            matches!(error, SseError::Codec(CodecError::Unsupported(ref label))
                            if label == "file_citation insertion units")
                        );
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
        assert!(decoder.finish().is_err());
        assert!(decoder.materialize().is_err());
    }
}

#[test]
fn audit_repeated_citations_ignore_extensions_but_reject_changed_evidence() {
    for changed in [false, true] {
        let mut values = wire::events(2);
        for v in &mut values {
            if v["type"] == "response.output_text.annotation.added" {
                v["annotation"]["future_field"] = json!(true);
            }
            if v["type"] == "response.content_part.done" {
                v["part"]["annotations"][0]["future_field"] = json!(true);
                if changed {
                    v["part"]["annotations"][0]["url"] = json!("https://example.test/changed");
                }
            }
        }
        let bytes: Vec<_> = values
            .iter()
            .flat_map(|v| encode_frame(v, SseLimits::default().max_event_bytes).unwrap())
            .collect();
        let mut d = decoder(SseLimits::default());
        let mut rejected = false;
        'chunks: for chunk in bytes.chunks(7) {
            let mut rest = chunk;
            while !rest.is_empty() {
                match d.consume(rest) {
                    Ok((n, events)) => {
                        assert!(
                            !changed
                                || !events
                                    .iter()
                                    .any(|e| matches!(e, StreamEvent::Terminal { .. }))
                        );
                        rest = &rest[n..];
                    }
                    Err(e) => {
                        assert!(changed, "{e}");
                        assert!(e.to_string().contains("annotation snapshot"));
                        rejected = true;
                        break 'chunks;
                    }
                }
            }
        }
        assert_eq!(rejected, changed);
        if changed {
            assert!(d.finish().is_err());
            assert!(d.materialize().is_err());
        } else {
            d.finish().unwrap();
            use morphiecore::semantic::task::generation::*;
            let decoded = d.materialize().unwrap();
            let Item::Message(message) = &decoded.semantic.items()[0].1 else {
                panic!("message")
            };
            let ContentPart::Text(text) = &message.parts[0].content else {
                panic!("text")
            };
            assert_eq!(text.as_str(), "{\"ok\":false}");
            let citation = text.annotations()[0].value();
            assert_eq!(
                citation.claim,
                ClaimAnchor::Range(TextRange {
                    unit: TextUnit::UnicodeScalars,
                    start: 0,
                    end: 1
                })
            );
            assert!(matches!(decoded.semantic.resources().get(citation.source),
                Some(ResourceLocation::Url(url)) if url.as_str() == "https://example.test/source"));
        }
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
                                    morphiecore::protocol::stream::SseError::Codec(
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
fn malformed_http_event_identity_eof_and_post_terminal_fail_closed() {
    // Unregistered event types are skipped and counted, as Codex does.
    let mut events = wire::events(2);
    events.insert(
        1,
        json!({"type":"response.future","item":{"type":"future"}}),
    );
    let mut unknown = decoder(SseLimits::default());
    for (n, event) in events.iter_mut().enumerate() {
        event["sequence_number"] = json!(n);
        let frame = encode_frame(event, SseLimits::default().max_event_bytes).unwrap();
        consume_all(&mut unknown, &frame, 7);
    }
    assert_eq!(unknown.ignored_events(), 1);
    unknown.finish().unwrap();
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
fn rejected_metadata_update_poisoned_stream_cannot_emit_success() {
    let mut decoder = EventDecoder::new(Profile::Responses).with_replay_origin(origin());
    assert!(decoder.finish().is_err());
    assert!(
        decoder.done().is_err(),
        "wrong-profile DONE does not poison"
    );
    decoder.push(&wire::events(2)[0]).unwrap();
    let metadata = decoder.metadata().unwrap().clone();
    let mut encoder = ResponsesSseEncoder::new(
        metadata.clone(),
        TargetCaps::full(),
        SseLimits::default(),
        Obfuscation::Disabled,
    )
    .unwrap();
    let mut changed = metadata.clone();
    changed.id = Some("different-response".into());
    assert!(encoder.update_metadata(changed).is_err());
    assert!(
        encoder
            .encode(&StreamEvent::Started, &FidelityRecords::default())
            .is_err()
    );
    assert!(encoder.finish().is_err());

    let mut update = wire::events(2)[0].clone();
    update["type"] = json!("response.in_progress");
    update["sequence_number"] = json!(1);
    decoder.push(&update).unwrap();
    update["sequence_number"] = json!(2);
    update["response"]["service_tier"] = json!("priority");
    assert!(decoder.push(&update).is_err());
    assert!(decoder.finish().is_err());
    assert!(decoder.push(&wire::events(2)[1]).is_err());

    let mut encoder = ResponsesSseEncoder::new(
        metadata.clone(),
        TargetCaps::full(),
        SseLimits::default(),
        Obfuscation::Disabled,
    )
    .unwrap();
    encoder
        .encode(&StreamEvent::Started, &FidelityRecords::default())
        .unwrap();
    let mut changed = metadata;
    changed
        .context
        .reports
        .latest_mut()
        .expect("report")
        .service_class = morphiecore::semantic::value::Presence::Value(
        morphiecore::semantic::context::ServiceClass::PriorityProcessing,
    );
    assert!(encoder.update_metadata(changed).is_err());
    assert!(encoder.finish().is_err());
}

#[test]
fn empty_terminal_summary_requires_real_closed_items_and_exact_nonempty_snapshots() {
    use morphiecore::{
        execution::Attempt,
        protocol::{
            openai::{Adapter, Dialect, Profile},
            stream::SseLimits,
        },
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
        (Dialect::Siwc, true, json!([]), true),
        (Dialect::Standard, true, json!([]), false),
        (Dialect::Grok, true, json!([]), false),
        (Dialect::Siwc, false, json!([]), false),
        (Dialect::Siwc, true, Value::Null, false),
        (
            Dialect::Siwc,
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
        if accepted {
            let response = attempt.response().unwrap();
            assert!(matches!(&response.semantic.items()[0].1,
                morphiecore::semantic::task::generation::Item::Message(message)
                if matches!(&message.parts[0].content,
                    morphiecore::semantic::task::generation::ContentPart::Text(text)
                        if text.as_str()=="{\"ok\":false}")));
        }
    }
}
#[test]
fn subscription_dialects_keep_raw_call_fragments_final_replay_and_strict_eof() {
    use morphiecore::{
        protocol::openai::{Adapter, Dialect},
        semantic::task::generation::*,
    };
    for (dialect, grouped) in [(Dialect::Grok, false), (Dialect::Siwc, true)] {
        let adapter = Adapter::new(Profile::Responses, dialect, Some(origin()));
        let events = wire::response_events(wire::subscription_response(grouped));
        for failure in ["none", "cipher", "item_done", "terminal", "done_trailer"] {
            let mut values = events.clone();
            match failure {
                "cipher" => {
                    values.last_mut().unwrap()["response"]["output"][0]["encrypted_content"] =
                        json!("changed")
                }
                "item_done" => values.retain(|v| {
                    !(v["type"] == "response.output_item.done" && v["item"]["type"] == "reasoning")
                }),
                "terminal" => {
                    values.pop();
                }
                _ => {}
            }
            let mut bytes = values
                .iter()
                .enumerate()
                .flat_map(|(i, v)| {
                    let mut v = v.clone();
                    v["sequence_number"] = json!(i);
                    encode_frame(&v, SseLimits::default().max_event_bytes).unwrap()
                })
                .collect::<Vec<_>>();
            if failure == "done_trailer" {
                bytes.extend_from_slice(b"data: [DONE]\n\n");
            }
            let mut d = ResponsesSseDecoder::with_decoder(
                200,
                "text/event-stream",
                SseLimits::default(),
                adapter.event_decoder(),
            )
            .unwrap();
            let mut valid = true;
            'chunks: for chunk in bytes.chunks(if failure == "none" { 1 } else { 17 }) {
                let mut rest = chunk;
                while !rest.is_empty() {
                    match d.consume(rest) {
                        Ok((used, _)) => {
                            assert!(used > 0);
                            rest = &rest[used..];
                        }
                        Err(_) => {
                            valid = false;
                            break 'chunks;
                        }
                    }
                }
            }
            assert!(d.materialize().is_err());
            valid = valid && d.finish().is_ok();
            assert_eq!(valid, failure == "none", "{dialect:?} {failure}");
            if valid {
                let response = d.materialize().unwrap();
                assert_eq!(response.semantic.outcome(), Outcome::Completed);
                assert!(matches!(&response.semantic.items()[0].1,Item::Reasoning(r)
                    if r.replay.as_ref().is_some_and(|v|v.replay_token()==Some("synthetic-final-token"))));
                assert!(matches!(&response.semantic.items()[1].1,Item::ToolCall(c)
                    if c.arguments.as_raw()==Some("{ \"n\":9007199254740993 }")));
                assert!(matches!(&response.semantic.items()[2].1,Item::Message(m)
                    if m.phase==Some(Phase::Commentary)&&matches!(&m.parts[0].content,
                        ContentPart::Text(text) if text.as_str()=="checking")));
            } else {
                assert!(d.materialize().is_err());
            }
        }
    }
}
