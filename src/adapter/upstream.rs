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
        contract: &GenerationRepresentationContract,
    ) -> Result<Value, AdapterError> {
        match self {
            Self::OpenAi(adapter) => adapter.encode_request(request, model, contract),
            Self::AnthropicMessages(profile) => {
                request.check_native_context(
                    contract.adaptation.rules.opencode_go_headers,
                    contract.cache,
                )?;
                request.check_semantic(&contract.semantics)?;
                request.check_semantic(&profile.semantic_contract())?;
                let origin = contract
                    .replay_origin
                    .as_ref()
                    .ok_or_else(|| CodecError::Unsupported("native replay scope".into()))?;
                let target = anthropic::ReplayTarget::new(origin.as_str(), model)?;
                Ok(profile
                    .encode_generation_request(
                        &request.task.semantic,
                        &request.task.fidelity,
                        &target,
                    )?
                    .value)
            }
        }
    }
    pub(crate) fn request_headers(
        &self,
        request: &Request,
        cache: crate::protocol::cache::CacheProjection,
    ) -> Result<Vec<(String, String)>, CodecError> {
        self.openai()?.request_headers(request, cache)
    }
    pub fn decode_response(&self, bytes: &[u8]) -> Result<DecodedResponse, CodecError> {
        self.openai()?.decode_response(bytes)
    }
}
