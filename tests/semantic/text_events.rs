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
use serde_json::json;

#[test]
fn cancelled_is_a_static_outcome_not_a_standard_stream_event() {
    use morphiecore::protocol::openai::{lowering::lower_response, responses};
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
