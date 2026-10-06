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
fn exact_arguments_and_deleted_fragments_drive_static_and_event_output() {
    let mut events = vec![StreamEvent::Started];
    events.extend(call(7, 8, "c", "{broken"));
    events.push(terminal(StreamTerminal::Completed));
    let r = materialize(&apply(&events).unwrap()).unwrap();
    let Item::ToolCall(c) = &r.items()[0].1 else {
        panic!()
    };
    assert_eq!(c.arguments.as_raw(), Some("{broken"));
    assert_eq!(c.call_id.as_str(), "c");
    let wire = encode(&events, Profile::Responses, &FidelityRecords::default());
    assert_eq!(
        wire.last().unwrap()["response"]["output"],
        json!([call_item("item_7", "c", "{broken", "completed")])
    );
    events.retain(|e| !matches!(e, StreamEvent::Delta { .. }));
    let wire = encode(&events, Profile::Responses, &FidelityRecords::default());
    assert_eq!(
        wire.last().unwrap()["response"]["output"][0]["arguments"],
        ""
    );
}
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
fn mixed_output_uses_one_index_space_and_original_order() {
    let mut events = vec![StreamEvent::Started];
    events.extend(call(90, 30, "a", "{}"));
    events.push(start(40, ItemKind::Message { phase: None }));
    events.extend(part(40, 20, PartKind::Text, "answer"));
    events.push(close(40, ItemLifecycle::Completed));
    events.extend(call(10, 70, "b", "[]"));
    events.push(terminal(StreamTerminal::Completed));
    let wire = encode(&events, Profile::Responses, &FidelityRecords::default());
    for v in &wire {
        if let Some(id) = v.get("item_id").and_then(|v| v.as_str()) {
            assert_eq!(
                v["output_index"],
                match id {
                    "item_90" => 0,
                    "item_40" => 1,
                    "item_10" => 2,
                    _ => panic!(),
                }
            );
        }
    }
    let output = wire.last().unwrap()["response"]["output"]
        .as_array()
        .unwrap();
    assert_eq!(
        output
            .iter()
            .map(|v| v["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["item_90", "item_40", "item_10"]
    );
    let mut decoder = EventDecoder::new(Profile::Responses);
    for v in wire {
        decoder.push(&v).unwrap();
    }
    assert_eq!(decoder.materialize().unwrap().semantic.items().len(), 3);
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
fn snapshot_identity_name_and_status_conflicts_are_rejected() {
    for key in ["name", "call_id", "id", "arguments"] {
        let mut d = EventDecoder::new(Profile::Responses);
        d.push(&created()).unwrap();
        d.push(&json!({"type":"response.output_item.added","output_index":0,"item":call_item("fc","c","{}","in_progress")})).unwrap();
        d.push(&json!({"type":"response.function_call_arguments.done","output_index":0,"item_id":"fc","arguments":"{}"})).unwrap();
        let mut item = call_item("fc", "c", "{}", "completed");
        item[key] = json!("conflict");
        assert!(
            d.push(&json!({"type":"response.output_item.done","output_index":0,"item":item}))
                .is_err()
        );
    }
}
#[test]
fn chat_usage_tail_requires_actual_done_and_closes_to_same_function_semantics() {
    let mut d = EventDecoder::new(Profile::Chat);
    d.push(&json!({"id":"r","object":"chat.completion.chunk","created":0,"model":"synthetic","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"c","type":"function","function":{"name":"lookup","arguments":"{}"}}]},"finish_reason":"tool_calls"}]})).unwrap();
    assert!(d.finish().is_err());
    d.push(&json!({"id":"r","object":"chat.completion.chunk","created":0,"model":"synthetic","choices":[],"usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}})).unwrap();
    d.done().unwrap();
    let r = d.materialize().unwrap().semantic;
    assert_eq!(r.usage().unwrap().total_tokens, Some(5));
    assert_eq!(r.outcome(), Outcome::Completed);
    assert_eq!(
        r.continuation(),
        Continuation::ToolResults(vec![CallReference {
            item: ItemId::new(2),
            call_id: "c",
        }])
    );
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
