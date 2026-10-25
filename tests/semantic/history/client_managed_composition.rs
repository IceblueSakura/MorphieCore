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
            participant: None,
            role: MessageRole::User,
            parts: vec![Part {
                replay: None,
                id: PartId::new(id),
                content: ContentPart::Text(text(value).into()),
                cache: None,
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
        let mut reasoning = ReasoningRequest::present(None);
        let mut usage = Usage::operation(4106, 20, 4126);
        usage.cached_input_tokens = Some(4096);
        let action = match family {
            "responses" => ProviderAction::Search {
                query: None,
                queries: None,
            },
            "interactions-v1" => {
                reasoning = ReasoningRequest::present(Some(ReasoningSummary::Auto));
                usage.cached_input_tokens = None;
                ProviderAction::Search {
                    query: None,
                    queries: Some(vec![text("alpha"), text("beta")]),
                }
            }
            "messages" => {
                reasoning.mode = Presence::Value(ReasoningMode::Adaptive);
                reasoning.presentation.visibility = Presence::Value(ReasoningDisplay::Omitted);
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
            instructions: Presence::Value(text("current configuration").into()),
            effort: Presence::Value(EffortControl {
                scope: if family == "messages" {
                    EffortScope::Response
                } else {
                    EffortScope::Reasoning
                },
                level: EffortLevel::High,
            }),
            tools: Some(vec![ToolDefinition::Function(FunctionTool {
                name: text("lookup"),
                description: Some("original definition".into()),
                input_examples: None,
                parameters: Some(
                    serde_json::json!({"type":"object","properties":{"n":{"type":"integer"}}})
                        .into(),
                ),
                strict: FunctionStrictness::Explicit(false),
                output_schema: None,
                dispatch: Default::default(),
                cache: None,
                eager_input: None,
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
                        participant: None,
                        role: MessageRole::Assistant,
                        parts: vec![Part {
                            replay: None,
                            id: PartId::scoped(LocalScope::new(1), 1),
                            cache: None,
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
                        replay: None,
                        call_id: text("C"),
                        cache: None,
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
                is_error: None,
                cache: None,
                call_id: Some(text("C")),
                name: None,
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
                        participant: None,
                        parts: vec![Part::new(PartId::new(10), text("observe; do not execute"))],
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
    let mut relabeled = item(&original, ItemId::new(10)).clone();
    let Item::Instruction(instruction) = &mut relabeled else {
        panic!()
    };
    instruction.participant = Some(text("another operator"));
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
            item: relabeled,
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
