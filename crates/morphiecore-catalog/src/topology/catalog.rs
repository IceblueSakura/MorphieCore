//! Compile explicit model declarations and deployment bindings. No alias inference.
mod data;
pub use data::{
    api_key_bindings, embedding_bindings, google_bindings, image_bindings, messages_bindings,
    speech_bindings, subscription_bindings, transcription_bindings,
};
mod bindings;
mod decisions;
pub use decisions::{DecisionBinding, decision_bindings};
mod messages;
pub use messages::MessagesBinding;
mod embeddings;
pub use embeddings::EmbeddingBinding;
mod google;
pub use google::GoogleBinding;
mod images;
mod models;
pub use images::ImageBinding;
mod speech;
pub use speech::SpeechBinding;
mod transcription;
pub use transcription::TranscriptionBinding;
mod subscriptions;
use crate::{
    protocol::openai::{GenerationCodec, TargetContract},
    provider::{CredentialBindingId, CredentialKind, catalog},
    topology::*,
};
pub use bindings::ApiKeyBinding;
// Agent sessions resend whole histories and may stream long turns; these only
// bound memory and time per exchange. Upstreams enforce their own maximums.
const GENERATION_REQUEST_BYTES: usize = 32 << 20;
const GENERATION_RESPONSE_BYTES: usize = 64 << 20;
const GENERATION_TIMEOUT_MS: u64 = 60 * 60 * 1000;
pub use subscriptions::SubscriptionBinding;

fn task_entries<T>(
    entries: impl Iterator<Item = T>,
    provider: impl Fn(&T) -> &crate::provider::ProviderId,
) -> Vec<T> {
    // Each family looks up exactly one declared operation per Provider.
    let mut seen = std::collections::BTreeSet::new();
    entries
        .filter(|entry| seen.insert(provider(entry).clone()))
        .collect()
}

/// The catalog with each public model's bindings in catalog order.
pub fn default_topology() -> Result<CompiledTopology, TopologyError> {
    static TOPOLOGY: std::sync::OnceLock<Result<CompiledTopology, TopologyError>> =
        std::sync::OnceLock::new();
    TOPOLOGY
        .get_or_init(|| {
            data::checked()?;
            compile_catalog()
        })
        .clone()
}

fn compile_catalog() -> Result<CompiledTopology, TopologyError> {
    let mut endpoints = vec![];
    let mut routes = vec![];
    let mut public_models: Vec<PublicModel> = vec![];
    for binding in api_key_bindings() {
        endpoints.extend(
            binding
                .protocols
                .iter()
                .map(|&p| binding.endpoint(p))
                .collect::<Result<Vec<_>, _>>()?,
        );
        if public_models.iter().any(|p| p.id.as_str() == binding.model) {
            continue;
        }
        let group: Vec<_> = api_key_bindings()
            .iter()
            .filter(|b| b.model == binding.model)
            .collect();
        let public = binding.public_model();
        // Alternatives share one public contract; candidates differ only in delivery.
        if group.iter().any(|b| {
            let other = b.public_model();
            other.contract != public.contract || other.canonical_model != public.canonical_model
        }) {
            return Err(TopologyError::SemanticUnsatisfiable);
        }
        routes.push(Route {
            id: public.route.clone(),
            policy: RoutePolicy::for_candidates(group.len()),
            endpoints: group
                .iter()
                .flat_map(|b| b.protocols.iter().map(|&p| b.endpoint_id(p)))
                .collect(),
        });
        public_models.push(public);
    }
    for binding in subscription_bindings() {
        let public = binding.public_model();
        routes.push(Route {
            id: public.route.clone(),
            policy: RoutePolicy::default(),
            endpoints: vec![binding.endpoint_id()],
        });
        endpoints.push(binding.endpoint());
        public_models.push(public);
    }
    let mut canonical_models = models::canonical_models();
    for binding in messages_bindings() {
        endpoints.push(binding.endpoint());
        routes.push(binding.route());
        public_models.push(binding.public_model());
        canonical_models.push(binding.canonical_model());
    }
    for binding in google_bindings() {
        endpoints.push(binding.endpoint());
        routes.push(binding.route());
        public_models.push(binding.public_model());
        canonical_models.push(binding.canonical_model());
    }
    // Authentication and wire families do not create separate public identities.
    // Keep each family's endpoint contract and credential binding unchanged.
    let mut merged: Vec<PublicModel> = Vec::new();
    for public in public_models {
        if let Some(existing) = merged.iter().find(|p| p.id == public.id) {
            if existing.canonical_model != public.canonical_model
                || existing.contract != public.contract
            {
                return Err(TopologyError::SemanticUnsatisfiable);
            }
            let index = routes
                .iter()
                .position(|r| r.id == public.route)
                .ok_or(TopologyError::ContractUnsatisfiable)?;
            let alternative = routes.remove(index);
            let route = routes
                .iter_mut()
                .find(|r| r.id == existing.route)
                .ok_or(TopologyError::ContractUnsatisfiable)?;
            route.endpoints.extend(alternative.endpoints);
            route.policy = RoutePolicy::for_candidates(route.endpoints.len());
        } else {
            merged.push(public);
        }
    }
    compile(catalog::all(), endpoints, routes, merged, canonical_models)?
        .with_images(
            task_entries(
                image_bindings()
                    .iter()
                    .flat_map(|b| b.providers.iter().copied().map(ImageBinding::operation)),
                |e| &e.provider,
            ),
            image_bindings()
                .iter()
                .flat_map(|b| b.providers.iter().map(|provider| b.route(provider)))
                .collect(),
        )?
        .with_speech(
            task_entries(speech_bindings().iter().map(|b| b.operation()), |e| {
                &e.provider
            }),
            speech_bindings().iter().map(|b| b.route()).collect(),
        )?
        .with_transcriptions(
            task_entries(
                transcription_bindings().iter().map(|b| b.operation()),
                |e| &e.provider,
            ),
            transcription_bindings().iter().map(|b| b.route()).collect(),
        )?
        .with_embeddings(
            task_entries(embedding_bindings().iter().map(|b| b.operation()), |e| {
                &e.provider
            }),
            embedding_bindings().iter().map(|b| b.route()).collect(),
        )?
        .with_model_metadata(
            data::models()
                .iter()
                .map(|m| {
                    (
                        ModelId::new(m.id).expect("static id"),
                        ModelMetadata::new(m.released_at, m.developer).expect("static publication"),
                    )
                })
                .chain(messages_bindings().iter().map(|b| {
                    (
                        ModelId::new(b.model).expect("static messages id"),
                        b.metadata(),
                    )
                }))
                .chain(google_bindings().iter().map(|b| {
                    (
                        ModelId::new(b.model).expect("static google id"),
                        b.metadata(),
                    )
                }))
                .chain(image_bindings().iter().map(|b| {
                    (
                        ModelId::new(b.model).expect("static image id"),
                        b.metadata(),
                    )
                }))
                .chain(speech_bindings().iter().map(|b| {
                    (
                        ModelId::new(b.model).expect("static speech id"),
                        b.metadata(),
                    )
                }))
                .chain(embedding_bindings().iter().map(|b| {
                    (
                        ModelId::new(b.model).expect("static embedding id"),
                        b.metadata(),
                    )
                }))
                .chain(data::transcription_metadata()),
        )
}
