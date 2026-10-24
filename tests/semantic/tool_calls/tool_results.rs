//! Tool result kind, artifact lifecycle and wire representability are independent.
use crate::events_support::text;
use crate::resources_support as resources;
use morphiecore::semantic::task::generation::*;
use serde_json::json;
fn call() -> (ItemId, Item) {
    (
        ItemId::new(1),
        Item::ToolCall(ToolCall {
            replay: None,
            call_id: text("c"),
            cache: None,
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
            .map(|(id, resource)| (ResourceId::new(id), resource.location))
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
                    cache: None,
                    call_id: Some(text("c")),
                    name: None,
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
        description: ResourceDescription::Image {
            detail: None,
            size_policy: ImageSizePolicy::AllowDownsize,
        },
    }
}
fn media(url: &str) -> Payload {
    (
        ToolOutput::Parts(vec![
            Part::new(PartId::new(10), InputPart::Text(text("caption"))),
            Part::new(PartId::new(11), InputPart::Resource(resources::image(1))),
        ]),
        vec![(1, image(url))],
    )
        .into()
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
    use morphiecore::protocol::{
        fidelity::FidelityRecords,
        openai::{Profile, TargetCaps, lowering::lower_request, responses},
    };
    let oracle = json!({"input":[
            {"type":"function_call","call_id":"c","name":"lookup","arguments":"{}"},
            {"type":"function_call_output","call_id":"c","output":[
                {"type":"input_text","text":"caption"},
                {"type":"input_image","image_url":"https://example.invalid/a.png"}]}]});
    let decoded = responses::decode_generation(&oracle).unwrap();
    let Item::ToolResult(result) = &decoded.semantic.items()[1].1 else {
        panic!("tool result")
    };
    let ToolOutput::Parts(parts) = &result.output else {
        panic!("parts")
    };
    assert_eq!(result.required_call_id().unwrap(), "c");
    assert!(matches!(&parts[0].content, InputPart::Text(t) if t.as_str() == "caption"));
    let InputPart::Resource(resource) = &parts[1].content else {
        panic!("image")
    };
    let view = resource.media(decoded.semantic.resources()).unwrap();
    assert_eq!(view.kind(), ResourceKind::Image);
    assert!(
        matches!(view.location, ResourceLocation::Url(u) if u.as_str() == "https://example.invalid/a.png")
    );
    // Encode independently built IR, not the decoded request.
    let outbound = request(media("https://example.invalid/a.png"), None).unwrap();
    let fidelity = FidelityRecords::default();
    let target =
        lower_request(&outbound, &fidelity, Profile::Responses, TargetCaps::full()).unwrap();
    assert_eq!(
        responses::encode_generation(&target).unwrap()["input"],
        oracle["input"]
    );
    let history = request(
        media("https://example.invalid/a.png"),
        Some(ItemLifecycle::Incomplete),
    )
    .unwrap();
    let requirements = GenerationRequirements::derive(&history);
    assert_eq!(requirements.tool_result_media, 1);
    assert_eq!(requirements.image_inputs, 1);
    let mut text_only = GenerationSemanticContract::full();
    text_only.image_input = false;
    assert_eq!(
        text_only.check(&history),
        Err(GenerationFeature::ImageInput)
    );
    assert!(
        request(
            ToolOutput::Parts(vec![
                Part::new(PartId::new(10), InputPart::Text(text("one"))),
                Part::new(PartId::new(10), InputPart::Text(text("two")))
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
                ToolOutput::Parts(vec![Part::new(
                    PartId::new(10),
                    InputPart::Resource(resources::image(1))
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
    ] {
        let wire = json!({"input":[
            {"type":"function_call","call_id":"c","name":"lookup","arguments":"{}"},
            {"type":"function_call_output","call_id":"c","output":[part]}
        ]});
        assert!(responses::decode_generation(&wire).is_err());
    }
}
