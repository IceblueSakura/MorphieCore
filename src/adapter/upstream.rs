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
                let origin = contract
                    .replay_origin
                    .as_ref()
                    .ok_or_else(|| CodecError::Unsupported("native replay scope".into()))?;
                let target = anthropic::ReplayTarget::new(origin.as_str(), model)?;
                let stripped = request.without_foreign_replay(Some(&target.origin()))?;
                let request = stripped.as_ref().unwrap_or(request);
                request.check_native_context(
                    contract.adaptation.rules.opencode_go_headers,
                    contract.cache,
                )?;
                request.check_semantic(&contract.semantics)?;
                request.check_semantic(&profile.semantic_contract())?;
                let mut semantic = profile.group_client_history(&request.task.semantic)?;
                let mut settings = semantic.settings().clone();
                // Native signatures are always returned; OpenAI's include flag
                // selects the downstream carrier, not a native thinking control.
                settings.reasoning = settings.reasoning.with_encrypted_output(false);
                if contract.adaptation.rules.reject_reasoning_controls
                    && settings.reasoning != Default::default()
                {
                    return Err(CodecError::Unsupported(
                        "unverified native reasoning controls".into(),
                    )
                    .into());
                }
                semantic = semantic.with_settings(settings).map_err(CodecError::from)?;
                let mut wire = profile
                    .encode_generation_request(&semantic, &request.task.fidelity, &target)?
                    .value;
                if let Some(stream) = request.delivery.stream.value() {
                    wire["stream"] = serde_json::json!(stream);
                }
                Ok(wire)
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
