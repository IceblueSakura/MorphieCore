//! Typed Speech boundary; explicit audio delivery is not an SSE or latency promise.
use crate::protocol::failure::ShapeReason;
use crate::semantic::task::speech_synthesis::SpeechRequest;

/// Trusted upstream wire contract, independent of the standard client boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Profile {
    Standard,
    OpenRouterMp3,
    AliyunMp3,
    Xiaomi,
}
impl Profile {
    pub fn encode_request(
        self,
        request: &Request,
        model: &str,
    ) -> Result<serde_json::Value, crate::protocol::CodecError> {
        match self {
            Self::Standard => crate::protocol::openai::speech::encode_request(request, model),
            Self::OpenRouterMp3 => {
                crate::protocol::openrouter_speech::encode_request(request, model)
            }
            Self::AliyunMp3 => crate::protocol::aliyun_speech::encode_request(request, model),
            Self::Xiaomi => crate::protocol::xiaomi_audio::encode_speech(request, model),
        }
    }
    pub fn upstream_sse(self) -> bool {
        self == Self::AliyunMp3
    }
    pub fn reported_encoding(
        self,
        content_type: &str,
        task: &SpeechRequest,
    ) -> Result<Option<crate::semantic::value::AudioEncoding>, crate::protocol::CodecError> {
        if self.upstream_sse() {
            return Err(crate::protocol::CodecError::invalid());
        }
        if self == Self::OpenRouterMp3 {
            crate::protocol::openrouter_speech::validate_task(task)?;
        }
        if self == Self::Xiaomi {
            let mime: mime::Mime = content_type
                .parse()
                .map_err(|_| crate::protocol::CodecError::Invalid(ShapeReason::Json))?;
            if mime.essence_str() != "application/json"
                || mime
                    .params()
                    .any(|(k, v)| k != mime::CHARSET || v != mime::UTF_8)
            {
                return Err(crate::protocol::CodecError::Invalid(ShapeReason::Json));
            }
            return Ok(Some(task.requested_format()));
        }
        crate::protocol::openai::speech::reported_encoding(content_type, task)
    }
    pub(crate) fn supports(
        self,
        capabilities: &crate::lowering::speech::SpeechCapabilities,
    ) -> bool {
        if self == Self::Xiaomi {
            return !capabilities.speed
                && capabilities
                    .formats
                    .iter()
                    .all(|f| crate::protocol::xiaomi_audio::FORMATS.contains(f));
        }
        self == Self::Standard
            || !capabilities.instructions
                && !capabilities.speed
                && capabilities
                    .formats
                    .iter()
                    .all(|f| *f == crate::semantic::value::AudioEncoding::Mp3)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Request {
    pub model: String,
    pub task: SpeechRequest,
    /// Preserve an explicit standard audio carrier separately from task semantics.
    pub explicit_audio_delivery: bool,
}
impl Request {
    pub fn new(model: impl Into<String>, task: SpeechRequest) -> Self {
        Self {
            model: model.into(),
            task,
            explicit_audio_delivery: false,
        }
    }
}
