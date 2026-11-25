//! Google AI Studio bindings. Explicit public models and their native Google Interactions endpoint.
use crate::lowering::disposition::Dispositions;
use crate::{
    protocol::{google::interactions, reasoning::ReasoningRequestPolicy},
    provider::{CredentialBindingId, CredentialKind, ProviderDefinition},
    topology::*,
};
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoogleBinding {
    pub(super) provider: &'static str,
    pub model: &'static str,
    pub upstream: &'static str,
    pub released_at: u64,
    pub developer: &'static str,
    #[serde(default)]
    pub(super) limits: crate::protocol::model_limits::ModelLimits,
    pub(super) scope_effort: Option<super::data::ScopeEffort>,
    #[serde(default)]
    messages_terminal_projection: bool,
}

impl GoogleBinding {
    pub(super) fn endpoint_id(&self) -> Result<EndpointId, TopologyError> {
        EndpointId::new(&format!("google-{}-interactions", self.model))
            .map_err(|_| TopologyError::InvalidModelBinding)
    }
    pub(super) fn route_id(&self) -> Result<RouteId, TopologyError> {
        RouteId::new(&format!("google-{}", self.model))
            .map_err(|_| TopologyError::InvalidModelBinding)
    }

    pub fn credential(&self) -> CredentialBindingId {
        CredentialBindingId::new(self.provider().id.as_str()).expect("catalog Provider identity")
    }
    fn reasoning_policy(&'static self) -> Option<ReasoningRequestPolicy> {
        self.scope_effort
            .as_ref()
            .map(|effort| effort.policy(self.model))
    }
    fn accepted_contract(&'static self) -> Dispositions {
        let mut contract = interactions::semantic_contract();
        contract.set_native(
            crate::semantic::task::generation::Feature::ResponseEffort,
            self.reasoning_policy().is_some(),
        );
        contract
    }

    pub fn provider(&self) -> ProviderDefinition {
        super::data::provider(self.provider).clone()
    }
    pub fn metadata(&self) -> ModelMetadata {
        ModelMetadata::new(self.released_at, self.developer).expect("static publication")
    }
    pub fn endpoint(&'static self) -> Endpoint {
        let provider = self.provider();
        let path = provider
            .generation_entry(crate::provider::GenerationOperation::GoogleInteractions)
            .expect("declared path")
            .path
            .clone();
        Endpoint {
            bound_replay_scope: None,
            id: self.endpoint_id().expect("validated Google endpoint"),
            provider: provider.id,
            target: EndpointTarget {
                origin: provider.origin,
                path,
            },
            protocol: GenerationProtocol::GoogleInteractions,
            upstream_model: self.upstream.into(),
            canonical_model: ModelId::new(self.model).expect("static model"),
            representation: interactions::TargetContract {
                sampling: crate::protocol::google::SamplingPolicy::Ignored,
                model_limits: self.limits.clone(),
                reasoning_policy: self.reasoning_policy(),
                messages_terminal_projection: self.messages_terminal_projection,
                ..Default::default()
            }
            .into(),
            execution: ExecutionContract {
                streaming: true,
                request_body_limit: super::GENERATION_REQUEST_BYTES,
                response_body_limit: super::GENERATION_RESPONSE_BYTES,
                credential_kind: CredentialKind::ApiKey,
                timeout_ms: super::GENERATION_TIMEOUT_MS,
            },
            credential: self.credential(),
        }
    }
    pub fn route(&'static self) -> Route {
        Route {
            id: self.route_id().expect("validated Google route"),
            endpoints: vec![self.endpoint_id().expect("validated Google endpoint")],
            policy: RoutePolicy::default(),
        }
    }
    pub fn public_model(&'static self) -> PublicModel {
        let model = ModelId::new(self.model).expect("static model");
        PublicModel::new(
            model.clone(),
            model,
            self.route_id().expect("validated Google route"),
            self.accepted_contract(),
        )
    }
    pub fn canonical_model(&'static self) -> CanonicalModel {
        CanonicalModel {
            id: ModelId::new(self.model).expect("static model"),
            contract: self.accepted_contract(),
        }
    }
}
