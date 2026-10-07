//! Explicit context transformations and bounded diagnostics, never automatic compaction.
use morphiecore::semantic::{
    task::generation::*,
    value::{Presence, Text},
};

fn text(s: &str) -> Text {
    Text::new(s, "synthetic", MAX_TEXT_BYTES).unwrap()
}
fn message(id: u64, s: &str) -> (ItemId, Item) {
    (
        ItemId::new(id),
        Item::Message(Message {
            role: MessageRole::User,
            parts: vec![Part {
                id: PartId::new(id),
                content: ContentPart::Text(text(s).into()),
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
        .transform(
            vec![
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
            ],
            &[],
        )
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
            .transform(vec![ContextEdit::Reorder(vec![ItemId::new(1)])], &[])
            .is_err()
    );
    assert!(
        source
            .transform(vec![ContextEdit::Delete(vec![ItemId::new(99)])], &[])
            .is_err()
    );
}
#[test]
fn external_summary_is_new_user_content_with_explicit_sources_and_no_old_authority() {
    let source = source();
    let branch = source
        .clone()
        .transform(
            vec![ContextEdit::Summary {
                sources: vec![ItemId::new(1), ItemId::new(2)],
                owner: ItemId::new(8),
                part: PartId::new(8),
                text: text("external summary"),
            }],
            &[],
        )
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
        parts: vec![(PartId::new(1), text("protected"))],
        status: None,
    });
    let protected = GenerationRequest::new(items, GenerationControls::default()).unwrap();
    let result = protected.transform(
        vec![ContextEdit::Summary {
            sources: vec![ItemId::new(1)],
            owner: ItemId::new(9),
            part: PartId::new(9),
            text: text("untrusted summary"),
        }],
        &[],
    );
    assert_eq!(result.unwrap_err().stage, ContextStage::Edit);
}
#[test]
fn transforms_protect_instruction_placement_authority_and_assistant_phase() {
    let instruction = (
        ItemId::new(1),
        Item::Instruction(Instruction {
            authority: InstructionAuthority::Developer,
            parts: vec![(PartId::new(1), text("do not execute"))],
            status: None,
        }),
    );
    let source = GenerationRequest::new(
        vec![instruction, message(2, "input")],
        GenerationControls::default(),
    )
    .unwrap();
    assert!(
        source
            .clone()
            .transform(
                vec![ContextEdit::Reorder(vec![ItemId::new(2), ItemId::new(1)])],
                &[]
            )
            .is_err()
    );
    let mut elevated = source.items()[0].1.clone();
    let Item::Instruction(value) = &mut elevated else {
        panic!()
    };
    value.authority = InstructionAuthority::System;
    assert!(
        source
            .transform(
                vec![ContextEdit::Replace {
                    owner: ItemId::new(1),
                    item: elevated
                }],
                &[]
            )
            .is_err()
    );
    let mut phased = message(2, "commentary");
    let Item::Message(value) = &mut phased.1 else {
        panic!()
    };
    value.role = MessageRole::Assistant;
    value.phase = Some(Phase::Commentary);
    let source =
        GenerationRequest::new(vec![phased.clone()], GenerationControls::default()).unwrap();
    let Item::Message(value) = &mut phased.1 else {
        panic!()
    };
    value.phase = Some(Phase::FinalAnswer);
    assert!(
        source
            .transform(
                vec![ContextEdit::Replace {
                    owner: phased.0,
                    item: phased.1
                }],
                &[]
            )
            .is_err()
    );
}
#[test]
fn selection_and_dependency_failures_have_distinct_content_free_stages() {
    let source = source();
    let selected = ClientManaged::new(GenerationSettings::default())
        .unwrap()
        .select_request_items(&source, &[ItemId::new(1), ItemId::new(3)])
        .unwrap()
        .finish(&[])
        .unwrap();
    assert_eq!(
        selected
            .request()
            .items()
            .iter()
            .map(|(id, _)| id.get())
            .collect::<Vec<_>>(),
        vec![1, 3]
    );
    assert_eq!(
        selected.selections()[0].owners,
        vec![ItemId::new(1), ItemId::new(3)]
    );
    let bad = ClientManaged::new(GenerationSettings::default())
        .unwrap()
        .select_request_items(&source, &[ItemId::new(99)])
        .unwrap_err();
    assert_eq!(bad.stage, ContextStage::Selection);
    let proof = RequestDependencyProof::capture(
        &source,
        HistoryDependency::Owners(vec![ItemId::new(1)]),
        SettingsDependency::None,
    )
    .unwrap();
    let failed = source
        .clone()
        .transform(
            vec![ContextEdit::Replace {
                owner: ItemId::new(1),
                item: message(1, "changed").1,
            }],
            &[proof],
        )
        .unwrap_err();
    assert_eq!(failed.stage, ContextStage::Dependency);
    assert_eq!(failed.index, Some(0));
    let unsupported = source
        .transform(
            vec![ContextEdit::Delete(vec![ItemId::new(1)]); MAX_ITEMS + 1],
            &[],
        )
        .unwrap_err();
    assert_eq!(unsupported.error, GenerationError::Limit);
}
#[test]
fn only_explicit_new_configuration_can_change_a_versioned_context() {
    let config = ConfigurationSnapshot::new(
        ConfigurationId::new(LocalScope::ROOT, 1),
        GenerationSettings::default(),
    )
    .unwrap();
    let source = source().with_configuration(config).unwrap();
    let settings = GenerationSettings {
        instructions: Presence::Value(text("new current instruction")),
        ..Default::default()
    };
    let edited = source
        .transform(
            vec![ContextEdit::Configuration(
                ConfigurationSnapshot::new(ConfigurationId::new(LocalScope::ROOT, 2), settings)
                    .unwrap(),
            )],
            &[],
        )
        .unwrap();
    assert_eq!(edited.request().configuration_revision().unwrap().get(), 2);
    assert_eq!(edited.changes().len(), 1);
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
    let mut usage = Usage::operation(10, 20, 30);
    usage.scope = UsageScope::Item(ItemId::new(2));
    let response = GenerationResponse::new(items, Outcome::Completed)
        .unwrap()
        .with_usage(usage)
        .unwrap()
        .with_progress(InteractionProgress::NeedsContinuation)
        .unwrap();
    let selected = ClientManaged::new(GenerationSettings::default())
        .unwrap()
        .select_response_items(&response, &[ItemId::new(1)])
        .unwrap()
        .finish(&[])
        .unwrap();
    assert_eq!(selected.request().items().len(), 1);
    assert_eq!(selected.provider_continuations().count(), 0);
    assert_eq!(
        selected.selections()[0].progress,
        Some(InteractionProgress::NeedsContinuation)
    );
    assert_eq!(response.usage_reports(), &[usage]);
    assert_eq!(response.items().len(), 2);
}

#[test]
fn summary_cannot_reuse_removed_part_identity_or_cross_an_instruction_boundary() {
    let reused = source().transform(
        vec![ContextEdit::Summary {
            sources: vec![ItemId::new(1)],
            owner: ItemId::new(8),
            part: PartId::new(1),
            text: text("new summary"),
        }],
        &[],
    );
    assert!(reused.is_err());
    let instruction = (
        ItemId::new(9),
        Item::Instruction(Instruction {
            authority: InstructionAuthority::Developer,
            parts: vec![(PartId::new(9), text("changed scope"))],
            status: None,
        }),
    );
    let source = GenerationRequest::new(
        vec![message(1, "before"), instruction, message(2, "after")],
        GenerationControls::default(),
    )
    .unwrap();
    assert!(
        source
            .transform(
                vec![ContextEdit::Summary {
                    sources: vec![ItemId::new(1), ItemId::new(2)],
                    owner: ItemId::new(8),
                    part: PartId::new(8),
                    text: text("merged across scopes"),
                }],
                &[]
            )
            .is_err()
    );
}

#[test]
fn revision_dependency_binds_contents_not_a_forgeable_local_label() {
    let revision = ConfigurationId::new(LocalScope::ROOT, 1);
    let source = source()
        .with_configuration(
            ConfigurationSnapshot::new(revision, GenerationSettings::default()).unwrap(),
        )
        .unwrap();
    let proof = RequestDependencyProof::capture(
        &source,
        HistoryDependency::Owners(vec![ItemId::new(1)]),
        SettingsDependency::only(SettingsField::ConfigurationRevision),
    )
    .unwrap();
    let changed = GenerationRequest::new(source.items().to_vec(), GenerationControls::default())
        .unwrap()
        .with_configuration(
            ConfigurationSnapshot::new(
                revision,
                GenerationSettings {
                    instructions: Presence::Value(text("different contents under same label")),
                    ..Default::default()
                },
            )
            .unwrap(),
        )
        .unwrap();
    assert!(proof.check(&changed).is_err());
}

#[test]
fn selection_receipts_are_bounded_even_for_empty_reported_outputs() {
    let response = GenerationResponse::new(vec![], Outcome::Completed).unwrap();
    for count in [MAX_ITEMS - 1, MAX_ITEMS, MAX_ITEMS + 1] {
        let mut context = ClientManaged::new(GenerationSettings {
            instructions: Presence::Value(text("hello")),
            ..Default::default()
        });
        for _ in 0..count {
            context = context.and_then(|value| value.select_response(&response));
        }
        if count > MAX_ITEMS {
            assert_eq!(context.unwrap_err(), GenerationError::Limit);
        } else {
            let output = context.unwrap().finish(&[]).unwrap();
            assert_eq!(output.selections().len(), count);
            assert_eq!(output.client_results(), Continuation::Unreported);
        }
    }
}

#[test]
fn replacing_item_kind_cannot_bypass_phase_or_owner_protection() {
    let mut item = message(1, "commentary");
    let Item::Message(value) = &mut item.1 else {
        panic!()
    };
    value.role = MessageRole::Assistant;
    value.phase = Some(Phase::Commentary);
    let source = GenerationRequest::new(vec![item], GenerationControls::default()).unwrap();
    let replacement = Item::Reasoning(ReasoningItem {
        parts: vec![(PartId::new(2), ReasoningContent::Text(text("rewritten")))],
        status: ItemLifecycle::Completed,
        replay: None,
    });
    assert!(
        source
            .clone()
            .with_items(vec![(ItemId::new(1), replacement.clone())])
            .is_err()
    );
    assert!(
        source
            .transform(
                vec![ContextEdit::Replace {
                    owner: ItemId::new(1),
                    item: replacement,
                }],
                &[]
            )
            .is_err()
    );
}

#[test]
fn atomic_call_revision_never_retargets_results_and_relations_have_one_final_owner() {
    let call = |alias| {
        Item::ToolCall(ToolCall {
            call_id: text(alias),
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
                    call_id: text("C"),
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
    .with_message_owners(vec![(ItemId::new(1), ItemId::new(9))])
    .unwrap();
    let revision = ContextEdit::ReviseCall {
        source: ItemId::new(1),
        replacement: (ItemId::new(3), call("D")),
    };
    let failed = source
        .clone()
        .transform(vec![revision.clone()], &[])
        .unwrap_err();
    assert_eq!(failed.stage, ContextStage::Association);
    let edited = source
        .clone()
        .transform(
            vec![
                revision,
                ContextEdit::Delete(vec![ItemId::new(2)]),
                ContextEdit::ReplayGroups(vec![
                    ReplayGroup::new(
                        GroupId::new(LocalScope::ROOT, 1),
                        vec![ItemId::new(9), ItemId::new(3)],
                    )
                    .unwrap(),
                ]),
                ContextEdit::MessageOwners(vec![(ItemId::new(3), ItemId::new(9))]),
            ],
            &[],
        )
        .unwrap();
    assert_eq!(
        edited.request().call_derivations().get(&ItemId::new(3)),
        Some(&ItemId::new(1))
    );
    assert_eq!(edited.request().message_owners().len(), 1);
    assert_eq!(
        edited.request().message_owners().get(&ItemId::new(3)),
        Some(&ItemId::new(9))
    );
    assert_eq!(edited.request().items().len(), 2);
    assert_eq!(source.items().len(), 3);
}
