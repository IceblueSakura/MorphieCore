//! Explicit context transformations and bounded diagnostics, never automatic compaction.
use morphiecore::semantic::{task::generation::*, value::Text};

fn text(s: &str) -> Text {
    Text::new(s, "synthetic", MAX_TEXT_BYTES).unwrap()
}
fn message(id: u64, s: &str) -> (ItemId, Item) {
    (
        ItemId::new(id),
        Item::Message(Message {
            participant: None,
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
fn participant_labels_survive_selection_and_cannot_be_reattributed_under_the_same_owner() {
    let (id, mut item) = message(1, "hello");
    let Item::Message(m) = &mut item else {
        unreachable!()
    };
    m.participant = Some(text("alice"));
    let source = GenerationRequest::new(vec![(id, item.clone())], Default::default()).unwrap();
    let selected = ClientManaged::new(Default::default())
        .unwrap()
        .select_request(&source)
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(selected.items()[0].1.participant(), Some(&text("alice")));
    let mut edited = item.clone();
    let Item::Message(m) = &mut edited else {
        unreachable!()
    };
    m.participant = None;
    assert_eq!(
        source.clone().with_items(vec![(id, edited)]),
        Err(GenerationError::InvalidContextTransform)
    );
    assert!(
        source
            .clone()
            .transform(vec![ContextEdit::Summary {
                sources: vec![id],
                owner: ItemId::new(9),
                part: PartId::new(9),
                text: text("summary")
            }])
            .is_err()
    );
    let mut oversized = item;
    let Item::Message(m) = &mut oversized else {
        unreachable!()
    };
    m.participant = Some(
        Text::allowing_empty("x".repeat(MAX_TEXT_BYTES + 1), "synthetic", MAX_TOTAL_BYTES).unwrap(),
    );
    assert_eq!(
        GenerationRequest::new(vec![(id, oversized)], Default::default()),
        Err(GenerationError::Limit)
    );
    let mut items = vec![message(1, "one"), message(2, "two")];
    let label =
        Text::allowing_empty("x".repeat(MAX_TEXT_BYTES), "synthetic", MAX_TEXT_BYTES).unwrap();
    for (_, item) in &mut items {
        let Item::Message(m) = item else {
            unreachable!()
        };
        m.participant = Some(label.clone());
    }
    assert_eq!(
        GenerationRequest::new(items, Default::default()),
        Err(GenerationError::Limit)
    );
    assert_eq!(source.items()[0].1.participant(), Some(&text("alice")));
}
#[test]
fn retained_calls_cannot_change_their_effective_participant_through_envelopes() {
    use morphiecore::protocol::openai::{Adapter, Dialect, Profile};
    use serde_json::json;
    let adapter = Adapter::new(Profile::Chat, Dialect::Standard, None);
    for custom in [false, true] {
        for (from, to) in [
            (Some("alice"), Some("bob")),
            (Some("alice"), None),
            (None, Some("bob")),
            (None, Some("")),
            (None, None),
            (Some("alice"), Some("alice")),
        ] {
            let call = if custom {
                json!({"id":"c","type":"custom","custom":{"name":"lookup","input":"raw"}})
            } else {
                json!({"id":"c","type":"function","function":{"name":"lookup","arguments":"{}"}})
            };
            let mut wire = json!({"model":"m","messages":[
                {"role":"assistant","content":"Checking","tool_calls":[call]},
                {"role":"tool","tool_call_id":"c","content":"ok"},
                {"role":"assistant","content":"Continue"}]});
            if let Some(name) = from {
                wire["messages"][0]["name"] = json!(name);
            }
            if let Some(name) = to {
                wire["messages"][2]["name"] = json!(name);
            }
            let source = adapter
                .decode_request(wire.to_string().as_bytes())
                .unwrap()
                .task
                .semantic;
            let ids: Vec<_> = source.items().iter().map(|(id, _)| *id).collect();
            assert_eq!(ids.len(), 4);
            if from.is_some() {
                assert_eq!(
                    source.clone().with_message_envelopes(vec![]),
                    Err(GenerationError::InvalidContextTransform)
                );
                source
                    .clone()
                    .with_message_envelopes(source.message_envelopes().to_vec())
                    .unwrap();
            } else if to.is_some() {
                let items = vec![
                    source.items()[3].clone(),
                    source.items()[1].clone(),
                    source.items()[2].clone(),
                ];
                let unassociated =
                    GenerationRequest::new(items.clone(), Default::default()).unwrap();
                let groups = vec![
                    MessageEnvelope::new(
                        GroupId::new(LocalScope::ROOT, 98),
                        MessageEnvelopeRole::Assistant,
                        vec![ids[3], ids[1]],
                    )
                    .unwrap(),
                ];
                assert_eq!(
                    unassociated.with_message_envelopes(groups.clone()),
                    Err(GenerationError::InvalidContextTransform)
                );
                GenerationRequest::from_resources_and_envelopes(
                    items,
                    Default::default(),
                    Default::default(),
                    groups,
                )
                .unwrap();
            }
            let moved = source.clone().transform(vec![
                ContextEdit::Reorder(vec![ids[0], ids[3], ids[1], ids[2]]),
                ContextEdit::MessageEnvelopes(vec![
                    MessageEnvelope::new(
                        GroupId::new(LocalScope::ROOT, 99),
                        MessageEnvelopeRole::Assistant,
                        vec![ids[3], ids[1]],
                    )
                    .unwrap(),
                ]),
            ]);
            if from == to {
                moved.unwrap();
            } else {
                let error = moved.unwrap_err();
                assert_eq!(error.stage, ContextStage::Association);
                assert_eq!(error.error, GenerationError::InvalidContextTransform);
            }
            if from.is_some() {
                assert_eq!(
                    source.clone().with_items(source.items()[1..].to_vec()),
                    Err(GenerationError::InvalidContextTransform)
                );
            }
            assert_eq!(source.items()[0].1.participant().map(Text::as_str), from);
        }
    }
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
        participant: None,
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
