//! Compile trusted topology input into one immutable relation set.

use crate::{
    provider::ProviderDefinition,
    topology::{CanonicalModel, Dispositions, Endpoint, EndpointId, PublicModel, Route, RouteId},
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
fn promised(semantic: &Dispositions, endpoint: &crate::lowering::target::TargetContract) -> bool {
    endpoint.dispositions().supports(semantic)
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompiledTopology {
    providers: BTreeMap<String, ProviderDefinition>,
    endpoint_providers: BTreeMap<String, ProviderDefinition>,
    endpoints: BTreeMap<String, Endpoint>,
    routes: BTreeMap<String, Route>,
    models: BTreeMap<String, PublicModel>,
    canonical_models: BTreeMap<String, CanonicalModel>,
    pub(super) image_routes: BTreeMap<String, super::images::ImageRoute>,
    pub(super) speech_routes: BTreeMap<String, super::speech::SpeechRoute>,
    pub(super) transcription_routes: BTreeMap<String, super::transcription::TranscriptionRoute>,
    pub(super) embedding_routes: BTreeMap<String, super::embeddings::EmbeddingRoute>,
    pub(super) model_metadata: BTreeMap<String, super::ModelMetadata>,
}

impl CompiledTopology {
    pub fn provider(&self, id: &str) -> Option<&ProviderDefinition> {
        self.providers.get(id)
    }

    pub fn endpoint(&self, id: &EndpointId) -> Option<&Endpoint> {
        self.endpoints.get(id.as_str())
    }

    /// Deployment-local declarations preserve the credential domain without
    /// retargeting another binding that uses the same Provider.
    pub fn endpoint_provider(&self, endpoint: &Endpoint) -> Option<&ProviderDefinition> {
        self.endpoint_providers
            .get(endpoint.id.as_str())
            .or_else(|| self.provider(endpoint.provider.as_str()))
    }

    pub(crate) fn with_generation(
        self,
        candidates: Vec<(Endpoint, ProviderDefinition)>,
        routes: Vec<Route>,
        models: Vec<PublicModel>,
    ) -> Result<Self, TopologyError> {
        let mut declarations = BTreeMap::new();
        let mut endpoints = Vec::new();
        for (endpoint, provider) in candidates {
            if declarations
                .insert(endpoint.id.as_str().to_owned(), provider)
                .is_some()
            {
                return Err(TopologyError::DuplicateEndpoint);
            }
            endpoints.push(endpoint);
        }
        let mut compiled = compile_deployments(
            self.providers.into_values().collect(),
            endpoints,
            routes,
            models,
            self.canonical_models.into_values().collect(),
            declarations,
        )?;
        compiled.image_routes = self.image_routes;
        compiled.speech_routes = self.speech_routes;
        compiled.transcription_routes = self.transcription_routes;
        compiled.embedding_routes = self.embedding_routes;
        compiled.model_metadata = self.model_metadata;
        Ok(compiled)
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
    compile_deployments(
        providers,
        endpoints,
        routes,
        models,
        canonical_models,
        BTreeMap::new(),
    )
}

fn compile_deployments(
    providers: Vec<ProviderDefinition>,
    endpoints: Vec<Endpoint>,
    routes: Vec<Route>,
    models: Vec<PublicModel>,
    canonical_models: Vec<CanonicalModel>,
    endpoint_providers: BTreeMap<String, ProviderDefinition>,
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
        let definition = endpoint_providers
            .get(endpoint.id.as_str())
            .or_else(|| provider_map.get(endpoint.provider.as_str()))
            .ok_or(TopologyError::UnknownProvider)?;
        if definition.id != endpoint.provider || definition.origin != endpoint.target.origin {
            return Err(TopologyError::TargetMismatch);
        }
        let operation = definition
            .generation_entry(endpoint.protocol.operation())
            .ok_or(TopologyError::TargetMismatch)?;
        if endpoint.declared_path(operation.path).as_ref().ok() != Some(&endpoint.target.path)
            || operation.auth.kind() != endpoint.execution.credential_kind
            || operation.auth.kind() != definition.auth.kind()
        {
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
            || route
                .policy
                .total_timeout_ms
                .is_some_and(|ms| !(1..=86_400_000).contains(&ms))
        {
            return Err(TopologyError::InvalidRoutePolicy);
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut siwc_seen = false;
        for candidate in &route.endpoints {
            if !seen.insert(candidate) {
                return Err(TopologyError::DuplicateCandidate);
            }
            let endpoint = endpoint_map
                .get(candidate.as_str())
                .ok_or(TopologyError::UnknownEndpoint)?;
            if endpoint.execution.credential_kind.single_account() {
                // Another Provider is a legal fallback, not another SIWC account
                // or endpoint. A standalone subscription still has one attempt.
                if siwc_seen
                    || route.endpoints.len() == 1
                        && (route.policy.max_attempts != 1
                            || route.policy.fallback != super::FallbackPolicy::Disabled)
                {
                    return Err(TopologyError::InvalidRoutePolicy);
                }
                siwc_seen = true;
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
            // Skippable candidates can be narrower than the public input contract.
            if route.policy.candidates == super::CandidatePolicy::RequireAll
                && !promised(&model.contract, &endpoint.representation)
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
        endpoint_providers,
        endpoints: endpoint_map,
        routes: route_map,
        models: model_map,
        canonical_models: canonical_map,
        image_routes: BTreeMap::new(),
        speech_routes: BTreeMap::new(),
        transcription_routes: BTreeMap::new(),
        embedding_routes: BTreeMap::new(),
        model_metadata: BTreeMap::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        protocol::openai::TargetContract,
        provider::{
            CredentialBindingId, CredentialKind, EndpointPath, ProviderDefinition, ProviderId,
            TrustedOrigin, auth::AuthScheme,
        },
        topology::{
            Endpoint, EndpointTarget, ExecutionContract, GenerationProtocol, ModelId, TaskKind,
        },
    };

    fn provider() -> ProviderDefinition {
        ProviderDefinition {
            id: ProviderId::new("fixture").unwrap(),
            origin: TrustedOrigin::parse("http://127.0.0.1:39217").unwrap(),
            endpoints: [
                (
                    crate::provider::ProviderOperation::ChatCompletions,
                    crate::provider::ProviderEndpoint {
                        path: EndpointPath::new("/chat/completions").unwrap(),
                        auth: None,
                    },
                ),
                (
                    crate::provider::ProviderOperation::Responses,
                    crate::provider::ProviderEndpoint {
                        path: EndpointPath::new("/responses").unwrap(),
                        auth: None,
                    },
                ),
            ]
            .into(),
            credential_pool: None,
            auth: AuthScheme::Bearer,
        }
    }

    fn endpoint(protocol: GenerationProtocol) -> Endpoint {
        let definition = provider();
        let path = definition
            .generation_entry(protocol.operation())
            .expect("test operation")
            .path
            .clone();
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
            representation: TargetContract::full().into(),
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
            contract: Dispositions::full(),
        }
    }
    fn model(contract: TargetContract) -> PublicModel {
        PublicModel::new(
            ModelId::new("fixture-model").unwrap(),
            ModelId::new("fixture-model").unwrap(),
            TaskKind::Generation,
            RouteId::new("fixture-route").unwrap(),
            contract.dispositions,
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
    fn siwc_cross_provider_routes_keep_one_subscription_candidate() {
        for (attempts, fallback, other_kind, accepted) in [
            (1, false, None, true),
            (2, false, None, false),
            (1, true, None, false),
            (2, true, Some(CredentialKind::ApiKey), true),
            (2, true, Some(CredentialKind::OAuth("openai")), false),
        ] {
            let mut provider = provider();
            provider.auth = AuthScheme::OAuthBearer("openai");
            let mut endpoint = endpoint(GenerationProtocol::OpenAiResponses);
            endpoint.execution.credential_kind = CredentialKind::OAuth("openai");
            let mut providers = vec![provider.clone()];
            let mut endpoints = vec![endpoint.clone()];
            let mut route = route();
            route.policy.max_attempts = attempts;
            if fallback {
                route.policy.fallback = crate::topology::FallbackPolicy::BeforeCommit;
            }
            if let Some(kind) = other_kind {
                provider.id = ProviderId::new("other").unwrap();
                if kind == CredentialKind::ApiKey {
                    provider.auth = AuthScheme::Bearer;
                }
                endpoint.provider = provider.id.clone();
                endpoint.execution.credential_kind = kind;
                endpoint.id = EndpointId::new("second").unwrap();
                route.endpoints.push(endpoint.id.clone());
                endpoints.push(endpoint);
                providers.push(provider);
            }
            assert_eq!(
                compile(
                    providers,
                    endpoints,
                    vec![route],
                    vec![model(TargetContract::full())],
                    vec![canonical()]
                )
                .is_ok(),
                accepted,
                "attempts={attempts} fallback={fallback} other_kind={other_kind:?}"
            );
        }
    }

    #[test]
    fn canonical_binding_and_public_semantics_cannot_be_reinterpreted_by_a_route() {
        let mut other = canonical();
        other.id = ModelId::new("another-model").unwrap();
        let mut target = endpoint(GenerationProtocol::OpenAiChat);
        target.canonical_model = other.id.clone();
        assert_eq!(
            compile(
                vec![provider()],
                vec![target],
                vec![route()],
                vec![model(TargetContract::full())],
                vec![canonical(), other]
            ),
            Err(TopologyError::CanonicalModelMismatch)
        );
        assert_eq!(
            compile(
                vec![provider()],
                vec![endpoint(GenerationProtocol::OpenAiChat)],
                vec![route()],
                vec![model(TargetContract::full())],
                vec![]
            ),
            Err(TopologyError::UnknownCanonicalModel)
        );
        let mut limited = canonical();
        limited.contract.set_native(
            crate::semantic::task::generation::Feature::ImageInput,
            false,
        );
        assert_eq!(
            compile(
                vec![provider()],
                vec![endpoint(GenerationProtocol::OpenAiChat)],
                vec![route()],
                vec![model(TargetContract::full())],
                vec![limited]
            ),
            Err(TopologyError::SemanticUnsatisfiable)
        );
    }

    #[test]
    fn duplicate_relations_are_rejected() {
        assert_eq!(
            compile_fixture(
                vec![
                    endpoint(GenerationProtocol::OpenAiChat),
                    endpoint(GenerationProtocol::OpenAiChat)
                ],
                vec![route()],
                vec![model(TargetContract::full())],
            ),
            Err(TopologyError::DuplicateEndpoint)
        );
        assert_eq!(
            compile_fixture(
                vec![endpoint(GenerationProtocol::OpenAiChat)],
                vec![route(), route()],
                vec![model(TargetContract::full())],
            ),
            Err(TopologyError::DuplicateRoute)
        );
        assert_eq!(
            compile_fixture(
                vec![endpoint(GenerationProtocol::OpenAiChat)],
                vec![route()],
                vec![model(TargetContract::full()), model(TargetContract::full()),],
            ),
            Err(TopologyError::DuplicateModel)
        );
    }

    #[test]
    fn target_must_stay_bound_to_provider_origin_and_protocol_entry() {
        let mut chat_only = provider();
        chat_only
            .endpoints
            .remove(&crate::provider::ProviderOperation::Responses);
        assert_eq!(
            compile(
                vec![chat_only.clone()],
                vec![endpoint(GenerationProtocol::OpenAiResponses)],
                vec![route()],
                vec![model(TargetContract::full())],
                vec![canonical()]
            ),
            Err(TopologyError::TargetMismatch)
        );
        assert!(
            compile(
                vec![chat_only],
                vec![endpoint(GenerationProtocol::OpenAiChat)],
                vec![route()],
                vec![model(TargetContract::full())],
                vec![canonical()]
            )
            .is_ok()
        );
        // Wrong origin: business-selectable targets never compile.
        let mut wrong_origin = endpoint(GenerationProtocol::OpenAiResponses);
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
                vec![model(TargetContract::full())],
            ),
            Err(TopologyError::TargetMismatch)
        );
        // Wrong entry: a Responses endpoint cannot ride the chat entry path.
        let mut wrong_entry = endpoint(GenerationProtocol::OpenAiResponses);
        wrong_entry.target.path = provider()
            .entry(crate::provider::ProviderOperation::ChatCompletions)
            .unwrap()
            .path
            .clone();
        assert_eq!(
            compile_fixture(
                vec![wrong_entry],
                vec![route()],
                vec![model(TargetContract::full())],
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
                vec![endpoint(GenerationProtocol::OpenAiChat)],
                vec![],
                vec![model(TargetContract::full())],
            ),
            Err(TopologyError::UnknownRoute)
        );
        assert_eq!(
            compile_fixture(
                vec![endpoint(GenerationProtocol::OpenAiChat)],
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
                    ..endpoint(GenerationProtocol::OpenAiChat)
                }],
                vec![route()],
                vec![model(TargetContract::full())],
            ),
            Err(TopologyError::UnknownProvider)
        );
    }

    #[test]
    fn public_contract_must_be_satisfiable_by_every_candidate() {
        let promise = TargetContract {
            dispositions: (Dispositions::full()).with_native(
                crate::semantic::task::generation::Feature::StrictTools,
                true,
            ),
            ..TargetContract::full()
        };
        let candidate = TargetContract {
            dispositions: (Dispositions::full()).with_native(
                crate::semantic::task::generation::Feature::StrictTools,
                false,
            ),
            ..TargetContract::full()
        };
        assert_eq!(
            compile_fixture(
                vec![Endpoint {
                    representation: candidate.into(),
                    ..endpoint(GenerationProtocol::OpenAiChat)
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
                        ..endpoint(GenerationProtocol::OpenAiChat).execution
                    },
                    ..endpoint(GenerationProtocol::OpenAiChat)
                }],
                vec![route()],
                vec![model(TargetContract::full())],
            ),
            Err(TopologyError::InvalidExecutionLimits)
        );
        assert_eq!(
            compile_fixture(
                vec![Endpoint {
                    upstream_model: "model with spaces".into(),
                    ..endpoint(GenerationProtocol::OpenAiChat)
                }],
                vec![route()],
                vec![model(TargetContract::full())],
            ),
            Err(TopologyError::InvalidModelBinding)
        );
    }
}
