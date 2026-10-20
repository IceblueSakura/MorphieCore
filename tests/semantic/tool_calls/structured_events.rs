//! Structured builders become one JSON authority, never a repaired string at terminal.
use crate::events_support::{self as events, text};
use morphiecore::semantic::task::generation::*;
fn opening() -> Vec<StreamEvent> {
    vec![
        StreamEvent::Started,
        events::start(
            1,
            ItemKind::ToolCall {
                format: ArgumentFormat::Json,
                call_id: text("c"),
                name: text("lookup"),
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
