//! A01: validated edits must uphold the constructor's guarantee.
use morphiecore::semantic::{task::generation::*, value::Text};

fn request() -> GenerationRequest {
    GenerationRequest::from_settings(
        vec![],
        GenerationSettings {
            instructions: Some(Text::new("synthetic", "instruction", 64).unwrap().into()),
            parallel_tool_calls: Some(false),
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn reasoning_edit_must_not_return_an_invalid_request() {
    let mut reasoning = ReasoningRequest::absent();
    reasoning.activation = Some(ReasoningActivation::Disabled);
    let edited = request().with_reasoning(reasoning);
    assert_eq!(edited.unwrap_err(), GenerationError::InvalidControl);
}

#[test]
fn whole_history_edits_check_call_identity_without_pairwise_scans() {
    // 32k call/result pairs fit the item bound; pairwise checks took seconds.
    const CALLS: u64 = 32_000;
    let text = |value: &str| Text::new(value, "text", 64).unwrap();
    let call = |id: u64, name: &str| {
        Item::ToolCall(ToolCall {
            call_id: text(&format!("c{id}")),
            name: text(name),
            arguments: ToolArguments::Raw("{}".into()),
            status: ItemLifecycle::Completed,
            context: CallContext::default(),
            cache: None,
            replay: None,
        })
    };
    let mut items = vec![(
        ItemId::new(0),
        Item::Message(Message {
            participant: None,
            role: MessageRole::User,
            parts: vec![Part {
                replay: None,
                id: PartId::new(0),
                cache: None,
                content: ContentPart::Text(text("go").into()),
            }],
            status: ItemLifecycle::Completed,
            phase: None,
        }),
    )];
    for i in 0..CALLS {
        items.push((ItemId::new(2 * i + 1), call(i, "f")));
        items.push((
            ItemId::new(2 * i + 2),
            Item::ToolResult(ToolResult {
                call_id: Some(text(&format!("c{i}"))),
                name: None,
                output: ToolOutput::Text("ok".into()),
                is_error: None,
                execution: None,
                status: None,
                context: CallContext::default(),
                cache: None,
            }),
        ));
    }
    let request = GenerationRequest::new(items.clone(), GenerationControls::default()).unwrap();
    assert_eq!(
        request.clone().retain_items(|_, _| true).unwrap().items(),
        items.as_slice()
    );

    let last = items.len() - 2;
    let mut renamed = items.clone();
    renamed[last].1 = call(CALLS - 1, "g");
    assert_eq!(
        request.clone().with_items(renamed).unwrap_err(),
        GenerationError::InvalidDependency
    );

    let mut realiased = items;
    realiased[last].0 = ItemId::new(u64::MAX);
    realiased[last].1 = call(CALLS - 1, "g");
    assert_eq!(
        request.with_items(realiased).unwrap_err(),
        GenerationError::InvalidDependency
    );
}
