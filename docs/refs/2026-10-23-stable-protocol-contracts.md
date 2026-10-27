# Stable-release protocol contracts

Checked **2026-10-23** against official releases, SDK source, and current API documentation. The [source lock](../../tests/upstream/sources.lock.json) owns exact release revisions; [Architecture](../architecture.md) owns accepted design, and [Roadmap](../roadmap.md#standards-reset-and-redesign) owns execution.

## Contents

- [Source selection](#source-selection)
  - [Latest-candidate review (2026-10-24)](#latest-candidate-review-2026-10-24)
- [Anthropic API 2026-10-21](#anthropic-api-2026-10-21)
- [Prefix-cache boundary ownership](#prefix-cache-boundary-ownership)
- [Participant labels and ordinary direct tools](#participant-labels-and-ordinary-direct-tools)
- [OpenAI Responses strictness](#openai-responses-strictness)
- [Google API versions and semantic differences](#google-api-versions-and-semantic-differences)
- [Remaining mapping recheck](#remaining-mapping-recheck)
- [Consumer and fixture evidence](#consumer-and-fixture-evidence)
- [Official contract difference inventory](#official-contract-difference-inventory)
  - [OpenAI](#openai)
  - [Anthropic](#anthropic)
  - [Google](#google)
- [Instruction authority research](#instruction-authority-research)
- [Imported asset boundaries](#imported-asset-boundaries)

## Source selection

Pins use registry stable releases and matching tags, not HEAD branches. OpenAI Python 3.37.0 resolves to `e8a0fd28bbdc1699c97ff0f602b1af3dd378382d`. Vercel package tags resolve to repository revision `594495a5af70547d9d7a13d6df334ae30db49622`. Anthropic Python 2.1.0 and TypeScript 1.1.0 use Apache-2.0. Imported licenses and byte digests are locked in [assets.lock.json](../../tests/upstream/assets.lock.json).

### Latest-candidate review (2026-10-24)

Candidate drift evaluated using `tests/upstream/candidates.py` across 14 release sources: 11 remained unchanged, and 3 exhibited candidate releases. No pins were automatically promoted.

| Candidate | Source delta and boundary |
|---|---|
| Anthropic Python 2.3.0 (`202db72c87d7`) | Adds server-held threads and cost APIs; beta route qualifiers replace `beta=true`. Ordinary Messages types unchanged |
| Anthropic TypeScript 1.3.0 (`cb5fba419808`) | Adds threads and tool-runner plumbing. Ordinary Messages types unchanged |
| Vercel `ai` 7.0.152 (`2deca8c46f62`) | Gateway dependency bumps; `packages/ai/src` unchanged. OpenAI/Anthropic/Google packages unchanged |

The verified **2026-10-23 baseline** remains in effect. Thread APIs, automatic tool execution, and unselected providers remain deferred.

## Anthropic API 2026-10-21

Sources: [migration guide](https://platform.claude.com/docs/en/api/migration-2026-10-21), Python 2.1.0 source.

- `temperature`, `top_p`, and `top_k` are removed from Messages requests; sending them returns 400.
- Direct documents in assistant messages reject; documents nested in tool results remain supported.
- HTTP header `anthropic-version: 2026-10-21` is required by default; MorphieCore enforces this header strictly before body collection.
- `MessageParam` is role-discriminated.
- Ordinary tools declare `allowed_callers` (defaults to `["direct"]`) and optional `type: "custom"`.
- `transformations.oversized_image: "error"` rejects silent downscaling.

## Prefix-cache boundary ownership

Rechecked **2026-10-24** at pinned OpenAI Python revision.
- Responses `prompt_cache_breakpoint: {mode: "explicit"}` on text, image, and file inputs inherits `prompt_cache_options.ttl` (default `30m`).
- Responses function output items report optional, nullable breakpoints.
- Anthropic `cache_control` ephemeral markers declare owner TTL (`5m` or `1h`), defaulting to `5m`.

## Participant labels and ordinary direct tools

- OpenAI Chat messages (`system`, `developer`, `user`, `assistant`) declare optional non-null `name`, distinguishing participants with identical roles.
- Anthropic tools default to direct calling (`allowed_callers: ["direct"]`). Non-direct and hosted discriminators reject at ingress.

## OpenAI Responses strictness

- Explicit `strict: true` requires rigid schema compliance.
- When `strict` is omitted, the service attempts normalization and falls back to best-effort (`strict: false` reported). Chat omission remains non-strict.

## Google API versions and semantic differences

- Interactions v1 omits sampling controls; current generateContent models ignore sampling on Gemini 3.6+.
- Interactions signatures exist on thought/built-in steps, not ordinary function calls.
- `responseFormat.text.{mimeType, schema}` replaces older carriers while retaining deprecations.
- generateContent `store` controls logging; Interactions `store` controls resource continuation.
- Thinking budgets (`thinkingBudget` int32 vs `thinking_level`) are validated per target model capability.

## Remaining mapping recheck

Rechecked **2026-10-24**:
- OpenAI Responses/Chat schema configuration descriptions and containers are non-null.
- Anthropic tool results permit nullable cache markers, but `is_error` and effort/format remain non-null.
- Google Interactions v1 accepts `function_result.result` as string, object, or text/image array. generateContent tool-result media rejects due to missing `$ref`/`displayName` support in the public API.
- Interactions streaming accepts initial empty thought signatures followed by real deltas, completing at `interaction.completed`.

## Consumer and fixture evidence

- SDK permissive parsing does not imply gateway compatibility. Anthropic stream accumulator only retains enumerated delta fields in its final message.
- OpenAI Responses classified error events require `message`, `sequence_number`, and `type: "error"`. Gateway classifications use standalone `error` events rather than fabricated response objects.

## Official contract difference inventory

Bounded inventory of request, response, event, and HTTP boundaries checked **2026-10-23** (updated **2026-10-25**).

### OpenAI

| Boundary / Minimal example | Behavior and evidence | Classification |
|---|---|---|
| Responses `stream_options` sibling fields | Directional field admission matching Chat | Corrected 2026-10-25 |
| Chat `prediction: {type: content}` | Retained until target selection; supported Chat is exact | Corrected; catalog support explicit |
| Chat function parameters/description null, long name | Strict 64-char name limit, non-null descriptions | Corrected 2026-10-25 |
| Responses `prompt_cache_breakpoint` | Inherited boundary owned by `Part.cache` | Corrected; native TTL distinct |
| Chat history participant `name` | Retained through context edits and request encoding | Corrected; unsupported targets reject |
| Responses schema `oneOf` without `strict` | Omission preserved for service fallback | Corrected; no invented false report |
| Format / schema descriptions null | Strict rejection of null request format controls | Corrected |
| Gateway error before frame or post-publication | Emits standalone `error` event with continuous sequence | Corrected standard error carrier |
| Chat `n > 1` or multiple choices | Single candidate supported | Explicit coverage exclusion |
| Moderation controls/reports | Rejection at intake | Known safety-report gap |

### Anthropic

| Boundary / Minimal example | Behavior and evidence | Classification |
|---|---|---|
| Tool ID > 64 chars or invalid label grammar | Strict public grammar validation before publishing | Corrected 2026-10-25 |
| Unsigned reasoning returned in history | Compatibility-first summary projection with declared loss | Implemented 2026-10-25 |
| Version header `anthropic-version: 2026-10-21` | Required before request body collection | Corrected HTTP/version gate |
| Removed sampling parameters (`temperature` etc.) | Immediate 400 rejection at intake | Corrected |
| Text citations in assistant history | Rejection at shape validation | Explicit citation gap |
| Ordinary tools `allowed_callers: [direct]` | Normalizes direct-only to inactive hosted dispatch | Corrected |
| Tool `eager_input_streaming` or `cache_control` null | Null normalized to absent owner | Corrected |
| Top-level controls null | Strict rejection of non-nullable request controls | Corrected 2026-10-24 |
| Multiple leading instructions | Ordered composition preserving system boundaries | Corrected |
| Thinking `display: null` | Rejection at intake | SDK/schema discrepancy |
| Manufactured empty signed thinking blocks | Removed; native signatures preserved, missing rejected | Corrected 2026-10-24 |
| Stream block sequence safety | Rejects out-of-order block starts | Corrected |

### Google

| Boundary / Minimal example | Behavior and evidence | Classification |
|---|---|---|
| Interactions v1 sampling fields | Omitted by encoder; rejected for generic targets | Corrected |
| generateContent sampling on Gemini 3.8+ | Binding selects omission with declared field loss | Corrected model capability |
| Structured function result objects | Exact JSON preserved; native object carrier | Corrected |
| Empty assistant envelope | Rejected rather than silently dropped | Corrected 2026-10-24 |
| Interactions result subcontent (audio/video) | Rejection at boundary | Public schema disagreement |
| Thought signature opening empty placeholder | Accepted, followed by real deltas | Corrected |
| Inline generateContent tool-result media | Rejected due to missing public linkage support | Corrected |
| Interactions terminal EOF without DONE | Final envelope accepted at clean EOF | Corrected |

## Instruction authority research

Instruction authority analysis, legacy-to-modern role migration, provider dialect differences, and Vercel cross-checks are recorded in the dedicated research note:
- [Instruction authority and Provider dialects](2026-10-25-instruction-authority.md)

## Imported asset boundaries

[The asset manifest](../../tests/upstream/assets.lock.json) records source bytes and framing digests. [Development](../development.md#upstream-test-sources) owns execution and extraction rules.

- **OpenAI**: Tool and length streaming fixtures exercise argument fragments, tool identity, trailing usage, and terminal truth.
- **Anthropic**: Basic and tool fixtures append only missing delimiters; untouched inputs must fail strict EOF. `unknown_events_response.txt` exercises rejection of out-of-order block markers.
- **Vercel**: Chunks are framed from JSON payload lines; event names derive from existing discriminators without invented sequence numbers. Custom-tool streams without sequence numbers exercise negative grammar.
- **Google**: Text and Interactions static fixtures provide companion reference objects; Interactions streams exercise terminal EOF without DONE.
- **EventSource Parser**: Framing extracts literal chunk arguments from named test functions without executing TypeScript runtimes.
