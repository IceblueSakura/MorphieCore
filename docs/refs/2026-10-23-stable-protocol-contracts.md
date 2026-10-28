# Stable-release protocol contracts

Checked **2026-10-23**, with dated rechecks below, against official releases, SDK source, and then-current API documentation. The [source lock](../../tests/upstream/sources.lock.json) owns exact release revisions; [Architecture](../architecture.md) owns accepted design, and [Roadmap](../roadmap.md#standards-reset-and-redesign) owns execution.

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
- [Protocol distinctions](#protocol-distinctions)
- [Instruction authority research](#instruction-authority-research)
- [Imported asset boundaries](#imported-asset-boundaries)

## Source selection

Pins use registry stable releases and matching tags, not HEAD branches. OpenAI Python 3.37.0 resolves to `e8a0fd28bbdc1699c97ff0f602b1af3dd378382d`. Vercel package tags resolve to repository revision `594495a5af70547d9d7a13d6df334ae30db49622`. Anthropic Python 2.1.0 and TypeScript 1.1.0 use Apache-2.0. Imported licenses and byte digests are locked in [assets.lock.json](../../tests/upstream/assets.lock.json).

### Latest-candidate review (2026-10-24)

Candidates inspected on that date; no pins were promoted. The [2026-10-27 SDK guide](2026-10-27-official-sdk-sources.md#source-selection-and-licenses) supersedes this as a release-navigation entry point, not as a regression baseline.

| Candidate | Source delta and boundary |
|---|---|
| Anthropic Python 2.3.0 (`202db72c87d7`) | Adds server-held threads and cost APIs; beta route qualifiers replace `beta=true`. Ordinary Messages types unchanged |
| Anthropic TypeScript 1.3.0 (`cb5fba419808`) | Adds threads and tool-runner plumbing. Ordinary Messages types unchanged |
| Vercel `ai` 7.0.152 (`2deca8c46f62`) | Gateway dependency bumps; `packages/ai/src` unchanged. OpenAI/Anthropic/Google packages unchanged |

The verified **2026-10-23 baseline** remains in effect. Thread APIs, automatic tool execution, and unselected providers remain deferred.

## Anthropic API 2026-10-21

Sources: [migration guide](https://platform.claude.com/docs/en/api/migration-2026-10-21), pinned [Python 2.1.0 types](https://github.com/anthropics/anthropic-sdk-python/tree/9313f13a5a643ceed10fafba6785eab8e04b6e6e/src/anthropic/types).

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

- Interactions v1 omits sampling controls. For generateContent, schema presence and model-effective sampling support are separate checks; use the selected target contract rather than a family-wide version threshold.
- The pinned Interactions [thought step](https://github.com/googleapis/python-genai/blob/92f72b3a236a5bc56e8e3327ab9563f77dd119c3/google/genai/_gaos/types/interactions/thoughtstep.py) and built-in steps declare signatures; the ordinary [function-call step](https://github.com/googleapis/python-genai/blob/92f72b3a236a5bc56e8e3327ab9563f77dd119c3/google/genai/_gaos/types/interactions/functioncallstep.py) does not. Do not infer generateContent call-signature support from that separate union.
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

## Protocol distinctions

The October 23–25 review established several distinctions that remain useful beyond the repair:

- Responses omitted `strict` requests service normalization/fallback; it is not Chat's non-strict default. Required nullable response fields must be checked separately from request optionality.
- Prediction is an optimization, not an output constraint. Exact native carriage and omission on an unsupported target are different operations.
- Thinking activation, effort scope and display are independent facts. Messages can express disabled thinking plus response-wide effort; Google `minimal` does not establish disabled thinking. Client budget presets are [allocation policy](2026-10-27-client-and-proxy-sources.md#pi-actual-consumer-and-history-behavior), not universal level equivalence.
- Google `parameters` and `parametersJsonSchema` are different schema carriers; constrained function-choice modes are not OpenAI schema normalization.
- Matching service-tier names do not prove equal latency, billing or priority guarantees. Requested policy and reported effective service remain separate.
- Single-choice generation, unsupported moderation/citations and Google media restrictions are bounded implementation choices, not reasons to weaken known fields.

The old corrected/implemented field matrix is superseded by the [request counterexamples](2026-10-26-standard-request-boundaries.md) and [Client/IR source distinctions](2026-10-27-client-ir-boundaries.md). In particular, unsigned Messages reasoning is not a supported compatibility projection: current standard delivery requires genuine native replay/signature state. Accepted policies live in [Architecture](../architecture.md#field-handling).

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
