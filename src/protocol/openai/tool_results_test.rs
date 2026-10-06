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
                message: None,
                status: ItemLifecycle::Completed,
                context: CallContext::default(),
            }),
        )],
        Outcome::Completed,
    )
    .unwrap();
    let mut items = base.items().to_vec();
    let Item::ToolCall(call) = &mut items[0].1 else {
        unreachable!()
    };
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
    let fidelity = crate::protocol::fidelity::FidelityRecords::default();
    let metadata = ResponseMetadata {
        id: "r".into(),
        model: "synthetic".into(),
        created: 1.into(),
        context: Default::default(),
        instruction_fidelity: Default::default(),
    };
    for semantic in [&structured, &progress, &scoped, &replay] {
        for profile in [Profile::Chat, Profile::Responses] {
            let forged = ResponseRepresentation {
                semantic,
                fidelity: &fidelity,
                metadata: &metadata,
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
                ToolResultPart::Resource(Resource {
                    location: ResourceLocation::Url(text("https://example.invalid/image.png")),
                    description: ResourceDescription::Image { detail: None },
                }),
            )]),
            None,
        ),
    ];
    for (output, execution) in outputs {
        let semantic = GenerationRequest::new(
            vec![
                (
                    ItemId::new(1),
                    Item::ToolCall(ToolCall {
                        call_id: text("c"),
                        name: text("lookup"),
                        arguments: "{}".into(),
                        message: None,
                        status: ItemLifecycle::Completed,
                        context: CallContext::default(),
                    }),
                ),
                (
                    ItemId::new(2),
                    Item::ToolResult(ToolResult {
                        execution,
                        call_id: text("c"),
                        output,
                        status: None,
                        context: CallContext::default(),
                    }),
                ),
            ],
            GenerationControls::default(),
        )
        .unwrap();
        let media = matches!(&semantic.items()[1].1, Item::ToolResult(r) if matches!(r.output, ToolOutput::Parts(_)));
        let fidelity = FidelityRecords::default();
        for profile in [Profile::Chat, Profile::Responses] {
            let forged = RequestRepresentation {
                semantic: &semantic,
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
                    semantic: &semantic,
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
