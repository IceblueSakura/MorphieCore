//! Tool result kind, artifact lifecycle and wire representability are independent.
use crate::events_support::text;
use crate::resources_support as resources;
use morphiecore::{
    lowering::generation::RepresentationError,
    protocol::{
        fidelity::FidelityRecords,
        openai::{Profile, TargetCaps as Contract, lowering::lower_request},
    },
    semantic::task::generation::*,
};
use serde_json::json;
fn call() -> (ItemId, Item) {
    (
        ItemId::new(1),
        Item::ToolCall(ToolCall {
            call_id: text("c"),
            name: text("lookup"),
            arguments: "{}".into(),
            status: ItemLifecycle::Completed,
            context: CallContext::default(),
        }),
    )
}
struct Payload {
    value: ToolOutput,
    resources: Vec<(u64, Resource)>,
}
impl From<ToolOutput> for Payload {
    fn from(value: ToolOutput) -> Self {
        Self {
            value,
            resources: vec![],
        }
    }
}
impl From<(ToolOutput, Vec<(u64, Resource)>)> for Payload {
    fn from((value, resources): (ToolOutput, Vec<(u64, Resource)>)) -> Self {
        Self { value, resources }
    }
}
fn request(
    output: impl Into<Payload>,
    status: Option<ItemLifecycle>,
) -> Result<GenerationRequest, GenerationError> {
    request_with_execution(output, status, None)
}
fn request_with_execution(
    output: impl Into<Payload>,
    status: Option<ItemLifecycle>,
    execution: Option<ToolExecution>,
) -> Result<GenerationRequest, GenerationError> {
    let output = output.into();
    let table = ResourceTable::new(
        output
            .resources
            .into_iter()
            .map(|(id, resource)| {
                (
                    ResourceId::new(id),
                    ResourceDeclaration {
                        body: ResourceBody::Media(resource.location),
                        conditions: Default::default(),
                    },
                )
            })
            .collect(),
    )?;
    GenerationRequest::from_resources(
        vec![
            call(),
            (
                ItemId::new(2),
                Item::ToolResult(ToolResult {
                    is_error: None,
                    execution,
                    call_id: text("c"),
                    output: output.value,
                    status,
                    context: CallContext::default(),
                }),
            ),
        ],
        GenerationSettings::default(),
        table,
    )
}
fn image(url: &str) -> Resource {
    Resource {
        location: ResourceLocation::Url(text(url)),
        description: ResourceDescription::Image { detail: None },
    }
}
fn media(url: &str) -> Payload {
    (
        ToolOutput::Parts(vec![
            (PartId::new(10), ToolResultPart::Text(text("caption"))),
            (
                PartId::new(11),
                ToolResultPart::Resource(resources::tool(1)),
            ),
        ]),
        vec![(1, image(url))],
    )
        .into()
}
#[test]
fn structured_value_preserves_integer_precision_order_and_bounds() {
    let value: serde_json::Value =
        serde_json::from_str(r#"{"z":18446744073709551615,"a":null}"#).unwrap();
    let structured = StructuredValue::new(value.clone()).unwrap();
    assert_eq!(structured.value()["z"].as_u64(), Some(u64::MAX));
    assert_eq!(
        structured
            .value()
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["z", "a"]
    );
    let request = request(ToolOutput::Structured(structured), None).unwrap();
    assert!(GenerationRequirements::derive(&request).structured_tool_results);
    assert_eq!(request.continuation(), Continuation::Unreported);
    let mut deep = json!(null);
    for _ in 0..40 {
        deep = json!([deep]);
    }
    assert_eq!(
        StructuredValue::new(deep).unwrap_err(),
        GenerationError::Limit
    );
    assert!(StructuredValue::new(json!(vec![0; 8193])).is_err());
    assert!(StructuredValue::new(json!("x".repeat(MAX_TEXT_BYTES + 1))).is_err());
}
#[test]
fn execution_error_is_a_terminal_result_not_a_generation_failure() {
    let error = ToolExecution::Failed {
        code: Some(text("not_found")),
    };
    let payload = ToolOutput::Text("synthetic failure".into());
    assert!(
        request_with_execution(
            payload.clone(),
            Some(ItemLifecycle::InProgress),
            Some(error.clone())
        )
        .is_err()
    );
    let history =
        request_with_execution(payload, Some(ItemLifecycle::Completed), Some(error)).unwrap();
    let response = GenerationResponse::new(vec![call()], Outcome::Completed).unwrap();
    let exchange = ResponseContinuation::new(
        ResponseRelation::new(TurnId::new(1), ResponseId::new(1)),
        &response,
    );
    assert_eq!(
        exchange.inspect(&history).unwrap(),
        ResultReadiness::ResultsComplete
    );
    assert_eq!(response.outcome(), Outcome::Completed);
    assert!(GenerationRequirements::derive(&history).tool_execution_reports);
}
#[test]
fn ordered_media_has_unique_parts_and_distinct_requirements() {
    let history = request(
        media("https://example.invalid/a.png"),
        Some(ItemLifecycle::Incomplete),
    )
    .unwrap();
    let requirements = GenerationRequirements::derive(&history);
    assert_eq!(requirements.tool_result_images, 1);
    assert_eq!(requirements.image_inputs, 0);
    let proof = RequestDependencyProof::capture(
        &history,
        HistoryDependency::PrefixThrough(ItemId::new(2)),
        morphiecore::semantic::task::generation::SettingsDependency::All,
    )
    .unwrap();
    assert!(
        proof
            .check(
                &request(
                    media("https://example.invalid/b.png"),
                    Some(ItemLifecycle::Incomplete)
                )
                .unwrap()
            )
            .is_err()
    );
    assert!(
        request(
            ToolOutput::Parts(vec![
                (PartId::new(10), ToolResultPart::Text(text("one"))),
                (PartId::new(10), ToolResultPart::Text(text("two")))
            ]),
            None
        )
        .is_err()
    );
    assert!(request(media("https://user:secret@example.invalid/a.png"), None).is_err());
    let mut opaque = image("https://example.invalid/a.png");
    opaque.location = ResourceLocation::OpaqueReference(text("issuer-file"));
    assert!(
        request(
            (
                ToolOutput::Parts(vec![(
                    PartId::new(10),
                    ToolResultPart::Resource(resources::tool(1))
                )]),
                vec![(1, opaque)]
            ),
            None
        )
        .is_err()
    );
}

#[test]
fn tool_image_decode_rejects_wrong_branches_and_unsafe_resources() {
    use morphiecore::protocol::openai::responses;
    for part in [
        json!({"type":"input_image","image_url":"https://example.invalid/a.png","text":"cross-kind"}),
        json!({"type":"input_text","text":"caption","image_url":"https://example.invalid/a.png"}),
        json!({"type":"input_image","file_id":"issuer-file"}),
        json!({"type":"input_file","file_id":"issuer-file"}),
        json!({"type":"input_image","image_url":"https://example.invalid/a.png","detail":null}),
        json!({"type":"input_image","image_url":"https://user:secret@example.invalid/a.png"}),
        json!({"type":"input_image","image_url":"file:///etc/passwd"}),
        json!({"type":"input_image","image_url":"data:image/png;base64,???"}),
        json!({"type":"input_image","image_url":"https://example.invalid/a.png","prompt_cache_breakpoint":{"mode":"explicit"}}),
    ] {
        let wire = json!({"input":[
            {"type":"function_call","call_id":"c","name":"lookup","arguments":"{}"},
            {"type":"function_call_output","call_id":"c","output":[part]}
        ]});
        assert!(responses::decode_generation(&wire).is_err());
    }
}

#[test]
fn tool_and_user_images_share_target_limits_and_edits_remove_media() {
    use morphiecore::protocol::openai::responses;
    let mut items = request(media("https://example.invalid/a.png"), None)
        .unwrap()
        .items()
        .to_vec();
    items.insert(
        0,
        (
            ItemId::new(9),
            Item::Message(Message {
                role: MessageRole::User,
                status: ItemLifecycle::Completed,
                phase: None,
                parts: vec![Part {
                    replay: None,
                    id: PartId::new(90),
                    content: ContentPart::Resource(resources::input(2)),
                }],
            }),
        ),
    );
    let history = GenerationRequest::from_resources(
        items,
        GenerationSettings::default(),
        resources::table([
            (1, image("https://example.invalid/a.png")),
            (2, image("https://example.invalid/user.png")),
        ]),
    )
    .unwrap();
    let fidelity = FidelityRecords::default();
    let mut contract = Contract::full();
    contract.images.max_images = 1;
    assert_eq!(
        lower_request(&history, &fidelity, Profile::Responses, contract).err(),
        Some(RepresentationError::ImageInput)
    );
    let mut contract = Contract::full();
    contract.images.urls = false;
    assert_eq!(
        lower_request(&history, &fidelity, Profile::Responses, contract).err(),
        Some(RepresentationError::ImageInput)
    );
    let inline = request(
        (
            ToolOutput::Parts(vec![(
                PartId::new(10),
                ToolResultPart::Resource(ResourceUse {
                    description: ResourceDescription::Image {
                        detail: Some(ImageDetail::Low),
                    },
                    ..resources::tool(1)
                }),
            )]),
            vec![(
                1,
                Resource {
                    description: ResourceDescription::Image {
                        detail: Some(ImageDetail::Low),
                    },
                    location: ResourceLocation::Inline {
                        media_type: text("image/png"),
                        data_base64: text("AQID"),
                    },
                },
            )],
        ),
        None,
    )
    .unwrap();
    for limit in 0..3 {
        let mut contract = Contract::full();
        match limit {
            0 => contract.images.max_inline_bytes = 2,
            1 => contract.images.inline_formats.clear(),
            _ => contract.images.details.clear(),
        }
        assert_eq!(
            lower_request(&inline, &fidelity, Profile::Responses, contract).err(),
            Some(RepresentationError::ImageInput)
        );
    }
    let mut breakpoint = FidelityRecords::default();
    breakpoint.record_cache_breakpoint(PartId::new(10)).unwrap();
    assert_eq!(
        lower_request(&inline, &breakpoint, Profile::Responses, Contract::full()).err(),
        Some(RepresentationError::Controls)
    );
    let mut items = history.items().to_vec();
    let Item::ToolResult(result) = &mut items[2].1 else {
        panic!("result")
    };
    let ToolOutput::Parts(parts) = &mut result.output else {
        panic!("parts")
    };
    parts.retain(|(_, p)| matches!(p, ToolResultPart::Text(_)));
    let edited = history.with_items(items).unwrap();
    assert_eq!(
        GenerationRequirements::derive(&edited).tool_result_images,
        0
    );
    let target = lower_request(&edited, &fidelity, Profile::Responses, Contract::full()).unwrap();
    assert_eq!(
        responses::encode_generation(&target).unwrap()["input"][2]["output"],
        json!([{"type":"input_text","text":"caption"}])
    );
}
