//! Explicit canonical declarations. Upstream aliases and public routes never create models.
use crate::{
    semantic::task::generation::GenerationSemanticContract,
    topology::{CanonicalModel, ModelId, TaskKind},
};
pub struct ModelDefinition {
    pub id: &'static str,
    pub images: bool,
    pub files: bool,
    pub released_at: u64,
    pub developer: &'static str,
}
// Official publication dates normalized to UTC midnight, not router listing timestamps.
pub const MODELS: &[ModelDefinition] = &[
    // Source: https://openai.com/index/introducing-gpt-6-1-sol/ (2026-09-29).
    ModelDefinition {
        id: "gpt-6.1-sol",
        images: false,
        files: false,
        released_at: 1_790_640_000,
        developer: "OpenAI",
    },
    // Source: https://x.ai/news/grok-4-7 (2026-09-21).
    ModelDefinition {
        id: "grok-4.7",
        images: false,
        files: false,
        released_at: 1_789_948_800,
        developer: "SpaceXAI",
    },
    // The official alias names V4.1 Flash.
    // Source: https://api-docs.deepseek.com/updates/ (2026-09-10).
    ModelDefinition {
        id: "deepseek-flash",
        images: true,
        files: false,
        released_at: 1_788_998_400,
        developer: "DeepSeek",
    },
    // Source: https://mimo.xiaomi.com/mimo-v2-6/article (2026-09-22).
    ModelDefinition {
        id: "mimo-v2.6-pro",
        images: true,
        files: false,
        released_at: 1_790_035_200,
        developer: "Xiaomi",
    },
    // Source: https://mimo.xiaomi.com/mimo-v2-6/article (2026-09-22).
    ModelDefinition {
        id: "mimo-v2.6-flash",
        images: true,
        files: false,
        released_at: 1_790_035_200,
        developer: "Xiaomi",
    },
    // Publication: https://openai.com/index/introducing-gpt-6-sol-and-luna/ (2026-09-22).
    // Admission: https://openrouter.ai/api/v1/models/openai/gpt-6-luna/endpoints
    ModelDefinition {
        id: "gpt-6-luna",
        images: true,
        files: true,
        released_at: 1_790_035_200,
        developer: "OpenAI",
    },
    // Source: https://longcat.chat/platform/docs/zh/change-log (2026-09-25).
    ModelDefinition {
        id: "longcat-2.5-preview",
        images: false,
        files: false,
        released_at: 1_790_294_400,
        developer: "Meituan",
    },
    // Source: https://blogs.nvidia.com/blog/nemotron-3-super-agentic-ai/ (2026-03-11).
    ModelDefinition {
        id: "nemotron-3-super",
        images: false,
        files: false,
        released_at: 1_773_187_200,
        developer: "NVIDIA",
    },
    // Source: https://www.alibabacloud.com/en/press-room/alibaba-unveils-qwen3-8-max (2026-08-03).
    ModelDefinition {
        id: "qwen3.8-max",
        images: true,
        files: false,
        released_at: 1_785_715_200,
        developer: "Alibaba",
    },
    // Source: https://qwen.ai/blog?id=qwen3.8-flash-next (2026-08-26).
    ModelDefinition {
        id: "qwen3.8-flash",
        images: false,
        files: false,
        released_at: 1_787_702_400,
        developer: "Alibaba",
    },
    // Source: https://github.com/OpenBMB/MiniCPM (2026-05-19).
    ModelDefinition {
        id: "minicpm5-1b",
        images: false,
        files: false,
        released_at: 1_779_148_800,
        developer: "ModelBest",
    },
    // Source: https://github.com/OpenBMB/MiniCPM (2026-09-07).
    ModelDefinition {
        id: "minicpm5-2b",
        images: false,
        files: false,
        released_at: 1_788_739_200,
        developer: "ModelBest",
    },
    // Source: https://github.com/OpenBMB/MiniCPM-V (2026-05-11).
    ModelDefinition {
        id: "minicpm-v-4.6",
        images: true,
        files: false,
        released_at: 1_778_457_600,
        developer: "ModelBest",
    },
    // Source: https://www.tencent.com/tencent-releases-and-open-sources-tencent-hy4-preview/ (2026-08-28).
    ModelDefinition {
        id: "hy4-preview",
        images: false,
        files: false,
        released_at: 1_787_875_200,
        developer: "Tencent",
    },
    // Source: https://www.moonshot.ai/ -> https://www.kimi.ai/blog/kimi-k3 (2026-07-16).
    ModelDefinition {
        id: "kimi-k3",
        images: false,
        files: false,
        released_at: 1_784_160_000,
        developer: "Moonshot AI",
    },
    // Source: https://docs.bigmodel.cn/cn/update/new-releases#2026-8-19 (2026-08-19).
    ModelDefinition {
        id: "glm-5.3",
        images: false,
        files: false,
        released_at: 1_787_097_600,
        developer: "Zhipu AI",
    },
    // Source: https://docs.bigmodel.cn/cn/update/new-releases#2026-08-26 (2026-08-26).
    ModelDefinition {
        id: "glm-5.3-flash",
        images: true,
        files: false,
        released_at: 1_787_702_400,
        developer: "Zhipu AI",
    },
];
pub fn contract(id: &str) -> GenerationSemanticContract {
    let definition = MODELS
        .iter()
        .find(|m| m.id == id)
        .expect("explicit canonical reference");
    GenerationSemanticContract {
        image_input: definition.images,
        file_input: definition.files,
        ..GenerationSemanticContract::text_images()
    }
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
