//! Upstream wire dispatch, independent of the standard client's OpenAI profile.
use super::{AdapterError, Request};
use crate::protocol::openai::Adapter;
use crate::{
    lowering::target::TargetCaps,
    protocol::{CodecError, DecodedResponse, anthropic},
};
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UpstreamAdapter {
    OpenAi(Box<Adapter>),
    AnthropicMessages(anthropic::Profile),
}
impl From<Adapter> for UpstreamAdapter {
    fn from(adapter: Adapter) -> Self {
        Self::OpenAi(Box::new(adapter))
    }
}
impl UpstreamAdapter {
    /// The generic native adapter requires explicit context; never treat it as Chat.
    pub fn openai(&self) -> Result<&Adapter, CodecError> {
        match self {
            Self::OpenAi(adapter) => Ok(adapter),
            Self::AnthropicMessages(_) => Err(CodecError::Unsupported(
                "native adapter requires explicit request context".into(),
            )),
        }
    }
    pub fn messages(&self) -> Result<anthropic::Profile, CodecError> {
        match self {
            Self::AnthropicMessages(profile) => Ok(*profile),
            Self::OpenAi(_) => Err(CodecError::ProfileMismatch),
        }
    }
    pub fn encode_request(
        &self,
        request: &Request,
        model: &str,
        contract: &TargetCaps,
    ) -> Result<Value, AdapterError> {
        match (self, contract) {
            (Self::OpenAi(adapter), TargetCaps::OpenAi(caps)) => {
                adapter.encode_request(request, model, caps)
            }
            (Self::AnthropicMessages(profile), TargetCaps::AnthropicMessages(caps))
                if *profile == caps.profile =>
            {
                caps.encode_request(request, model)
            }
            _ => Err(CodecError::ProfileMismatch.into()),
        }
    }
    pub(crate) fn request_headers(
        &self,
        request: &Request,
        caps: &TargetCaps,
    ) -> Result<Vec<(String, String)>, CodecError> {
        match (self, caps) {
            (Self::OpenAi(adapter), TargetCaps::OpenAi(caps)) => {
                adapter.request_headers(request, caps.cache)
            }
            (Self::AnthropicMessages(profile), TargetCaps::AnthropicMessages(caps))
                if *profile == caps.profile =>
            {
                caps.request_headers(request)
            }
            _ => Err(CodecError::ProfileMismatch),
        }
    }
    pub fn decode_response(&self, bytes: &[u8]) -> Result<DecodedResponse, CodecError> {
        self.openai()?.decode_response(bytes)
    }
}
