//! A01: validated edits must uphold the constructor's guarantee.
use morphiecore::semantic::{
    task::generation::*,
    value::{Presence, Text},
};
use serde_json::json;

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
fn output_edit_must_not_return_an_invalid_request() {
    let edited = request().with_output(OutputConstraint::JsonSchema {
        name: Text::new("answer", "schema name", 64).unwrap(),
        description: None,
        schema: json!({"type":"unknown"}).into(),
        strict: Some(false),
    });
    assert_eq!(edited.unwrap_err(), GenerationError::InvalidSchema);
}

#[test]
fn reasoning_edit_must_not_return_an_invalid_request() {
    let mut reasoning = ReasoningRequest::absent();
    reasoning.effort = Presence::Value(ReasoningEffort::High);
    let edited = request().with_reasoning(reasoning);
    assert_eq!(edited.unwrap_err(), GenerationError::InvalidControl);
}
