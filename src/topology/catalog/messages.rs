//! Explicit native library bindings; never activate the public OpenAI bootstrap.
use crate::{
    protocol::anthropic::{Dialect, Profile, TargetCaps},
    provider::{CredentialBindingId, CredentialKind, ProviderDefinition, catalog},
    topology::*,
};
pub struct MessagesBinding {
    pub model: &'static str,
    pub upstream: &'static str,
    pub credential: &'static str,
}
/// Selected development target; registration is not account or inference evidence.
/// Sources: https://opencode.ai/docs/go/ and the pinned route in the reference guide.
pub const MESSAGES_BINDINGS: &[MessagesBinding] = &[MessagesBinding {
    model: "claude-haiku-5-5",
    upstream: "claude-haiku-5-5",
    credential: "opencode-go-api-key",
}];
impl MessagesBinding {
    pub fn provider(&self) -> ProviderDefinition {
        catalog::opencode_go()
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
    pub fn topology(&self) -> Result<CompiledTopology, TopologyError> {
        let endpoint = self.endpoint();
        let route = Route {
            id: RouteId::new("opencode-go-haiku-native").expect("static route"),
            task: TaskKind::Generation,
            endpoints: vec![endpoint.id.clone()],
            policy: RoutePolicy {
                max_attempts: 1,
                ..RoutePolicy::default()
            },
        };
        let mut public = PublicModel::new(
            endpoint.canonical_model.clone(),
            endpoint.canonical_model.clone(),
            TaskKind::Generation,
            route.id.clone(),
            Profile::AdaptiveTextTools.semantic_contract(),
        );
        public.standard_context = false;
        compile(
            vec![self.provider()],
            vec![endpoint],
            vec![route],
            vec![public],
            vec![CanonicalModel {
                id: ModelId::new(self.model).expect("static model"),
                task: TaskKind::Generation,
                contract: Profile::AdaptiveTextTools.semantic_contract(),
            }],
        )
    }
}
