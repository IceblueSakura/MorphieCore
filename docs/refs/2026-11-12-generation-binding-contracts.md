# Generation binding contracts

Primary sources reviewed on **2026-11-12**. These are model/service contracts, separate from live account eligibility and complete Agent-loop qualification. Accepted projections belong in [Protocol adaptation](../architecture/adaptation-and-dialects.md); unresolved qualification belongs in the [issue register](../roadmap/unscheduled-issues.md).

## SIWC tool carriage

The official [preview limitations](https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations.md) require HTTP `store:false`, `stream:true` and array input. Function/custom tools use namespaces or `additional_tools` input items. This is a carrier restriction, not an absence of client-executed tools. Hosted-tool availability and unsupported output ceilings remain independent service limitations.

The existing Responses developer `ToolBatch` represents the latter carrier. Moving flat root definitions into a leading batch preserves their availability before historical calls, names, schemas, strictness and direct call/result references. Existing inline declarations retain their order; the projection neither chooses a schema winner nor rewrites history. Root namespaces stay native. The same function/custom definition writer serves root and inline carriage.

## GPT model facts

The official [Astra](https://developers.openai.com/api/docs/models/gpt-6-astra.md), [Sol](https://developers.openai.com/api/docs/models/gpt-6.1-sol.md) and [Luna](https://developers.openai.com/api/docs/models/gpt-6-luna.md) pages list function calling, structured outputs and image input. The [function-calling guide](https://developers.openai.com/api/docs/guides/function-calling.md#strict-mode) defines strict function schemas through the structured-output carrier. Sol tool calling requires Responses; Luna Chat function calling requires reasoning off. These facts do not grant a proxy or billing-plan endpoint another service's capacity, pricing or guarantees.

Astra/Sol expose positive reasoning levels `low`, `medium`, `high`, `xhigh`, `max`. Luna additionally permits `none`. The [model guide](https://developers.openai.com/api/docs/guides/latest-model) explicitly excludes `none` for Astra/Sol. A generic SDK enum therefore cannot supply every model's activation or native level domain.

## Grok reasoning

The official [reasoning guide](https://docs.x.ai/developers/model-capabilities/text/reasoning.md) and [Grok 4.7 page](https://docs.x.ai/developers/grok-4-7.md) expose `low`, `medium`, `high`, `xhigh`. Reasoning cannot be disabled; omission uses the service default `high`. Mapping a non-native positive level to an existing level is distinct from replacing explicit off with active thinking.

Grok 4.7 returns Responses encrypted reasoning by default and documents unchanged next-request replay. The [Responses streaming contract](https://developers.openai.com/api/reference/resources/responses/streaming-events) supplies owner closure and native replay shapes independently of binding history assessment.

## Parameter contract recheck — 2026-11-13

The loader-mediated [OpenCode Go directory](https://opencode.ai/inference/go/v1/models)
declares HY4 reasoning efforts `none/high`, and Muse positive efforts
`minimal/low/medium/high/xhigh`. HY4's standard Chat effort carrier distinguishes
enabled reasoning from explicit off; Muse accepts the standard Responses effort
carrier. These facts do not qualify Go MiMo controls, Muse off behavior, numeric
thinking budgets or structured-text enforcement. Missing controls retain the
service default. Non-native positive levels use the existing nearest-level rule.

The [Luna router endpoint inventory](https://openrouter.ai/api/v1/models/openai/gpt-6-luna/endpoints)
lists `verbosity` on every listed endpoint. It does not justify removing Luna's
other selected request restrictions or certifying arbitrary nested guarantees.

The native DeepSeek [Chat reference](https://api-docs.deepseek.com/api/create-chat-completion/)
allows `response_format.type` only as `text/json_object`. Default thinking is on;
`required` and named forcing require explicit thinking off. Its native selection
enum has no `allowed_tools` carrier. Selecting the complete function inventory is
equivalent to `auto/required`; filtering a proper subset would remove declaration
visibility and is not selected. The [Responses guide](https://api-docs.deepseek.com/guides/responses_api/)
separately supports `text.format` and required selection. Chat restrictions
therefore belong to native Chat target projection, not public Chat intake,
canonical model flags, native Responses or the Bailian-hosted model.

## Session carriers — 2026-11-13

- OpenCode Go's [official integration contract](https://github.com/anomalyco/opencode/blob/a5170d6e019341864d948189d1420096005de8e0/services/site/src/docs/content/console/go.mdx#L112-L116)
  names a stable per-conversation `x-opencode-session` and an application
  User-Agent for routing and prompt caching. It does not require a UUID, a
  `ses_` prefix, a 30-character wire limit or a body `session_id`.
- The [stable OpenCode request builder](https://github.com/anomalyco/opencode/blob/v1.19.5/packages/opencode/src/session/llm/request.ts#L186-L205)
  sends native session headers on its OpenCode-provider path instead of the
  generic fallback headers. The [current builder](https://github.com/anomalyco/opencode/blob/a5170d6e019341864d948189d1420096005de8e0/packages/core/src/session/model-request.ts#L321-L336)
  distinguishes actual session identity from parent/fork
  [affinity](https://github.com/anomalyco/opencode/blob/a5170d6e019341864d948189d1420096005de8e0/packages/core/src/session/affinity.ts#L6-L8).
  `x-opencode-request` changes per request and is not a conversation carrier.
- OpenRouter's [official caching guide](https://openrouter.ai/docs/guides/best-practices/prompt-caching#using-session_id-for-sticky-sessions)
  and [pinned documentation](https://github.com/OpenRouterTeam/docs/blob/a174c7eaf53336d04ef759ed1d9cfa25c16a199f/guides/best-practices/prompt-caching.mdx)
  accept body `session_id` or header `x-session-id`, with body precedence and a
  256-character session limit. Chat and Responses accept both carriers.
  Explicit sessions activate stickiness on successful requests before a cache
  hit; absent sessions fall back to `prompt_cache_key` and cache-observed
  activation. Sessions also group observability, so these inputs are not
  semantically interchangeable. Provider availability, manual order and expiry
  keep stickiness best effort.
- Current Pi's Responses serializer sends a stable `session-id` alongside
  `prompt_cache_key`. Public SIWC documentation does not establish that header
  as mandatory. Controlled service observations with fixed synthetic bodies
  and keys establish it as an effective cache-affinity carrier for Luna and
  Sol, with raw cache counts preserved by the codecs. This is target-scoped
  behavior, not a universal cache-hit or backend-worker guarantee.

The accepted [projection contract](../architecture/runtime-and-routing.md#upstream-session-projection)
owns derivation, ingress precedence and scope. These references do not activate
additional endpoints or credential lifecycle operations.

## Replay identity and carrier

A bound target's source identity and its protocol carrier are separate facts. Projection and final request/response/event checks must resolve the same identity. The explicit library origin wins for strict target resolution; otherwise the bound codec scope is used. Native reasoning then uses the separately bound service-source scope when present, while compaction and other strict attachments retain target-specific identity. An unbound reasoning scope alone grants no replay authority.

A catalog boolean that only suppresses the explicit target origin cannot establish that an upstream has no reasoning carrier: decoding can still issue a trusted opaque owner. Carrier qualification stays with the protocol/dialect, and unassessed history acceptance delegates to upstream validation within the existing trust boundary.
