//! Independent control intents, not generated facts or approximate wire mappings.
use morphiecore::{
    adapter::{Adapter, Dialect},
    lowering::generation::{GenerationRepresentationContract, RepresentationError, lower_request},
    protocol::{fidelity::FidelityRecords, openai::Profile},
    semantic::{
        task::generation::*,
        value::{Presence, Text},
    },
};
use serde_json::json;

fn adaptive() -> ReasoningRequest {
    let mut value = ReasoningRequest::present(Some(ReasoningEffort::High), None);
    value.mode = Presence::Value(ReasoningMode::Adaptive);
    value
}
#[test]
fn adaptive_effort_budget_and_display_have_independent_authority() {
    let mut control = adaptive();
    control.display = Presence::Value(ReasoningDisplay::Omitted);
    control.budget = Some(ReasoningBudget {
        hard_limit: Some(8192),
        soft_target: Some(4096),
    });
    control.validate().unwrap();
    assert_eq!(control.effort(), Some(ReasoningEffort::High));
    assert_eq!(control.budget.unwrap().hard_limit, Some(8192));
    let settings = GenerationSettings {
        instructions: Presence::Value(Text::new("hello", "test", 32).unwrap()),
        reasoning: control.clone(),
        ..Default::default()
    };
    let request = GenerationRequest::from_settings(vec![], settings).unwrap();
    for profile in [Profile::Responses, Profile::Chat] {
        assert_eq!(
            lower_request(
                &request,
                &FidelityRecords::default(),
                profile,
                GenerationRepresentationContract::full()
            )
            .err(),
            Some(RepresentationError::Reasoning)
        );
    }
    assert_eq!(request.reasoning(), &control);
    assert!(!control.encrypted_output());
}

#[test]
fn contradictory_controls_and_hidden_presence_fail_without_erasing_intent() {
    for budget in [
        ReasoningBudget {
            hard_limit: None,
            soft_target: None,
        },
        ReasoningBudget {
            hard_limit: Some(0),
            soft_target: None,
        },
        ReasoningBudget {
            hard_limit: None,
            soft_target: Some(0),
        },
        ReasoningBudget {
            hard_limit: Some(8),
            soft_target: Some(9),
        },
    ] {
        let mut value = adaptive();
        value.budget = Some(budget);
        assert_eq!(value.validate(), Err(GenerationError::InvalidControl));
    }
    let mut value = adaptive();
    value.mode = Presence::Value(ReasoningMode::Budgeted);
    assert!(value.validate().is_err());
    value.budget = Some(ReasoningBudget {
        hard_limit: None,
        soft_target: Some(1024),
    });
    value.validate().unwrap();
    value.effort = Presence::Value(ReasoningEffort::None);
    assert!(value.validate().is_err());
    value.effort = Presence::Absent;
    value.presence = ReasoningPresence::Absent;
    assert!(value.validate().is_err());
    value.presence = ReasoningPresence::Present;
    value.display = Presence::Value(ReasoningDisplay::Omitted);
    value.summary = Presence::Value(ReasoningSummary::Detailed);
    assert!(value.validate().is_err());
    value.summary = Presence::Value(ReasoningSummary::Disabled);
    value.validate().unwrap();
}

#[test]
fn numeric_budget_boundaries_preserve_hard_and_soft_meanings() {
    for n in [1, u64::MAX - 1, u64::MAX] {
        for budget in [
            ReasoningBudget {
                hard_limit: Some(n),
                soft_target: None,
            },
            ReasoningBudget {
                hard_limit: None,
                soft_target: Some(n),
            },
        ] {
            let mut control = ReasoningRequest::present(None, None);
            control.budget = Some(budget);
            control.validate().unwrap();
            assert_eq!(control.budget, Some(budget));
        }
    }
}

#[test]
fn adding_core_modes_does_not_expand_standard_request_echo_or_event_admission() {
    use morphiecore::protocol::openai::{events::EventDecoder, responses};
    let adapter = Adapter::new(Profile::Responses, Dialect::Standard, None);
    for control in [
        json!({"mode":"adaptive","effort":"high"}),
        json!({"mode":"budgeted"}),
        json!({"budget":{"hard_limit":1024}}),
        json!({"display":"omitted"}),
    ] {
        let request = json!({"model":"synthetic","input":"hello","reasoning":control});
        assert!(
            adapter
                .decode_request(request.to_string().as_bytes())
                .is_err()
        );
        let mut response = crate::events_support::envelope("completed", json!([]));
        response["reasoning"] = control.clone();
        assert!(responses::decode_response(&response).is_err());
        let mut event = crate::events_support::created();
        event["response"]["reasoning"] = control;
        assert!(EventDecoder::new(Profile::Responses).push(&event).is_err());
    }
}

#[test]
fn each_new_control_independently_rejects_request_echo_and_stream_projection() {
    use morphiecore::{
        lowering::generation::lower_response, protocol::openai::events::EventEncoder,
    };
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
                lower_request(
                    &request,
                    &fidelity,
                    profile,
                    GenerationRepresentationContract::full()
                )
                .err(),
                Some(RepresentationError::Reasoning)
            );
            assert_eq!(
                lower_response(
                    &response,
                    &fidelity,
                    &metadata,
                    profile,
                    GenerationRepresentationContract::full()
                )
                .err(),
                Some(RepresentationError::Reasoning)
            );
        }
        let mut encoder = EventEncoder::new(Profile::Responses, metadata).unwrap();
        assert!(encoder.encode(&StreamEvent::Started, &fidelity).is_err());
        assert!(encoder.encode(&StreamEvent::Started, &fidelity).is_err());
    }
}
