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
    lowering::generation::ReportedFactPolicy,
    protocol::openai::{Adapter, TargetCaps},
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
impl ApiKeyBinding {
    pub fn public_model(&self) -> PublicModel {
        PublicModel {
            id: ModelId::new(self.model).expect("static id"),
            canonical_model: ModelId::new(self.canonical_model).expect("static id"),
            task: TaskKind::Generation,
            route: RouteId::new(&format!("{}-generation", self.endpoint_prefix))
                .expect("static id"),
            contract: self
                .public_slice
                .apply(models::contract(self.canonical_model)),
            standard_context: false,
            reported_facts: ReportedFactPolicy::Faithful,
        }
    }
    pub fn endpoint(&self, protocol: ProtocolProfile) -> Result<Endpoint, TopologyError> {
        let provider = (self.provider)();
        let (family, path, replay) = match protocol {
            ProtocolProfile::OpenAiChat => (
                crate::protocol::openai::Profile::Chat,
                provider.chat_completions.clone().expect("declared entry"),
                self.replay_chat,
            ),
            ProtocolProfile::OpenAiResponses => (
                crate::protocol::openai::Profile::Responses,
                provider.responses.clone().expect("declared entry"),
                self.replay_responses,
            ),
            ProtocolProfile::AnthropicMessages
            | ProtocolProfile::GoogleGenerateContent
            | ProtocolProfile::GoogleInteractions => {
                return Err(TopologyError::ContractUnsatisfiable);
            }
        };
        let scope = crate::protocol::replay_envelope::issuer(provider.id.as_str(), self.upstream);
        let contract = TargetCaps {
            semantics: self
                .endpoint_slice
                .apply(models::contract(self.canonical_model)),
            replay_origin: replay.then_some(scope.clone()),
            cache: Adapter::new(family, self.dialect, None).adaptation.cache,
            identity_hints: false,
            standard_context: false,
            ..TargetCaps::full()
        };
        Ok(Endpoint {
            id: self.endpoint_id(protocol),
            canonical_model: ModelId::new(self.canonical_model).expect("static id"),
            provider: provider.id.clone(),
            target: EndpointTarget {
                origin: provider.origin,
                path,
            },
            task: TaskKind::Generation,
            protocol,
            upstream_model: self.upstream.into(),
            representation: Adapter::new(family, self.dialect, Some(scope))
                .contract(&contract)
                .into(),
            execution: ExecutionContract {
                streaming: true,
                retry_before_commit: false,
                request_body_limit: GENERATION_REQUEST_BYTES,
                response_body_limit: GENERATION_RESPONSE_BYTES,
                credential_kind: CredentialKind::ApiKey,
                timeout_ms: GENERATION_TIMEOUT_MS,
            },
            credential: CredentialBindingId::new(self.credential).expect("static binding"),
        })
    }
    pub fn endpoint_id(&self, protocol: ProtocolProfile) -> EndpointId {
        let suffix = match protocol {
            ProtocolProfile::OpenAiChat => "chat",
            ProtocolProfile::OpenAiResponses => "responses",
            ProtocolProfile::AnthropicMessages => "messages",
            ProtocolProfile::GoogleGenerateContent => "generate-content",
            ProtocolProfile::GoogleInteractions => "interactions",
        };
        EndpointId::new(&format!("{}-{suffix}", self.endpoint_prefix)).expect("static id")
    }
}
pub fn default_topology() -> Result<CompiledTopology, TopologyError> {
    topology(&Default::default())
}
/// The catalog with each public model's bindings in failover order: the trusted
/// configured Provider order (`model -> [provider]`) first, then catalog order.
pub fn topology(
    failover: &std::collections::BTreeMap<String, Vec<String>>,
) -> Result<CompiledTopology, TopologyError> {
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
        let mut group: Vec<_> = API_KEY_BINDINGS
            .iter()
            .filter(|b| b.model == binding.model)
            .collect();
        let order = failover.get(binding.model);
        group.sort_by_key(|b| {
            let provider = (b.provider)().id;
            order
                .and_then(|o| o.iter().position(|p| p == provider.as_str()))
                .unwrap_or(usize::MAX)
        });
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
            task: public.task,
            // Failover only: skip candidates that cannot serve the request, advance
            // before commit, never spread load.
            policy: if group.len() > 1 {
                RoutePolicy {
                    candidates: CandidatePolicy::SkipUnrepresentable,
                    fallback: FallbackPolicy::BeforeCommit,
                    ..RoutePolicy::default()
                }
            } else {
                RoutePolicy::default()
            },
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
            task: public.task,
            policy: RoutePolicy {
                max_attempts: if binding.profile == "openai" {
                    1
                } else {
                    RoutePolicy::default().max_attempts
                },
                ..Default::default()
            },
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
                || existing.standard_context != public.standard_context
                || existing.reported_facts != public.reported_facts
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
            route.policy = RoutePolicy {
                candidates: CandidatePolicy::SkipUnrepresentable,
                fallback: FallbackPolicy::BeforeCommit,
                ..RoutePolicy::default()
            };
        } else {
            merged.push(public);
        }
    }
    for public in &merged {
        if let Some(order) = failover.get(public.id.as_str()) {
            let route = routes
                .iter_mut()
                .find(|r| r.id == public.route)
                .ok_or(TopologyError::ContractUnsatisfiable)?;
            route.endpoints.sort_by_key(|id| {
                let endpoint = endpoints
                    .iter()
                    .find(|e| &e.id == id)
                    .expect("declared endpoint");
                order
                    .iter()
                    .position(|p| p == endpoint.provider.as_str())
                    .unwrap_or(usize::MAX)
            });
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
                // Public credential-domain aliases share the original canonical publication.
                .chain(
                    TRANSCRIPTION_BINDINGS
                        .iter()
                        .filter(|b| b.model == b.canonical_model)
                        .map(|b| {
                            (
                                ModelId::new(b.model).expect("static transcription id"),
                                b.metadata(),
                            )
                        }),
                ),
        )
}
#[cfg(test)]
#[test]
fn declared_topology_compiles() {
    let topology = default_topology().unwrap();
    for (model, providers) in [
        ("claude-haiku-5-5", &["openrouter", "opencode-go"][..]),
        ("mimo-v2.6-pro", &["xiaomi", "opencode-go"][..]),
        (
            "glm-5.3",
            &["zhipu", "aliyun-dashscope-cn", "aliyun-tokenplan-cn"][..],
        ),
    ] {
        let public = topology.model(model).unwrap();
        let route = topology.route(&public.route).unwrap();
        let actual: std::collections::BTreeSet<_> = route
            .endpoints
            .iter()
            .map(|id| topology.endpoint(id).unwrap().provider.as_str())
            .collect();
        assert_eq!(actual, providers.iter().copied().collect());
    }
    for model in [
        "gpt-6-astra",
        "mimo-v2.6-pro-ultraspeed",
        "muse-spark-1.3-contributor",
        "kimi-k3",
    ] {
        assert!(topology.model(model).is_some(), "{model}");
    }
    let muse = topology.model("muse-spark-1.3-contributor").unwrap();
    let muse_endpoints = topology.route_endpoints(&muse.route);
    assert_eq!(muse_endpoints.len(), 1);
    assert_eq!(muse_endpoints[0].protocol, ProtocolProfile::OpenAiResponses);
    assert_eq!(
        muse_endpoints[0].target.path.as_str(),
        "/zen/go/v1/responses"
    );
    assert!(topology.speech_route("mimo-v2.5-tts").is_some());
    assert!(topology.transcription_route("mimo-v2.5-asr").is_some());
    for excluded in ["mimo-v2.5-tts-voiceclone", "mimo-v2.5-tts-voicedesign"] {
        assert!(topology.speech_route(excluded).is_none());
        assert!(topology.model(excluded).is_none());
    }
    for public in topology.models() {
        assert!(topology.model_metadata(&public.canonical_model).is_some());
    }
    for binding in IMAGE_BINDINGS {
        let route = topology.image_route(binding.model).unwrap();
        assert!(topology.model_metadata(&route.canonical_model).is_some());
    }
    for binding in SPEECH_BINDINGS {
        let route = topology.speech_route(binding.model).unwrap();
        assert!(topology.model_metadata(&route.canonical_model).is_some());
    }
    for binding in TRANSCRIPTION_BINDINGS {
        let route = topology.transcription_route(binding.model).unwrap();
        assert!(topology.model_metadata(&route.canonical_model).is_some());
    }
}
#[cfg(test)]
#[test]
fn modelbest_vision_versions_use_upstream_ids_and_multimodal_chat_contracts() {
    let topology = default_topology().unwrap();
    for id in ["MiniCPM-V-4.7-1B", "MiniCPM-V-4.7-35B-A3B"] {
        let binding = API_KEY_BINDINGS.iter().find(|b| b.model == id).unwrap();
        assert_eq!(binding.upstream, id);
        assert_eq!(binding.canonical_model, id);
        assert_eq!(binding.protocols, &[ProtocolProfile::OpenAiChat]);
        let public = topology.model(id).unwrap();
        assert!(public.contract.image_input && public.contract.video_input);
        let endpoint = binding.endpoint(ProtocolProfile::OpenAiChat).unwrap();
        assert_eq!(endpoint.provider.as_str(), "modelbest");
        assert_eq!(endpoint.upstream_model, id);
        assert_eq!(endpoint.credential.as_str(), "modelbest-api-key");
    }
    assert!(topology.model("minicpm-v-4.6").is_none());
    assert!(
        API_KEY_BINDINGS
            .iter()
            .all(|b| b.upstream != "MiniCPM-V-4.6")
    );
    for id in ["minicpm5-1b", "minicpm5-2b"] {
        assert!(topology.model(id).is_some());
    }
}
#[cfg(test)]
#[test]
fn removed_providers_have_no_routes_and_unrelated_bindings_survive() {
    let removed = default_topology().unwrap();
    for provider in ["longcat", "kimi"] {
        assert!(removed.provider(provider).is_none());
    }
    assert!(removed.model("longcat-2.5-preview").is_none());
    assert!(removed.provider("opencode-go").is_some());
}
