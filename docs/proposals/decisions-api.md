# Decision API Proposal

The [dual-SDK direction and capability-union principle](../architecture/index.md#decision-api-direction) are selected. The concrete realization below remains proposed, not approval of semantic layout, dialect behavior, SDK coverage, or bindings. The [2026-10-23 contract note](../refs/2026-10-23-decision-contracts.md) and [2026-11-25 expansion note](../refs/2026-11-25-decision-contracts.md) own protocol findings and discrepancies; the [roadmap](../roadmap/index.md#stage-4-pending-scope) owns ordering and gates.

See [Proposals Framework](index.md) for shared boundaries and [Roadmap](../roadmap/index.md) for Stage 4 execution gates.

## Contents

- [Decisions](#decisions)
  - [SDK surfaces and discovery](#sdk-surfaces-and-discovery)
  - [Decision ownership and wire families](#decision-ownership-and-wire-families)
  - [Decision semantics and compatibility](#decision-semantics-and-compatibility)
    - [Decision request and evidence union](#decision-request-and-evidence-union)
    - [Decision questions and evaluation requirements](#decision-questions-and-evaluation-requirements)
    - [Decision source results and accounting](#decision-source-results-and-accounting)
    - [Named Decision projections](#named-decision-projections)
    - [Decision reuse and review examples](#decision-reuse-and-review-examples)
  - [Decision execution and review choices](#decision-execution-and-review-choices)

## Decisions

### SDK surfaces and discovery

Provide both official downstream SDK families—OpenAI and TypeSafe—without monkey-patches, custom answer types, or a third public downstream schema. Google Gemini (`gemini-decision-1`), Cloudflare Clef, Perplexity Decider, Qwen, Microsoft Decision, and future decision models are upstream targets only. Clients may configure the gateway base URL, gateway credential, public model, and documented standard options. Stable access includes request serialization, typed answer consumption, model discovery, errors, and cancellation within an explicit feature/version profile; merely parsing a success body is insufficient.

The first native Python SDK profile uses the roots below. See the [public profile](../usage/api-reference.md#decisions) and [explicit activation](../usage/gateway-configuration.md#decision-models) for the implemented contract.

| Client | SDK base URL | Gateway operations |
|---|---|---|
| OpenAI | `http://127.0.0.1:<port>/v1` | `POST /v1/decisions`; retain existing OpenAI `GET /v1/models` and model retrieval |
| TypeSafe | `http://127.0.0.1:<port>/typesafe` | SDK-appended `POST /typesafe/v1/systemone` and `GET /typesafe/v1/models` |

Both official APIs use `GET /v1/models`, but TypeSafe returns `models:[{name,description,release_date}]` while OpenAI returns `object:"list",data:[...]`. The distinct TypeSafe root avoids header/User-Agent sniffing, merged response shapes, and replacing existing discovery. Discovery consumes explicitly activated public bindings, not upstream enumeration. JavaScript/TypeScript qualification remains separate.

TypeSafe Python `0.7.6` emits default zstd for bodies of at least 1 KiB when its compressor is available (the `zstd` extra before Python 3.14). The native profile accepts the SDK's single-frame zstd encoding after authentication, independently bounds wire bytes, decoder window and expanded bytes, and cancels decoding with the exchange. It sends ordinary JSON to the target. Pin JavaScript/TypeScript SDK releases and exercise their defaults separately before claiming support. A package pin alone does not qualify a gateway route.

Keep error delivery native to the selected client family, including authentication before body decoding, validation paths, unsupported-operation errors, overload/rate-limit status, and safe `Retry-After`. Do not relay private upstream error payloads. SDK retries are client behavior, not gateway retries; disable them in bounded acceptance runs and test error classification separately.

### Decision ownership and wire families

```text
OpenAI Decisions codec ──┐
                         ├─ Decision IR → target admission/projection
TypeSafe System One ─────┘              → upstream codec (OpenAI | System One | Gemini)
                                        → trusted transport
native result → source facts → request-bound validation → client delivery view
```

Keep official baselines under three distinct upstream wire owners: `openai/decisions`, `typesafe/systemone`, and `google/decisions` (`POST /v1beta/decisions`). Perplexity's `/v1/decisions` path does not make it OpenAI wire-compatible: its named questions and `state` belong to the System One-style family. Conversely, Google Gemini's keyed `questions` map does not make it System One: its `input` reuses Interactions multimodal `Content` blocks, `PromptFeedback`, and `Usage` (`input_tokens_by_modality`), warranting its own native codec reusing Interactions leaf decoders rather than forcing System One dialect inheritance. Prefer shared TypeSafe baseline mapping plus explicit Perplexity/Cloudflare/Qwen/Microsoft dialect differences only where primary schema evidence supports that relationship; use a separate native codec whenever a contract cannot be represented honestly as that baseline plus typed extensions.

Separate five responsibilities:

| Owner | Responsibility |
|---|---|
| Semantic task | Evidence, typed questions/answers, ordered association, and value invariants; no Provider or protocol dependency |
| Protocol codec / dialect | Native fields, unions, map/array representation, required/nullable rules, response envelopes, strict EOF, and documented deviations |
| Projection / delivery policy | Candidate-local lowering, explicit cross-family adaptations, atomic `Prepared { target, expectation, delivery, adaptations }` coupling, and derived client fields preserving source facts |
| Target contract | Versioned model contract across four orthogonal layers (below), bound in catalog/protocol outside Semantic IR |
| Adapter / execution / transport | Trusted endpoint/model/account binding, preparation, credentials, HTTP lifecycle, budgets, and publication |

Factor versioned target contracts in `morphiecore-protocol` / `morphiecore-catalog` into four orthogonal layers rather than a flat tuple:

1. **Exposure & overflow:** `question_visibility` (`Isolated` vs `SharedWithinNativeGroups`), native grouping boundaries, and `overflow_policy` (`Reject` vs `TruncateTextState`).
2. **Position × QuestionKind × Carrier matrix:** Distinct permitted carriers per position (shared evidence, role messages, question instructions, binary true/false criteria, choice descriptions, score levels)—for example, Gemini admits 5-modality `input` while restricting `instructions` to `TextContent`.
3. **Per-primitive evaluation & statistical rules:** Per-question-kind constraints including `SingleChoice` key sorting and tie-breaking (Gemini UTF-8 byte sort + alphabetical tie-break), `Predicate` output support domain (`[0.01, 0.99]` vs `[0, 1]`), option/level count bounds (`10` vs `255`), per-field rounding precision, and attested `ConfidenceMeaning`.
4. **Accounting & envelope:** `state_billing_unit` (`OncePerRequest`, `PerNativeGroup`, `PerQuestion`), cache inclusion semantics, `input_by_modality` completeness, REST envelope wrappers (Cloudflare `{result}`), compression, and opt-in headers.

Encoded requests and return-path expectations (such as wire question keys, Gemini's UTF-8 choice key permutation and reverse slot map, rubric echo expectations, and owner-bound media spelling records) must be produced by **one atomic `Prepared` pass**. Cloudflare's account-bearing path and REST envelope must not leak into semantic state. Preserve missing native directions rather than manufacturing a universal protocol DTO or generic plugin framework.

### Decision semantics and compatibility

**Capability-union principle selected; three-stage governance proposed.** Follow [Generation's unique-owner discipline](../architecture/index.md#semantic-ir), modeling by *what the question asks and what the answer proves* rather than collapsing distinct semantics because provider field names match. Separate three decisions explicitly:

1. **Design specification union:** Recognizes Core probability primitives (`Predicate`, `SingleChoice`, `OrdinalScore`), three-way `Evidence::{State, Parts, History}`, two-tier outcomes (`RequestDisposition`, `QuestionOutcome`), `SingleBox` (TypeSafe estimated `NormalizedRect` without existence promise), `TextLocate` (Perplexity plain-text span localization with explicit coordinate space), and clean sibling semantics for Liquid `MultiLabelChoice` (independent marginals, sum `!= 1`) and `QuantityRange` (median + `[p10, p90]` + unit). Concrete schemas for Liquid `TypedExtract` / `spans` / `box`/`boxes` detection and Inception `KeyedExtract` remain deferred until primary schemas exist; do not add `Extension(Value)` placeholders.
2. **Production Rust enum (staged to avoid reject-only placeholders):**
   - **Wave 1:** `Evidence::{State, Parts, History}` (text/JSON first), `Predicate`, `SingleChoice`, `OrdinalScore`, `RequestDisposition::{Evaluated, Blocked}`, `QuestionOutcome::{Answered, Refused}`, and `DecisionUsage` (with `ModalityTokenCount`).
   - **Wave 2:** Multimodal (`Image`, `Audio`, `Video`, `Document` PDF, `MediaPresentation`), `SingleBox`, and pinned `TextLocate`.
   - **Wave 3:** Sibling extensions (`MultiLabelChoice`, `QuantityRange`, etc.) once primary schemas are pinned and an admitted consumer path exists.
3. **Activated bindings:** Public routes expose only model/direction pairs that pass target admission and SDK verification.

One meaning has one authoritative field. `state` and `input` map to typed `Evidence` variants; `noul` and `predicate` map to `Predicate`; `criteria`, `choices`, and `levels` map to the corresponding typed question. Requested meaning, a reported fact, and a derived client value are three different authorities, not three competing request fields. Arbitrary business JSON is content; arbitrary protocol/options bags are not allowed.

#### Decision request and evidence union

The sketches below describe semantic ownership, not compilable Rust or a third wire API. Names may change at implementation; no source discriminator is selected by the client model name.

```rust
struct DecisionRequest {
    evidence: Evidence,
    resources: DecisionResources,
    questions: Vec<(QuestionId, Question)>,
    requirements: EvaluationRequirements,
}

enum Evidence {
    State {
        root: DecisionValue,
        side_inputs: Vec<ContextMediaGroup>,
    },
    Parts(Vec<EvidencePart>),
    History(Vec<(EvidenceId, EvidenceItem)>),
}

struct MediaUse {
    occurrence: OccurrenceId,
    resource: ResourceId,
    kind: MediaKind, // Image | Audio | Video | Document
    presentation: Option<MediaPresentation>, // ImageDetail (OpenAI) vs MediaResolution (Gemini)
}

enum DecisionValue {
    Null, Bool(bool), Number(ExactNumber),
    Text(RichText),
    Array(Vec<DecisionValue>),
    Object(Vec<(Text, DecisionValue)>),
    Media(MediaUse),
}

enum DecisionPrompt {
    Value(DecisionValue),
    Parts(Vec<PromptPart>), // Ordered text, JSON or media parts.
}
```

Use a three-way `Evidence` union (`State`, `Parts`, `History`) rather than forcing role-less Gemini `Content[]` sequences into `Conversation` user messages or flattening OpenAI `call_id`-linked tool history into plain `Content[]`. Wave 1 lowering projects `State` and `History` into `Parts` when targeting Gemini.

Keep three identities strictly separated:
1. **`ResourceId`:** Declared media source (`InlineBytes` with validated format or inert `PublicHttpUrl` where permitted). Defer `OpaqueProviderUri` (Gemini Files API URI) until Stage 4 `Files & Uploads` defines account/project authorization; inline PDF does not depend on Files API, and provider URIs must never fall back to public HTTP fetches or cross-account reuse.
2. **`OccurrenceId` / text owner:** Specific placement in `state`, `Parts`, `History`, or a question's `instructions`/`criteria`. Reusing one `ResourceId` across `state` and Q2 creates two distinct occurrences; deduplicating bytes never collapses occurrences.
3. **`CoordinateSpace`:** Exact `(owner, text_view, endpoint_convention, unit)` for text spans, or image frame for boxes. Decoded JSON strings, escaped JSON serializations, concatenated blocks, and placeholder-expanded strings are distinct text views; never cross-convert offsets across views or repair spans via `.find(text)`. Likewise, `NormalizedRect` never silently binds to "the first image" when multiple images exist.

`DecisionValue` is a bounded, duplicate-rejecting data tree with exact JSON numbers and preserved object/array order. Decouple Decision admission limits from [`JsonLimits::STRUCTURED`](../../crates/morphiecore-semantic/src/value/json.rs#L28-L32) (`8,192` nodes) and [`JsonLimits::ENVELOPE`](../../crates/morphiecore-semantic/src/value/json.rs#L17-L21) (`64 MiB`), explicitly distinguishing gateway resource ceilings, native counting units (e.g., OpenAI `10,000` values / `256 KiB` compact UTF-8), and decoded media sizes. Do not claim full native range if a smaller gateway limit is enforced.

Only the source codec recognizes TypeSafe placeholders or Perplexity media markers; OpenAI `input_json` keys such as `role` or `image_url` remain data. Resolve recognized media into resource uses once, keep bounded immutable `MediaSpelling` records outside the task bound to `(owner, occurrence, resource)` to restore TypeSafe `legend` spellings without leaking generated target aliases, reject ambiguous overlapping substitutions, and count both unique resource bytes and expanded uses.

`MediaPresentation` keeps OpenAI `ImageDetail` (`Auto`, `Low`, `High`, `Original`) distinct from Gemini `MediaResolution` (`Low`, `Medium`, `High`, `UltraHigh` on image; `Low`, `Medium`, `High` on video)—matching label names are not semantically equivalent. `ContextMediaGroup` preserves ordered uses within one modality and attested placement (`BeforeState` for Cloudflare images, `Unspecified` otherwise) without fabricating one global cross-modality sequence across separate audio/video arrays.

`EvidenceItem` has a closed union:

| Variant | Sole semantic payload |
|---|---|
| Instruction | System/developer authority and ordered text parts, at its original position |
| Message | User/assistant role, ordered content, optional assistant phase; user content admits text/JSON/media, assistant content admits text or historical refusal under current sources |
| FunctionCall | Call identity, name, optional namespace, exact raw argument text, and source-supported lifecycle |
| CustomCall | Call identity, name, optional namespace, exact raw input, and source-supported lifecycle |
| FunctionResult / CustomResult | Matching preceding call identity, ordered text/media evidence, and source-supported lifecycle |

Historical tool input is inert evidence, not a tool declaration or execution request. Preserve raw argument strings rather than parsing them into a second authority. Local evidence identity, optional native item ID, and call linkage are distinct; typed native identity/presence records remain owner-bound, never an unbounded replay sidecar.

Shared evidence parts may own an explicit cache boundary. Question instructions and rubric parts do not acquire one: OpenAI excludes them from the reusable prefix. Service intent lives in `DecisionServiceHints` outside business evidence, reusing neutral `CacheMode`/`CacheTtl` leaves without importing Generation's [`ExecutionHints`](../../crates/morphiecore-semantic/src/context.rs#L168-L176) (`store`, `background`, `max_tool_calls`):

| Hint | Meaning and boundary |
|---|---|
| Cache partition | Caller-provided advisory reuse key; not identity, an upstream cache ID, or a route-affinity override |
| Cache options | Implicit versus explicit-only boundary selection, plus **minimum** retention duration (`30m` is not a deletion deadline; explicit mode with no markers disables cache use) |
| Safety identifier | Opaque caller attribution; never authenticated Agent identity or authorization |
| Context allowance | Standard versus explicitly enabled extended context; not an exact token budget, permission to truncate, or evidence of equal accuracy |

Keep all fields private behind validating constructors, expose borrowed views only, and use redacted `Debug` implementations (mirroring [`EmbeddingInput`](../../crates/morphiecore-semantic/src/task/embedding.rs#L32-L38)) on evidence, media sources, options, and block details so business payloads never leak into telemetry.

#### Decision questions and evaluation requirements

```rust
struct Question {
    name: Option<Text>,
    instructions: Presence<DecisionPrompt>,
    kind: QuestionKind,
}

enum QuestionKind {
    // ── Wave 1 Core Tier ──
    Predicate {
        criteria: Presence<BinaryCriteria>,
    },
    SingleChoice {
        options: Vec<ChoiceOption>,
    },
    OrdinalScore {
        levels: Vec<ScoreLevel>,
    },
    // ── Wave 2 Sibling Extensions (admitted once pinned & consumer-backed) ──
    // SingleBox,
    // TextLocate { target: PlainTextOwner },
    // ── Wave 3 Sibling Extensions (design-recognized, deferred from Rust enum) ──
    // MultiLabelChoice { options: Vec<ChoiceOption> },
    // QuantityRange { unit: Option<Text> },
}

struct BinaryCriteria {
    when_true: Presence<DecisionPrompt>,
    when_false: Presence<DecisionPrompt>,
}
struct ChoiceOption {
    value: ChoiceValue, // String or Boolean; typed equality.
    criterion: Presence<DecisionPrompt>,
}
struct ScoreLevel {
    label: Option<Text>,
    criterion: Presence<DecisionPrompt>,
}
struct EvaluationRequirements {
    question_visibility: Isolated | SharedWithinNativeGroups | Unspecified,
    overflow: Reject | PermitNativeStateTextTruncation,
}
```

Use `Option<T>` by default and reserve [`Presence<T>`](../../crates/morphiecore-semantic/src/value/presence.rs#L3-L16) for boundaries where `null` and absent are observably distinct; because `Presence<T>` serializes both `Absent` and `Null` as `None`, enclosing codecs own field omission (`skip_serializing_if`) versus explicit `null`. At nullable instruction/criterion boundaries, `Presence::Null` is the only representation of field-level null: reject or normalize `Value(DecisionValue::Null)` there while preserving nested data nulls.

Question IDs are unique request-local identities, independent of optional external names, array positions, and target wire keys. Choice values are unique under typed equality (`true` and `"true"` remain distinct), preserving source order without Unicode NFC, case, or stringification normalization; target key sorting (such as Gemini's UTF-8 byte order) and its inverse permutation live exclusively in `Prepared.expectation`. Score coordinates are `0..n-1` (`"10"` is coordinate 10, never lexicographically sorted before `"2"`).

Sibling extensions remain strictly separated by what their answers prove:
- **`SingleBox` (TypeSafe 1.14) vs detection:** `SingleBox` returns one normalized rectangle `[top, bottom, left, right]` with no existence claim (`not_found` is absent; an independent `Predicate` check is required). Never merge `SingleBox` with Liquid's `box`/`boxes` (`found` probability + cardinality) into one mega-variant; they share only the `NormalizedRect` geometry leaf.
- **`TextLocate` (Perplexity v1.3):** Carries no `criteria` and targets an untransformed plain-text `State` owner initially, returning validated spans bound to explicit `(owner, text_view, endpoint_convention, unit)`. Leave `TextLocate` out of Wave 1 production enums until Perplexity `v1.3` offset units and endpoint rules are pinned.
- **`MultiLabelChoice` & `QuantityRange` (Liquid d1.2):** `MultiLabelChoice` returns independent marginal probabilities (sum `!= 1`) and selected set (`>= 0.5`), forbidding `Choice { multiple: bool }`; `QuantityRange` returns median + `[p10, p90]` quantiles + optional unit, forbidding `Score { continuous: bool }`.

#### Decision source results and accounting

```rust
struct DecisionResult {
    disposition: RequestDisposition,
    usage: Option<DecisionUsage>,
}

enum RequestDisposition {
    Evaluated(AnswerSet),
    Blocked(PromptBlockReport),
}

struct AnswerSet(Vec<(QuestionId, QuestionOutcome)>); // Private; constructed only via Prepared validation.

enum QuestionOutcome {
    Answered(Answer),
    Refused,
}

enum Answer {
    Predicate {
        probability_true: Probability,
        confidence: Presence<ConfidenceReport>,
    },
    SingleChoice(SingleChoiceReport),   // selected: ChoiceIndex, probabilities: Vec<Probability>, confidence: Presence<ConfidenceReport>
    OrdinalScore(OrdinalScoreReport),   // expected_index: ExactNumber, probabilities: Vec<Probability>, confidence: Presence<ConfidenceReport>
    // SingleBox(SingleBoxReport),      // edges: NormalizedRect, confidence: Presence<ConfidenceReport>
    // TextLocate(TextLocateReport),    // spans: Vec<VerifiedTextSpan>
}

struct ConfidenceReport {
    value: Probability,
    meaning: Unspecified
        | BinaryWinningMass { of_reported_p: bool }
        | MaximumMass
        | UniformAdjustedMaximum { singleton_one: bool }
        | OrdinalSpread {
            baseline: CenteredUniform | PeakCenteredUniform,
            peak_tie: Unspecified | FirstLevel,
        },
}
```

Distinguish five outcomes without conflating them:
1. **Whole-request policy block (`RequestDisposition::Blocked`):** Positive protocol block evidence (Gemini HTTP 200 with `prompt_feedback.block_reason` and empty `answers`); never fabricated from `answers: {}` alone, and never rewritten into per-question `Refused` outcomes.
2. **Per-question refusal (`QuestionOutcome::Refused` inside `Evaluated`):** Validated 1:1 association still covers every requested question; OpenAI mixed-outcome requests remain valid `Evaluated` responses.
3. **Zero-match extraction (`TextLocate` `spans: []`):** Successful answer reporting zero matched spans, not `Refused`.
4. **Low-existence / null extraction (Liquid `found < 0.5` or `value: null`):** Independent native observations; do not synthesize `NotFound` or strip co-returned coordinates/text unless the pinned contract defines that rule.
5. **Upstream contract / transport failure:** Missing question, duplicate/extra wire keys, contradictory `block_reason` + answers, invalid distributions, or truncated EOF before validation.

Validate `Probability` on bounded exact JSON decimal text in `[0, 1]` before any `f64` conversion so values like `1.0000000000000000001` reject cleanly. Do not write target-specific clamps like Gemini `[0.01, 0.99]` into global `Probability`; store them in the target contract, never unclamp reported values, and never substitute `confidence` for `P(true)`. Validate simplex sums, winners, expected scores, and derived statistics against source-attested per-entry rounding intervals rather than a single global epsilon, and never renormalize invalid upstream distributions.

`AnswerSet` provides no public raw-`Vec` constructor and enforces `validate_for(&DecisionRequest)` checking question IDs, variants, option/level cardinalities, and owner bindings (going beyond [`EmbeddingResponse::validate_for`](../../crates/morphiecore-semantic/src/task/embedding.rs#L251-L262)'s length check). For targets that sort options before evaluation (Gemini), validate `choice` and tie-breaks under the target's sorted expectation *before* applying the inverse permutation back to source slots.

`DecisionUsage` keeps optional `input_tokens`, `output_tokens`, `total_tokens`, cache-read/write and reasoning details, `accounting_basis`, and optional `input_by_modality: Vec<ModalityTokenCount>` (sharing [`ModalityTokenCount`](../../crates/morphiecore-semantic/src/task/embedding.rs#L172-L175)). Aggregate/detail reconciliation runs only when modality details are attested complete and disjoint; aggregator fields like OpenRouter `usage.cost` remain report metadata outside token counts.

Native field ownership is explicit rather than an extension bag:

| Native carrier | Proposed owner |
|---|---|
| OpenAI `input` string / message / call-output sequence | `Evidence::History`, with native shorthand normalized under its source contract |
| Google Gemini `input` (`string \| Content \| Content[]`) | `Evidence::Parts` (text, image, audio, video, PDF `document`, `resolution`) |
| TypeSafe/Perplexity/Cloudflare/Qwen/Microsoft `state` | `Evidence::State.root`, not parsed as messages by inspecting business keys |
| TypeSafe `assets`; Perplexity image markers | `resources` + `OccurrenceId`; source spelling records only for native echo fidelity |
| Cloudflare `images`, `audio`, `videos` | Typed resource uses in `State.side_inputs` with attested placement |
| `questions` array or map, `name` | Ordered `(QuestionId, Question)` plus optional external name and `Prepared` correlation |
| OpenAI `predicate`; Gemini `true_description`/`false_description`; System One `noul` | `Predicate` and its `BinaryCriteria` |
| OpenAI `choices[].value/description`; other `Choice` `criteria` | `SingleChoice.options[].value/criterion` |
| OpenAI `levels[].label/description`; other `Score` `criteria[]` | `OrdinalScore.levels[].label/criterion` |
| TypeSafe `box` (Wave 2); Perplexity `locate` (Wave 2) | `SingleBox` (no `found`); `TextLocate` bound to plain-text `State` owner |
| OpenAI prompt cache fields / `safety_identifier`; TypeSafe long-context header | Owner-local cache boundaries and `DecisionServiceHints`, not question content |
| Native answers, `NoulAnswer.confidence`, `prompt_feedback.block_reason` | `RequestDisposition` + `QuestionOutcome` + `ConfidenceReport` |
| Native model / `usage` / `input_tokens_by_modality` / REST envelope | `DecisionSourceReport` + `DecisionUsage`; transport envelope stays outside semantic state |

#### Named Decision projections

Use this precedence per meaning: **exact carrier → justified derivation → selected approximation/simple estimate → rejection**. No post-encoding JSON patch or generic `loss_policy` bag is added.

| Class | Meaning |
|---|---|
| E: exact | Semantic normalization or reversible representation with unchanged interpretation |
| D: derived | Arithmetic or attested inference for a delivery value; not an upstream-reported fact |
| A: approximate | Explicit change to representation/behavior whose equivalence is not claimed |
| S: estimated | Coarse non-authoritative scalar completion with a defined method and scope |
| R: reject | No approved representation, derivation, or acceptable approximation |

| Proposed rule | Class / direction | Preconditions and effect |
|---|---|---|
| `decision.correlate-questions.v1` | E or A, request/result | Exact (`E`) only when target wire keys are provably model-invisible (e.g., Gemini, OpenAI positional questions); Approximate (`A`) when peer questions or the model may observe keys (e.g., Perplexity native groups). Validates bijection and restores client association |
| `decision.resolve-media-occurrences.v1` | E, source decode/target encode | Recognizes only the selected source's explicit media syntax, retains `OccurrenceId` and `ResourceId`, generates collision-free target placeholders; rejects ambiguous overlaps |
| `decision.evidence-to-parts.v1` | A, request | Projects `State` or `History` into role-less `Parts` (for Gemini) and `Parts` into `State`/`History`; preserves ordered content while acknowledging changed role/container framing |
| `decision.state-as-user-evidence.v1` | A, request | Places shared `State` into one user-evidence message using native `input_json` when representable; acknowledges introduced role framing |
| `decision.structured-value-to-text.v1` | A, request | Serializes structured state/instruction/criterion deterministically (preserving keys, exact numbers, order); never replaces media with a URL/name |
| `decision.history-to-records.v1` | A, request | Encodes ordered roles, phases, raw call payloads, links, and statuses as structured records for state-only targets; relinquishes native role authority |
| `decision.criteria-into-instructions.v1` | A, request | Renders `BinaryCriteria` or option/level criteria into question instructions via fixed formatting without conflating criteria with shared evidence |
| `decision.typed-choice-labels.v1` | A, request/result | Maps typed `true`/`"true"` options to unique model-visible surrogates with a bijective return map and target-first validation |
| `decision.score-rubric-rendering.v1` | A, request/result | Supplies missing target labels or combines label/description while preserving level order/count and `0..n-1` scale; validates target rubric echoes before restoring original spellings |
| `decision.context-media-to-parts.v1` | A, request | Moves `State.side_inputs` into content parts under a fixed placement rule; unsupported modalities reject |
| `decision.relax-question-isolation.v1` | A, request (**opt-in**) | Allows targeting a group-shared model (Perplexity `v1.2`) from isolated source questions; requires explicit target binding/profile opt-in |
| `decision.regroup-question-context.v1` | A, request (**opt-in**) | Permits changed native group boundaries for an already group-shared source; cannot target an isolated model without separate policy |
| `decision.allow-native-state-truncation.v1` | A, request (**opt-in**) | Relinquishes no-truncation for targets with documented silent text truncation (Cloudflare Clef); requires explicit binding/profile opt-in and never uses byte estimates as proof of fit |
| `decision.derive-distribution-statistic.v1` | D, delivery | Computes client-defined confidence from a complete validated distribution (`BinaryWinningMass`, TypeSafe `UniformAdjustedMaximum` / `CenteredUniform` `OrdinalSpread`) with explicit singleton/tie/rounding rules |
| `decision.complete-delivery-usage.v1` | D/S, delivery | Preserves native reports, derives commensurate totals/details first, and estimates only missing admissible presentation counters without repairing invalid mandatory reports |
| `decision.label-bound-model.v1` | D, delivery | Completes an absent permitted model report with the public binding label (presentation identity only) |
| `decision.omit-advisory-cache-key.v1` | A, request | Omits advisory cache partitioning when storage/cache semantics remain acceptable; does not drop explicit-only mode, boundaries, retention, or safety attribution |
| `decision.refusal-to-client-error.v1` | R, delivery | Fails the complete operation with a non-retryable client-native error when any `QuestionOutcome::Refused` lacks a legal downstream carrier; no partial success or retry |
| `decision.blocked-to-client-error.v1` | R, delivery | Fails the complete operation with a non-retryable client-native error when `RequestDisposition::Blocked` lacks a legal downstream carrier |

**Default cross-family gating policy:**
- *Standard cross-family profile:* Admits proven directions. Evidence carrier projections (`State` ↔ `Parts` ↔ `History`) and prompt/rubric/typed-choice-surrogate rendering need named L3 receipts and explicit selection when they downgrade content or an explicit guarantee.
- *Explicit opt-in only:* Relaxing question isolation (`decision.relax-question-isolation.v1`), regrouping shared contexts, and permitting silent text truncation (`decision.allow-native-state-truncation.v1`) must be opted into explicitly on the target binding/profile.
- *Confidence delivery policy (withdrawing default opaque transfer):* Default `decision.transfer-opaque-confidence.v1` is **withdrawn**. For clients with documented formulas (TypeSafe), derive confidence from the complete validated distribution via `decision.derive-distribution-statistic.v1` or reject. For clients without a documented formula (OpenAI), allow explicit profile-selected derivation or opt-in approximate transfer without claiming portable cross-model thresholds.
- *Fail-closed outcomes:* Unrepresentable `Blocked` or `Refused` outcomes, unsupported modalities, group-shared → isolated context loss, and guessed `Box`/`TextLocate` coordinates always reject cleanly (`R` / L5).

Simple usage completion follows **source facts → justified inference → simple estimate** over Decision-owned inputs: estimate only missing admissible fields (`max(1, ceil((state_bytes * state_copies + question_bytes) / 4))` with attested `state_copies`, and `ceil(answer_payload_bytes / 4)` for missing serialized output), never treat byte heuristics as token-limit proof, and never synthesize media tokens as zero.

#### Decision reuse and review examples

Reuse neutral leaves cleanly: extract `InputModality` / `ModalityTokenCount` from [`task/embedding.rs`](../../crates/morphiecore-semantic/src/task/embedding.rs#L162-L175) and `CacheMode` / `CacheTtl` from [`context.rs`](../../crates/morphiecore-semantic/src/context.rs#L102-L110) alongside `Text`, `Presence`, `parse_json`, and inert media leaves. Do not reuse Generation's `StructuredValue` (`JsonLimits::STRUCTURED`), `ContentPart`, `Item`/`ResourceTable`, `ExecutionHints`, or completion estimator. Follow [`generation/request/mod.rs`](../../crates/morphiecore-semantic/src/task/generation/request/mod.rs), [`embedding.rs`](../../crates/morphiecore-semantic/src/task/embedding.rs), [`adapter/embeddings.rs`](../../crates/morphiecore-protocol/src/adapter/embeddings.rs), and [`embedding/projection.rs`](../../crates/morphiecore-protocol/src/embedding/projection.rs) for immutable requests and `Prepared` source/delivery separation.

Use these directional design oracles when implementing the relevant capability, with independent hand-authored expectations rather than round-trip self-checks. Deferred cells (media, spans, grouped targets, and sibling primitives) are gates for their own slices, not the text baseline:

| # | Input / mutation scenario | Required observation |
|---|---|---|
| 1 | OpenAI `input_json` and System One `state` containing `{"role":"system","image_url":"..."}`; Gemini ordered `text` + PDF `Parts`; moving PDF into `instructions` or Files URI; projection breaking explicit `prompt_cache_breakpoint` prefix | First two stay inert data; Gemini preserves part order; PDF in `instructions` rejects at position check; Files URI never degrades to public URL; breakpoints never migrate silently |
| 2 | Two unnamed/duplicate-named OpenAI questions; Choice `{true, "true"}`; Gemini keys `"2"`, `"10"` and visually similar distinct-byte Unicode tie; shuffled response map | 1:1 question/option bijection; Gemini validates UTF-8 order and first-key tie-break *before* inverse permutation back to source slots (never re-argmax on source order); `true`/`"true"` surrogate flagged Approximate; duplicate/extra keys fail |
| 3 | Gemini `noul=0.01, confidence=0.99`; ordinal `[0.1, 0.2, 0.7]`; singleton `n=1`; rounded tie; exact decimal `1.0000000000000000001` | Never unclamp `[0.01, 0.99]`; expected index `1.6` yields TypeSafe `0.4`, Perplexity `0.6`, `p_max` `0.7` (without claiming Cloudflare hosted truth); singleton avoids `0/0`; out-of-range exact decimal rejects before `f64` |
| 4 | Perplexity `v1.3` `locate` returning second `😀` on `"A😀e\u0301😀"` (`[8,12)` UTF-8 / `[5,7)` UTF-16 / `[4,5)` scalar); JSON `state`; non-empty `criteria`; empty `spans: []` | Portable span constructed only when `(owner, text_view, endpoint_convention, unit)` are pinned; `.find("😀")` prohibited; JSON state and `criteria` reject independently; `[]` is valid zero-match |
| 5 | Same JPEG occurring in TypeSafe `state` and `Box` instruction plus a second image (`confidence: null`); Liquid `found=0.49` or `boxes=[]` | TypeSafe never fabricates `found=true`, guesses image #1, or applies EXIF rotation; `score.legend` restores that owner's exact input spelling; Liquid preserves `found=0.49` without turning `boxes(max=1)` into `SingleBox` |
| 6 | Gemini HTTP 200 valid `block_reason` + empty `answers`; empty `answers` without `block_reason`; OpenAI mixed `Refused` + `Answered`; truncated valid body before EOF | Classified respectively as `Blocked`, upstream contract error, mixed `Evaluated`, and incomplete-stream error; unrepresentable `Blocked`/`Refused` returns non-retryable native client error; 0 published bytes before EOF |
| 7 | Qwen 11-level `score` routed to a 10-level target; conflicting Qwen response-key fixtures; Liquid `MultiLabelChoice` `[0.8, 0.7, 0.2]`, all `<0.5`, and exact `0.5` routed to `SingleChoice` | 11 levels reject without merging levels; Qwen passes only against its locked schema; multi-label allows 0 or >1 selections with sum `>1`, never taking argmax or splitting into `Predicate`s |
| 8 | Q2 referencing a term defined in Q1 sent to Perplexity `v1.2` (grouped), Flash (isolated), Gemini (isolated), and Cloudflare (silent text truncation), plus usage inspection | Do not generalize one group's visibility across groupings, silently split/merge, or admit Cloudflare via byte heuristic without truncation proof; keep per-request/per-group/per-question `state` billing distinct from `ModalityTokenCount` |

### Decision execution and review choices

Recommend synchronous complete JSON and one target per request in Wave 1. Do not split/merge questions, retry low confidence or safety outcomes, add speculative fan-out, silently truncate input, or substitute ordinary Generation. Keep the immutable request and `Prepared` association through strict EOF before validation and publication. Future Batch or streaming answer contracts stay outside `task::decision` until an explicit incremental/batch protocol exists; HTTP chunking or partial JSON never publishes `Evaluated`.

Review these choices before dependent implementation:

| Decision | Recommendation | Still to select / lock |
|---|---|---|
| Downstream SDK extension | Keep separate `/typesafe` and OpenAI `/v1` roots; no third downstream Gemini surface | Non-Python qualification and errors for newly supported features |
| Wave 1 Semantic IR | `Evidence::{State, Parts, History}`, Core 3 primitives (`Predicate`, `SingleChoice`, `OrdinalScore`), two-tier outcomes (`Blocked` vs `Refused`), exact decimal `Probability`, `ModalityTokenCount` | Human review of concrete Rust layout, custom Decision `JsonLimits`, and Wave 1 text/JSON bounds |
| Wave 1 Upstream targets | OpenAI and TypeSafe native text first; Gemini native codec and then Perplexity/Cloudflare as separate slices | Use the pinned Python sources in [`tests/upstream/sources.lock.json`](../../tests/upstream/sources.lock.json); qualify the implemented directions before activation |
| Cross-family adaptation | Qualify each direction; require selection for carrier/rubric/surrogate L3 that downgrades content or guarantees; isolation/truncation opt-in only; no default opaque confidence transfer; uncarriable `Blocked`/`Refused` L5 | Per-profile rule selection, rounding interval tolerances, and the applicable directional oracles |
| Wave 2 & Wave 3 extensions | Wave 2: multimodal (`Image`/`Audio`/`Video`/`Document` PDF, `MediaPresentation`), `SingleBox`, `TextLocate`; Wave 3: `MultiLabelChoice`, `QuantityRange`; Files URI deferred to Stage 4 Files & Uploads | Primary schema pinning for Qwen (`decision-model-preview`), Perplexity `v1.3` (`locate`), Microsoft `1.1`, Liquid `d1.2`, and Inception `mercury-decide` |

Neither the dual-SDK direction nor the capability-union principle approves these concrete recommendations automatically. Use the [roadmap gates](../roadmap/index.md#stage-4-pending-scope), not this section, as the implementation queue.
