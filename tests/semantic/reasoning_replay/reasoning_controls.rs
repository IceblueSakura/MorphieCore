//! Directional request projections and independently validated incidental reports.
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
fn requests_reject_unselected_controls_while_incidental_reports_project() {
    use morphiecore::protocol::openai::{events::EventEncoder, lowering::lower_response};
    use morphiecore::protocol::reasoning::{
        ReasoningConversion, ReasoningRequestPolicy, SHARED_BUDGETS, ThinkingBudgets,
    };
    assert_eq!(
        SHARED_BUDGETS.entries.as_ref(),
        &[
            (EffortLevel::Minimal, 1024),
            (EffortLevel::Low, 2048),
            (EffortLevel::Medium, 4096),
            (EffortLevel::High, 8192),
            (EffortLevel::XHigh, 16384),
            (EffortLevel::Max, 24576),
        ]
    );
    assert_eq!(SHARED_BUDGETS.nearest(1536).unwrap().0, EffortLevel::Low);
    assert_eq!(SHARED_BUDGETS.nearest(3072).unwrap().0, EffortLevel::Medium);
    assert_eq!(SHARED_BUDGETS.nearest(6144).unwrap().0, EffortLevel::High);
    assert_eq!(SHARED_BUDGETS.nearest(20480).unwrap().0, EffortLevel::Max);
    assert_eq!(
        SHARED_BUDGETS.nearest(12288).unwrap(),
        (EffortLevel::XHigh, 16384)
    );
    assert_eq!(
        SHARED_BUDGETS.nearest(8192).unwrap(),
        (EffortLevel::High, 8192)
    );
    assert!(SHARED_BUDGETS.nearest(0).is_err());
    let repeated = ThinkingBudgets {
        entries: vec![
            (EffortLevel::XHigh, 1000),
            (EffortLevel::Low, 500),
            (EffortLevel::High, 1000),
            (EffortLevel::Medium, 500),
        ]
        .into(),
    };
    assert_eq!(repeated.nearest(750).unwrap().0, EffortLevel::XHigh);
    assert_eq!(repeated.nearest(500).unwrap().0, EffortLevel::Medium);
    assert_eq!(repeated.nearest(1000).unwrap().0, EffortLevel::XHigh);
    assert_eq!(repeated.nearest(u64::MAX).unwrap().0, EffortLevel::XHigh);
    static QUANTIZE: ReasoningRequestPolicy = ReasoningRequestPolicy {
        canonical_model: std::borrow::Cow::Borrowed("m"),
        rule: "synthetic.budget-to-effort.v1",
        conversion: ReasoningConversion::BudgetToEffort {
            scope: EffortScope::Reasoning,
            table: SHARED_BUDGETS,
        },
    };
    let mut settings = GenerationSettings {
        instructions: Presence::Value(Text::new("hello", "test", 32).unwrap()),
        reasoning: ReasoningRequest::present(None),
        ..Default::default()
    };
    settings.reasoning.budget_tokens = Some(12288);
    settings.reasoning.mode = Presence::Value(ReasoningMode::Budgeted);
    settings.controls.max_output_tokens = Some(16384);
    let request = GenerationRequest::from_settings(vec![], settings.clone()).unwrap();
    let source = request.clone();
    let semantic = QUANTIZE
        .project_request(&request, |_| false)
        .unwrap()
        .unwrap();
    assert_eq!(
        semantic.settings().effort.value().unwrap().level,
        EffortLevel::XHigh
    );
    assert_eq!(request, source);
    let mut adapter = morphiecore::protocol::openai::Adapter::new(
        Profile::Responses,
        morphiecore::protocol::openai::Dialect::Standard,
        None,
    );
    adapter.adaptation.reasoning_policy = Some(QUANTIZE.clone());
    let mut envelope = adapter
        .decode_request(br#"{"model":"m","input":"hi"}"#)
        .unwrap();
    envelope.task.semantic = request;
    let (wire, rules) = adapter
        .encode_request_with_rules(&envelope, "m", &TargetCaps::full())
        .unwrap();
    assert_eq!(wire["reasoning"]["effort"], "xhigh");
    assert_eq!(rules, ["synthetic.budget-to-effort.v1"]);
    let allocate = ReasoningRequestPolicy {
        canonical_model: "m".into(),
        rule: "synthetic.effort-to-budget.v1",
        conversion: ReasoningConversion::EffortToBudget {
            scope: EffortScope::Reasoning,
            table: SHARED_BUDGETS,
        },
    };
    let mut effort = semantic.settings().clone();
    effort.controls.max_output_tokens = Some(4096);
    assert!(allocate.project(&effort).is_err());
    effort.controls.max_output_tokens = Some(16384);
    let allocated = allocate.project(&effort).unwrap();
    assert_eq!(allocated.reasoning.budget_tokens, Some(16384));
    effort.reasoning.mode = Presence::Value(ReasoningMode::Pro);
    assert!(allocate.project(&effort).is_err());
    assert_eq!(effort.reasoning.mode, Presence::Value(ReasoningMode::Pro));
    effort.reasoning.mode = Presence::Absent;
    let allocated = GenerationRequest::from_settings(vec![], allocated).unwrap();
    let (wire, _) = morphiecore::protocol::google::generate_content::encode_generation_request(
        &allocated,
        &FidelityRecords::default(),
        &morphiecore::protocol::native::ReplayTarget::new("synthetic", "m").unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(
        wire["generationConfig"]["thinkingConfig"]["thinkingBudget"],
        16384
    );
    let scoped = ReasoningRequestPolicy {
        canonical_model: "m".into(),
        rule: "synthetic.scope.v1",
        conversion: ReasoningConversion::Scope {
            from: EffortScope::Reasoning,
            to: EffortScope::Response,
            levels: &[(EffortLevel::XHigh, EffortLevel::High)],
        },
    }
    .project(&effort)
    .unwrap();
    assert_eq!(scoped.effort.value().unwrap().scope, EffortScope::Response);
    let mut disabled = scoped;
    disabled.reasoning.activation = Presence::Value(ReasoningActivation::Disabled);
    let disabled = ReasoningRequestPolicy {
        canonical_model: "m".into(),
        rule: "synthetic.preserve-off.v1",
        conversion: ReasoningConversion::PreserveDisabled,
    }
    .project(&disabled)
    .unwrap();
    assert!(disabled.effort.is_absent());
    assert_eq!(
        disabled.reasoning.activation.value(),
        Some(&ReasoningActivation::Disabled)
    );
    let mut client = morphiecore::protocol::openai::Adapter::new(
        Profile::Chat,
        morphiecore::protocol::openai::Dialect::Standard,
        None,
    );
    let instruction = client
        .decode_request(br#"{"model":"m","messages":[{"role":"user","content":"hi"}]}"#)
        .unwrap();
    let mut instruction = instruction;
    let mut settings = instruction.task.semantic.settings().clone();
    settings.instructions = Presence::Value(Text::new("instructions", "test", 32).unwrap());
    instruction.task.semantic = instruction.task.semantic.with_settings(settings).unwrap();
    assert_eq!(
        client
            .encode_request(&instruction, "m", &TargetCaps::full())
            .unwrap()["messages"][0]["role"],
        "developer"
    );
    client.adaptation.rules.chat_system_instructions = true;
    let (wire, rules) = client
        .encode_request_with_rules(&instruction, "m", &TargetCaps::full())
        .unwrap();
    assert_eq!(
        wire["messages"][0],
        serde_json::json!({"role":"system","content":"instructions"})
    );
    assert_eq!(rules, ["chat.instructions-as-system.v1"]);
    for dimension in 0..4 {
        let mut control = ReasoningRequest::present(None);
        match dimension {
            0 => control.budget_tokens = Some(1024),
            1 => control.presentation.visibility = Presence::Value(ReasoningDisplay::Omitted),
            2 => control.presentation.visibility = Presence::Value(ReasoningDisplay::Summary),
            _ => control.mode = Presence::Value(ReasoningMode::Adaptive),
        }
        let settings = GenerationSettings {
            instructions: Presence::Value(Text::new("hello", "test", 32).unwrap()),
            reasoning: control,
            effort: Presence::Value(EffortControl {
                scope: EffortScope::Reasoning,
                level: EffortLevel::High,
            }),
            ..Default::default()
        };
        let request = GenerationRequest::from_settings(vec![], settings.clone()).unwrap();
        let mut metadata = crate::events_support::metadata();
        metadata
            .context
            .reports
            .latest_mut()
            .expect("report")
            .settings = Some(settings);
        let response = GenerationResponse::new(
            vec![(
                ItemId::new(1),
                Item::Message(Message {
                    participant: None,
                    role: MessageRole::Assistant,
                    phase: None,
                    status: ItemLifecycle::Completed,
                    parts: vec![Part {
                        replay: None,
                        id: PartId::new(1),
                        content: ContentPart::Text(Text::new("ok", "test", 32).unwrap().into()),
                        cache: None,
                    }],
                }),
            )],
            Outcome::Completed,
        )
        .unwrap();
        let fidelity = FidelityRecords::default();
        for profile in [Profile::Responses, Profile::Chat] {
            let exact = dimension == 2 && profile == Profile::Responses;
            if exact {
                let target =
                    lower_request(&request, &fidelity, profile, TargetCaps::full()).unwrap();
                assert_eq!(
                    morphiecore::protocol::openai::responses::encode_generation(&target).unwrap()["reasoning"],
                    serde_json::json!({"effort":"high","summary":"auto"})
                );
            } else {
                assert_eq!(
                    lower_request(&request, &fidelity, profile, TargetCaps::full()).err(),
                    Some(RepresentationError::Reasoning)
                );
            }
            let target =
                lower_response(&response, &fidelity, &metadata, profile, TargetCaps::full())
                    .unwrap();
            assert_eq!(
                target.projection().iter().any(|stage| stage.loss.is_some()),
                !exact
            );
            assert_eq!(
                metadata.context.settings().unwrap().reasoning,
                request.settings().reasoning
            );
            let json = match profile {
                Profile::Responses => {
                    morphiecore::protocol::openai::responses::envelope::encode_response(&target)
                        .unwrap()
                }
                Profile::Chat => {
                    morphiecore::protocol::openai::chat::envelope::encode_response(&target).unwrap()
                }
            };
            if profile == Profile::Responses {
                assert_eq!(
                    json["reasoning"],
                    if exact {
                        serde_json::json!({"effort":"high","summary":"auto"})
                    } else {
                        serde_json::json!({"effort":"high"})
                    }
                );
            } else {
                assert!(target.metadata().context.settings().is_none());
            }
            let mut encoder = EventEncoder::new(profile, metadata.clone()).unwrap();
            let mut frames = encoder.encode(&StreamEvent::Started, &fidelity).unwrap();
            frames.extend(
                encoder
                    .encode(
                        &crate::events_support::start(1, ItemKind::Message { phase: None }),
                        &fidelity,
                    )
                    .unwrap(),
            );
            for event in crate::events_support::part(1, 1, PartKind::Text, "ok") {
                frames.extend(encoder.encode(&event, &fidelity).unwrap());
            }
            frames.extend(
                encoder
                    .encode(
                        &crate::events_support::close(1, ItemLifecycle::Completed),
                        &fidelity,
                    )
                    .unwrap(),
            );
            let terminal = StreamEvent::Terminal {
                terminal: StreamTerminal::Completed,
                details: Default::default(),
            };
            frames.extend(encoder.encode(&terminal, &fidelity).unwrap());
            encoder.finish().unwrap();
            let mut intake = morphiecore::protocol::openai::events::EventDecoder::new(profile);
            for frame in &frames {
                intake.push(frame).unwrap();
            }
            if profile == Profile::Chat {
                intake.done().unwrap();
            }
            let materialized = intake.materialize().unwrap();
            let mut expected = target.metadata().context.settings().cloned();
            if exact {
                // The native auto label expresses unconstrained visible summary granularity.
                expected.as_mut().unwrap().reasoning.presentation.summary =
                    Presence::Value(ReasoningSummary::Auto);
            }
            assert_eq!(materialized.metadata.context.settings(), expected.as_ref());
            assert!(
                matches!(&materialized.semantic.items()[0].1, Item::Message(message)
                    if matches!(&message.parts[0].content, ContentPart::Text(text) if text.as_str() == "ok"))
            );
            assert_eq!(
                encoder
                    .projection()
                    .iter()
                    .any(|stage| stage.loss.is_some()),
                !exact
            );
        }
    }
}
