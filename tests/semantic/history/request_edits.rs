//! A01: validated edits must uphold the constructor's guarantee.
use morphiecore::semantic::{task::generation::*, value::Text};

fn request() -> GenerationRequest {
    GenerationRequest::from_settings(
        vec![],
        GenerationSettings {
            instructions: Some(Text::new("synthetic", "instruction", 64).unwrap().into()),
            parallel_tool_calls: Some(false),
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn reasoning_edit_must_not_return_an_invalid_request() {
    let mut reasoning = ReasoningRequest::absent();
    reasoning.activation = Some(ReasoningActivation::Disabled);
    let edited = request().with_reasoning(reasoning);
    assert_eq!(edited.unwrap_err(), GenerationError::InvalidControl);
}
