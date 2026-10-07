//! Bounded, body-free evidence for the selected response projection rules.
use super::generation::RepresentationError;
use crate::{protocol::openai::Profile, semantic::task::generation::Usage};

pub const MAX_PROJECTION_STAGES: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LossRule {
    OmitChatInputImageTokens,
}
impl LossRule {
    pub const fn name(self) -> &'static str {
        match self {
            Self::OmitChatInputImageTokens => "chat.omit-input-image-tokens.v1",
        }
    }
    pub const fn owner(self) -> &'static str {
        "generation.usage.input_image_tokens"
    }
    pub const fn consequence(self) -> &'static str {
        "The reported image-token detail is omitted; totals are unchanged."
    }
}
impl std::str::FromStr for LossRule {
    type Err = RepresentationError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "chat.omit-input-image-tokens.v1" => Ok(Self::OmitChatInputImageTokens),
            _ => Err(RepresentationError::UnknownProjectionRule),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionDirection {
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
