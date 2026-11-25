//! Image codec dispatch.
use crate::{CodecError, image::Request};
/// Trusted upstream image wire contract, independent of the public Images operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamAdapter {
    GptImage,
    QwenImage,
    GeminiImage,
    GrokImage,
}

pub struct EncodedBody {
    pub content_type: String,
    pub body: Vec<u8>,
}

impl UpstreamAdapter {
    pub fn supports_native_static(self) -> bool {
        self != Self::GptImage
    }
    pub fn supports_streaming(self) -> bool {
        self == Self::GptImage
    }

    pub fn encode_request(
        self,
        request: &Request,
        model: &str,
        limit: usize,
    ) -> Result<EncodedBody, CodecError> {
        if self == Self::GptImage && request.task.is_edit() {
            let encoded = crate::openai::images::encode_edit_request(request, model, limit)?;
            return Ok(EncodedBody {
                content_type: encoded.content_type,
                body: encoded.body,
            });
        }
        let value = match self {
            Self::GptImage => crate::openai::images::encode_request(request, model)?,
            Self::QwenImage => crate::aliyun_images::encode_request(request, model)?,
            Self::GeminiImage => crate::image::native::gemini_request(request, model)?,
            Self::GrokImage => crate::image::native::grok_request(request, model)?,
        };
        crate::semantic::value::json_size(&value, limit).map_err(|_| CodecError::Limit)?;
        let body = serde_json::to_vec(&value).map_err(|_| CodecError::invalid())?;
        if body.len() > limit {
            return Err(CodecError::Limit);
        }
        Ok(EncodedBody {
            content_type: "application/json".into(),
            body,
        })
    }
    pub fn decode_response(
        self,
        bytes: &[u8],
        expected_inputs: usize,
    ) -> Result<crate::semantic::task::image_generation::ImageGenerationResponse, CodecError> {
        match self {
            Self::GptImage => crate::openai::images::decode_response(bytes),
            Self::QwenImage => crate::aliyun_images::decode_response(bytes, expected_inputs),
            Self::GeminiImage => crate::image::native::gemini_response(bytes),
            Self::GrokImage => crate::image::native::grok_response(bytes),
        }
    }
}
