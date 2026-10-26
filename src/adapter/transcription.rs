//! Transcription codec dispatch.
use crate::protocol::transcription::Request;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpstreamAdapter {
    AliyunFlash,
    Xiaomi,
}
impl UpstreamAdapter {
    pub fn encode_request(
        self,
        request: &Request,
        model: &str,
    ) -> Result<serde_json::Value, crate::protocol::CodecError> {
        match self {
            Self::AliyunFlash => crate::protocol::aliyun_asr::encode_request(request, model),
            Self::Xiaomi => crate::protocol::xiaomi_audio::encode_transcription(request, model),
        }
    }
    pub fn decode_response(
        self,
        bytes: &[u8],
    ) -> Result<
        crate::semantic::task::speech_recognition::RecognitionResult,
        crate::protocol::CodecError,
    > {
        match self {
            Self::AliyunFlash => crate::protocol::aliyun_asr::decode_response(bytes),
            Self::Xiaomi => crate::protocol::xiaomi_audio::decode_transcription(bytes),
        }
    }
}
