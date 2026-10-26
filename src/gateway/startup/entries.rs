//! Explicit task activation consumed by `Gateway::new`.
use crate::{adapter::ClientProtocol, protocol::openai::Dialect, topology::EndpointId};
/// Trusted public-model contract; independent of upstream dialects.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatProfile {
    #[default]
    Standard,
    ReadableReasoning,
}
impl ChatProfile {
    pub(super) fn dialect(self) -> Dialect {
        match self {
            Self::Standard => Dialect::Standard,
            Self::ReadableReasoning => Dialect::ChatReasoning,
        }
    }
}
/// One public Generation model exposed through one client protocol and served
/// by one endpoint of its route. Startup emits every client protocol for each
/// configured candidate; tests may expose a narrower set.
#[derive(Clone, Debug)]
pub struct Entry {
    pub model: String,
    pub protocol: ClientProtocol,
    /// Chat client dialect; Responses and Messages always use their standard one.
    pub chat_profile: ChatProfile,
    pub endpoint: EndpointId,
}
#[derive(Clone, Debug)]
pub struct ImageEntry {
    pub model: String,
}
#[derive(Clone, Debug)]
pub struct SpeechEntry {
    pub model: String,
}
#[derive(Clone, Debug)]
pub struct TranscriptionEntry {
    pub model: String,
    pub binding: Option<EndpointId>,
}
#[derive(Clone, Debug)]
pub struct EmbeddingEntry {
    pub model: String,
}
/// Explicit task activation. Empty task lists activate nothing.
#[derive(Clone, Debug, Default)]
pub struct TaskEntries {
    pub generation: Vec<Entry>,
    pub images: Vec<ImageEntry>,
    pub speech: Vec<SpeechEntry>,
    pub transcriptions: Vec<TranscriptionEntry>,
    pub embeddings: Vec<EmbeddingEntry>,
}
