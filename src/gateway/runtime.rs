//! Bound entries and request-independent gateway state.
use super::{
    ApiError, Limits, diagnostics,
    ingress::{auth, models},
    routing::{executors, prefix_cache as affinity},
    startup::credentials,
    tasks::{embeddings, images, speech, transcription},
};
use crate::{
    adapter::{ClientAdapter, ClientProtocol},
    protocol::openai::Profile,
    provider::ProviderDefinition,
    topology::{Endpoint, PublicModel},
    transport::http::HttpTransport,
};
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
pub(super) struct BoundCandidate {
    pub(super) endpoint: Endpoint,
    pub(super) provider: ProviderDefinition,
    pub(super) secret: credentials::Source,
    pub(super) credential_fallback: bool,
    pub(super) cache_scope: crate::semantic::value::ReplayOrigin,
}
#[derive(Clone)]
pub(super) struct BoundEntry {
    pub(super) public: PublicModel,
    pub(super) client: ClientAdapter,
    pub(super) policy: crate::topology::RoutePolicy,
    pub(super) candidates: Vec<Arc<BoundCandidate>>,
}
impl BoundEntry {
    pub(super) fn eligible(
        &self,
        request: &crate::adapter::Request,
    ) -> Result<Vec<Arc<BoundCandidate>>, ApiError> {
        let selected = crate::execution::plan::select_candidates(
            request,
            self.policy.candidates,
            self.candidates.iter().map(|c| &c.endpoint),
        )
        .map_err(|_| ApiError::invalid())?;
        Ok(selected
            .candidates
            .iter()
            .map(|planned| self.candidates[planned.ordinal].clone())
            .collect())
    }
}
pub(super) struct Runtime {
    pub(super) history: Arc<crate::gateway::generation::history::Store>,
    pub(super) diagnostics: Option<diagnostics::Sink>,
    pub(super) affinity: affinity::Index,
    pub(super) executors: executors::Executors,
    pub(super) auth: auth::Auth,
    pub(super) entries: BTreeMap<(ClientProtocol, String), Arc<BoundEntry>>,
    pub(super) images: BTreeMap<String, Arc<images::BoundImage>>,
    pub(super) speech: BTreeMap<String, Arc<speech::BoundSpeech>>,
    pub(super) transcriptions: BTreeMap<String, Arc<transcription::BoundTranscription>>,
    pub(super) embeddings: BTreeMap<String, Arc<embeddings::BoundEmbedding>>,
    pub(super) models: models::ModelView,
    pub(super) limits: Limits,
    pub(super) permits: Arc<Semaphore>,
    pub(super) transport: HttpTransport,
    pub(super) shutdown: CancellationToken,
}
pub(super) fn family(profile: Profile) -> ClientProtocol {
    profile.into()
}
pub(super) const MESSAGES_FAMILY: ClientProtocol = ClientProtocol::Messages;
