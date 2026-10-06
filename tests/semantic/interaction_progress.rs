//! Reported progress never substitutes for operation outcome or execution permission.
use crate::events_support::{self as events, text};
use morphiecore::{
    lowering::generation::{
        GenerationRepresentationContract as Contract, RepresentationError, lower_response,
    },
    protocol::{
        fidelity::FidelityRecords,
        openai::{Profile, responses},
    },
    semantic::task::generation::*,
};
use serde_json::json;
fn call() -> (ItemId, Item) {
    (
        ItemId::new(1),
        Item::ToolCall(ToolCall {
            call_id: text("c"),
            name: text("lookup"),
            arguments: "{}".into(),
            message: None,
            status: ItemLifecycle::Completed,
            context: CallContext::default(),
        }),
    )
}
#[test]
fn outcome_and_pending_views_never_invent_reported_turn_completion() {
    let empty = GenerationResponse::new(vec![], Outcome::Completed).unwrap();
    assert_eq!(empty.progress(), InteractionProgress::Unreported);
    assert_eq!(empty.continuation(), Continuation::Unreported);
    let finished = empty
        .with_progress(InteractionProgress::TurnFinished)
        .unwrap();
    assert_eq!(finished.progress(), InteractionProgress::TurnFinished);
    let response = GenerationResponse::new(vec![call()], Outcome::Completed)
        .unwrap()
        .with_progress(InteractionProgress::AwaitingToolResults)
        .unwrap();
    assert_eq!(response.outcome(), Outcome::Completed);
    assert!(matches!(
        response.continuation(),
        Continuation::ToolResults(_)
    ));
    assert!(
        response
            .clone()
            .with_progress(InteractionProgress::TurnFinished)
            .is_err()
    );
    assert!(response.clone().with_items(vec![]).is_err());
    assert!(
        response
            .with_progress(InteractionProgress::Unreported)
            .unwrap()
            .with_items(vec![])
            .is_ok()
    );
    assert!(
        GenerationResponse::new(vec![], Outcome::Completed)
            .unwrap()
            .with_progress(InteractionProgress::AwaitingToolResults)
            .is_err()
    );
}
#[test]
fn static_and_event_progress_close_the_same_typed_response() {
    let mut input = vec![StreamEvent::Started];
    input.extend(events::call(1, 2, "c", "{}"));
    input.push(StreamEvent::Progress(
        InteractionProgress::AwaitingToolResults,
    ));
    let state = events::apply(&input).unwrap();
    assert!(
        reduce(
            state.clone(),
            events::start(3, ItemKind::Message { phase: None })
        )
        .is_err()
    );
    assert!(
        reduce(
            state.clone(),
            StreamEvent::Progress(InteractionProgress::TurnFinished)
        )
        .is_err()
    );
    let state = reduce(state, events::terminal(StreamTerminal::Completed)).unwrap();
    let response = materialize(&state).unwrap();
    let expected = GenerationResponse::new(vec![call()], Outcome::Completed)
        .unwrap()
        .with_progress(InteractionProgress::AwaitingToolResults)
        .unwrap();
    assert_eq!(response, expected);
    assert!(
        reduce(
            state,
            StreamEvent::Progress(InteractionProgress::TurnFinished)
        )
        .is_err()
    );
    let mut partial = vec![
        StreamEvent::Started,
        events::start(
            1,
            ItemKind::ToolCall {
                format: ArgumentFormat::Raw,
                call_id: text("c"),
                name: text("lookup"),
                message: None,
                context: CallContext::default(),
            },
        ),
    ];
    partial.push(StreamEvent::Progress(
        InteractionProgress::AwaitingToolResults,
    ));
    assert!(events::apply(&partial).is_err());
}
#[test]
fn existing_wire_intake_is_unreported_and_targets_do_not_drop_new_facts() {
    let wire = json!({"id":"r","object":"response","created_at":0,"model":"synthetic","status":"completed","output":[],"usage":null});
    assert_eq!(
        responses::decode_response(&wire)
            .unwrap()
            .semantic
            .progress(),
        InteractionProgress::Unreported
    );
    let response = GenerationResponse::new(vec![call()], Outcome::Completed)
        .unwrap()
        .with_progress(InteractionProgress::AwaitingToolResults)
        .unwrap();
    for profile in [Profile::Chat, Profile::Responses] {
        assert_eq!(
            lower_response(
                &response,
                &FidelityRecords::default(),
                &events::metadata(),
                profile,
                Contract::full()
            )
            .err(),
            Some(RepresentationError::InteractionProgress)
        );
    }
}
