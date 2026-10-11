//! Tool result kind, artifact lifecycle and wire representability are independent.
use crate::events_support::text;
use crate::resources_support as resources;
use morphiecore::semantic::task::generation::*;
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
