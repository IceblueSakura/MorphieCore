//! Explicit canonical declarations. Upstream aliases and public routes never create models.
use crate::{
    lowering::disposition::Dispositions,
    topology::{CanonicalModel, ModelId, TaskKind},
};
pub struct ModelDefinition {
    pub id: &'static str,
    pub images: bool,
    pub files: bool,
    pub strict_tools: bool,
    pub released_at: u64,
    pub developer: &'static str,
}
// Official publication dates normalized to UTC midnight, not router listing timestamps.
pub const MODELS: &[ModelDefinition] = &[
    // https://openai.com/index/safety-overview-gpt-6-astra/ (2026-09-03).
    ModelDefinition {
        id: "gpt-6-astra",
        images: true,
        files: true,
        strict_tools: true,
        released_at: 1_788_393_600,
        developer: "OpenAI",
    },
    // https://mimo.mi.com/docs/updates/model (2026-09-22).
    ModelDefinition {
        id: "mimo-v2.6-pro-ultraspeed",
        images: true,
        files: false,
        strict_tools: false,
        released_at: 1_790_035_200,
        developer: "Xiaomi",
    },
    // Family publication, not Contributor-tier listing time.
    // https://research.meta.ai/blog/introducing-muse-spark-1-3 (2026-09-02).
    ModelDefinition {
        id: "muse-spark-1.3-contributor",
        images: true,
        files: false,
        strict_tools: false,
        released_at: 1_788_307_200,
        developer: "Meta",
    },
    // Original publication, not the later announcement update.
    // https://www.kimi.com/en/blog/ (2026-07-16).
    ModelDefinition {
        id: "kimi-k3",
        images: true,
        files: false,
        strict_tools: false,
        released_at: 1_784_160_000,
        developer: "Moonshot AI",
    },
    // https://platform.openai.com/docs/changelog (2025-10-06 release).
    ModelDefinition {
        id: "gpt-audio-mini",
        images: false,
        files: false,
        strict_tools: false,
        released_at: 1_759_708_800,
        developer: "OpenAI",
    },
    // Source: https://openai.com/index/introducing-gpt-6-1-sol/ (2026-09-29).
    ModelDefinition {
        id: "gpt-6.1-sol",
        images: true,
        files: false,
        strict_tools: true,
        released_at: 1_790_640_000,
        developer: "OpenAI",
    },
    // Source: https://x.ai/news/grok-4-7 (2026-09-21).
    // Image input: https://docs.x.ai/developers/grok-4-7
    ModelDefinition {
        id: "grok-4.7",
        images: true,
        files: false,
        strict_tools: false,
        released_at: 1_789_948_800,
        developer: "SpaceXAI",
    },
    // The official alias names V4.1 Flash.
    // Source: https://api-docs.deepseek.com/updates/ (2026-09-10).
    // Strict tool schemas are validated by the beta endpoint binding, not the
    // standard endpoint. Source: https://api-docs.deepseek.com/guides/tool_calls/
    ModelDefinition {
        id: "deepseek-flash",
        images: true,
        files: false,
        strict_tools: true,
        released_at: 1_788_998_400,
        developer: "DeepSeek",
    },
    // Source: https://mimo.xiaomi.com/mimo-v2-6/article (2026-09-22).
    ModelDefinition {
        id: "mimo-v2.6-pro",
        images: true,
        files: false,
        strict_tools: false,
        released_at: 1_790_035_200,
        developer: "Xiaomi",
    },
    // Source: https://mimo.xiaomi.com/mimo-v2-6/article (2026-09-22).
    ModelDefinition {
        id: "mimo-v2.6-flash",
        images: true,
        files: false,
        strict_tools: false,
        released_at: 1_790_035_200,
        developer: "Xiaomi",
    },
    // Publication: https://openai.com/index/introducing-gpt-6-sol-and-luna/ (2026-09-22).
    // Native function strictness uses the official OpenAI carrier; endpoint
    // inventories must not impose another Provider's assessment on this model.
    ModelDefinition {
        id: "gpt-6-luna",
        images: true,
        files: true,
        strict_tools: true,
        released_at: 1_790_035_200,
        developer: "OpenAI",
    },
    // Source: https://blogs.nvidia.com/blog/nemotron-3-super-agentic-ai/ (2026-03-11).
    ModelDefinition {
        id: "nemotron-3-super",
        images: false,
        files: false,
        strict_tools: false,
        released_at: 1_773_187_200,
        developer: "NVIDIA",
    },
    // Source: https://www.alibabacloud.com/en/press-room/alibaba-unveils-qwen3-8-max (2026-08-03).
    ModelDefinition {
        id: "qwen3.8-max",
        images: true,
        files: false,
        strict_tools: true,
        released_at: 1_785_715_200,
        developer: "Alibaba",
    },
    // Source: https://qwen.ai/blog?id=qwen3.8-flash-next (2026-08-26).
    ModelDefinition {
        id: "qwen3.8-flash",
        images: false,
        files: false,
        strict_tools: true,
        released_at: 1_787_702_400,
        developer: "Alibaba",
    },
    // Source: https://github.com/OpenBMB/MiniCPM (2026-05-19).
    ModelDefinition {
        id: "minicpm5-1b",
        images: false,
        files: false,
        strict_tools: false,
        released_at: 1_779_148_800,
        developer: "ModelBest",
    },
    // Source: https://github.com/OpenBMB/MiniCPM (2026-09-07).
    ModelDefinition {
        id: "minicpm5-2b",
        images: false,
        files: false,
        strict_tools: false,
        released_at: 1_788_739_200,
        developer: "ModelBest",
    },
    // Source: https://github.com/OpenBMB/MiniCPM-V (2026-10-12).
    ModelDefinition {
        id: "MiniCPM-V-4.7-1B",
        images: true,
        files: false,
        strict_tools: false,
        released_at: 1_791_763_200,
        developer: "ModelBest",
    },
    // Source: https://github.com/OpenBMB/MiniCPM-V (2026-10-12).
    ModelDefinition {
        id: "MiniCPM-V-4.7-35B-A3B",
        images: true,
        files: false,
        strict_tools: false,
        released_at: 1_791_763_200,
        developer: "ModelBest",
    },
    // Source: https://www.tencent.com/tencent-releases-and-open-sources-tencent-hy4-preview/ (2026-08-28).
    ModelDefinition {
        id: "hy4-preview",
        images: false,
        files: false,
        strict_tools: false,
        released_at: 1_787_875_200,
        developer: "Tencent",
    },
    // Source: https://docs.bigmodel.cn/cn/update/new-releases#2026-8-19 (2026-08-19).
    ModelDefinition {
        id: "glm-5.3",
        images: false,
        files: false,
        strict_tools: false,
        released_at: 1_787_097_600,
        developer: "Zhipu AI",
    },
    // Source: https://docs.bigmodel.cn/cn/update/new-releases#2026-08-26 (2026-08-26).
    ModelDefinition {
        id: "glm-5.3-flash",
        images: true,
        files: false,
        strict_tools: false,
        released_at: 1_787_702_400,
        developer: "Zhipu AI",
    },
];
pub fn contract(id: &str) -> Dispositions {
    if let Some(binding) = super::MESSAGES_BINDINGS.iter().find(|b| b.model == id) {
        // Shared public admission is narrower than the model's native capabilities.
        // Do not copy Messages-only cache support into OpenAI-compatible bindings.
        return binding.public_model().contract;
    }
    let definition = MODELS
        .iter()
        .find(|m| m.id == id)
        .expect("explicit canonical reference");
    (Dispositions::text_images())
        .with_native(
            crate::semantic::task::generation::Feature::CustomTools,
            matches!(id, "gpt-6-astra" | "gpt-6.1-sol" | "gpt-6-luna"),
        )
        .with_native(
            crate::semantic::task::generation::Feature::ImageInput,
            definition.images,
        )
        .with_native(
            crate::semantic::task::generation::Feature::FileInput,
            definition.files,
        )
        .with_native(
            crate::semantic::task::generation::Feature::AudioInput,
            id == "gpt-audio-mini",
        )
        // Sources: MiniCPM-V docs/api.md and the Zhipu Chat reference (docs/refs/multimodal.md).
        .with_native(
            crate::semantic::task::generation::Feature::VideoInput,
            matches!(
                id,
                "MiniCPM-V-4.7-1B" | "MiniCPM-V-4.7-35B-A3B" | "glm-5.3-flash"
            ),
        )
        .with_native(
            crate::semantic::task::generation::Feature::StrictTools,
            definition.strict_tools,
        )
        // Admission still needs the media kind's input. Only Responses carries tool
        // media; standard Chat has no carrier, so Chat targets reject it.
        .with_native(
            crate::semantic::task::generation::Feature::ToolResultMedia,
            true,
        )
}
pub fn canonical_models() -> Vec<CanonicalModel> {
    MODELS
        .iter()
        .map(|m| CanonicalModel {
            id: ModelId::new(m.id).expect("static id"),
            task: TaskKind::Generation,
            contract: contract(m.id),
        })
        .collect()
}

pub(super) fn model_limits(
    model: &str,
    provider: &str,
) -> crate::protocol::model_limits::ModelLimits {
    // Service-qualified declarations and explicitly selected planning policy.
    // See docs/refs/2026-10-26-model-discovery.md.
    let context_window_tokens = match (model, provider) {
        // Operator-selected SIWC planning window matches the public OpenAI API.
        ("gpt-6-astra" | "gpt-6.1-sol" | "gpt-6-luna", "openai" | "chatgpt") => Some(1_050_000),
        ("gpt-6-luna", "openrouter") => Some(1_050_000),
        ("gpt-audio-mini", "openrouter") => Some(128_000),
        ("grok-4.7", "grok") => Some(500_000),
        ("deepseek-flash", "deepseek" | "deepseek-beta") => Some(1_048_576),
        ("claude-haiku-5-5", "opencode-go") => Some(1_000_000),
        ("mimo-v2.6-flash" | "mimo-v2.6-pro" | "muse-spark-1.3-contributor", "opencode-go") => {
            Some(1_048_576)
        }
        ("hy4-preview", "opencode-go") => Some(1_024_000),
        ("minicpm5-1b" | "minicpm5-2b", "modelbest") => Some(128_000),
        ("MiniCPM-V-4.7-1B" | "MiniCPM-V-4.7-35B-A3B", "modelbest") => Some(256_000),
        ("qwen3.8-max" | "qwen3.8-flash", "aliyun-dashscope-cn") => Some(1_000_000),
        ("glm-5.3" | "glm-5.3-prime" | "kimi-k3", "aliyun-dashscope-cn") => Some(1_048_576),
        _ => None,
    };
    let maximum_input_tokens = match (model, provider) {
        ("gpt-6-astra" | "gpt-6.1-sol" | "gpt-6-luna", "openai") => Some(922_000),
        ("glm-5.3" | "glm-5.3-prime" | "kimi-k3", "aliyun-dashscope-cn") => Some(1_048_576),
        _ => None,
    };
    use crate::semantic::task::generation::EffortLevel as E;
    let allowed_effort: Option<Box<[_]>> = match model {
        "grok-4.7" => Some(vec![E::Low, E::Medium, E::High, E::XHigh].into()),
        "gpt-6-astra" | "gpt-6.1-sol" | "gpt-6-luna" => {
            Some(vec![E::Low, E::Medium, E::High, E::XHigh, E::Max].into())
        }
        _ => None,
    };
    crate::protocol::model_limits::ModelLimits {
        reject_disabled_reasoning: matches!(model, "grok-4.7" | "gpt-6-astra" | "gpt-6.1-sol"),
        allowed_effort,
        context_window_tokens,
        maximum_input_tokens,
        ..Default::default()
    }
}
