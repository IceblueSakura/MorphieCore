//! A07/T06: overlapping groups borrow ordered members; no wire carrier is invented.
use morphiecore::{
    lowering::generation::{GenerationRepresentationContract as Contract, lower_request},
    protocol::{fidelity::FidelityRecords, openai::Profile},
    semantic::{task::generation::*, value::Text},
};
fn text(value: &str) -> Text {
    Text::new(value, "synthetic", 256).unwrap()
}
fn group(value: u64, owners: &[u64]) -> ReplayGroup {
    ReplayGroup::new(
        GroupId::new(LocalScope::new(7), value),
        owners.iter().map(|n| ItemId::new(*n)).collect(),
    )
    .unwrap()
}
fn items() -> Vec<(ItemId, Item)> {
    vec![
        (
            ItemId::new(1),
            Item::Reasoning(ReasoningItem {
                parts: vec![(
                    PartId::new(1),
                    ReasoningContent::Summary(text("reported summary")),
                )],
                status: ItemLifecycle::Completed,
                replay: None,
            }),
        ),
        (
            ItemId::new(2),
            Item::ToolCall(ToolCall {
                call_id: text("C"),
                name: text("lookup"),
                arguments: "{}".into(),
                status: ItemLifecycle::Completed,
                context: CallContext::default(),
            }),
        ),
        (
            ItemId::new(3),
            Item::ToolCall(ToolCall {
                call_id: text("other"),
                name: text("lookup"),
                arguments: "{}".into(),
                status: ItemLifecycle::Completed,
                context: CallContext::default(),
            }),
        ),
    ]
}
fn request() -> GenerationRequest {
    GenerationRequest::new(items(), GenerationControls::default())
        .unwrap()
        .with_replay_groups(vec![group(1, &[1, 2]), group(2, &[2, 3])])
        .unwrap()
}
#[test]
fn shared_member_has_two_derived_views_and_no_second_body() {
    let source = request();
    assert_eq!(source.replay_groups_for(ItemId::new(2)).count(), 2);
    let groups: Vec<_> = source.replay_group_views().collect();
    assert!(std::ptr::eq(
        groups[0].items().nth(1).unwrap(),
        &source.items()[1]
    ));
    assert!(std::ptr::eq(
        groups[1].items().next().unwrap(),
        &source.items()[1]
    ));
    assert_eq!(source.items().len(), 3);
    assert_eq!(
        groups[0].declaration().members(),
        &[ItemId::new(1), ItemId::new(2)]
    );
    assert!(!format!("{:?}", groups[0]).contains("reported summary"));
    assert!(!format!("{:?}", groups[0]).contains("lookup"));
}
#[test]
fn dangling_duplicate_and_reversed_memberships_fail_at_the_core_owner() {
    let source = request();
    assert!(
        source
            .clone()
            .retain_items(|id, _| id != ItemId::new(2))
            .is_err()
    );
    for groups in [
        vec![group(1, &[1, 2]), group(1, &[2, 3])],
        vec![group(1, &[1, 99])],
        vec![group(1, &[2, 1])],
    ] {
        assert_eq!(
            source.clone().with_replay_groups(groups).unwrap_err(),
            GenerationError::InvalidReplayGroup
        );
    }
    assert_eq!(
        ReplayGroup::new(GroupId::new(LocalScope::new(7), 1), vec![]).unwrap_err(),
        GenerationError::InvalidReplayGroup
    );
    assert!(
        ReplayGroup::new(GroupId::new(LocalScope::new(7), 1), vec![ItemId::new(1); 2]).is_err()
    );
    assert_eq!(source.replay_groups().len(), 2);
}
#[test]
fn groups_have_no_private_or_silently_dropped_standard_carrier() {
    let source = request();
    for profile in [Profile::Responses, Profile::Chat] {
        assert!(
            lower_request(
                &source,
                &FidelityRecords::default(),
                profile,
                Contract::full()
            )
            .is_err()
        );
    }
    let plain = source.with_replay_groups(vec![]).unwrap();
    assert!(
        lower_request(
            &plain,
            &FidelityRecords::default(),
            Profile::Responses,
            Contract::full()
        )
        .is_ok()
    );
}

#[test]
fn shared_dependencies_invalidate_each_group_without_rejecting_outside_edits() {
    let source = request();
    let proofs: Vec<_> = source
        .replay_groups()
        .iter()
        .map(|group| {
            RequestDependencyProof::capture(
                &source,
                HistoryDependency::ReplayGroup(group.id()),
                SettingsDependency::None,
            )
            .unwrap()
        })
        .collect();
    let mut changed = source.items().to_vec();
    let Item::ToolCall(call) = &mut changed[1].1 else {
        unreachable!()
    };
    call.arguments = r#"{"changed":true}"#.into();
    // A separately supplied context is not a mutation API or evidence that this
    // call actually happened. Old proofs still must reject changed dependencies.
    let changed = GenerationRequest::new(changed, GenerationControls::default())
        .unwrap()
        .with_replay_groups(source.replay_groups().to_vec())
        .unwrap();
    for proof in &proofs {
        assert_eq!(
            proof.check(&changed).unwrap_err(),
            GenerationError::InvalidDependency
        );
    }
    let mut outside = source.items().to_vec();
    outside.insert(
        1,
        (
            ItemId::new(99),
            Item::Message(Message {
                role: MessageRole::Assistant,
                parts: vec![],
                status: ItemLifecycle::Completed,
                phase: None,
            }),
        ),
    );
    let outside = source.clone().with_items(outside).unwrap();
    for proof in &proofs {
        proof.check(&outside).unwrap();
    }
    assert_eq!(source.items().len(), 3);
    assert_eq!(source.replay_groups_for(ItemId::new(2)).count(), 2);
    let removed = source.with_replay_groups(vec![group(2, &[2, 3])]).unwrap();
    assert!(proofs[0].check(&removed).is_err());
    proofs[1].check(&removed).unwrap();
}

#[test]
fn aggregate_membership_budget_counts_shared_references_independently() {
    for count in [MAX_ITEMS - 1, MAX_ITEMS, MAX_ITEMS + 1] {
        let groups = (0..count).map(|n| group(n as u64, &[2])).collect();
        let result = request().with_replay_groups(groups);
        assert_eq!(result.is_ok(), count <= MAX_ITEMS);
        if let Err(error) = result {
            assert_eq!(error, GenerationError::Limit);
        }
    }
    assert_eq!(
        ReplayGroup::new(
            GroupId::new(LocalScope::ROOT, 1),
            vec![ItemId::new(1); MAX_ITEMS + 1]
        )
        .unwrap_err(),
        GenerationError::Limit
    );
}

#[test]
fn normative_group_events_match_an_independent_static_declaration_and_cannot_reopen_terminal() {
    let mut state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
    for id in [1, 2, 3] {
        state = reduce(
            state,
            StreamEvent::ItemStarted {
                item: ItemId::new(id),
                kind: ItemKind::Message { phase: None },
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
    for group in [group(1, &[1, 2]), group(2, &[2, 3])] {
        let event = StreamEvent::ReplayGroup(group);
        for profile in [Profile::Chat, Profile::Responses] {
            assert!(
                morphiecore::lowering::events::check_event(
                    &state,
                    &event,
                    profile,
                    &Contract::full()
                )
                .is_err()
            );
        }
        state = reduce(state, event).unwrap();
    }
    assert!(reduce(state.clone(), StreamEvent::ReplayGroup(group(1, &[1, 3]))).is_err());
    assert!(reduce(state.clone(), StreamEvent::ReplayGroup(group(3, &[1, 99]))).is_err());
    state = reduce(
        state,
        StreamEvent::Terminal {
            terminal: StreamTerminal::Completed,
            details: TerminalDetails::default(),
        },
    )
    .unwrap();
    let observed = materialize(&state).unwrap();
    let expected = GenerationResponse::new(
        (1..=3)
            .map(|id| {
                (
                    ItemId::new(id),
                    Item::Message(Message {
                        role: MessageRole::Assistant,
                        parts: vec![],
                        status: ItemLifecycle::Completed,
                        phase: None,
                    }),
                )
            })
            .collect(),
        Outcome::Completed,
    )
    .unwrap()
    .with_replay_groups(vec![group(1, &[1, 2]), group(2, &[2, 3])])
    .unwrap();
    assert_eq!(observed, expected);
    assert!(reduce(state, StreamEvent::ReplayGroup(group(3, &[1, 3]))).is_err());
}
