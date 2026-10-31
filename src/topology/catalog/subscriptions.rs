//! Explicit subscription bindings. Account aliases remain deployment inputs.
use super::{models, *};
use crate::lowering::disposition::Dispositions;
use crate::protocol::openai::Dialect;
pub struct SubscriptionBinding {
    pub profile: &'static str,
    pub identity: &'static str,
    pub model: &'static str,
    pub upstream: &'static str,
    pub dialect: Dialect,
    pub reasoning_history: Option<crate::protocol::reasoning::ReasoningHistory>,
    pub provider: fn() -> crate::provider::ProviderDefinition,
}
pub const SUBSCRIPTION_BINDINGS: &[SubscriptionBinding] = &[
    SubscriptionBinding {
        profile: "openai",
        identity: "openai",
        reasoning_history: Some(crate::protocol::reasoning::ReasoningHistory::Accepted),
        model: "gpt-6.1-sol",
        upstream: "gpt-6.1-sol",
        dialect: Dialect::Siwc,
        provider: catalog::openai,
    },
    SubscriptionBinding {
        profile: "openai",
        identity: "openai-astra",
        reasoning_history: Some(crate::protocol::reasoning::ReasoningHistory::Accepted),
        model: "gpt-6-astra",
        upstream: "gpt-6-astra",
        dialect: Dialect::Siwc,
        provider: catalog::openai,
    },
    SubscriptionBinding {
        profile: "openai",
        identity: "openai-luna",
        reasoning_history: Some(crate::protocol::reasoning::ReasoningHistory::Accepted),
        model: "gpt-6-luna",
        upstream: "gpt-6-luna",
        dialect: Dialect::Siwc,
        provider: catalog::openai,
    },
    SubscriptionBinding {
        profile: "grok",
        identity: "grok",
        reasoning_history: Some(crate::protocol::reasoning::ReasoningHistory::Accepted),
        model: "grok-4.7",
        upstream: "grok-4.7",
        dialect: Dialect::Grok,
        provider: catalog::grok,
    },
];
impl SubscriptionBinding {
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
        (self.provider)()
            .credential_pool
            .expect("catalog credential pool")
    }
    pub fn endpoint_id(&self) -> EndpointId {
        EndpointId::new(&format!("{}-responses", self.identity)).expect("static id")
    }
    pub fn public_model(&self) -> PublicModel {
        PublicModel {
            id: ModelId::new(self.model).unwrap(),
            canonical_model: ModelId::new(self.model).unwrap(),
            task: TaskKind::Generation,
            route: RouteId::new(&format!("{}-generation", self.identity)).unwrap(),
            // An alternative keeps the existing public promise; SIWC restrictions
            // still apply to this endpoint during candidate preparation.
            contract: API_KEY_BINDINGS
                .iter()
                .find(|binding| binding.model == self.model)
                .map(|binding| binding.public_model().contract)
                .unwrap_or_else(|| self.contract()),
            reported_facts: ReportedFactPolicy::Faithful,
        }
    }
    pub fn endpoint(&self) -> Endpoint {
        let provider = (self.provider)();
        let scope = crate::protocol::replay_envelope::issuer(provider.id.as_str(), self.upstream);
        let adapter = GenerationCodec::new(
            crate::protocol::openai::GenerationApi::Responses,
            self.dialect,
            Some(scope.clone()),
        );
        let contract = TargetContract {
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
            id: self.endpoint_id(),
            canonical_model: ModelId::new(self.model).unwrap(),
            provider: provider.id,
            target: EndpointTarget {
                origin: provider.origin,
                path,
            },
            task: TaskKind::Generation,
            protocol: GenerationProtocol::OpenAiResponses,
            upstream_model: self.upstream.into(),
            representation: adapter.contract(&contract).into(),
            execution: ExecutionContract {
                streaming: true,
                retry_before_commit: false,
                request_body_limit: super::GENERATION_REQUEST_BYTES,
                response_body_limit: super::GENERATION_RESPONSE_BYTES,
                credential_kind: CredentialKind::OAuth(self.profile),
                timeout_ms: super::GENERATION_TIMEOUT_MS,
            },
            credential: self.credential(),
        }
    }
}
