# Generation projection contracts

Researched **2026-10-15** against official protocol specifications, SDK sources, and model documentation. [Architecture](../architecture.md) owns accepted canonical design, and [Roadmap](../roadmap.md) owns execution gates.

## Contents

- [Sources and version boundaries](#sources-and-version-boundaries)
- [Protocol baselines and explicit extensions](#protocol-baselines-and-explicit-extensions)
  - [Cache count aliases](#cache-count-aliases)
  - [Readable and structured reasoning](#readable-and-structured-reasoning)
- [OpenAI parameter and content boundaries](#openai-parameter-and-content-boundaries)
  - [Output limits](#output-limits)
  - [Function tools and call history](#function-tools-and-call-history)
  - [Prediction](#prediction-is-an-optimization-not-an-output-constraint)
  - [Cache options and breakpoints](#cache-options-breakpoints-and-usage)
- [Thinking activation, effort, budgets and display](#thinking-activation-effort-budgets-and-display)
  - [Anthropic activation and effort](#anthropic-has-independent-activation-and-effort)
  - [Google carriers](#google-carriers-differ)
  - [Cross-protocol activation and effort](#cross-protocol-activation-and-effort)
  - [Hand-authored level calibration](#hand-authored-same-model-level-calibration)
  - [Pi thinking-budget presets](#pi-thinking-budget-presets)
- [Sampling, schemas and service policy](#sampling-schemas-and-service-policy)
  - [Sampling declarations](#sampling-declarations-are-not-model-guarantees)
  - [Function schemas and choices](#function-schemas-and-choices)
  - [Service tiers](#service-tiers-are-official-but-not-universally-equivalent)

## Sources and version boundaries

- **OpenAI**: Python SDK (Chat and Responses separate types).
- **Anthropic**: Python SDK 2.1.0 (Messages thinking, effort, cache control).
- **Google**: Gen AI SDK 2.33.0 & Gemini v1beta Discovery (`20261012`).
- **DeepSeek**: Thinking mode, cache token aliases.
- **OpenRouter**: Reasoning tokens and routing approximations.
- **MiMo**: Effort aliases and thinking strength controls.

## Protocol baselines and explicit extensions

Chat, Responses, Messages, and Gemini are independent protocol baselines, not interchangeable dialects. Provider spellings normalize before semantic IR decoding.

### Cache count aliases

DeepSeek defines `prompt_cache_hit_tokens` and `prompt_cache_miss_tokens`. In MorphieCore:
- Standard OpenAI profiles reject or ignore undocumented alias keys.
- Registered dialect profiles validate that alias counts agree with canonical `prompt_tokens_details.cached_tokens`.

### Readable and structured reasoning

Official Chat schemas omit `reasoning_content` or `reasoning_details`. DeepSeek and OpenRouter provide extension contracts.
- MorphieCore preserves registered plaintext reasoning extensions without confusing them with ordinary assistant speech.
- Unknown reasoning fields under standard profiles are ignored strictly.

## OpenAI parameter and content boundaries

### Output limits

- Chat: `max_completion_tokens` (total output) and deprecated `max_tokens`.
- Responses: `max_output_tokens`. Singular `max_completion_token` is an invalid alias.

### Function tools and call history

- Chat: `name`, `description`, `parameters`, and `strict`.
- Responses: Adds dispatch controls and `output_schema`.
- Chat custom tools use nested custom definitions; unsupported tools reject at intake.

### Prediction is an optimization, not an output constraint

Chat `prediction` provides predicted content to reduce first-token latency. Omitting prediction does not authorize constraining generated text; MorphieCore treats it as a performance optimization and drops it safely under declared rules when unsupported.

### Cache options, breakpoints and usage

- `PromptCacheOptions`: `mode` and `ttl` are optional members. Empty objects `{}` must not invent synthetic defaults.
- Breakpoints: Chat `prompt_cache_breakpoint: {"mode":"explicit"}` marks exact prefix boundaries, inheriting TTL from request options.

## Thinking activation, effort, budgets and display

### Anthropic has independent activation and effort

- Thinking variants: `disabled`, `enabled` (with `budget_tokens >= 1024`), and `adaptive`.
- Effort: `output_config.effort` (`low`, `medium`, `high`) is independent of thinking activation and affects response text and tool calls even when thinking is disabled.

### Google carriers differ

- `generateContent`: `thinkingBudget` (`0` = off, `-1` = dynamic) and `thinkingLevel`.
- `Interactions`: `thinking_level` (`minimal`, `low`, `medium`, `high`) and summary controls. `minimal` is low reasoning, not guaranteed disabled.

### Cross-protocol activation and effort

| Protocol | Activation / Off representation | Strength control | Independent off + effort? |
|---|---|---|---|
| OpenAI Chat | `reasoning_effort: none` | Same enum | No (single field) |
| OpenAI Responses | `reasoning.effort: none` | `effort` + `mode` | No |
| Google generateContent | `thinkingBudget: 0` | Positive budget or level | No |
| Google Interactions | No documented off member | `thinking_level` | No |
| Anthropic Messages | `thinking: {type: "disabled"}` | `output_config.effort` | **Yes** (independent controls) |

MorphieCore models thinking activation and scope-qualified effort separately in IR to preserve official distinctions.

### Hand-authored same-model level calibration

MiMo aliases `minimal → low` and `xhigh/max/ultra → high`. Different providers expose different levels; hand-authored target policies handle conversions explicitly without inventing global equivalences.

### Pi thinking-budget presets

Pi's options helper (`4a5d372`) maps qualitative levels to token allocations:

| Pi level | Shared default thinking tokens |
|---|---|
| minimal | 1,024 |
| low | 2,048 |
| medium | 8,192 |
| high | 16,384 |

MorphieCore adopts a fine-grained six-budget quantization scheme (`minimal=1024`, `low=2048`, `medium=8192`, `high=16384`, `xhigh=32768`, `max=65536`) with nearest-distance matching and ties resolving upward. Budgets exceeding client-owned output ceilings reject fail-closed.

## Sampling, schemas and service policy

### Sampling declarations are not model guarantees

- Interactions v1 deprecates `temperature` and `top_p`; newer Gemini models ignore them.
- Anthropic Messages removes sampling parameters in API version `2026-10-21`.
- Sampling seed is a best-effort control, not a cross-model determinism promise.

### Function schemas and choices

- Google distinguishes `parameters` (OpenAPI subset) from `parametersJsonSchema`.
- Tool choices `AUTO`, `ANY`, `NONE`, `VALIDATED` in Google APIs define constrained decoding, which is distinct from OpenAI schema normalization.

### Service tiers are official but not universally equivalent

- OpenAI: `auto`, `default`, `flex`, `priority`.
- Anthropic: `auto`, `standard_only`.
- Google: `unspecified`, `standard`, `flex`, `priority`.
Names do not guarantee equivalent latency, billing, or priority SLAs across providers.
