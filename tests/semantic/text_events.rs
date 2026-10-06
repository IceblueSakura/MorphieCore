//! Text/refusal parts preserve boundaries, empty values and independent item lifecycle.
use crate::events_support::*;
use morphiecore::{
    protocol::{
        fidelity::FidelityRecords,
        openai::{
            Profile,
            events::{EventDecoder, EventEncoder},
        },
    },
    semantic::task::generation::*,
};
use serde_json::{Value, json};

#[test]
fn cancelled_is_a_static_outcome_not_a_standard_stream_event() {
    use morphiecore::{lowering::generation::lower_response, protocol::openai::responses};
    let cancelled = envelope("cancelled", json!([]));
    let decoded = responses::decode_response(&cancelled).unwrap();
    assert_eq!(decoded.semantic.outcome(), Outcome::Cancelled);
    let target = lower_response(
        &decoded.semantic,
        &decoded.fidelity,
        &decoded.metadata,
        Profile::Responses,
        contract(),
    )
    .unwrap();
    assert_eq!(
        responses::encode_response(&target).unwrap()["status"],
        "cancelled"
    );

    let mut decoder = EventDecoder::new(Profile::Responses);
    decoder.push(&created()).unwrap();
    assert!(
        decoder
            .push(&json!({"type":"response.cancelled","response":cancelled}))
            .is_err()
    );
    assert!(decoder.finish().is_err());
    assert!(decoder.materialize().is_err());

    let mut encoder = EventEncoder::new(Profile::Responses, metadata()).unwrap();
    let source = FidelityRecords::default();
    encoder.encode(&StreamEvent::Started, &source).unwrap();
    assert!(
        encoder
            .encode(&terminal(StreamTerminal::Cancelled), &source)
            .is_err()
    );
    assert!(
        encoder
            .encode(&terminal(StreamTerminal::Completed), &source)
            .is_err()
    );
    assert!(encoder.finish().is_err());
}

#[test]
fn independent_text_wire_decodes_empty_and_multiple_parts() {
    let mut d = EventDecoder::new(Profile::Responses);
    d.push(&created()).unwrap();
    d.push(&json!({"type":"response.output_item.added","output_index":0,"item":{"id":"m","type":"message","role":"assistant","status":"in_progress","content":[]}})).unwrap();
    for (i, s) in ["", "你好"].into_iter().enumerate() {
        for v in [
            json!({"type":"response.content_part.added","output_index":0,"item_id":"m","content_index":i,"part":{"type":"output_text","text":"","annotations":[]}}),
            json!({"type":"response.output_text.delta","output_index":0,"item_id":"m","content_index":i,"delta":s,"logprobs":[]}),
            json!({"type":"response.output_text.done","output_index":0,"item_id":"m","content_index":i,"text":s,"logprobs":[]}),
            json!({"type":"response.content_part.done","output_index":0,"item_id":"m","content_index":i,"part":{"type":"output_text","text":s,"annotations":[]}}),
        ] {
            d.push(&v).unwrap();
        }
    }
    let m = json!({"id":"m","type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"","annotations":[]},{"type":"output_text","text":"你好","annotations":[]}]});
    d.push(&json!({"type":"response.output_item.done","output_index":0,"item":m}))
        .unwrap();
    d.push(&json!({"type":"response.completed","response":envelope("completed",json!([m]))}))
        .unwrap();
    let r = d.materialize().unwrap().semantic;
    let Item::Message(m) = &r.items()[0].1 else {
        panic!()
    };
    assert_eq!(m.parts.len(), 2);
    assert_ne!(m.parts[0].id, m.parts[1].id);
    assert_eq!(
        m.parts[0].content,
        ContentPart::Text(
            morphiecore::semantic::value::Text::allowing_empty("", "test", 1)
                .unwrap()
                .into()
        )
    );
}
#[test]
fn independently_constructed_text_ir_encodes_expected_boundaries_and_refusal() {
    let mut e = vec![
        StreamEvent::Started,
        start(9, ItemKind::Message { phase: None }),
    ];
    e.extend(part(9, 40, PartKind::Text, ""));
    e.extend(part(9, 70, PartKind::Refusal, "Cannot comply"));
    e.push(close(9, ItemLifecycle::Completed));
    e.push(terminal(StreamTerminal::Completed));
    let wire = encode(&e, Profile::Responses, &FidelityRecords::default());
    assert_eq!(
        wire.last().unwrap()["response"]["output"][0]["content"],
        json!([{"type":"output_text","text":"","annotations":[]},{"type":"refusal","refusal":"Cannot comply"}])
    );
    assert!(
        wire.iter()
            .any(|v| v["type"] == "response.refusal.delta" && v["delta"] == "Cannot comply")
    );
}
#[test]
fn text_delta_deletion_and_replacement_change_all_encoder_output() {
    let mut e = vec![
        StreamEvent::Started,
        start(1, ItemKind::Message { phase: None }),
    ];
    e.extend(part(1, 9, PartKind::Text, "old"));
    e.push(close(1, ItemLifecycle::Completed));
    e.push(terminal(StreamTerminal::Completed));
    for event in &mut e {
        if let StreamEvent::Delta { fragment, .. } = event {
            *fragment = "new".into();
        }
    }
    for p in [Profile::Responses, Profile::Chat] {
        let wire = encode(&e, p, &FidelityRecords::default());
        assert!(!serde_json::to_string(&wire).unwrap().contains("old"));
        assert!(serde_json::to_string(&wire).unwrap().contains("new"));
    }
}
#[test]
fn part_close_does_not_close_item_and_snapshot_grammar_is_checked() {
    let mut d = EventDecoder::new(Profile::Responses);
    d.push(&created()).unwrap();
    d.push(&json!({"type":"response.output_item.added","output_index":0,"item":{"id":"m","type":"message","role":"assistant","status":"in_progress","content":[]}})).unwrap();
    d.push(&json!({"type":"response.content_part.added","output_index":0,"item_id":"m","content_index":0,"part":{"type":"output_text","text":"","annotations":[]}})).unwrap();
    d.push(&json!({"type":"response.output_text.done","output_index":0,"item_id":"m","content_index":0,"text":"","logprobs":[]})).unwrap();
    d.push(&json!({"type":"response.content_part.done","output_index":0,"item_id":"m","content_index":0,"part":{"type":"output_text","text":"","annotations":[]}})).unwrap();
    assert!(d.push(&json!({"type":"response.completed","response":envelope("completed",json!([{"id":"m","type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"","annotations":[]}]}]))})).is_err());
}
#[test]
fn empty_failed_and_incomplete_static_event_closure_keeps_details() {
    for (terminal, status) in [
        (StreamTerminal::Failed, "failed"),
        (StreamTerminal::Incomplete, "incomplete"),
    ] {
        let details = match terminal {
            StreamTerminal::Failed => TerminalDetails {
                error: Some(ResponseError {
                    code: Some(text("server_error")),
                    message: text("synthetic error"),
                    param: None,
                }),
                incomplete: None,
            },
            StreamTerminal::Incomplete => TerminalDetails {
                error: None,
                incomplete: Some(IncompleteReason::MaxOutputTokens),
            },
            _ => TerminalDetails::default(),
        };
        let e = vec![
            StreamEvent::Started,
            StreamEvent::Terminal {
                terminal,
                details: details.clone(),
            },
        ];
        let wire = encode(&e, Profile::Responses, &FidelityRecords::default());
        assert_eq!(wire.last().unwrap()["response"]["status"], status);
        assert_eq!(wire.last().unwrap()["response"]["output"], json!([]));
        let mut d = EventDecoder::new(Profile::Responses);
        for v in wire {
            d.push(&v).unwrap();
        }
        assert_eq!(d.materialize().unwrap().semantic.details(), &details);
    }
}
#[test]
fn chat_refusal_and_empty_text_survive_usage_and_done() {
    for (kind, value) in [
        (PartKind::Refusal, "no"),
        (PartKind::Refusal, ""),
        (PartKind::Text, ""),
    ] {
        let mut e = vec![
            StreamEvent::Started,
            start(1, ItemKind::Message { phase: None }),
        ];
        e.extend(part(1, 1, kind, value));
        e.push(close(1, ItemLifecycle::Completed));
        e.push(StreamEvent::Usage(Usage {
            scope: UsageScope::Operation,
            basis: UsageBasis::Final,
            output_relation: OutputTokenRelation::IncludesReasoning,
            total_relation: TotalTokenRelation::InputAndOutput,
            input_tokens: Some(3),
            output_tokens: Some(2),
            total_tokens: Some(5),
            reasoning_tokens: Some(1),
            cached_input_tokens: Some(0),
            input_cache_write_tokens: None,
            input_text_tokens: None,
            input_image_tokens: None,
            input_audio_tokens: None,
            output_audio_tokens: None,
            output_text_tokens: None,
            accepted_prediction_tokens: None,
            rejected_prediction_tokens: None,
        }));
        e.push(terminal(StreamTerminal::Completed));
        let wire = encode(&e, Profile::Chat, &FidelityRecords::default());
        assert_eq!(wire.last().unwrap()["choices"], json!([]));
        let mut d = EventDecoder::new(Profile::Chat);
        for v in wire {
            d.push(&v).unwrap();
        }
        d.done().unwrap();
        assert_eq!(
            d.materialize().unwrap().semantic,
            materialize(&apply(&e).unwrap()).unwrap()
        );
    }
}
#[test]
fn queued_lifecycle_is_one_pre_created_snapshot_that_reencodes() {
    let mut d = EventDecoder::new(Profile::Responses);
    let mut events = vec![];
    events.extend(
        d.push(&json!({"type":"response.queued","response":envelope("queued",json!([]))}))
            .unwrap(),
    );
    events.extend(d.push(&created()).unwrap());
    events.extend(
        d.push(&json!({"type":"response.completed","response":envelope("completed",json!([]))}))
            .unwrap(),
    );
    d.materialize().unwrap();
    let wire = encode(&events, Profile::Responses, d.fidelity());
    assert_eq!(wire[0]["type"], json!("response.queued"));
    assert_eq!(wire[0]["response"]["status"], json!("queued"));
    assert_eq!(wire[0]["response"]["output"], json!([]));
    assert_eq!(wire[1]["type"], json!("response.created"));
    assert_eq!(wire.last().unwrap()["type"], json!("response.completed"));
}
#[test]
fn queued_order_duplication_and_snapshot_grammar_fail_closed() {
    let queued = json!({"type":"response.queued","response":envelope("queued",json!([]))});
    let mut d = EventDecoder::new(Profile::Responses);
    d.push(&queued).unwrap();
    assert!(d.push(&queued).is_err());
    let mut d = EventDecoder::new(Profile::Responses);
    d.push(&created()).unwrap();
    assert!(d.push(&queued).is_err());
    let mut d = EventDecoder::new(Profile::Responses);
    d.push(&queued).unwrap();
    assert!(
        d.push(
            &json!({"type":"response.in_progress","response":envelope("in_progress",json!([]))})
        )
        .is_err()
    );
    let mut d = EventDecoder::new(Profile::Responses);
    assert!(
        d.push(&json!({"type":"response.queued","response":envelope("in_progress",json!([]))}))
            .is_err()
    );
    let mut d = EventDecoder::new(Profile::Responses);
    assert!(
        d.push(&json!({"type":"response.queued","response":envelope("queued",json!([{"id":"m","type":"message","role":"assistant","status":"completed","content":[]}]))}))
            .is_err()
    );
}
#[test]
fn initial_response_snapshots_require_the_output_array() {
    for (typ, status) in [
        ("response.queued", "queued"),
        ("response.created", "in_progress"),
        ("response.in_progress", "in_progress"),
    ] {
        for replacement in [None, Some(Value::Null)] {
            let mut d = EventDecoder::new(Profile::Responses);
            if typ == "response.in_progress" {
                d.push(&created()).unwrap();
            }
            let mut snapshot = envelope(status, json!([]));
            if let Some(ref value) = replacement {
                snapshot["output"] = value.clone();
            } else {
                snapshot.as_object_mut().unwrap().remove("output");
            }
            assert!(
                d.push(&json!({"type":typ,"response":snapshot})).is_err(),
                "{typ} {replacement:?}"
            );
        }
    }
}
#[test]
fn queued_is_a_single_pre_created_state_and_never_a_terminal() {
    assert!(
        apply(&[
            StreamEvent::Queued,
            StreamEvent::Started,
            terminal(StreamTerminal::Completed)
        ])
        .is_ok()
    );
    assert!(apply(&[StreamEvent::Started, StreamEvent::Queued]).is_err());
    assert!(apply(&[StreamEvent::Queued, StreamEvent::Queued]).is_err());
    let state = apply(&[StreamEvent::Queued]).unwrap();
    assert!(end_of_stream(&state).is_err());
}
#[test]
fn queued_lifecycle_has_no_chat_projection() {
    let mut chat = EventEncoder::new(Profile::Chat, metadata()).unwrap();
    assert!(
        chat.encode(&StreamEvent::Queued, &FidelityRecords::default())
            .is_err()
    );
    let mut responses = EventEncoder::new(Profile::Responses, metadata()).unwrap();
    let v = responses
        .encode(&StreamEvent::Queued, &FidelityRecords::default())
        .unwrap();
    assert_eq!(v[0]["type"], json!("response.queued"));
    assert_eq!(v[0]["response"]["status"], json!("queued"));
}
