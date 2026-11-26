//! Closed inference task family.
pub mod decision;
pub mod embedding;
pub mod generation;
pub mod image_generation;
pub mod speech_recognition;
pub mod speech_synthesis;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputModality {
    Unspecified,
    Text,
    Image,
    Audio,
    Video,
    Document,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModalityTokenCount {
    pub modality: InputModality,
    pub tokens: u64,
}

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
