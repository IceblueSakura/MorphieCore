//! Minimal HTTP ingress over immutable, explicitly bound task entries.
mod admission;
mod aliyun_speech;
mod auth;
mod body;
pub mod bootstrap;
mod config;
#[cfg(test)]
mod credential_fallback_tests;
mod credentials;
mod diagnostics;
mod error;
mod exchange;
mod http;
mod images;
mod intake;
#[cfg(test)]
mod model_tests;
mod models;
#[cfg(test)]
mod route_fallback_tests;
#[cfg(test)]
mod shutdown_tests;
mod speech;
#[cfg(test)]
mod speech_tests;
#[cfg(test)]
mod subscription_tests;
#[cfg(test)]
mod tests;
mod transcription;
#[cfg(test)]
mod transcription_tests;
#[cfg(test)]
use crate::provider::SecretMaterial;
use crate::{
    adapter::Adapter,
    lowering::generation::GenerationRepresentationContract,
    protocol::openai::Profile,
    provider::ProviderDefinition,
    topology::{Endpoint, PublicModel},
    transport::http::HttpTransport,
};
pub use config::{Entry, ImageEntry, Limits, SpeechEntry, StartupError, TranscriptionEntry};
pub use credentials::Credentials;
use error::ApiError;
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
struct BoundCandidate {
    endpoint: Endpoint,
    provider: ProviderDefinition,
    secret: credentials::Source,
    credential_fallback: bool,
}
struct BoundEntry {
    public: PublicModel,
    client: Adapter,
    downstream: GenerationRepresentationContract,
    policy: crate::topology::RoutePolicy,
    candidates: Vec<Arc<BoundCandidate>>,
}
impl BoundEntry {
    fn eligible(
        &self,
        request: &crate::adapter::Request,
    ) -> Result<Vec<Arc<BoundCandidate>>, ApiError> {
        // No shared model scope can authorize another issuer's opaque continuation.
        if self.candidates.len() > 1
            && (request.task.semantic.reasoning().encrypted_output()
                || request
                    .task
                    .semantic
                    .items()
                    .iter()
                    .any(|(_, item)| matches!(item, crate::semantic::task::generation::Item::Reasoning(r) if r.replay.is_some())))
        {
            return Err(ApiError::invalid());
        }
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
struct Runtime {
    diagnostics: Option<diagnostics::Sink>,
    auth: auth::Auth,
    entries: BTreeMap<(u8, String), Arc<BoundEntry>>,
    images: BTreeMap<String, Arc<images::BoundImage>>,
    speech: BTreeMap<String, Arc<speech::BoundSpeech>>,
    transcriptions: BTreeMap<String, Arc<transcription::BoundTranscription>>,
    models: models::ModelView,
    limits: Limits,
    permits: Arc<Semaphore>,
    transport: HttpTransport,
    shutdown: CancellationToken,
}
#[derive(Clone)]
pub struct Gateway {
    state: Arc<Runtime>,
}
fn family(profile: Profile) -> u8 {
    match profile {
        Profile::Chat => 0,
        Profile::Responses => 1,
    }
}
