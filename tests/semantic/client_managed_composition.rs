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
                    operation: ProviderOperation::Reference(ProviderOperationReference::Native(
                        text("S"),
                    )),
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
fn revised_configuration(sample: &Sample) -> ConfigurationSnapshot {
    let mut settings = sample.config.settings().clone();
    settings.instructions = Presence::Value(text("new configuration"));
    let ToolDefinition::Function(tool) = &mut settings.tools.as_mut().unwrap()[0] else {
        panic!()
    };
    tool.description = Some("new definition, same name".into());
    ConfigurationSnapshot::new(ConfigurationId::new(LocalScope::new(9), 2), settings).unwrap()
}

#[test]
fn three_source_configured_successors_preserve_authorities_and_separate_progress() {
    for family in ["responses", "interactions-v1", "messages"] {
        let sample = Sample::new(family);
        let frozen = sample.r1.clone();
        let selected = sample.selected();
        let before = selected.clone().finish(&[]).unwrap();
        assert!(
            matches!(before.client_results(), Continuation::ToolResults(ref calls)
            if calls.len()==1 && calls[0].item==owner(1,3))
        );
        assert_eq!(before.provider_continuations().count(), 0);
        let condition = ProviderContinuationRequirement::capture(
            before.request(),
            owner(1, 4),
            SettingsDependency::only(SettingsField::ConfigurationRevision),
        )
        .unwrap();
        let group = RequestDependencyProof::capture(
            before.request(),
            HistoryDependency::ReplayGroup(GroupId::new(LocalScope::new(1), 1)),
            SettingsDependency::All,
        )
        .unwrap();
        let successor = sample
            .successor(
                selected
                    .require_provider_continuation(condition.clone())
                    .unwrap(),
            )
            .finish(std::slice::from_ref(&group))
            .unwrap();
        assert_eq!(successor.client_results(), Continuation::Unreported);
        assert_eq!(
            successor.provider_continuations().collect::<Vec<_>>(),
            vec![&condition]
        );
        assert_eq!(
            successor.selections()[0].progress,
            Some(InteractionProgress::NeedsContinuation)
        );
        assert_eq!(
            successor.selections()[1].progress,
            Some(InteractionProgress::Unreported)
        );
        assert_eq!(&successor.request().items()[3..7], frozen.items());
        assert_eq!(successor.request().replay_groups(), frozen.replay_groups());
        assert_eq!(successor.request().settings(), sample.config.settings());
        let Item::ProviderTool(result) = item(successor.request(), owner(2, 1)) else {
            panic!()
        };
        assert_eq!(
            result.resolve(successor.request().items()).unwrap(),
            owner(1, 4)
        );
        assert_eq!(sample.r2.outcome(), Outcome::Completed);
        assert!(matches!(
            result.execution,
            Some(ToolExecution::Failed { .. })
        ));
        assert_eq!(result.output, Some("reported diagnostic".into()));
        assert_eq!(sample.r1, frozen);
        // Missing history and conflicting original configuration fail independently.
        assert!(
            ClientManaged::from_configuration(sample.config.clone())
                .unwrap()
                .select_response(&sample.r2)
                .is_err()
        );
        let mut conflict = sample.config.settings().clone();
        conflict.instructions = Presence::Value(text("conflicting revision"));
        assert!(
            ClientManaged::from_configuration(
                ConfigurationSnapshot::new(sample.config.revision(), conflict,).unwrap()
            )
            .unwrap()
            .select_response(&sample.r1)
            .is_err()
        );
        let drifted = sample
            .successor(sample.selected())
            .require_provider_continuation(condition)
            .unwrap()
            .edit(vec![ContextEdit::Configuration(revised_configuration(
                &sample,
            ))])
            .unwrap()
            .finish(&[])
            .unwrap_err();
        assert_eq!(drifted.stage, ContextStage::Continuation);
        assert_eq!(drifted.index, Some(0));
        assert_eq!(sample.r1, frozen);
    }
}

#[test]
fn independent_branches_preserve_original_definitions_arguments_and_reports() {
    for family in ["responses", "interactions-v1", "messages"] {
        let sample = Sample::new(family);
        let selected = sample.successor(sample.selected());
        let source = selected.clone().finish(&[]).unwrap();
        let original = source.request().clone();
        let r1 = sample.r1.clone();
        let r2 = sample.r2.clone();
        let condition = ProviderContinuationRequirement::capture(
            &original,
            owner(1, 4),
            SettingsDependency::only(SettingsField::Reasoning),
        )
        .unwrap();
        let proof = RequestDependencyProof::capture(
            &original,
            HistoryDependency::ReplayGroup(GroupId::new(LocalScope::new(1), 1)),
            SettingsDependency::None,
        )
        .unwrap();
        let base = selected
            .require_provider_continuation(condition.clone())
            .unwrap();
        let order = vec![
            ItemId::new(10),
            ItemId::new(13),
            ItemId::new(11),
            owner(1, 1),
            owner(1, 2),
            owner(1, 3),
            owner(1, 4),
            owner(2, 2),
            owner(2, 1),
        ];
        let edits = vec![
            ContextEdit::Delete(vec![ItemId::new(12)]),
            ContextEdit::Insert {
                at: 2,
                item: message(13, "branch A inserted"),
            },
            ContextEdit::Replace {
                owner: ItemId::new(11),
                item: message(11, "branch A changed").1,
            },
            ContextEdit::Reorder(order.clone()),
        ];
        let a = base
            .clone()
            .edit(edits)
            .unwrap()
            .finish(std::slice::from_ref(&proof))
            .unwrap();
        let config = revised_configuration(&sample);
        let b = base
            .clone()
            .edit(vec![ContextEdit::Configuration(config.clone())])
            .unwrap()
            .finish(std::slice::from_ref(&proof))
            .unwrap();
        assert_eq!(
            a.request()
                .items()
                .iter()
                .map(|(id, _)| *id)
                .collect::<Vec<_>>(),
            order
        );
        assert_eq!(
            item(a.request(), ItemId::new(11)),
            &message(11, "branch A changed").1
        );
        assert_eq!(b.request().items(), original.items());
        assert_eq!(a.request().settings(), original.settings());
        assert_eq!(b.request().settings(), config.settings());
        for branch in [&a, &b] {
            assert_eq!(branch.client_results(), Continuation::Unreported);
            assert_eq!(
                branch.provider_continuations().collect::<Vec<_>>(),
                vec![&condition]
            );
            assert_eq!(branch.selections(), source.selections());
            assert_eq!(
                item(branch.request(), ItemId::new(10)),
                item(&original, ItemId::new(10))
            );
            for id in [
                owner(1, 1),
                owner(1, 2),
                owner(1, 3),
                owner(1, 4),
                owner(2, 1),
                owner(2, 2),
            ] {
                assert_eq!(item(branch.request(), id), item(&original, id));
            }
            let Item::ToolCall(call) = item(branch.request(), owner(1, 3)) else {
                panic!()
            };
            assert_eq!(
                call.context.definition.as_ref().unwrap().snapshot(),
                &sample.config
            );
            let ToolDefinition::Function(definition) =
                call.context.definition.as_ref().unwrap().definition()
            else {
                panic!()
            };
            assert_eq!(
                definition.description.as_deref(),
                Some("original definition")
            );
            match &call.arguments {
                ToolArguments::Raw(raw) => {
                    assert_eq!(family, "responses");
                    assert_eq!(raw, r#"{"n":9007199254740993,"a":"\u0061"}"#);
                }
                ToolArguments::Structured(value) => {
                    assert_ne!(family, "responses");
                    assert_eq!(
                        serde_json::to_string(value.value()).unwrap(),
                        r#"{"n":9007199254740993,"a":"a"}"#
                    );
                }
                ToolArguments::StructuredPartial(_) => panic!("partial became complete"),
            }
        }
        // Configuration independence is scoped, not a blanket acceptance of drift.
        let pinned = RequestDependencyProof::capture(
            &original,
            HistoryDependency::Owners(vec![owner(1, 3)]),
            SettingsDependency::only(SettingsField::ConfigurationRevision),
        )
        .unwrap();
        assert!(pinned.check(a.request()).is_ok());
        assert!(pinned.check(b.request()).is_err());
        let mut rebound = item(&original, owner(1, 3)).clone();
        let Item::ToolCall(call) = &mut rebound else {
            panic!()
        };
        call.context.definition = Some(
            ToolDefinitionBinding::new(
                config,
                ToolReference {
                    kind: ToolKind::Function,
                    name: text("lookup"),
                    namespace: None,
                },
            )
            .unwrap(),
        );
        let mut changed_arguments = item(&original, owner(1, 3)).clone();
        let Item::ToolCall(call) = &mut changed_arguments else {
            panic!()
        };
        call.arguments = if family == "responses" {
            r#"{"n":9007199254740994,"a":"\u0061"}"#.into()
        } else {
            // Same JSON members with different order are a different authority.
            ToolArguments::Structured(
                StructuredValue::from_bytes(br#"{"a":"a","n":9007199254740993}"#).unwrap(),
            )
        };
        for replacement in [rebound, changed_arguments] {
            let failed = base
                .clone()
                .edit(vec![ContextEdit::Replace {
                    owner: owner(1, 3),
                    item: replacement,
                }])
                .unwrap_err();
            assert_eq!(failed.stage, ContextStage::Association);
            assert_eq!(failed.error, GenerationError::InvalidDependency);
        }
        assert_eq!(base.finish(&[]).unwrap().request(), &original);
        assert_eq!(sample.r1, r1);
        assert_eq!(sample.r2, r2);
    }
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
