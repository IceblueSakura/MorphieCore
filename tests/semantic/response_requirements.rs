//! Output requirements do not manufacture a request or inherit request controls.
use crate::events_support::{metadata, text};
use morphiecore::{
    lowering::generation::{GenerationRepresentationContract as Contract, lower_response},
    protocol::ResponseMetadata,
    protocol::{fidelity::FidelityRecords, openai::Profile},
    semantic::task::generation::*,
};
#[test]
fn response_requirements_are_derived_from_output_not_request_configuration() {
    let response = GenerationResponse::new(
        vec![(
            ItemId::new(1),
            Item::Message(Message {
                role: MessageRole::Assistant,
                parts: vec![Part {
                    id: PartId::new(2),
                    content: ContentPart::Text(text("answer").into()),
                }],
                status: ItemLifecycle::Completed,
                phase: None,
            }),
        )],
        Outcome::Completed,
    )
    .unwrap();
    let q = GenerationResponseRequirements::derive(&response);
    assert!(!q.tools && !q.reasoning && !q.text_metadata && !q.logprobs && !q.structured_arguments);
    let mut contract = Contract::full();
    contract.semantics.instructions = false;
    contract.semantics.temperature = false;
    contract.semantics.max_output_tokens = false;
    contract.semantics.structured_output = false;
    let m: ResponseMetadata = metadata();
    for profile in [Profile::Chat, Profile::Responses] {
        lower_response(
            &response,
            &FidelityRecords::default(),
            &m,
            profile,
            contract.clone(),
        )
        .unwrap();
    }
}
#[test]
fn self_describing_response_program_output_does_not_require_request_history() {
    let response = GenerationResponse::new(
        vec![(
            ItemId::new(1),
            Item::ProgramOutput(ProgramOutput {
                call_id: text("p"),
                result: "reported".into(),
                status: ItemLifecycle::Completed,
            }),
        )],
        Outcome::Completed,
    )
    .unwrap();
    assert!(
        GenerationRequest::new(response.items().to_vec(), GenerationControls::default()).is_err()
    );
    let fidelity = FidelityRecords::default();
    let metadata = metadata();
    let target = lower_response(
        &response,
        &fidelity,
        &metadata,
        Profile::Responses,
        Contract::full(),
    )
    .unwrap();
    let wire = morphiecore::protocol::openai::responses::encode_response(&target).unwrap();
    assert_eq!(
        wire["output"][0],
        serde_json::json!({"type":"program_output","id":"item_1","call_id":"p","result":"reported","status":"completed"})
    );
}

#[test]
fn output_requirements_still_enforce_tool_and_reasoning_domains() {
    let response = GenerationResponse::new(
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
    let q = GenerationResponseRequirements::derive(&response);
    assert!(q.tools);
    let mut semantic = GenerationSemanticContract::full();
    semantic.tools = false;
    assert_eq!(
        semantic.check_response(&response).err(),
        Some(GenerationFeature::Tools)
    );
    let changed = response.with_items(vec![]).unwrap();
    assert!(!GenerationResponseRequirements::derive(&changed).tools);
}
