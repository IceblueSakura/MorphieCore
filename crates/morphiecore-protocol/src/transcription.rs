//! Native transcription envelope, independent of target selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Request {
    pub model: String,
    pub task: crate::semantic::task::speech_recognition::RecognitionRequest,
}
