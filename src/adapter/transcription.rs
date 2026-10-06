//! Standard transcription identity and task; only synchronous JSON is admitted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Profile {
    AliyunFlash,
}
impl Profile {
    pub fn encode_request(
        self,
        request: &Request,
        model: &str,
    ) -> Result<serde_json::Value, crate::protocol::CodecError> {
        match self {
            Self::AliyunFlash => crate::protocol::aliyun_asr::encode_request(request, model),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Request {
    pub model: String,
    pub task: crate::semantic::task::speech_recognition::RecognitionRequest,
}
