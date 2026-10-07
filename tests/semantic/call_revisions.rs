//! A05: observed arguments cannot change under an existing call identity.
use morphiecore::semantic::{task::generation::*, value::Text};
fn text(value: &str) -> Text {
    Text::new(value, "synthetic", 256).unwrap()
}
fn source() -> GenerationRequest {
    GenerationRequest::new(
        vec![
            (
                ItemId::new(1),
                Item::ToolCall(ToolCall {
                    call_id: text("old"),
                    name: text("lookup"),
                    arguments: "{}".into(),
                    status: ItemLifecycle::Completed,
                    context: CallContext::default(),
                }),
            ),
            (
                ItemId::new(2),
                Item::ToolResult(ToolResult {
                    call_id: text("old"),
                    output: "reported result".into(),
                    execution: None,
                    status: None,
                    context: CallContext::default(),
                }),
            ),
        ],
        GenerationControls::default(),
    )
    .unwrap()
}
#[test]
fn changed_arguments_cannot_reuse_owner_or_move_an_old_result() {
    let original = source();
    for replace_owner in [false, true] {
        let mut items = original.items().to_vec();
        if replace_owner {
            items[0].0 = ItemId::new(3);
        }
        let Item::ToolCall(call) = &mut items[0].1 else {
            unreachable!()
        };
        call.arguments = r#"{"changed":true}"#.into();
        assert_eq!(
            original.clone().with_items(items).unwrap_err(),
            GenerationError::InvalidDependency
        );
    }
    assert_eq!(original.items()[0].0, ItemId::new(1));
    let Item::ToolCall(call) = &original.items()[0].1 else {
        unreachable!()
    };
    assert_eq!(call.arguments.as_raw(), Some("{}"));
}

#[test]
fn explicit_revision_preserves_source_and_does_not_synthesize_or_retarget_results() {
    let original = source();
    let replacement = (
        ItemId::new(3),
        Item::ToolCall(ToolCall {
            call_id: text("new"),
            name: text("lookup"),
            arguments: r#"{"changed":true}"#.into(),
            status: ItemLifecycle::Completed,
            context: CallContext::default(),
        }),
    );
    assert_eq!(
        original
            .clone()
            .revise_call(ItemId::new(1), replacement.clone())
            .unwrap_err(),
        GenerationError::InvalidToolResult
    );
    let selected = original
        .clone()
        .retain_items(|_, item| !matches!(item, Item::ToolResult(_)))
        .unwrap();
    let edited = selected
        .clone()
        .revise_call(ItemId::new(1), replacement)
        .unwrap();
    assert_eq!(
        edited.call_derivations().get(&ItemId::new(3)),
        Some(&ItemId::new(1))
    );
    assert_eq!(edited.items().len(), 1);
    assert!(
        matches!(edited.continuation(), Continuation::ToolResults(ref pending) if pending[0].call_id == "new")
    );
    assert_eq!(original.items().len(), 2);
    assert!(selected.call_derivations().is_empty());
    let mut recycled = edited.items()[0].clone();
    recycled.0 = ItemId::new(1);
    let Item::ToolCall(call) = &mut recycled.1 else {
        unreachable!()
    };
    call.call_id = text("third");
    assert_eq!(
        edited
            .clone()
            .revise_call(ItemId::new(3), recycled)
            .unwrap_err(),
        GenerationError::InvalidDependency
    );
    for profile in [
        morphiecore::protocol::openai::Profile::Chat,
        morphiecore::protocol::openai::Profile::Responses,
    ] {
        assert!(
            morphiecore::lowering::generation::lower_request(
                &edited,
                &Default::default(),
                profile,
                morphiecore::lowering::generation::GenerationRepresentationContract::full(),
            )
            .is_err()
        );
    }
}

#[test]
fn structured_argument_order_is_authority_even_when_value_equality_ignores_it() {
    let mut items = source().items().to_vec();
    let Item::ToolCall(call) = &mut items[0].1 else {
        unreachable!()
    };
    call.arguments =
        ToolArguments::Structured(StructuredValue::from_bytes(br#"{"a":1,"b":2}"#).unwrap());
    let source = GenerationRequest::new(items, GenerationControls::default()).unwrap();
    let mut edited = source.items().to_vec();
    let Item::ToolCall(call) = &mut edited[0].1 else {
        unreachable!()
    };
    call.arguments =
        ToolArguments::Structured(StructuredValue::from_bytes(br#"{"b":2,"a":1}"#).unwrap());
    assert_eq!(
        source.with_items(edited).unwrap_err(),
        GenerationError::InvalidDependency
    );
}

#[test]
fn explicit_revision_keeps_declared_message_owner_without_moving_old_results() {
    let mut items = source().items().to_vec();
    items.pop();
    items.insert(
        0,
        (
            ItemId::new(9),
            Item::Message(Message {
                role: MessageRole::Assistant,
                parts: vec![],
                status: ItemLifecycle::Completed,
                phase: None,
            }),
        ),
    );
    let source = GenerationRequest::new(items, GenerationControls::default())
        .unwrap()
        .with_message_owners(vec![(ItemId::new(1), ItemId::new(9))])
        .unwrap();
    let mut replacement = source.items()[1].clone();
    replacement.0 = ItemId::new(3);
    let Item::ToolCall(call) = &mut replacement.1 else {
        unreachable!()
    };
    call.call_id = text("new");
    call.arguments = r#"{"changed":true}"#.into();
    let changed = source
        .clone()
        .revise_call(ItemId::new(1), replacement)
        .unwrap();
    assert_eq!(
        changed.message_owners().get(&ItemId::new(3)),
        Some(&ItemId::new(9))
    );
    assert!(!changed.message_owners().contains_key(&ItemId::new(1)));
    assert_eq!(
        source.message_owners().get(&ItemId::new(1)),
        Some(&ItemId::new(9))
    );
}
