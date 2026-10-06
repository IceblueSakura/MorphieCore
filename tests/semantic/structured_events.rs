//! Structured builders become one JSON authority, never a repaired string at terminal.
use crate::events_support::{self as events, text};
use morphiecore::{
    protocol::{
        fidelity::FidelityRecords,
        openai::{Profile, events::EventEncoder},
    },
    semantic::task::generation::*,
};
use serde_json::json;
fn opening() -> Vec<StreamEvent> {
    vec![
        StreamEvent::Started,
        events::start(
            1,
            ItemKind::ToolCall {
                format: ArgumentFormat::Json,
                call_id: text("c"),
                name: text("lookup"),
                message: None,
                context: CallContext::default(),
            },
        ),
        StreamEvent::PartStarted {
            item: ItemId::new(1),
            part: PartId::new(2),
            kind: PartKind::StructuredArguments,
        },
    ]
}
fn delta(fragment: &str) -> StreamEvent {
    StreamEvent::Delta {
        item: ItemId::new(1),
        part: PartId::new(2),
        fragment: fragment.into(),
        logprobs: vec![],
    }
}
#[test]
fn complete_structured_events_equal_independent_static_values_and_release_the_builder() {
    let mut input = opening();
    input.extend([
        delta("{\"z\":18446744073709551616001,"),
        delta("\"a\":null}"),
        StreamEvent::ValueFinished {
            item: ItemId::new(1),
            part: PartId::new(2),
        },
        StreamEvent::PartFinished {
            item: ItemId::new(1),
            part: PartId::new(2),
        },
        events::close(1, ItemLifecycle::Completed),
        events::terminal(StreamTerminal::Completed),
    ]);
    let state = events::apply(&input).unwrap();
    assert!(matches!(
        state.part(ItemId::new(1), PartId::new(2)).unwrap().value(),
        StreamPartValue::Json(_)
    ));
    let value = StructuredValue::from_bytes(br#"{"z":18446744073709551616001,"a":null}"#).unwrap();
    let expected = GenerationResponse::new(
        vec![(
            ItemId::new(1),
            Item::ToolCall(ToolCall {
                call_id: text("c"),
                name: text("lookup"),
                arguments: ToolArguments::Structured(value),
                message: None,
                status: ItemLifecycle::Completed,
                context: CallContext::default(),
            }),
        )],
        Outcome::Completed,
    )
    .unwrap();
    assert_eq!(materialize(&state).unwrap(), expected);
}
#[test]
fn incomplete_or_invalid_json_never_becomes_a_completed_argument() {
    for fragment in ["{", "{\"x\":1,\"x\":2}"] {
        let mut input = opening();
        input.push(delta(fragment));
        let state = events::apply(&input).unwrap();
        assert!(
            reduce(
                state.clone(),
                StreamEvent::ValueFinished {
                    item: ItemId::new(1),
                    part: PartId::new(2)
                }
            )
            .is_err()
        );
        assert!(reduce(state, events::terminal(StreamTerminal::Completed)).is_err());
    }
    let mut input = opening();
    input.push(delta("{"));
    input.push(events::close(1, ItemLifecycle::Incomplete));
    input.push(events::terminal(StreamTerminal::Incomplete));
    let response = materialize(&events::apply(&input).unwrap()).unwrap();
    assert!(
        matches!(&response.items()[0].1,Item::ToolCall(c) if matches!(&c.arguments,ToolArguments::StructuredPartial(s) if s=="{"))
    );
    assert_eq!(response.continuation(), Continuation::Unreported);
}
#[test]
fn unsupported_event_targets_reject_before_publishing_the_structured_call() {
    for profile in [Profile::Chat, Profile::Responses] {
        let mut encoder = EventEncoder::new(profile, events::metadata())
            .unwrap()
            .with_contract(events::contract());
        let input = opening();
        encoder
            .encode(&input[0], &FidelityRecords::default())
            .unwrap();
        assert!(
            encoder
                .encode(&input[1], &FidelityRecords::default())
                .is_err()
        );
        assert!(
            encoder
                .encode(
                    &events::terminal(StreamTerminal::Completed),
                    &FidelityRecords::default()
                )
                .is_err()
        );
    }
    assert_eq!(
        StructuredValue::from_bytes(b"[]").unwrap().value(),
        &json!([])
    );
    assert!(StructuredValue::from_bytes(&vec![b' '; MAX_TEXT_BYTES + 1]).is_err());
}
