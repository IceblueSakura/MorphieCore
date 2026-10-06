//! Closed inference task family.
pub mod generation;
pub mod image_generation;
pub mod speech_recognition;
pub mod speech_synthesis;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskKind {
    Generation,
    Embedding,
    ImageGeneration,
    SpeechRecognition,
    SpeechSynthesis,
    VoiceDesign,
    VoiceClone,
}
