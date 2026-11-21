//! Native Messages bindings. Directory bootstrap activates one only when `models` lists it.
use crate::{
    lowering::disposition::Dispositions,
    protocol::anthropic::messages::{Dialect, Profile, TargetContract},
    provider::{CredentialBindingId, CredentialKind, ProviderDefinition},
    topology::*,
};
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MessagesBinding {
    pub(super) provider: &'static str,
    pub model: &'static str,
    pub upstream: &'static str,
    pub released_at: u64,
    pub developer: &'static str,
    #[serde(default)]
    pub(super) limits: crate::protocol::model_limits::ModelLimits,
}

/// The only (Provider, model) pair whose endpoint identity, dialect and contract
/// below are reviewed; topology data may select it but not describe another.
pub(super) const REVIEWED: (&str, &str) = ("opencode-go", "claude-haiku-5-5");

/// Reviewed native Haiku inputs and pi's ordered agent profile.
fn haiku_contract(mut contract: Dispositions) -> Dispositions {
    contract.set_native(
        crate::semantic::task::generation::Feature::StrictTools,
        true,
    );
    contract.set_native(
        crate::semantic::task::generation::Feature::ToolExamples,
        true,
    );
    contract.set_native(
        crate::semantic::task::generation::Feature::DocumentContext,
        true,
    );
    contract.set_native(
        crate::semantic::task::generation::Feature::AutomaticPrefixCache,
        true,
    );
    contract.set_native(
        crate::semantic::task::generation::Feature::OrderedInstructions,
        true,
    );
    contract.set_native(
        crate::semantic::task::generation::Feature::ToolChanges,
        true,
    );
    contract
}
impl MessagesBinding {
    pub fn credential(&self) -> CredentialBindingId {
        CredentialBindingId::new(self.provider().id.as_str()).expect("catalog Provider identity")
    }
    pub fn provider(&self) -> ProviderDefinition {
        super::data::provider(self.provider).clone()
    }
    pub fn metadata(&self) -> ModelMetadata {
        ModelMetadata::new(self.released_at, self.developer).expect("static publication")
    }
    pub fn endpoint(&self) -> Endpoint {
        let provider = self.provider();
        let mut representation =
            TargetContract::new(Profile::AdaptiveTextTools, Dialect::OpenCodeGo);
        // Preserve the pinned OpenCode Messages fallback as binding policy.
        // https://github.com/anomalyco/opencode/blob/e5a557efeee02c94486d391afc48f24a49ebc9a5/packages/ai/src/protocols/anthropic-messages.ts
        representation.default_output_tokens = Some(32_000);
        representation.model_limits = self.limits.clone();
        representation.dispositions.set_native(
            crate::semantic::task::generation::Feature::ReasoningBudget,
            false,
        );
        representation.automatic_cache_default =
            Some(crate::semantic::task::generation::PrefixCacheTtl::FiveMinutes);
        // Haiku supports owner-local 5m/1h markers; the pinned Go bridge preserves them.
        // This does not admit request-wide automatic caching or certify live cache use.
        representation.dispositions.set_native(
            crate::semantic::task::generation::Feature::ExplicitCache,
            true,
        );
        representation.dispositions = haiku_contract(representation.dispositions);
        representation.dispositions.set_native(
            crate::semantic::task::generation::Feature::ReplayMismatch,
            true,
        );
        representation.dispositions.set_native(
            crate::semantic::task::generation::Feature::MessageEffort,
            true,
        );
        let path = provider
            .generation_entry(crate::provider::GenerationOperation::AnthropicMessages)
            .expect("declared path")
            .path
            .clone();
        Endpoint {
            bound_replay_scope: None,
            id: EndpointId::new("opencode-go-haiku-messages").expect("static id"),
            provider: provider.id,
            target: EndpointTarget {
                origin: provider.origin,
                path,
            },
            protocol: GenerationProtocol::AnthropicMessages,
            upstream_model: self.upstream.into(),
            canonical_model: ModelId::new(self.model).expect("static model"),
            representation: representation.into(),
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
    pub fn route(&self) -> Route {
        Route {
            id: RouteId::new("opencode-go-haiku-native").expect("static route"),
            endpoints: vec![self.endpoint().id],
            policy: RoutePolicy {
                ..RoutePolicy::default()
            },
        }
    }
    pub(super) fn public_contract(&self) -> Dispositions {
        let mut contract = Profile::AdaptiveTextTools.semantic_contract();
        // Accepted cross-protocol projection, not native Messages carriers.
        contract.set_native(
            crate::semantic::task::generation::Feature::Temperature,
            true,
        );
        contract.set_native(crate::semantic::task::generation::Feature::TopP, true);
        contract
    }
    pub fn public_model(&self) -> PublicModel {
        let model = ModelId::new(self.model).expect("static model");
        PublicModel::new(
            model.clone(),
            model,
            self.route().id,
            self.public_contract(),
        )
    }
    pub fn canonical_model(&self) -> CanonicalModel {
        let mut contract = haiku_contract(Profile::AdaptiveTextTools.semantic_contract());
        contract.set_native(
            crate::semantic::task::generation::Feature::Temperature,
            true,
        );
        contract.set_native(crate::semantic::task::generation::Feature::TopP, true);
        contract.set_native(
            crate::semantic::task::generation::Feature::ExplicitCache,
            true,
        );
        CanonicalModel {
            id: ModelId::new(self.model).expect("static model"),
            contract,
        }
    }
}
