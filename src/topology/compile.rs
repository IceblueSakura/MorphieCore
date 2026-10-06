//! Compile trusted topology input into one immutable relation set.

use crate::{
    lowering::generation::GenerationRepresentationContract,
    provider::ProviderDefinition,
    topology::{
        CanonicalModel, Endpoint, EndpointId, GenerationSemanticContract, ProtocolProfile,
        PublicModel, Route, RouteId,
    },
};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum TopologyError {
    #[error("duplicate provider")]
    DuplicateProvider,
    #[error("duplicate endpoint")]
    DuplicateEndpoint,
    #[error("duplicate route")]
    DuplicateRoute,
    #[error("duplicate public model")]
    DuplicateModel,
    #[error("duplicate canonical model")]
    DuplicateCanonicalModel,
    #[error("unknown canonical model")]
    UnknownCanonicalModel,
    #[error("route candidate refers to another canonical model")]
    CanonicalModelMismatch,
    #[error("public semantics exceed canonical model support")]
    SemanticUnsatisfiable,
    #[error("endpoint references an unknown provider")]
    UnknownProvider,
    #[error("route references an unknown endpoint")]
    UnknownEndpoint,
    #[error("model references an unknown route")]
    UnknownRoute,
    #[error("endpoint target is not bound to its provider origin/entry")]
    TargetMismatch,
    #[error("task kind mismatch across topology relations")]
    TaskMismatch,
    #[error("route has no candidates")]
    EmptyRoute,
    #[error("public contract is not satisfied by every route candidate")]
    ContractUnsatisfiable,
    #[error("execution limits must be non-zero")]
    InvalidExecutionLimits,
    #[error("invalid upstream model binding")]
    InvalidModelBinding,
    #[error("invalid bounded route policy")]
    InvalidRoutePolicy,
    #[error("duplicate route candidate")]
    DuplicateCandidate,
    #[error("invalid model publication metadata")]
    InvalidModelMetadata,
    #[error("duplicate model publication metadata")]
    DuplicateModelMetadata,
}

/// `promise` must be pointwise implied by `endpoint`. Whether a narrower promise
/// is published is a route policy decision made in trusted configuration.
fn promised(
    semantic: &GenerationSemanticContract,
    endpoint: &GenerationRepresentationContract,
) -> bool {
    endpoint.semantics.supports(semantic)
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompiledTopology {
    providers: BTreeMap<String, ProviderDefinition>,
    endpoints: BTreeMap<String, Endpoint>,
    routes: BTreeMap<String, Route>,
    models: BTreeMap<String, PublicModel>,
    canonical_models: BTreeMap<String, CanonicalModel>,
    pub(super) image_routes: BTreeMap<String, super::images::ImageRoute>,
    pub(super) speech_routes: BTreeMap<String, super::speech::SpeechRoute>,
    pub(super) transcription_routes: BTreeMap<String, super::transcription::TranscriptionRoute>,
    pub(super) model_metadata: BTreeMap<String, super::ModelMetadata>,
}

impl CompiledTopology {
    pub fn provider(&self, id: &str) -> Option<&ProviderDefinition> {
        self.providers.get(id)
    }

    pub fn endpoint(&self, id: &EndpointId) -> Option<&Endpoint> {
        self.endpoints.get(id.as_str())
    }

    pub fn route(&self, id: &RouteId) -> Option<&Route> {
        self.routes.get(id.as_str())
    }

    pub fn canonical_model(&self, id: &super::ModelId) -> Option<&CanonicalModel> {
        self.canonical_models.get(id.as_str())
    }

    pub fn model(&self, label: &str) -> Option<&PublicModel> {
        self.models.get(label)
    }

    /// Route candidates in fixed order; none for unknown routes.
    pub fn route_endpoints(&self, id: &RouteId) -> Vec<&Endpoint> {
        self.routes
            .get(id.as_str())
            .map(|route| {
                route
                    .endpoints
                    .iter()
                    .filter_map(|e| self.endpoints.get(e.as_str()))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn models(&self) -> impl Iterator<Item = &PublicModel> {
        self.models.values()
    }

    pub fn endpoints(&self) -> impl Iterator<Item = &Endpoint> {
        self.endpoints.values()
    }
}

/// Compile immutable topology. Validation pins provider/target binding, fixed
/// route order and public-contract satisfiability before any request exists.
pub fn compile(
    providers: Vec<ProviderDefinition>,
    endpoints: Vec<Endpoint>,
    routes: Vec<Route>,
    models: Vec<PublicModel>,
    canonical_models: Vec<CanonicalModel>,
) -> Result<CompiledTopology, TopologyError> {
    let mut canonical_map = BTreeMap::new();
    for model in canonical_models {
        if canonical_map
            .insert(model.id.as_str().to_owned(), model)
            .is_some()
        {
            return Err(TopologyError::DuplicateCanonicalModel);
        }
    }
    let mut provider_map = BTreeMap::new();
    for provider in providers {
        if provider_map
            .insert(provider.id.as_str().to_string(), provider)
            .is_some()
        {
            return Err(TopologyError::DuplicateProvider);
        }
    }
    let mut endpoint_map = BTreeMap::new();
    for endpoint in endpoints {
        endpoint.validate()?;
        let canonical = canonical_map
            .get(endpoint.canonical_model.as_str())
            .ok_or(TopologyError::UnknownCanonicalModel)?;
        if canonical.task != endpoint.task {
            return Err(TopologyError::TaskMismatch);
        }
        let definition = provider_map
            .get(endpoint.provider.as_str())
            .ok_or(TopologyError::UnknownProvider)?;
        if definition.origin != endpoint.target.origin {
            return Err(TopologyError::TargetMismatch);
        }
        let bound_path = match endpoint.protocol {
            ProtocolProfile::OpenAiChat => definition
                .chat_completions
                .as_ref()
                .ok_or(TopologyError::TargetMismatch)?,
            ProtocolProfile::OpenAiResponses => definition
                .responses
                .as_ref()
                .ok_or(TopologyError::TargetMismatch)?,
        };
        if bound_path != &endpoint.target.path {
            return Err(TopologyError::TargetMismatch);
        }
        if definition.auth.kind() != endpoint.execution.credential_kind {
            return Err(TopologyError::TargetMismatch);
        }
        if endpoint_map
            .insert(endpoint.id.as_str().to_string(), endpoint)
            .is_some()
        {
            return Err(TopologyError::DuplicateEndpoint);
        }
    }
    let mut route_map = BTreeMap::new();
    for route in routes {
        if route.endpoints.is_empty() {
            return Err(TopologyError::EmptyRoute);
        }
        if route.endpoints.len() > super::route::MAX_ROUTE_CANDIDATES
            || !(1..=super::route::MAX_ROUTE_CANDIDATES).contains(&route.policy.max_attempts)
        {
            return Err(TopologyError::InvalidRoutePolicy);
        }
        let mut seen = std::collections::BTreeSet::new();
        for candidate in &route.endpoints {
            if !seen.insert(candidate) {
                return Err(TopologyError::DuplicateCandidate);
            }
            let endpoint = endpoint_map
                .get(candidate.as_str())
                .ok_or(TopologyError::UnknownEndpoint)?;
            if endpoint.execution.credential_kind == crate::provider::CredentialKind::OAuth("siwc")
                && (route.endpoints.len() != 1
                    || route.policy.max_attempts != 1
                    || route.policy.fallback != super::FallbackPolicy::Disabled)
            {
                return Err(TopologyError::InvalidRoutePolicy);
            }
            if endpoint.task != route.task {
                return Err(TopologyError::TaskMismatch);
            }
        }
        if route_map
            .insert(route.id.as_str().to_string(), route)
            .is_some()
        {
            return Err(TopologyError::DuplicateRoute);
        }
    }
    let mut model_map = BTreeMap::new();
    for model in models {
        let canonical = canonical_map
            .get(model.canonical_model.as_str())
            .ok_or(TopologyError::UnknownCanonicalModel)?;
        if canonical.task != model.task {
            return Err(TopologyError::TaskMismatch);
        }
        if !canonical.contract.supports(&model.contract) {
            return Err(TopologyError::SemanticUnsatisfiable);
        }
        let route = route_map
            .get(model.route.as_str())
            .ok_or(TopologyError::UnknownRoute)?;
        if route.task != model.task {
            return Err(TopologyError::TaskMismatch);
        }
        for candidate in &route.endpoints {
            let endpoint = &endpoint_map[candidate.as_str()];
            if endpoint.canonical_model != model.canonical_model {
                return Err(TopologyError::CanonicalModelMismatch);
            }
            if route.policy.candidates == super::CandidatePolicy::RequireAll
                && (!promised(&model.contract, &endpoint.representation)
                    || model.standard_context && !endpoint.representation.standard_context)
            {
                return Err(TopologyError::ContractUnsatisfiable);
            }
        }
        if model_map
            .insert(model.id.as_str().to_string(), model)
            .is_some()
        {
            return Err(TopologyError::DuplicateModel);
        }
    }
    Ok(CompiledTopology {
        providers: provider_map,
        endpoints: endpoint_map,
        routes: route_map,
        models: model_map,
        canonical_models: canonical_map,
        image_routes: BTreeMap::new(),
        speech_routes: BTreeMap::new(),
        transcription_routes: BTreeMap::new(),
        model_metadata: BTreeMap::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        lowering::generation::GenerationRepresentationContract,
        provider::{
            CredentialBindingId, CredentialKind, EndpointPath, ProviderDefinition, ProviderId,
            TrustedOrigin, auth::AuthScheme,
        },
        topology::{Endpoint, EndpointTarget, ExecutionContract, ModelId, TaskKind},
    };

    fn provider() -> ProviderDefinition {
        ProviderDefinition {
            id: ProviderId::new("fixture").unwrap(),
            origin: TrustedOrigin::parse("http://127.0.0.1:39217").unwrap(),
            chat_completions: Some(EndpointPath::new("/chat/completions").unwrap()),
            responses: Some(EndpointPath::new("/responses").unwrap()),
            auth: AuthScheme::Bearer,
        }
    }

    fn endpoint(protocol: ProtocolProfile) -> Endpoint {
        let definition = provider();
        let path = match protocol {
            ProtocolProfile::OpenAiChat => definition.chat_completions.expect("test Chat entry"),
            ProtocolProfile::OpenAiResponses => definition.responses.expect("test Responses entry"),
        };
        Endpoint {
            id: EndpointId::new("fixture-endpoint").unwrap(),
            provider: ProviderId::new("fixture").unwrap(),
            target: EndpointTarget {
                origin: definition.origin,
                path,
            },
            task: TaskKind::Generation,
            protocol,
            upstream_model: "fixture-model".into(),
            canonical_model: ModelId::new("fixture-model").unwrap(),
            representation: GenerationRepresentationContract::full(),
            execution: ExecutionContract {
                streaming: true,
                retry_before_commit: false,
                request_body_limit: 4096,
                response_body_limit: 4096,
                credential_kind: CredentialKind::ApiKey,
                timeout_ms: 1000,
            },
            credential: CredentialBindingId::new("fixture-key").unwrap(),
        }
    }

    fn route() -> Route {
        Route {
            policy: super::super::RoutePolicy::default(),
            id: RouteId::new("fixture-route").unwrap(),
            task: TaskKind::Generation,
            endpoints: vec![EndpointId::new("fixture-endpoint").unwrap()],
        }
    }

    fn canonical() -> CanonicalModel {
        CanonicalModel {
            id: ModelId::new("fixture-model").unwrap(),
            task: TaskKind::Generation,
            contract: GenerationSemanticContract::full(),
        }
    }
    fn model(contract: GenerationRepresentationContract) -> PublicModel {
        PublicModel::new(
            ModelId::new("fixture-model").unwrap(),
            ModelId::new("fixture-model").unwrap(),
            TaskKind::Generation,
            RouteId::new("fixture-route").unwrap(),
            contract.semantics,
        )
    }

    fn compile_fixture(
        endpoints: Vec<Endpoint>,
        routes: Vec<Route>,
        models: Vec<PublicModel>,
    ) -> Result<CompiledTopology, TopologyError> {
        compile(
            vec![provider()],
            endpoints,
            routes,
            models,
            vec![canonical()],
        )
    }

    #[test]
    fn responses_only_provider_cannot_compile_a_chat_endpoint() {
        let mut responses_only = provider();
        responses_only.chat_completions = None;
        let build = |protocol| {
            compile(
                vec![responses_only.clone()],
                vec![endpoint(protocol)],
                vec![route()],
                vec![model(GenerationRepresentationContract::full())],
                vec![canonical()],
            )
        };
        assert!(build(ProtocolProfile::OpenAiResponses).is_ok());
        assert_eq!(
            build(ProtocolProfile::OpenAiChat),
            Err(TopologyError::TargetMismatch)
        );
    }

    #[test]
    fn siwc_cannot_enter_a_fallback_route_even_through_embedded_configuration() {
        for (attempts, fallback, count, accepted) in [
            (1, false, 1, true),
            (2, false, 1, false),
            (1, true, 1, false),
            (1, false, 2, false),
        ] {
            let mut provider = provider();
            provider.auth = AuthScheme::OAuthBearer("siwc");
            let mut endpoint = endpoint(ProtocolProfile::OpenAiResponses);
            endpoint.execution.credential_kind = CredentialKind::OAuth("siwc");
            let mut endpoints = vec![endpoint.clone()];
            let mut route = route();
            route.policy.max_attempts = attempts;
            if fallback {
                route.policy.fallback = crate::topology::FallbackPolicy::BeforeCommit;
            }
            if count == 2 {
                endpoint.id = EndpointId::new("second").unwrap();
                route.endpoints.push(endpoint.id.clone());
                endpoints.push(endpoint);
            }
            assert_eq!(
                compile(
                    vec![provider],
                    endpoints,
                    vec![route],
                    vec![model(GenerationRepresentationContract::full())],
                    vec![canonical()]
                )
                .is_ok(),
                accepted,
                "attempts={attempts} fallback={fallback} count={count}"
            );
        }
    }

    #[test]
    fn fixed_route_order_and_provider_specific_aliases_survive_compilation() {
        let mut responses = endpoint(ProtocolProfile::OpenAiResponses);
        responses.id = EndpointId::new("e-responses").unwrap();
        responses.upstream_model = "native-model-name".into();
        let mut other_provider = provider();
        other_provider.id = ProviderId::new("other").unwrap();
        let mut chat = endpoint(ProtocolProfile::OpenAiChat);
        chat.id = EndpointId::new("e-chat").unwrap();
        chat.provider = other_provider.id.clone();
        chat.upstream_model = "hosted-model-alias".into();
        let topology = compile(
            vec![provider(), other_provider],
            vec![chat, responses],
            vec![Route {
                policy: super::super::RoutePolicy::default(),
                id: RouteId::new("fixture-route").unwrap(),
                task: TaskKind::Generation,
                endpoints: vec![
                    EndpointId::new("e-responses").unwrap(),
                    EndpointId::new("e-chat").unwrap(),
                ],
            }],
            vec![model(GenerationRepresentationContract::full())],
            vec![canonical()],
        )
        .unwrap();
        let order: Vec<_> = topology
            .route_endpoints(&RouteId::new("fixture-route").unwrap())
            .into_iter()
            .map(|e| e.id.as_str())
            .collect();
        assert_eq!(order, ["e-responses", "e-chat"]);
        let request = crate::adapter::Adapter::new(
            crate::protocol::openai::Profile::Responses,
            crate::adapter::Dialect::Standard,
            None,
        )
        .decode_request(br#"{"model":"fixture-model","input":"keep"}"#)
        .unwrap();
        let original = request.clone();
        let secret = crate::provider::SecretMaterial::new("synthetic-upstream-key").unwrap();
        for (id, provider_id, upstream_name) in [
            ("e-responses", "fixture", "native-model-name"),
            ("e-chat", "other", "hosted-model-alias"),
        ] {
            let target = topology.endpoint(&EndpointId::new(id).unwrap()).unwrap();
            assert_eq!(target.provider.as_str(), provider_id);
            assert_eq!(target.canonical_model.as_str(), "fixture-model");
            let prepared = crate::execution::prepare(
                target,
                topology.provider(provider_id).unwrap(),
                &secret,
                &request,
            )
            .unwrap();
            let wire: serde_json::Value = serde_json::from_slice(&prepared.body).unwrap();
            assert_eq!(wire["model"], upstream_name);
            assert_eq!(request, original);
        }
        assert!(topology.model("fixture/fixture-model").is_none());
    }

    #[test]
    fn canonical_binding_and_public_semantics_cannot_be_reinterpreted_by_a_route() {
        let mut other = canonical();
        other.id = ModelId::new("another-model").unwrap();
        let mut target = endpoint(ProtocolProfile::OpenAiChat);
        target.canonical_model = other.id.clone();
        assert_eq!(
            compile(
                vec![provider()],
                vec![target],
                vec![route()],
                vec![model(GenerationRepresentationContract::full())],
                vec![canonical(), other]
            ),
            Err(TopologyError::CanonicalModelMismatch)
        );
        assert_eq!(
            compile(
                vec![provider()],
                vec![endpoint(ProtocolProfile::OpenAiChat)],
                vec![route()],
                vec![model(GenerationRepresentationContract::full())],
                vec![]
            ),
            Err(TopologyError::UnknownCanonicalModel)
        );
        let mut limited = canonical();
        limited.contract.image_input = false;
        assert_eq!(
            compile(
                vec![provider()],
                vec![endpoint(ProtocolProfile::OpenAiChat)],
                vec![route()],
                vec![model(GenerationRepresentationContract::full())],
                vec![limited]
            ),
            Err(TopologyError::SemanticUnsatisfiable)
        );
    }

    #[test]
    fn public_aliases_share_one_explicit_canonical_without_mutating_either_contract() {
        let first = model(GenerationRepresentationContract::full());
        let mut second = first.clone();
        second.id = ModelId::new("another-public-alias").unwrap();
        second.contract.temperature = false;
        let compiled = compile_fixture(
            vec![endpoint(ProtocolProfile::OpenAiChat)],
            vec![route()],
            vec![first, second],
        )
        .unwrap();
        assert!(
            compiled
                .canonical_model(&ModelId::new("fixture-model").unwrap())
                .unwrap()
                .contract
                .temperature
        );
        assert!(
            !compiled
                .model("another-public-alias")
                .unwrap()
                .contract
                .temperature
        );
        assert!(
            compiled
                .model("fixture-model")
                .unwrap()
                .contract
                .temperature
        );
    }
    #[test]
    fn duplicate_relations_are_rejected() {
        assert_eq!(
            compile_fixture(
                vec![
                    endpoint(ProtocolProfile::OpenAiChat),
                    endpoint(ProtocolProfile::OpenAiChat)
                ],
                vec![route()],
                vec![model(GenerationRepresentationContract::full())],
            ),
            Err(TopologyError::DuplicateEndpoint)
        );
        assert_eq!(
            compile_fixture(
                vec![endpoint(ProtocolProfile::OpenAiChat)],
                vec![route(), route()],
                vec![model(GenerationRepresentationContract::full())],
            ),
            Err(TopologyError::DuplicateRoute)
        );
        assert_eq!(
            compile_fixture(
                vec![endpoint(ProtocolProfile::OpenAiChat)],
                vec![route()],
                vec![
                    model(GenerationRepresentationContract::full()),
                    model(GenerationRepresentationContract::full()),
                ],
            ),
            Err(TopologyError::DuplicateModel)
        );
    }

    #[test]
    fn target_must_stay_bound_to_provider_origin_and_protocol_entry() {
        let mut chat_only = provider();
        chat_only.responses = None;
        assert_eq!(
            compile(
                vec![chat_only.clone()],
                vec![endpoint(ProtocolProfile::OpenAiResponses)],
                vec![route()],
                vec![model(GenerationRepresentationContract::full())],
                vec![canonical()]
            ),
            Err(TopologyError::TargetMismatch)
        );
        assert!(
            compile(
                vec![chat_only],
                vec![endpoint(ProtocolProfile::OpenAiChat)],
                vec![route()],
                vec![model(GenerationRepresentationContract::full())],
                vec![canonical()]
            )
            .is_ok()
        );
        // Wrong origin: business-selectable targets never compile.
        let mut wrong_origin = endpoint(ProtocolProfile::OpenAiResponses);
        wrong_origin.target.origin = TrustedOrigin::parse("http://127.0.0.1:40000").unwrap();
        assert_eq!(
            compile_fixture(
                vec![wrong_origin],
                vec![Route {
                    id: RouteId::new("fixture-route").unwrap(),
                    task: TaskKind::Generation,
                    endpoints: vec![EndpointId::new("fixture-endpoint").unwrap()],
                    policy: super::super::RoutePolicy::default(),
                }],
                vec![model(GenerationRepresentationContract::full())],
            ),
            Err(TopologyError::TargetMismatch)
        );
        // Wrong entry: a Responses endpoint cannot ride the chat entry path.
        let mut wrong_entry = endpoint(ProtocolProfile::OpenAiResponses);
        wrong_entry.target.path = provider().chat_completions.unwrap();
        assert_eq!(
            compile_fixture(
                vec![wrong_entry],
                vec![route()],
                vec![model(GenerationRepresentationContract::full())],
            ),
            Err(TopologyError::TargetMismatch)
        );
    }

    #[test]
    fn dangling_and_mismatched_relations_are_rejected() {
        assert_eq!(
            compile_fixture(
                vec![],
                vec![Route {
                    id: RouteId::new("fixture-route").unwrap(),
                    task: TaskKind::Generation,
                    endpoints: vec![EndpointId::new("missing").unwrap()],
                    policy: super::super::RoutePolicy::default(),
                }],
                vec![],
            ),
            Err(TopologyError::UnknownEndpoint)
        );
        assert_eq!(
            compile_fixture(
                vec![endpoint(ProtocolProfile::OpenAiChat)],
                vec![],
                vec![model(GenerationRepresentationContract::full())],
            ),
            Err(TopologyError::UnknownRoute)
        );
        assert_eq!(
            compile_fixture(
                vec![endpoint(ProtocolProfile::OpenAiChat)],
                vec![Route {
                    id: RouteId::new("fixture-route").unwrap(),
                    task: TaskKind::Generation,
                    endpoints: vec![],
                    policy: super::super::RoutePolicy::default(),
                }],
                vec![],
            ),
            Err(TopologyError::EmptyRoute)
        );
        assert_eq!(
            compile_fixture(
                vec![Endpoint {
                    provider: ProviderId::new("missing").unwrap(),
                    ..endpoint(ProtocolProfile::OpenAiChat)
                }],
                vec![route()],
                vec![model(GenerationRepresentationContract::full())],
            ),
            Err(TopologyError::UnknownProvider)
        );
    }

    #[test]
    fn public_contract_must_be_satisfiable_by_every_candidate() {
        let promise = GenerationRepresentationContract {
            semantics: GenerationSemanticContract {
                strict_tools: true,
                ..GenerationSemanticContract::full()
            },
            ..GenerationRepresentationContract::full()
        };
        let candidate = GenerationRepresentationContract {
            semantics: GenerationSemanticContract {
                strict_tools: false,
                ..GenerationSemanticContract::full()
            },
            ..GenerationRepresentationContract::full()
        };
        assert_eq!(
            compile_fixture(
                vec![Endpoint {
                    representation: candidate,
                    ..endpoint(ProtocolProfile::OpenAiChat)
                }],
                vec![route()],
                vec![model(promise)],
            ),
            Err(TopologyError::ContractUnsatisfiable)
        );
    }

    #[test]
    fn execution_limits_and_model_bindings_must_be_concrete() {
        assert_eq!(
            compile_fixture(
                vec![Endpoint {
                    execution: ExecutionContract {
                        request_body_limit: 0,
                        ..endpoint(ProtocolProfile::OpenAiChat).execution
                    },
                    ..endpoint(ProtocolProfile::OpenAiChat)
                }],
                vec![route()],
                vec![model(GenerationRepresentationContract::full())],
            ),
            Err(TopologyError::InvalidExecutionLimits)
        );
        assert_eq!(
            compile_fixture(
                vec![Endpoint {
                    upstream_model: "model with spaces".into(),
                    ..endpoint(ProtocolProfile::OpenAiChat)
                }],
                vec![route()],
                vec![model(GenerationRepresentationContract::full())],
            ),
            Err(TopologyError::InvalidModelBinding)
        );
    }
}
