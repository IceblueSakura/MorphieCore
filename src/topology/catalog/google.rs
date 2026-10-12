//! Google AI Studio bindings. One public model, its Google endpoints in failover order.
use crate::{
    protocol::google,
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
            representation: google::TargetCaps::default().into(),
            execution: ExecutionContract {
                streaming: true,
                retry_before_commit: false,
                request_body_limit: 256 << 10,
                response_body_limit: 8 << 20,
                credential_kind: CredentialKind::ApiKey,
                timeout_ms: 120_000,
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
        endpoint.representation = crate::protocol::interactions::TargetCaps::default().into();
        endpoint
    }
    pub fn public_model(&self) -> PublicModel {
        let model = ModelId::new(self.model).expect("static model");
        let mut public = PublicModel::new(
            model.clone(),
            model,
            TaskKind::Generation,
            self.route().id,
            google::semantic_contract(),
        );
        public.standard_context = false;
        public
    }
    pub fn canonical_model(&self) -> CanonicalModel {
        CanonicalModel {
            id: ModelId::new(self.model).expect("static model"),
            task: TaskKind::Generation,
            contract: google::semantic_contract(),
        }
    }
}
