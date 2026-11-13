# Model capacity and discovery evidence — 2026-10-26

[Architecture](../architecture/adaptation-and-dialects.md#generation-text-model-discovery) owns the selected design. This note records source evidence, not runtime health or authorization to activate tasks.

## Capacity sources

### Service-qualified capacity audit — 2026-11-13

The following exact integer declarations were read from official service sources.
Context is a total window; independent input capacity is never calculated by
subtracting output, and input plus output never creates a new context fact.

| Model / service | Context window | Independent input maximum | Source |
|---|---:|---:|---|
| `gpt-6-astra`, `gpt-6.1-sol`, `gpt-6-luna` / OpenAI API | 1,050,000 | 922,000 | [Astra](https://developers.openai.com/api/docs/models/gpt-6-astra.md), [Sol](https://developers.openai.com/api/docs/models/gpt-6.1-sol.md), [Luna](https://developers.openai.com/api/docs/models/gpt-6-luna.md) |
| `gpt-6.1-sol` / SIWC | 500,000 | Unknown | Loader-mediated SIWC model directory; [service catalog contract](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference.md) |
| `gpt-6-astra`, `gpt-6-luna` / SIWC | 272,000 | Unknown | Same SIWC directory |
| `grok-4.7` / Grok OAuth | 500,000 | Unknown | Loader-mediated subscription model directory, `context_length`; corroborating [native API guide](https://docs.x.ai/developers/grok-4-7.md) |
| `deepseek-v4.1-flash-1017` / native DeepSeek and beta | 1,048,576 | Unknown | Loader-mediated native directory, `context_window`; [model-list definition](https://api-docs.deepseek.com/api/list-models.md), [alias/version relationship](https://api-docs.deepseek.com/quick_start/pricing.md) |
| `claude-haiku-5-5` / OpenCode Go | 1,000,000 | Unknown | [Go service directory](https://opencode.ai/inference/go/v1/models), `limit.context` |
| `mimo-v2.6-flash`, `mimo-v2.6-pro`, `muse-spark-1.3-contributor` / OpenCode Go | 1,048,576 | Unknown | Same Go directory |
| `hy4-preview` / OpenCode Go | 1,024,000 | Unknown | Same Go directory |
| `minicpm5-1b`, `minicpm5-2b` / ModelBest | 128,000 | Unknown | [Hosted service directory](https://platform.modelbest.cn/api/models), `context_window`; [API service](https://github.com/OpenBMB/MiniCPM-V/blob/main/docs/api.md) |
| `MiniCPM-V-4.7-1B`, `MiniCPM-V-4.7-35B-A3B` / ModelBest | 256,000 | Unknown | Same hosted directory |
| `qwen3.8-max`, `qwen3.8-flash` / DashScope | 1,000,000 | Mode-dependent; not seeded | [Max](https://help.aliyun.com/zh/model-studio/qwen3-8-max), [Flash](https://help.aliyun.com/zh/model-studio/qwen3-8-flash) |
| `glm-5.3`, `glm-5.3-prime` / DashScope | 1,048,576 | 1,048,576 | [GLM service table](https://help.aliyun.com/zh/model-studio/glm-5-3); [Prime retains base limits](https://help.aliyun.com/zh/model-studio/prime-mode) |
| `kimi-k3` / DashScope | 1,048,576 | 1,048,576 | [Detailed service limit table](https://help.aliyun.com/zh/model-studio/kimi-k3) |
| `gemini-3.8-flash` / Google | Unknown | 1,048,576 | [Google specification](https://ai.google.dev/gemini-api/docs/models/gemini-3.8-flash); output maximum remains independently 65,536 |
| `gpt-6-luna` / OpenRouter | 1,050,000 | Unknown | [Endpoint inventory](https://openrouter.ai/api/v1/models/openai/gpt-6-luna/endpoints); all listed endpoints agree on context, not input |
| `gpt-audio-mini` / OpenRouter | 128,000 | Unknown | [Endpoint inventory](https://openrouter.ai/api/v1/models/openai/gpt-audio-mini/endpoints) |

SIWC's selected model records also return `max_context_window:872000`. These
directory values remain source observations. The operator selected a fixed
1,050,000-token planning window for Sol, Astra and Luna on SIWC, matching the public
OpenAI API and superseding the earlier use of SIWC's smaller `context_window`
defaults. This is catalog planning policy, not measured SIWC capacity or an
independent SIWC input/output declaration. Discovery remains startup-only and never
performs these network queries; the audit used one bounded model-list request per
credential-backed service and retained only safe capacity fields.

DeepSeek's model-list definition explicitly counts input plus output in the total
and supplies exact integers, resolving the earlier `1M` ambiguity. The public
dated ID still sends the native `deepseek-flash` alias; it is not an immutable
upstream snapshot guarantee. ModelBest's hosted values take precedence over weight
configuration sizes; its `max_output_tokens:0` is not a usable positive maximum.

Unknown service facts stay unknown: TokenPlan Qwen/GLM, native Xiaomi and Zhipu
context abbreviations, NVIDIA Responses context, and heterogeneous OpenRouter
GLM Flash capacities are not filled from another service. In particular,
[GLM Flash router endpoints](https://openrouter.ai/api/v1/models/z-ai/glm-5.3-flash/endpoints)
publish different context values; the largest advertised value is not route-common.
Qwen's input maxima depend on thinking mode. Kimi's introductory “100 万” differs
from its explicit 1,048,576 service table; the latter supplies the binding fact.

Catalog declarations and candidate configuration keep context, input and output
independent. This audit fills context/input metadata, not new output ceilings or
defaults. A common route value requires a comparable declaration on every candidate;
one unknown candidate keeps it null. Trusted deployment configuration may narrow
a known declaration or supply an unknown one. An upstream-model, origin or path
replacement invalidates inherited context/input facts unless explicitly redeclared.

## Installed Pi domain

Pi 1.4.0's `pi-ai` model union has chat, classifier and image operations, not embedding/audio/realtime operations. Its chat user/tool-result content supports text and images. `api/openai-responses-shared.js` emits inline data URLs with explicit `detail: "auto"` both in user input and in function/custom-call output arrays. Image input must therefore be assessed at both placements and with that detail, not inferred from a vision flag or an omitted-detail witness.

Pi's image metadata is not a universal enforcement layer. `inputLimits.images.resize` is client preprocessing; count/request-byte fields do not automatically constrain every request path. The gateway's finite discovery domain is not a general size/count guarantee. Schema fields and client-generated payload behavior require separate checks.

Public model admission may be narrower than official model specifications. Discovery reports the effective input domains; capacity evidence alone does not broaden media admission.

## Native effort and Pi planning follow-up — 2026-11-10

Re-read the official DeepSeek [thinking guide](https://api-docs.deepseek.com/guides/thinking_mode/), [Chat request reference](https://api-docs.deepseek.com/api/create-chat-completion/) and [Responses request reference](https://api-docs.deepseek.com/api/create-response/). They distinguish native `low/high/max` from compatibility inputs: `minimal → low`, `medium → high`, `xhigh → high`; `none` disables thinking. The native `deepseek-flash` Chat/Responses and beta Chat bindings now declare this exact mapping. Do not copy Bailian's independently selected mapping to native DeepSeek, or assume an accepted source spelling is a distinct effective level. No new model, API or `ultra` source enum is activated.

The 2026-11-10 installed Pi source exposes `Model.thinkingLevelMap`, `getSupportedThinkingLevels` and `clampThinkingLevel`. Explicit null entries hide unsupported choices; a low/high/max model clamps a medium preference upward to high. Its public model metadata has `contextWindow` and `maxTokens`, without an independent input-token maximum. The user selected actual total context first, then an input-only maximum as a planning fallback when total context is absent; never add input and output, or publish that fallback as a new total-context fact. Unresolved service abbreviations still need independent numeric evidence.

Successful registry refresh does not itself replace an already active/scoped model object. The adapter reconciles metadata at idle boundaries through public `setModel`/`setThinkingLevel`; the native preference/clamping policy stays in Pi. Offline/failed refresh retains qualified cached metadata rather than guessing new facts.

## Other tasks

Embedding already has independent typed contracts and runtime behavior. Reusing its discovery vocabulary does not connect it to the enhanced model list or Pi. Image/Audio/Video/Decision/Realtime schema components are metadata shapes only, with no new routes, callbacks, codecs or session lifecycle. Realtime transport and interruption/recovery contracts remain unimplemented and separately selected.
