//! Transcription codec dispatch.
use crate::transcription::Request;
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamAdapter {
    AliyunFlash,
    Xiaomi,
}
impl UpstreamAdapter {
    pub fn encode_request(
        self,
        request: &Request,
        model: &str,
    ) -> Result<serde_json::Value, crate::CodecError> {
        match self {
            Self::AliyunFlash => crate::aliyun_asr::encode_request(request, model),
            Self::Xiaomi => crate::xiaomi_audio::encode_transcription(request, model),
        }
    }
    pub fn decode_response(
        self,
        bytes: &[u8],
    ) -> Result<crate::semantic::task::speech_recognition::RecognitionResult, crate::CodecError>
    {
        match self {
            Self::AliyunFlash => crate::aliyun_asr::decode_response(bytes),
            Self::Xiaomi => crate::xiaomi_audio::decode_transcription(bytes),
        }
    }
}
