//! Upstream response dispatch, independent of the standard client's OpenAI profile.
use crate::protocol::{CodecError, DecodedResponse, anthropic, openai::Adapter};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UpstreamAdapter {
    OpenAi(Box<Adapter>),
    /// Native intake labels opaque values with the trusted Provider/model binding.
    AnthropicMessages {
        profile: anthropic::Profile,
        target: anthropic::ReplayTarget,
    },
}
impl From<Adapter> for UpstreamAdapter {
    fn from(adapter: Adapter) -> Self {
        Self::OpenAi(Box::new(adapter))
    }
}
impl UpstreamAdapter {
    pub fn openai(&self) -> Result<&Adapter, CodecError> {
        match self {
            Self::OpenAi(adapter) => Ok(adapter),
            Self::AnthropicMessages { .. } => Err(CodecError::ProfileMismatch),
        }
    }
    pub fn decode_response(&self, bytes: &[u8]) -> Result<DecodedResponse, CodecError> {
        self.openai()?.decode_response(bytes)
    }
}
