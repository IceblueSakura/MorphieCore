//! Encoding guards also protect crate-internal callers from forged representations.
use super::*;
use crate::semantic::{task::generation::*, value::Text};

#[test]
fn static_codecs_reject_new_domains_even_for_forged_target_handles() {
    let base = GenerationResponse::new(
        vec![(
            ItemId::new(1),
            Item::ToolCall(ToolCall {
                call_id: text("c"),
                name: text("lookup"),
                arguments: "{}".into(),
                status: ItemLifecycle::Completed,
                context: CallContext::default(),
            }),
        )],
        Outcome::Completed,
    )
    .unwrap();
    let mut items = base.items().to_vec();
    items[0].0 = ItemId::new(99);
    let Item::ToolCall(call) = &mut items[0].1 else {
        unreachable!()
    };
    call.call_id = text("structured-proposal");
    call.arguments =
        ToolArguments::Structured(StructuredValue::new(serde_json::json!({"n":1})).unwrap());
    let structured = base.clone().with_items(items).unwrap();
    let progress = base
        .clone()
        .with_progress(InteractionProgress::AwaitingToolResults)
        .unwrap();
    let mut usage = Usage::operation(2, 3, 5);
    usage.scope = UsageScope::Session;
    let scoped = base.with_usage(usage).unwrap();
    let replay = GenerationResponse::new(
        vec![(
            ItemId::new(1),
            Item::Reasoning(ReasoningItem {
                status: ItemLifecycle::Completed,
                parts: vec![],
                replay: Some(ReplayValue::final_value(
                    ReplayFormat::GoogleInteractionsV1Thought,
                    text("opaque"),
                )),
            }),
        )],
        Outcome::Completed,
    )
    .unwrap();
    let grouped = GenerationResponse::new(progress.items().to_vec(), Outcome::Completed)
        .unwrap()
        .with_replay_groups(vec![
            ReplayGroup::new(GroupId::new(LocalScope::ROOT, 1), vec![ItemId::new(1)]).unwrap(),
        ])
        .unwrap();
    let fidelity = crate::protocol::fidelity::FidelityRecords::default();
    let metadata = ResponseMetadata {
        id: "r".into(),
        model: "synthetic".into(),
        created: Some(1.into()),
        context: Default::default(),
        instruction_fidelity: Default::default(),
    };
    let provider = GenerationResponse::new(
        vec![(
            ItemId::new(1),
            Item::ProviderTool(ProviderToolObservation {
                replay: None,
                source: NativeAliasDomain {
                    source: text("abstract"),
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
                output: None,
                artifact_status: None,
            }),
        )],
        Outcome::Completed,
    )
    .unwrap();
    for semantic in [
        &structured,
        &progress,
        &scoped,
        &replay,
        &grouped,
        &provider,
    ] {
        for profile in [Profile::Chat, Profile::Responses] {
            let forged = ResponseRepresentation {
                semantic: std::borrow::Cow::Borrowed(semantic),
                projection: vec![],
                fidelity: &fidelity,
                metadata: std::borrow::Cow::Borrowed(&metadata),
                profile,
                adaptation: Default::default(),
            };
            let result = match profile {
                Profile::Chat => static_response::encode_chat(&forged),
                Profile::Responses => static_response::encode_responses(&forged),
            };
            assert!(matches!(result, Err(CodecError::Unsupported(_))));
        }
    }
}

fn text(value: &str) -> Text {
    Text::new(value, "synthetic", 256).unwrap()
}
fn resource_table(url: &str) -> ResourceTable {
    ResourceTable::new(vec![(
        ResourceId::new(1),
        ResourceDeclaration {
            body: ResourceBody::Media(ResourceLocation::Url(text(url))),
            conditions: Default::default(),
        },
    )])
    .unwrap()
}
#[test]
fn forged_handles_cannot_drop_owner_local_replay_or_assistant_media() {
    let replay = ReplayValue::final_value(
        ReplayFormat::GoogleGenerateContentPart,
        text("synthetic-opaque"),
    );
    let mut items = vec![Item::ToolCall(ToolCall {
        call_id: text("C"),
        name: text("lookup"),
        arguments: "{}".into(),
        status: ItemLifecycle::Completed,
        context: CallContext {
            replay: Some(replay.clone()),
            ..Default::default()
        },
    })];
    for (content, attachment) in [
        (ContentPart::Text(text("text").into()), Some(replay)),
        (
            ContentPart::Resource(ResourceUse {
                id: ResourceId::new(1),
                purpose: ResourcePurpose::Output,
                description: ResourceDescription::Image { detail: None },
            }),
            None,
        ),
    ] {
        items.push(Item::Message(Message {
            role: MessageRole::Assistant,
            status: ItemLifecycle::Completed,
            phase: None,
            parts: vec![Part {
                id: PartId::new(1),
                content,
                replay: attachment,
            }],
        }));
    }
    let fidelity = FidelityRecords::default();
    let metadata = ResponseMetadata {
        id: "r".into(),
        model: "synthetic".into(),
        created: Some(1.into()),
        context: Default::default(),
        instruction_fidelity: Default::default(),
    };
    for item in items {
        let source = GenerationRequest::from_resources(
            vec![(ItemId::new(1), item)],
            GenerationSettings::default(),
            resource_table("https://example.test/image.png"),
        )
        .unwrap();
        let response = GenerationResponse::from_resources(
            source.items().to_vec(),
            Outcome::Completed,
            source.resources().clone(),
        )
        .unwrap();
        for profile in [Profile::Chat, Profile::Responses] {
            let forged = RequestRepresentation {
                semantic: std::borrow::Cow::Borrowed(&source),
                projection: vec![],
                fidelity: &fidelity,
                profile,
                adaptation: Default::default(),
            };
            assert!(
                match profile {
                    Profile::Chat => chat::encode_generation(&forged),
                    Profile::Responses => responses::encode_generation(&forged),
                }
                .is_err()
            );
            let forged = ResponseRepresentation {
                semantic: std::borrow::Cow::Borrowed(&response),
                projection: vec![],
                fidelity: &fidelity,
                profile,
                adaptation: Default::default(),
                metadata: std::borrow::Cow::Borrowed(&metadata),
            };
            assert!(
                match profile {
                    Profile::Chat => static_response::encode_chat(&forged),
                    Profile::Responses => static_response::encode_responses(&forged),
                }
                .is_err()
            );
        }
    }
}
#[test]
fn request_codecs_enforce_result_carriers_before_rendering() {
    let outputs = [
        (
            ToolOutput::Structured(StructuredValue::new(serde_json::json!({"value":42})).unwrap()),
            None,
        ),
        (
            ToolOutput::Text("failed".into()),
            Some(ToolExecution::Failed { code: None }),
        ),
        (
            ToolOutput::Text("success".into()),
            Some(ToolExecution::Succeeded),
        ),
        (
            ToolOutput::Parts(vec![(
                PartId::new(1),
                ToolResultPart::Resource(ResourceUse {
                    id: ResourceId::new(1),
                    purpose: ResourcePurpose::ToolResult,
                    description: ResourceDescription::Image { detail: None },
                }),
            )]),
            None,
        ),
    ];
    for (output, execution) in outputs {
        let semantic = GenerationRequest::from_resources(
            vec![
                (
                    ItemId::new(1),
                    Item::ToolCall(ToolCall {
                        call_id: text("c"),
                        name: text("lookup"),
                        arguments: "{}".into(),
                        status: ItemLifecycle::Completed,
                        context: CallContext::default(),
                    }),
                ),
                (
                    ItemId::new(2),
                    Item::ToolResult(ToolResult {
                        is_error: None,
                        execution,
                        call_id: text("c"),
                        output,
                        status: None,
                        context: CallContext::default(),
                    }),
                ),
            ],
            GenerationSettings::default(),
            resource_table("https://example.invalid/image.png"),
        )
        .unwrap();
        let media = matches!(&semantic.items()[1].1, Item::ToolResult(r) if matches!(r.output, ToolOutput::Parts(_)));
        let fidelity = FidelityRecords::default();
        for profile in [Profile::Chat, Profile::Responses] {
            let forged = RequestRepresentation {
                semantic: std::borrow::Cow::Borrowed(&semantic),
                projection: vec![],
                fidelity: &fidelity,
                profile,
                adaptation: Default::default(),
            };
            let result = match profile {
                Profile::Chat => chat::encode_generation(&forged),
                Profile::Responses => responses::encode_generation(&forged),
            };
            if media && profile == Profile::Responses {
                assert_eq!(
                    result.unwrap()["input"][1]["output"],
                    serde_json::json!([
                        {"type":"input_image","image_url":"https://example.invalid/image.png"}
                    ])
                );
                let mut breakpoint = FidelityRecords::default();
                breakpoint.record_cache_breakpoint(PartId::new(1)).unwrap();
                let forged = RequestRepresentation {
                    semantic: std::borrow::Cow::Borrowed(&semantic),
                    projection: vec![],
                    fidelity: &breakpoint,
                    profile,
                    adaptation: Default::default(),
                };
                assert!(matches!(
                    responses::encode_generation(&forged),
                    Err(CodecError::Unsupported(_))
                ));
            } else {
                assert!(matches!(result, Err(CodecError::Unsupported(_))));
            }
        }
    }
}
