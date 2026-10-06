//! Startup-only resolution of immutable entry, contract and credential ownership.
use super::{BoundCandidate, BoundEntry, Credentials, Gateway, Runtime, auth, family};
use crate::{
    adapter::{Adapter, Dialect},
    lowering::generation::GenerationRepresentationContract,
    protocol::openai::Profile,
    provider::SecretMaterial,
    semantic::value::ReplayOrigin,
    topology::{CompiledTopology, EndpointId},
    transport::http::HttpTransport,
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
#[derive(Clone, Debug)]
pub struct Entry {
    pub model: String,
    pub protocol: Profile,
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
}
#[derive(Clone, Debug)]
pub struct Limits {
    pub request_bytes: usize,
    pub response_bytes: usize,
    /// Decoded output bytes per image; cannot enlarge response or endpoint ceilings.
    pub image_bytes: usize,
    /// Aggregate decoded output bytes, independent of count and JSON limits.
    pub images_bytes: usize,
    /// Complete binary audio bytes, also bounded by endpoint and response ceilings.
    pub speech_bytes: usize,
    /// Whole multipart envelope, independently bounded from JSON ingress.
    pub transcription_request_bytes: usize,
    pub event_bytes: usize,
    pub max_events: usize,
    pub concurrency: usize,
    pub body_timeout: Duration,
    pub exchange_timeout: Duration,
    pub default_output_tokens: u64,
    pub max_output_tokens: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            request_bytes: 256 << 10,
            response_bytes: 8 << 20,
            image_bytes: 2 << 20,
            images_bytes: 2 << 20,
            speech_bytes: 8 << 20,
            transcription_request_bytes: crate::protocol::openai::transcription::MAX_UPLOAD_BYTES,
            event_bytes: 1 << 20,
            max_events: 65_536,
            concurrency: 16,
            body_timeout: Duration::from_secs(10),
            exchange_timeout: Duration::from_secs(120),
            default_output_tokens: 1024,
            max_output_tokens: 16384,
        }
    }
}
#[cfg(test)]
#[test]
fn image_limits_keep_defaults_and_reject_out_of_range_configuration() {
    let mut limits = Limits::default();
    assert_eq!(limits.image_bytes, 2 << 20);
    limits.image_bytes = 0;
    assert!(!limits.validate());
    limits.image_bytes = crate::semantic::task::image_generation::MAX_IMAGE_BYTES;
    assert!(limits.validate());
    limits.image_bytes += 1;
    assert!(!limits.validate());
    limits.image_bytes = 1;
    assert_eq!(limits.images_bytes, 2 << 20);
    limits.images_bytes = 0;
    assert!(!limits.validate());
    limits.images_bytes = crate::semantic::task::image_generation::MAX_IMAGES_BYTES;
    assert!(limits.validate());
    limits.images_bytes += 1;
    assert!(!limits.validate());
}
impl Limits {
    fn validate(&self) -> bool {
        (1..=4 << 20).contains(&self.request_bytes)
            && (1..=64 << 20).contains(&self.response_bytes)
            && (1..=crate::semantic::task::image_generation::MAX_IMAGE_BYTES)
                .contains(&self.image_bytes)
            && (1..=crate::semantic::task::image_generation::MAX_IMAGES_BYTES)
                .contains(&self.images_bytes)
            && (1..=crate::semantic::value::MAX_AUDIO_BYTES).contains(&self.speech_bytes)
            && (1..=crate::protocol::openai::transcription::MAX_UPLOAD_BYTES)
                .contains(&self.transcription_request_bytes)
            && (1..=4 << 20).contains(&self.event_bytes)
            && (1..=1_000_000).contains(&self.max_events)
            && (1..=256).contains(&self.concurrency)
            && !self.body_timeout.is_zero()
            && self.body_timeout <= Duration::from_secs(60)
            && !self.exchange_timeout.is_zero()
            && self.exchange_timeout <= Duration::from_secs(300)
            && self.default_output_tokens > 0
            && self.default_output_tokens <= self.max_output_tokens
            && self.max_output_tokens <= 1_000_000
    }
}
#[derive(Clone, Copy, Debug, thiserror::Error)]
pub enum StartupError {
    #[error("invalid gateway limits")]
    Limits,
    #[error("missing or invalid credential configuration")]
    Credentials,
    #[error("invalid or duplicate fixed entry binding")]
    Binding,
    #[error("invalid HTTP transport configuration")]
    Transport,
    #[error("invalid loopback listener configuration")]
    Listener,
    #[error("invalid startup environment")]
    Environment,
}
impl Gateway {
    /// Enable bounded metadata in a new private operator file, before sharing the gateway.
    pub fn with_probe_diagnostics(mut self, path: &std::path::Path) -> Result<Self, StartupError> {
        let state = Arc::get_mut(&mut self.state).ok_or(StartupError::Environment)?;
        state.diagnostics =
            Some(super::diagnostics::Sink::open(path).map_err(|_| StartupError::Environment)?);
        Ok(self)
    }
    /// Best-effort bounded drain, for an owner shutting down its test listener.
    pub async fn flush_probe_diagnostics(&self) {
        if let Some(sink) = &self.state.diagnostics {
            sink.flush().await;
        }
    }
    pub fn new(
        topology: CompiledTopology,
        entries: Vec<Entry>,
        credentials: impl Into<Credentials>,
        client_key: SecretMaterial,
        limits: Limits,
        proxy: Option<&str>,
    ) -> Result<Self, StartupError> {
        Self::new_with_images(
            topology,
            entries,
            vec![],
            credentials,
            client_key,
            limits,
            proxy,
        )
    }
    /// Explicit embedded activation; no image catalog or private-file discovery is implied.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_images(
        topology: CompiledTopology,
        entries: Vec<Entry>,
        image_entries: Vec<ImageEntry>,
        credentials: impl Into<Credentials>,
        client_key: SecretMaterial,
        limits: Limits,
        proxy: Option<&str>,
    ) -> Result<Self, StartupError> {
        Self::new_with_media(
            topology,
            entries,
            image_entries,
            vec![],
            vec![],
            credentials,
            client_key,
            limits,
            proxy,
        )
    }
    /// Explicit task activation. No product catalog, file discovery or implicit media permission.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_media(
        topology: CompiledTopology,
        entries: Vec<Entry>,
        image_entries: Vec<ImageEntry>,
        speech_entries: Vec<SpeechEntry>,
        transcription_entries: Vec<TranscriptionEntry>,
        credentials: impl Into<Credentials>,
        client_key: SecretMaterial,
        limits: Limits,
        proxy: Option<&str>,
    ) -> Result<Self, StartupError> {
        let credentials = credentials.into();
        let images = super::images::bind(&topology, image_entries, &credentials)?;
        let speech = super::speech::bind(&topology, speech_entries, &credentials)?;
        let transcriptions =
            super::transcription::bind(&topology, transcription_entries, &credentials)?;
        if !limits.validate() {
            return Err(StartupError::Limits);
        }
        let auth = auth::Auth::new(client_key)?;
        let transport = HttpTransport::new(proxy).map_err(|_| StartupError::Transport)?;
        let mut bound: BTreeMap<(u8, String), BoundEntry> = BTreeMap::new();
        for entry in entries {
            let public = topology
                .model(&entry.model)
                .ok_or(StartupError::Binding)?
                .clone();
            let route = topology.route(&public.route).ok_or(StartupError::Binding)?;
            if !route.endpoints.contains(&entry.endpoint) {
                return Err(StartupError::Binding);
            }
            let endpoint = topology
                .endpoint(&entry.endpoint)
                .ok_or(StartupError::Binding)?
                .clone();
            let provider = topology
                .provider(endpoint.provider.as_str())
                .ok_or(StartupError::Binding)?
                .clone();
            let binding = credentials
                .get(&endpoint.credential)
                .ok_or(StartupError::Credentials)?;
            if bound
                .get(&(family(entry.protocol), entry.model.clone()))
                .is_some_and(|group| {
                    group
                        .candidates
                        .iter()
                        .any(|candidate| candidate.endpoint.id == endpoint.id)
                })
            {
                return Err(StartupError::Binding);
            }
            for source in &binding.sources {
                source.check(endpoint.execution.credential_kind, provider.id.as_str())?;
            }
            let count = if binding.fallback {
                binding.max_attempts
            } else {
                1
            };
            for secret in binding.sources.iter().take(count) {
                let secret = secret.clone();
                let mut endpoint = endpoint.clone();
                let provenance = secret.provenance();
                // Length prefixes avoid ambiguous concatenation. Internal provenance
                // is neither a credential locator nor attestation of token issuance.
                let mut hash = Sha256::new();
                hash.update(auth.digest);
                for value in [
                    provenance.as_str(),
                    public.id.as_str(),
                    provider.id.as_str(),
                    endpoint.upstream_model.as_str(),
                ] {
                    hash.update((value.len() as u64).to_le_bytes());
                    hash.update(value.as_bytes());
                }
                let digest: String = hash
                    .finalize()
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect();
                let scope = ReplayOrigin::new(&format!("gateway-{digest}"))
                    .map_err(|_| StartupError::Binding)?;
                endpoint.representation.adaptation.scope = Some(scope.clone());
                if endpoint.representation.replay_origin.is_some() {
                    endpoint.representation.replay_origin = Some(scope.clone());
                }
                // Provider compatibility does not authorize private downstream
                // Responses carriers. Chat retains its explicit compatibility profile.
                let dialect = match entry.protocol {
                    Profile::Responses => Dialect::Standard,
                    Profile::Chat => Dialect::MorphieCore,
                };
                let client = Adapter::new(entry.protocol, dialect, Some(scope.clone()));
                // Model input admission must not filter facts reported on the client wire
                // (e.g. empty logprobs or defaults a model cannot accept as controls).
                let downstream = GenerationRepresentationContract {
                    replay_origin: Some(scope),
                    reported_facts: public.reported_facts,
                    ..GenerationRepresentationContract::full()
                };
                let group = bound
                    .entry((family(entry.protocol), entry.model.clone()))
                    .or_insert_with(|| BoundEntry {
                        public: public.clone(),
                        client,
                        downstream,
                        policy: route.policy.clone(),
                        candidates: vec![],
                    });
                if group.candidates.len() >= crate::execution::plan::MAX_CANDIDATES {
                    return Err(StartupError::Binding);
                }
                group.candidates.push(Arc::new(BoundCandidate {
                    endpoint,
                    provider: provider.clone(),
                    secret,
                    credential_fallback: binding.fallback,
                }));
            }
        }
        if bound.is_empty() && images.is_empty() && speech.is_empty() && transcriptions.is_empty() {
            return Err(StartupError::Binding);
        }
        let mut activated = BTreeMap::new();
        for (key, mut group) in bound {
            let route = topology
                .route(&group.public.route)
                .ok_or(StartupError::Binding)?;
            group.candidates.sort_by_key(|c| {
                route
                    .endpoints
                    .iter()
                    .position(|id| id == &c.endpoint.id)
                    .expect("validated member")
            });
            if group.candidates.len() > 1 {
                group.client.adaptation.scope = None;
                group.downstream.replay_origin = None;
            }
            activated.insert(key, Arc::new(group));
        }
        let shutdown = CancellationToken::new();
        let permits = Arc::new(Semaphore::new(limits.concurrency));
        let models = super::models::ModelView::bind(
            &topology,
            activated
                .keys()
                .map(|(_, label)| label.as_str())
                .chain(images.keys().map(String::as_str))
                .chain(speech.keys().map(String::as_str))
                .chain(transcriptions.keys().map(String::as_str)),
            limits.response_bytes,
        )?;
        Ok(Self {
            state: Arc::new(Runtime {
                diagnostics: None,
                auth,
                entries: activated,
                images,
                speech,
                transcriptions,
                models,
                limits,
                permits,
                transport,
                shutdown,
            }),
        })
    }
}
