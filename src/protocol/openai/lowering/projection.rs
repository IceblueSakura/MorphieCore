//! Bounded, body-free evidence for the selected response projection rules.
use crate::protocol::openai::lowering::RepresentationError;
use crate::{protocol::openai::Profile, semantic::task::generation::Usage};

pub const MAX_PROJECTION_STAGES: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LossRule {
    UsageReportView,
    OmitReportedReasoningControls,
    OmitChatReportedSettings,
    OmitResponsesSystemFingerprint,
    OmitResponsesInputImageTokens,
    OmitResponsesInputTextTokens,
    OmitResponsesOutputTextTokens,
    OmitResponsesInputAudioTokens,
    OmitResponsesOutputAudioTokens,
    OmitResponsesAcceptedPredictionTokens,
    OmitResponsesRejectedPredictionTokens,
    OmitResponsesMessageEnvelopes,
    RegroupChatMessageItems,
    CoarsenChatCompletionCause,
    OmitResponsesCompletionCause,
    RecitationAsContentFilter,
}
impl LossRule {
    pub const fn name(self) -> &'static str {
        match self {
            Self::UsageReportView => "generation.usage-report-view.v1",
            Self::OmitReportedReasoningControls => {
                "responses.omit-incidental-reasoning-controls.v1"
            }
            Self::OmitChatReportedSettings => "chat.omit-incidental-settings.v1",
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
            Self::RegroupChatMessageItems => "chat.regroup-message-items.v1",
            Self::CoarsenChatCompletionCause => "chat.coarsen-completion-cause.v1",
            Self::OmitResponsesCompletionCause => "responses.omit-completion-cause.v1",
            Self::RecitationAsContentFilter => "openai.recitation-as-content-filter.v1",
        }
    }
    pub const fn owner(self) -> &'static str {
        match self {
            Self::UsageReportView => "generation.usage.reports",
            Self::OmitReportedReasoningControls => "response.reported.reasoning",
            Self::OmitChatReportedSettings => "response.reported.settings",
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
            Self::CoarsenChatCompletionCause | Self::OmitResponsesCompletionCause => {
                "response.terminal.completion"
            }
            Self::RecitationAsContentFilter => "response.terminal.incomplete",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionDirection {
    Request,
    Response,
    Event,
}

/// This changes reported echoes only, never request admission or generated items.
pub(crate) fn reported_settings(
    metadata: &mut crate::protocol::ResponseMetadata,
    profile: Profile,
) -> Result<Option<LossRule>, RepresentationError> {
    use crate::semantic::{task::generation::*, value::Presence};
    metadata
        .context
        .validate()
        .map_err(|_| RepresentationError::Metadata)?;
    if metadata
        .context
        .settings
        .as_ref()
        .is_some_and(|settings| !settings.safety.is_empty())
    {
        return Err(RepresentationError::Safety);
    }
    if profile == Profile::Chat {
        if metadata.context.settings.is_none() && metadata.context.instruction_messages.is_none() {
            return Ok(None);
        }
        metadata.context.settings = None;
        metadata.context.instruction_messages = None;
        metadata.instruction_fidelity = Default::default();
        return Ok(Some(LossRule::OmitChatReportedSettings));
    }
    let Some(settings) = &mut metadata.context.settings else {
        return Ok(None);
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
    settings.reasoning.display = Presence::Absent;
    if matches!(
        settings.reasoning.mode.value(),
        Some(ReasoningMode::Adaptive | ReasoningMode::Budgeted)
    ) {
        settings.reasoning.mode = Presence::Absent;
    }
    if settings.reasoning.summary() == Some(ReasoningSummary::Disabled) {
        settings.reasoning.summary = Presence::Absent;
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
    Ok((before != *settings).then_some(LossRule::OmitReportedReasoningControls))
}

/// Evidence describes a checked value, not authorization or issuer acceptance.
/// Empty loss means exact for this stage, not that an earlier loss was undone.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectionStage {
    pub target: Profile,
    pub profile_id: &'static str,
    pub direction: ProjectionDirection,
    pub revision: u32,
    pub loss: Option<LossRule>,
}
impl ProjectionStage {
    pub(crate) fn evidence(&self) -> Option<crate::lowering::generation::LossEvidence> {
        let loss = self.loss?;
        Some(crate::lowering::generation::LossEvidence {
            rule: loss.name(),
            field: loss.owner(),
            direction: match self.direction {
                ProjectionDirection::Request => "request.encode",
                ProjectionDirection::Response => "response.encode",
                ProjectionDirection::Event => "event.encode",
            },
            action: match loss {
                LossRule::RegroupChatMessageItems => "regroup",
                LossRule::CoarsenChatCompletionCause | LossRule::RecitationAsContentFilter => {
                    "approximate"
                }
                LossRule::UsageReportView => "project",
                _ => "omit",
            },
        })
    }
    pub(crate) fn response(
        target: Profile,
        profile_id: &'static str,
        loss: Option<LossRule>,
    ) -> Self {
        Self {
            target,
            profile_id,
            direction: ProjectionDirection::Response,
            revision: 1,
            loss,
        }
    }
}

/// Fixed Responses only carries cache and reasoning breakdowns. Named profiles
/// can additionally carry image/text counts; admission is not enlarged here.
/// https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses/response_usage.py
pub(crate) fn responses_usage(
    mut usage: Usage,
    rules: &crate::protocol::openai::dialect::adaptation::WireRules,
) -> Result<(Usage, Vec<LossRule>), RepresentationError> {
    usage.validate()?;
    let mut losses = Vec::new();
    for (value, carrier, rule) in [
        (
            &mut usage.input_image_tokens,
            rules.responses_image_usage,
            LossRule::OmitResponsesInputImageTokens,
        ),
        (
            &mut usage.input_text_tokens,
            rules.responses_text_usage,
            LossRule::OmitResponsesInputTextTokens,
        ),
        (
            &mut usage.output_text_tokens,
            rules.responses_text_usage,
            LossRule::OmitResponsesOutputTextTokens,
        ),
        (
            &mut usage.input_audio_tokens,
            false,
            LossRule::OmitResponsesInputAudioTokens,
        ),
        (
            &mut usage.output_audio_tokens,
            false,
            LossRule::OmitResponsesOutputAudioTokens,
        ),
        (
            &mut usage.accepted_prediction_tokens,
            false,
            LossRule::OmitResponsesAcceptedPredictionTokens,
        ),
        (
            &mut usage.rejected_prediction_tokens,
            false,
            LossRule::OmitResponsesRejectedPredictionTokens,
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

    fn metadata(settings: GenerationSettings) -> crate::protocol::ResponseMetadata {
        crate::protocol::ResponseMetadata {
            storage: None,
            id: Some("resp".into()),
            model: "m".into(),
            created: None,
            context: crate::semantic::context::ResponseContext {
                settings: Some(settings),
                ..Default::default()
            },
            instruction_fidelity: Default::default(),
        }
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
            reported_settings(&mut chat, Profile::Chat),
            Ok(Some(LossRule::OmitChatReportedSettings))
        );
        assert!(chat.context.settings.is_none());

        let mut responses = metadata(settings.clone());
        assert_eq!(
            reported_settings(&mut responses, Profile::Responses),
            Ok(Some(LossRule::OmitReportedReasoningControls))
        );
        let kept = responses.context.settings.unwrap();
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
        assert_eq!(reported_settings(&mut exact, Profile::Responses), Ok(None));

        let mut safety = settings;
        safety.safety = vec![SafetySetting {
            category: SafetyCategory::Harassment,
            threshold: SafetyThreshold::BlockNone,
        }];
        for profile in [Profile::Chat, Profile::Responses] {
            assert_eq!(
                reported_settings(&mut metadata(safety.clone()), profile),
                Err(RepresentationError::Safety)
            );
        }
    }
}
