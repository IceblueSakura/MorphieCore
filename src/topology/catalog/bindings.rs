//! Deployment bindings, not model discovery. Aliases and profiles are explicit.
use crate::{
    protocol::{
        openai::Dialect,
        reasoning::{ReasoningEffortMapping, ReasoningHistory},
    },
    provider::{ProviderDefinition, catalog},
    semantic::task::generation::{EffortLevel as E, EffortScope},
    topology::GenerationProtocol,
};
#[derive(Clone, Copy)]
pub enum RequestSlice {
    Canonical,
    RestrictedLuna,
    OpenCodeHy4,
    AudioText,
}
impl RequestSlice {
    pub fn apply(
        self,
        mut contract: crate::semantic::task::generation::GenerationFeatureSet,
    ) -> crate::semantic::task::generation::GenerationFeatureSet {
        if matches!(self, Self::RestrictedLuna) {
            contract.temperature = false;
            contract.top_p = false;
            contract.logprobs = false;
            contract.verbosity = false;
            contract.truncation = false;
            contract.parallel_tool_calls = false;
        }
        if matches!(self, Self::OpenCodeHy4) {
            // A coding-model listing does not establish these optional request controls.
            // Source: https://opencode.ai/docs/go/#endpoints
            contract.structured_output = false;
            // Readable reasoning history remains semantic input; unverified controls are
            // rejected by the target adapter rather than disabling this shared owner.
            contract.logprobs = false;
            contract.verbosity = false;
            contract.truncation = false;
            contract.parallel_tool_calls = false;
        }
        if matches!(self, Self::AudioText) {
            contract.reasoning = false;
            contract.verbosity = false;
            contract.truncation = false;
            contract.structured_output = false;
            contract.logprobs = false;
            contract.parallel_tool_calls = false;
        }
        contract
    }
}
pub struct ApiKeyBinding {
    pub provider: fn() -> ProviderDefinition,
    pub dialect: Dialect,
    pub endpoint_prefix: &'static str,
    /// Downstream label only; clients cannot choose a Provider through it.
    pub model: &'static str,
    /// Shared semantic identity, independent of upstream naming.
    pub canonical_model: &'static str,
    /// Exact model spelling accepted by this Provider, injected during encoding.
    pub upstream: &'static str,
    pub protocols: &'static [GenerationProtocol],
    pub effort_mappings: &'static [(GenerationProtocol, &'static ReasoningEffortMapping)],
    /// Missing protocol members are explicitly unassessed.
    pub reasoning_history: &'static [(GenerationProtocol, ReasoningHistory)],
    pub replay_responses: bool,
    pub replay_chat: bool,
    pub public_slice: RequestSlice,
    pub endpoint_slice: RequestSlice,
}
use super::{GENERATION_REQUEST_BYTES, GENERATION_RESPONSE_BYTES, GENERATION_TIMEOUT_MS, models};
use crate::{
    protocol::{
        conversion::generation::ReportedFactPolicy,
        openai::{GenerationCodec, TargetContract},
    },
    provider::{CredentialBindingId, CredentialKind},
    topology::{
        Endpoint, EndpointId, EndpointTarget, ExecutionContract, ModelId, PublicModel, RouteId,
        TaskKind, TopologyError,
    },
};
impl ApiKeyBinding {
    pub fn credential(&self) -> CredentialBindingId {
        (self.provider)()
            .credential_pool
            .expect("catalog credential pool")
    }
    pub fn public_model(&self) -> PublicModel {
        PublicModel {
            id: ModelId::new(self.model).expect("static id"),
            canonical_model: ModelId::new(self.canonical_model).expect("static id"),
            task: TaskKind::Generation,
            route: RouteId::new(&format!("{}-generation", self.endpoint_prefix))
                .expect("static id"),
            contract: self
                .public_slice
                .apply(models::contract(self.canonical_model)),
            standard_context: false,
            reported_facts: ReportedFactPolicy::Faithful,
        }
    }
    pub fn endpoint(&self, protocol: GenerationProtocol) -> Result<Endpoint, TopologyError> {
        let provider = (self.provider)();
        let (family, path, replay) = match protocol {
            GenerationProtocol::OpenAiChat => (
                crate::protocol::openai::GenerationApi::Chat,
                provider
                    .generation_entry(protocol.operation())
                    .expect("declared entry")
                    .path
                    .clone(),
                self.replay_chat,
            ),
            GenerationProtocol::OpenAiResponses => (
                crate::protocol::openai::GenerationApi::Responses,
                provider
                    .generation_entry(protocol.operation())
                    .expect("declared entry")
                    .path
                    .clone(),
                self.replay_responses,
            ),
            GenerationProtocol::AnthropicMessages
            | GenerationProtocol::GoogleGenerateContent
            | GenerationProtocol::GoogleInteractions => {
                return Err(TopologyError::ContractUnsatisfiable);
            }
        };
        let scope = crate::protocol::replay_envelope::issuer(provider.id.as_str(), self.upstream);
        let mut adapter = GenerationCodec::new(family, self.dialect, Some(scope.clone()));
        // This hosted model's published Chat contract permits one initial string
        // system message. Do not extend it to Responses or other NVIDIA models.
        // https://docs.api.nvidia.com/nim/reference/nvidia-nemotron-3-super-120b-a12b-infer
        if self.dialect == Dialect::Nvidia
            && family == crate::protocol::openai::GenerationApi::Chat
            && self.upstream == "nvidia/nemotron-3-super-120b-a12b"
        {
            adapter.adaptation.rules.system_instruction_messages = true;
            adapter.adaptation.rules.chat_string_instructions = true;
            adapter.adaptation.rules.chat_initial_instruction_only = true;
        }
        adapter.adaptation.rules.responses_lifecycle_reports = self.dialect == Dialect::OpenRouter
            && family == crate::protocol::openai::GenerationApi::Responses
            && self.canonical_model == "gpt-6-luna";
        let mut mappings = self
            .effort_mappings
            .iter()
            .filter(|(member, _)| *member == protocol);
        adapter.adaptation.effort_mapping = mappings.next().map(|(_, mapping)| *mapping);
        if adapter.adaptation.effort_mapping.is_none()
            && self.dialect == crate::protocol::openai::Dialect::Bailian
        {
            adapter.adaptation.effort_mapping =
                crate::protocol::openai::dialect::bailian::effort_mapping(
                    self.canonical_model,
                    family,
                );
        }
        if mappings.next().is_some() {
            return Err(TopologyError::InvalidModelBinding);
        }
        let mut history = self
            .reasoning_history
            .iter()
            .filter(|(member, _)| *member == protocol);
        let reasoning_history = history.next().map(|(_, fact)| *fact);
        if history.next().is_some() {
            return Err(TopologyError::InvalidModelBinding);
        }
        let contract = TargetContract {
            reasoning_history,
            // Keep unassessed TokenPlan admission unchanged during phased migration.
            chat_thinking_output: reasoning_history.is_none()
                && family == crate::protocol::openai::GenerationApi::Chat
                && self.dialect == Dialect::Bailian
                && matches!(self.canonical_model, "qwen3.8-flash" | "qwen3.8-max"),
            semantics: self
                .endpoint_slice
                .apply(models::contract(self.canonical_model)),
            replay_origin: replay.then_some(scope.clone()),
            cache: adapter.adaptation.cache,
            identity_hints: false,
            standard_context: false,
            prediction: false,
            model_limits: models::planning_limits(self.canonical_model, provider.id.as_str()),
            ..TargetContract::full()
        };
        Ok(Endpoint {
            id: self.endpoint_id(protocol),
            canonical_model: ModelId::new(self.canonical_model).expect("static id"),
            provider: provider.id.clone(),
            target: EndpointTarget {
                origin: provider.origin,
                path,
            },
            task: TaskKind::Generation,
            protocol,
            upstream_model: self.upstream.into(),
            representation: adapter.contract(&contract).into(),
            execution: ExecutionContract {
                streaming: true,
                retry_before_commit: false,
                request_body_limit: GENERATION_REQUEST_BYTES,
                response_body_limit: GENERATION_RESPONSE_BYTES,
                credential_kind: CredentialKind::ApiKey,
                timeout_ms: GENERATION_TIMEOUT_MS,
            },
            credential: self.credential(),
        })
    }
    pub fn endpoint_id(&self, protocol: GenerationProtocol) -> EndpointId {
        let suffix = match protocol {
            GenerationProtocol::OpenAiChat => "chat",
            GenerationProtocol::OpenAiResponses => "responses",
            GenerationProtocol::AnthropicMessages => "messages",
            GenerationProtocol::GoogleGenerateContent => "generate-content",
            GenerationProtocol::GoogleInteractions => "interactions",
        };
        EndpointId::new(&format!("{}-{suffix}", self.endpoint_prefix)).expect("static id")
    }
}
const BOTH: &[GenerationProtocol] = &[
    GenerationProtocol::OpenAiResponses,
    GenerationProtocol::OpenAiChat,
];
const CHAT: &[GenerationProtocol] = &[GenerationProtocol::OpenAiChat];
const RESPONSES: &[GenerationProtocol] = &[GenerationProtocol::OpenAiResponses];
// Endpoint-qualified facts; carrier support and history scope remain separate.
// Source: docs/refs/2026-10-27-client-ir-boundaries.md#binding-history-source-review-2026-10-28.
const ACCEPTED_HISTORY: &[(GenerationProtocol, ReasoningHistory)] = &[
    (GenerationProtocol::OpenAiChat, ReasoningHistory::Accepted),
    (
        GenerationProtocol::OpenAiResponses,
        ReasoningHistory::Accepted,
    ),
];
const TOOL_HISTORY: &[(GenerationProtocol, ReasoningHistory)] = &[
    (
        GenerationProtocol::OpenAiChat,
        ReasoningHistory::RequiredWithTools,
    ),
    (
        GenerationProtocol::OpenAiResponses,
        ReasoningHistory::Accepted,
    ),
];
// Documented wire aliases, not a claim of distinct effective thinking strength.
// https://mimo.mi.com/docs/zh-CN/api/chat/responses
const MIMO_FLASH_RESPONSES_EFFORT: ReasoningEffortMapping = ReasoningEffortMapping {
    canonical_model: "mimo-v2.6-flash",
    rule: "mimo-v2.6-flash.responses.effort.v1",
    scope: EffortScope::Reasoning,
    native_levels: &[E::Minimal, E::Low, E::Medium, E::High, E::XHigh, E::Max],
    encode: &[
        (E::Minimal, E::Low),
        (E::Low, E::Low),
        (E::Medium, E::Medium),
        (E::High, E::High),
        (E::XHigh, E::High),
        (E::Max, E::High),
    ],
    decode: &[
        (E::Minimal, E::Low),
        (E::Low, E::Low),
        (E::Medium, E::Medium),
        (E::High, E::High),
        (E::XHigh, E::High),
        (E::Max, E::High),
    ],
};
// Preserve the previously declared positive calibration, without a Provider-wide
// switch or an off-to-low conversion. Reports use their own explicit direction.
const ZHIPU_GLM_EFFORT: ReasoningEffortMapping = ReasoningEffortMapping {
    canonical_model: "glm-5.3",
    rule: "zhipu.glm-5.3.effort.v1",
    scope: EffortScope::Reasoning,
    native_levels: &[E::Low, E::High, E::Max],
    encode: &[
        (E::Minimal, E::Low),
        (E::Low, E::Low),
        (E::Medium, E::High),
        (E::High, E::High),
        (E::XHigh, E::Max),
        (E::Max, E::Max),
    ],
    decode: &[(E::Low, E::Low), (E::High, E::High), (E::Max, E::Max)],
};
const ZHIPU_GLM_FLASH_EFFORT: ReasoningEffortMapping = ReasoningEffortMapping {
    canonical_model: "glm-5.3-flash",
    rule: "zhipu.glm-5.3-flash.effort.v1",
    ..ZHIPU_GLM_EFFORT
};
/// Only declared protocol members are activated by the environment bootstrap.
pub const API_KEY_BINDINGS: &[ApiKeyBinding] = &[
    ApiKeyBinding {
        provider: catalog::xiaomi,
        dialect: Dialect::Xiaomi,
        endpoint_prefix: "xiaomi-ultraspeed",
        effort_mappings: &[],
        model: "mimo-v2.6-pro-ultraspeed",
        canonical_model: "mimo-v2.6-pro-ultraspeed",
        upstream: "mimo-v2.6-pro-ultraspeed",
        protocols: BOTH,
        reasoning_history: ACCEPTED_HISTORY,
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    // https://opencode.ai/docs/go/#endpoints
    ApiKeyBinding {
        provider: catalog::opencode_go,
        dialect: Dialect::OpenCodeGo,
        endpoint_prefix: "opencode-go-muse",
        effort_mappings: &[],
        model: "muse-spark-1.3-contributor",
        canonical_model: "muse-spark-1.3-contributor",
        upstream: "muse-spark-1.3-contributor",
        protocols: RESPONSES,
        reasoning_history: &[],
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::OpenCodeHy4,
        endpoint_slice: RequestSlice::OpenCodeHy4,
    },
    ApiKeyBinding {
        provider: catalog::opencode_go,
        dialect: Dialect::OpenCodeGo,
        endpoint_prefix: "opencode-go-mimo-pro",
        effort_mappings: &[],
        model: "mimo-v2.6-pro",
        canonical_model: "mimo-v2.6-pro",
        upstream: "mimo-v2.6-pro",
        protocols: CHAT,
        reasoning_history: &[],
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::OpenCodeHy4,
    },
    ApiKeyBinding {
        provider: catalog::opencode_go,
        dialect: Dialect::OpenCodeGo,
        endpoint_prefix: "opencode-go-mimo-flash",
        effort_mappings: &[],
        model: "mimo-v2.6-flash",
        canonical_model: "mimo-v2.6-flash",
        upstream: "mimo-v2.6-flash",
        protocols: CHAT,
        reasoning_history: &[],
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::OpenCodeHy4,
    },
    ApiKeyBinding {
        provider: catalog::aliyun_dashscope_cn,
        dialect: Dialect::Bailian,
        endpoint_prefix: "dashscope-flash",
        effort_mappings: &[],
        model: "qwen3.8-flash",
        canonical_model: "qwen3.8-flash",
        upstream: "qwen3.8-flash",
        protocols: BOTH,
        reasoning_history: ACCEPTED_HISTORY,
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    // https://help.aliyun.com/en/model-studio/kimi-k3
    ApiKeyBinding {
        provider: catalog::aliyun_dashscope_cn,
        dialect: Dialect::Bailian,
        endpoint_prefix: "dashscope-kimi",
        effort_mappings: &[],
        model: "kimi-k3",
        canonical_model: "kimi-k3",
        upstream: "kimi-k3",
        protocols: CHAT,
        reasoning_history: &[],
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    ApiKeyBinding {
        provider: catalog::aliyun_dashscope_cn,
        dialect: Dialect::Bailian,
        endpoint_prefix: "dashscope-glm",
        effort_mappings: &[],
        model: "glm-5.3",
        canonical_model: "glm-5.3",
        upstream: "glm-5.3",
        protocols: CHAT,
        reasoning_history: &[],
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    // Prime is an inference mode, not a new canonical model publication.
    // https://help.aliyun.com/en/model-studio/prime-mode
    ApiKeyBinding {
        provider: catalog::aliyun_dashscope_cn,
        dialect: Dialect::Bailian,
        endpoint_prefix: "dashscope-glm-prime",
        effort_mappings: &[],
        model: "glm-5.3-prime",
        canonical_model: "glm-5.3",
        upstream: "glm-5.3-prime",
        protocols: CHAT,
        reasoning_history: &[],
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    ApiKeyBinding {
        provider: catalog::aliyun_tokenplan_cn,
        dialect: Dialect::Bailian,
        endpoint_prefix: "tokenplan-max",
        effort_mappings: &[],
        model: "qwen3.8-max",
        canonical_model: "qwen3.8-max",
        upstream: "qwen3.8-max",
        protocols: BOTH,
        reasoning_history: &[],
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    ApiKeyBinding {
        provider: catalog::aliyun_tokenplan_cn,
        dialect: Dialect::Bailian,
        endpoint_prefix: "tokenplan-glm",
        effort_mappings: &[],
        model: "glm-5.3",
        canonical_model: "glm-5.3",
        upstream: "glm-5.3",
        protocols: CHAT,
        reasoning_history: &[],
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    // https://openrouter.ai/api/v1/models/openai/gpt-audio-mini/endpoints
    ApiKeyBinding {
        provider: catalog::openrouter,
        dialect: Dialect::OpenRouter,
        endpoint_prefix: "openrouter-audio",
        effort_mappings: &[],
        model: "gpt-audio-mini",
        canonical_model: "gpt-audio-mini",
        upstream: "openai/gpt-audio-mini",
        protocols: CHAT,
        reasoning_history: &[],
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::AudioText,
        endpoint_slice: RequestSlice::AudioText,
    },
    ApiKeyBinding {
        provider: catalog::deepseek,
        dialect: Dialect::DeepSeek,
        endpoint_prefix: "deepseek",
        effort_mappings: &[],
        model: "deepseek-flash",
        canonical_model: "deepseek-flash",
        upstream: "deepseek-flash",
        protocols: BOTH,
        reasoning_history: TOOL_HISTORY,
        replay_responses: true,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    ApiKeyBinding {
        provider: catalog::xiaomi,
        dialect: Dialect::Xiaomi,
        endpoint_prefix: "xiaomi",
        effort_mappings: &[],
        model: "mimo-v2.6-pro",
        canonical_model: "mimo-v2.6-pro",
        upstream: "mimo-v2.6-pro",
        protocols: BOTH,
        reasoning_history: ACCEPTED_HISTORY,
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    ApiKeyBinding {
        provider: catalog::xiaomi,
        dialect: Dialect::Xiaomi,
        endpoint_prefix: "xiaomi-flash",
        effort_mappings: &[(
            GenerationProtocol::OpenAiResponses,
            &MIMO_FLASH_RESPONSES_EFFORT,
        )],
        model: "mimo-v2.6-flash",
        canonical_model: "mimo-v2.6-flash",
        upstream: "mimo-v2.6-flash",
        protocols: BOTH,
        reasoning_history: ACCEPTED_HISTORY,
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    // Separate public and endpoint slices preserve the existing admitted contract.
    // Source: https://openrouter.ai/api/v1/models/openai/gpt-6-luna/endpoints
    ApiKeyBinding {
        provider: catalog::openrouter,
        dialect: Dialect::OpenRouter,
        endpoint_prefix: "openrouter",
        effort_mappings: &[],
        model: "gpt-6-luna",
        canonical_model: "gpt-6-luna",
        upstream: "openai/gpt-6-luna",
        protocols: BOTH,
        reasoning_history: &[],
        replay_responses: true,
        replay_chat: true,
        public_slice: RequestSlice::RestrictedLuna,
        endpoint_slice: RequestSlice::RestrictedLuna,
    },
    ApiKeyBinding {
        provider: catalog::nvidia,
        dialect: Dialect::Nvidia,
        endpoint_prefix: "nvidia",
        effort_mappings: &[],
        model: "nemotron-3-super",
        canonical_model: "nemotron-3-super",
        upstream: "nvidia/nemotron-3-super-120b-a12b",
        protocols: BOTH,
        reasoning_history: &[],
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    ApiKeyBinding {
        provider: catalog::aliyun_dashscope_cn,
        dialect: Dialect::Bailian,
        endpoint_prefix: "aliyun-dashscope-cn",
        effort_mappings: &[],
        model: "qwen3.8-max",
        canonical_model: "qwen3.8-max",
        upstream: "qwen3.8-max",
        protocols: BOTH,
        reasoning_history: ACCEPTED_HISTORY,
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    ApiKeyBinding {
        provider: catalog::aliyun_tokenplan_cn,
        dialect: Dialect::Bailian,
        endpoint_prefix: "aliyun-tokenplan-cn",
        effort_mappings: &[],
        model: "qwen3.8-flash",
        canonical_model: "qwen3.8-flash",
        upstream: "qwen3.8-flash",
        protocols: BOTH,
        reasoning_history: &[],
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    ApiKeyBinding {
        provider: catalog::modelbest,
        dialect: Dialect::ModelBest,
        endpoint_prefix: "modelbest-minicpm5-1b",
        effort_mappings: &[],
        model: "minicpm5-1b",
        canonical_model: "minicpm5-1b",
        upstream: "MiniCPM5-1B",
        protocols: CHAT,
        reasoning_history: &[],
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    ApiKeyBinding {
        provider: catalog::modelbest,
        dialect: Dialect::ModelBest,
        endpoint_prefix: "modelbest-minicpm5-2b",
        effort_mappings: &[],
        model: "minicpm5-2b",
        canonical_model: "minicpm5-2b",
        upstream: "MiniCPM5-2B",
        protocols: CHAT,
        reasoning_history: &[],
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    ApiKeyBinding {
        provider: catalog::modelbest,
        dialect: Dialect::ModelBest,
        endpoint_prefix: "modelbest-minicpm-v-4.7-1b",
        effort_mappings: &[],
        model: "MiniCPM-V-4.7-1B",
        canonical_model: "MiniCPM-V-4.7-1B",
        upstream: "MiniCPM-V-4.7-1B",
        protocols: CHAT,
        reasoning_history: &[],
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    ApiKeyBinding {
        provider: catalog::modelbest,
        dialect: Dialect::ModelBest,
        endpoint_prefix: "modelbest-minicpm-v-4.7-35b-a3b",
        effort_mappings: &[],
        model: "MiniCPM-V-4.7-35B-A3B",
        canonical_model: "MiniCPM-V-4.7-35B-A3B",
        upstream: "MiniCPM-V-4.7-35B-A3B",
        protocols: CHAT,
        reasoning_history: &[],
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    ApiKeyBinding {
        provider: catalog::opencode_go,
        dialect: Dialect::OpenCodeGo,
        endpoint_prefix: "opencode-go",
        effort_mappings: &[],
        model: "hy4-preview",
        canonical_model: "hy4-preview",
        upstream: "hy4-preview",
        protocols: CHAT,
        reasoning_history: &[],
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::OpenCodeHy4,
        endpoint_slice: RequestSlice::OpenCodeHy4,
    },
    ApiKeyBinding {
        provider: catalog::zhipu,
        dialect: Dialect::Zhipu,
        endpoint_prefix: "zhipu",
        effort_mappings: &[
            (GenerationProtocol::OpenAiChat, &ZHIPU_GLM_EFFORT),
            (GenerationProtocol::OpenAiResponses, &ZHIPU_GLM_EFFORT),
        ],
        model: "glm-5.3",
        canonical_model: "glm-5.3",
        upstream: "glm-5.3",
        protocols: BOTH,
        reasoning_history: TOOL_HISTORY,
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    // https://openrouter.ai/anthropic/claude-haiku-5.5
    ApiKeyBinding {
        provider: catalog::openrouter,
        dialect: Dialect::OpenRouter,
        endpoint_prefix: "openrouter-haiku",
        effort_mappings: &[],
        model: "claude-haiku-5-5",
        canonical_model: "claude-haiku-5-5",
        upstream: "anthropic/claude-haiku-5.5",
        protocols: CHAT,
        reasoning_history: &[],
        replay_responses: false,
        replay_chat: true,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    ApiKeyBinding {
        provider: catalog::zhipu,
        dialect: Dialect::Zhipu,
        endpoint_prefix: "zhipu-flash",
        effort_mappings: &[
            (GenerationProtocol::OpenAiChat, &ZHIPU_GLM_FLASH_EFFORT),
            (GenerationProtocol::OpenAiResponses, &ZHIPU_GLM_FLASH_EFFORT),
        ],
        model: "glm-5.3-flash",
        canonical_model: "glm-5.3-flash",
        upstream: "glm-5.3-flash",
        protocols: BOTH,
        reasoning_history: TOOL_HISTORY,
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    // Failover alternative for glm-5.3-flash, and its Responses video carrier.
    // Source: https://openrouter.ai/api/v1/models/z-ai/glm-5.3-flash/endpoints
    ApiKeyBinding {
        provider: catalog::openrouter,
        dialect: Dialect::OpenRouter,
        endpoint_prefix: "openrouter-glm-5.3-flash",
        effort_mappings: &[],
        model: "glm-5.3-flash",
        canonical_model: "glm-5.3-flash",
        upstream: "z-ai/glm-5.3-flash",
        protocols: BOTH,
        reasoning_history: &[],
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
];
