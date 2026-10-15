//! Explicit subscription bindings. Account aliases remain deployment inputs.
use super::{models, *};
use crate::protocol::openai::Dialect;
pub struct SubscriptionBinding {
    pub profile: &'static str,
    pub identity: &'static str,
    pub model: &'static str,
    pub upstream: &'static str,
    pub dialect: Dialect,
    pub provider: fn() -> crate::provider::ProviderDefinition,
}
pub const SUBSCRIPTION_BINDINGS: &[SubscriptionBinding] = &[
    SubscriptionBinding {
        profile: "openai",
        identity: "openai",
        model: "gpt-6.1-sol",
        upstream: "gpt-6.1-sol",
        dialect: Dialect::Siwc,
        provider: catalog::openai,
    },
    SubscriptionBinding {
        profile: "openai",
        identity: "openai-astra",
        model: "gpt-6-astra",
        upstream: "gpt-6-astra",
        dialect: Dialect::Siwc,
        provider: catalog::openai,
    },
    SubscriptionBinding {
        profile: "grok",
        identity: "grok",
        model: "grok-4.7",
        upstream: "grok-4.7",
        dialect: Dialect::Grok,
        provider: catalog::grok,
    },
];
impl SubscriptionBinding {
    fn contract(&self) -> GenerationSemanticContract {
        let mut contract = models::contract(self.model);
        if self.profile == "openai" {
            contract.max_output_tokens = false;
            contract.temperature = false;
            contract.top_p = false;
            contract.logprobs = false;
            contract.truncation = false;
        }
        contract
    }
    pub fn credential(&self) -> CredentialBindingId {
        CredentialBindingId::new(&format!("{}-oauth", (self.provider)().id.as_str()))
            .expect("static binding")
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
            contract: self.contract(),
            standard_context: false,
            reported_facts: ReportedFactPolicy::Faithful,
        }
    }
    pub fn endpoint(&self) -> Endpoint {
        let provider = (self.provider)();
        let scope = crate::protocol::replay_envelope::issuer(provider.id.as_str(), self.upstream);
        let adapter = Adapter::new(
            crate::protocol::openai::Profile::Responses,
            self.dialect,
            Some(scope.clone()),
        );
        let contract = TargetCaps {
            semantics: self.contract(),
            replay_origin: Some(scope),
            cache: adapter.adaptation.cache,
            identity_hints: false,
            standard_context: false,
            ..TargetCaps::full()
        };
        Endpoint {
            id: self.endpoint_id(),
            canonical_model: ModelId::new(self.model).unwrap(),
            provider: provider.id,
            target: EndpointTarget {
                origin: provider.origin,
                path: provider.responses.unwrap(),
            },
            task: TaskKind::Generation,
            protocol: ProtocolProfile::OpenAiResponses,
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
