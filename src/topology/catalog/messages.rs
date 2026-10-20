//! Native Messages bindings. Directory bootstrap activates one only when `models` lists it.
use crate::{
    protocol::anthropic::messages::{Carriers, Dialect, Profile, TargetCaps},
    provider::{CredentialBindingId, CredentialKind, ProviderDefinition, catalog},
    semantic::task::generation::GenerationSemanticContract,
    topology::*,
};
pub struct MessagesBinding {
    pub model: &'static str,
    pub upstream: &'static str,
    pub credential: &'static str,
    pub released_at: u64,
    pub developer: &'static str,
}
/// Selected development target; registration is not account or inference evidence.
/// Sources: https://opencode.ai/docs/go/ and the pinned route in the reference guide.
pub const MESSAGES_BINDINGS: &[MessagesBinding] = &[MessagesBinding {
    model: "claude-haiku-5-5",
    upstream: "claude-haiku-5-5",
    credential: "opencode-go-api-key",
    // Source: https://www.anthropic.com/claude-haiku-5-5 (2026-10-07).
    released_at: 1_791_331_200,
    developer: "Anthropic",
}];
/// pi's built-in Haiku profile: strict tools, mid-conversation system messages and
/// ordered tool changes.
fn haiku_contract(mut contract: GenerationSemanticContract) -> GenerationSemanticContract {
    contract.strict_tools = true;
    contract.ordered_instructions = true;
    contract.tool_changes = true;
    contract
}
impl MessagesBinding {
    pub fn provider(&self) -> ProviderDefinition {
        catalog::opencode_go()
    }
    pub fn metadata(&self) -> ModelMetadata {
        ModelMetadata::new(self.released_at, self.developer).expect("static publication")
    }
    pub fn endpoint(&self) -> Endpoint {
        let provider = self.provider();
        let mut representation = TargetCaps::new(Profile::AdaptiveTextTools, Dialect::OpenCodeGo);
        // Preserve the pinned OpenCode Messages fallback as binding policy.
        // https://github.com/anomalyco/opencode/blob/e5a557efeee02c94486d391afc48f24a49ebc9a5/packages/ai/src/protocols/anthropic-messages.ts
        representation.default_output_tokens = Some(32_000);
        representation.sampling = crate::protocol::anthropic::messages::SamplingPolicy::Fixed;
        representation.manual_thinking = false;
        representation.automatic_cache_default =
            Some(crate::semantic::task::generation::PrefixCacheTtl::FiveMinutes);
        // Haiku supports owner-local 5m/1h markers; the pinned Go bridge preserves them.
        // This does not admit request-wide automatic caching or certify live cache use.
        representation.semantics.explicit_cache = true;
        representation.semantics = haiku_contract(representation.semantics);
        representation.carriers = Carriers::all();
        Endpoint {
            id: EndpointId::new("opencode-go-haiku-messages").expect("static id"),
            provider: provider.id,
            target: EndpointTarget {
                origin: provider.origin,
                path: provider.messages.expect("declared path"),
            },
            task: TaskKind::Generation,
            protocol: ProtocolProfile::AnthropicMessages,
            upstream_model: self.upstream.into(),
            canonical_model: ModelId::new(self.model).expect("static model"),
            representation: representation.into(),
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
            id: RouteId::new("opencode-go-haiku-native").expect("static route"),
            task: TaskKind::Generation,
            endpoints: vec![self.endpoint().id],
            policy: RoutePolicy {
                max_attempts: 1,
                ..RoutePolicy::default()
            },
        }
    }
    pub fn public_model(&self) -> PublicModel {
        let model = ModelId::new(self.model).expect("static model");
        let mut public = PublicModel::new(
            model.clone(),
            model,
            TaskKind::Generation,
            self.route().id,
            Profile::AdaptiveTextTools.semantic_contract(),
        );
        public.standard_context = false;
        public
    }
    pub fn canonical_model(&self) -> CanonicalModel {
        let mut contract = haiku_contract(Profile::AdaptiveTextTools.semantic_contract());
        contract.explicit_cache = true;
        CanonicalModel {
            id: ModelId::new(self.model).expect("static model"),
            task: TaskKind::Generation,
            contract,
        }
    }
}
