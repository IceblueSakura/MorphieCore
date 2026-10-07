//! Explicit selection and current configuration, never a session or executor.
use morphiecore::semantic::{
    task::generation::*,
    value::{Presence, Text},
};
fn text(value: &str) -> Text {
    Text::new(value, "synthetic", 256).unwrap()
}
fn call() -> Item {
    Item::ToolCall(ToolCall {
        call_id: text("C"),
        name: text("lookup"),
        arguments: "{}".into(),
        status: ItemLifecycle::Completed,
        context: CallContext::default(),
    })
}
fn result() -> Item {
    Item::ToolResult(ToolResult {
        call_id: text("C"),
        output: "reported".into(),
        execution: None,
        status: None,
        context: CallContext::default(),
    })
}
fn provider(reference: bool) -> Item {
    Item::ProviderTool(ProviderToolObservation {
        source: NativeAliasDomain {
            source: text("abstract"),
            scope: LocalScope::new(8),
        },
        operation: if reference {
            ProviderOperation::Reference(ProviderOperationReference::Native(text("S")))
        } else {
            ProviderOperation::Reported {
                tool: text("search"),
                alias: Some(text("S")),
                requester: ProviderRequester::Model,
            }
        },
        progress: None,
        execution: None,
        output: reference.then(|| "answer".into()),
        artifact_status: None,
    })
}
#[test]
fn selected_mixed_history_and_explicit_settings_build_a_successor_without_execution() {
    let r1 = GenerationResponse::new(
        vec![(ItemId::new(1), call()), (ItemId::new(2), provider(false))],
        Outcome::Completed,
    )
    .unwrap()
    .with_replay_groups(vec![
        ReplayGroup::new(
            GroupId::new(LocalScope::ROOT, 1),
            vec![ItemId::new(1), ItemId::new(2)],
        )
        .unwrap(),
    ])
    .unwrap();
    let settings = GenerationSettings {
        instructions: Presence::Value(text("current")),
        ..Default::default()
    };
    let selected = ClientManaged::new(settings.clone())
        .unwrap()
        .select_response(&r1)
        .unwrap();
    let pending = selected.clone().build(&[]).unwrap();
    assert!(
        matches!(pending.continuation(),Continuation::ToolResults(ref calls) if calls.len()==1)
    );
    let r2 = GenerationResponse::new(vec![(ItemId::new(4), provider(true))], Outcome::Completed)
        .unwrap();
    let final_request = selected
        .append_items(vec![(ItemId::new(3), result())])
        .unwrap()
        .select_response(&r2)
        .unwrap()
        .build(&[])
        .unwrap();
    assert_eq!(final_request.settings(), &settings);
    assert_eq!(final_request.items().len(), 4);
    assert_eq!(final_request.replay_groups(), r1.replay_groups());
    assert_eq!(final_request.continuation(), Continuation::Unreported);
    assert_eq!(r1.items().len(), 2);
    assert!(
        ClientManaged::new(settings)
            .unwrap()
            .select_response(&r2)
            .is_err()
    );
}
#[test]
fn explicit_configuration_and_declared_dependencies_prevent_implicit_inheritance() {
    let old = GenerationRequest::from_settings(
        vec![(ItemId::new(1), call())],
        GenerationSettings {
            instructions: Presence::Value(text("old")),
            ..Default::default()
        },
    )
    .unwrap();
    let proof = RequestDependencyProof::capture(
        &old,
        HistoryDependency::Owners(vec![ItemId::new(1)]),
        SettingsDependency::only(SettingsField::Instructions),
    )
    .unwrap();
    let changed = ClientManaged::new(GenerationSettings::default())
        .unwrap()
        .select_request(&old)
        .unwrap();
    assert!(changed.clone().build(&[proof]).is_err());
    let request = changed.build(&[]).unwrap();
    assert!(request.settings().instructions.is_absent());
    assert_eq!(old.settings().instructions, Presence::Value(text("old")));
    assert!(
        ClientManaged::new(GenerationSettings::default())
            .unwrap()
            .append_items(vec![(ItemId::new(3), result())])
            .is_err()
    );
    assert!(
        ClientManaged::new(GenerationSettings::default())
            .unwrap()
            .select_request(&old)
            .unwrap()
            .select_request(&old)
            .is_err()
    );
}
#[test]
fn first_request_needs_no_session_and_selected_relations_are_not_lost() {
    // Initial input does not require a distinct Fresh mode.
    let message = Item::Message(Message {
        role: MessageRole::Assistant,
        parts: vec![],
        status: ItemLifecycle::Completed,
        phase: None,
    });
    let response = GenerationResponse::new(
        vec![(ItemId::new(9), message), (ItemId::new(1), call())],
        Outcome::Completed,
    )
    .unwrap()
    .with_message_owners(vec![(ItemId::new(1), ItemId::new(9))])
    .unwrap();
    let request = ClientManaged::new(GenerationSettings::default())
        .unwrap()
        .select_response(&response)
        .unwrap()
        .build(&[])
        .unwrap();
    assert_eq!(request.message_owners(), response.message_owners());
    let first = ClientManaged::new(GenerationSettings::default())
        .unwrap()
        .append_items(vec![(
            ItemId::new(1),
            Item::Message(Message {
                role: MessageRole::User,
                parts: vec![Part {
                    id: PartId::new(1),
                    content: ContentPart::Text(text("hello").into()),
                }],
                status: ItemLifecycle::Completed,
                phase: None,
            }),
        )])
        .unwrap()
        .build(&[])
        .unwrap();
    assert_eq!(first.items().len(), 1);
    assert!(
        ClientManaged::new(GenerationSettings::default())
            .unwrap()
            .build(&[])
            .is_err()
    );
}

#[test]
fn selected_derivations_and_budgets_survive_without_copying_source_configuration() {
    let source = GenerationRequest::new(
        vec![(ItemId::new(1), call())],
        GenerationControls::default(),
    )
    .unwrap();
    let mut replacement = call();
    let Item::ToolCall(value) = &mut replacement else {
        unreachable!()
    };
    value.call_id = text("C2");
    let source = source
        .revise_call(ItemId::new(1), (ItemId::new(2), replacement))
        .unwrap();
    let selected = ClientManaged::new(GenerationSettings::default())
        .unwrap()
        .select_request(&source)
        .unwrap()
        .build(&[])
        .unwrap();
    assert_eq!(selected.call_derivations(), source.call_derivations());
    let oversized = (0..=MAX_ITEMS)
        .map(|index| (ItemId::new(index as u64), provider(false)))
        .collect();
    assert!(matches!(
        ClientManaged::new(GenerationSettings::default())
            .unwrap()
            .append_items(oversized),
        Err(GenerationError::Limit)
    ));
    let mut invalid = GenerationSettings::default();
    invalid.controls.max_output_tokens = Some(0);
    assert!(ClientManaged::new(invalid).is_err());
}
