//! Protocol-neutral image request envelope for Gateway and typed consumers.
use crate::semantic::{
    context::{ClientIdentityHints, DeliveryIntent},
    task::image_generation::ImageGenerationRequest,
};
#[derive(Clone, Eq, PartialEq)]
pub struct Request {
    pub model: String,
    pub task: ImageGenerationRequest,
    pub delivery: DeliveryIntent,
    pub identity: ClientIdentityHints,
}
impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageRequest")
            .field("model", &self.model)
            .field("task", &self.task)
            .field("delivery", &self.delivery)
            .field("identity", &"[redacted]")
            .finish()
    }
}
/// Trusted upstream image wire contract, independent of the public Images operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Profile {
    GptImage,
    OpenRouterFlare,
}
impl Profile {
    pub fn encode_request(
        self,
        request: &Request,
        model: &str,
    ) -> Result<serde_json::Value, crate::protocol::CodecError> {
        match self {
            Self::GptImage => crate::protocol::openai::images::encode_request(request, model),
            Self::OpenRouterFlare => {
                crate::protocol::openrouter_images::encode_request(request, model)
            }
        }
    }
    pub fn decode_response(
        self,
        bytes: &[u8],
    ) -> Result<
        crate::semantic::task::image_generation::ImageGenerationResponse,
        crate::protocol::CodecError,
    > {
        match self {
            Self::GptImage => crate::protocol::openai::images::decode_response(bytes),
            Self::OpenRouterFlare => crate::protocol::openrouter_images::decode_response(bytes),
        }
    }
}
impl Request {
    pub fn new(model: impl Into<String>, task: ImageGenerationRequest) -> Self {
        Self {
            model: model.into(),
            task,
            delivery: DeliveryIntent::default(),
            identity: ClientIdentityHints::default(),
        }
    }
}
