use morphiecore::{
    protocol::{
        CodecError, DecodedRequest, GenerationRequestEnvelope as Request,
        anthropic::{
            self,
            messages::{Dialect, Profile, TargetContract},
        },
        fidelity::FidelityRecords,
        native::ReplayTarget,
    },
    semantic::{
        task::generation::*,
        value::{Presence, Text},
    },
};
use serde_json::{Value, json};
fn target() -> ReplayTarget {
    ReplayTarget::new("synthetic-native", "m").unwrap()
}
fn caps() -> TargetContract {
    let mut caps = TargetContract::new(Profile::AdaptiveTextTools, Dialect::Standard);
    caps.dispositions = Profile::AdaptiveTextTools.representation_contract();
    caps
}
fn typed(budget: Option<u64>) -> Request {
    let mut settings = GenerationSettings::default();
    settings.controls.max_output_tokens = Some(4096);
    if let Some(budget) = budget {
        settings.reasoning = ReasoningRequest::present(None);
        settings.reasoning.activation = Presence::Value(ReasoningActivation::Enabled);
        settings.reasoning.mode = Presence::Value(ReasoningMode::Budgeted);
        settings.reasoning.budget_tokens = Some(budget);
    }
    let value = Text::new("go", "test", 8).unwrap();
    let semantic = GenerationRequest::from_settings(
        vec![(
            ItemId::new(1),
            Item::Message(Message {
                participant: None,
                role: MessageRole::User,
                status: ItemLifecycle::Completed,
                phase: None,
                parts: vec![Part::new(
                    PartId::new(1),
                    ContentPart::Text(TextContent::new(value, vec![], Presence::Absent).unwrap()),
                )],
            }),
        )],
        settings,
    )
    .unwrap();
    Request::from_generation(
        DecodedRequest {
            evidence: Default::default(),
            semantic,
            fidelity: FidelityRecords::default(),
        },
        "m",
    )
    .unwrap()
}
#[test]
fn nullable_summary_preference_never_becomes_a_hidden_display_guarantee() {
    for budget in [None, Some(1024)] {
        for (visibility, summary, display) in [
            (Presence::Absent, Presence::Absent, None),
            (
                Presence::Value(ReasoningDisplay::Omitted),
                Presence::Absent,
                Some("omitted"),
            ),
            (
                Presence::Value(ReasoningDisplay::Summary),
                Presence::Value(ReasoningSummary::Auto),
                None,
            ),
            (Presence::Absent, Presence::Null, None),
        ] {
            let mut request = typed(budget);
            let mut reasoning = request.task.semantic.reasoning().clone();
            reasoning.presence = ReasoningPresence::Present;
            reasoning.activation = Presence::Value(ReasoningActivation::Enabled);
            reasoning.mode = Presence::Value(if budget.is_some() {
                ReasoningMode::Budgeted
            } else {
                ReasoningMode::Adaptive
            });
            reasoning.presentation.visibility = visibility;
            reasoning.presentation.summary = summary.clone();
            request.task.semantic = request.task.semantic.with_reasoning(reasoning).unwrap();
            let before = request.clone();
            let encoded = caps().encode_request(&request, &target());
            if summary == Presence::Null {
                assert!(encoded.is_err());
                assert!(
                    Profile::AdaptiveTextTools
                        .encode_generation_request(
                            &request.task.semantic,
                            &request.task.fidelity,
                            &target(),
                        )
                        .is_err()
                );
            } else {
                let encoded = encoded.unwrap();
                assert_eq!(
                    encoded.value["thinking"].get("display"),
                    display.map(|v| json!(v)).as_ref()
                );
            }
            assert_eq!(request, before);
        }
    }
}

#[test]
fn configured_fallback_never_replaces_supported_native_allocation_or_visibility() {
    use morphiecore::protocol::reasoning::{
        ReasoningConversion, ReasoningRequestPolicy, SHARED_BUDGETS,
    };
    let mut request = typed(Some(1024));
    let mut settings = request.task.semantic.settings().clone();
    settings.effort = Some(EffortControl {
        scope: EffortScope::Response,
        level: EffortLevel::Medium,
    });
    settings.reasoning.presentation.visibility = Presence::Value(ReasoningDisplay::Omitted);
    request.task.semantic = request.task.semantic.with_settings(settings).unwrap();
    let before = request.clone();
    let mut configured = caps();
    configured.reasoning_policy = Some(ReasoningRequestPolicy {
        canonical_model: "m".into(),
        rule: "synthetic.budget-to-effort.v1",
        conversion: ReasoningConversion::BudgetToEffort {
            scope: EffortScope::Response,
            table: SHARED_BUDGETS.clone(),
        },
    });
    let result = configured.encode_request(&request, &target()).unwrap();
    assert_eq!(
        result.value["thinking"],
        json!({"type":"enabled","budget_tokens":1024,"display":"omitted"})
    );
    assert_eq!(result.value["output_config"]["effort"], "medium");
    assert!(result.evidence.iter().all(|rule| !matches!(
        rule,
        morphiecore::protocol::conversion::generation::Effect::Reasoning { .. }
    )));
    assert_eq!(request, before);
    let mut settings = request.task.semantic.settings().clone();
    settings.reasoning.budget_tokens = None;
    settings.reasoning.mode = Presence::Absent;
    settings.reasoning.activation = Presence::Value(ReasoningActivation::Disabled);
    request.task.semantic = request.task.semantic.with_settings(settings).unwrap();
    configured.reasoning_policy.as_mut().unwrap().conversion =
        ReasoningConversion::PreserveDisabled;
    let result = configured.encode_request(&request, &target()).unwrap();
    assert_eq!(result.value["thinking"], json!({"type":"disabled"}));
    assert_eq!(result.value["output_config"]["effort"], "medium");
}

#[test]
fn model_maximum_checks_inserted_default_without_replacing_explicit_caps() {
    use morphiecore::protocol::ConversionFailure;
    assert_eq!(caps().default_output_tokens, None);
    let mut request = typed(None);
    let mut settings = request.task.semantic.settings().clone();
    settings.controls.max_output_tokens = None;
    request.task.semantic = request.task.semantic.with_settings(settings).unwrap();
    let before = request.clone();
    let mut limited = caps();
    assert!(matches!(limited.encode_request(&request, &target()),
        Err(ConversionFailure { cause: morphiecore::protocol::ConversionCause::Codec(CodecError::Unsupported(value)), .. })
            if value == morphiecore::protocol::failure::CarrierReason::Unavailable));
    limited.default_output_tokens = Some(32_000);
    limited.model_limits.maximum_output_tokens = Some(31_999);
    assert!(matches!(
        limited.encode_request(&request, &target()),
        Err(ConversionFailure {
            cause: morphiecore::protocol::ConversionCause::Codec(CodecError::Invalid(
                morphiecore::protocol::failure::ShapeReason::InvalidValue
            )),
            ..
        })
    ));
    limited.model_limits.maximum_output_tokens = Some(32_000);
    let projected = limited.encode_request(&request, &target()).unwrap();
    assert_eq!(projected.value["max_tokens"], 32_000);
    assert_eq!(
        projected
            .evidence
            .iter()
            .filter(|rule| rule.rule() == "messages.default-max-tokens.v1")
            .count(),
        0
    );
    assert_eq!(request, before);
    limited.default_output_tokens = Some(2048);
    limited.model_limits.maximum_output_tokens = Some(4096);
    let explicit = typed(None);
    let projected = limited.encode_request(&explicit, &target()).unwrap();
    assert_eq!(projected.value["max_tokens"], 4096);
    assert!(
        !projected
            .evidence
            .contains_rule("messages.default-max-tokens.v1")
    );
    limited.model_limits.maximum_output_tokens = Some(4095);
    let clamped = limited.encode_request(&explicit, &target()).unwrap();
    assert_eq!(clamped.value["max_tokens"], 4095);
    assert!(
        clamped
            .evidence
            .contains_rule("generation.clamp-output-to-model-maximum.v1")
    );
    limited.default_output_tokens = Some(0);
    assert!(matches!(
        limited.encode_request(&explicit, &target()),
        Err(ConversionFailure {
            cause: morphiecore::protocol::ConversionCause::Codec(CodecError::Invalid(
                morphiecore::protocol::failure::ShapeReason::InvalidValue
            )),
            ..
        })
    ));
    limited.default_output_tokens = None;
    limited.model_limits.maximum_output_tokens = Some(4096);
    assert_eq!(
        limited.encode_request(&explicit, &target()).unwrap().value["max_tokens"],
        4096
    );
    assert!(matches!(limited.encode_request(&request, &target()),
        Err(ConversionFailure { cause: morphiecore::protocol::ConversionCause::Codec(CodecError::Unsupported(value)), .. })
            if value == morphiecore::protocol::failure::CarrierReason::Unavailable));
}
#[test]
fn output_defaults_check_budgets_after_selected_conversions() {
    use morphiecore::protocol::{
        ConversionFailure,
        reasoning::{ReasoningConversion, ReasoningRequestPolicy, SHARED_BUDGETS},
    };
    let mut request = typed(Some(2048));
    let mut settings = request.task.semantic.settings().clone();
    settings.controls.max_output_tokens = None;
    request.task.semantic = request.task.semantic.with_settings(settings).unwrap();
    let before = request.clone();
    let mut target_caps = caps();
    target_caps.default_output_tokens = Some(2048);
    assert!(matches!(
        target_caps.encode_request(&request, &target()),
        Err(ConversionFailure {
            cause: morphiecore::protocol::ConversionCause::Representation(
                morphiecore::protocol::conversion::generation::RepresentationError::Admission(
                    Feature::ReasoningBudget
                )
            ),
            ..
        })
    ));
    target_caps.default_output_tokens = Some(2049);
    let projected = target_caps.encode_request(&request, &target()).unwrap();
    assert_eq!(projected.value["max_tokens"], 2049);
    assert_eq!(projected.value["thinking"]["budget_tokens"], 2048);

    // A numeric allocation removed by an approved mapping must not constrain
    // the subsequently inserted cap; the final effort still has its own guard.
    target_caps.default_output_tokens = Some(512);
    target_caps.reasoning_policy = Some(ReasoningRequestPolicy {
        canonical_model: "m".into(),
        rule: "synthetic.budget-to-effort.v1",
        conversion: ReasoningConversion::BudgetToEffort {
            scope: EffortScope::Response,
            table: SHARED_BUDGETS.clone(),
        },
    });
    target_caps.model_limits.allowed_effort = Some(vec![EffortLevel::Low].into());
    let projected = target_caps.encode_request(&request, &target()).unwrap();
    assert_eq!(projected.value["max_tokens"], 512);
    assert!(projected.value["thinking"].get("budget_tokens").is_none());
    assert_eq!(projected.value["output_config"]["effort"], "low");
    assert_eq!(
        projected
            .evidence
            .iter()
            .filter(|r| r.rule() == "messages.default-max-tokens.v1")
            .count(),
        0
    );
    target_caps.model_limits.allowed_effort = Some(vec![EffortLevel::High].into());
    assert_eq!(
        target_caps
            .encode_request(&request, &target())
            .unwrap()
            .value["output_config"]["effort"],
        "high"
    );
    assert_eq!(request, before);

    let mut settings = typed(None).task.semantic.settings().clone();
    settings.controls.max_output_tokens = None;
    settings.effort = Some(EffortControl {
        scope: EffortScope::Response,
        level: EffortLevel::Medium,
    });
    let mut qualitative = typed(None);
    qualitative.task.semantic = qualitative.task.semantic.with_settings(settings).unwrap();
    let original = qualitative.clone();
    target_caps.model_limits.allowed_effort = None;
    target_caps.reasoning_policy = Some(ReasoningRequestPolicy {
        canonical_model: "m".into(),
        rule: "synthetic.effort-to-budget.v1",
        conversion: ReasoningConversion::EffortToBudget {
            scope: EffortScope::Response,
            table: SHARED_BUDGETS.clone(),
        },
    });
    target_caps.default_output_tokens = Some(4096);
    let exact = target_caps.encode_request(&qualitative, &target()).unwrap();
    assert_eq!(exact.value["output_config"]["effort"], "medium");
    assert!(exact.value["thinking"].get("budget_tokens").is_none());
    assert!(exact.evidence.iter().all(|rule| !matches!(
        rule,
        morphiecore::protocol::conversion::generation::Effect::Reasoning { .. }
    )));
    assert_eq!(qualitative, original);
    // Only the non-native reasoning-scoped effort needs the configured numeric fallback.
    let mut settings = qualitative.task.semantic.settings().clone();
    settings.effort = Some(EffortControl {
        scope: EffortScope::Reasoning,
        level: EffortLevel::Medium,
    });
    qualitative.task.semantic = qualitative.task.semantic.with_settings(settings).unwrap();
    let original = qualitative.clone();
    target_caps.reasoning_policy.as_mut().unwrap().conversion =
        ReasoningConversion::EffortToBudget {
            scope: EffortScope::Reasoning,
            table: SHARED_BUDGETS.clone(),
        };
    assert!(matches!(
        target_caps.encode_request(&qualitative, &target()),
        Err(ConversionFailure {
            cause: morphiecore::protocol::ConversionCause::Representation(
                morphiecore::protocol::conversion::generation::RepresentationError::Admission(
                    Feature::ReasoningBudget
                )
            ),
            ..
        })
    ));
    target_caps.default_output_tokens = Some(4097);
    let projected = target_caps.encode_request(&qualitative, &target()).unwrap();
    assert_eq!(projected.value["thinking"]["budget_tokens"], 4096);
    assert_eq!(projected.value["max_tokens"], 4097);
    assert_eq!(qualitative, original);
}
#[test]
fn ordinary_budgets_preserve_numeric_intent_and_native_strict_relation() {
    let wire = json!({"model":"m","max_tokens":4096,"thinking":{"type":"enabled","budget_tokens":1024},
            "messages":[{"role":"user","content":"go"}]});
    let decoded =
        anthropic::messages::client::decode_client_request(wire.to_string().as_bytes()).unwrap();
    assert_eq!(decoded.task.semantic.reasoning().budget_tokens, Some(1024));
    assert_eq!(
        decoded.task.semantic.reasoning().mode,
        Presence::Value(ReasoningMode::Budgeted)
    );
    let request = typed(Some(1024));
    let before = request.clone();
    let native = caps().encode_request(&request, &target()).unwrap();
    assert_eq!(
        native.value["thinking"],
        json!({"type":"enabled","budget_tokens":1024})
    );
    assert_eq!(native.value["max_tokens"], 4096);
    assert!(native.evidence.iter().all(|r| !matches!(
        r,
        morphiecore::protocol::conversion::generation::Effect::Reasoning { .. }
    )));
    assert_eq!(request, before);
    for (budget, cap) in [
        (1023, 4096),
        (1024, 1024),
        (2048, 1024),
        (u64::MAX, u64::MAX),
    ] {
        let mut bad = wire.clone();
        bad["thinking"]["budget_tokens"] = json!(budget);
        bad["max_tokens"] = json!(cap);
        assert!(matches!(
            anthropic::messages::client::decode_client_request(bad.to_string().as_bytes()),
            Err(CodecError::Invalid(
                morphiecore::protocol::failure::ShapeReason::InvalidValue
            ))
        ));
        assert!(
            Profile::AdaptiveTextTools
                .decode_request(bad.to_string().as_bytes())
                .is_err()
        );
    }
    for budget in [1023, 4096] {
        assert!(
            caps()
                .encode_request(&typed(Some(budget)), &target())
                .is_err()
        );
    }
    let mut adaptive_only = caps();
    adaptive_only.dispositions.set_native(
        morphiecore::semantic::task::generation::Feature::ReasoningBudget,
        false,
    );
    let projected = adaptive_only.encode_request(&request, &target()).unwrap();
    assert_eq!(projected.value["thinking"]["type"], "adaptive");
    assert!(projected.value["thinking"].get("budget_tokens").is_none());
    assert_eq!(projected.value["output_config"]["effort"], "low");
}
#[test]
fn removed_sampling_omits_advisory_controls_without_changing_the_source() {
    let mut request = typed(None);
    let mut settings = request.task.semantic.settings().clone();
    settings.controls = settings
        .controls
        .with_temperature(0.25)
        .unwrap()
        .with_top_p(0.75)
        .unwrap();
    settings.controls.top_k = Some(7);
    request.task.semantic = request.task.semantic.with_settings(settings).unwrap();
    let source = request.clone();
    let profile = Profile::AdaptiveTextTools;
    let contract = profile.representation_contract();
    assert!(
        !contract.native(morphiecore::semantic::task::generation::Feature::Temperature)
            && !contract.native(morphiecore::semantic::task::generation::Feature::TopP)
            && !contract.native(morphiecore::semantic::task::generation::Feature::TopK)
    );
    let binding = &morphiecore::topology::catalog::MESSAGES_BINDINGS[0];
    assert!(
        binding
            .public_model()
            .contract
            .native(morphiecore::semantic::task::generation::Feature::Temperature)
    );
    assert!(
        !binding
            .endpoint()
            .representation
            .dispositions()
            .native(Feature::Temperature)
    );
    let projected = caps().encode_request(&request, &target()).unwrap();
    for (field, rule) in [
        ("temperature", "generation.omit-temperature.v1"),
        ("top_p", "generation.omit-top-p.v1"),
        ("top_k", "generation.omit-top-k.v1"),
    ] {
        assert!(projected.value.get(field).is_none());
        assert!(projected.evidence.contains_rule(rule));
    }
    let native = profile
        .encode_generation_request(&request.task.semantic, &Default::default(), &target())
        .unwrap();
    assert_eq!(native.value, projected.value);
    assert_eq!(request, source);
}
#[test]
fn native_identity_keeps_presence_and_bounds_with_selected_cross_projection() {
    for value in [Value::Null, json!(""), json!("用".repeat(512))] {
        let wire = json!({"model":"m","max_tokens":4096,"metadata":{"user_id":value},
                "messages":[{"role":"user","content":"go"}]});
        let decoded =
            anthropic::messages::client::decode_client_request(wire.to_string().as_bytes())
                .unwrap();
        let expected = if value.is_null() {
            None
        } else {
            Some(value.as_str().unwrap().into())
        };
        assert_eq!(decoded.context.identity.user, expected);
        let mut request = typed(None);
        request.context.identity.user = expected;
        let before = request.clone();
        let native = caps().encode_request(&request, &target()).unwrap();
        if value.is_null() {
            assert!(native.value.get("metadata").is_none());
        } else {
            assert_eq!(native.value["metadata"], json!({"user_id":value}));
        }
        assert!(
            !native
                .evidence
                .iter()
                .any(|r| r.rule() == "generation.omit-identity-hints.v1")
        );
        assert_eq!(request, before);
    }
    for metadata in [
        Value::Null,
        json!({"user_id":7}),
        json!({"user_id":"x".repeat(513)}),
    ] {
        let wire = json!({"model":"m","max_tokens":4096,"metadata":metadata,
                "messages":[{"role":"user","content":"go"}]});
        assert!(
            anthropic::messages::client::decode_client_request(wire.to_string().as_bytes())
                .is_err()
        );
    }
    let plain = caps().encode_request(&typed(None), &target()).unwrap();
    assert!(plain.value.get("metadata").is_none());
    let openai = morphiecore::protocol::openai::GenerationCodec::new(
        morphiecore::protocol::openai::GenerationApi::Chat,
        morphiecore::protocol::openai::Dialect::Standard,
        None,
    );
    let foreign = openai
        .decode_client_request(
            br#"{"model":"m","max_completion_tokens":4096,"messages":[{"role":"user","content":"go"}],"user":"legacy"}"#,
        )
        .unwrap();
    let encoded = caps().encode_request(&foreign, &target()).unwrap();
    assert_eq!(encoded.value["metadata"]["user_id"], "legacy");
    assert!(
        !encoded
            .evidence
            .iter()
            .any(|r| r.rule() == "generation.omit-identity-hints.v1")
    );
    assert!(
        encoded
            .evidence
            .iter()
            .any(|r| r.rule() == "cache.omit-legacy-user-bucketing.v1")
    );
}
