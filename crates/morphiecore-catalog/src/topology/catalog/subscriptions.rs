//! Explicit subscription bindings. Account aliases remain deployment inputs.
use super::{models, *};
use crate::lowering::disposition::Dispositions;
use crate::protocol::openai::Dialect;
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubscriptionBinding {
    pub profile: &'static str,
    pub identity: &'static str,
    pub model: &'static str,
    pub upstream: &'static str,
    pub dialect: Dialect,
    pub reasoning_history: Option<crate::protocol::reasoning::ReasoningHistory>,
    pub(super) provider: &'static str,
    #[serde(default)]
    pub(super) limits: crate::protocol::model_limits::ModelLimits,
}

impl SubscriptionBinding {
    pub fn provider(&self) -> crate::provider::ProviderDefinition {
        super::data::provider(self.provider).clone()
    }

    fn contract(&self) -> Dispositions {
        let mut contract = models::contract(self.model);
        if self.profile == "openai" {
            contract.set_native(
                crate::semantic::task::generation::Feature::MaxOutputTokens,
                false,
            );
            contract.set_native(
                crate::semantic::task::generation::Feature::Temperature,
                false,
            );
            contract.set_native(crate::semantic::task::generation::Feature::TopP, false);
            contract.set_native(crate::semantic::task::generation::Feature::Logprobs, false);
            contract.set_native(
                crate::semantic::task::generation::Feature::Truncation,
                false,
            );
        }
        contract
    }
    pub fn credential(&self) -> CredentialBindingId {
        CredentialBindingId::new(self.provider().id.as_str()).expect("catalog Provider identity")
    }
    pub fn endpoint_id(&self) -> EndpointId {
        EndpointId::new(&format!("{}-responses", self.identity)).expect("static id")
    }
    pub fn public_model(&self) -> PublicModel {
        PublicModel {
            id: ModelId::new(self.model).unwrap(),
            canonical_model: ModelId::new(self.model).unwrap(),
            route: RouteId::new(&format!("{}-generation", self.identity)).unwrap(),
            // An alternative keeps the existing public promise; SIWC restrictions
            // still apply to this endpoint during candidate preparation.
            contract: api_key_bindings()
                .iter()
                .find(|binding| binding.model == self.model)
                .map(|binding| binding.public_model().contract)
                .unwrap_or_else(|| self.contract()),
        }
    }
    pub fn endpoint(&self) -> Endpoint {
        let provider = self.provider();
        let scope = crate::protocol::replay_envelope::issuer(provider.id.as_str(), self.upstream);
        let adapter = GenerationCodec::new(
            crate::protocol::openai::GenerationApi::Responses,
            self.dialect,
            Some(scope.clone()),
        );
        let contract = TargetContract {
            model_limits: self.limits.clone(),
            reasoning_history: self.reasoning_history,
            dispositions: self
                .contract()
                .with_native(crate::semantic::task::generation::Feature::Identity, false)
                .with_native(crate::semantic::task::generation::Feature::Metadata, false)
                .with_native(
                    crate::semantic::task::generation::Feature::ServicePolicy,
                    false,
                )
                .with_native(
                    crate::semantic::task::generation::Feature::ToolBudget,
                    false,
                )
                .with_native(
                    crate::semantic::task::generation::Feature::Prediction,
                    false,
                ),
            replay_origin: Some(scope),
            cache: adapter.adaptation.cache,
            ..TargetContract::full()
        };
        let path = provider
            .generation_entry(crate::provider::GenerationOperation::Responses)
            .expect("declared path")
            .path
            .clone();
        Endpoint {
            bound_replay_scope: None,
            id: self.endpoint_id(),
            canonical_model: ModelId::new(self.model).unwrap(),
            provider: provider.id,
            target: EndpointTarget {
                origin: provider.origin,
                path,
            },
            protocol: GenerationProtocol::OpenAiResponses,
            upstream_model: self.upstream.into(),
            representation: adapter.contract(&contract).into(),
            execution: ExecutionContract {
                streaming: true,
                request_body_limit: super::GENERATION_REQUEST_BYTES,
                response_body_limit: super::GENERATION_RESPONSE_BYTES,
                credential_kind: CredentialKind::OAuth(self.profile),
                timeout_ms: super::GENERATION_TIMEOUT_MS,
            },
            credential: self.credential(),
        }
    }
}
