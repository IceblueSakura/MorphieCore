//! Explicit subscription bindings. Account aliases remain deployment inputs.
use super::{models, *};
use crate::adapter::Dialect;
pub struct SubscriptionBinding {
    pub profile: &'static str,
    pub model: &'static str,
    pub upstream: &'static str,
    pub dialect: Dialect,
    pub provider: fn() -> crate::provider::ProviderDefinition,
}
pub const SUBSCRIPTION_BINDINGS: &[SubscriptionBinding] = &[
    SubscriptionBinding {
        profile: "siwc",
        model: "gpt-6.1-sol",
        upstream: "gpt-6.1-sol",
        dialect: Dialect::Siwc,
        provider: catalog::siwc,
    },
    SubscriptionBinding {
        profile: "grok",
        model: "grok-4.7",
        upstream: "grok-4.7",
        dialect: Dialect::Grok,
        provider: catalog::grok,
    },
];
impl SubscriptionBinding {
    fn contract(&self) -> GenerationSemanticContract {
        let mut contract = models::contract(self.model);
        if self.profile == "siwc" {
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
        EndpointId::new(&format!("{}-responses", (self.provider)().id.as_str())).expect("static id")
    }
    pub fn public_model(&self) -> PublicModel {
        PublicModel {
            id: ModelId::new(self.model).unwrap(),
            canonical_model: ModelId::new(self.model).unwrap(),
            task: TaskKind::Generation,
            route: RouteId::new(&format!("{}-generation", (self.provider)().id.as_str())).unwrap(),
            contract: self.contract(),
            standard_context: false,
            reported_facts: ReportedFactPolicy::Faithful,
        }
    }
    pub fn endpoint(&self) -> Endpoint {
        let provider = (self.provider)();
        let scope = ReplayOrigin::new(self.profile).unwrap();
        let adapter = Adapter::new(
            crate::protocol::openai::Profile::Responses,
            self.dialect,
            Some(scope.clone()),
        );
        let contract = GenerationRepresentationContract {
            semantics: self.contract(),
            replay_origin: Some(scope),
            cache: adapter.adaptation.cache,
            identity_hints: false,
            standard_context: false,
            ..GenerationRepresentationContract::full()
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
            representation: adapter.contract(&contract),
            execution: ExecutionContract {
                streaming: true,
                retry_before_commit: false,
                request_body_limit: 256 << 10,
                response_body_limit: 8 << 20,
                credential_kind: CredentialKind::OAuth(self.profile),
                timeout_ms: 120_000,
            },
            credential: self.credential(),
        }
    }
}
