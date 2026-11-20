//! Compile explicit model declarations and deployment bindings. No alias inference.
mod bindings;
mod messages;
pub use messages::{MESSAGES_BINDINGS, MessagesBinding};
mod embeddings;
pub use embeddings::{EMBEDDING_BINDINGS, EmbeddingBinding};
mod google;
pub use google::{GOOGLE_BINDINGS, GoogleBinding};
mod images;
mod models;
pub use images::{IMAGE_BINDINGS, ImageBinding};
mod speech;
pub use speech::{SPEECH_BINDINGS, SpeechBinding};
mod transcription;
pub use transcription::{TRANSCRIPTION_BINDINGS, TranscriptionBinding};
mod subscriptions;
use crate::{
    protocol::openai::{GenerationCodec, TargetContract},
    provider::{CredentialBindingId, CredentialKind, catalog},
    topology::*,
};
pub use bindings::{API_KEY_BINDINGS, ApiKeyBinding};
// Agent sessions resend whole histories and may stream long turns; these only
// bound memory and time per exchange. Upstreams enforce their own maximums.
const GENERATION_REQUEST_BYTES: usize = 32 << 20;
const GENERATION_RESPONSE_BYTES: usize = 64 << 20;
const GENERATION_TIMEOUT_MS: u64 = 60 * 60 * 1000;
pub use subscriptions::{SUBSCRIPTION_BINDINGS, SubscriptionBinding};
/// The catalog with each public model's bindings in catalog order.
pub fn default_topology() -> Result<CompiledTopology, TopologyError> {
    let mut endpoints = vec![];
    let mut routes = vec![];
    let mut public_models: Vec<PublicModel> = vec![];
    for binding in API_KEY_BINDINGS {
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
        let group: Vec<_> = API_KEY_BINDINGS
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
    for binding in SUBSCRIPTION_BINDINGS {
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
    for binding in MESSAGES_BINDINGS {
        endpoints.push(binding.endpoint());
        routes.push(binding.route());
        public_models.push(binding.public_model());
        canonical_models.push(binding.canonical_model());
    }
    for binding in GOOGLE_BINDINGS {
        endpoints.push(binding.endpoint());
        endpoints.push(binding.interactions_endpoint());
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
            IMAGE_BINDINGS.iter().map(|b| b.operation()).collect(),
            IMAGE_BINDINGS.iter().map(|b| b.route()).collect(),
        )?
        .with_speech(
            SPEECH_BINDINGS.iter().map(|b| b.operation()).collect(),
            SPEECH_BINDINGS.iter().map(|b| b.route()).collect(),
        )?
        .with_transcriptions(
            TRANSCRIPTION_BINDINGS
                .iter()
                .map(|b| b.operation())
                .collect(),
            TRANSCRIPTION_BINDINGS.iter().map(|b| b.route()).collect(),
        )?
        .with_embeddings(
            EMBEDDING_BINDINGS.iter().map(|b| b.operation()).collect(),
            EMBEDDING_BINDINGS.iter().map(|b| b.route()).collect(),
        )?
        .with_model_metadata(
            models::MODELS
                .iter()
                .map(|m| {
                    (
                        ModelId::new(m.id).expect("static id"),
                        ModelMetadata::new(m.released_at, m.developer).expect("static publication"),
                    )
                })
                .chain(MESSAGES_BINDINGS.iter().map(|b| {
                    (
                        ModelId::new(b.model).expect("static messages id"),
                        b.metadata(),
                    )
                }))
                .chain(GOOGLE_BINDINGS.iter().map(|b| {
                    (
                        ModelId::new(b.model).expect("static google id"),
                        b.metadata(),
                    )
                }))
                .chain(IMAGE_BINDINGS.iter().map(|b| {
                    (
                        ModelId::new(b.model).expect("static image id"),
                        b.metadata(),
                    )
                }))
                .chain(SPEECH_BINDINGS.iter().map(|b| {
                    (
                        ModelId::new(b.model).expect("static speech id"),
                        b.metadata(),
                    )
                }))
                .chain(EMBEDDING_BINDINGS.iter().map(|b| {
                    (
                        ModelId::new(b.model).expect("static embedding id"),
                        b.metadata(),
                    )
                }))
                // Provider bindings share one canonical publication.
                .chain(
                    TRANSCRIPTION_BINDINGS
                        .iter()
                        .map(|b| {
                            (
                                ModelId::new(b.canonical_model).expect("static transcription id"),
                                b.metadata(),
                            )
                        })
                        .collect::<std::collections::BTreeMap<_, _>>(),
                ),
        )
}
