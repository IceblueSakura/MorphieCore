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
    pub const fn consequence(self) -> &'static str {
        match self {
            Self::UsageReportView => {
                "Only the representable operation accounting view is delivered; source buckets, scope and provenance stay in the original IR."
            }
            Self::OmitReportedReasoningControls | Self::OmitChatReportedSettings => {
                "Incidental settings echoes without a target carrier are omitted; content and required continuation are unchanged."
            }
            Self::OmitResponsesSystemFingerprint => {
                "The Chat-only fingerprint field is omitted; response identity and content are unchanged."
            }
            Self::OmitResponsesMessageEnvelopes => {
                "Native container boundaries are omitted; ordered items and call-result identity remain."
            }
            Self::RegroupChatMessageItems => {
                "Independent message and call items are assembled for Chat; native ownership is not inferred."
            }
            Self::CoarsenChatCompletionCause => {
                "A natural or stop-sequence completion becomes Chat's combined stop, or tool_calls when calls are pending; a matched sequence is not reported."
            }
            Self::OmitResponsesCompletionCause => {
                "Responses completed status has no cause carrier; the outcome and content are unchanged."
            }
            Self::RecitationAsContentFilter => {
                "A recitation stop is reported as the closest OpenAI content_filter reason; the response stays incomplete."
            }
            Self::OmitResponsesInputImageTokens
            | Self::OmitResponsesInputTextTokens
            | Self::OmitResponsesOutputTextTokens
            | Self::OmitResponsesInputAudioTokens
            | Self::OmitResponsesOutputAudioTokens
            | Self::OmitResponsesAcceptedPredictionTokens
            | Self::OmitResponsesRejectedPredictionTokens => {
                "This reported token detail has no target carrier; totals and representable details are unchanged."
            }
        }
    }
}
impl std::str::FromStr for LossRule {
    type Err = RepresentationError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "generation.usage-report-view.v1" => Ok(Self::UsageReportView),
            "responses.omit-incidental-reasoning-controls.v1" => {
                Ok(Self::OmitReportedReasoningControls)
            }
            "chat.omit-incidental-settings.v1" => Ok(Self::OmitChatReportedSettings),
            "responses.omit-system-fingerprint.v1" => Ok(Self::OmitResponsesSystemFingerprint),
            "responses.omit-input-image-tokens.v1" => Ok(Self::OmitResponsesInputImageTokens),
            "responses.omit-input-text-tokens.v1" => Ok(Self::OmitResponsesInputTextTokens),
            "responses.omit-output-text-tokens.v1" => Ok(Self::OmitResponsesOutputTextTokens),
            "responses.omit-input-audio-tokens.v1" => Ok(Self::OmitResponsesInputAudioTokens),
            "responses.omit-output-audio-tokens.v1" => Ok(Self::OmitResponsesOutputAudioTokens),
            "responses.omit-accepted-prediction-tokens.v1" => {
                Ok(Self::OmitResponsesAcceptedPredictionTokens)
            }
            "responses.omit-rejected-prediction-tokens.v1" => {
                Ok(Self::OmitResponsesRejectedPredictionTokens)
            }
            "responses.omit-message-envelopes.v1" => Ok(Self::OmitResponsesMessageEnvelopes),
            "chat.regroup-message-items.v1" => Ok(Self::RegroupChatMessageItems),
            "chat.coarsen-completion-cause.v1" => Ok(Self::CoarsenChatCompletionCause),
            "responses.omit-completion-cause.v1" => Ok(Self::OmitResponsesCompletionCause),
            "openai.recitation-as-content-filter.v1" => Ok(Self::RecitationAsContentFilter),
            _ => Err(RepresentationError::UnknownProjectionRule),
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
