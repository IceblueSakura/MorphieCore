//! A08/T28: semantic membership is independent of target contiguity.
use morphiecore::{
    lowering::generation::{GenerationRepresentationContract as Contract, lower_request},
    protocol::{fidelity::FidelityRecords, openai::Profile},
    semantic::{task::generation::*, value::Text},
};
fn text(value: &str) -> Text {
    Text::new(value, "synthetic", 64).unwrap()
}
#[test]
fn noncontiguous_explicit_message_membership_is_core_data_not_a_target_permission() {
    let items = values();
    let request = GenerationRequest::new(items, GenerationControls::default())
        .unwrap()
        .with_message_owners(vec![(ItemId::new(3), ItemId::new(1))])
        .unwrap();
    let group = request.message_groups().next().unwrap();
    assert_eq!(group.owner(), ItemId::new(1));
    assert_eq!(group.calls().next().unwrap().item, ItemId::new(3));
    for profile in [Profile::Chat, Profile::Responses] {
        assert!(
            lower_request(
                &request,
                &FidelityRecords::default(),
                profile,
                Contract::full()
            )
            .is_err()
        );
    }
}
fn values() -> Vec<(ItemId, Item)> {
    vec![
        (
            ItemId::new(1),
            Item::Message(Message {
                role: MessageRole::Assistant,
                parts: vec![],
                status: ItemLifecycle::Completed,
                phase: None,
            }),
        ),
        (
            ItemId::new(2),
            Item::Reasoning(ReasoningItem {
                parts: vec![],
                status: ItemLifecycle::Completed,
                replay: None,
            }),
        ),
        (
            ItemId::new(3),
            Item::ToolCall(ToolCall {
                call_id: text("C"),
                name: text("lookup"),
                arguments: "{}".into(),
                status: ItemLifecycle::Completed,
                context: CallContext::default(),
            }),
        ),
    ]
}
#[test]
fn membership_is_unique_and_dangling_or_wrong_kind_owners_are_rejected() {
    let source = GenerationRequest::new(values(), GenerationControls::default()).unwrap();
    for declarations in [
        vec![
            (ItemId::new(3), ItemId::new(1)),
            (ItemId::new(3), ItemId::new(1)),
        ],
        vec![(ItemId::new(3), ItemId::new(2))],
        vec![(ItemId::new(3), ItemId::new(99))],
        vec![(ItemId::new(2), ItemId::new(1))],
    ] {
        assert_eq!(
            source
                .clone()
                .with_message_owners(declarations)
                .unwrap_err(),
            GenerationError::InvalidMessageGroup
        );
    }
    let linked = source
        .clone()
        .with_message_owners(vec![(ItemId::new(3), ItemId::new(1))])
        .unwrap();
    assert!(source.message_owners().is_empty());
    assert_eq!(linked.message_owners().len(), 1);
    assert!(
        linked
            .clone()
            .retain_items(|id, _| id != ItemId::new(1))
            .is_err()
    );
    let deleted = linked.retain_items(|id, _| id != ItemId::new(3)).unwrap();
    assert!(deleted.message_owners().is_empty());
}
#[test]
fn prefix_cannot_cut_an_interleaved_explicit_group() {
    use morphiecore::semantic::cache::{CachePrefixContext, CachePrefixIntent, CachePrefixScope};
    let request = GenerationRequest::new(values(), GenerationControls::default())
        .unwrap()
        .with_message_owners(vec![(ItemId::new(3), ItemId::new(1))])
        .unwrap();
    let scope = CachePrefixScope::new("synthetic").unwrap();
    let hints = Default::default();
    let context = CachePrefixContext {
        model: "synthetic",
        hints: &hints,
        grouping: None,
    };
    assert!(
        CachePrefixIntent::through(ItemId::new(2))
            .capture(&request, context, &scope)
            .is_err()
    );
    assert!(
        CachePrefixIntent::through(ItemId::new(3))
            .capture(&request, context, &scope)
            .is_ok()
    );
}

#[test]
fn noncontiguous_membership_events_match_static_and_targets_reject_before_call_emission() {
    use morphiecore::lowering::events::check_event;
    let mut state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
    for (id, kind) in [
        (1, ItemKind::Message { phase: None }),
        (2, ItemKind::Reasoning),
    ] {
        state = reduce(
            state,
            StreamEvent::ItemStarted {
                item: ItemId::new(id),
                kind,
                replay: None,
            },
        )
        .unwrap();
        state = reduce(
            state,
            StreamEvent::ItemFinished {
                item: ItemId::new(id),
                status: ItemLifecycle::Completed,
                replay: None,
            },
        )
        .unwrap();
    }
    let call = StreamEvent::ItemStarted {
        item: ItemId::new(3),
        kind: ItemKind::ToolCall {
            format: ArgumentFormat::Raw,
            call_id: text("C"),
            name: text("lookup"),
            message: Some(ItemId::new(1)),
            context: CallContext::default(),
        },
        replay: None,
    };
    for profile in [Profile::Chat, Profile::Responses] {
        assert!(check_event(&state, &call, profile, &Contract::full()).is_err());
    }
    state = reduce(state, call).unwrap();
    state = reduce(
        state,
        StreamEvent::PartStarted {
            item: ItemId::new(3),
            part: PartId::new(1),
            kind: PartKind::Arguments,
        },
    )
    .unwrap();
    state = reduce(
        state,
        StreamEvent::Delta {
            item: ItemId::new(3),
            part: PartId::new(1),
            fragment: "{}".into(),
            logprobs: vec![],
        },
    )
    .unwrap();
    state = reduce(
        state,
        StreamEvent::ValueFinished {
            item: ItemId::new(3),
            part: PartId::new(1),
        },
    )
    .unwrap();
    state = reduce(
        state,
        StreamEvent::PartFinished {
            item: ItemId::new(3),
            part: PartId::new(1),
        },
    )
    .unwrap();
    state = reduce(
        state,
        StreamEvent::ItemFinished {
            item: ItemId::new(3),
            status: ItemLifecycle::Completed,
            replay: None,
        },
    )
    .unwrap();
    state = reduce(
        state,
        StreamEvent::Terminal {
            terminal: StreamTerminal::Completed,
            details: TerminalDetails::default(),
        },
    )
    .unwrap();
    let expected = GenerationResponse::new(values(), Outcome::Completed)
        .unwrap()
        .with_message_owners(vec![(ItemId::new(3), ItemId::new(1))])
        .unwrap();
    assert_eq!(materialize(&state).unwrap(), expected);
}

#[test]
fn continuation_rejects_lost_replay_group_and_added_message_membership() {
    let items = vec![values()[0].clone(), values()[2].clone()];
    let group = ReplayGroup::new(
        GroupId::new(LocalScope::ROOT, 1),
        vec![ItemId::new(1), ItemId::new(3)],
    )
    .unwrap();
    let response = GenerationResponse::new(items.clone(), Outcome::Completed)
        .unwrap()
        .with_replay_groups(vec![group.clone()])
        .unwrap();
    let exchange = ResponseContinuation::new(
        ResponseRelation::new(TurnId::new(1), ResponseId::new(1)),
        &response,
    );
    let plain = GenerationRequest::new(items, GenerationControls::default()).unwrap();
    assert_eq!(
        exchange.inspect(&plain).unwrap_err(),
        ContinuationError::ChangedResponse
    );
    let history = plain.with_replay_groups(vec![group]).unwrap();
    exchange.inspect(&history).unwrap();
    let changed = history
        .with_message_owners(vec![(ItemId::new(3), ItemId::new(1))])
        .unwrap();
    assert_eq!(
        exchange.inspect(&changed).unwrap_err(),
        ContinuationError::ChangedResponse
    );
}
