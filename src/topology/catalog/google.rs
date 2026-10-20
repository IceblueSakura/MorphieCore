//! Google AI Studio bindings. One public model, its Google endpoints in failover order.
use crate::{
    protocol::{
        google::{generate_content, interactions},
        model_limits::ModelLimits,
    },
    provider::{CredentialBindingId, CredentialKind, ProviderDefinition, catalog},
    topology::*,
};
pub struct GoogleBinding {
    pub model: &'static str,
    pub upstream: &'static str,
    pub credential: &'static str,
    pub released_at: u64,
    pub developer: &'static str,
}
/// Selected development target; registration is not account or inference evidence.
pub const GOOGLE_BINDINGS: &[GoogleBinding] = &[GoogleBinding {
    model: "gemini-3.8-flash",
    upstream: "gemini-3.8-flash",
    credential: "google-api-key",
    // Source: https://ai.google.dev/gemini-api/docs/changelog (2026-09-02).
    released_at: 1_788_307_200,
    developer: "Google",
}];
impl GoogleBinding {
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
        let collection = provider.generate_content.expect("declared path");
        Endpoint {
            id: EndpointId::new(&format!("google-{}-generate-content", self.model))
                .expect("static id"),
            provider: provider.id,
            target: EndpointTarget {
                origin: provider.origin,
                path: crate::provider::EndpointPath::new(&format!(
                    "{}/{}:streamGenerateContent",
                    collection.as_str(),
                    self.upstream
                ))
                .expect("static path"),
            },
            task: TaskKind::Generation,
            protocol: ProtocolProfile::GoogleGenerateContent,
            upstream_model: self.upstream.into(),
            canonical_model: ModelId::new(self.model).expect("static model"),
            representation: generate_content::TargetCaps {
                model_limits: self.model_limits(),
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
            credential: CredentialBindingId::new(self.credential).expect("static credential"),
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
        endpoint.protocol = ProtocolProfile::GoogleInteractions;
        endpoint.target.path = self.provider().interactions.expect("declared path");
        endpoint.representation = interactions::TargetCaps {
            model_limits: self.model_limits(),
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
            generate_content::semantic_contract(),
        );
        public.standard_context = false;
        public
    }
    pub fn canonical_model(&self) -> CanonicalModel {
        CanonicalModel {
            id: ModelId::new(self.model).expect("static model"),
            task: TaskKind::Generation,
            contract: generate_content::semantic_contract(),
        }
    }
}
