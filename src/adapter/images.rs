//! Image codec dispatch.
use crate::protocol::image::Request;
/// Trusted upstream image wire contract, independent of the public Images operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpstreamAdapter {
    GptImage,
    OpenRouterFlare,
    GrokImagineImage2,
}
impl UpstreamAdapter {
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
            Self::GrokImagineImage2 => {
                crate::protocol::grok::images::encode_request(request, model)
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
            Self::GrokImagineImage2 => crate::protocol::grok::images::decode_response(bytes),
        }
    }
}
