# Generation projection contracts

Researched on 2026-10-15. These notes establish official wire contracts and mapping constraints, not implemented support or live Provider acceptance. Code and tests define behavior. Integration limits and deferred product choices belong in [Roadmap](../roadmap.md#integration-handoff).

## Sources and version boundaries

| Source | Revision | Contract |
|---|---|---|
| [OpenAI Python SDK](https://github.com/openai/openai-python/tree/be9d66628ad7377bd36fe5a76ae6d735843f0e76) | `be9d66628ad7377bd36fe5a76ae6d735843f0e76` | Separate Chat and Responses request, content, tool and usage types |
| [Anthropic Python SDK](https://github.com/anthropics/anthropic-sdk-python/tree/b4b7916deaf4e5570cd627b1ae7ee4394a4de39f) | `b4b7916deaf4e5570cd627b1ae7ee4394a4de39f` | Messages thinking variants, effort, service tier and cache prewarming |
| [Google Gen AI Python SDK](https://github.com/googleapis/python-genai/tree/78f6d3412caaaed44f99ce57be2d16d0b8c407cb) | `78f6d3412caaaed44f99ce57be2d16d0b8c407cb` | generateContent types and Interactions v1 generation configuration |
| [Gemini v1beta discovery](https://generativelanguage.googleapis.com/$discovery/rest?version=v1beta) | observed revision `20261012`, id `generativelanguage:v1beta` | GenerationConfig, ThinkingConfig, FunctionCallingConfig and service-tier wire enums |
| [DeepSeek Chat](https://api-docs.deepseek.com/api/create-chat-completion), [thinking mode](https://api-docs.deepseek.com/guides/thinking_mode) | unversioned, read 2026-10-15 | Cache count aliases and readable reasoning history |
| [OpenRouter reasoning](https://openrouter.ai/docs/guides/best-practices/reasoning-tokens) | unversioned, read 2026-10-15 | Plaintext aliases, structured reasoning and routing-level approximations |
| [MiMo Responses](https://mimo.mi.com/docs/zh-CN/api/chat/responses) | unversioned, read 2026-10-15 | Explicit effort aliases and the distinction between accepted labels and effective thinking strength |

The discovery URL is mutable, not an archived schema or an Interactions v1 descriptor. The observed response SHA-256 was `d0bc24031ace09fe6b7bd3f0bee4aae327d097e9df412fffa42fdb72b21e4b96`; the pinned SDK supplies a separately retrievable revision. A revision, SDK declaration or guide does not establish account access or effective support on every model.

Live guides can describe models and constraints newer than pinned types. Do not silently merge versions. In particular, a newer image-detail enum in the multimodal research does not enlarge this pinned Chat baseline.

## Protocol baselines and explicit extensions

Chat and Responses are different official schemas even when they share a field name. Anthropic and Google are independent baselines, not OpenAI dialects. Normalize equivalent Provider spellings before semantic decoding, and represent non-equivalent extensions explicitly. The selected boundary is recorded in [Architecture](../architecture.md#protocol-baselines-and-provider-dialects).

### Cache count aliases

DeepSeek documents `prompt_cache_hit_tokens` as cached prompt tokens and `prompt_cache_miss_tokens` as uncached prompt tokens. Its Chat report also has the canonical `prompt_tokens_details.cached_tokens` position. The reported aliases must agree with that count and the prompt total when present.

These aliases are absent from the pinned [OpenAI completion usage type](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/completion_usage.py). Their presence cannot change interpretation under a standard profile that has not declared them.

Minimal conflict example:

```json
{
  "prompt_tokens": 3, "completion_tokens": 2, "total_tokens": 5,
  "prompt_tokens_details": {"cached_tokens": 1},
  "prompt_cache_hit_tokens": 0
}
```

A declared alias profile must reject the disagreement. Standard upstream parsing ignores the unregistered alias and preserves the canonical count. Never estimate an absent count from a default invented by another dialect.

### Readable and structured reasoning

The pinned official Chat [message](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/chat/chat_completion_message.py) and [delta](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/chat/chat_completion_chunk.py) types do not declare `reasoning_content`, `reasoning` or `reasoning_details`. DeepSeek and OpenRouter supply separate extension contracts.

OpenRouter declares `reasoning_content` as an alias of `reasoning`. A declared plaintext alias must not depend on a structured-details sibling being present. If multiple nonempty readable views are supplied, normalize and check agreement before constructing IR. MorphieCore's selected nonempty-only plaintext agreement is recorded in [Architecture](../architecture.md#chat-reasoning-extensions); the public alias description alone does not specify every conflicting combination.

An extension that carries reasoning is semantic input, not merely a value for later wire restoration. Keep it distinct from ordinary assistant text. Structured text, summaries and encrypted continuation values have different ownership and replay requirements; unfamiliar formats are not automatically Responses ciphertext.

An unknown upstream reasoning-shaped field is ignored under the standard profile. Client extension admission is a separate contract. Preserve registered MorphieCore extensions rather than removing them in the name of standardization.

## OpenAI parameter and content boundaries

### Output limits

The pinned [Chat create type](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/chat/completion_create_params.py) declares `max_completion_tokens` and deprecated `max_tokens`; [Responses](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses/response_create_params.py) uses `max_output_tokens`. The singular `max_completion_token` is not an official alias. Legacy spelling and model eligibility belong to protocol/target checks, not arbitrary alias acceptance.

### Function tools and call history

The pinned Chat [function definition](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/shared_params/function_definition.py) declares `name`, `description`, `parameters` and `strict`. The Responses [function tool](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses/function_tool_param.py) additionally declares dispatch controls and `output_schema`. Shared reader code does not make those members official Chat fields.

A misplaced active field such as Chat `function.async: true` cannot be both diagnosed as ignored and used to construct dispatch semantics. Register and interpret a deliberate extension, reject known misplaced execution intent, or ignore it without reading it. Never let an ignored value reach another protocol through IR.

Official Chat custom tools use a nested [custom definition](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/chat/chat_completion_custom_tool_param.py), unlike the flat Responses shape. Unsupported custom tools are a missing carrier, not evidence that their contents can be treated as unknown metadata.

The official assistant message type retains deprecated `function_call`. Dropping that member while accepting the accompanying assistant text loses a call. Explicit rejection can protect history until a deliberate translation exists.

### Prediction is an optimization, not an output constraint

Chat `prediction` supplies predicted output content to reduce latency, as documented by the pinned create type. Accepted/rejected prediction token counts are independent usage facts; they cannot reconstruct this request intent.

Omitting prediction does not authorize replacing or constraining generated text. D3 selects named omission after source validation until a faithful carrier exists; see [the request policy](../architecture.md#selected-request-approximations). This is a product decision, not a protocol requirement. Treating it as an unregistered unknown gives no explicit account of that policy.

### Cache options, breakpoints and usage

Both pinned create types define `PromptCacheOptions` with `total=False`: `mode` and `ttl` are optional, not mandatory paired members. Chat has `mode` and `ttl`; Responses additionally has nullable `comparison_response_id` and non-nullable `prewarm`. Declaring `{}` must not invent explicit defaults.

The pinned Chat [text](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/chat/chat_completion_content_part_text_param.py) and [image](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/chat/chat_completion_content_part_image_param.py) parts declare `prompt_cache_breakpoint: {"mode":"explicit"}`. It inherits TTL from request cache options and marks an exact prefix boundary. This is an official carrier, not a Provider-only field. Adding missing carriers still follows the later support gate.

Pinned Chat prompt-token details include `image_tokens`, `text_tokens`, `audio_tokens`, `cache_write_tokens` and `cached_tokens`; completion details include text, audio, reasoning and prediction counters. In particular, a reported image count is not inherently an extension. Preserve reported zero versus absence; do not promote estimates or broaden Responses positions because Chat has a carrier.

The pinned Chat image-detail enum is `auto/low/high`. DeepSeek's `original` and newer official source revisions require separate versioned admission. A later official addition does not make every older target accept it.

## Thinking activation, effort, budgets and display

### Anthropic has independent activation and effort

The pinned SDK separates [disabled](https://github.com/anthropics/anthropic-sdk-python/blob/b4b7916deaf4e5570cd627b1ae7ee4394a4de39f/src/anthropic/types/thinking_config_disabled_param.py), [enabled](https://github.com/anthropics/anthropic-sdk-python/blob/b4b7916deaf4e5570cd627b1ae7ee4394a4de39f/src/anthropic/types/thinking_config_enabled_param.py) and [adaptive](https://github.com/anthropics/anthropic-sdk-python/blob/b4b7916deaf4e5570cd627b1ae7ee4394a4de39f/src/anthropic/types/thinking_config_adaptive_param.py) variants. `output_config.effort` is a separate control. The [effort guide](https://platform.claude.com/docs/en/build-with-claude/effort), read 2026-10-15, explains that effort affects the response and tools even when thinking is disabled; supported combinations vary by model.

Minimal independent-intent example for an eligible target:

```json
{"thinking":{"type":"disabled"},"output_config":{"effort":"high"}}
```

A single effort enum cannot use `none` for disabled and `high` for response effort simultaneously. Low effort is not off. D1 must also resolve the scope of effort, not merely add a boolean; an extension bag must not conceal the missing semantic dimension.

The [extended-thinking guide](https://platform.claude.com/docs/en/build-with-claude/extended-thinking), read 2026-10-15, requires manual `budget_tokens >= 1024` and normally `< max_tokens`, with an interleaved-thinking exception. Validate the selected variant before normalization: budget zero is not Anthropic's disabled spelling, and unsupported known members must not disappear while changing modes.

The numeric budget is guidance, not a guaranteed token count or a strict cap. `max_tokens` remains the hard total-output ceiling. Official migration from manual to adaptive thinking describes a behavioral change, not an exact conversion from numeric budgets to effort levels. Deprecation and acceptance are model-specific.

Display suppression controls what is returned, not whether reasoning runs. Do not infer off from `display: omitted`, hidden thoughts or a disabled summary. Absent defaults vary by model/source revision; avoid synthesizing an activation change to carry display.

### Google carriers differ

The discovery descriptor and pinned SDK's [types](https://github.com/googleapis/python-genai/blob/78f6d3412caaaed44f99ce57be2d16d0b8c407cb/google/genai/types.py) distinguish `thinkingBudget`, `thinkingLevel` and `includeThoughts`. The [generateContent thinking guide](https://ai.google.dev/gemini-api/docs/generate-content/thinking), read 2026-10-15, documents budget `0` as disabled and `-1` as automatic, subject to model support. Numeric guidance may underflow or overflow.

`MINIMAL` is little or no thinking, not guaranteed disabled. Some models cannot disable reasoning, and allowed levels vary. Preserve numeric guidance on an eligible budget carrier; do not automatically replace it with a coarse level or treat lower levels as equivalent across models.

The pinned Interactions [generation config](https://github.com/googleapis/python-genai/blob/78f6d3412caaaed44f99ce57be2d16d0b8c407cb/google/genai/_gaos/types/interactions/generationconfig.py) exposes thinking levels and summaries but no declared numeric budget or explicit off member. Hiding summaries is not an off carrier. Any selected budget or scope approximation follows the [product policy](../architecture.md#reasoning-level-calibration); it is not an equivalence established by this protocol source.

### Cross-protocol activation and effort

The following comparison uses the pinned SDKs above and the linked guides read on 2026-10-15. It describes wire controls, not the model capabilities of every binding.

| API | Activation or off representation | Strength and other controls | Independent off plus non-off effort? |
|---|---|---|---|
| OpenAI Chat | `reasoning_effort: none` on eligible models; no separate declared `enabled` field | The same enum selects reasoning effort | Not in the selected schema: one field cannot simultaneously be `none` and `high` |
| OpenAI Responses | `reasoning.effort: none` on eligible models; no separate declared `enabled` field | `effort` selects reasoning strength; `mode` selects standard/pro execution; `summary` controls a returned view | Mode and effort are independent, but standard/pro is not an on/off switch |
| Google generateContent | Eligible budget-based models use `thinkingBudget: 0`; dynamic allocation uses `-1` | A positive numeric budget or a model-supported thinking level guides reasoning; `includeThoughts` selects returned summaries | No documented independent off-plus-nonzero-strength contract established here; budget zero is a value of the allocation control |
| Google Interactions v1 | No documented off member in the selected generation config | Levels `minimal/low/medium/high`; summaries `auto/none` | No documented independent switch; `minimal` and hidden summaries do not establish off |
| Anthropic Messages | Distinct disabled/enabled/adaptive thinking variants, subject to model restrictions | `output_config.effort` affects response text, tool calls and thinking when active | Yes on eligible models; disabled plus high effort is a meaningful counterexample |

OpenAI's pinned [reasoning type](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/shared_params/reasoning.py) says effort constrains reasoning. Its [reasoning guide](https://developers.openai.com/api/docs/guides/reasoning) distinguishes execution mode from effort. Supported efforts vary by model; the existence of `none` in the SDK enum is not universal off support. OpenRouter's `reasoning.enabled` is a separate router extension, not proof that OpenAI has that official field.

Google's [thinking-level](https://github.com/googleapis/python-genai/blob/78f6d3412caaaed44f99ce57be2d16d0b8c407cb/google/genai/_gaos/types/interactions/thinkinglevel.py) and [summary](https://github.com/googleapis/python-genai/blob/78f6d3412caaaed44f99ce57be2d16d0b8c407cb/google/genai/_gaos/types/interactions/thinkingsummaries.py) types are distinct. Neither a hidden summary nor the lowest level supplies an independent activation contract. Do not infer that every combination of budget and level fields is valid merely because a generated type declares both.

Anthropic's [How effort works](https://platform.claude.com/docs/en/build-with-claude/effort#how-effort-works) explicitly applies effort to all response tokens, including tool arguments and text without thinking. Therefore it is broader than OpenAI's documented reasoning effort. Adding `enabled: false` next to an unchanged, supposedly universal reasoning-effort field would retain two wire values but would not by itself resolve their semantic scope.

The value of independent activation is faithful expression of an official contract, even when only one API has that carrier. It is not evidence that off plus response-wide high effort can be translated to OpenAI `none` plus `high`, Google low/minimal, or a verbosity setting. The selected [IR direction](../architecture.md#reasoning-activation-and-effort-scope) models activation and one scope-qualified effort control. This preserves the official counterexample without adding parallel effort controls that no selected request requires together. Any selected cross-scope conversion is a product approximation, not an equivalence established by these sources.

### Hand-authored same-model level calibration

MiMo's Responses reference explicitly aliases `minimal` to `low`, and `xhigh`, `max` and its extra `ultra` label to `high`. It also states that all non-off levels currently enable the same thinking behavior: accepted labels are not evidence of distinct effective effort. A table can document those wire aliases without claiming performance calibration. Do not infer the same contract for an older Chat documentation page or another intermediary.

Deliberate level calibration in encode/decode for the same model is a product policy, not an official claim that different Providers expose equivalent levels. Current approximation decisions belong in [Architecture](../architecture.md#reasoning-level-calibration), separately from protocol facts.

For example, one Provider may expose medium/high and another medium/high/xhigh. A hand-authored encode rule may choose `low → medium`. It does not imply a decode rule `medium → low`, nor determine how to invert high/xhigh if both were mapped to one canonical level. Configure each intended direction explicitly under trusted binding selection.

The existing level interface is limited to qualitative levels. Scope or numeric-budget approximation needs a separate explicit rule, not an inferred extension of a level table. Keep request intent separate from what the upstream actually reports, and do not synthesize effective settings from the request. The rule belongs at the codec boundary, not in protocol-independent IR or arbitrary client JSON. The [initial mapping plan](../architecture.md#initial-mapping-plan) uses hand-authored Rust policies selected by trusted bindings; no external mapping configuration interface is selected.

### Pi thinking-budget presets

Pi's current `main`, pinned for this lookup at `4a5d372` on 2026-10-15, supplies the same shared defaults as the broader client pin `4270522`. These are product-level presets, not a protocol equivalence. The [current simple-options helper](https://github.com/earendil-works/pi/blob/4a5d37291531ad48b8e536c892557877d219463c/packages/ai/src/api/simple-options.ts) defines:

| Pi level | Shared default thinking tokens |
|---|---|
| minimal | 1024 |
| low | 2048 |
| medium | 8192 |
| high | 16384 |

That helper folds `xhigh` and `max` into `high` for budget lookup and accepts custom budget overrides. This many-to-one mapping supplies no unique inverse. It does not specify how an arbitrary incoming numeric budget should be quantized to a level.

The [current Anthropic codec](https://github.com/earendil-works/pi/blob/4a5d37291531ad48b8e536c892557877d219463c/packages/ai/src/api/anthropic-messages.ts) uses the helper for manual-budget models and a separate effort path for adaptive models. The [current Google codec](https://github.com/earendil-works/pi/blob/4a5d37291531ad48b8e536c892557877d219463c/packages/ai/src/api/google-generative-ai.ts) has target-specific numeric tables and a separate discrete-level path. The shared table is therefore not a universal Google or Anthropic model contract.

Pi's helper can add the thinking budget to a supplied response allowance, then cap it at the model limit; it reserves answer room when necessary. MorphieCore must not copy that adjustment onto an explicit client-owned total-output ceiling. Reusing preset values does not authorize raising a hard cap or converting disabled activation into positive thinking.

MorphieCore selects a finer six-budget preset rather than copying pi's collapsed high-budget alias. The gateway matches a positive numeric budget by minimum absolute distance with the highest level winning every tie; target-specific tables may still contain duplicates. Both the selected values and reverse quantization are product decisions, not official level equivalences or behavior established by pi's forward helper. Original numeric intent stays in IR; see [the complete P3 contract](../architecture.md#thinking-budget-quantization). The earlier proposal to lower conversion-generated budgets is superseded: P3b rejects explicit or conversion-generated budgets above the applicable client ceiling, without reducing the budget, raising the ceiling or adding gateway-defined answer-room reservations.

## Sampling, schemas and service policy

### Sampling declarations are not model guarantees

The pinned Interactions SDK still declares and serializes `temperature` and `top_p`, marked deprecated. Their omission from a reference page does not prove server rejection. The current [Interactions text guide](https://ai.google.dev/gemini-api/docs/interactions/text-generation) also warns that newer models ignore custom sampling parameters. Determine model-effective support separately from parser acceptance; do not remove fields merely because one documentation view omits them.

generateContent has top-k, stopping sequences, seed and penalties; Interactions declares seed and stopping sequences. They are not interchangeable field inventories. The pinned Anthropic create type no longer declares sampling controls, while broader live references retain model-dependent legacy parameters. Do not call a selected profile's sampling omission a universal Messages protocol rule.

Rechecked 2026-10-16: [generateContent](https://ai.google.dev/api/generate-content#v1beta.GenerationConfig) permits up to five stop sequences. The [v1beta discovery schema](https://generativelanguage.googleapis.com/$discovery/rest?version=v1beta) declares signed `int32` carriers for `seed` and `topK`; an IR integer outside that target range needs rejection, not truncation. A sampling seed is a best-effort control, not a cross-model determinism promise.

Messages reports a matched `stop_sequence` separately from `stop_reason`. Chat `finish_reason: "stop"` and Google `finishReason: "STOP"` do not identify the matched text or distinguish natural completion from sequence stopping. Google's report remains independent of any function calls in the same candidate. Do not reconstruct these facts from the requested stop list or generated suffix.

### Function schemas and choices

Google's descriptor distinguishes `parameters` from `parametersJsonSchema`; these are mutually exclusive and the latter describes a JSON object instance. A JSON-object schema document such as `{"type":"string"}` does not satisfy that instance requirement. Check the declared target contract without implementing a general JSON Schema theorem prover.

Google declares `AUTO/ANY/NONE/VALIDATED`; the pinned Interactions function-choice types declare corresponding modes and allowed-tool filters. Constrained decoding is not automatically equivalent to OpenAI strict schema normalization. A missing allowed-tool mapping is a later support task, not permission to replace its semantics.

### Service tiers are official but not universally equivalent

Pinned OpenAI Chat accepts `auto/default/flex/scale/priority/fast`; Responses also accepts `ultrafast` through its [ServiceTier type](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses/service_tier.py). Reported service may differ from requested service; `fast` can report `priority`.

Anthropic Messages declares request values `auto/standard_only`; its [live reference](https://platform.claude.com/docs/en/api/messages/create), read 2026-10-15, reports actual service separately in usage. Anthropic speed controls are a separate contract, not an OpenAI tier alias.

Google's observed request enum is `unspecified/standard/flex/priority`, with actual service also reportable in usage. The names do not establish equivalent capacity, billing or latency promises across APIs. Preserve those distinctions when designing execution context; do not solve them by copying every wire enum into one supposedly universal tier.

## Later support boundaries

Anthropic's pinned [create parameters](https://github.com/anthropics/anthropic-sdk-python/blob/b4b7916deaf4e5570cd627b1ae7ee4394a4de39f/src/anthropic/types/message_create_params.py) document `max_tokens: 0` cache prewarming. The [cache guide](https://platform.claude.com/docs/en/build-with-claude/prompt-caching#pre-warming-the-cache) restricts streaming, manual enabled thinking, structured output and forced tools. This is an operation with its own response contract, not grounds to remove all positive-output validation.

Top-k, stopping sequences, prediction carriage, additional schema/tool carriers, safety settings, native cache resources and service-policy translation remain separately scoped support work. Complete existing-projection repair first. Messages citations and other deferred hosted/media contracts retain their existing roadmap decisions.
