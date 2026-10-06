//! Compile explicit model declarations and deployment bindings. No alias inference.
mod bindings;
mod images;
mod models;
pub use images::{IMAGE_BINDINGS, ImageBinding};
mod speech;
pub use speech::{SPEECH_BINDINGS, SpeechBinding};
mod transcription;
pub use transcription::{TRANSCRIPTION_BINDINGS, TranscriptionBinding};
mod subscriptions;
use crate::{
    adapter::Adapter,
    lowering::generation::{GenerationRepresentationContract, ReportedFactPolicy},
    provider::{CredentialBindingId, CredentialKind, catalog},
    semantic::value::ReplayOrigin,
    topology::*,
};
pub use bindings::{API_KEY_BINDINGS, ApiKeyBinding};
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
    pub fn endpoint(&self, protocol: ProtocolProfile) -> Endpoint {
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
        };
        let scope = ReplayOrigin::new(provider.id.as_str()).expect("static scope");
        let contract = GenerationRepresentationContract {
            semantics: self
                .endpoint_slice
                .apply(models::contract(self.canonical_model)),
            replay_origin: replay.then_some(scope.clone()),
            cache: Adapter::new(family, self.dialect, None).adaptation.cache,
            identity_hints: false,
            standard_context: false,
            ..GenerationRepresentationContract::full()
        };
        Endpoint {
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
            representation: Adapter::new(family, self.dialect, Some(scope)).contract(&contract),
            execution: ExecutionContract {
                streaming: true,
                retry_before_commit: false,
                request_body_limit: 256 * 1024,
                response_body_limit: 8 * 1024 * 1024,
                credential_kind: CredentialKind::ApiKey,
                timeout_ms: 120_000,
            },
            credential: CredentialBindingId::new(self.credential).expect("static binding"),
        }
    }
    pub fn endpoint_id(&self, protocol: ProtocolProfile) -> EndpointId {
        let suffix = match protocol {
            ProtocolProfile::OpenAiChat => "chat",
            ProtocolProfile::OpenAiResponses => "responses",
        };
        EndpointId::new(&format!("{}-{suffix}", self.endpoint_prefix)).expect("static id")
    }
}
pub fn default_topology() -> Result<CompiledTopology, TopologyError> {
    let mut endpoints = vec![];
    let mut routes = vec![];
    let mut public_models = vec![];
    for binding in API_KEY_BINDINGS {
        let public = binding.public_model();
        routes.push(Route {
            id: public.route.clone(),
            task: public.task,
            policy: RoutePolicy::default(),
            endpoints: binding
                .protocols
                .iter()
                .map(|&p| binding.endpoint_id(p))
                .collect(),
        });
        endpoints.extend(binding.protocols.iter().map(|&p| binding.endpoint(p)));
        public_models.push(public);
    }
    for binding in SUBSCRIPTION_BINDINGS {
        let public = binding.public_model();
        routes.push(Route {
            id: public.route.clone(),
            task: public.task,
            policy: RoutePolicy {
                max_attempts: if binding.profile == "siwc" {
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
    compile(
        catalog::all(),
        endpoints,
        routes,
        public_models,
        models::canonical_models(),
    )?
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
    .with_model_metadata(
        models::MODELS
            .iter()
            .map(|m| {
                (
                    ModelId::new(m.id).expect("static id"),
                    ModelMetadata::new(m.released_at, m.developer).expect("static publication"),
                )
            })
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
            .chain(TRANSCRIPTION_BINDINGS.iter().map(|b| {
                (
                    ModelId::new(b.model).expect("static transcription id"),
                    b.metadata(),
                )
            })),
    )
}
#[cfg(test)]
#[test]
fn declared_topology_compiles() {
    let topology = default_topology().unwrap();
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
fn siwc_registration_is_public_responses_only_and_does_not_reuse_product_bindings() {
    let topology = default_topology().unwrap();
    assert!(topology.provider("codex").is_none());
    let provider = topology.provider("openai-siwc").unwrap();
    assert_eq!(provider.origin.as_str(), "https://api.openai.com");
    assert_eq!(
        provider.responses.as_ref().unwrap().as_str(),
        "/v1/responses"
    );
    assert!(provider.chat_completions.is_none());
    assert_eq!(
        provider.auth,
        crate::provider::AuthScheme::OAuthBearer("siwc")
    );
    let endpoint = topology
        .endpoint(&EndpointId::new("openai-siwc-responses").unwrap())
        .unwrap();
    assert_eq!(endpoint.credential.as_str(), "openai-siwc-oauth");
    assert_eq!(
        endpoint.representation.adaptation.profile_id,
        "openai-siwc-responses"
    );
    assert!(!endpoint.representation.semantics.max_output_tokens);
    assert!(
        endpoint
            .representation
            .adaptation
            .rules
            .responses_forced_stream
    );
    assert!(
        !endpoint
            .representation
            .adaptation
            .rules
            .responses_product_accounting
    );
    let route = topology
        .route(&RouteId::new("openai-siwc-generation").unwrap())
        .unwrap();
    assert_eq!(route.policy.max_attempts, 1);
    assert_eq!(route.policy.fallback, FallbackPolicy::Disabled);
    assert_eq!(route.endpoints.len(), 1);
}
