//! Independent control intents, not generated facts or approximate wire mappings.
use morphiecore::{
    lowering::generation::RepresentationError,
    protocol::{
        fidelity::FidelityRecords,
        openai::{Profile, TargetCaps, lowering::lower_request},
    },
    semantic::{
        task::generation::*,
        value::{Presence, Text},
    },
};

#[test]
fn each_new_control_independently_rejects_request_echo_and_stream_projection() {
    use morphiecore::protocol::openai::{events::EventEncoder, lowering::lower_response};
    for dimension in 0..5 {
        let mut control = ReasoningRequest::present(Some(ReasoningEffort::High), None);
        match dimension {
            0 => {
                control.budget = Some(ReasoningBudget {
                    hard_limit: Some(1024),
                    soft_target: None,
                })
            }
            1 => {
                control.budget = Some(ReasoningBudget {
                    hard_limit: None,
                    soft_target: Some(1024),
                })
            }
            2 => control.display = Presence::Value(ReasoningDisplay::Omitted),
            3 => control.display = Presence::Value(ReasoningDisplay::Summary),
            _ => control.mode = Presence::Value(ReasoningMode::Adaptive),
        }
        let settings = GenerationSettings {
            instructions: Presence::Value(Text::new("hello", "test", 32).unwrap()),
            reasoning: control,
            ..Default::default()
        };
        let request = GenerationRequest::from_settings(vec![], settings.clone()).unwrap();
        let mut metadata = crate::events_support::metadata();
        metadata.context.settings = Some(settings);
        let response = GenerationResponse::new(vec![], Outcome::Completed).unwrap();
        let fidelity = FidelityRecords::default();
        for profile in [Profile::Responses, Profile::Chat] {
            assert_eq!(
                lower_request(&request, &fidelity, profile, TargetCaps::full()).err(),
                Some(RepresentationError::Reasoning)
            );
            assert_eq!(
                lower_response(&response, &fidelity, &metadata, profile, TargetCaps::full()).err(),
                Some(RepresentationError::Reasoning)
            );
        }
        let mut encoder = EventEncoder::new(Profile::Responses, metadata).unwrap();
        assert!(encoder.encode(&StreamEvent::Started, &fidelity).is_err());
        assert!(encoder.encode(&StreamEvent::Started, &fidelity).is_err());
    }
}
