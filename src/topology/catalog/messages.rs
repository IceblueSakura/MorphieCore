//! Native Messages bindings. Directory bootstrap activates one only when `models` lists it.
use crate::{
    protocol::anthropic::{Dialect, Profile, TargetCaps},
    provider::{CredentialBindingId, CredentialKind, ProviderDefinition, catalog},
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
impl MessagesBinding {
    pub fn provider(&self) -> ProviderDefinition {
        catalog::opencode_go()
    }
    pub fn metadata(&self) -> ModelMetadata {
        ModelMetadata::new(self.released_at, self.developer).expect("static publication")
    }
    pub fn endpoint(&self) -> Endpoint {
        let provider = self.provider();
        let representation = TargetCaps::new(Profile::AdaptiveTextTools, Dialect::OpenCodeGo);
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
                request_body_limit: 256 << 10,
                response_body_limit: 2 << 20,
                credential_kind: CredentialKind::ApiKey,
                timeout_ms: 120_000,
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
        CanonicalModel {
            id: ModelId::new(self.model).expect("static model"),
            task: TaskKind::Generation,
            contract: Profile::AdaptiveTextTools.semantic_contract(),
        }
    }
}
