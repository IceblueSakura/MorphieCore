//! Deployment bindings, not model discovery. Aliases and profiles are explicit.
use crate::{
    adapter::Dialect,
    provider::{ProviderDefinition, catalog},
    topology::ProtocolProfile,
};
#[derive(Clone, Copy)]
pub enum RequestSlice {
    Canonical,
    RestrictedLuna,
    OpenCodeHy4,
}
impl RequestSlice {
    pub fn apply(
        self,
        mut contract: crate::semantic::task::generation::GenerationSemanticContract,
    ) -> crate::semantic::task::generation::GenerationSemanticContract {
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
    pub credential: &'static str,
    pub protocols: &'static [ProtocolProfile],
    pub replay_responses: bool,
    pub replay_chat: bool,
    pub public_slice: RequestSlice,
    pub endpoint_slice: RequestSlice,
}
const BOTH: &[ProtocolProfile] = &[
    ProtocolProfile::OpenAiResponses,
    ProtocolProfile::OpenAiChat,
];
const CHAT: &[ProtocolProfile] = &[ProtocolProfile::OpenAiChat];
/// Only declared protocol members are activated by the environment bootstrap.
pub const API_KEY_BINDINGS: &[ApiKeyBinding] = &[
    ApiKeyBinding {
        provider: catalog::deepseek,
        dialect: Dialect::DeepSeek,
        endpoint_prefix: "deepseek",
        model: "deepseek-flash",
        canonical_model: "deepseek-flash",
        upstream: "deepseek-flash",
        credential: "deepseek-api-key",
        protocols: BOTH,
        replay_responses: true,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    ApiKeyBinding {
        provider: catalog::xiaomi,
        dialect: Dialect::Xiaomi,
        endpoint_prefix: "xiaomi",
        model: "mimo-v2.6-pro",
        canonical_model: "mimo-v2.6-pro",
        upstream: "mimo-v2.6-pro",
        credential: "xiaomi-api-key",
        protocols: BOTH,
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    ApiKeyBinding {
        provider: catalog::xiaomi,
        dialect: Dialect::Xiaomi,
        endpoint_prefix: "xiaomi-flash",
        model: "mimo-v2.6-flash",
        canonical_model: "mimo-v2.6-flash",
        upstream: "mimo-v2.6-flash",
        credential: "xiaomi-api-key",
        protocols: BOTH,
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
        model: "gpt-6-luna",
        canonical_model: "gpt-6-luna",
        upstream: "openai/gpt-6-luna",
        credential: "openrouter-api-key",
        protocols: BOTH,
        replay_responses: true,
        replay_chat: true,
        public_slice: RequestSlice::RestrictedLuna,
        endpoint_slice: RequestSlice::RestrictedLuna,
    },
    ApiKeyBinding {
        provider: catalog::longcat,
        dialect: Dialect::LongCat,
        endpoint_prefix: "longcat",
        model: "longcat-2.5-preview",
        canonical_model: "longcat-2.5-preview",
        upstream: "LongCat-2.5-Preview",
        credential: "longcat-api-key",
        protocols: BOTH,
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    ApiKeyBinding {
        provider: catalog::nvidia,
        dialect: Dialect::Nvidia,
        endpoint_prefix: "nvidia",
        model: "nemotron-3-super",
        canonical_model: "nemotron-3-super",
        upstream: "nvidia/nemotron-3-super-120b-a12b",
        credential: "nvidia-api-key",
        protocols: BOTH,
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    ApiKeyBinding {
        provider: catalog::aliyun_dashscope_cn,
        dialect: Dialect::Bailian,
        endpoint_prefix: "aliyun-dashscope-cn",
        model: "qwen3.8-max",
        canonical_model: "qwen3.8-max",
        upstream: "qwen3.8-max",
        credential: "aliyun-dashscope-cn-api-key",
        protocols: BOTH,
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    ApiKeyBinding {
        provider: catalog::aliyun_tokenplan_cn,
        dialect: Dialect::Bailian,
        endpoint_prefix: "aliyun-tokenplan-cn",
        model: "qwen3.8-flash",
        canonical_model: "qwen3.8-flash",
        upstream: "qwen3.8-flash",
        credential: "aliyun-tokenplan-cn-api-key",
        protocols: BOTH,
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    ApiKeyBinding {
        provider: catalog::modelbest,
        dialect: Dialect::ModelBest,
        endpoint_prefix: "modelbest-minicpm5-1b",
        model: "minicpm5-1b",
        canonical_model: "minicpm5-1b",
        upstream: "MiniCPM5-1B",
        credential: "modelbest-api-key",
        protocols: CHAT,
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    ApiKeyBinding {
        provider: catalog::modelbest,
        dialect: Dialect::ModelBest,
        endpoint_prefix: "modelbest-minicpm5-2b",
        model: "minicpm5-2b",
        canonical_model: "minicpm5-2b",
        upstream: "MiniCPM5-2B",
        credential: "modelbest-api-key",
        protocols: CHAT,
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    ApiKeyBinding {
        provider: catalog::modelbest,
        dialect: Dialect::ModelBest,
        endpoint_prefix: "modelbest-minicpm-v-4.6",
        model: "minicpm-v-4.6",
        canonical_model: "minicpm-v-4.6",
        upstream: "MiniCPM-V-4.6",
        credential: "modelbest-api-key",
        protocols: CHAT,
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    ApiKeyBinding {
        provider: catalog::opencode_go,
        dialect: Dialect::OpenCodeGo,
        endpoint_prefix: "opencode-go",
        model: "hy4-preview",
        canonical_model: "hy4-preview",
        upstream: "hy4-preview",
        credential: "opencode-go-api-key",
        protocols: CHAT,
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::OpenCodeHy4,
        endpoint_slice: RequestSlice::OpenCodeHy4,
    },
    ApiKeyBinding {
        provider: catalog::kimi,
        dialect: Dialect::Kimi,
        endpoint_prefix: "kimi",
        model: "kimi-k3",
        canonical_model: "kimi-k3",
        upstream: "kimi-k3",
        credential: "kimi-api-key",
        protocols: CHAT,
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    ApiKeyBinding {
        provider: catalog::zhipu,
        dialect: Dialect::Zhipu,
        endpoint_prefix: "zhipu",
        model: "glm-5.3",
        canonical_model: "glm-5.3",
        upstream: "glm-5.3",
        credential: "zhipu-api-key",
        protocols: BOTH,
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
    ApiKeyBinding {
        provider: catalog::zhipu,
        dialect: Dialect::Zhipu,
        endpoint_prefix: "zhipu-flash",
        model: "glm-5.3-flash",
        canonical_model: "glm-5.3-flash",
        upstream: "glm-5.3-flash",
        credential: "zhipu-api-key",
        protocols: BOTH,
        replay_responses: false,
        replay_chat: false,
        public_slice: RequestSlice::Canonical,
        endpoint_slice: RequestSlice::Canonical,
    },
];
