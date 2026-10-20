//! Model restrictions apply after projection, not to protocol-wide carriers.
use morphiecore::{
    adapter::{AdapterError, Request},
    execution::{
        AttemptError,
        plan::{RejectionReason, representable},
        prepare,
    },
    protocol::{
        CodecError,
        model_limits::ModelLimits,
        openai::{Adapter, Dialect, Profile, TargetCaps},
        reasoning::{
            ReasoningConversion, ReasoningEffortMapping, ReasoningRequestPolicy, SHARED_BUDGETS,
        },
    },
    provider::SecretMaterial,
    semantic::task::generation::*,
    topology::{Endpoint, catalog::GOOGLE_BINDINGS},
};
use serde_json::{Value, json};

fn source(value: Value) -> Request {
    Adapter::new(Profile::Responses, Dialect::Standard, None)
        .decode_request(value.to_string().as_bytes())
        .unwrap()
}

fn rejected(endpoint: &Endpoint, request: &Request, reason: &str) {
    let before = request.clone();
    assert!(matches!(endpoint.encode_request(request),
        Err(AdapterError::Codec(CodecError::Unsupported(value))) if value == reason));
    assert_eq!(
        representable(endpoint, request),
        Err(RejectionReason::Representation)
    );
    assert!(matches!(prepare(endpoint, &GOOGLE_BINDINGS[0].provider(),
        &SecretMaterial::new("synthetic").unwrap(), request),
        Err(AttemptError::Adapter(AdapterError::Codec(CodecError::Unsupported(value)))) if value == reason));
    assert_eq!(request, &before);
}

#[test]
fn gemini_model_values_are_checked_by_both_preflight_and_preparation() {
    let binding = &GOOGLE_BINDINGS[0];
    for mut endpoint in [binding.endpoint(), binding.interactions_endpoint()] {
        let request = source(json!({"model":binding.model,"input":"go",
            "reasoning":{"effort":"high"},"max_output_tokens":65536}));
        let before = request.clone();
        assert_eq!(representable(&endpoint, &request), Ok(()));
        let prepared = prepare(
            &endpoint,
            &binding.provider(),
            &SecretMaterial::new("synthetic").unwrap(),
            &request,
        )
        .unwrap();
        let wire: Value = serde_json::from_slice(&prepared.body).unwrap();
        if endpoint.protocol == morphiecore::topology::ProtocolProfile::GoogleGenerateContent {
            assert_eq!(
                wire["generationConfig"],
                json!({"maxOutputTokens":65536,"thinkingConfig":{"thinkingLevel":"HIGH"}})
            );
        } else {
            assert_eq!(
                wire["generation_config"],
                json!({"max_output_tokens":65536,"thinking_level":"high"})
            );
        }
        assert_eq!(request, before);
        let minimal = source(json!({"model":binding.model,"input":"go",
            "reasoning":{"effort":"minimal"},"max_output_tokens":4096}));
        rejected(&endpoint, &minimal, "target effort");
        let excessive =
            source(json!({"model":binding.model,"input":"go","max_output_tokens":65537}));
        rejected(&endpoint, &excessive, "target maximum output tokens");

        // Restrictions belong to this binding, not the generic Google carriers.
        *endpoint.representation.model_limits_mut() = ModelLimits::default();
        let wire = endpoint.encode_request(&minimal).unwrap();
        if endpoint.protocol == morphiecore::topology::ProtocolProfile::GoogleGenerateContent {
            assert_eq!(
                wire["generationConfig"]["thinkingConfig"]["thinkingLevel"],
                "MINIMAL"
            );
        } else {
            assert_eq!(wire["generation_config"]["thinking_level"], "minimal");
        }
        assert!(endpoint.encode_request(&excessive).is_ok());
    }
}

#[test]
fn gemini_checks_the_converted_effort_without_requantizing() {
    let binding = &GOOGLE_BINDINGS[0];
    let plain = source(json!({"model":binding.model,"input":"go","max_output_tokens":4096}));
    let mut settings = plain.task.semantic.settings().clone();
    settings.reasoning = ReasoningRequest::present(None);
    settings.reasoning.budget_tokens = Some(1024);
    let mut request = plain.clone();
    request.task.semantic = request.task.semantic.with_settings(settings).unwrap();
    let before = request.clone();
    for mut endpoint in [binding.endpoint(), binding.interactions_endpoint()] {
        let policy = Some(ReasoningRequestPolicy {
            canonical_model: binding.model.into(),
            rule: "synthetic.budget-to-effort.v1",
            conversion: ReasoningConversion::BudgetToEffort {
                scope: EffortScope::Reasoning,
                table: SHARED_BUDGETS.clone(),
            },
        });
        match &mut endpoint.representation {
            morphiecore::lowering::target::TargetCaps::GoogleGenerateContent(c) => {
                c.reasoning_policy = policy
            }
            morphiecore::lowering::target::TargetCaps::GoogleInteractions(c) => {
                c.reasoning_policy = policy
            }
            _ => unreachable!(),
        }
        rejected(&endpoint, &request, "target effort");
        let mut settings = request.task.semantic.settings().clone();
        settings.reasoning.budget_tokens = Some(2048);
        let mut low = request.clone();
        low.task.semantic = low.task.semantic.with_settings(settings).unwrap();
        let (wire, rules) = endpoint.encode_request_with_rules(&low).unwrap();
        assert_eq!(
            rules
                .iter()
                .filter(|r| **r == "synthetic.budget-to-effort.v1")
                .count(),
            1
        );
        if endpoint.protocol == morphiecore::topology::ProtocolProfile::GoogleGenerateContent {
            assert_eq!(
                wire["generationConfig"]["thinkingConfig"]["thinkingLevel"],
                "LOW"
            );
        } else {
            assert_eq!(wire["generation_config"]["thinking_level"], "low");
        }
        assert!(endpoint.encode_request(&plain).is_ok());
    }
    assert_eq!(request, before);
}

#[test]
fn openai_limits_check_calibrated_levels_and_do_not_insert_caps() {
    let request = source(json!({"model":"public","input":"go","reasoning":{"effort":"medium"}}));
    let before = request.clone();
    for profile in [Profile::Chat, Profile::Responses] {
        let mut adapter = Adapter::new(profile, Dialect::Standard, None);
        adapter.adaptation.effort_mapping = Some(&ReasoningEffortMapping {
            canonical_model: "synthetic",
            rule: "synthetic.effort.v1",
            scope: EffortScope::Reasoning,
            native_levels: &[EffortLevel::High],
            encode: &[
                (EffortLevel::Medium, EffortLevel::High),
                (EffortLevel::High, EffortLevel::High),
            ],
            decode: &[(EffortLevel::High, EffortLevel::Medium)],
        });
        let mut caps = TargetCaps::full();
        caps.model_limits = ModelLimits {
            allowed_effort: Some(vec![EffortLevel::High].into()),
            maximum_output_tokens: Some(32),
        };
        let (wire, rules) = adapter
            .encode_request_with_rules(&request, "synthetic", &caps)
            .unwrap();
        assert_eq!(
            rules
                .iter()
                .filter(|r| **r == "synthetic.effort.v1")
                .count(),
            1
        );
        if profile == Profile::Chat {
            assert_eq!(wire["reasoning_effort"], "high");
            assert!(wire.get("max_completion_tokens").is_none());
        } else {
            assert_eq!(wire["reasoning"]["effort"], "high");
            assert!(wire.get("max_output_tokens").is_none());
        }
        caps.model_limits.allowed_effort = Some(vec![EffortLevel::Medium].into());
        assert!(matches!(
            adapter.encode_request(&request, "synthetic", &caps),
            Err(AdapterError::Representation(
                morphiecore::lowering::generation::RepresentationError::Controls
            ))
        ));
        caps.model_limits.allowed_effort = Some(vec![EffortLevel::High].into());
        let mut capped = request.clone();
        let mut settings = capped.task.semantic.settings().clone();
        settings.controls.max_output_tokens = Some(33);
        capped.task.semantic = capped.task.semantic.with_settings(settings).unwrap();
        assert!(matches!(
            adapter.encode_request(&capped, "synthetic", &caps),
            Err(AdapterError::Representation(
                morphiecore::lowering::generation::RepresentationError::Controls
            ))
        ));
        let mut settings = capped.task.semantic.settings().clone();
        settings.controls.max_output_tokens = Some(32);
        capped.task.semantic = capped.task.semantic.with_settings(settings).unwrap();
        let wire = adapter.encode_request(&capped, "synthetic", &caps).unwrap();
        assert_eq!(
            wire[if profile == Profile::Chat {
                "max_completion_tokens"
            } else {
                "max_output_tokens"
            }],
            32
        );
    }
    assert_eq!(request, before);
}
