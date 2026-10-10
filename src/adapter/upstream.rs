//! Upstream wire dispatch, independent of the standard client's OpenAI profile.
use super::{Adapter, AdapterError, Request};
use crate::{
    lowering::generation::GenerationRepresentationContract,
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
    /// Native semantic mapping is a separate slice; never treat native wire as Chat.
    pub fn openai(&self) -> Result<&Adapter, CodecError> {
        match self {
            Self::OpenAi(adapter) => Ok(adapter),
            Self::AnthropicMessages(_) => {
                Err(CodecError::Unsupported("Anthropic semantic mapping".into()))
            }
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
        contract: &GenerationRepresentationContract,
    ) -> Result<Value, AdapterError> {
        self.openai()?.encode_request(request, model, contract)
    }
    pub(crate) fn request_headers(
        &self,
        request: &Request,
    ) -> Result<Vec<(String, String)>, CodecError> {
        self.openai()?.request_headers(request)
    }
    pub fn decode_response(&self, bytes: &[u8]) -> Result<DecodedResponse, CodecError> {
        self.openai()?.decode_response(bytes)
    }
}
