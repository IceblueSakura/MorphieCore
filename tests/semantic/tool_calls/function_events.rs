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
fn function_done_supplies_missing_argument_suffix_but_never_rewrites() {
    // Some Responses servers stream a call's arguments partly or not at all.
    for (deltas, suffix) in [
        (vec![], Some(r#"{"a":1}"#)),
        (vec![r#"{"a":"#], Some("1}")),
        (vec![r#"{"a":1}"#], None),
        (vec!["{}"], None),
    ] {
        let mut decoder = EventDecoder::new(Profile::Responses);
        decoder.push(&created()).unwrap();
        decoder.push(&json!({"type":"response.output_item.added","output_index":0,"item":call_item("fc","c","","in_progress")})).unwrap();
        for delta in &deltas {
            decoder.push(&json!({"type":"response.function_call_arguments.delta","output_index":0,"item_id":"fc","delta":delta})).unwrap();
        }
        let done = decoder.push(&json!({"type":"response.function_call_arguments.done","output_index":0,"item_id":"fc","arguments":r#"{"a":1}"#}));
        if deltas == ["{}"] {
            assert!(done.is_err(), "a conflicting done is rejected");
            continue;
        }
        let emitted: Vec<_> = done
            .unwrap()
            .into_iter()
            .filter_map(|event| match event {
                StreamEvent::Delta { fragment, .. } => Some(fragment),
                _ => None,
            })
            .collect();
        assert_eq!(
            emitted,
            suffix.into_iter().map(String::from).collect::<Vec<_>>()
        );
    }
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
    assert!(
        e.finish().is_err(),
        "an early completeness check is not rejection"
    );
    e.encode(&StreamEvent::Started, &f).unwrap();
    e.encode(&terminal(StreamTerminal::Completed), &f).unwrap();
    e.finish().unwrap();
    assert!(e.encode(&terminal(StreamTerminal::Completed), &f).is_err());
    assert!(
        e.finish().is_err(),
        "a rejected completed stream stays rejected"
    );
    let mut e = EventEncoder::new(Profile::Chat, metadata()).unwrap();
    e.encode(&StreamEvent::Started, &f).unwrap();
    e.encode(&start(1, ItemKind::Message { phase: None }), &f)
        .unwrap();
    assert!(
        e.encode(&start(2, ItemKind::Message { phase: None }), &f)
            .is_err()
    );
    assert!(e.encode(&terminal(StreamTerminal::Completed), &f).is_err());
    assert!(e.finish().is_err());
}
