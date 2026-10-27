# Generation contract boundaries

Researched **2026-10-17** and subsequent checkpoints against official protocol specifications, SDK sources, and model documentation. [Architecture](../architecture.md) owns accepted design, and [Roadmap](../roadmap.md) owns execution order.

## Contents

- [Sources](#sources)
- [Manual model enumeration](#manual-model-enumeration-2026-10-22)
- [Exact carriers and source validation](#exact-carriers-and-source-validation)
  - [Google foreground contracts (2026-10-20)](#google-foreground-contracts-2026-10-20)
  - [Streamed score scope (2026-10-20)](#streamed-score-scope-2026-10-20)
  - [Google text-report source recheck (2026-10-21)](#google-text-report-source-recheck-2026-10-21)
  - [P7 binding evidence](#p7-binding-evidence-2026-10-18)
  - [Messages owner-local cache admission](#messages-owner-local-cache-admission-2026-10-19)
  - [Anthropic input and cache recheck](#anthropic-input-and-cache-recheck-2026-10-21)
  - [Responses lifecycle reports (2026-10-19)](#responses-lifecycle-reports-2026-10-19)
  - [Responses text-container presence (2026-10-20)](#responses-text-container-presence-2026-10-20)
  - [Function strictness defaults](#function-strictness-defaults)
  - [Responses media and error metadata](#responses-media-and-error-metadata)
  - [Signed Google parts](#signed-google-parts)
- [Client acceptance and host reacceptance](#client-acceptance-and-host-reacceptance)
- [Scoped accounting contract](#scoped-accounting-contract)
- [Selected visible-history transfer](#selected-visible-history-transfer)
- [Messages budget and identity boundaries](#messages-budget-and-identity-boundaries-2026-10-19)
- [Model-effective limits](#model-effective-limits-2026-10-20)
- [Cross-provider user attribution](#cross-provider-user-attribution-2026-10-19)
- [Contract and evidence map](#contract-and-evidence-map)

## Sources

- **OpenAI**: [Responses](https://developers.openai.com/api/reference/resources/responses/methods/create), [Chat](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create), [reasoning guide](https://developers.openai.com/api/docs/guides/reasoning), Python SDK 3.37.0.
- **Anthropic**: [Messages](https://platform.claude.com/docs/en/api/messages), [prompt caching](https://platform.claude.com/docs/en/build-with-claude/prompt-caching), Python SDK 2.1.0, TypeScript SDK 1.1.0.
- **Google**: [generateContent](https://ai.google.dev/api/generate-content), [Interactions v1](https://ai.google.dev/api/interactions-api-v1), [v1beta](https://ai.google.dev/api/interactions-api), Python SDK 2.33.0.

## Manual model enumeration, 2026-10-22

Catalog bindings distinguish models with verifiable endpoints and declared capacities from generic OpenAI-compatible wrappers. Default model-list discovery paths vary across providers; bindings rely on explicit target declarations.

## Exact carriers and source validation

### Google foreground contracts, 2026-10-20

- **Interactions Lifecycle Resources**: [Interactions v1 reference](https://ai.google.dev/api/interactions-api-v1) and pinned SDK [`78f6d34`](https://github.com/googleapis/python-genai/tree/78f6d3412caaaed44f99ce57be2d16d0b8c407cb) distinguish nested created/completed resources from top-level status updates.
- **Status Union**: `in_progress`, `requires_action`, `completed`, `failed`, `cancelled`, `incomplete`. Tool-call streams conclude with `interaction.completed` carrying `requires_action`.
- **Tool Result Structure**: [Function calling guide](https://ai.google.dev/gemini-api/docs/generate-content/function-calling#multimodal-function-responses) documents `response.output`/`error` structures. Multimodal tool results require explicit linkage; see [multimodal source discrepancies](2026-10-12-multimodal-carriers.md#tool-result-media).
- **Log Probabilities**: Google proto declares `avgLogprobs` and `logProbabilitySum` as separate reported aggregates. Discarding them loses distinct candidate scoring facts. Terminal explanations are owned by dedicated reports.
- **Sparse Identity Binding**: Intake admits partial opening where creation ID/status are omitted, binding the first valid real identity observed rather than synthesizing state.

### Streamed score scope, 2026-10-20

Evaluated streaming score aggregation boundaries against SDK `364889a`:

| Evidence | Established meaning | Not established |
|---|---|---|
| Developer API `LogprobsResult` | Candidate average and token sums indexed by decoding step | Whether SSE chunk scores describe fragments or cumulative outputs |
| SDK streamed intake (`models.py`) | Parses and yields native responses individually | No shared score accumulator |
| Continuation helper | Adds numeric sums across continuation hops | Not an ordinary SSE frame reducer |

Stream sums cannot be aggregated without explicit knowledge of whether frames are disjoint fragments or cumulative snapshots. Terminal explanations remain separate from score aggregation.

### Google text-report source recheck, 2026-10-21

Rechecked v1 and v1beta OpenAPI specifications (`2026-10-21`):

| Carrier | Established source fact | Boundary |
|---|---|---|
| Interactions `service_tier` | v1 declares optional `standard`, `flex`, `priority` on requests and resources | Status updates have no tier field |
| generateContent `usageMetadata.serviceTier` | Lists `standard`, `flex`, `priority` | Response metadata, not token counter |
| Candidate `finishMessage` | Optional explanation populated alongside a finish reason | Candidate-scoped; not an operation error |

### P7 binding evidence, 2026-10-18

- **DashScope Qwen**: `max_completion_tokens` covers total output (thinking + response); `max_tokens` covers answer only. Qwen 3.8 Flash/Max bindings preserve total-cap semantics.
- **DeepSeek Responses**: Responses API treats developer input as user, accepts summary without generation, and ignores `parallel_tool_calls`.
- **Client Strictness**: pi `supportsStrictMode: false` omits the field; OpenCode explicitly emits `strict: false`. Omitted Responses strictness defaults to schema normalization.
- **Messages / Go**: pi defaults to short cache retention; OpenCode sets up to 4 inline markers. Haiku 5.5 restricts thinking blocks to issuing accounts.

### Messages owner-local cache admission, 2026-10-19

- Explicit breakpoints limited to four markers in `tools → system → messages` order.
- TTL values: `5m` (default) and `1h`. Automatic caching alongside explicit markers or conflicting TTLs is rejected.
- All-native configured candidates forward markers faithfully; mixed routes remain closed.

### Anthropic input and cache recheck, 2026-10-21

- **Task Budgets**: Advisory loop budgets in beta (`output_config.task_budget`); not enforceable response ceilings.
- **Client Toolsets**: `(toolset_name, name)` pairs in tool calls are validated before filtering.
- **Document Context**: `context` in document blocks reaches the model as text, not citable source blocks.
- **Automatic Caching**: Targets eligible tails; deferred tool definitions are excluded from the system prefix.

### Responses lifecycle reports, 2026-10-19

- OpenRouter and OpenAI Responses distinguish requested `service_tier` from effective reported processing modes (`default`, `flex`, `priority`).
- MorphieCore enforces phase-tagged reports and preserves opening/terminal snapshot transitions.
- OpenCode assistant history shorthand: Assistant text grouped by item ID/phase emits `output_text` without `annotations`. MorphieCore accepts input-only history shorthand without relaxing upstream output constraints.

### Responses text-container presence, 2026-10-20

- Pinned OpenAI Python response type marks `text` optional/nullable; `Presence<()>` preserves absent vs null containers, preventing loss of explicit `text: null` reports.

### Function strictness defaults

- OpenAI Chat defaults to non-strict (`strict: false`).
- Responses omits `strict` by default to trigger schema normalization. MorphieCore preserves requested policy separately from observed effective strictness.

### Responses media and error metadata

- Nullable `file_id` on image/file inputs names no resource; non-null values require credential-owned file APIs and are rejected if unsupported.
- `misalignment` error details in Responses are preserved or rejected strictly rather than silently lost.

### Signed Google parts

- Single frames with mixed signed and unsigned parts are preserved with distinct boundaries.
- `Part.replay` and `ToolCall.replay` own thought/call signatures directly. Mismatched or dropped signatures degrade gracefully when permitted by provider rules.

## Client acceptance and host reacceptance

Downstream client profiles (pi and OpenCode), sustained-operation boundaries, synthetic suites, and real Token Plan acceptance are detailed in the dedicated note:
- [Downstream client profiles and host acceptance](2026-10-21-client-acceptance.md)

## Scoped accounting contract

IR retains one latest typed `UsageReport` per operation or response-local step.

Let $P$ = prompt input, $U$ = tool-use input, $O$ = generated output, $R$ = thoughts, $C$ = cached input:
- **generateContent**: $Total = P + U + O + R$, with $C \subseteq P$. Buckets are stored independently.
- **Interactions**: $Total = P + O + \text{internal}$. Tool input relationships remain unsummed unless explicitly evidenced.
- **Anthropic Messages**: Disjoint input, read, and write categories with `cache_creation = 5m + 1h`. `thinking_tokens` is an output subset.

## Selected visible-history transfer

Cross-model transfer preserves user messages, assistant text, and tool call/result pairs, while stripping foreign thinking blocks and model-bound signatures. Compatible replay retains strict verification; incompatible thinking is cleanly omitted before grouping to prevent empty turns.

## Messages budget and identity boundaries, 2026-10-19

- Manual thinking budgets (`budget_tokens`) require $1024 \le \text{budget} < \text{max\_tokens}$.
- `metadata.user_id` permits up to 512 characters for abuse attribution without affecting cache grouping.

## Model-effective limits, 2026-10-20

Gemini 3.8 Flash defines 65,536 maximum output tokens and low/medium/high thinking levels (rejecting minimal). Trusted target caps enforce these limits at preflight without silent clamping.

## Cross-provider user attribution, 2026-10-19

Attribution carriers across protocols:

| Protocol | Carrier | Constraints |
|---|---|---|
| OpenAI Chat / Responses | `safety_identifier` (128 chars); deprecated `user` | Abuse attribution; separate from cache keys |
| Anthropic Messages | `metadata.user_id` (512 chars) | End-user attribution |
| Google generateContent | `labels.safety_identifier` (63 chars) | Regex: `[\p{Ll}\p{Lo}\p{N}_-]{0,63}` |

Shared attribution prioritizes explicit safety identifiers over legacy user fields without lowercase mutations or truncation.

## Contract and evidence map

| Area | Current owner / boundary |
|---|---|
| History, roles, tool pairing | `Item`, message envelopes, `ClientManaged` |
| Tools and schemas | Typed definitions, exact JSON, strictness defaults |
| Reasoning and replay | Dedicated replay slots, signed part preservation |
| Media input | Resource tables, inline data, tool-result media |
| Attribution | Exact native fields, bounded safety projection |
| Usage | Typed source-scoped reports, exact modality breakdown |
