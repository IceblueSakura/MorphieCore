//! Startup-only resolution of immutable entry, contract and credential ownership.
use super::{
    credentials::Binding,
    entries::{Entry, TaskEntries},
    error::StartupError,
    limits::Limits,
};
use crate::{
    adapter::ClientAdapter,
    gateway::{BoundCandidate, BoundEntry, Credentials, Gateway, Runtime, ingress::auth},
    protocol::{
        ClientProtocol,
        openai::{GenerationApi, GenerationCodec, TargetContract},
    },
    provider::{ProviderDefinition, SecretMaterial},
    semantic::value::ReplayOrigin,
    topology::{CompiledTopology, Endpoint, PublicModel},
    transport::http::HttpTransport,
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

type GenerationEntries = BTreeMap<(ClientProtocol, String), Arc<BoundEntry>>;

impl Gateway {
    pub fn new(
        topology: CompiledTopology,
        entries: TaskEntries,
        credentials: impl Into<Credentials>,
        agents: BTreeMap<String, SecretMaterial>,
        limits: Limits,
        proxy: Option<&str>,
    ) -> Result<Self, StartupError> {
        Self::bind(topology, entries, credentials, agents, limits, proxy, false)
    }
    pub(crate) fn bind(
        topology: CompiledTopology,
        entries: TaskEntries,
        credentials: impl Into<Credentials>,
        agents: BTreeMap<String, SecretMaterial>,
        limits: Limits,
        proxy: Option<&str>,
        environment: bool,
    ) -> Result<Self, StartupError> {
        let TaskEntries {
            generation,
            images,
            speech,
            transcriptions,
            embeddings,
        } = entries;
        let credentials = credentials.into();
        let images = crate::gateway::tasks::images::bind(&topology, images, &credentials)?;
        let speech = crate::gateway::tasks::speech::bind(&topology, speech, &credentials)?;
        let transcriptions =
            crate::gateway::tasks::transcription::bind(&topology, transcriptions, &credentials)?;
        let embeddings =
            crate::gateway::tasks::embeddings::bind(&topology, embeddings, &credentials)?;
        limits.validate()?;
        let auth = auth::Auth::new(agents)?;
        let transport = HttpTransport::with_environment_proxy(proxy, environment)
            .map_err(|_| StartupError::Transport)?;
        let entries = bind_generation(&topology, generation, &credentials, &auth.digest)?;
        if entries.is_empty()
            && images.is_empty()
            && speech.is_empty()
            && transcriptions.is_empty()
            && embeddings.is_empty()
        {
            return Err(StartupError::Binding);
        }
        let models = crate::gateway::ingress::models::ModelView::bind(
            &topology,
            entries
                .keys()
                .map(|(_, label)| label.as_str())
                .chain(images.keys().map(String::as_str))
                .chain(speech.keys().map(String::as_str))
                .chain(transcriptions.keys().map(String::as_str))
                .chain(embeddings.keys().map(String::as_str)),
            limits.response_bytes,
        )?;
        let contracts =
            crate::gateway::ingress::discovery::compile(&entries, limits.request_bytes)?;
        let capabilities = crate::gateway::ingress::capabilities::CapabilityView::from_contracts(
            &entries,
            &embeddings,
            &contracts,
            limits.response_bytes,
        )?;
        let text_models = crate::gateway::ingress::discovery::TextModelView::from_contracts(
            &topology,
            &entries,
            &contracts,
            limits.response_bytes,
        )?;
        Ok(Self {
            state: Arc::new(Runtime {
                #[cfg(test)]
                oauth_scan_complete: tokio::sync::Notify::new(),
                oauth_accounts: credentials.oauth_accounts,
                diagnostics: None,
                request_ids: crate::gateway::ingress::correlation::RequestIds::new()
                    .map_err(|_| StartupError::Environment)?,
                capture: None,
                history: Arc::new(crate::gateway::generation::history::Store::new(&limits)),
                affinity: crate::gateway::routing::prefix_cache::Index::new(
                    limits.implicit_cache_affinity,
                ),
                executors: crate::gateway::routing::executors::Executors::new(
                    limits.affinity_ttl,
                    limits.affinity_capacity,
                    limits.provider_recovery.clone(),
                ),
                auth,
                entries,
                images,
                speech,
                transcriptions,
                embeddings,
                models,
                text_models,
                capabilities,
                permits: Arc::new(Semaphore::new(limits.concurrency)),
                limits,
                transport,
                shutdown: CancellationToken::new(),
            }),
        })
    }
}

/// Group Generation entries by client protocol and public model. Each group's
/// candidates follow the route order, one per account-bound endpoint.
fn bind_generation(
    topology: &CompiledTopology,
    entries: Vec<Entry>,
    credentials: &Credentials,
    namespace: &[u8; 32],
) -> Result<GenerationEntries, StartupError> {
    let mut groups: BTreeMap<(ClientProtocol, String), BoundEntry> = BTreeMap::new();
    for entry in entries {
        let public = topology.model(&entry.model).ok_or(StartupError::Binding)?;
        let route = topology.route(&public.route).ok_or(StartupError::Binding)?;
        if !route.endpoints.contains(&entry.endpoint) {
            return Err(StartupError::Binding);
        }
        let endpoint = topology
            .endpoint(&entry.endpoint)
            .ok_or(StartupError::Binding)?;
        let provider = topology
            .endpoint_provider(endpoint)
            .ok_or(StartupError::Binding)?;
        let binding = credentials
            .get(&endpoint.credential)
            .ok_or(StartupError::Credentials)?;
        let group = groups
            .entry((entry.protocol, entry.model.clone()))
            .or_insert_with(|| BoundEntry {
                public: public.clone(),
                client: client_adapter(entry.protocol),
                policy: route.policy.clone(),
                candidates: vec![],
            });
        if group
            .candidates
            .iter()
            .any(|c| c.endpoint.id == endpoint.id)
        {
            return Err(StartupError::Binding);
        }
        if group.candidates.len() >= crate::execution::plan::MAX_CANDIDATES {
            return Err(StartupError::Binding);
        }
        group.candidates.push(Arc::new(bind_candidate(
            public, endpoint, provider, binding, namespace,
        )?));
    }
    groups
        .into_iter()
        .map(|(key, mut group)| {
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
            Ok((key, Arc::new(group)))
        })
        .collect()
}

/// The client-side codec for one public protocol. Provider dialects never
/// authorize private public-API carriers.
fn client_adapter(protocol: ClientProtocol) -> ClientAdapter {
    let profile = match protocol {
        ClientProtocol::Messages => return ClientAdapter::Messages,
        ClientProtocol::Chat => GenerationApi::Chat,
        ClientProtocol::Responses => GenerationApi::Responses,
    };
    let client_scope = crate::protocol::replay_envelope::CLIENT_ORIGIN.clone();
    let client = GenerationCodec::new(
        profile,
        crate::protocol::openai::Dialect::Standard,
        Some(client_scope.clone()),
    );
    // Model input admission must not filter facts reported on the client wire
    // (e.g. empty logprobs or defaults a model cannot accept as controls).
    let downstream = TargetContract {
        replay_origin: Some(client_scope),
        ..TargetContract::full()
    };
    ClientAdapter::openai(client, downstream)
}

/// Bind one trusted account source to one effective target.
fn bind_candidate(
    public: &PublicModel,
    endpoint: &Endpoint,
    provider: &ProviderDefinition,
    binding: &Binding,
    namespace: &[u8; 32],
) -> Result<BoundCandidate, StartupError> {
    let secret = &binding.source;
    secret.check(endpoint.execution.credential_kind, provider.id.as_str())?;
    let provenance = secret.provenance();
    let mut endpoint = endpoint.clone();
    let health =
        crate::gateway::routing::executors::Scopes::bind(namespace, &endpoint, &provenance);
    let cache_scope = cache_scope(namespace, &provenance, public, provider, &endpoint)?;
    endpoint.bound_replay_scope = Some(cache_scope);
    let scope = endpoint.replay_origin();
    if let Some(caps) = endpoint.representation.openai_mut() {
        caps.adaptation.scope = Some(scope.clone());
        if caps.replay_origin.is_some() {
            caps.replay_origin = Some(scope);
        }
    }
    Ok(BoundCandidate {
        endpoint,
        provider: provider.clone(),
        secret: secret.clone(),
        account: binding.name.clone(),
        health,
    })
}

/// Private replay/cache scope of one candidate. Length prefixes avoid ambiguous
/// concatenation; provenance is neither a credential locator nor attestation
/// of token issuance.
fn cache_scope(
    namespace: &[u8; 32],
    provenance: &str,
    public: &PublicModel,
    provider: &ProviderDefinition,
    endpoint: &Endpoint,
) -> Result<ReplayOrigin, StartupError> {
    let mut hash = Sha256::new();
    hash.update(namespace);
    hash.update([endpoint.protocol.identity_tag()]);
    for value in [
        provenance,
        public.id.as_str(),
        provider.id.as_str(),
        endpoint.target.origin.as_str(),
        endpoint.target.path.as_str(),
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
    ReplayOrigin::new(&format!("gateway-{digest}")).map_err(|_| StartupError::Binding)
}
