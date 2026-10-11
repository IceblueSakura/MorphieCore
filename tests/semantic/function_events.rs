//! Function argument, mixed-item identity, snapshot and terminal conformance.
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
fn independent_wire_decodes_arguments_and_close_without_inventing_a_terminal() {
    let mut d = EventDecoder::new(Profile::Responses);
    d.push(&created()).unwrap();
    d.push(&json!({"type":"response.output_item.added","output_index":0,"item":call_item("fc","c","","in_progress")})).unwrap();
    d.push(&json!({"type":"response.function_call_arguments.delta","output_index":0,"item_id":"fc","delta":"{"})).unwrap();
    d.push(&json!({"type":"response.function_call_arguments.done","output_index":0,"item_id":"fc","arguments":"{"})).unwrap();
    d.push(&json!({"type":"response.output_item.done","output_index":0,"item":call_item("fc","c","{","incomplete")})).unwrap();
    assert!(d.finish().is_err());
    d.push(&json!({"type":"response.incomplete","response":envelope("incomplete",json!([call_item("fc","c","{","incomplete")]))})).unwrap();
    let decoded = d.materialize().unwrap();
    assert_eq!(decoded.semantic.outcome(), Outcome::Incomplete);
    let Item::ToolCall(c) = &decoded.semantic.items()[0].1 else {
        panic!()
    };
    assert_eq!(c.arguments.as_raw(), Some("{"));
    assert_eq!(c.status, ItemLifecycle::Incomplete);
}
#[test]
fn function_done_name_echo_must_match_its_existing_call_owner() {
    for name in [json!("lookup"), json!("different"), json!(null)] {
        let mut decoder = EventDecoder::new(Profile::Responses);
        decoder.push(&created()).unwrap();
        decoder.push(&json!({"type":"response.output_item.added","output_index":0,"item":call_item("fc","c","{}","in_progress")})).unwrap();
        let result=decoder.push(&json!({"type":"response.function_call_arguments.done","output_index":0,"item_id":"fc","name":name,"arguments":"{}"}));
        assert_eq!(result.is_ok(), name == json!("lookup"));
        if result.is_err() {
            assert!(decoder.finish().is_err());
        }
    }
}

#[test]
fn value_done_disallows_later_delta_and_poisoned_decoders_cannot_resume() {
    let mut d = EventDecoder::new(Profile::Responses);
    d.push(&created()).unwrap();
    d.push(&json!({"type":"response.output_item.added","output_index":0,"item":call_item("fc","c","{}","in_progress")})).unwrap();
    d.push(&json!({"type":"response.function_call_arguments.done","output_index":0,"item_id":"fc","arguments":"{}"})).unwrap();
    assert!(d.push(&json!({"type":"response.function_call_arguments.delta","output_index":0,"item_id":"fc","delta":"x"})).is_err());
    assert!(d.push(&json!({"type":"response.output_item.done","output_index":0,"item":call_item("fc","c","{}","completed")})).is_err());
}
#[test]
fn duplicate_call_part_and_item_identity_and_open_success_fail() {
    let base = vec![
        StreamEvent::Started,
        start(
            1,
            ItemKind::ToolCall {
                format: ArgumentFormat::Raw,
                call_id: text("a"),
                name: text("f"),
                message: None,
                context: CallContext::default(),
            },
        ),
    ];
    let state = apply(&base).unwrap();
    assert!(
        reduce(
            state.clone(),
            start(
                2,
                ItemKind::ToolCall {
                    format: ArgumentFormat::Raw,
                    call_id: text("a"),
                    name: text("g"),
                    message: None,
                    context: CallContext::default(),
                }
            )
        )
        .is_err()
    );
    assert!(reduce(state.clone(), terminal(StreamTerminal::Completed)).is_err());
    assert!(reduce(state, start(1, ItemKind::Message { phase: None })).is_err());
    let mut e = vec![
        StreamEvent::Started,
        start(1, ItemKind::Message { phase: None }),
    ];
    e.extend(part(1, 5, PartKind::Text, "a"));
    e.push(start(2, ItemKind::Message { phase: None }));
    e.push(StreamEvent::PartStarted {
        item: ItemId::new(2),
        part: PartId::new(5),
        kind: PartKind::Text,
    });
    assert!(apply(&e).is_err());
}
#[test]
fn non_success_terminals_keep_partial_output_and_error_is_not_materializable() {
    for t in [
        StreamTerminal::Incomplete,
        StreamTerminal::Failed,
        StreamTerminal::Cancelled,
    ] {
        let events = vec![
            StreamEvent::Started,
            start(
                1,
                ItemKind::ToolCall {
                    format: ArgumentFormat::Raw,
                    call_id: text("a"),
                    name: text("f"),
                    message: None,
                    context: CallContext::default(),
                },
            ),
            StreamEvent::PartStarted {
                item: ItemId::new(1),
                part: PartId::new(1),
                kind: PartKind::Arguments,
            },
            StreamEvent::Delta {
                item: ItemId::new(1),
                part: PartId::new(1),
                fragment: "{".into(),
                logprobs: vec![],
            },
            terminal(t),
        ];
        if t == StreamTerminal::Cancelled {
            // Cancellation still materializes partial typed output, but cannot
            // invent a standard wire event to deliver that snapshot.
            let response = materialize(&apply(&events).unwrap()).unwrap();
            assert_eq!(response.outcome(), Outcome::Cancelled);
            let Item::ToolCall(call) = &response.items()[0].1 else {
                panic!("partial call")
            };
            assert_eq!(call.arguments.as_raw(), Some("{"));
            assert_eq!(response.continuation(), Continuation::Unreported);
            let mut encoder = EventEncoder::new(Profile::Responses, metadata()).unwrap();
            let source = FidelityRecords::default();
            for event in &events[..events.len() - 1] {
                encoder.encode(event, &source).unwrap();
            }
            assert!(encoder.encode(events.last().unwrap(), &source).is_err());
            assert!(encoder.finish().is_err());
            continue;
        }
        let wire = encode(&events, Profile::Responses, &FidelityRecords::default());
        assert_eq!(
            wire.last().unwrap()["response"]["output"][0]["arguments"],
            "{"
        );
        assert_eq!(
            wire.last().unwrap()["response"]["output"][0]["status"],
            "in_progress"
        );
        let mut d = EventDecoder::new(Profile::Responses);
        for v in wire {
            d.push(&v).unwrap();
        }
        let response = d.materialize().unwrap().semantic;
        assert_ne!(response.outcome(), Outcome::Completed);
        assert_eq!(response.continuation(), Continuation::Unreported);
    }
    let mut d = EventDecoder::new(Profile::Responses);
    d.push(&json!({"type":"error","code":"server_error","message":"synthetic error","param":null}))
        .unwrap();
    assert!(d.finish().is_ok());
    assert!(d.materialize().is_err());
}
#[test]
fn event_encoder_rejects_duplicate_terminal_and_unrepresentable_chat_grouping() {
    let mut e = EventEncoder::new(Profile::Responses, metadata()).unwrap();
    let f = FidelityRecords::default();
    e.encode(&StreamEvent::Started, &f).unwrap();
    e.encode(&terminal(StreamTerminal::Completed), &f).unwrap();
    assert!(e.encode(&terminal(StreamTerminal::Completed), &f).is_err());
    let mut e = EventEncoder::new(Profile::Chat, metadata()).unwrap();
    e.encode(&StreamEvent::Started, &f).unwrap();
    e.encode(&start(1, ItemKind::Message { phase: None }), &f)
        .unwrap();
    assert!(
        e.encode(&start(2, ItemKind::Message { phase: None }), &f)
            .is_err()
    );
}
