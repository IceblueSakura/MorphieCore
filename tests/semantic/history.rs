//! Client-managed history and explicit edits.
mod client_managed {
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
            is_error: None,
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
                        role: MessageRole::Assistant,
                        parts: vec![Part {
                            id: PartId::new(5),
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
        let r2 =
            GenerationResponse::new(vec![(ItemId::new(4), provider(true))], Outcome::Completed)
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
}
mod client_managed_composition {
    //! Combined pure history consumption; source families are evidence, not native codecs.
    use morphiecore::semantic::{
        task::generation::*,
        value::{Presence, Text},
    };

    fn text(value: &str) -> Text {
        Text::new(value, "synthetic", MAX_TEXT_BYTES).unwrap()
    }
    fn owner(response: u64, item: u64) -> ItemId {
        ItemId::scoped(LocalScope::new(response), item)
    }
    fn message(id: u64, value: &str) -> (ItemId, Item) {
        (
            ItemId::new(id),
            Item::Message(Message {
                role: MessageRole::User,
                parts: vec![Part {
                    id: PartId::new(id),
                    content: ContentPart::Text(text(value).into()),
                }],
                status: ItemLifecycle::Completed,
                phase: None,
            }),
        )
    }

    struct Sample {
        settings: GenerationSettings,
        r1: GenerationResponse,
        r2: GenerationResponse,
        client_result: (ItemId, Item),
    }
    impl Sample {
        fn new(family: &str) -> Self {
            // Provider action/controls/accounting evidence is fixed in upstream-sync.md.
            let mut reasoning = ReasoningRequest::present(Some(ReasoningEffort::High), None);
            let mut usage = Usage::operation(4106, 20, 4126);
            usage.cached_input_tokens = Some(4096);
            let action = match family {
                "responses" => ProviderAction::Search {
                    query: None,
                    queries: None,
                },
                "interactions-v1" => {
                    reasoning.summary = Presence::Value(ReasoningSummary::Auto);
                    usage.cached_input_tokens = None;
                    ProviderAction::Search {
                        query: None,
                        queries: Some(vec![text("alpha"), text("beta")]),
                    }
                }
                "messages" => {
                    reasoning.mode = Presence::Value(ReasoningMode::Adaptive);
                    reasoning.display = Presence::Value(ReasoningDisplay::Omitted);
                    usage.input_relation = InputTokenRelation::ExcludesCacheReadAndWrite;
                    usage.total_relation = TotalTokenRelation::Unreported;
                    usage.input_tokens = Some(10);
                    usage.input_cache_write_tokens = Some(0);
                    usage.total_tokens = None;
                    ProviderAction::Search {
                        query: Some(text("alpha")),
                        queries: None,
                    }
                }
                _ => panic!("unknown abstract sample"),
            };
            let settings = GenerationSettings {
                instructions: Presence::Value(text("current configuration")),
                tools: Some(vec![ToolDefinition::Function(FunctionTool {
                    name: text("lookup"),
                    description: Some("original definition".into()),
                    parameters: Some(
                        serde_json::json!({"type":"object","properties":{"n":{"type":"integer"}}})
                            .into(),
                    ),
                    strict: FunctionStrictness::Explicit(false),
                    output_schema: None,
                    dispatch: Default::default(),
                })]),
                reasoning,
                ..Default::default()
            };
            let arguments = if family == "responses" {
                ToolArguments::from(r#"{"n":9007199254740993,"a":"\u0061"}"#)
            } else {
                ToolArguments::Structured(
                    StructuredValue::from_bytes(br#"{"n":9007199254740993,"a":"a"}"#).unwrap(),
                )
            };
            let domain = NativeAliasDomain {
                source: text(family),
                scope: LocalScope::new(8),
            };
            let r1 = GenerationResponse::new(
                vec![
                    (
                        owner(1, 1),
                        Item::Message(Message {
                            role: MessageRole::Assistant,
                            parts: vec![Part {
                                id: PartId::scoped(LocalScope::new(1), 1),
                                content: ContentPart::Text(text("reported commentary").into()),
                            }],
                            status: ItemLifecycle::Completed,
                            phase: Some(Phase::Commentary),
                        }),
                    ),
                    (
                        owner(1, 2),
                        Item::Reasoning(ReasoningItem {
                            parts: vec![(
                                PartId::scoped(LocalScope::new(1), 2),
                                ReasoningContent::Summary(text("reported summary")),
                            )],
                            status: ItemLifecycle::Completed,
                            replay: None,
                        }),
                    ),
                    (
                        owner(1, 3),
                        Item::ToolCall(ToolCall {
                            call_id: text("C"),
                            name: text("lookup"),
                            arguments,
                            status: ItemLifecycle::Completed,
                            context: CallContext::default(),
                        }),
                    ),
                    (
                        owner(1, 4),
                        Item::ProviderTool(ProviderToolObservation {
                            source: domain.clone(),
                            operation: ProviderOperation::Reported {
                                tool: text("search"),
                                alias: Some(text("S")),
                                requester: ProviderRequester::Model,
                                action: Some(action),
                            },
                            progress: None,
                            execution: None,
                            output: None,
                            artifact_status: None,
                        }),
                    ),
                ],
                Outcome::Completed,
            )
            .unwrap()
            .with_progress(InteractionProgress::NeedsContinuation)
            .unwrap()
            .with_usage(Some(usage))
            .unwrap();
            let r2 = GenerationResponse::new(
                vec![(
                    owner(2, 1),
                    Item::ProviderTool(ProviderToolObservation {
                        source: domain,
                        operation: ProviderOperation::Reference(
                            ProviderOperationReference::Native(text("S")),
                        ),
                        progress: None,
                        execution: Some(ToolExecution::Failed {
                            code: Some(text("denied")),
                        }),
                        output: Some(ToolOutput::Text("reported diagnostic".into())),
                        artifact_status: Some(ItemLifecycle::Completed),
                    }),
                )],
                Outcome::Completed,
            )
            .unwrap();
            let client_result = (
                owner(2, 2),
                Item::ToolResult(ToolResult {
                    is_error: None,
                    call_id: text("C"),
                    output: "client-reported result".into(),
                    execution: Some(ToolExecution::Unknown),
                    status: Some(ItemLifecycle::Completed),
                    context: Default::default(),
                }),
            );
            Self {
                settings,
                r1,
                r2,
                client_result,
            }
        }
        fn selected(&self) -> ClientManaged {
            ClientManaged::new(self.settings.clone())
                .unwrap()
                .append_items(vec![
                    (
                        ItemId::new(10),
                        Item::Instruction(Instruction {
                            authority: InstructionAuthority::Developer,
                            parts: vec![(
                                PartId::new(10),
                                InputPart::Text(text("observe; do not execute")),
                            )],
                            status: None,
                        }),
                    ),
                    message(11, "first user input"),
                    message(12, "second user input"),
                ])
                .unwrap()
                .select_response(&self.r1)
                .unwrap()
        }
        fn successor(&self, selected: ClientManaged) -> ClientManaged {
            selected
                .append_items(vec![self.client_result.clone()])
                .unwrap()
                .select_response(&self.r2)
                .unwrap()
        }
    }

    fn item(request: &GenerationRequest, id: ItemId) -> &Item {
        &request
            .items()
            .iter()
            .find(|(owner, _)| *owner == id)
            .unwrap()
            .1
    }

    #[test]
    fn combined_history_rejects_broken_association_authority_and_phase() {
        let sample = Sample::new("responses");
        let selected = sample.successor(sample.selected());
        let original = selected.clone().finish().unwrap().into_request();
        let accepted = selected
            .clone()
            .edit(vec![ContextEdit::Replace {
                owner: ItemId::new(12),
                item: message(12, "edited input").1,
            }])
            .unwrap()
            .finish()
            .unwrap();
        assert_eq!(
            item(accepted.request(), ItemId::new(11)),
            item(&original, ItemId::new(11))
        );
        let mut elevated = item(&original, ItemId::new(10)).clone();
        let Item::Instruction(instruction) = &mut elevated else {
            panic!()
        };
        instruction.authority = InstructionAuthority::System;
        let mut phased = item(&original, owner(1, 1)).clone();
        let Item::Message(message) = &mut phased else {
            panic!()
        };
        message.phase = Some(Phase::FinalAnswer);
        let mut reordered: Vec<_> = original.items().iter().map(|(id, _)| *id).collect();
        reordered.swap(0, 1);
        for edits in [
            vec![ContextEdit::Replace {
                owner: ItemId::new(10),
                item: elevated,
            }],
            vec![ContextEdit::Replace {
                owner: owner(1, 1),
                item: phased,
            }],
            vec![ContextEdit::Reorder(reordered)],
            vec![ContextEdit::Delete(vec![owner(1, 4)])],
        ] {
            assert_eq!(
                selected.clone().edit(edits).unwrap_err().stage,
                ContextStage::Association
            );
        }
        let missing = ClientManaged::new(sample.settings.clone())
            .unwrap()
            .select_response_items(&sample.r1, &[owner(1, 99)])
            .unwrap_err();
        assert_eq!(missing.stage, ContextStage::Selection);
        assert_eq!(selected.finish().unwrap().request(), &original);
    }
}
mod context_transforms {
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
            parts: vec![(PartId::new(1), InputPart::Text(text("protected")))],
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
                        is_error: None,
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
}
mod request_edits {
    //! A01: validated edits must uphold the constructor's guarantee.
    use morphiecore::semantic::{
        task::generation::*,
        value::{Presence, Text},
    };

    fn request() -> GenerationRequest {
        GenerationRequest::from_settings(
            vec![],
            GenerationSettings {
                instructions: Presence::Value(Text::new("synthetic", "instruction", 64).unwrap()),
                parallel_tool_calls: Some(false),
                ..Default::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn reasoning_edit_must_not_return_an_invalid_request() {
        let mut reasoning = ReasoningRequest::absent();
        reasoning.effort = Presence::Value(ReasoningEffort::High);
        let edited = request().with_reasoning(reasoning);
        assert_eq!(edited.unwrap_err(), GenerationError::InvalidControl);
    }
}
mod resource_table {
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
                (
                    PartId::new(1),
                    InputPart::Resource(ResourceUse {
                        id,
                        description: ResourceDescription::Image { detail: None },
                    }),
                ),
                (
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
                Item::Message(Message {
                    role: MessageRole::Assistant,
                    status: ItemLifecycle::Completed,
                    phase: None,
                    parts: vec![Part {
                        id: PartId::new(1),
                        content: ContentPart::Resource(ResourceUse {
                            id,
                            description: ResourceDescription::Image { detail: None },
                        }),
                    }],
                }),
            )],
            Outcome::Completed,
            ResourceTable::new(vec![(id, declaration())]).unwrap(),
        )
        .unwrap()
        .with_usage(Some(Usage::operation(3, 5, 8)))
        .unwrap();
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
}
