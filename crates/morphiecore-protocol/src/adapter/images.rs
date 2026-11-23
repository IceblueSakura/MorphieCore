//! Image codec dispatch.
use crate::image::Request;
/// Trusted upstream image wire contract, independent of the public Images operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamAdapter {
    GptImage,
    QwenImage,
    GeminiImage,
    GrokImage,
}
impl UpstreamAdapter {
    pub fn supports_native_static(self) -> bool {
        self != Self::GptImage
    }

    pub fn encode_request(
        self,
        request: &Request,
        model: &str,
    ) -> Result<serde_json::Value, crate::CodecError> {
        match self {
            Self::GptImage => crate::openai::images::encode_request(request, model),
            Self::QwenImage => crate::aliyun_images::encode_request(request, model),
            Self::GeminiImage => crate::image::native::gemini_request(request),
            Self::GrokImage => crate::image::native::grok_request(request, model),
        }
    }
    pub fn decode_response(
        self,
        bytes: &[u8],
    ) -> Result<crate::semantic::task::image_generation::ImageGenerationResponse, crate::CodecError>
    {
        match self {
            Self::GptImage => crate::openai::images::decode_response(bytes),
            Self::QwenImage => crate::aliyun_images::decode_response(bytes),
            Self::GeminiImage => crate::image::native::gemini_response(bytes),
            Self::GrokImage => crate::image::native::grok_response(bytes),
        }
    }
}
