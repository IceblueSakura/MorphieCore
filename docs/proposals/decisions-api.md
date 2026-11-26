# Decision API Extension Proposals

This page covers extensions to the [delivered native Python profile](../usage/api-reference.md#decisions), not a second baseline spec. [Architecture](../architecture/index.md#decision-api-direction) owns the independent Decision IR and public SDK families; the [roadmap](../roadmap/index.md#stage-4-pending-scope) owns execution order, and the [issue register](../roadmap/unscheduled-issues.md#decision-qualification) owns unscheduled gaps. An extension is not a binding or a support claim until its direction passes verification.

## Gemini native execution

The next slice is text-only `gemini-decision-1` at Google's `POST /v1beta/decisions`. The [contract evidence](../refs/2026-11-25-decision-contracts.md#google-gemini-decision-1-p1) and [direct predicate probe](../refs/2026-11-25-decision-contracts.md#native-qualification-evidence) establish a callable endpoint, not a gateway codec, binding, or client mapping. Use the [locked upstream sources](../../tests/upstream/sources.lock.json) and add a distinct codec: keyed questions resemble System One, but Gemini owns its input parts, binary criteria, usage and whole-request blocking. Reuse neutral Interactions content leaves where their meanings agree. Do not add a third downstream SDK family or activate a public model as part of the upstream slice.

| Contract | Gate for the native slice |
|---|---|
| Ordered text evidence and criteria | Preserve part order and Gemini's concatenation contract. Check shared-input and question-position rules independently. Treat structured JSON serialization, role removal and media as separate projections, not implicit text handling. |
| Noul | Carry true/false descriptions without folding them into instructions. Validate the reported `[0.01, 0.99]` value and native confidence as distinct facts; never unclamp or substitute one for the other. |
| Choice | Validate UTF-8 option ordering and target tie-breaks before restoring source slots. Associate the complete keyed answer set through a request-bound expectation; reject missing, duplicate or extra keys. |
| Score and usage | Keep ordinal rubric order, validate native probabilities, reported score and confidence against the locked contract, and preserve reported totals and modality details without inventing counts. |
| Block and failure | Represent an explicit whole-request block separately from per-question refusal. Empty answers without block evidence, contradictory answers plus block evidence, and incomplete EOF are errors, not evaluated results. |

Exit with directional codec expectations, a synthetic gateway-to-upstream exchange covering evaluation and block/failure cases, and a real mixed three-primitive native request. The predicate probe alone does not close this slice.

## Cross-family directions

No off-diagonal route is enabled. Select and qualify one source, target, evidence carrier and primitive subset at a time; qualification in one direction does not authorize the reverse.

| Downstream SDK | Pending upstream directions |
|---|---|
| OpenAI | TypeSafe System One; Gemini after native execution qualifies |
| TypeSafe | OpenAI Decisions; Gemini after native execution qualifies |

Before activating a direction, classify its fields under the [conversion ladder](../architecture/adaptation.md#conversion-ladder). Start with the smallest faithful subset. Keep request preparation and answer association coupled: emitted keys, target option order and rubric expectations must be recorded with the request and checked before shaping a client answer.

| Open choice | Review rule |
|---|---|
| Evidence and criteria | Moving role-less state into user history introduces role framing; converting history to state records loses native authority. Serializing structured evidence or embedding binary criteria in instructions is not an exact conversion. Use a named, explicitly selected approximation or reject. Never reparse business keys as protocol fields. |
| Typed options and rubrics | Preserve typed identity (`true` is not `"true"`), option association and the original ordinal scale. Alternate labels or rendered rubrics change model-visible input; require explicit selection and validate the target result before reversing the mapping. |
| Confidence, model and usage | Do not copy opaque confidence into a different statistic or synthesize a required confidence by default. Select and validate any derivation separately; otherwise reject an unrepresentable answer. Keep source measurements distinct from any selected presentation rule and never fill missing required reports with guessed values. |
| Blocks and refusals | Do not turn a blocked request into per-question refusals, drop refused slots, or publish partial success. When the client has no faithful carrier, require a non-retryable client-native error. |
| Isolation, context and overflow | No implicit regrouping, context loss or silent truncation. A target that shares questions or truncates text requires verified behavior and a separately selected policy, not a byte estimate or a retry with changed input. |

A public direction needs independent request and result expectations, negative tests for uncarriable fields and outcomes, and an actual pinned downstream SDK loop against the intended target. Preserve complete JSON publication after EOF and the existing single-target lifecycle.

## Deferred extensions

These are research or review items, not Gemini prerequisites or current activation scope. Re-enter only for a selected consumer and a pinned target contract.

| Extension | What is still needed |
|---|---|
| Additional SDK profiles | Qualify actual JavaScript/TypeScript releases separately. Verify URL joining, discovery, validation errors, compression and retry defaults, typed answers, and cancellation. Python acceptance does not transfer. |
| Wider native evidence and controls | Select OpenAI non-user history and inert historical tool results, structured prompts/rubrics, cache boundaries/retention and attribution, or TypeSafe binary criteria/extended context independently. Keep historical calls inert, cache hints separate from identity, and explicit opt-ins separate from the portable default. |
| Media and `SingleBox` | Pin carrier and byte limits for each position, including TypeSafe assets/multipart and Gemini input parts versus text-only instructions. Keep resource identity, occurrence and exact image frame distinct. Preserve media presentation and native echo spelling; a TypeSafe rectangle is not an existence claim. Provider Files URIs require the [Files & Uploads authorization contract](files-and-uploads.md); inline media does not. |
| Perplexity Decider and `TextLocate` | Pin grouping and state billing per exact version, including whether questions share context. Before `locate`, pin the owner, text view, offset unit and endpoint convention; test a repeated multibyte substring and distinguish valid empty spans from refusal. Never repair offsets with substring search. |
| Cloudflare Clef | Qualify hosted confidence, REST/account scoping, and the text-state truncation boundary before selecting a binding. An isolated target or a context-size estimate does not prove the requested input survives intact. |
| Qwen, Microsoft, Liquid, Inception, OpenRouter | Resolve [primary-schema and wire discrepancies](../refs/2026-11-25-decision-contracts.md#new-and-updated-p2-provider-observations) before selecting a binding. Keep Liquid multi-label marginals, quantile ranges, and extraction/detection, and Inception keyed extraction separate from categorical Choice, ordinal Score and single-box localization. Do not add reject-only IR variants. |

Keep new native limits distinct from gateway resource ceilings, and do not weaken known-field validation to reach nominal vendor capacity. Stateful continuation remains [frozen](../architecture/history-and-replay.md#stateful-history). Incremental answers or batch execution would need a distinct selected protocol; HTTP chunking alone is not an answer stream.
