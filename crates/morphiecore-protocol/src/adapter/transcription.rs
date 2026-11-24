//! Transcription codec dispatch.
use crate::{
    CodecError,
    semantic::task::speech_recognition::{OutputFormat, RecognitionResult},
    transcription::Request,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamAdapter {
    Standard,
    AliyunFlash,
    Xiaomi,
}

pub struct EncodedBody {
    pub content_type: String,
    pub body: Vec<u8>,
}

impl UpstreamAdapter {
    pub fn supports_streaming(self) -> bool {
        self == Self::Standard
    }

    pub fn expected_response_media(self, format: OutputFormat, stream: bool) -> &'static str {
        match (self, format, stream) {
            (Self::Standard, _, true) => "text/event-stream",
            (Self::Standard, OutputFormat::Text, false) => "text/plain",
            _ => "application/json",
        }
    }

    pub fn encode_request(
        self,
        request: &Request,
        model: &str,
        limit: usize,
    ) -> Result<EncodedBody, CodecError> {
        if self == Self::Standard {
            let encoded = crate::openai::transcription::encode_request(request, model, limit)?;
            return Ok(EncodedBody {
                content_type: encoded.content_type,
                body: encoded.body,
            });
        }
        let value = match self {
            Self::AliyunFlash => crate::aliyun_asr::encode_request(request, model)?,
            Self::Xiaomi => crate::xiaomi_audio::encode_transcription(request, model)?,
            Self::Standard => unreachable!(),
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
        format: OutputFormat,
    ) -> Result<RecognitionResult, CodecError> {
        match self {
            Self::Standard => crate::openai::transcription::decode_response(bytes, format),
            Self::AliyunFlash => crate::aliyun_asr::decode_response(bytes),
            Self::Xiaomi => crate::xiaomi_audio::decode_transcription(bytes),
        }
    }
}
