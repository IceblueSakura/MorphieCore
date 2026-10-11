//! Client-managed history, explicit edits, revisions and dependency proofs.
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
            replay: None,
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
    fn selected_derivations_and_budgets_survive_without_copying_source_configuration() {
        let source = GenerationRequest::new(
            vec![(ItemId::new(1), call())],
            GenerationControls::default(),
        )
        .unwrap();
        let mut replacement = call();
        let Item::ToolCall(value) = &mut replacement else {
            unreachable!()
        };
        value.call_id = text("C2");
        let source = source
            .revise_call(ItemId::new(1), (ItemId::new(2), replacement))
            .unwrap();
        let selected = ClientManaged::new(GenerationSettings::default())
            .unwrap()
            .select_request(&source)
            .unwrap()
            .build(&[])
            .unwrap();
        assert_eq!(selected.call_derivations(), source.call_derivations());
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
    fn configured_mixed_successor_separates_client_results_provider_progress_and_revision_dependencies()
     {
        let settings = GenerationSettings {
            instructions: Presence::Value(text("original configuration")),
            tools: Some(vec![ToolDefinition::Function(FunctionTool {
                name: text("lookup"),
                description: None,
                parameters: Some(serde_json::json!({"type":"object"}).into()),
                strict: FunctionStrictness::Explicit(false),
                output_schema: None,
                dispatch: Default::default(),
            })]),
            ..Default::default()
        };
        let config = ConfigurationSnapshot::new(
            ConfigurationId::new(LocalScope::new(1), 1),
            settings.clone(),
        )
        .unwrap();
        let mut client = call();
        let Item::ToolCall(c) = &mut client else {
            panic!()
        };
        c.context.definition = Some(
            ToolDefinitionBinding::new(
                config.clone(),
                ToolReference {
                    kind: ToolKind::Function,
                    name: text("lookup"),
                    namespace: None,
                },
            )
            .unwrap(),
        );
        let r1 = GenerationResponse::new(
            vec![
                (
                    ItemId::new(5),
                    Item::Message(Message {
                        role: MessageRole::Assistant,
                        parts: vec![Part {
                            replay: None,
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
                (ItemId::new(1), client),
                (ItemId::new(2), provider(false)),
            ],
            Outcome::Completed,
        )
        .unwrap()
        .with_replay_groups(vec![
            ReplayGroup::new(
                GroupId::new(LocalScope::ROOT, 1),
                vec![
                    ItemId::new(5),
                    ItemId::new(6),
                    ItemId::new(1),
                    ItemId::new(2),
                ],
            )
            .unwrap(),
        ])
        .unwrap()
        .with_progress(InteractionProgress::NeedsContinuation)
        .unwrap();
        let selected = ClientManaged::from_configuration(config.clone())
            .unwrap()
            .select_response(&r1)
            .unwrap();
        let pending = selected.clone().finish(&[]).unwrap();
        let Continuation::ToolResults(calls) = pending.client_results() else {
            panic!()
        };
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].item, ItemId::new(1));
        assert_eq!(pending.provider_continuations().count(), 0);
        let requirement = ProviderContinuationRequirement::capture(
            pending.request(),
            ItemId::new(2),
            SettingsDependency::only(SettingsField::ConfigurationRevision),
        )
        .unwrap();
        assert!(
            ProviderContinuationRequirement::capture(
                pending.request(),
                ItemId::new(1),
                SettingsDependency::All
            )
            .is_err()
        );
        let selected = selected
            .require_provider_continuation(requirement.clone())
            .unwrap();
        assert_eq!(
            selected
                .clone()
                .finish(&[])
                .unwrap()
                .provider_continuations()
                .count(),
            1
        );
        let proof = RequestDependencyProof::capture(
            pending.request(),
            HistoryDependency::ReplayGroup(GroupId::new(LocalScope::ROOT, 1)),
            SettingsDependency::only(SettingsField::ConfigurationRevision),
        )
        .unwrap();
        let r2 =
            GenerationResponse::new(vec![(ItemId::new(4), provider(true))], Outcome::Completed)
                .unwrap();
        let successor = selected
            .append_items(vec![(ItemId::new(3), result())])
            .unwrap()
            .select_response(&r2)
            .unwrap()
            .finish(std::slice::from_ref(&proof))
            .unwrap();
        assert_eq!(successor.client_results(), Continuation::Unreported);
        assert_eq!(
            successor.selections()[1].progress,
            Some(InteractionProgress::Unreported)
        );
        let Item::ProviderTool(s) = &successor.request().items().last().unwrap().1 else {
            panic!()
        };
        assert_eq!(
            s.resolve(successor.request().items()).unwrap(),
            ItemId::new(2)
        );
        assert!(
            ClientManaged::from_configuration(config)
                .unwrap()
                .select_response(&r2)
                .is_err()
        );
        let changed = ClientManaged::from_configuration(
            ConfigurationSnapshot::new(ConfigurationId::new(LocalScope::new(1), 2), settings)
                .unwrap(),
        )
        .unwrap()
        .select_request(successor.request())
        .unwrap()
        .finish(&[proof])
        .unwrap_err();
        assert_eq!(changed.stage, ContextStage::Dependency);
        let changed = ClientManaged::from_configuration(
            ConfigurationSnapshot::new(
                ConfigurationId::new(LocalScope::new(1), 3),
                successor.request().settings().clone(),
            )
            .unwrap(),
        )
        .unwrap()
        .select_request(successor.request())
        .unwrap()
        .require_provider_continuation(requirement)
        .unwrap()
        .finish(&[])
        .unwrap_err();
        assert_eq!(changed.stage, ContextStage::Continuation);
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
                    replay: None,
                    id: PartId::new(id),
                    content: ContentPart::Text(text(value).into()),
                }],
                status: ItemLifecycle::Completed,
                phase: None,
            }),
        )
    }

    struct Sample {
        config: ConfigurationSnapshot,
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
            let config = ConfigurationSnapshot::new(
                ConfigurationId::new(LocalScope::new(9), 1),
                GenerationSettings {
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
                },
            )
            .unwrap();
            let binding = ToolDefinitionBinding::new(
                config.clone(),
                ToolReference {
                    kind: ToolKind::Function,
                    name: text("lookup"),
                    namespace: None,
                },
            )
            .unwrap();
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
                                replay: None,
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
                            context: CallContext {
                                definition: Some(binding),
                                ..Default::default()
                            },
                        }),
                    ),
                    (
                        owner(1, 4),
                        Item::ProviderTool(ProviderToolObservation {
                            replay: None,
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
            .with_replay_groups(vec![
                ReplayGroup::new(
                    GroupId::new(LocalScope::new(1), 1),
                    vec![owner(1, 1), owner(1, 2), owner(1, 3), owner(1, 4)],
                )
                .unwrap(),
            ])
            .unwrap()
            .with_progress(InteractionProgress::NeedsContinuation)
            .unwrap()
            .with_usage(usage)
            .unwrap();
            let r2 = GenerationResponse::new(
                vec![(
                    owner(2, 1),
                    Item::ProviderTool(ProviderToolObservation {
                        replay: None,
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
                config,
                r1,
                r2,
                client_result,
            }
        }
        fn selected(&self) -> ClientManaged {
            ClientManaged::from_configuration(self.config.clone())
                .unwrap()
                .append_items(vec![
                    (
                        ItemId::new(10),
                        Item::Instruction(Instruction {
                            authority: InstructionAuthority::Developer,
                            parts: vec![(PartId::new(10), text("observe; do not execute"))],
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
    fn combined_history_rejects_broken_association_authority_phase_and_dependencies() {
        let sample = Sample::new("responses");
        let selected = sample.successor(sample.selected());
        let original = selected.clone().finish(&[]).unwrap().into_request();
        let proof = RequestDependencyProof::capture(
            &original,
            HistoryDependency::Owners(vec![ItemId::new(11)]),
            SettingsDependency::None,
        )
        .unwrap();
        let accepted = selected
            .clone()
            .edit(vec![ContextEdit::Replace {
                owner: ItemId::new(12),
                item: message(12, "outside dependency").1,
            }])
            .unwrap()
            .finish(std::slice::from_ref(&proof))
            .unwrap();
        assert_eq!(
            item(accepted.request(), ItemId::new(11)),
            item(&original, ItemId::new(11))
        );
        let failed = selected
            .clone()
            .edit(vec![ContextEdit::Replace {
                owner: ItemId::new(11),
                item: message(11, "inside dependency").1,
            }])
            .unwrap()
            .finish(&[proof])
            .unwrap_err();
        assert_eq!(failed.stage, ContextStage::Dependency);
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
            vec![
                ContextEdit::ReplayGroups(vec![]),
                ContextEdit::Delete(vec![owner(1, 4)]),
            ],
        ] {
            assert_eq!(
                selected.clone().edit(edits).unwrap_err().stage,
                ContextStage::Association
            );
        }
        let missing = ClientManaged::from_configuration(sample.config.clone())
            .unwrap()
            .select_response_items(&sample.r1, &[owner(1, 99)])
            .unwrap_err();
        assert_eq!(missing.stage, ContextStage::Selection);
        assert_eq!(selected.finish(&[]).unwrap().request(), &original);
    }
}
mod configuration_revisions {
    //! Historical definitions and explicit configuration revisions are not execution grants.
    use morphiecore::semantic::{
        task::generation::*,
        value::{Presence, Text},
    };
    use serde_json::json;

    fn text(value: &str) -> Text {
        Text::new(value, "synthetic", MAX_TEXT_BYTES).unwrap()
    }
    fn settings(description: &str) -> GenerationSettings {
        GenerationSettings {
            tools: Some(vec![ToolDefinition::Function(FunctionTool {
                name: text("lookup"),
                description: Some(description.into()),
                parameters: Some(json!({"type":"object","properties":{}}).into()),
                strict: FunctionStrictness::Explicit(false),
                output_schema: None,
                dispatch: ToolDispatch::default(),
            })]),
            ..Default::default()
        }
    }
    fn snapshot(revision: u64, settings: GenerationSettings) -> ConfigurationSnapshot {
        ConfigurationSnapshot::new(ConfigurationId::new(LocalScope::new(1), revision), settings)
            .unwrap()
    }
    fn binding(snapshot: ConfigurationSnapshot) -> ToolDefinitionBinding {
        ToolDefinitionBinding::new(
            snapshot,
            ToolReference {
                kind: ToolKind::Function,
                name: text("lookup"),
                namespace: None,
            },
        )
        .unwrap()
    }
    fn call(id: &str, binding: ToolDefinitionBinding) -> Item {
        Item::ToolCall(ToolCall {
            call_id: text(id),
            name: text("lookup"),
            arguments: "{}".into(),
            status: ItemLifecycle::Completed,
            context: CallContext {
                definition: Some(binding),
                ..Default::default()
            },
        })
    }
    fn request(item: Item) -> GenerationRequest {
        GenerationRequest::new(vec![(ItemId::new(1), item)], GenerationControls::default()).unwrap()
    }
    fn old_binding(request: &GenerationRequest) -> &ToolDefinitionBinding {
        let Item::ToolCall(call) = &request.items()[0].1 else {
            panic!("call")
        };
        call.context.definition.as_ref().unwrap()
    }

    #[test]
    fn selected_history_keeps_original_definition_without_inheriting_current_settings() {
        let original = snapshot(1, settings("old contract"));
        let original_binding = binding(original.clone());
        let response = GenerationResponse::new(
            vec![(ItemId::new(1), call("C", original_binding.clone()))],
            Outcome::Completed,
        )
        .unwrap();
        let current = snapshot(2, settings("new contract"));
        let successor = ClientManaged::from_configuration(current.clone())
            .unwrap()
            .select_response(&response)
            .unwrap()
            .append_items(vec![(
                ItemId::new(2),
                Item::ToolResult(ToolResult {
                    is_error: None,
                    call_id: text("C"),
                    output: "reported".into(),
                    execution: None,
                    status: None,
                    context: CallContext::default(),
                }),
            )])
            .unwrap()
            .build(&[])
            .unwrap();
        assert_eq!(successor.configuration_revision(), Some(current.revision()));
        assert_eq!(successor.settings(), current.settings());
        assert_eq!(old_binding(&successor), &original_binding);
        let ToolDefinition::Function(definition) = old_binding(&successor).definition() else {
            panic!("function")
        };
        assert_eq!(definition.description.as_deref(), Some("old contract"));
        assert_eq!(successor.continuation(), Continuation::Unreported);
        let unversioned = ClientManaged::new(GenerationSettings::default())
            .unwrap()
            .select_request(&successor)
            .unwrap()
            .build(&[])
            .unwrap();
        assert_eq!(unversioned.configuration_revision(), None);
        assert!(unversioned.tools().is_empty());
        assert_eq!(old_binding(&unversioned), &original_binding);
        assert_eq!(response.items().len(), 1);
    }

    #[test]
    fn same_revision_checks_schema_order_not_only_json_value_equality() {
        let mut left = settings("ordered");
        let mut right = left.clone();
        let ToolDefinition::Function(tool) = &mut left.tools.as_mut().unwrap()[0] else {
            panic!()
        };
        tool.parameters = Some(
            serde_json::from_str::<serde_json::Value>(
                r#"{"type":"object","properties":{"a":{"type":"string"},"b":{"type":"string"}}}"#,
            )
            .unwrap()
            .into(),
        );
        let ToolDefinition::Function(tool) = &mut right.tools.as_mut().unwrap()[0] else {
            panic!()
        };
        tool.parameters = Some(
            serde_json::from_str::<serde_json::Value>(
                r#"{"type":"object","properties":{"b":{"type":"string"},"a":{"type":"string"}}}"#,
            )
            .unwrap()
            .into(),
        );
        assert_eq!(left, right, "serde value equality ignores object order");
        let source = request(call("C", binding(snapshot(1, left.clone()))))
            .with_configuration(snapshot(1, left))
            .unwrap();
        assert!(source.clone().with_settings(right.clone()).is_err());
        assert!(source.with_configuration(snapshot(1, right)).is_err());
    }

    #[test]
    fn definitions_resolve_by_kind_namespace_and_name_and_results_cannot_own_them() {
        let custom = snapshot(
            3,
            GenerationSettings {
                tools: Some(vec![ToolDefinition::Custom(CustomTool {
                    name: text("lookup"),
                    description: None,
                    format: Some(CustomFormat::Text),
                    dispatch: ToolDispatch::default(),
                })]),
                ..Default::default()
            },
        );
        let custom = ToolDefinitionBinding::new(
            custom,
            ToolReference {
                kind: ToolKind::Custom,
                name: text("lookup"),
                namespace: None,
            },
        )
        .unwrap();
        let custom_call = Item::CustomCall(CustomCall {
            call_id: text("X"),
            name: text("lookup"),
            input: "raw".into(),
            context: CallContext {
                definition: Some(custom.clone()),
                ..Default::default()
            },
        });
        let response =
            GenerationResponse::new(vec![(ItemId::new(1), custom_call)], Outcome::Completed)
                .unwrap();
        assert!(
            response
                .clone()
                .with_items(vec![])
                .unwrap()
                .items()
                .is_empty()
        );
        let custom_start = StreamEvent::ItemStarted {
            item: ItemId::new(1),
            replay: None,
            kind: ItemKind::CustomCall {
                call_id: text("X"),
                name: text("lookup"),
                context: CallContext {
                    definition: Some(custom.clone()),
                    ..Default::default()
                },
            },
        };
        assert!(
            reduce(
                reduce(StreamState::new(), StreamEvent::Started).unwrap(),
                custom_start
            )
            .is_ok()
        );
        assert!(
            GenerationResponse::new(
                vec![(ItemId::new(1), call("C", custom))],
                Outcome::Completed
            )
            .is_err()
        );
        let base = settings("qualified").tools.unwrap().remove(0);
        let config = snapshot(
            1,
            GenerationSettings {
                tools: Some(vec![ToolDefinition::Namespace(ToolNamespace {
                    name: text("space"),
                    description: String::new(),
                    tools: vec![base],
                })]),
                ..Default::default()
            },
        );
        let reference = ToolReference {
            kind: ToolKind::Function,
            name: text("lookup"),
            namespace: Some(text("space")),
        };
        let valid = ToolDefinitionBinding::new(config.clone(), reference.clone()).unwrap();
        for bad in [
            ToolReference {
                namespace: None,
                ..reference.clone()
            },
            ToolReference {
                kind: ToolKind::Custom,
                ..reference.clone()
            },
            ToolReference {
                name: text("missing"),
                ..reference
            },
        ] {
            assert!(ToolDefinitionBinding::new(config.clone(), bad).is_err());
        }
        assert!(
            GenerationRequest::new(
                vec![(ItemId::new(1), call("C", valid.clone()))],
                GenerationControls::default()
            )
            .is_err()
        );
        let mut item = call("C", valid.clone());
        let Item::ToolCall(value) = &mut item else {
            panic!()
        };
        value.context.namespace = Some(text("space"));
        let source = request(item);
        let mut items = source.items().to_vec();
        items.push((
            ItemId::new(2),
            Item::ToolResult(ToolResult {
                is_error: None,
                call_id: text("C"),
                output: "reported".into(),
                execution: None,
                status: None,
                context: CallContext {
                    definition: Some(valid),
                    ..Default::default()
                },
            }),
        ));
        assert!(source.with_items(items).is_err());
    }

    #[test]
    fn snapshot_budgets_and_debug_do_not_hide_retained_configuration() {
        let mut invalid = settings("invalid");
        invalid.controls.max_output_tokens = Some(0);
        assert!(
            ConfigurationSnapshot::new(ConfigurationId::new(LocalScope::ROOT, 1), invalid).is_err()
        );
        let mut large = settings("private-definition");
        large.instructions = Presence::Value(text(&"x".repeat(MAX_TEXT_BYTES)));
        let retained = binding(snapshot(1, large));
        assert!(!format!("{retained:?}").contains("private-definition"));
        assert!(!format!("{:?}", retained.snapshot()).contains("private-definition"));
        assert!(
            GenerationResponse::new(
                (0..3)
                    .map(|n| (ItemId::new(n), call(&n.to_string(), retained.clone())))
                    .collect(),
                Outcome::Completed
            )
            .is_ok()
        );
        assert!(matches!(
            GenerationResponse::new(
                (0..4)
                    .map(|n| (ItemId::new(n), call(&n.to_string(), retained.clone())))
                    .collect(),
                Outcome::Completed
            ),
            Err(GenerationError::Limit)
        ));
    }

    fn start(binding: ToolDefinitionBinding) -> StreamEvent {
        StreamEvent::ItemStarted {
            item: ItemId::new(1),
            kind: ItemKind::ToolCall {
                format: ArgumentFormat::Raw,
                call_id: text("C"),
                name: text("lookup"),
                message: None,
                context: CallContext {
                    definition: Some(binding),
                    ..Default::default()
                },
            },
            replay: None,
        }
    }
    #[test]
    fn bound_events_match_static_and_reject_conflicting_revisions_at_intake() {
        let bound = binding(snapshot(1, settings("old")));
        let mut state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
        state = reduce(state, start(bound.clone())).unwrap();
        let mut conflict = start(binding(snapshot(1, settings("changed"))));
        let StreamEvent::ItemStarted {
            item,
            kind: ItemKind::ToolCall { call_id, .. },
            ..
        } = &mut conflict
        else {
            panic!()
        };
        *item = ItemId::new(2);
        *call_id = text("D");
        assert!(reduce(state.clone(), conflict).is_err());
        for event in [
            StreamEvent::PartStarted {
                item: ItemId::new(1),
                part: PartId::new(1),
                kind: PartKind::Arguments,
            },
            StreamEvent::Delta {
                item: ItemId::new(1),
                part: PartId::new(1),
                fragment: "{}".into(),
                logprobs: vec![],
            },
            StreamEvent::ValueFinished {
                item: ItemId::new(1),
                part: PartId::new(1),
            },
            StreamEvent::PartFinished {
                item: ItemId::new(1),
                part: PartId::new(1),
            },
            StreamEvent::ItemFinished {
                item: ItemId::new(1),
                status: ItemLifecycle::Completed,
                replay: None,
            },
            StreamEvent::Terminal {
                terminal: StreamTerminal::Completed,
                details: TerminalDetails::default(),
            },
        ] {
            state = reduce(state, event).unwrap();
        }
        let actual = materialize(&state).unwrap();
        let expected =
            GenerationResponse::new(vec![(ItemId::new(1), call("C", bound))], Outcome::Completed)
                .unwrap();
        assert_eq!(actual, expected);
    }
}
mod context_transforms {
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
                    replay: None,
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
}
mod history_dependencies {
    //! Dependency proofs bind relationships and source values, not just surviving IDs.
    use crate::events_support::text;
    use morphiecore::{
        lowering::generation::RepresentationError,
        protocol::{
            fidelity::FidelityRecords,
            openai::{Profile, TargetCaps as Contract, lowering::lower_request},
        },
        semantic::{task::generation::*, value::ReplayOrigin},
    };
    fn message(id: u64, body: &str) -> (ItemId, Item) {
        (
            ItemId::new(id),
            Item::Message(Message {
                role: MessageRole::Assistant,
                parts: vec![Part {
                    replay: None,
                    id: PartId::new(id),
                    content: ContentPart::Text(
                        TextContent::new(
                            text(body),
                            vec![],
                            morphiecore::semantic::value::Presence::Absent,
                        )
                        .unwrap(),
                    ),
                }],
                status: ItemLifecycle::Completed,
                phase: None,
            }),
        )
    }
    fn call(id: u64) -> (ItemId, Item) {
        (
            ItemId::new(id),
            Item::ToolCall(ToolCall {
                call_id: text(&format!("c{id}")),
                name: text("lookup"),
                arguments: "{}".into(),
                status: ItemLifecycle::Completed,
                context: CallContext::default(),
            }),
        )
    }
    fn history(items: Vec<(ItemId, Item)>) -> GenerationRequest {
        let owners = items
            .iter()
            .filter_map(|(id, item)| {
                matches!(item, Item::ToolCall(_)).then_some((*id, ItemId::new(1)))
            })
            .collect();
        GenerationRequest::new(items, GenerationControls::default())
            .unwrap()
            .with_message_owners(owners)
            .unwrap()
    }

    #[test]
    fn group_bound_replay_cannot_bypass_final_history_checks() {
        let origin = ReplayOrigin::new("synthetic-scope").unwrap();
        let reasoning = ReasoningItem {
            status: ItemLifecycle::Completed,
            parts: vec![],
            replay: Some(ReplayValue::final_value(
                ReplayFormat::ResponsesEncrypted,
                text("synthetic-token"),
            )),
        };
        let request = history(vec![
            (ItemId::new(9), Item::Reasoning(reasoning.clone())),
            message(1, "body"),
            call(2),
        ]);
        let mut fidelity = FidelityRecords::default();
        fidelity
            .record_replay(ItemId::new(9), &reasoning, Some(origin.clone()))
            .unwrap();
        let proof = RequestDependencyProof::capture(
            &request,
            HistoryDependency::MessageGroup(ItemId::new(1)),
            morphiecore::semantic::task::generation::SettingsDependency::All,
        )
        .unwrap();
        fidelity
            .bind_replay_dependency(ItemId::new(9), proof, &request)
            .unwrap();
        assert!(!fidelity.replay_matches(ItemId::new(9), &reasoning, Some(&origin)));
        assert!(fidelity.replay_matches_request(
            ItemId::new(9),
            &reasoning,
            Some(&origin),
            &request
        ));
        let mut contract = Contract::full();
        contract.replay_origin = Some(origin);
        // Boundary omission cannot bypass a necessary group-bound replay dependency.
        assert_eq!(
            lower_request(&request, &fidelity, Profile::Responses, contract.clone()).err(),
            Some(RepresentationError::ReplayOrigin)
        );
        let mut items = request.items().to_vec();
        items[1] = message(1, "changed");
        let changed = history(items);
        assert_eq!(
            lower_request(&changed, &fidelity, Profile::Responses, contract).err(),
            Some(RepresentationError::ReplayOrigin)
        );
        assert!(
            fidelity
                .bind_replay_dependency(
                    ItemId::new(9),
                    RequestDependencyProof::capture(
                        &changed,
                        HistoryDependency::MessageGroup(ItemId::new(1)),
                        morphiecore::semantic::task::generation::SettingsDependency::None
                    )
                    .unwrap(),
                    &changed
                )
                .is_err()
        );
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
mod cache_prefix {
    //! Prefix stability is an explicit dependency check, not a cache hit or route choice.
    use morphiecore::{
        protocol::openai::{Adapter, Dialect, Profile},
        semantic::{
            cache::{CachePrefixIntent, CachePrefixScope},
            task::generation::*,
            value::Presence,
        },
    };
    use serde_json::json;
    fn request() -> morphiecore::adapter::Request {
        Adapter::new(Profile::Responses, Dialect::Standard, None)
            .decode_request(
                &serde_json::to_vec(
                    &json!({"model":"synthetic","input":[{"role":"user","content":"prefix"}]}),
                )
                .unwrap(),
            )
            .unwrap()
    }
    fn scope() -> CachePrefixScope {
        CachePrefixScope::new("synthetic-cache-scope").unwrap()
    }
    fn intent() -> CachePrefixIntent {
        CachePrefixIntent::through(ItemId::new(1))
    }
    #[test]
    fn adapter_prefix_check_allows_append_and_rejects_prefix_context_and_scope_edits() {
        let source = request();
        let proof = source.capture_cache_prefix(intent(), &scope()).unwrap();
        source.check_cache_prefix(&proof, &scope()).unwrap();
        let mut append = source.clone();
        let mut items = append.task.semantic.items().to_vec();
        let mut new_item = items[0].clone();
        new_item.0 = ItemId::new(2);
        let Item::Message(m) = &mut new_item.1 else {
            panic!("message")
        };
        m.parts[0].id = PartId::new(2);
        items.push(new_item.clone());
        append.task.semantic = append.task.semantic.with_items(items).unwrap();
        append.check_cache_prefix(&proof, &scope()).unwrap();
        let inserted = source
            .task
            .semantic
            .clone()
            .with_items(vec![new_item, source.task.semantic.items()[0].clone()])
            .unwrap();
        append.task.semantic = inserted;
        assert!(append.check_cache_prefix(&proof, &scope()).is_err());
        let mut changed = request();
        let mut items = changed.task.semantic.items().to_vec();
        let Item::Message(message) = &mut items[0].1 else {
            panic!("message")
        };
        message.parts[0].content = ContentPart::Text(
            TextContent::new(
                morphiecore::semantic::value::Text::new("changed", "fixture", 32).unwrap(),
                vec![],
                Presence::Absent,
            )
            .unwrap(),
        );
        changed.task.semantic = changed.task.semantic.with_items(items).unwrap();
        assert!(changed.check_cache_prefix(&proof, &scope()).is_err());
        let mut changed = request();
        changed.model = "other".into();
        assert!(changed.check_cache_prefix(&proof, &scope()).is_err());
        let mut changed = request();
        changed.context.cache.prompt_cache_key = Presence::Value("new-key".into());
        assert!(changed.check_cache_prefix(&proof, &scope()).is_err());
        let mut changed = request();
        changed.context.service_tier =
            Presence::Value(morphiecore::semantic::context::ServiceTier::Priority);
        assert!(changed.check_cache_prefix(&proof, &scope()).is_err());
        assert!(
            source
                .check_cache_prefix(&proof, &CachePrefixScope::new("other-scope").unwrap())
                .is_err()
        );
        let mut changed = request();
        changed.context.metadata = Presence::Value(std::collections::BTreeMap::from([(
            "label".into(),
            "changed".into(),
        )]));
        assert!(changed.check_cache_prefix(&proof, &scope()).is_err());
        let mut changed = request();
        changed.cache_session =
            Some(morphiecore::protocol::cache::CacheSession::new("new-group").unwrap());
        assert!(changed.check_cache_prefix(&proof, &scope()).is_err());
        let mut changed = request();
        changed.conversation = Some(
            morphiecore::semantic::context::ConversationContext::conversation("logical-one")
                .unwrap(),
        );
        assert!(changed.check_cache_prefix(&proof, &scope()).is_err());
        let scoped = changed.capture_cache_prefix(intent(), &scope()).unwrap();
        changed.conversation = Some(
            morphiecore::semantic::context::ConversationContext::independent_request("logical-one")
                .unwrap(),
        );
        assert!(changed.check_cache_prefix(&scoped, &scope()).is_err());
    }

    #[test]
    fn retired_cache_part_records_cannot_reappear_on_a_later_value() {
        let source = request();
        let Item::Message(message) = &source.task.semantic.items()[0].1 else {
            panic!()
        };
        let owner = message.parts[0].id;
        let foreign = PartId::scoped(LocalScope::new(99), owner.get());
        let mut fidelity = morphiecore::protocol::fidelity::FidelityRecords::default();
        fidelity.record_cache_breakpoint(owner).unwrap();
        fidelity.record_cache_breakpoint(foreign).unwrap();
        fidelity.retain_owners(source.task.semantic.items());
        assert!(fidelity.cache_breakpoint(owner));
        assert!(!fidelity.cache_breakpoint(foreign));
        fidelity.retain_owners(&[]);
        assert!(!fidelity.cache_breakpoint(owner));
        fidelity.retain_owners(source.task.semantic.items());
        assert!(!fidelity.cache_breakpoint(owner));
    }
}
mod resource_table {
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
}
