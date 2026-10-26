//! Google AI Studio bindings. One public model, its Google endpoints in failover order.
use crate::{
    protocol::{
        google::{generate_content, interactions},
        model_limits::ModelLimits,
        reasoning::{ReasoningConversion, ReasoningRequestPolicy},
    },
    provider::{CredentialBindingId, CredentialKind, ProviderDefinition, catalog},
    topology::*,
};
pub struct GoogleBinding {
    pub model: &'static str,
    pub upstream: &'static str,
    pub released_at: u64,
    pub developer: &'static str,
}
/// Selected development target; registration is not account or inference evidence.
pub const GOOGLE_BINDINGS: &[GoogleBinding] = &[GoogleBinding {
    model: "gemini-3.8-flash",
    upstream: "gemini-3.8-flash",
    // Source: https://ai.google.dev/gemini-api/docs/changelog (2026-09-02).
    released_at: 1_788_307_200,
    developer: "Google",
}];
impl GoogleBinding {
    pub fn credential(&self) -> CredentialBindingId {
        self.provider()
            .credential_pool
            .expect("catalog credential pool")
    }
    fn reasoning_policy(&self) -> Option<ReasoningRequestPolicy> {
        use crate::semantic::task::generation::{EffortLevel::*, EffortScope};
        // Selected scope approximation, not equivalence of response-wide effort.
        (self.model == "gemini-3.8-flash").then(|| ReasoningRequestPolicy {
            canonical_model: self.model.into(),
            rule: "google.gemini-3.8-flash.response-effort-to-thinking.v1",
            conversion: ReasoningConversion::Scope {
                from: EffortScope::Response,
                to: EffortScope::Reasoning,
                levels: &[(Low, Low), (Medium, Medium), (High, High)],
            },
        })
    }
    fn accepted_contract(&self) -> GenerationFeatureSet {
        let mut contract = generate_content::semantic_contract();
        // The existing Interactions candidate retains its official image-result carrier.
        contract.tool_result_media = true;
        contract.response_effort = self.reasoning_policy().is_some();
        contract
    }

    fn model_limits(&self) -> ModelLimits {
        use crate::semantic::task::generation::EffortLevel::{High, Low, Medium};
        // https://ai.google.dev/gemini-api/docs/models/gemini-3.8-flash (2026-10-20).
        ModelLimits {
            allowed_effort: Some(vec![Low, Medium, High].into()),
            maximum_output_tokens: Some(65_536),
        }
    }
    pub fn provider(&self) -> ProviderDefinition {
        catalog::google()
    }
    pub fn metadata(&self) -> ModelMetadata {
        ModelMetadata::new(self.released_at, self.developer).expect("static publication")
    }
    pub fn endpoint(&self) -> Endpoint {
        let provider = self.provider();
        let ignored_sampling = self.model == "gemini-3.8-flash";
        let collection = provider
            .generation_entry(crate::provider::GenerationOperation::GoogleGenerateContent)
            .expect("declared path")
            .path;
        let path = crate::provider::GenerationOperation::GoogleGenerateContent
            .resolve_path(collection, self.upstream, true)
            .expect("static path");
        Endpoint {
            id: EndpointId::new(&format!("google-{}-generate-content", self.model))
                .expect("static id"),
            provider: provider.id,
            target: EndpointTarget {
                origin: provider.origin,
                path,
            },
            task: TaskKind::Generation,
            protocol: GenerationProtocol::GoogleGenerateContent,
            upstream_model: self.upstream.into(),
            canonical_model: ModelId::new(self.model).expect("static model"),
            representation: generate_content::TargetContract {
                semantics: GenerationFeatureSet {
                    temperature: !ignored_sampling,
                    top_p: !ignored_sampling,
                    ..generate_content::semantic_contract()
                },
                sampling: if ignored_sampling {
                    crate::protocol::google::SamplingPolicy::Ignored
                } else {
                    crate::protocol::google::SamplingPolicy::Native
                },
                model_limits: self.model_limits(),
                reasoning_policy: self.reasoning_policy(),
                messages_terminal_projection: self.model == "gemini-3.8-flash",
                ..Default::default()
            }
            .into(),
            execution: ExecutionContract {
                streaming: true,
                retry_before_commit: false,
                request_body_limit: super::GENERATION_REQUEST_BYTES,
                response_body_limit: super::GENERATION_RESPONSE_BYTES,
                credential_kind: CredentialKind::ApiKey,
                timeout_ms: super::GENERATION_TIMEOUT_MS,
            },
            credential: self.credential(),
        }
    }
    pub fn route(&self) -> Route {
        Route {
            id: RouteId::new(&format!("google-{}", self.model)).expect("static route"),
            task: TaskKind::Generation,
            endpoints: vec![self.endpoint().id, self.interactions_endpoint().id],
            policy: RoutePolicy {
                candidates: CandidatePolicy::SkipUnrepresentable,
                fallback: FallbackPolicy::BeforeCommit,
                ..RoutePolicy::default()
            },
        }
    }
    pub fn interactions_endpoint(&self) -> Endpoint {
        let mut endpoint = self.endpoint();
        endpoint.id =
            EndpointId::new(&format!("google-{}-interactions", self.model)).expect("static id");
        endpoint.protocol = GenerationProtocol::GoogleInteractions;
        endpoint.target.path = self
            .provider()
            .generation_entry(crate::provider::GenerationOperation::GoogleInteractions)
            .expect("declared path")
            .path
            .clone();
        endpoint.representation = interactions::TargetContract {
            sampling: if self.model == "gemini-3.8-flash" {
                crate::protocol::google::SamplingPolicy::Ignored
            } else {
                crate::protocol::google::SamplingPolicy::Reject
            },
            model_limits: self.model_limits(),
            reasoning_policy: self.reasoning_policy(),
            messages_terminal_projection: self.model == "gemini-3.8-flash",
            ..Default::default()
        }
        .into();
        endpoint
    }
    pub fn public_model(&self) -> PublicModel {
        let model = ModelId::new(self.model).expect("static model");
        let mut public = PublicModel::new(
            model.clone(),
            model,
            TaskKind::Generation,
            self.route().id,
            self.accepted_contract(),
        );
        public.standard_context = false;
        public
    }
    pub fn canonical_model(&self) -> CanonicalModel {
        CanonicalModel {
            id: ModelId::new(self.model).expect("static model"),
            task: TaskKind::Generation,
            contract: self.accepted_contract(),
        }
    }
}
