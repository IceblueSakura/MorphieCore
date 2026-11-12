//! Explicit task activation consumed by `Gateway::new`.
use crate::{protocol::ClientProtocol, topology::EndpointId};
/// One public Generation model exposed through one client protocol and served
/// by one endpoint of its route. Startup emits every client protocol for each
/// configured candidate; tests may expose a narrower set.
#[derive(Clone, Debug)]
pub struct Entry {
    pub model: String,
    pub protocol: ClientProtocol,
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
