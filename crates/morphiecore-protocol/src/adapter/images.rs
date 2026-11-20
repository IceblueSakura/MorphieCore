//! Image codec dispatch.
use crate::image::Request;
/// Trusted upstream image wire contract, independent of the public Images operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpstreamAdapter {
    GptImage,
    OpenRouterFlare,
}
impl UpstreamAdapter {
    pub fn encode_request(
        self,
        request: &Request,
        model: &str,
    ) -> Result<serde_json::Value, crate::CodecError> {
        match self {
            Self::GptImage => crate::openai::images::encode_request(request, model),
            Self::OpenRouterFlare => crate::openrouter_images::encode_request(request, model),
        }
    }
    pub fn decode_response(
        self,
        bytes: &[u8],
    ) -> Result<crate::semantic::task::image_generation::ImageGenerationResponse, crate::CodecError>
    {
        match self {
            Self::GptImage => crate::openai::images::decode_response(bytes),
            Self::OpenRouterFlare => crate::openrouter_images::decode_response(bytes),
        }
    }
}
