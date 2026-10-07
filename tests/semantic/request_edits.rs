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

#[test]
fn legal_edits_preserve_presence_and_other_fields() {
    let source = request();
    let edited = source
        .clone()
        .with_output(OutputConstraint::JsonObject)
        .unwrap();
    assert_eq!(edited.items(), source.items());
    assert_eq!(edited.instructions(), source.instructions());
    assert_eq!(edited.parallel_tool_calls(), Some(false));
    assert!(edited.text_options().presence);
    assert_eq!(
        edited.text_options().format,
        Presence::Value(OutputConstraint::JsonObject)
    );
    let reasoning = ReasoningRequest::present(None, None).with_encrypted_output(true);
    let edited = edited.with_reasoning(reasoning.clone()).unwrap();
    assert_eq!(edited.reasoning(), &reasoning);
    assert_eq!(edited.reasoning().effort, Presence::Absent);
    assert_eq!(edited.reasoning().summary, Presence::Absent);
    assert_eq!(source.text_options(), &TextOptions::default());
    assert_eq!(source.reasoning(), &ReasoningRequest::absent());
}

#[test]
fn output_edit_checks_combined_history_and_settings_budget() {
    for (remaining, accepted) in [(1, true), (0, true), (-1, false)] {
        let output = OutputConstraint::JsonSchema {
            name: Text::new("a", "schema name", 64).unwrap(),
            description: None,
            schema: json!({"type":"string"}).into(),
            strict: Some(false),
        };
        let mut settings = GenerationSettings::default();
        settings.text.presence = true;
        settings.text.format = Presence::Value(output.clone());
        let settings_bytes = settings.validate().unwrap();
        let length = (MAX_TOTAL_BYTES as isize - settings_bytes as isize - remaining) as usize;
        let mut left = length;
        let mut parts = vec![];
        while left > 0 {
            let count = left.min(MAX_TEXT_BYTES);
            parts.push(Part {
                id: PartId::new(parts.len() as u64),
                content: ContentPart::Text(
                    Text::new("x".repeat(count), "synthetic", MAX_TEXT_BYTES)
                        .unwrap()
                        .into(),
                ),
            });
            left -= count;
        }
        let source = GenerationRequest::new(
            vec![(
                ItemId::new(1),
                Item::Message(Message {
                    role: MessageRole::User,
                    parts,
                    status: ItemLifecycle::Completed,
                    phase: None,
                }),
            )],
            GenerationControls::default(),
        )
        .unwrap();
        let edited = source.with_output(output);
        assert_eq!(edited.is_ok(), accepted, "remaining bytes: {remaining}");
        if let Err(error) = edited {
            assert_eq!(error, GenerationError::Limit);
        }
    }
}
