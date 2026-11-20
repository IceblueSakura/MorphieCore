//! Qualified native request facts for exact Bailian model spellings.
//! Bound endpoints and direct native codecs share these declarations.
//! Evidence: docs/refs/2026-10-19-bailian-compatible-requests.md and
//! docs/refs/2026-11-13-model-catalog-expansion.md.
use crate::{
    lowering::capabilities::{ModelRequestPolicy, SamplingValues},
    openai::GenerationApi,
    semantic::task::generation::{EffortLevel, EffortScope},
};

use crate::reasoning::ReasoningEffortMapping;
const QWEN_FLASH: ReasoningEffortMapping = ReasoningEffortMapping {
    canonical_model: "qwen3.8-flash",
    rule: "bailian.qwen3.8-flash.effort.v1",
    scope: EffortScope::Reasoning,
    native_levels: &[EffortLevel::Low, EffortLevel::Medium, EffortLevel::XHigh],
    encode: &[
        (EffortLevel::Minimal, EffortLevel::Low),
        (EffortLevel::Low, EffortLevel::Low),
        (EffortLevel::Medium, EffortLevel::Medium),
        (EffortLevel::High, EffortLevel::XHigh),
        (EffortLevel::XHigh, EffortLevel::XHigh),
        (EffortLevel::Max, EffortLevel::XHigh),
    ],
    decode: &[
        (EffortLevel::Low, EffortLevel::Low),
        (EffortLevel::Medium, EffortLevel::Medium),
        (EffortLevel::XHigh, EffortLevel::XHigh),
    ],
};
const QWEN_MAX: ReasoningEffortMapping = ReasoningEffortMapping {
    canonical_model: "qwen3.8-max",
    rule: "bailian.qwen3.8-max.effort.v1",
    ..QWEN_FLASH
};
const GLM_CHAT: ReasoningEffortMapping = ReasoningEffortMapping {
    canonical_model: "glm-5.3",
    rule: "bailian.glm-5.3.chat.effort.v1",
    scope: EffortScope::Reasoning,
    native_levels: &[EffortLevel::Low, EffortLevel::High, EffortLevel::Max],
    encode: &[
        (EffortLevel::Low, EffortLevel::Low),
        (EffortLevel::High, EffortLevel::High),
        (EffortLevel::Max, EffortLevel::Max),
    ],
    decode: &[
        (EffortLevel::Low, EffortLevel::Low),
        (EffortLevel::High, EffortLevel::High),
        (EffortLevel::Max, EffortLevel::Max),
    ],
};
const KIMI_CHAT: ReasoningEffortMapping = ReasoningEffortMapping {
    canonical_model: "kimi-k3.1-pro",
    rule: "bailian.kimi-k3.1-pro.chat.effort.v1",
    scope: EffortScope::Reasoning,
    native_levels: &[
        EffortLevel::Low,
        EffortLevel::Medium,
        EffortLevel::High,
        EffortLevel::Max,
    ],
    encode: &[
        (EffortLevel::Minimal, EffortLevel::Low),
        (EffortLevel::Low, EffortLevel::Low),
        (EffortLevel::Medium, EffortLevel::Medium),
        (EffortLevel::High, EffortLevel::High),
        (EffortLevel::XHigh, EffortLevel::Max),
        (EffortLevel::Max, EffortLevel::Max),
    ],
    decode: &[
        (EffortLevel::Low, EffortLevel::Low),
        (EffortLevel::Medium, EffortLevel::Medium),
        (EffortLevel::High, EffortLevel::High),
        (EffortLevel::Max, EffortLevel::Max),
    ],
};
const QWEN4_MAX: ReasoningEffortMapping = ReasoningEffortMapping {
    canonical_model: "qwen4-max",
    rule: "bailian.qwen4-max.effort.v1",
    scope: EffortScope::Reasoning,
    native_levels: &[EffortLevel::Low, EffortLevel::High, EffortLevel::XHigh],
    encode: &[
        (EffortLevel::Minimal, EffortLevel::Low),
        (EffortLevel::Low, EffortLevel::Low),
        (EffortLevel::Medium, EffortLevel::High),
        (EffortLevel::High, EffortLevel::High),
        (EffortLevel::XHigh, EffortLevel::XHigh),
        (EffortLevel::Max, EffortLevel::XHigh),
    ],
    decode: &[
        (EffortLevel::Low, EffortLevel::Low),
        (EffortLevel::High, EffortLevel::High),
        (EffortLevel::XHigh, EffortLevel::XHigh),
    ],
};
// Native low/high/max controls qualified on both Bailian APIs, 2026-11-07.
const DEEPSEEK: ReasoningEffortMapping = ReasoningEffortMapping {
    canonical_model: "deepseek-flash",
    rule: "bailian.deepseek-flash.effort.v1",
    ..GLM_CHAT
};
const GLM_RESPONSES: ReasoningEffortMapping = ReasoningEffortMapping {
    rule: "bailian.glm-5.3.responses.effort.v1",
    encode: &[
        (EffortLevel::Minimal, EffortLevel::Low),
        (EffortLevel::Low, EffortLevel::Low),
        (EffortLevel::Medium, EffortLevel::High),
        (EffortLevel::High, EffortLevel::High),
        (EffortLevel::XHigh, EffortLevel::Max),
        (EffortLevel::Max, EffortLevel::Max),
    ],
    ..GLM_CHAT
};

#[derive(Clone, Copy)]
struct ApiPolicy {
    effort: Option<&'static ReasoningEffortMapping>,
    total_ceiling: bool,
    text_only: bool,
    reject_required_tools: bool,
    reject_forced_while_thinking: bool,
}
const STANDARD_API: ApiPolicy = ApiPolicy {
    effort: None,
    total_ceiling: false,
    text_only: false,
    reject_required_tools: false,
    reject_forced_while_thinking: false,
};

#[derive(Clone, Copy)]
struct Model {
    chat: ApiPolicy,
    responses: ApiPolicy,
    sampling: Option<SamplingValues>,
    always_thinking: bool,
    reject_budget: bool,
    reject_logprobs: bool,
}
const UNKNOWN: Model = Model {
    chat: STANDARD_API,
    responses: STANDARD_API,
    sampling: None,
    always_thinking: false,
    reject_budget: false,
    reject_logprobs: false,
};
const QWEN: Model = Model {
    chat: ApiPolicy {
        effort: Some(&QWEN_FLASH),
        reject_required_tools: true,
        ..STANDARD_API
    },
    responses: ApiPolicy {
        effort: Some(&QWEN_FLASH),
        total_ceiling: true,
        reject_forced_while_thinking: true,
        ..STANDARD_API
    },
    sampling: SamplingValues::minimum_temperature_while_thinking(0.6),
    reject_logprobs: true,
    ..UNKNOWN
};
const GLM: Model = Model {
    chat: ApiPolicy {
        effort: Some(&GLM_CHAT),
        total_ceiling: true,
        text_only: true,
        ..STANDARD_API
    },
    responses: ApiPolicy {
        effort: Some(&GLM_RESPONSES),
        reject_forced_while_thinking: true,
        ..STANDARD_API
    },
    always_thinking: true,
    reject_budget: true,
    reject_logprobs: true,
    ..UNKNOWN
};
fn model(name: &str) -> Model {
    match name {
        "qwen3.8-flash" => QWEN,
        "qwen3.8-max" => Model {
            chat: ApiPolicy {
                effort: Some(&QWEN_MAX),
                ..QWEN.chat
            },
            responses: ApiPolicy {
                effort: Some(&QWEN_MAX),
                reject_forced_while_thinking: false,
                ..QWEN.responses
            },
            ..QWEN
        },
        "qwen4-max" => {
            let api = ApiPolicy {
                effort: Some(&QWEN4_MAX),
                total_ceiling: true,
                ..STANDARD_API
            };
            Model {
                chat: api,
                responses: api,
                ..UNKNOWN
            }
        }
        "glm-5.3" => GLM,
        "glm-5.3-prime" => Model {
            responses: ApiPolicy {
                reject_forced_while_thinking: false,
                ..GLM.responses
            },
            ..GLM
        },
        "kimi-k3.1-pro" => Model {
            chat: ApiPolicy {
                effort: Some(&KIMI_CHAT),
                ..STANDARD_API
            },
            sampling: SamplingValues::fixed(1.0, 0.95, 0.0),
            reject_budget: true,
            reject_logprobs: true,
            ..UNKNOWN
        },
        "deepseek-flash" | "deepseek-v4.1-flash-1017" => {
            let api = ApiPolicy {
                effort: Some(&DEEPSEEK),
                ..STANDARD_API
            };
            Model {
                chat: api,
                responses: api,
                ..UNKNOWN
            }
        }
        _ => UNKNOWN,
    }
}
impl Model {
    fn api(self, profile: GenerationApi) -> ApiPolicy {
        match profile {
            GenerationApi::Chat => self.chat,
            GenerationApi::Responses => self.responses,
        }
    }
}

pub fn effort_mapping(
    name: &str,
    profile: GenerationApi,
) -> Option<&'static ReasoningEffortMapping> {
    model(name).api(profile).effort
}

pub(super) fn total_output_ceiling(name: &str, profile: GenerationApi) -> bool {
    model(name).api(profile).total_ceiling
}

pub(crate) fn request_policy(name: &str, profile: GenerationApi) -> ModelRequestPolicy {
    let model = model(name);
    let api = model.api(profile);
    ModelRequestPolicy {
        reject_reasoning_off: model.always_thinking,
        text_output_only: api.text_only,
        reject_required_tools: api.reject_required_tools,
        reject_forced_tools_while_thinking: api.reject_forced_while_thinking,
        reply_only_ceiling_while_thinking: profile == GenerationApi::Responses
            && !name.is_empty()
            && !api.total_ceiling,
        function_strictness: Some(function_strictness(name, profile)),
    }
}

pub(crate) fn rejects_budget(name: &str) -> bool {
    model(name).reject_budget
}
pub(super) fn rejects_logprobs(name: &str) -> bool {
    model(name).reject_logprobs
}
pub(crate) fn sampling_values(name: &str) -> SamplingValues {
    model(name).sampling.unwrap_or_default()
}

// Qualification is specific to the shared service, exact model and API.
// Canonical model flags and successful ordinary calls are not enforcement evidence.
// docs/refs/2026-10-19-bailian-compatible-requests.md
const VERIFIED_STRICT_TOOLS: &[(&str, GenerationApi)] = &[];

pub fn function_strictness(
    model: &str,
    profile: GenerationApi,
) -> crate::lowering::disposition::Disposition {
    use crate::lowering::disposition::{BAILIAN_RELAX_STRICTNESS, Disposition};
    if VERIFIED_STRICT_TOOLS.contains(&(model, profile)) {
        Disposition::Native
    } else {
        Disposition::Approximate(BAILIAN_RELAX_STRICTNESS)
    }
}
