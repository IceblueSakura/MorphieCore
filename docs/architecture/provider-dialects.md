# Provider Dialects and Service Contracts

This document specifies Provider- and API-specific wire normalization, restrictions, and carrier selections implemented across MorphieCore codecs and adapters.

Authoritative per-model registrations (enabled models, upstream IDs, binding IDs, token limits, effort mappings, and rule identifiers) live in [providers.json](../../crates/morphiecore-catalog/src/provider/providers.json), [topology.json](../../crates/morphiecore-catalog/src/topology/catalog/topology.json), and `morphiecore --catalog` rather than being duplicated here. See [Protocol adaptation](adaptation.md) for cross-provider rules.

## Contents

- [Google Interactions](#google-interactions)
  - [Google strict function arguments](#google-strict-function-arguments)
  - [Google response-effort projection](#google-response-effort-projection)
- [Anthropic Messages](#anthropic-messages)
  - [Request rules](#request-rules)
  - [Missing output ceilings](#missing-output-ceilings)
  - [Owner-local cache admission](#owner-local-cache-admission)
  - [Caller-selected automatic cache admission](#caller-selected-automatic-cache-admission)
  - [Response rules](#response-rules)
- [OpenAI and subscription Responses](#openai-and-subscription-responses)
  - [OpenAI Terra](#openai-terra)
  - [Subscription Responses](#subscription-responses)
- [Aliyun Bailian](#aliyun-bailian)
  - [Shared Aliyun billing dialect](#shared-aliyun-billing-dialect)
- [DeepSeek](#deepseek)
  - [Native DeepSeek Chat](#native-deepseek-chat)
- [OpenCode Go](#opencode-go)
  - [OpenCode Go controls](#opencode-go-controls)
- [ModelBest MiniCPM](#modelbest-minicpm)
  - [Hosted MiniCPM Chat](#hosted-minicpm-chat)
- [OpenRouter](#openrouter)

## Google Interactions

MorphieCore uses `/v1/interactions` (`store: false`, streaming generation intake; synchronous `user_input` step JSON for `gemini-nano-banana-2.1` image tasks) with `x-goog-api-key` authentication.
- Sampling controls (`temperature`, `top_p`, `top_k`, penalties) have no Interactions v1 carrier on deployed Gemini bindings and omit under named adaptation rules.
- Ordinary Interactions text/call signatures without a lawful standard carrier reject on stateless Responses and Chat/Messages delivery ([stateless boundary](history-and-replay.md#stateless-api-boundaries)).
- Static images (`gemini-nano-banana-2.1`): send ordered `image` parts plus `text` inside one `[{"type": "user_input", "content": [...]}]` step with `"response_format": {"type": "image"}`; publish exactly one inline image from `model_output` steps only after `"status": "completed"`, empty `errors`, and EOF (`images.gemini.artifact-only.v1` omits accompanying text/thoughts).

### Google strict function arguments

Interactions targets admit a qualified closed-object function-argument subset via request-wide `tool_choice: "validated"` (or `"any"` when required), independent of JSON text schemas:
- **Permitted vocabulary:** `type`, `properties`, `required`, `additionalProperties: false`, array `items`, string/numeric `enum` (safe binary64 integers/exact floats; boolean types supported, boolean enums excluded), `title`, and `description`, with closed nested objects and two-branch nullable `anyOf` (`[<supported>, {"type": "null"}]` + optional title/description). Calls enabled require literal root `type: "object"`. Unsupported constraints (length/pattern/general composition) reject explicit strictness pre-dispatch.
- **Bounded references:** Root `$defs` and `#/$defs/<name>` pointers (using standard `~0`/`~1` JSON Pointer decoding via `serde_json::Value::pointer`) check all definitions to a concrete object root and max logical depth 64 without cycles or remote/percent-encoded refs.
- **Whole-inventory mode & no-call equivalence:** When calls are enabled with any strict tool, every declaration in the effective inventory must fit the subset; conditional omitted strictness falls back non-strict when unclosed or overflowing grammar budget, while explicit `strict: true` never falls back. Under explicit `tool_choice: "none"` (`none`), strictness and serial intent satisfy vacuously without conditional fallback or scalar-root rejection (L2). Missing parameter schemas stay parameterless; serial calls reject when calls are enabled.

### Google response-effort projection

Model-qualified `google.<model>.response-effort-to-thinking.v1` rules map response-scoped effort onto Interactions `generation_config.thinking_level` via `ReasoningConversion::Scope`. Native positive level domains and mandatory-thinking constraints (`low`/`medium`/`high`, plus `xhigh` on Hydrogen and Neon) live in [topology.json](../../crates/morphiecore-catalog/src/topology/catalog/topology.json).

## Anthropic Messages

### Request rules

- Top-level root instructions compose into `system` text blocks; subsequent mid-conversation instructions and configuration updates become mid-conversation system messages.
- JSON Schema output constraints map to `output_config.format`; `input_examples` and document `context` project natively on supported bindings.
- Manual thinking's historical thinking-prefix constraint is checked on the native Messages candidate after projection, leaving cross-protocol history transfers valid.

### Missing output ceilings

Bindings with a catalog default `max_tokens` (such as Go Haiku's `32000`) inject that default only when the client omitted an output ceiling and configuration has not replaced or cleared (`null`) it. Uncapped Messages candidates without an effective default reject locally.

### Owner-local cache admission

Owner-local `cache_control` markers (`5m` / `1h`) are admitted across all routes: native-capable candidates preserve breakpoints, while non-native candidates drop them per owner under named L4 rules without skipping route candidates.

### Caller-selected automatic cache admission

Admitted Messages routes accept root `"cache_control": {"type": "ephemeral", "ttl": "1h" | "5m"}` (defaulting to `5m`), placing up to 4 automatic turn breakpoints; omitting root cache control preserves the binding's marker-free default.

### Response rules

- Thinking delivery requires valid non-empty signatures (`messages.omit-uncarried-reasoning.v1` omits unsigned/multi-summary uncarried thinking; malformed signatures reject).
- Empty-string zero-arg call inputs normalize to `{}` (L2); other non-object tool inputs remain rejected.
- Argument fragments buffer leading whitespace prefixes to keep streaming JSON grammar valid.
- Trusted OpenCode Go/Haiku intake closes an open tool-input block at explicit `max_tokens` delta when buffered bytes form a valid unfinished object prefix, followed by `message_stop` and strict EOF—marking the call incomplete without claiming executable completion.

## OpenAI and subscription Responses

Public Chat, Responses, and Messages adhere strictly to pinned official specifications without vendor replay extensions. Native OpenAI Platform bindings carry strict schemas directly; `openai` API-key and `chatgpt` SIWC OAuth share codecs while keeping isolated catalogs, request rules, and replay provenance.

### OpenAI Terra

`gpt-6.1-terra` retains sampling (`temperature`, `top_p`, `logprobs`) only when resolved reasoning activation is explicitly off; active or default thinking omits sampling under named L4 rules without collapsing otherwise-native configuration history.

### Subscription Responses

SIWC (`chatgpt`) and `grok` run stateless Responses streams (`store: false`), deriving `prompt_cache_key` from cache affinity and reconstructing terminal items strictly from events:
- **SIWC flat root tools and default namespace replay:** `siwc.inline-root-tools.v1` (L2) moves flat function/custom definitions into a leading developer `additional_tools` batch while preserving names, schemas, strictness, and history order. When SIWC omits `namespace` on historical `function_call` / `custom_tool_call` items declared inside the default `functions` namespace (root or inline `additional_tools`), admission accepts the unqualified replay while continuing to reject unqualified calls for non-default namespaces; cross-protocol targets apply reversible namespace aliasing (`generation.alias-tool-namespaces.v1`) and grammar-free plain custom-text wrapping (`generation.wrap-plain-custom-tools.v1`).
- **SIWC error mapping:** `responses.coarsen-provider-error-code.v1` maps quota codes to `rate_limit_exceeded`, unsupported capability codes to `invalid_prompt`, and auth/service/permission errors to `server_error` while retaining source evidence.
- **Grok:** Preserves request order and explicit tool policies unchanged; no strictness relaxation, prompt rewriting, or implicit retry alters upstream omissions.

## Aliyun Bailian

### Shared Aliyun billing dialect

`aliyun-dashscope-cn` and `aliyun-tokenplan-cn` share the `Bailian` wire dialect across Chat and Responses ([models.rs](../../crates/morphiecore-protocol/src/openai/dialect/bailian/models.rs)):
- **Reasoning & output carriers:** Readable reasoning maps to `reasoning_content` with `preserve_thinking`. Output ceilings map to `max_completion_tokens` (Chat) or `max_output_tokens` (Responses, recording `generation.reply-only-output-ceiling.v1` where reply-only scope applies; model-qualified Qwen4-Max uses total-output semantics across both). Bailian Responses retains `text.format` without rejecting the whole text config when `verbosity` lacks a carrier.
- **Model-qualified event aliases & `summary`:** Binding-qualified rules map `reasoning_text` to summaries on legacy shorthand vs. reasoning content on qualified hosted bindings; TokenPlan Qwen4-Max retains explicit `summary: "auto"` without inventing it and takes concrete summaries over null placeholders.
- **Tool policies & Kimi controls:** Bailian Responses carries `parallel_tool_calls`, filters single-function forced choices when needed on qualified models, and rejects forced/specific tool selection under default/enabled thinking on bindings where upstream fails those controls (`qwen3.8-flash`, `glm-5.3`). Kimi K3.1 Pro omits incompatible optional sampling values on candidate copies while rejecting known invalid values on native source validation.
- **Verified-only function strictness policy:** Because DashScope and TokenPlan share one upstream billing implementation with unverified schema enforcement, both default to `bailian.relax-function-strictness.v1` (L3)—relaxing explicit `strict: true` and conditional defaults to non-strict on candidate copies (omitting `strict` in Bailian declarations and emitting `strict: false` in retained Responses batches) while leaving structured text output strictness, tool identity, required selection, and thinking intent strict.

## DeepSeek

`deepseek` exposes `/beta/chat/completions` and `/beta/responses` by default (explicit account API-root overrides keep exact user prefixes without path inference); `deepseek-v4.1-flash-1017` targets upstream `deepseek-flash`. Native DeepSeek Responses omits summary/encrypted-output preferences with receipts while retaining effort.

### Native DeepSeek Chat

- Rejects JSON Schema text constraints and thinking-mode tool forcing when thinking is active or default (explicit thinking-off retains required tool choice; the gateway never disables thinking automatically).
- `deepseek.full-tool-inventory-choice.v1` (L2) maps full-inventory allowed sets to scalar `auto`/`required`, while proper subsets reject rather than deleting declarations.

## OpenCode Go

### OpenCode Go controls

- HY4 and MiniMax M3.1 Flash bind native service effort domains on Chat with `generation.project-effort.v1` fallback; HY4 retains standard thinking-off while MiniMax's always-thinking contract rejects explicit off and restricts `tool_choice` to auto.
- **Go Messages top-level composition:** Rejects root `anyOf`, `allOf`, and `oneOf` in tool parameter schemas (including inline definitions and no-call requests) cleanly pre-dispatch while leaving nested compositions untouched.

## ModelBest MiniCPM

### Hosted MiniCPM Chat

MiniCPM bindings map explicit thinking activation via `chat_template_kwargs.enable_thinking` (positive reasoning effort also activates thinking while retaining `reasoning_effort`; omitted keeps the model default). Readable response history normalizes both `reasoning_content` (MiniCPM5) and `reasoning` (MiniCPM-V 4.7) into one typed reasoning owner across JSON and SSE.

## OpenRouter

OpenRouter Responses maps an empty first-frame `response.failed` event as a classified stream error rather than a start event or success, preserving strict terminal checks and bounded access-program report fields without inferring billing tokens.
