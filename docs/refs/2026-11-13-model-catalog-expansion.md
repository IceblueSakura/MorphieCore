# Model catalog contract refresh

Sources reviewed on 2026-11-13. The compiled catalog owns model availability and binding IDs; these notes retain the service contracts behind the selected additions. [Earlier capacity research](2026-10-26-model-discovery.md) covers unchanged declarations. SDK/source locks remain independent of model registration.

## OpenAI Terra and SIWC

The [Terra model reference](https://developers.openai.com/api/docs/models/gpt-6.1-terra), [changelog](https://developers.openai.com/api/docs/changelog) and [GPT migration guide](https://developers.openai.com/api/docs/guides/latest-model) identify `gpt-6.1-terra`, published 2026-10-26, with text/image/file input and function/structured-output support. Tool execution uses Responses. Official API capacities are 1,050,000 context, 922,000 independent input and 128,000 output tokens.

Native positive effort levels are low/medium/high/xhigh/max; `none` disables thinking. Temperature, top-p and logprobs have a carrier only with thinking explicitly disabled. Candidate-local lowering omits those optional controls while thinking is active/default, retaining the effort and recording the existing omission rules.

The [SIWC model/inference contract](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference) and [limitations](https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations) retain their own restrictions: Responses, stateless full history, streaming and inline tool declarations. Official API ceilings are not SIWC service ceilings. The existing operator-selected API-matching context planning policy extends to Terra; account availability and independent SIWC input/output maxima require account-scoped evidence.

## OpenCode Go

Primary sources: [public model directory](https://opencode.ai/inference/go/v1/models), [native endpoints](https://opencode.ai/docs/console/go/#endpoints) and [translated harness endpoints](https://opencode.ai/docs/console/harnesses/#base-urls).

The selected native models use Chat Completions at `/inference/go/openai/v1/chat/completions`. Messages uses `/inference/go/anthropic/v1/messages`; the documented Responses translation uses `/inference/go/openai/v1/responses`. Translation availability does not add unselected model/protocol bindings.

| Model | Context | Independent input | Output | Positive effort |
|---|---:|---:|---:|---|
| `minimax-m3.1-flash` | 1,000,000 | Unknown | 512,000 | low, medium, high, xhigh, max |
| `hy4` | 1,048,576 | 983,010 | 65,536 | high; explicit off supported |
| `step-5-preview` | 1,000,000 | 1,000,000 | 65,536 | low, medium, high |

MiniMax and Step have image/video inputs; Hy4 has image input. The OpenAI-compatible Chat carrier is selected without activating a new downstream video extension. New readable history uses `reasoning_content`. MiniMax explicitly supports only automatic tool selection; restrictive choices cannot be silently ignored. Step advertises structured output; no strict function-generation guarantee is imported from its separate native Responses service. Missing capability declarations do not establish new negative protocol facts.

Publication sources: [MiniMax](https://www.minimax.io/blog/minimax-m31f), 2026-10-19; [Tencent Hy4](https://www.tencent.com/tencent-launches-hy4-joining-the-ranks-of-the-worlds-leading-open-source-ai-models/), 2026-11-04; [StepFun](https://www.stepfun.com/news/step-5-preview), public announcement 2026-09-20. The Go directory labels Step's release 2026-09-16; publication metadata follows the dated developer announcement rather than a router listing timestamp.

## OpenRouter

Sources: [model directory](https://openrouter.ai/api/v1/models), [Muse Ember endpoints](https://openrouter.ai/api/v1/models/meta/muse-ember/endpoints), [Step endpoints](https://openrouter.ai/api/v1/models/stepfun/step-5-preview/endpoints), [Meta publication](https://research.meta.ai/blog/muse-ember) and [Meta tool strictness](https://dev.meta.ai/docs/tool-calling/#strict-tool-schemas).

- `meta/muse-ember` is published as `muse-ember`, dated 2026-11-09. It accepts text/image/video/file/audio and requires thinking. Both listed endpoints have 1,048,576 context; their output maxima differ (524,288 versus 943,718), so no single route-wide output maximum is seeded. Positive effort spans minimal through max. Only automatic tool selection is declared. Meta's strict flag validates the submitted schema without guaranteeing generated argument conformance; no strict function guarantee is advertised by the canonical contract.
- `stepfun/step-5-preview` shares canonical/public `step-5-preview` with Go, but keeps its own replay issuer, wire model and limits. OpenRouter endpoint contexts differ (262,144 versus 1,000,000), as do output maxima (32,768 versus 64,000). Those unpinned route-wide capacities stay unknown. Image/video, structured output and low/medium/high thinking are declared; thinking cannot be disabled. The selected Chat/Responses bindings do not inherit Go's input/output ceilings or StepFun native Responses strictness.

OpenRouter's fixed `require_parameters` policy remains responsible for endpoint parameter selection. The separately scheduled Luna SSE intake issue remains in the [issue register](../roadmap/unscheduled-issues.md#content-and-client-qualification).

## Google Gemini 4

Sources: [release changelog](https://ai.google.dev/gemini-api/docs/changelog), [Argon](https://ai.google.dev/gemini-api/docs/models/gemini-4-argon), [Hydrogen](https://ai.google.dev/gemini-api/docs/models/gemini-4-hydrogen), [Carbon](https://ai.google.dev/gemini-api/docs/models/gemini-4-carbon) and [thinking controls](https://ai.google.dev/gemini-api/docs/thinking).

Exact public/native IDs are `gemini-4-argon` (2026-10-13), `gemini-4-hydrogen` (2026-11-12) and `gemini-4-carbon` (2026-11-10). All retain text/image/audio/video/file input, function tools and structured output through the existing GenerateContent and Interactions carriers. Their 1,048,576-token capacity is one input + thinking + output window; it does not seed an independent input maximum. The selected streaming bindings allow 65,536 output tokens. Carbon's smaller non-streaming limit does not describe these always-streaming upstream targets.

Thinking is mandatory and default high. Argon/Carbon natively support low/medium/high; Hydrogen also supports xhigh. GenerateContent spells the highest level `XHIGH`; Interactions spells it `xhigh`. Existing nearest-level projection applies outside each native domain; response-scoped effort uses a named model-specific scope conversion. Explicit thinking off rejects rather than disappearing. Sampling controls are ignored and omitted with existing named rules. No explicit-cache capability or 3.8-only terminal projection is activated for these new models.

## Aliyun plans

### Kimi K3.1 Pro

[Hosted model specification](https://help.aliyun.com/zh/model-studio/kimi-k3-1-pro), [Kimi API](https://help.aliyun.com/zh/model-studio/kimi-api), [thinking controls](https://help.aliyun.com/zh/model-studio/deep-thinking) and [developer publication](https://www.kimi.com/en/blog/kimi-k3-1-pro), 2026-10-24.

The DashScope spelling is `kimi-k3.1-pro`. Text/images, tools and structured output are supported; video and general file input are not advertised. Context, independent input and maximum output are each 1,048,576; the reasoning and output limits are not additive capacity. Native effort is low/medium/high/max. Thinking off is supported and numeric thinking budgets are not. Temperature, top-p and presence penalty accept only defaults 1.0, 0.95 and 0.0, including with thinking off: candidate-local lowering omits incompatible values under the existing named field rules; native validation rejects them. Historical reasoning remains in `reasoning_content`. The selected binding remains Chat; other documented protocols do not activate themselves.

### TokenPlan

[TokenPlan catalog](https://help.aliyun.com/zh/model-studio/token-plan-personal-overview), [base URLs](https://help.aliyun.com/zh/model-studio/base-url), [Codex configuration](https://help.aliyun.com/zh/model-studio/codex), [Qwen4 specification](https://help.aliyun.com/zh/model-studio/qwen4-max-preview) and [compatibility notice](https://help.aliyun.com/zh/model-studio/api-compatibility-upgrade-notice).

- `glm-5.3` selects Responses with its existing low/high/max mapping; minimal maps to low, medium to high, and xhigh to max. DashScope capacities do not become TokenPlan capacities.
- The exact new ID is `qwen4-max-preview`; no local `qwen4-max` alias is selected. It is TokenPlan-only. The published preview window is 2026-11-11 through 2026-11-25 (UTC+8). Its output maximum is 393,216; the model page's abbreviated `1M` context and a client configuration example do not establish an independent exact service input/context ceiling.
- Qwen4 supports text/images/video, function tools, JSON Object and JSON Schema. Its low/medium/xhigh controls reuse the shared Qwen effort calibration under a model-specific rule. Thinking and answer share the output budget: Chat `max_tokens` and `max_completion_tokens` are equivalent total ceilings, and Responses uses `max_output_tokens`. This does not change older Qwen Chat's reply-only `max_tokens` interpretation. Numeric budget and effort remain mutually exclusive.
- Function strictness follows the selected shared Bailian relaxation; generic file input remains unadvertised. Account access and remaining service qualification belong to the issue register, not protocol parsing.

## One DeepSeek entry

Sources: [Chat reference](https://api-docs.deepseek.com/api/create-chat-completion), [Responses guide](https://api-docs.deepseek.com/guides/responses_api), [Responses reference](https://api-docs.deepseek.com/api/create-response), [strict tools](https://api-docs.deepseek.com/guides/tool_calls), [thinking](https://api-docs.deepseek.com/guides/thinking_mode), and [prefix completion](https://api-docs.deepseek.com/guides/chat_prefix_completion).

The guides make `/beta` a prerequisite for strict mode, not a command to force every request into strict mode. Strict remains optional/default false in the Chat reference. Prefix completion also demonstrates beta use without tools. The Responses reference documents the normal origin but does not independently specify `/beta/responses`.

Loader-mediated native qualification confirmed `/beta/chat/completions` and `/beta/responses` accept ordinary requests, unchanged non-strict inventories, strict tool samples, thinking on/off and original tool/reasoning history continuation. This supports one `deepseek` Provider with those two default paths and the existing `deepseek` credential namespace. Public `deepseek-v4.1-flash-1017`, canonical/upstream `deepseek-flash`, source reasoning/history rules and explicit strictness remain unchanged. Account API-root overrides continue appending operation suffixes; no implicit prefix repair or alternate-path retry is introduced.

A source discrepancy remains scoped: the strict guide excludes `minLength`, but the beta endpoints accepted a strict schema containing it. Acceptance alone does not establish enforcement. Existing strict-schema validation is retained; no schema rewriting, forced strictness or expanded strict guarantee is inferred from this observation.
