//! Owner-local replay values and trusted bindings, without native Google/Anthropic codecs.
use crate::resources_support as resources;
use morphiecore::{
    lowering::generation::{GenerationRepresentationContract, lower_request},
    protocol::{fidelity::FidelityRecords, openai::Profile},
    semantic::{
        task::generation::*,
        value::{ReplayOrigin, Text},
    },
};
fn text(s: &str) -> Text {
    Text::new(s, "synthetic", MAX_TEXT_BYTES).unwrap()
}
fn token(format: ReplayFormat) -> ReplayValue {
    ReplayValue::final_value(format, text("synthetic-private-token"))
}
fn call() -> Item {
    Item::ToolCall(ToolCall {
        call_id: text("C"),
        name: text("lookup"),
        arguments: "{}".into(),
        status: ItemLifecycle::Completed,
        context: CallContext {
            replay: Some(token(ReplayFormat::GoogleGenerateContentPart)),
            ..Default::default()
        },
    })
}
fn message() -> Item {
    Item::Message(Message {
        role: MessageRole::Assistant,
        status: ItemLifecycle::Completed,
        phase: None,
        parts: vec![
            Part {
                id: PartId::new(1),
                content: ContentPart::Text(text("visible").into()),
                replay: Some(token(ReplayFormat::GoogleGenerateContentPart)),
            },
            Part {
                id: PartId::new(2),
                content: ContentPart::Resource(resources::output(1)),
                replay: Some(token(ReplayFormat::GoogleGenerateContentPart)),
            },
            Part {
                id: PartId::new(3),
                content: ContentPart::Text(text("outside").into()),
                replay: None,
            },
        ],
    })
}
fn request() -> GenerationRequest {
    GenerationRequest::from_resources(
        vec![(ItemId::new(1), message()), (ItemId::new(2), call())],
        GenerationSettings::default(),
        resources::table([(
            1,
            Resource {
                location: ResourceLocation::Url(text("https://example.test/image.png")),
                description: ResourceDescription::Image { detail: None },
            },
        )]),
    )
    .unwrap()
}
#[test]
fn local_attachments_have_one_owner_and_no_existing_public_carrier() {
    let source = request();
    let requirements = GenerationRequirements::derive(&source);
    assert!(requirements.assistant_media);
    assert_eq!(requirements.image_inputs, 0);
    assert_eq!(requirements.resource_count, 1);
    let owners = [
        ReplayOwner::Part {
            item: ItemId::new(1),
            part: PartId::new(1),
        },
        ReplayOwner::Part {
            item: ItemId::new(1),
            part: PartId::new(2),
        },
        ReplayOwner::Item(ItemId::new(2)),
    ];
    let origin = ReplayOrigin::new("synthetic:model-a:version-1").unwrap();
    for owner in owners {
        assert_eq!(
            owner.value(source.items()).unwrap().replay_token(),
            Some("synthetic-private-token")
        );
        let mut fidelity = FidelityRecords::default();
        fidelity
            .record_attachment(owner, &source, origin.clone())
            .unwrap();
        assert!(fidelity.attachment_matches(owner, &source, Some(&origin)));
        assert!(!fidelity.attachment_matches(owner, &source, None));
        assert!(!fidelity.attachment_matches(
            owner,
            &source,
            Some(&ReplayOrigin::new("synthetic:model-a:version-2").unwrap())
        ));
        assert!(!format!("{source:?}{fidelity:?}").contains("synthetic-private-token"));
        for profile in [Profile::Chat, Profile::Responses] {
            assert!(
                lower_request(
                    &source,
                    &fidelity,
                    profile,
                    GenerationRepresentationContract::full()
                )
                .is_err()
            );
        }
        assert!(GenerationSemanticContract::full().check(&source).is_err());
    }
}
#[test]
fn selected_part_binding_ignores_siblings_but_rejects_own_edits_and_resurrection() {
    let source = request();
    let owner = ReplayOwner::Part {
        item: ItemId::new(1),
        part: PartId::new(2),
    };
    let origin = ReplayOrigin::new("synthetic:model-a:version-1").unwrap();
    let mut fidelity = FidelityRecords::default();
    fidelity
        .record_attachment(owner, &source, origin.clone())
        .unwrap();
    let mut outside = source.items().to_vec();
    let Item::Message(m) = &mut outside[0].1 else {
        panic!()
    };
    m.parts[2].content = ContentPart::Text(text("changed sibling").into());
    let outside = source.clone().with_items(outside).unwrap();
    assert!(fidelity.attachment_matches(owner, &outside, Some(&origin)));
    for edit in 0..5 {
        let mut items = source.items().to_vec();
        let mut declarations = source.resources().clone();
        let Item::Message(m) = &mut items[0].1 else {
            panic!()
        };
        match edit {
            0 => {
                let ContentPart::Resource(r) = &mut m.parts[1].content else {
                    panic!()
                };
                let mut value = r.media(source.resources()).unwrap().to_owned();
                value.location = ResourceLocation::Url(text("https://example.test/changed.png"));
                declarations = resources::replace(&declarations, r.id, value);
            }
            1 => {
                m.parts[1].replay = Some(ReplayValue::final_value(
                    ReplayFormat::GoogleGenerateContentPart,
                    text("changed-token"),
                ))
            }
            2 => m.parts[1].replay = None,
            3 => m.parts[1].id = PartId::scoped(LocalScope::new(8), 2),
            _ => {
                m.parts.remove(1);
            }
        }
        let candidate = source
            .clone()
            .with_resources(declarations)
            .unwrap()
            .with_items(items)
            .unwrap();
        assert!(!fidelity.attachment_matches(owner, &candidate, Some(&origin)));
        assert!(
            fidelity
                .clone()
                .record_attachment(owner, &candidate, origin.clone())
                .is_err()
        );
        if edit >= 2 {
            assert!(owner.value(candidate.items()).is_none());
        }
    }
    let proof = RequestDependencyProof::capture(
        &source,
        HistoryDependency::PrefixThrough(ItemId::new(2)),
        SettingsDependency::All,
    )
    .unwrap();
    fidelity
        .bind_attachment_dependency(owner, proof, &source)
        .unwrap();
    assert!(!fidelity.attachment_matches(owner, &outside, Some(&origin)));
}
#[test]
fn formats_are_bound_to_legal_nodes_and_partial_values_are_not_replayable() {
    let mut items = request().items().to_vec();
    let Item::Message(m) = &mut items[0].1 else {
        panic!()
    };
    m.parts[0].replay = Some(token(ReplayFormat::ResponsesEncrypted));
    assert_eq!(
        request().with_items(items).unwrap_err(),
        GenerationError::InvalidReplay
    );
    let mut items = request().items().to_vec();
    let Item::ToolCall(c) = &mut items[1].1 else {
        panic!()
    };
    c.context.replay = Some(ReplayValue::partial(
        ReplayFormat::GoogleGenerateContentPart,
        text("partial"),
    ));
    let source = request().with_items(items).unwrap();
    let mut fidelity = FidelityRecords::default();
    let origin = ReplayOrigin::new("synthetic").unwrap();
    let owner = ReplayOwner::Item(ItemId::new(2));
    assert!(fidelity.record_attachment(owner, &source, origin).is_err());
    for format in [
        ReplayFormat::ResponsesEncrypted,
        ReplayFormat::GoogleInteractionsV1Thought,
        ReplayFormat::AnthropicMessagesThinking,
    ] {
        GenerationResponse::new(
            vec![(
                ItemId::new(1),
                Item::Reasoning(ReasoningItem {
                    parts: vec![],
                    status: ItemLifecycle::Completed,
                    replay: Some(token(format)),
                }),
            )],
            Outcome::Completed,
        )
        .unwrap();
    }
}
#[test]
fn late_part_attachment_is_finalized_before_item_closure_and_matches_static() {
    // The native parser must finish opaque assembly before this canonical event.
    for batches in [vec!["vis", "ible"], vec!["visible"]] {
        let mut state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
        state = reduce(
            state,
            StreamEvent::ItemStarted {
                item: ItemId::new(1),
                kind: ItemKind::Message { phase: None },
                replay: None,
            },
        )
        .unwrap();
        state = reduce(
            state,
            StreamEvent::PartStarted {
                item: ItemId::new(1),
                part: PartId::new(1),
                kind: PartKind::Text,
            },
        )
        .unwrap();
        for fragment in batches {
            state = reduce(
                state,
                StreamEvent::Delta {
                    item: ItemId::new(1),
                    part: PartId::new(1),
                    fragment: fragment.into(),
                    logprobs: vec![],
                },
            )
            .unwrap();
        }
        for event in [
            StreamEvent::ValueFinished {
                item: ItemId::new(1),
                part: PartId::new(1),
            },
            StreamEvent::PartFinished {
                item: ItemId::new(1),
                part: PartId::new(1),
            },
        ] {
            state = reduce(state, event).unwrap();
        }
        let event = StreamEvent::ReplayFinalized {
            owner: ReplayOwner::Part {
                item: ItemId::new(1),
                part: PartId::new(1),
            },
            value: token(ReplayFormat::GoogleGenerateContentPart),
        };
        state = reduce(state, event.clone()).unwrap();
        assert!(reduce(state.clone(), event.clone()).is_err());
        for profile in [Profile::Chat, Profile::Responses] {
            assert!(
                morphiecore::lowering::events::check_event(
                    &state,
                    &event,
                    profile,
                    &GenerationRepresentationContract::full()
                )
                .is_err()
            );
        }
        state = reduce(
            state,
            StreamEvent::ItemFinished {
                item: ItemId::new(1),
                status: ItemLifecycle::Completed,
                replay: None,
            },
        )
        .unwrap();
        assert!(reduce(state.clone(), event).is_err());
        state = reduce(
            state,
            StreamEvent::Terminal {
                terminal: StreamTerminal::Completed,
                details: TerminalDetails::default(),
            },
        )
        .unwrap();
        let mut expected = message();
        let Item::Message(m) = &mut expected else {
            panic!()
        };
        m.parts.truncate(1);
        assert_eq!(
            materialize(&state).unwrap(),
            GenerationResponse::new(vec![(ItemId::new(1), expected)], Outcome::Completed).unwrap()
        );
    }
}

#[test]
fn call_and_provider_step_events_preserve_their_own_attachments() {
    for format in [
        ReplayFormat::GoogleGenerateContentPart,
        ReplayFormat::GoogleInteractionsV1Step,
    ] {
        let mut state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
        state = reduce(
            state,
            StreamEvent::ItemStarted {
                item: ItemId::new(2),
                kind: ItemKind::ToolCall {
                    format: ArgumentFormat::Raw,
                    call_id: text("C"),
                    name: text("lookup"),
                    message: None,
                    context: Default::default(),
                },
                replay: None,
            },
        )
        .unwrap();
        let attachment = StreamEvent::ReplayFinalized {
            owner: ReplayOwner::Item(ItemId::new(2)),
            value: token(format),
        };
        assert!(reduce(state.clone(), attachment.clone()).is_err());
        for event in [
            StreamEvent::PartStarted {
                item: ItemId::new(2),
                part: PartId::new(1),
                kind: PartKind::Arguments,
            },
            StreamEvent::Delta {
                item: ItemId::new(2),
                part: PartId::new(1),
                fragment: "{}".into(),
                logprobs: vec![],
            },
            StreamEvent::ValueFinished {
                item: ItemId::new(2),
                part: PartId::new(1),
            },
            StreamEvent::PartFinished {
                item: ItemId::new(2),
                part: PartId::new(1),
            },
        ] {
            state = reduce(state, event).unwrap();
        }
        assert!(
            reduce(
                state.clone(),
                StreamEvent::ReplayFinalized {
                    owner: ReplayOwner::Item(ItemId::new(2)),
                    value: ReplayValue::partial(format, text("partial")),
                }
            )
            .is_err()
        );
        assert!(
            reduce(
                state.clone(),
                StreamEvent::ReplayFinalized {
                    owner: ReplayOwner::Item(ItemId::new(2)),
                    value: token(ReplayFormat::ResponsesEncrypted),
                }
            )
            .is_err()
        );
        state = reduce(state, attachment).unwrap();
        state = reduce(
            state,
            StreamEvent::ItemFinished {
                item: ItemId::new(2),
                status: ItemLifecycle::Completed,
                replay: None,
            },
        )
        .unwrap();
        state = reduce(
            state,
            StreamEvent::Terminal {
                terminal: StreamTerminal::Completed,
                details: TerminalDetails::default(),
            },
        )
        .unwrap();
        let mut expected = call();
        let Item::ToolCall(c) = &mut expected else {
            panic!()
        };
        c.context.replay = Some(token(format));
        assert_eq!(
            materialize(&state).unwrap(),
            GenerationResponse::new(vec![(ItemId::new(2), expected)], Outcome::Completed).unwrap()
        );
    }
    let mut observed = ProviderToolObservation {
        replay: None,
        source: NativeAliasDomain {
            source: text("interactions-v1"),
            scope: LocalScope::new(8),
        },
        operation: ProviderOperation::Reported {
            tool: text("search"),
            alias: Some(text("S")),
            requester: ProviderRequester::Model,
            action: Some(ProviderAction::Search {
                query: None,
                queries: Some(vec![text("query")]),
            }),
        },
        progress: None,
        execution: None,
        output: None,
        artifact_status: None,
    };
    let mut state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
    state = reduce(
        state,
        StreamEvent::ItemStarted {
            item: ItemId::new(1),
            kind: ItemKind::ProviderTool(observed.clone()),
            replay: None,
        },
    )
    .unwrap();
    state = reduce(
        state,
        StreamEvent::ReplayFinalized {
            owner: ReplayOwner::Item(ItemId::new(1)),
            value: token(ReplayFormat::GoogleInteractionsV1Step),
        },
    )
    .unwrap();
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
            details: TerminalDetails::default(),
        },
    )
    .unwrap();
    observed.replay = Some(token(ReplayFormat::GoogleInteractionsV1Step));
    let expected = GenerationResponse::new(
        vec![(ItemId::new(1), Item::ProviderTool(observed))],
        Outcome::Completed,
    )
    .unwrap();
    assert_eq!(materialize(&state).unwrap(), expected);
    let source =
        GenerationRequest::new(expected.items().to_vec(), GenerationControls::default()).unwrap();
    let origin = ReplayOrigin::new("synthetic:step:v1").unwrap();
    let mut fidelity = FidelityRecords::default();
    fidelity
        .record_attachment(ReplayOwner::Item(ItemId::new(1)), &source, origin.clone())
        .unwrap();
    assert!(fidelity.attachment_matches(ReplayOwner::Item(ItemId::new(1)), &source, Some(&origin)));
    let mut changed = source.items().to_vec();
    let Item::ProviderTool(p) = &mut changed[0].1 else {
        panic!()
    };
    p.replay = Some(ReplayValue::final_value(
        ReplayFormat::GoogleInteractionsV1Step,
        text("changed"),
    ));
    let changed = source.with_items(changed).unwrap();
    assert!(!fidelity.attachment_matches(
        ReplayOwner::Item(ItemId::new(1)),
        &changed,
        Some(&origin)
    ));
}

#[test]
fn shared_group_and_settings_dependencies_are_checked_without_cross_candidate_pollution() {
    let a = GroupId::new(LocalScope::ROOT, 1);
    let b = GroupId::new(LocalScope::ROOT, 2);
    let source = request()
        .with_replay_groups(vec![
            ReplayGroup::new(a, vec![ItemId::new(1), ItemId::new(2)]).unwrap(),
            ReplayGroup::new(b, vec![ItemId::new(1)]).unwrap(),
        ])
        .unwrap();
    let call = ReplayOwner::Item(ItemId::new(2));
    let media = ReplayOwner::Part {
        item: ItemId::new(1),
        part: PartId::new(2),
    };
    let origin = ReplayOrigin::new("synthetic").unwrap();
    let mut fidelity = FidelityRecords::default();
    for (owner, group, field) in [
        (call, a, SettingsField::Tools),
        (media, b, SettingsField::Text),
    ] {
        fidelity
            .record_attachment(owner, &source, origin.clone())
            .unwrap();
        fidelity
            .bind_attachment_dependency(
                owner,
                RequestDependencyProof::capture(
                    &source,
                    HistoryDependency::ReplayGroup(group),
                    SettingsDependency::only(field),
                )
                .unwrap(),
                &source,
            )
            .unwrap();
        assert!(fidelity.attachment_matches(owner, &source, Some(&origin)));
    }
    let mut settings = source.settings().clone();
    settings.controls.max_output_tokens = Some(100);
    let outside = source.clone().with_settings(settings).unwrap();
    assert!(fidelity.attachment_matches(call, &outside, Some(&origin)));
    assert!(fidelity.attachment_matches(media, &outside, Some(&origin)));
    let mut settings = source.settings().clone();
    settings.tools = Some(vec![]);
    let tools = source.clone().with_settings(settings).unwrap();
    assert!(!fidelity.attachment_matches(call, &tools, Some(&origin)));
    assert!(fidelity.attachment_matches(media, &tools, Some(&origin)));
    let mut settings = source.settings().clone();
    settings.text.presence = true;
    settings.text.format =
        morphiecore::semantic::value::Presence::Value(OutputConstraint::JsonObject);
    let schema = source.clone().with_settings(settings).unwrap();
    assert!(fidelity.attachment_matches(call, &schema, Some(&origin)));
    assert!(!fidelity.attachment_matches(media, &schema, Some(&origin)));
    let mut items = source.items().to_vec();
    let Item::Message(m) = &mut items[0].1 else {
        panic!()
    };
    m.parts[2].content = ContentPart::Text(text("changed shared member").into());
    let changed = source.clone().with_items(items).unwrap();
    assert!(!fidelity.attachment_matches(call, &changed, Some(&origin)));
    assert!(!fidelity.attachment_matches(media, &changed, Some(&origin)));
    assert!(fidelity.attachment_matches(call, &source, Some(&origin)));
    let no_groups = source.clone().with_replay_groups(vec![]).unwrap();
    assert!(!fidelity.attachment_matches(call, &no_groups, Some(&origin)));
    assert!(!fidelity.attachment_matches(media, &no_groups, Some(&origin)));
    assert!(
        fidelity
            .bind_attachment_dependency(
                call,
                RequestDependencyProof::capture(
                    &changed,
                    HistoryDependency::Owners(vec![ItemId::new(2)]),
                    SettingsDependency::None,
                )
                .unwrap(),
                &changed
            )
            .is_err()
    );
}

#[test]
fn attachment_budgets_count_each_value_and_the_aggregate_in_static_and_events() {
    for size in [0, MAX_TEXT_BYTES - 1, MAX_TEXT_BYTES, MAX_TEXT_BYTES + 1] {
        let value = ReplayValue::final_value(
            ReplayFormat::GoogleGenerateContentPart,
            Text::allowing_empty("x".repeat(size), "synthetic", MAX_TEXT_BYTES + 1).unwrap(),
        );
        let mut call = call();
        let Item::ToolCall(c) = &mut call else {
            panic!()
        };
        c.context.replay = Some(value);
        assert_eq!(
            GenerationRequest::new(vec![(ItemId::new(1), call)], GenerationControls::default())
                .is_ok(),
            size > 0 && size <= MAX_TEXT_BYTES
        );
    }
    for last in [9, 10, 11] {
        let sizes = [
            MAX_TEXT_BYTES,
            MAX_TEXT_BYTES,
            MAX_TEXT_BYTES,
            MAX_TEXT_BYTES - 10,
            last,
        ];
        let mut parts = vec![];
        let mut state = Ok(reduce(StreamState::new(), StreamEvent::Started).unwrap());
        state = state.and_then(|s| {
            reduce(
                s,
                StreamEvent::ItemStarted {
                    item: ItemId::new(1),
                    kind: ItemKind::Message { phase: None },
                    replay: None,
                },
            )
        });
        for (i, size) in sizes.into_iter().enumerate() {
            let id = PartId::new(i as u64);
            let value = ReplayValue::final_value(
                ReplayFormat::GoogleGenerateContentPart,
                text(&"x".repeat(size)),
            );
            parts.push(Part {
                id,
                content: ContentPart::Text(
                    Text::allowing_empty("", "synthetic", MAX_TEXT_BYTES)
                        .unwrap()
                        .into(),
                ),
                replay: Some(value.clone()),
            });
            for event in [
                StreamEvent::PartStarted {
                    item: ItemId::new(1),
                    part: id,
                    kind: PartKind::Text,
                },
                StreamEvent::ValueFinished {
                    item: ItemId::new(1),
                    part: id,
                },
                StreamEvent::PartFinished {
                    item: ItemId::new(1),
                    part: id,
                },
                StreamEvent::ReplayFinalized {
                    owner: ReplayOwner::Part {
                        item: ItemId::new(1),
                        part: id,
                    },
                    value,
                },
            ] {
                state = state.and_then(|s| reduce(s, event));
            }
        }
        let response = GenerationResponse::new(
            vec![(
                ItemId::new(1),
                Item::Message(Message {
                    role: MessageRole::Assistant,
                    status: ItemLifecycle::Completed,
                    phase: None,
                    parts,
                }),
            )],
            Outcome::Completed,
        );
        assert_eq!(response.is_ok(), last <= 10);
        assert_eq!(state.is_ok(), last <= 10);
    }
}

#[test]
fn cleared_call_attachments_do_not_return_from_fidelity() {
    // A retained binding has no payload from which to reconstruct a deleted value.
    use morphiecore::protocol::openai::responses;
    let source = GenerationRequest::new(
        vec![(ItemId::new(1), call())],
        GenerationControls::default(),
    )
    .unwrap();
    let mut fidelity = FidelityRecords::default();
    let origin = ReplayOrigin::new("synthetic").unwrap();
    fidelity
        .record_attachment(ReplayOwner::Item(ItemId::new(1)), &source, origin)
        .unwrap();
    let mut items = source.items().to_vec();
    let Item::ToolCall(c) = &mut items[0].1 else {
        panic!()
    };
    c.context.replay = None;
    let cleared = source.with_items(items).unwrap();
    let output = responses::encode_generation(
        &lower_request(
            &cleared,
            &fidelity,
            Profile::Responses,
            GenerationRepresentationContract::full(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(!output.to_string().contains("synthetic-private-token"));
}

#[test]
fn attachment_binding_budget_is_shared_with_reasoning_bindings() {
    let parts = (0..MAX_ITEMS)
        .map(|n| Part {
            id: PartId::new(n as u64),
            content: ContentPart::Text(text("x").into()),
            replay: Some(token(ReplayFormat::GoogleGenerateContentPart)),
        })
        .collect();
    let source = GenerationRequest::new(
        vec![(
            ItemId::new(1),
            Item::Message(Message {
                role: MessageRole::Assistant,
                status: ItemLifecycle::Completed,
                phase: None,
                parts,
            }),
        )],
        GenerationControls::default(),
    )
    .unwrap();
    let origin = ReplayOrigin::new("synthetic").unwrap();
    let mut fidelity = FidelityRecords::default();
    for n in 0..MAX_ITEMS {
        let owner = ReplayOwner::Part {
            item: ItemId::new(1),
            part: PartId::new(n as u64),
        };
        fidelity
            .record_attachment(owner, &source, origin.clone())
            .unwrap();
        assert!(fidelity.attachment_matches(owner, &source, Some(&origin)));
    }
    let reasoning = ReasoningItem {
        parts: vec![],
        status: ItemLifecycle::Completed,
        replay: Some(token(ReplayFormat::ResponsesEncrypted)),
    };
    assert!(
        fidelity
            .record_replay(ItemId::new(9), &reasoning, Some(origin.clone()))
            .is_err()
    );
    let owner = ReplayOwner::Part {
        item: ItemId::new(1),
        part: PartId::new(0),
    };
    fidelity
        .record_attachment(owner, &source, origin.clone())
        .unwrap();
    fidelity.retain_owners(&[]);
    assert!(!fidelity.attachment_matches(owner, &source, Some(&origin)));
}
