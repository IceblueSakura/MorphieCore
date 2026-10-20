//! Explicit context transformations and bounded diagnostics, never automatic compaction.
use morphiecore::semantic::{task::generation::*, value::Text};

fn text(s: &str) -> Text {
    Text::new(s, "synthetic", MAX_TEXT_BYTES).unwrap()
}
fn message(id: u64, s: &str) -> (ItemId, Item) {
    (
        ItemId::new(id),
        Item::Message(Message {
            role: MessageRole::User,
            parts: vec![Part {
                replay: None,
                id: PartId::new(id),
                content: ContentPart::Text(text(s).into()),
                cache: None,
            }],
            status: ItemLifecycle::Completed,
            phase: None,
        }),
    )
}
fn source() -> GenerationRequest {
    GenerationRequest::new(
        vec![
            message(1, "private-first"),
            message(2, "second"),
            message(3, "third"),
        ],
        GenerationControls::default(),
    )
    .unwrap()
}
#[test]
fn atomic_edits_report_ids_preserve_source_and_do_not_leak_bodies() {
    let source = source();
    let branch = source
        .clone()
        .transform(vec![
            ContextEdit::Delete(vec![ItemId::new(2)]),
            ContextEdit::Insert {
                at: 1,
                item: message(4, "private-inserted"),
            },
            ContextEdit::Replace {
                owner: ItemId::new(3),
                item: message(3, "changed").1,
            },
            ContextEdit::Reorder(vec![ItemId::new(4), ItemId::new(1), ItemId::new(3)]),
        ])
        .unwrap();
    assert_eq!(
        branch
            .request()
            .items()
            .iter()
            .map(|(id, _)| id.get())
            .collect::<Vec<_>>(),
        vec![4, 1, 3]
    );
    assert_eq!(
        source
            .items()
            .iter()
            .map(|(id, _)| id.get())
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert_eq!(branch.changes().len(), 4);
    assert!(!format!("{:?}", branch.changes()).contains("private"));
    assert!(!format!("{branch:?}").contains("private"));
    assert!(
        source
            .clone()
            .transform(vec![ContextEdit::Reorder(vec![ItemId::new(1)])])
            .is_err()
    );
    assert!(
        source
            .transform(vec![ContextEdit::Delete(vec![ItemId::new(99)])])
            .is_err()
    );
}
#[test]
fn external_summary_is_new_user_content_with_explicit_sources_and_no_old_authority() {
    let source = source();
    let branch = source
        .clone()
        .transform(vec![ContextEdit::Summary {
            sources: vec![ItemId::new(1), ItemId::new(2)],
            owner: ItemId::new(8),
            part: PartId::new(8),
            text: text("external summary"),
        }])
        .unwrap();
    assert_eq!(branch.request().items().len(), 2);
    let Item::Message(m) = &branch.request().items()[0].1 else {
        panic!()
    };
    assert_eq!(m.role, MessageRole::User);
    assert_eq!(m.phase, None);
    assert_eq!(
        branch.changes(),
        &[ContextChange::Summarized {
            sources: vec![ItemId::new(1), ItemId::new(2)],
            owner: ItemId::new(8),
        }]
    );
    let mut items = source.items().to_vec();
    items[0].1 = Item::Instruction(Instruction {
        authority: InstructionAuthority::Developer,
        parts: vec![Part::new(PartId::new(1), text("protected"))],
        status: None,
    });
    let protected = GenerationRequest::new(items, GenerationControls::default()).unwrap();
    let result = protected.transform(vec![ContextEdit::Summary {
        sources: vec![ItemId::new(1)],
        owner: ItemId::new(9),
        part: PartId::new(9),
        text: text("untrusted summary"),
    }]);
    assert_eq!(result.unwrap_err().stage, ContextStage::Edit);
}
#[test]
fn sparse_response_selection_does_not_edit_or_reattribute_original_reports() {
    let mut items = vec![message(1, "one"), message(2, "two")];
    for (_, item) in &mut items {
        let Item::Message(message) = item else {
            panic!()
        };
        message.role = MessageRole::Assistant;
    }
    let usage = Usage::operation(10, 20, 30);
    let response = GenerationResponse::new(items, Outcome::Completed)
        .unwrap()
        .with_usage(Some(usage))
        .unwrap()
        .with_progress(InteractionProgress::NeedsContinuation)
        .unwrap();
    let selected = ClientManaged::new(GenerationSettings::default())
        .unwrap()
        .select_response_items(&response, &[ItemId::new(1)])
        .unwrap()
        .finish()
        .unwrap();
    assert_eq!(selected.request().items().len(), 1);
    assert_eq!(
        selected.selections()[0].progress,
        Some(InteractionProgress::NeedsContinuation)
    );
    assert_eq!(response.usage(), Some(usage));
    assert_eq!(response.items().len(), 2);
}

#[test]
fn edits_never_orphan_results_and_relations_have_one_final_owner() {
    let envelope = || {
        MessageEnvelope::new(
            GroupId::new(LocalScope::ROOT, 9),
            MessageEnvelopeRole::Assistant,
            vec![ItemId::new(9), ItemId::new(1)],
        )
        .unwrap()
    };
    let call = |alias| {
        Item::ToolCall(ToolCall {
            replay: None,
            call_id: text(alias),
            cache: None,
            name: text("lookup"),
            arguments: "{}".into(),
            status: ItemLifecycle::Completed,
            context: Default::default(),
        })
    };
    let mut parent = message(9, "answer");
    let Item::Message(m) = &mut parent.1 else {
        panic!()
    };
    m.role = MessageRole::Assistant;
    let source = GenerationRequest::new(
        vec![
            parent,
            (ItemId::new(1), call("C")),
            (
                ItemId::new(2),
                Item::ToolResult(ToolResult {
                    is_error: None,
                    cache: None,
                    call_id: Some(text("C")),
                    name: None,
                    output: "reported".into(),
                    execution: None,
                    status: None,
                    context: Default::default(),
                }),
            ),
        ],
        GenerationControls::default(),
    )
    .unwrap()
    .with_message_envelopes(vec![envelope()])
    .unwrap();
    let orphaned = source
        .clone()
        .transform(vec![ContextEdit::Delete(vec![ItemId::new(1)])])
        .unwrap_err();
    assert_eq!(orphaned.stage, ContextStage::Association);
    let edited = source
        .clone()
        .transform(vec![
            ContextEdit::Delete(vec![ItemId::new(2)]),
            ContextEdit::MessageEnvelopes(vec![envelope()]),
        ])
        .unwrap();
    assert_eq!(edited.request().message_envelopes(), [envelope()]);
    assert_eq!(edited.request().items().len(), 2);
    assert_eq!(source.items().len(), 3);
}
