//! Bounded, body-free evidence for the selected response projection rules.
use super::generation::RepresentationError;
use crate::{protocol::openai::Profile, semantic::task::generation::Usage};

pub const MAX_PROJECTION_STAGES: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LossRule {
    OmitResponsesSystemFingerprint,
    OmitChatInputImageTokens,
    OmitResponsesInputImageTokens,
    OmitResponsesInputTextTokens,
    OmitResponsesOutputTextTokens,
    OmitResponsesInputAudioTokens,
    OmitResponsesOutputAudioTokens,
    OmitResponsesAcceptedPredictionTokens,
    OmitResponsesRejectedPredictionTokens,
    OmitResponsesMessageEnvelopes,
    RegroupChatMessageItems,
}
impl LossRule {
    pub const fn name(self) -> &'static str {
        match self {
            Self::OmitResponsesSystemFingerprint => "responses.omit-system-fingerprint.v1",
            Self::OmitChatInputImageTokens => "chat.omit-input-image-tokens.v1",
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
        }
    }
    pub const fn owner(self) -> &'static str {
        match self {
            Self::OmitResponsesSystemFingerprint => "response.context.system_fingerprint",
            Self::OmitChatInputImageTokens | Self::OmitResponsesInputImageTokens => {
                "generation.usage.input_image_tokens"
            }
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
        }
    }
    pub const fn consequence(self) -> &'static str {
        match self {
            Self::OmitResponsesSystemFingerprint => {
                "The Chat-only fingerprint field is omitted; response identity and content are unchanged."
            }
            Self::OmitChatInputImageTokens => {
                "The reported image-token detail is omitted; totals are unchanged."
            }
            Self::OmitResponsesMessageEnvelopes => {
                "Native container boundaries are omitted; ordered items and call-result identity remain."
            }
            Self::RegroupChatMessageItems => {
                "Independent message and call items are assembled for Chat; native ownership is not inferred."
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
            "responses.omit-system-fingerprint.v1" => Ok(Self::OmitResponsesSystemFingerprint),
            "chat.omit-input-image-tokens.v1" => Ok(Self::OmitChatInputImageTokens),
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

/// Validation precedes omission, including for zero-valued reported details.
pub(crate) fn chat_usage(
    usage: Usage,
    image_carrier: bool,
) -> Result<(Usage, Option<LossRule>), RepresentationError> {
    usage.validate()?;
    if !image_carrier && usage.input_image_tokens.is_some() {
        Ok((
            Usage {
                input_image_tokens: None,
                ..usage
            },
            Some(LossRule::OmitChatInputImageTokens),
        ))
    } else {
        Ok((usage, None))
    }
}

/// Fixed Responses only carries cache and reasoning breakdowns. Named profiles
/// can additionally carry image/text counts; admission is not enlarged here.
/// https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses/response_usage.py
pub(crate) fn responses_usage(
    mut usage: Usage,
    rules: &crate::protocol::adaptation::WireRules,
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
