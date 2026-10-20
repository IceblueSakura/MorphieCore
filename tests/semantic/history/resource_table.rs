//! Resource identity is independent of locators and I/O.
use morphiecore::semantic::{task::generation::*, value::Text};
fn text(value: &str) -> Text {
    Text::new(value, "synthetic", MAX_TEXT_BYTES).unwrap()
}
fn declaration() -> ResourceLocation {
    ResourceLocation::Url(text("https://example.test/private.png"))
}

#[test]
fn provider_resource_declarations_materialize_once_and_cannot_arrive_after_closure() {
    let id = ResourceId::new(1);
    let observed = ProviderToolObservation {
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
            Part::new(
                PartId::new(1),
                InputPart::Resource(ResourceUse {
                    id,
                    description: ResourceDescription::Image { detail: None },
                }),
            ),
            Part::new(
                PartId::new(2),
                InputPart::Resource(ResourceUse {
                    id,
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
            // Responses carry media through observations, not assistant input uses.
            Item::ProviderTool(ProviderToolObservation {
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
                output: Some(ToolOutput::Parts(vec![Part::new(
                    PartId::new(1),
                    InputPart::Resource(ResourceUse {
                        id,
                        description: ResourceDescription::Image { detail: None },
                    }),
                )])),
            }),
        )],
        Outcome::Completed,
        ResourceTable::new(vec![(id, declaration())]).unwrap(),
    )
    .unwrap()
    .with_usage(Some(Usage::operation(3, 5, 8)))
    .unwrap();
    // Input media belong to user turns; generated media need their own owner.
    assert_eq!(
        GenerationResponse::from_resources(
            vec![(
                ItemId::new(1),
                Item::Message(Message {
                    role: MessageRole::Assistant,
                    status: ItemLifecycle::Completed,
                    phase: None,
                    parts: vec![Part::new(
                        PartId::new(1),
                        ContentPart::Resource(ResourceUse {
                            id,
                            description: ResourceDescription::Image { detail: None },
                        }),
                    )],
                }),
            )],
            Outcome::Completed,
            ResourceTable::new(vec![(id, declaration())]).unwrap(),
        ),
        Err(GenerationError::InvalidResource)
    );
    let changed = ResourceLocation::Url(text("https://example.test/edited.png"));
    let edited = response
        .clone()
        .with_resources(ResourceTable::new(vec![(id, changed)]).unwrap())
        .unwrap();
    assert_eq!(edited.usage(), response.usage());
    assert_eq!(edited.usage().unwrap().output_tokens, Some(5));
    let selected = ClientManaged::new(GenerationSettings::default())
        .unwrap()
        .select_response(&edited)
        .unwrap()
        .build()
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
    assert!(!format!("{:?}", declaration()).contains("private.png"));
    assert!(ResourceTable::new(vec![(a, declaration()), (a, declaration())]).is_err());
    let same = ResourceTable::new(vec![(a, declaration())]).unwrap();
    assert_eq!(table.clone().merge(&same).unwrap(), table);
    let changed = ResourceLocation::Url(text("https://example.test/replaced.png"));
    assert!(
        table
            .merge(&ResourceTable::new(vec![(a, changed)]).unwrap())
            .is_err()
    );
}
