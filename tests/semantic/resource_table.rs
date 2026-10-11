//! Resource identity and declared conditions are independent of locators and I/O.
use morphiecore::semantic::{task::generation::*, value::Text};
fn text(value: &str) -> Text {
    Text::new(value, "synthetic", MAX_TEXT_BYTES).unwrap()
}
fn declaration() -> ResourceDeclaration {
    ResourceDeclaration {
        body: ResourceBody::Media(ResourceLocation::Url(text(
            "https://example.test/private.png",
        ))),
        conditions: ResourceConditions::default(),
    }
}

#[test]
fn provider_resource_declarations_materialize_once_and_cannot_arrive_after_closure() {
    let id = ResourceId::new(1);
    let observed = ProviderToolObservation {
        replay: None,
        source: NativeAliasDomain {
            source: text("synthetic"),
            scope: LocalScope::ROOT,
        },
        operation: ProviderOperation::Reported {
            tool: text("search"),
            alias: None,
            requester: ProviderRequester::Unreported,
            action: None,
        },
        progress: None,
        execution: None,
        artifact_status: Some(ItemLifecycle::Completed),
        output: Some(ToolOutput::Parts(vec![
            (
                PartId::new(1),
                ToolResultPart::Resource(ResourceUse {
                    id,
                    purpose: ResourcePurpose::ToolResult,
                    description: ResourceDescription::Image { detail: None },
                }),
            ),
            (
                PartId::new(2),
                ToolResultPart::Resource(ResourceUse {
                    id,
                    purpose: ResourcePurpose::ToolResult,
                    description: ResourceDescription::Image { detail: None },
                }),
            ),
        ])),
    };
    let opened = reduce(StreamState::new(), StreamEvent::Started).unwrap();
    let item = StreamEvent::ItemStarted {
        item: ItemId::new(1),
        kind: ItemKind::ProviderTool(observed.clone()),
        replay: None,
    };
    assert!(reduce(opened.clone(), item.clone()).is_err());
    let declared = StreamEvent::ResourceDeclared {
        id,
        resource: declaration(),
    };
    let mut state = reduce(opened, declared.clone()).unwrap();
    assert!(reduce(state.clone(), declared.clone()).is_err());
    state = reduce(state, item).unwrap();
    state = reduce(
        state,
        StreamEvent::ItemFinished {
            item: ItemId::new(1),
            status: ItemLifecycle::Completed,
            replay: None,
        },
    )
    .unwrap();
    state = reduce(
        state,
        StreamEvent::Terminal {
            terminal: StreamTerminal::Completed,
            details: Default::default(),
        },
    )
    .unwrap();
    assert!(
        reduce(
            state.clone(),
            StreamEvent::ResourceDeclared {
                id: ResourceId::new(2),
                resource: declaration(),
            }
        )
        .is_err()
    );
    let expected = GenerationResponse::from_resources(
        vec![(ItemId::new(1), Item::ProviderTool(observed))],
        Outcome::Completed,
        ResourceTable::new(vec![(id, declaration())]).unwrap(),
    )
    .unwrap();
    assert_eq!(materialize(&state).unwrap(), expected);
    assert_eq!(expected.resources().len(), 1);
}

#[test]
fn resource_edits_preserve_original_usage_and_do_not_report_new_measurements() {
    let id = ResourceId::new(1);
    let response = GenerationResponse::from_resources(
        vec![(
            ItemId::new(1),
            Item::Message(Message {
                role: MessageRole::Assistant,
                status: ItemLifecycle::Completed,
                phase: None,
                parts: vec![Part {
                    id: PartId::new(1),
                    replay: None,
                    content: ContentPart::Resource(ResourceUse {
                        id,
                        purpose: ResourcePurpose::Output,
                        description: ResourceDescription::Image { detail: None },
                    }),
                }],
            }),
        )],
        Outcome::Completed,
        ResourceTable::new(vec![(id, declaration())]).unwrap(),
    )
    .unwrap()
    .with_usage(Usage::operation(3, 5, 8))
    .unwrap();
    let mut changed = declaration();
    changed.body = ResourceBody::Media(ResourceLocation::Url(text(
        "https://example.test/edited.png",
    )));
    let edited = response
        .clone()
        .with_resources(ResourceTable::new(vec![(id, changed)]).unwrap())
        .unwrap();
    assert_eq!(edited.usage_reports(), response.usage_reports());
    assert_eq!(edited.usage().unwrap().output_tokens, Some(5));
    let selected = ClientManaged::new(GenerationSettings::default())
        .unwrap()
        .select_response(&edited)
        .unwrap()
        .build(&[])
        .unwrap();
    assert_eq!(selected.resources(), edited.resources());
    assert_eq!(response.resources().get(id), Some(&declaration()));
}
#[test]
fn resource_table_preserves_distinct_identity_and_rejects_conflicting_declarations() {
    let a = ResourceId::scoped(LocalScope::new(1), 1);
    let b = ResourceId::scoped(LocalScope::new(2), 1);
    let table = ResourceTable::new(vec![(a, declaration()), (b, declaration())]).unwrap();
    assert_eq!(table.len(), 2);
    assert_eq!(table.get(a), table.get(b));
    assert!(table.get(ResourceId::new(9)).is_none());
    assert!(ResourceTable::new(vec![(a, declaration()), (a, declaration())]).is_err());
    let same = ResourceTable::new(vec![(a, declaration())]).unwrap();
    assert_eq!(table.clone().merge(&same).unwrap(), table);
    let mut changed = declaration();
    changed.body = ResourceBody::Media(ResourceLocation::Url(text(
        "https://example.test/replaced.png",
    )));
    assert!(
        table
            .merge(&ResourceTable::new(vec![(a, changed)]).unwrap())
            .is_err()
    );
}
#[test]
fn explicit_resource_conditions_require_matching_scope_access_purpose_and_time() {
    let mut source = declaration();
    source.conditions = ResourceConditions {
        scope: Some(text("synthetic-scope")),
        access: Some(text("synthetic-read-condition")),
        expires_at: Some(100),
        purposes: Some(vec![ResourcePurpose::Input]),
    };
    let target = ResourceTarget {
        scope: Some("synthetic-scope"),
        access: Some("synthetic-read-condition"),
        now: Some(99),
    };
    source.check_target(ResourcePurpose::Input, target).unwrap();
    for target in [
        ResourceTarget {
            scope: None,
            ..target
        },
        ResourceTarget {
            scope: Some("different"),
            ..target
        },
        ResourceTarget {
            access: None,
            ..target
        },
        ResourceTarget {
            access: Some("different"),
            ..target
        },
        ResourceTarget {
            now: None,
            ..target
        },
        ResourceTarget {
            now: Some(100),
            ..target
        },
    ] {
        assert!(source.check_target(ResourcePurpose::Input, target).is_err());
    }
    assert!(
        source
            .check_target(ResourcePurpose::Output, target)
            .is_err()
    );
    assert!(!format!("{source:?}").contains("private.png"));
    assert!(!format!("{source:?}").contains("synthetic-read-condition"));
}
#[test]
fn resource_proof_binds_body_conditions_and_identity_not_unrelated_entries() {
    let id = ResourceId::new(1);
    let source = ResourceTable::new(vec![(id, declaration())]).unwrap();
    let proof = ResourceDependency::capture(&source, id).unwrap();
    proof.check(&source).unwrap();
    let other = source
        .clone()
        .merge(&ResourceTable::new(vec![(ResourceId::new(2), declaration())]).unwrap())
        .unwrap();
    proof.check(&other).unwrap();
    let mut changed = declaration();
    changed.conditions.expires_at = Some(1);
    assert!(
        proof
            .check(&ResourceTable::new(vec![(id, changed)]).unwrap())
            .is_err()
    );
    assert!(
        proof
            .check(
                &ResourceTable::new(vec![(
                    ResourceId::scoped(LocalScope::new(2), 1),
                    declaration()
                )])
                .unwrap()
            )
            .is_err()
    );
    assert!(proof.check(&ResourceTable::default()).is_err());
    assert!(source.select(&[ResourceId::new(2)]).is_err());
    assert!(source.select(&[]).unwrap().is_empty());
}

#[test]
fn resource_table_bounds_count_and_combined_declarations() {
    for count in [MAX_ITEMS - 1, MAX_ITEMS, MAX_ITEMS + 1] {
        let entries = (0..count)
            .map(|n| (ResourceId::new(n as u64), declaration()))
            .collect();
        assert_eq!(ResourceTable::new(entries).is_ok(), count <= MAX_ITEMS);
    }
    let overhead =
        4 * (std::mem::size_of::<ResourceId>() + std::mem::size_of::<ResourceConditions>());
    for excess in [-1isize, 0, 1] {
        let mut entries = vec![];
        for n in 0..4 {
            let bytes = if n == 3 {
                (MAX_TEXT_BYTES - overhead)
                    .checked_add_signed(excess)
                    .unwrap()
            } else {
                MAX_TEXT_BYTES
            };
            entries.push((
                ResourceId::new(n),
                ResourceDeclaration {
                    body: ResourceBody::Text(text(&"x".repeat(bytes))),
                    conditions: Default::default(),
                },
            ));
        }
        assert_eq!(ResourceTable::new(entries).is_ok(), excess <= 0);
    }
}

#[test]
fn opaque_reference_without_declared_scope_is_reportable_but_not_dereferenceable() {
    let mut source = declaration();
    source.body = ResourceBody::Media(ResourceLocation::OpaqueReference(text("synthetic-file")));
    ResourceTable::new(vec![(ResourceId::new(1), source.clone())]).unwrap();
    source
        .check_target(ResourcePurpose::Reference, ResourceTarget::default())
        .unwrap();
    assert!(
        source
            .check_target(ResourcePurpose::Input, ResourceTarget::default())
            .is_err()
    );
    assert!(
        source
            .check_target(
                ResourcePurpose::Input,
                ResourceTarget {
                    scope: Some("invented-target-scope"),
                    ..Default::default()
                }
            )
            .is_err()
    );
    source.conditions.scope = Some(text("declared-scope"));
    source
        .check_target(
            ResourcePurpose::Input,
            ResourceTarget {
                scope: Some("declared-scope"),
                ..Default::default()
            },
        )
        .unwrap();
    source.conditions.purposes = Some(vec![]);
    assert!(
        source
            .check_target(
                ResourcePurpose::Input,
                ResourceTarget {
                    scope: Some("declared-scope"),
                    ..Default::default()
                }
            )
            .is_err()
    );
}
