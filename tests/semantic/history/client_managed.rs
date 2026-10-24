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
        replay: None,
        call_id: text("C"),
        cache: None,
        name: text("lookup"),
        arguments: "{}".into(),
        status: ItemLifecycle::Completed,
        context: CallContext::default(),
    })
}
fn result() -> Item {
    Item::ToolResult(ToolResult {
        is_error: None,
        cache: None,
        call_id: Some(text("C")),
        name: None,
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
                action: Some(ProviderAction::Search {
                    query: None,
                    queries: Some(vec![text("synthetic query")]),
                }),
            }
        },
        progress: None,
        execution: None,
        output: reference.then(|| "answer".into()),
        artifact_status: None,
    })
}

#[test]
fn selection_budgets_and_settings_are_validated() {
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

    let reference = CachedContextReference::new(text("cache-a"), None).unwrap();
    let source = GenerationRequest::new(
        vec![(
            ItemId::new(60),
            Item::Message(Message {
                participant: None,
                role: MessageRole::User,
                parts: vec![Part::new(
                    PartId::new(60),
                    ContentPart::Text(text("question").into()),
                )],
                status: ItemLifecycle::Completed,
                phase: None,
            }),
        )],
        GenerationControls::default(),
    )
    .unwrap()
    .with_cached_context(Some(reference.clone()))
    .unwrap();
    let selected = ClientManaged::new(Default::default())
        .unwrap()
        .select_request(&source)
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(selected.cached_context(), Some(&reference));
    let edited = selected
        .clone()
        .transform(vec![ContextEdit::Replace {
            owner: ItemId::new(60),
            item: Item::Message(Message {
                participant: None,
                role: MessageRole::User,
                parts: vec![Part::new(
                    PartId::new(60),
                    ContentPart::Text(text("edited").into()),
                )],
                status: ItemLifecycle::Completed,
                phase: None,
            }),
        }])
        .unwrap()
        .into_request();
    assert_eq!(edited.cached_context(), Some(&reference));
    assert_eq!(source.cached_context(), Some(&reference));
    let other = GenerationRequest::new(
        vec![(
            ItemId::new(61),
            Item::Message(Message {
                participant: None,
                role: MessageRole::User,
                parts: vec![Part::new(
                    PartId::new(61),
                    ContentPart::Text(text("next").into()),
                )],
                status: ItemLifecycle::Completed,
                phase: None,
            }),
        )],
        GenerationControls::default(),
    )
    .unwrap()
    .with_cached_context(Some(
        CachedContextReference::new(text("cache-b"), None).unwrap(),
    ))
    .unwrap();
    assert!(matches!(
        ClientManaged::new(Default::default())
            .unwrap()
            .select_request(&source)
            .unwrap()
            .select_request(&other),
        Err(GenerationError::InvalidContextTransform)
    ));
    let same_base = other.with_cached_context(Some(reference.clone())).unwrap();
    let combined = ClientManaged::new(Default::default())
        .unwrap()
        .select_request(&source)
        .unwrap()
        .select_request(&same_base)
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(combined.cached_context(), Some(&reference));
    assert_eq!(combined.items().len(), 2);
    let no_base = source.clone().with_cached_context(None).unwrap();
    assert!(matches!(
        ClientManaged::new(Default::default())
            .unwrap()
            .select_request(&no_base)
            .unwrap()
            .select_request(&same_base),
        Err(GenerationError::InvalidContextTransform)
    ));
    assert!(
        CachedContextReference::new(Text::allowing_empty("", "fixture", 1).unwrap(), None).is_err()
    );
    assert!(!format!("{reference:?}").contains("cache-a"));
}

#[test]
fn mixed_successor_separates_client_results_and_provider_progress() {
    let settings = GenerationSettings {
        instructions: Presence::Value(text("original configuration")),
        ..Default::default()
    };
    let r1 = GenerationResponse::new(
        vec![
            (
                ItemId::new(5),
                Item::Message(Message {
                    participant: None,
                    role: MessageRole::Assistant,
                    parts: vec![Part {
                        replay: None,
                        id: PartId::new(5),
                        cache: None,
                        content: ContentPart::Text(text("reported text").into()),
                    }],
                    status: ItemLifecycle::Completed,
                    phase: None,
                }),
            ),
            (
                ItemId::new(6),
                Item::Reasoning(ReasoningItem {
                    parts: vec![],
                    status: ItemLifecycle::Completed,
                    replay: None,
                }),
            ),
            (ItemId::new(1), call()),
            (ItemId::new(2), provider(false)),
        ],
        Outcome::Completed,
    )
    .unwrap()
    .with_progress(InteractionProgress::NeedsContinuation)
    .unwrap();
    let selected = ClientManaged::new(settings.clone())
        .unwrap()
        .select_response(&r1)
        .unwrap();
    let pending = selected.clone().finish().unwrap();
    let Continuation::ToolResults(calls) = pending.client_results() else {
        panic!()
    };
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].item, ItemId::new(1));
    let r2 = GenerationResponse::new(vec![(ItemId::new(4), provider(true))], Outcome::Completed)
        .unwrap();
    let successor = selected
        .append_items(vec![(ItemId::new(3), result())])
        .unwrap()
        .select_response(&r2)
        .unwrap()
        .finish()
        .unwrap();
    assert_eq!(successor.client_results(), Continuation::Unreported);
    assert_eq!(
        successor.selections()[1].progress,
        Some(InteractionProgress::Unreported)
    );
    // A Provider reference must resolve to an earlier reported operation.
    assert!(
        ClientManaged::new(settings)
            .unwrap()
            .select_response(&r2)
            .is_err()
    );
    assert_eq!(r1.items().len(), 4);
    assert_eq!(r1.progress(), InteractionProgress::NeedsContinuation);
}
