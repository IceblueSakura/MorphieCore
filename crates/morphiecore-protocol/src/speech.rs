//! Speech request envelopes and pure representation contracts.
pub mod projection;
use crate::semantic::task::speech_synthesis::SpeechRequest;
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
