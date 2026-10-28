# Generation source distinctions

Research began **2026-10-17**; the dated observations below retain their original scope. Current contracts live in [Architecture](../architecture.md), current implementation evidence in [Development](../development.md#verification-gates), and source navigation in the [official SDK guide](2026-10-27-official-sdk-sources.md). This note retains distinctions not replaced by those documents, rather than a second feature inventory.

## Google foreground contracts, 2026-10-20

Sources: [Interactions v1 reference](https://ai.google.dev/api/interactions-api-v1), then-inspected SDK [`78f6d34`](https://github.com/googleapis/python-genai/tree/78f6d3412caaaed44f99ce57be2d16d0b8c407cb), [generateContent reference](https://ai.google.dev/api/generate-content) and [function-calling guide](https://ai.google.dev/gemini-api/docs/generate-content/function-calling#multimodal-function-responses).

- Interactions created/completed events carry nested resources; status updates are a different shape. The status domain includes `in_progress`, `requires_action`, `completed`, `failed`, `cancelled`, `incomplete`. An `interaction.completed` event can carry `requires_action`; the event name alone does not mean an ordinary completed answer.
- Opening resources may omit identity/status. Bind actual later evidence rather than manufacturing an ID or terminal state.
- `avgLogprobs` and `logProbabilitySum` are distinct candidate scoring reports. Neither is a terminal explanation, and neither has a safe inferred streaming reduction rule.
- Multimodal function responses need native linkage, not merely matching byte types; the [media note](2026-10-12-multimodal-carriers.md#tool-result-media) retains the guide/schema/SDK discrepancy.

## Streamed score scope, 2026-10-20

The source review at SDK revision `364889a` distinguished three operations:

| Evidence | Established meaning | Not established |
|---|---|---|
| Developer API `LogprobsResult` | Candidate average and token sums indexed by decoding step | Whether ordinary SSE scores cover fragments or cumulative output |
| SDK streamed intake (`models.py`) | Parses and yields native responses individually | A shared candidate-score accumulator |
| Continuation helper | Adds numeric sums across continuation hops | A single-request SSE frame reducer |

A finish reason does not establish whole-output score coverage. Repeated/missing fields cannot be reduced safely without knowing their scope. The [aggregate-score proposal](../api-expansion-design.md#google-output-report-owners-unselected) remains separate from accepted token-probability carriage. The [current SDK guide](2026-10-27-official-sdk-sources.md#google-gen-ai) supplies newer converter/transport entry points without upgrading this historical observation into new acceptance.

## Google text-report source recheck, 2026-10-21

Sources read: [Interactions v1 OpenAPI](https://ai.google.dev/static/api/interactions-v1.openapi.json), [v1beta OpenAPI](https://ai.google.dev/static/api/interactions.openapi.json) and generateContent reference. These URLs are mutable; later artifact fingerprints belong to [sources.lock.json](../../tests/upstream/sources.lock.json), not this earlier observation.

| Carrier | Established source fact | Boundary |
|---|---|---|
| Interactions `service_tier` | v1 declares optional `standard`, `flex`, `priority` on requests and resources | Status updates have no tier field |
| generateContent `usageMetadata.serviceTier` | Reports service tier | Response metadata, not a token counter |
| Candidate `finishMessage` | Optional explanation alongside finish reason | Candidate-scoped, not an operation error |

## Accounting source distinctions

Retained from the October 17–21 reviews; [scoped accounting](../architecture.md#scoped-accounting) owns the selected IR interpretation.

- generateContent reports prompt input, tool-use input, generated output and thoughts separately; cached input is a prompt subset. Do not flatten these into OpenAI's buckets before establishing the reported relationships.
- Interactions may report a total whose relationship to input/output/tool-input is not established. In particular, do not turn a residual into an invented `internal` counter or sum tool input by analogy with generateContent.
- Messages input, cache-read and cache-write categories are disjoint. Cache-write duration details and thinking-as-output-subset are separate relationships, not aliases for another protocol's counters.

## Remaining dated cross-protocol evidence

- **2026-10-18 target review:** Qwen total-output versus answer-only ceilings and ignored controls are retained in the [Bailian source note](2026-10-19-bailian-compatible-requests.md). DeepSeek developer-role demotion is retained in the [instruction-authority review](2026-10-25-instruction-authority.md). Client strictness/cache/compaction observations are retained in the [client guide](2026-10-27-client-and-proxy-sources.md#earlier-client-observations).
- **2026-10-18 Messages review:** Haiku 5.5 thinking replay was documented as issuing-account-bound. This is not evidence of cross-account portability; preserve the [replay provenance boundary](../architecture.md#reasoning-replay) and recheck the target before new acceptance.
- **2026-10-19 attribution review:** OpenAI `safety_identifier` (128 characters) and deprecated `user`, Messages `metadata.user_id` (512 characters), and Google `labels.safety_identifier` (63-character label grammar) are distinct native carriers. They are neither cache keys nor permission to lowercase/truncate an identity. [End-user attribution](../architecture.md#end-user-attribution) owns the current mapping.
- **2026-10-21 Anthropic review:** beta task budgets are advisory loop controls, not enforceable response ceilings; document `context` is model-visible text, not a citable source block; automatic caching selects eligible tails and excludes deferred tool definitions from the system prefix. Recheck the version-specific [SDK unions](2026-10-27-official-sdk-sources.md#anthropic) before admitting these semantics.

Earlier implementation claims about automatic-cache coexistence, annotation insertion, unsigned reasoning and broad signature transfer have been removed. Use [cache admission](../architecture.md#owner-local-cache-admission), [standard request boundaries](../architecture.md#standard-request-boundary) and [replay](../architecture.md#reasoning-replay), not superseded compatibility behavior.
