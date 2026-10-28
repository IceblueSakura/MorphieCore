//! Bounded, body-free evidence for the selected response projection rules.
use crate::{
    protocol::openai::{GenerationApi, lowering::RepresentationError},
    semantic::task::generation::Usage,
};

pub const MAX_PROJECTION_STAGES: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionRule {
    OmitPrediction,
    OmitOpeningReport,
    UsageReportView,
    OmitReportedReasoningControls,
    OmitChatReportedSettings,
    OmitChatTextItemStatus,
    EchoRequestMetadata,
    OmitResponsesSystemFingerprint,
    OmitResponsesInputImageTokens,
    OmitResponsesInputTextTokens,
    OmitResponsesOutputTextTokens,
    OmitResponsesInputAudioTokens,
    OmitResponsesOutputAudioTokens,
    OmitResponsesAcceptedPredictionTokens,
    OmitResponsesRejectedPredictionTokens,
    OmitResponsesMessageEnvelopes,
    OmitChatMessagePhase,
    RegroupChatMessageItems,
    CoarsenChatCompletionCause,
    OmitResponsesCompletionCause,
    RecitationAsContentFilter,
    ContextWindowAsLength,
    OmitResponsesContextWindowReason,
    OmitExplanatoryRefusalDetails,
    OmitTerminalExplanation,
    OmitUnrepresentableServiceClass,
}
impl ProjectionRule {
    pub const fn name(self) -> &'static str {
        match self {
            Self::OmitPrediction => "generation.omit-prediction.v1",
            Self::OmitOpeningReport => "responses.terminal-report-snapshot.v1",
            Self::UsageReportView => "generation.usage-report-view.v1",
            Self::OmitReportedReasoningControls => {
                "responses.omit-incidental-reasoning-controls.v1"
            }
            Self::OmitChatReportedSettings => "chat.omit-incidental-settings.v1",
            Self::OmitChatTextItemStatus => "chat.omit-text-item-status.v1",
            Self::EchoRequestMetadata => "responses.echo-request-metadata.v1",
            Self::OmitResponsesSystemFingerprint => "responses.omit-system-fingerprint.v1",
            Self::OmitResponsesInputImageTokens => "responses.omit-input-image-tokens.v1",
            Self::OmitResponsesInputTextTokens => "responses.omit-input-text-tokens.v1",
            Self::OmitResponsesOutputTextTokens => "responses.omit-output-text-tokens.v1",
            Self::OmitResponsesInputAudioTokens => "responses.omit-input-audio-tokens.v1",
            Self::OmitResponsesOutputAudioTokens => "responses.omit-output-audio-tokens.v1",
            Self::OmitResponsesAcceptedPredictionTokens => {
                "responses.omit-accepted-prediction-tokens.v1"
            }
            Self::OmitResponsesRejectedPredictionTokens => {
                "responses.omit-rejected-prediction-tokens.v1"
            }
            Self::OmitResponsesMessageEnvelopes => "responses.omit-message-envelopes.v1",
            Self::OmitChatMessagePhase => "chat.omit-message-phase.v1",
            Self::RegroupChatMessageItems => "chat.regroup-message-items.v1",
            Self::CoarsenChatCompletionCause => "chat.coarsen-completion-cause.v1",
            Self::OmitResponsesCompletionCause => "responses.omit-completion-cause.v1",
            Self::RecitationAsContentFilter => "openai.recitation-as-content-filter.v1",
            Self::ContextWindowAsLength => "chat.context-window-as-length.v1",
            Self::OmitResponsesContextWindowReason => "responses.omit-context-window-reason.v1",
            Self::OmitExplanatoryRefusalDetails => "openai.omit-explanatory-refusal-details.v1",
            Self::OmitTerminalExplanation => "openai.omit-terminal-explanation.v1",
            Self::OmitUnrepresentableServiceClass => "openai.omit-unrepresentable-service-class.v1",
        }
    }
    pub const fn field(self) -> &'static str {
        match self {
            Self::OmitPrediction => "generation.prediction",
            Self::OmitOpeningReport => "response.reports.opening",
            Self::UsageReportView => "generation.usage.reports",
            Self::OmitReportedReasoningControls => "response.reported.reasoning",
            Self::OmitChatReportedSettings => "response.reported.settings",
            Self::EchoRequestMetadata => "response.context.metadata",
            Self::OmitChatTextItemStatus => "generation.message.status",
            Self::OmitResponsesSystemFingerprint => "response.context.system_fingerprint",
            Self::OmitResponsesInputImageTokens => "generation.usage.input_image_tokens",
            Self::OmitResponsesInputTextTokens => "generation.usage.input_text_tokens",
            Self::OmitResponsesOutputTextTokens => "generation.usage.output_text_tokens",
            Self::OmitResponsesInputAudioTokens => "generation.usage.input_audio_tokens",
            Self::OmitResponsesOutputAudioTokens => "generation.usage.output_audio_tokens",
            Self::OmitResponsesAcceptedPredictionTokens => {
                "generation.usage.accepted_prediction_tokens"
            }
            Self::OmitResponsesRejectedPredictionTokens => {
                "generation.usage.rejected_prediction_tokens"
            }
            Self::OmitResponsesMessageEnvelopes | Self::RegroupChatMessageItems => {
                "generation.message_envelopes"
            }
            Self::OmitChatMessagePhase => "generation.message.phase",
            Self::CoarsenChatCompletionCause | Self::OmitResponsesCompletionCause => {
                "response.terminal.completion"
            }
            Self::RecitationAsContentFilter
            | Self::ContextWindowAsLength
            | Self::OmitResponsesContextWindowReason => "response.terminal.incomplete",
            Self::OmitExplanatoryRefusalDetails => "response.terminal.refusal",
            Self::OmitTerminalExplanation => "response.terminal.explanation",
            Self::OmitUnrepresentableServiceClass => "generation.service_class",
        }
    }
}

/// A served class is not the requested policy. Never invent an approximate tier.
pub(crate) fn service_class_view(
    class: &crate::semantic::value::Presence<crate::semantic::context::ServiceClass>,
    profile: GenerationApi,
) -> crate::semantic::value::Presence<crate::semantic::context::ServiceClass> {
    if class
        .value()
        .is_some_and(|class| crate::protocol::openai::service::class_wire(*class, profile).is_err())
    {
        crate::semantic::value::Presence::Absent
    } else {
        class.clone()
    }
}

/// Project rendering copies only; callers validate original lifecycle relations first.
pub(crate) fn reported_service_class(
    metadata: &mut crate::protocol::ResponseMetadata,
    profile: GenerationApi,
) -> Option<ProjectionRule> {
    let mut changed = false;
    for report in metadata.context.reports.iter_mut() {
        let view = service_class_view(&report.service_class, profile);
        changed |= view != report.service_class;
        report.service_class = view;
    }
    changed.then_some(ProjectionRule::OmitUnrepresentableServiceClass)
}

/// Chat length reports an overall token stop, not the lifecycle of each source
/// message. Build only the selected plain-text view; never edit source evidence.
pub(crate) fn chat_text_status(
    source: &crate::semantic::task::generation::GenerationResponse,
    policy: super::ReportedFactPolicy,
) -> Result<
    Option<(
        crate::semantic::task::generation::GenerationResponse,
        Vec<crate::protocol::conversion::generation::Effect>,
    )>,
    RepresentationError,
> {
    use crate::{
        protocol::conversion::generation::{ConversionBoundary, Owner},
        semantic::task::generation::*,
    };
    if source.outcome() != Outcome::Incomplete
        || source.details().incomplete != Some(IncompleteReason::MaxOutputTokens)
        || source.items().is_empty()
        || !source.items().iter().all(|(_, item)| {
            matches!(item, Item::Message(m)
                if m.role == MessageRole::Assistant && m.participant.is_none()
                    && matches!(m.status, ItemLifecycle::Completed | ItemLifecycle::Incomplete)
                    && !m.parts.is_empty()
                    && m.parts.iter().all(|p|
                        p.cache.is_none() && p.replay.is_none()
                            && matches!(&p.content, ContentPart::Text(t) if t.is_plain())))
        })
    {
        return Ok(None);
    }
    let effects: Vec<_> = source
        .items()
        .iter()
        .filter(|(_, item)| item.lifecycle() == Some(ItemLifecycle::Completed))
        .map(|(id, _)| {
            ProjectionRule::OmitChatTextItemStatus
                .effect(ConversionBoundary::ResponseProject)
                .at(Owner::item(*id))
        })
        .collect();
    if effects.is_empty() {
        return Ok(None);
    }
    if policy == super::ReportedFactPolicy::StrictComplete {
        return Err(RepresentationError::ReportedFacts);
    }
    let mut items = source.items().to_vec();
    for (_, item) in &mut items {
        if let Item::Message(message) = item {
            message.status = ItemLifecycle::Incomplete;
        }
    }
    Ok(Some((source.clone().with_items(items)?, effects)))
}

pub(crate) fn request_metadata_echo(
    metadata: &crate::protocol::ResponseMetadata,
    policy: super::ReportedFactPolicy,
) -> Result<Option<ProjectionRule>, RepresentationError> {
    if metadata.request_metadata_echo().is_none() {
        return Ok(None);
    }
    if policy == super::ReportedFactPolicy::StrictComplete {
        return Err(RepresentationError::ReportedFacts);
    }
    Ok(Some(ProjectionRule::EchoRequestMetadata))
}

/// This changes reported echoes only, never request admission or generated items.
pub(crate) fn reported_settings(
    metadata: &mut crate::protocol::ResponseMetadata,
    profile: GenerationApi,
) -> Result<Option<ProjectionRule>, RepresentationError> {
    use crate::semantic::{task::generation::*, value::Presence};
    metadata
        .context
        .validate()
        .map_err(|_| RepresentationError::Metadata)?;
    crate::protocol::reports::relation(&metadata.context.reports, true)
        .map_err(|_| RepresentationError::Metadata)?;
    if metadata
        .context
        .reports
        .iter()
        .any(|r| r.settings.as_ref().is_some_and(|s| !s.safety.is_empty()))
    {
        return Err(RepresentationError::Safety);
    }
    if profile == GenerationApi::Chat {
        let had_settings = metadata
            .context
            .reports
            .iter()
            .any(|r| r.settings.is_some())
            || metadata.context.instruction_messages.is_some();
        crate::protocol::reports::single_view(&mut metadata.context.reports)
            .map_err(|_| RepresentationError::Metadata)?;
        for report in metadata.context.reports.iter_mut() {
            report.settings = None;
        }
        metadata.context.instruction_messages = None;
        metadata.instruction_fidelity = Default::default();
        return Ok(had_settings.then_some(ProjectionRule::OmitChatReportedSettings));
    }
    let mut changed = false;
    for report in metadata.context.reports.iter_mut() {
        let Some(settings) = &mut report.settings else {
            continue;
        };
        if settings
            .tools
            .iter()
            .flatten()
            .flat_map(ToolDefinition::leaves)
            .any(|tool| matches!(tool, ToolDefinition::Function(tool) if tool.cache.is_some()))
        {
            return Err(RepresentationError::Controls);
        }
        let before = settings.clone();
        settings.reasoning.budget_tokens = None;
        if matches!(
            settings.reasoning.mode.value(),
            Some(ReasoningMode::Adaptive | ReasoningMode::Budgeted)
        ) {
            settings.reasoning.mode = Presence::Absent;
        }
        if matches!(
            settings.reasoning.presentation.visibility,
            Presence::Null | Presence::Value(ReasoningDisplay::Omitted)
        ) {
            settings.reasoning.presentation = Default::default();
        }
        if settings
            .effort
            .value()
            .is_some_and(|effort| effort.scope == EffortScope::Response)
        {
            settings.effort = Presence::Absent;
        }
        if settings.reasoning.activation == Presence::Null
            || settings.reasoning.activation == Presence::Value(ReasoningActivation::Enabled)
                && settings.effort.value().is_none()
        {
            settings.reasoning.activation = Presence::Absent;
        }
        settings.validate()?;
        changed |= before != *settings;
    }
    Ok(changed.then_some(ProjectionRule::OmitReportedReasoningControls))
}
impl ProjectionRule {
    pub(crate) fn effect(
        self,
        boundary: crate::protocol::conversion::generation::ConversionBoundary,
    ) -> crate::protocol::conversion::generation::Effect {
        use crate::protocol::conversion::generation::{Change, Effect};
        let change = match self {
            Self::EchoRequestMetadata => Change::InferFromRequest,
            Self::RegroupChatMessageItems
            | Self::CoarsenChatCompletionCause
            | Self::RecitationAsContentFilter
            | Self::ContextWindowAsLength => Change::Approximate,
            _ => Change::Omit,
        };
        Effect::new(self.name(), self.field().into(), boundary, change)
    }
}

/// Fixed Responses only carries cache and reasoning breakdowns. Named profiles
/// can additionally carry image/text counts; admission is not enlarged here.
/// https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses/response_usage.py
pub(crate) fn responses_usage(
    mut usage: Usage,
    rules: &crate::protocol::openai::dialect::adaptation::WireRules,
) -> Result<(Usage, Vec<ProjectionRule>), RepresentationError> {
    usage.validate()?;
    let mut losses = Vec::new();
    for (value, carrier, rule) in [
        (
            &mut usage.input_image_tokens,
            rules.responses_image_usage,
            ProjectionRule::OmitResponsesInputImageTokens,
        ),
        (
            &mut usage.input_text_tokens,
            rules.responses_text_usage,
            ProjectionRule::OmitResponsesInputTextTokens,
        ),
        (
            &mut usage.output_text_tokens,
            rules.responses_text_usage,
            ProjectionRule::OmitResponsesOutputTextTokens,
        ),
        (
            &mut usage.input_audio_tokens,
            false,
            ProjectionRule::OmitResponsesInputAudioTokens,
        ),
        (
            &mut usage.output_audio_tokens,
            false,
            ProjectionRule::OmitResponsesOutputAudioTokens,
        ),
        (
            &mut usage.accepted_prediction_tokens,
            false,
            ProjectionRule::OmitResponsesAcceptedPredictionTokens,
        ),
        (
            &mut usage.rejected_prediction_tokens,
            false,
            ProjectionRule::OmitResponsesRejectedPredictionTokens,
        ),
    ] {
        if !carrier && value.take().is_some() {
            losses.push(rule);
        }
    }
    usage.validate()?;
    Ok((usage, losses))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::{task::generation::*, value::Presence};

    #[test]
    fn text_status_omission_is_token_cap_only_and_strict_complete_rejects_it() {
        use super::super::ReportedFactPolicy::{Faithful, StrictComplete};
        use crate::protocol::openai::{Dialect, GenerationCodec, TargetContract};
        use serde_json::json;
        // Independent Responses shape: an item can complete before the response
        // reaches its token cap. Chat length has no per-item status carrier.
        let body = json!({"id":"r","object":"response","model":"m","created_at":1,
            "status":"incomplete","incomplete_details":{"reason":"max_output_tokens"},
            "output":[{"id":"m","type":"message","role":"assistant","status":"completed",
                "content":[{"type":"output_text","text":"A.","annotations":[]}]}]});
        let codec = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
        let decoded = codec.decode_response(body.to_string().as_bytes()).unwrap();
        let source = &decoded.semantic;
        assert_eq!(
            source.items()[0].1.lifecycle(),
            Some(ItemLifecycle::Completed)
        );
        assert_eq!(source.outcome(), Outcome::Incomplete);
        let (view, effects) = chat_text_status(source, Faithful).unwrap().unwrap();
        assert_eq!(
            view.items()[0].1.lifecycle(),
            Some(ItemLifecycle::Incomplete)
        );
        assert_eq!(
            source.items()[0].1.lifecycle(),
            Some(ItemLifecycle::Completed)
        );
        assert_eq!(view.details(), source.details());
        assert_eq!(effects.len(), 1);
        // Direct decision check: missing unrelated settings/usage cannot mask
        // whether strict mode rejected this actual lifecycle omission.
        assert_eq!(
            chat_text_status(source, StrictComplete),
            Err(RepresentationError::ReportedFacts)
        );
        assert!(chat_text_status(&view, Faithful).unwrap().is_none());
        let target = super::super::lower_response(
            source,
            &decoded.fidelity,
            &decoded.metadata,
            GenerationApi::Chat,
            TargetContract::full(),
        )
        .unwrap();
        assert_eq!(target.semantic(), &view);
        assert_eq!(target.projection(), effects.as_slice());
        let native = super::super::lower_response(
            source,
            &decoded.fidelity,
            &decoded.metadata,
            GenerationApi::Responses,
            TargetContract::full(),
        )
        .unwrap();
        assert_eq!(native.semantic(), source);
        assert!(
            !native
                .projection()
                .iter()
                .any(|e| e.rule() == ProjectionRule::OmitChatTextItemStatus.name())
        );
        for reason in [
            IncompleteReason::ContentFilter,
            IncompleteReason::ContextWindowExceeded,
            IncompleteReason::Unspecified,
        ] {
            let other = source
                .clone()
                .with_details(TerminalDetails {
                    incomplete: Some(reason),
                    ..Default::default()
                })
                .unwrap();
            assert!(chat_text_status(&other, Faithful).unwrap().is_none());
        }
        let extras = [
            json!({"id":"f","type":"function_call","call_id":"c","name":"f",
                "arguments":"{}","status":"completed"}),
            json!({"id":"r2","type":"reasoning","summary":[{"type":"summary_text","text":"plan"}],
                "status":"completed"}),
            json!({"id":"m2","type":"message","role":"assistant","status":"completed",
                "content":[{"type":"refusal","refusal":"no"}]}),
            json!({"id":"m2","type":"message","role":"assistant","status":"completed",
                "content":[{"type":"output_text","text":"B","annotations":[],"logprobs":[]}]}),
        ];
        for item in extras {
            let mut mixed = body.clone();
            mixed["output"].as_array_mut().unwrap().push(item);
            let mixed = codec.decode_response(mixed.to_string().as_bytes()).unwrap();
            assert!(
                chat_text_status(&mixed.semantic, Faithful)
                    .unwrap()
                    .is_none()
            );
            assert!(
                super::super::lower_response(
                    &mixed.semantic,
                    &mixed.fidelity,
                    &mixed.metadata,
                    GenerationApi::Chat,
                    TargetContract::full()
                )
                .is_err()
            );
        }
    }

    fn metadata(settings: GenerationSettings) -> crate::protocol::ResponseMetadata {
        crate::protocol::ResponseMetadata {
            delivery: None,
            storage: None,
            id: Some("resp".into()),
            model: "m".into(),
            created: None,
            context: crate::semantic::context::ResponseContext {
                reports: crate::semantic::context::ResponseReports::single(
                    crate::semantic::context::ResponseReport {
                        settings: Some(settings),
                        ..Default::default()
                    },
                ),
                ..Default::default()
            },
            instruction_fidelity: Default::default(),
        }
    }

    #[test]
    fn served_classes_omit_only_missing_carriers_and_preserve_source_drift_checks() {
        use crate::protocol::openai::{
            Dialect, GenerationCodec, TargetContract, events::EventEncoder,
        };
        use crate::semantic::context::ServiceClass;
        use serde_json::json;
        let decoded=GenerationCodec::new(GenerationApi::Responses,Dialect::Standard,None).decode_response(
            json!({"id":"r","object":"response","model":"m","created_at":1,"status":"completed",
                "output":[{"id":"m","type":"message","role":"assistant","status":"completed",
                    "content":[{"type":"output_text","text":"A","annotations":[]}]}]}).to_string().as_bytes()
        ).unwrap();
        for class in [
            ServiceClass::StandardCapacity,
            ServiceClass::PriorityCapacity,
            ServiceClass::BatchProcessing,
            ServiceClass::StandardProcessing,
        ] {
            for profile in [GenerationApi::Chat, GenerationApi::Responses] {
                let mut metadata = decoded.metadata.clone();
                metadata.context.reports =
                    crate::semantic::context::ResponseReports::single(Default::default());
                let report = metadata.context.reports.iter_mut().next().unwrap();
                report.service_class = Presence::Value(class);
                if profile == GenerationApi::Responses {
                    report.settings = Some(GenerationSettings {
                        tools: Some(vec![]),
                        tool_choice: Some(ToolChoice::Auto),
                        parallel_tool_calls: Some(true),
                        ..Default::default()
                    });
                }
                let source = metadata.clone();
                let omitted = class != ServiceClass::StandardProcessing;
                let view = super::super::lower_response(
                    &decoded.semantic,
                    &decoded.fidelity,
                    &metadata,
                    profile,
                    TargetContract::full(),
                )
                .unwrap();
                assert_eq!(view.metadata().context.service_class().is_absent(), omitted);
                assert_eq!(
                    view.projection()
                        .iter()
                        .any(|e| e.rule() == "openai.omit-unrepresentable-service-class.v1"),
                    omitted
                );
                assert_eq!(metadata, source);
                let mut encoder = EventEncoder::new(profile, metadata.clone()).unwrap();
                let opening = encoder
                    .encode(&StreamEvent::Started, &decoded.fidelity)
                    .unwrap();
                for event in opening {
                    let body = if profile == GenerationApi::Responses {
                        &event["response"]
                    } else {
                        &event
                    };
                    if omitted {
                        assert!(body.get("service_tier").is_none());
                    } else {
                        assert_eq!(body["service_tier"], "default");
                    }
                }
                let mut changed = metadata.clone();
                changed
                    .context
                    .reports
                    .iter_mut()
                    .next()
                    .unwrap()
                    .service_class = Presence::Value(if class == ServiceClass::PriorityCapacity {
                    ServiceClass::StandardCapacity
                } else {
                    ServiceClass::PriorityCapacity
                });
                assert!(encoder.update_metadata(changed).is_err());
                if profile == GenerationApi::Responses {
                    let mut strict = TargetContract::full();
                    strict.reported_facts = super::super::ReportedFactPolicy::StrictComplete;
                    let result = super::super::lower_response(
                        &decoded.semantic,
                        &decoded.fidelity,
                        &metadata,
                        profile,
                        strict.clone(),
                    );
                    if omitted {
                        assert_eq!(result.err(), Some(RepresentationError::ReportedFacts));
                    } else {
                        assert!(result.is_ok());
                    }
                    let mut encoder = EventEncoder::new(profile, metadata)
                        .unwrap()
                        .with_contract(strict);
                    assert_eq!(
                        encoder
                            .encode(&StreamEvent::Started, &decoded.fidelity)
                            .is_err(),
                        omitted
                    );
                }
            }
        }
    }

    #[test]
    fn uncarried_lifecycle_classes_still_reject_conflicting_reports() {
        use crate::protocol::openai::{
            Dialect, GenerationCodec, TargetContract, events::EventEncoder,
        };
        use crate::semantic::context::{ResponseReport, ResponseReports, ServiceClass};
        let decoded=GenerationCodec::new(GenerationApi::Responses,Dialect::Standard,None).decode_response(
            br#"{"id":"r","object":"response","model":"m","created_at":1,"status":"completed","output":[]}"#
        ).unwrap();
        let mut metadata = decoded.metadata.clone();
        metadata.context.reports = ResponseReports::Lifecycle {
            opening: Some(Box::new(ResponseReport {
                service_class: Presence::Value(ServiceClass::StandardCapacity),
                ..Default::default()
            })),
            terminal: Some(Box::new(ResponseReport {
                service_class: Presence::Value(ServiceClass::PriorityCapacity),
                ..Default::default()
            })),
        };
        assert_eq!(
            super::super::lower_response(
                &decoded.semantic,
                &decoded.fidelity,
                &metadata,
                GenerationApi::Responses,
                TargetContract::full()
            )
            .err(),
            Some(RepresentationError::Metadata)
        );
        assert!(EventEncoder::new(GenerationApi::Responses, metadata.clone()).is_err());
        let mut opening = metadata.clone();
        let ResponseReports::Lifecycle { terminal, .. } = &mut opening.context.reports else {
            unreachable!()
        };
        *terminal = None;
        let mut encoder = EventEncoder::new(GenerationApi::Responses, opening.clone()).unwrap();
        encoder
            .encode(&StreamEvent::Started, &decoded.fidelity)
            .unwrap();
        let item = StreamEvent::ItemStarted {
            item: ItemId::scoped(LocalScope::new(1), 1),
            kind: ItemKind::Message { phase: None },
            replay: None,
        };
        let mut valid = EventEncoder::new(GenerationApi::Responses, opening).unwrap();
        valid
            .encode(&StreamEvent::Started, &decoded.fidelity)
            .unwrap();
        valid.encode(&item, &decoded.fidelity).unwrap();
        assert!(encoder.update_metadata(metadata).is_err());
        assert!(encoder.encode(&item, &decoded.fidelity).is_err());
        assert!(encoder.finish().is_err());
    }

    /// Settings echoes lose only controls without a target carrier; protected reports reject.
    #[test]
    fn reported_settings_omit_only_incidental_echoes() {
        let mut settings = GenerationSettings {
            reasoning: ReasoningRequest::present(Some(ReasoningSummary::Detailed)),
            effort: Presence::Value(EffortControl {
                scope: EffortScope::Response,
                level: EffortLevel::High,
            }),
            ..Default::default()
        };
        settings.reasoning.mode = Presence::Value(ReasoningMode::Budgeted);
        settings.reasoning.budget_tokens = Some(2048);
        let mut chat = metadata(settings.clone());
        assert_eq!(
            reported_settings(&mut chat, GenerationApi::Chat),
            Ok(Some(ProjectionRule::OmitChatReportedSettings))
        );
        assert!(chat.context.settings().is_none());

        let mut responses = metadata(settings.clone());
        assert_eq!(
            reported_settings(&mut responses, GenerationApi::Responses),
            Ok(Some(ProjectionRule::OmitReportedReasoningControls))
        );
        let kept = responses.context.settings().unwrap();
        assert_eq!(kept.reasoning.budget_tokens, None);
        assert_eq!(kept.reasoning.mode, Presence::Absent);
        assert_eq!(kept.effort, Presence::Absent);
        assert_eq!(
            kept.reasoning.summary(),
            Some(ReasoningSummary::Detailed),
            "a representable summary echo is not incidental"
        );

        // A representable reasoning-scoped effort is delivered unchanged.
        let mut exact = metadata(GenerationSettings {
            effort: Presence::Value(EffortControl {
                scope: EffortScope::Reasoning,
                level: EffortLevel::Low,
            }),
            ..Default::default()
        });
        assert_eq!(
            reported_settings(&mut exact, GenerationApi::Responses),
            Ok(None)
        );

        let mut safety = settings;
        safety.safety = vec![SafetySetting {
            category: SafetyCategory::Harassment,
            threshold: SafetyThreshold::BlockNone,
        }];
        for profile in [GenerationApi::Chat, GenerationApi::Responses] {
            assert_eq!(
                reported_settings(&mut metadata(safety.clone()), profile),
                Err(RepresentationError::Safety)
            );
        }
    }
}
