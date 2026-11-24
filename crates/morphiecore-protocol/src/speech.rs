//! Speech request envelopes and pure representation contracts.
pub mod projection;
use crate::semantic::task::speech_synthesis::SpeechRequest;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamFormat {
    Audio,
    Sse,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Request {
    pub model: String,
    pub task: SpeechRequest,
    /// Preserve an explicit delivery carrier separately from task semantics.
    pub stream_format: Option<StreamFormat>,
}
impl Request {
    pub fn new(model: impl Into<String>, task: SpeechRequest) -> Self {
        Self {
            model: model.into(),
            task,
            stream_format: None,
        }
    }
    pub fn streaming(&self) -> bool {
        self.stream_format == Some(StreamFormat::Sse)
    }
}
