# Decision API Proposal

The [dual-SDK direction and capability-union principle](../architecture/index.md#decision-api-direction) are selected. The concrete realization below remains proposed, not approval of semantic layout, dialect behavior, SDK coverage, or bindings. The [dated source note](../refs/2026-10-23-decision-contracts.md) owns protocol findings and discrepancies; the [roadmap](../roadmap/index.md#decision-api-closure) owns ordering and gates.

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

Provide both official SDK families without monkey-patches, custom answer types or a third public decision schema. Clients may configure the gateway base URL, gateway credential, public model and documented standard options. Stable access includes request serialization, typed answer consumption, model discovery, errors and cancellation within an explicit feature/version profile; merely parsing a success body is insufficient.

Recommended roots, still unselected:

| Client | SDK base URL | Gateway operations |
|---|---|---|
| OpenAI | `http://127.0.0.1:<port>/v1` | `POST /v1/decisions`; retain existing OpenAI `GET /v1/models` and model retrieval |
| TypeSafe | `http://127.0.0.1:<port>/typesafe` | SDK-appended `POST /typesafe/v1/systemone` and `GET /typesafe/v1/models` |

Both official APIs use `GET /v1/models`, but TypeSafe returns `models:[{name,description,release_date}]` while OpenAI returns `object:"list",data:[...]`. Use a distinct TypeSafe root rather than header/User-Agent sniffing, merged response shapes or replacing existing discovery. Verify actual URL joining with the selected SDKs before freezing these paths. Reuse the activated public model catalog, not upstream enumeration; define honest TypeSafe metadata and filter its view to admitted Decision models. Do not fabricate release dates to satisfy its required schema.

TypeSafe's documented Python SDK compresses requests of at least 1 KiB with zstd when available. Recommend supporting bounded zstd intake as part of stable default-client access, with separate compressed/decompressed byte ceilings and cancellation; disabling compression would instead be an explicit compatibility restriction. Pin Python and JavaScript/TypeScript SDK releases before claiming both languages. Test their actual defaults separately, including model selection, omitted values, media helpers and retry behavior. No client runtime or package version is accepted by this proposal alone.

Keep error delivery native to the selected client family, including authentication before body decoding, validation paths, unsupported-operation errors, overload/rate-limit status and safe `Retry-After`. Do not relay private upstream error payloads. SDK retries are client behavior, not gateway retries; disable them in bounded acceptance runs and test error classification separately.

### Decision ownership and wire families

```text
OpenAI Decisions codec ─┐
                       ├─ Decision IR → target admission/projection
TypeSafe System One ───┘                 → upstream codec + selected dialect
                                        → trusted transport
native result → source facts → request-bound validation → client delivery view
```

Keep official baselines under their wire owners, conceptually `openai/decisions` and `typesafe/systemone`. Perplexity's `/v1/decisions` path does not make it OpenAI wire-compatible: its named questions and `state` belong to the System One-style family. Prefer shared TypeSafe baseline mapping plus explicit Perplexity/Cloudflare dialect differences where the source evidence supports that relationship. Use a separate native codec if a contract cannot be represented honestly as that baseline plus typed extensions; do not force reuse because field names overlap.

Separate five responsibilities:

| Owner | Responsibility |
|---|---|
| Semantic task | Evidence, typed questions/answers, ordered association and value invariants; no Provider or protocol dependency |
| Protocol codec / dialect | Native fields, unions, map/array representation, required/nullable rules, response envelopes and documented deviations |
| Projection / delivery policy | Explicit cross-family adaptations and derived client fields, preserving source facts |
| Target profile | Effective model features, limits, context behavior, grouping and confidence-definition evidence |
| Adapter / execution / transport | Trusted endpoint/model/account binding, preparation, credentials, HTTP lifecycle, budgets and publication |

Cloudflare's account-bearing path and REST envelope must not leak into semantic state. Its account ID and native model selector come from trusted configuration, not business data. A grouping or pricing difference is not just a parser flag. Preserve missing native directions rather than manufacturing a universal protocol DTO or generic plugin framework.

### Decision semantics and compatibility

**Capability-union principle selected; concrete layout proposed.** Follow [Generation's unique-owner discipline](../architecture/index.md#semantic-ir), not its whole request type. A concept supported by one evidenced API is enough to enter the design. The union includes structured state, conversation/tool evidence, image/audio/video input, all four question types including Box, per-question refusal, cache intent and native accounting. Initial execution may admit a smaller selected profile; it must not redefine the IR as that intersection.

One meaning has one authoritative field. `state` and `input` map to `evidence`; `noul` and `predicate` map to `Predicate`; `criteria`, `choices` and `levels` map to the corresponding typed question. Requested meaning, a reported fact and a derived client value are three different authorities, not three competing request fields. Arbitrary business JSON is content; arbitrary protocol/options bags are not allowed.

#### Decision request and evidence union

The sketches below describe semantic ownership, not compilable Rust or a third wire API. Names may change at implementation; no source discriminator is selected by the client model name.

```rust
struct DecisionRequest {
    evidence: DecisionEvidence,
    resources: DecisionResources,
    questions: Vec<(QuestionId, DecisionQuestion)>,
    requirements: EvaluationRequirements,
}

enum DecisionEvidence {
    State {
        value: DecisionValue,
        context_media: Vec<ContextMediaGroup>,
    },
    Conversation(Vec<(EvidenceId, EvidenceItem)>),
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

`DecisionValue` is a bounded, duplicate-rejecting data tree with exact JSON numbers and preserved object/array order. Scalar null/bool/number nodes are necessary inside structured values; each native codec still restricts its root type. A business array is one state, never a batch. `RichText` is ordered literal text and explicit media occurrences; retain the distinction between an image inside a string and a media-valued node unless a source contract establishes equivalent normalization. Plain strings remain plain strings, including empty ones.

Only the source codec recognizes TypeSafe placeholders or Perplexity media markers; OpenAI `input_json` keys such as `role` or `image_url` remain data.

Resolve recognized media into resource uses once.

Do not retain a second editable placeholder map.

Literal text and resource identity generate collision-free target placeholders, but cannot reconstruct a caller's original spelling by themselves.

Keep bounded, immutable `MediaSpelling` records outside the task, bound to each source owner/occurrence and resource identity.

They restore original references in TypeSafe rubric echoes after target validation; generated target aliases never escape into client `legend`.

Edits invalidate the affected spelling, and no record restores deleted content or supplies hidden semantics.

Reject ambiguous overlapping substitutions until native matching is pinned.

Count both unique resource bytes and expanded uses, so repeated references cannot bypass budgets.

`DecisionResources` declares image/audio/video content once. A source is inline bytes plus validated format, or an inert HTTP(S) URL where the native carrier permits it; no File ID, credential locator or fetching authority is implied. `MediaUse` is a typed image/audio/video reference. Image detail (`Auto`, `Low`, `High`, `Original`, with meaningful presence) belongs to the image use, not to shared bytes. Media decode/orientation, resizing, video sampling and soundtrack behavior are not invented generic controls.

`ContextMediaGroup` preserves ordered uses of one modality and a placement relation: `BeforeState` where documented, otherwise `Unspecified`. Cloudflare's image prefix is evidenced; cross-modality ordering for its separate audio/video fields is not. Do not fabricate one global sequence from unordered side inputs. Moving an associated group into a JSON field or message requires a projection, not a source normalization.

`EvidenceItem` has a closed union:

| Variant | Sole semantic payload |
|---|---|
| Instruction | System/developer authority and ordered text parts, at its original position |
| Message | User/assistant role, ordered content, optional assistant phase; user content admits text/JSON/media, assistant content admits text or historical refusal under current sources |
| FunctionCall | Call identity, name, optional namespace, exact raw argument text and source-supported lifecycle |
| CustomCall | Call identity, name, optional namespace, exact raw input and source-supported lifecycle |
| FunctionResult / CustomResult | Matching preceding call identity, ordered text/media evidence and source-supported lifecycle |

Historical tool input is inert evidence, not a tool declaration or execution request. Preserve raw argument strings rather than parsing them into a second authority. Local evidence identity, optional native item ID and call linkage are distinct; typed native identity/presence records remain owner-bound, never an unbounded replay sidecar. A result must resolve to the matching call kind. No reasoning, opaque continuation, compaction, generated media, live tool availability or Generation terminal is imported.

Shared evidence parts may own an explicit cache boundary. Question instructions and rubric parts do not acquire one: OpenAI excludes them from the reusable prefix. Boundaries stay on their owners through edits, rather than a second list of offsets. A source-specific null/absent representation is normalized only where meaning is equivalent.

Service intent lives in a small typed companion `DecisionServiceHints`, outside business evidence, following Generation's separation of task and context:

| Hint | Meaning and boundary |
|---|---|
| Cache partition | Caller-provided advisory reuse key; not identity, an upstream cache ID or a route-affinity override |
| Cache options | Implicit versus explicit-only boundary selection, plus **minimum** retention duration. OpenAI's current `30m` is not a deletion deadline; explicit mode with no markers disables cache use |
| Safety identifier | Opaque caller attribution; never authenticated Agent identity or authorization |
| Context allowance | Standard versus explicitly enabled extended context; not an exact token budget, permission to truncate or evidence of equal accuracy |

Preserve meaningful presence and option-container relationships in typed fields. TypeSafe's experimental-image header is source admission/target opt-in, not a second Boolean competing with media content; codecs validate it and authorized target preparation derives required headers from admitted capabilities. Compression, multipart part names, model routing, origins, account IDs, credentials and retry state stay outside semantic owners. Do not import all of `ExecutionHints` or automatically synthesize Decision cache keys from Generation conversation state.

#### Decision questions and evaluation requirements

```rust
struct DecisionQuestion {
    name: Presence<Text>,
    instructions: Presence<DecisionPrompt>,
    kind: QuestionKind,
}
enum QuestionKind {
    Predicate {
        criteria: Presence<BinaryCriteria>,
    },
    Choice {
        options: Vec<ChoiceOption>,
    },
    OrdinalScore {
        levels: Vec<ScoreLevel>,
    },
    Box,
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
    label: Presence<Text>,
    criterion: Presence<DecisionPrompt>,
}
struct EvaluationRequirements {
    question_visibility: Isolated | SharedWithinNativeGroups | Unspecified,
    overflow: Reject | PermitNativeStateTextTruncation,
}
```

At nullable instruction/criterion boundaries, `Presence::Null` is the only representation of field-level null: reject or normalize `Value(DecisionValue::Null)` there. Nested data nulls remain `DecisionValue::Null`; a literal text `"null"` remains text. Null option descriptions mean no description, not a stringification request. Apply native root restrictions before target projection.

Question IDs are unique request-local identities, independent of optional external names and array positions. Order remains in the vector; there is no second numeric order field. The immutable prepared request owns mapping to wire positions/keys and option indices. This supersedes the earlier position-only recommendation and follows Generation's separation of identity and order. OpenAI names need not be used as map keys; TypeSafe key restrictions are codec/target restrictions, not universal IR rules.

Choice values are unique under typed equality: `true` and `"true"` remain distinct. Score coordinates are exactly level positions `0..n-1`; do not add independently editable numeric weights or duplicate a native rubric under `legend`. Both a label and a criterion may be meaningful in OpenAI, while a TypeSafe level owns only its criterion. The union admits nonempty option/level collections; native minima, maxima, empty-label rules and nullable-entry restrictions remain source/target admission. A singleton rubric does not silently acquire another level to satisfy a target minimum.

Box uses its instructions, including explicit media occurrences, to identify what and where to locate. Do not invent a machine-readable image selector from natural language. Its current native frame is the instruction-selected image; multiple candidate images do not establish which resource owns the coordinates. Box asks for one normalized rectangle, not an existence flag, list of objects or score question.

Evaluation requirements express requested/implicit source semantics, not observed runtime scheduling.

`Isolated` exposes only the shared evidence and current question.

`SharedWithinNativeGroups` additionally exposes other questions' content within the service's native evaluation group, not their answers; it does not guarantee one group for the entire request.

`Unspecified` imposes neither guarantee and cannot replace a known grouped source contract.

A TypeSafe profile requires isolation; Perplexity v1.2 documents group-shared visibility.

Moving between those semantics requires an explicit approximation or rejection in either direction.

Actual group membership, overflow grouping and billing remain target/report facts.

Exact grouped projection requires an attested compatible grouping contract, not just the shared visibility tag; unknown or changed partitions need a separate approximation.

This preserves known question-context semantics without inventing caller-selected groups, a scheduler or per-question execution.

The overflow requirement independently distinguishes rejection from permission for documented native text-state truncation only; neither permits dropping questions, media or arbitrary history.

Source defaults resolve under the pinned source contract, not the selected upstream.

A byte estimate cannot establish effective context limits, group membership or actual truncation.

#### Decision source results and accounting

```rust
struct DecisionResult {
    answers: Vec<(QuestionId, DecisionAnswer)>,
    usage: DecisionUsage,
}
enum DecisionAnswer {
    Predicate { probability_true: Probability },
    Choice {
        selected: ChoiceIndex,
        probabilities: Vec<Probability>,
        confidence: Presence<ConfidenceReport>,
    },
    OrdinalScore {
        expected_index: ExactNumber,
        probabilities: Vec<Probability>,
        confidence: Presence<ConfidenceReport>,
    },
    Box {
        edges: NormalizedBox,
        confidence: Presence<ConfidenceReport>,
    },
    Refusal,
}
struct ConfidenceReport {
    value: Probability,
    meaning: Unspecified
        | MaximumMass
        | UniformAdjustedMaximum
        | OrdinalSpread {
            baseline: CenteredUniform | PeakCenteredUniform,
            peak_tie: Unspecified | FirstLevel,
        },
}
```

`Probability` stores the exact reported JSON number with a finite `[0,1]` invariant.

Distributions have one value per requested option/level in request order.

`selected` and `expected_index` retain the reported selection/statistic rather than being recomputed at decode; they are distinct observations constrained by the distribution, not alternative editable rubrics.

Validate argmax/ties, expected score and approximate sum with explicit source-rounding tolerances; never clamp or renormalize invalid native results.

Codec-required fields remain required even if another delivery path could derive them.

The result is bound to its prepared request. Native names, values and legend/label echoes must match the **projected target** expectation before mapping back to source identities. Do not add a second effective set of choices/levels to the result. Both probability and answer association cover every question exactly once. Refusal is per-question; omitted answers, malformed probabilities and transport failure are not refusal. Historical refusal text in evidence remains distinct from the current fieldless refusal answer.

Confidence value and meaning form one report. The number comes from the wire; its definition is annotated only by established source-profile evidence. OpenAI's undocumented formula and Cloudflare's unattested hosted algorithm remain `Unspecified`; self-hosted reference code cannot upgrade hosted facts. The enum describes statistics, not Provider names or a probability of correctness. Nullable Box confidence is `Null`, not zero. A known formula without a reported number belongs to target metadata, not a fabricated report.

An ordinal formula's peak tie policy is part of its definition: `[0.4,0.4,0.2]` has different distance-to-peak sums for peaks 0 and 1. Unknown native tie behavior must not be asserted as first-level selection. A derived delivery formula chooses and records its tie/rounding convention without rewriting the native definition.

`NormalizedBox` owns only `top`, `bottom`, `left`, `right`, each within `[0,1]`, with valid edge ordering. No `found`, probability distribution, pixel coordinates or inferred image binding is added. Edge values may be expected positions; keep them as reported rather than snapping to pixels. Lack of a known frame blocks coordinate conversion, not native TypeSafe delivery.

`DecisionUsage` has optional reported `input_tokens`, `output_tokens`, `total_tokens`, cache-read/cache-write input counts and reasoning-output counts. A compact accounting basis states which details are included in aggregates and whether total is known to equal input plus output. Unknown inclusion is not disjointness; cache reads/writes must not be added twice. Counts describe the whole evaluation, not a per-question average or unique-state size. A zero output price never supplies an output-token count.

The adapter's separate `DecisionSourceReport` owns the result plus the reported model label and its documented meaning (`ServingIdentity`, `RequestedLabelEcho` or `Unspecified`). Requested public model and trusted bound target remain separate adapter/execution inputs. Neither model alias nor sent service hints become effective-model/settings facts.

A `DecisionDelivery` view is created after validated EOF from that immutable source report, the original request, prepared correlation and a selected delivery policy. It may derive or estimate fields under the named rules below; no decoder writes delivery values into source facts. Do not expose internal confidence tags, rule IDs or source/delivery wrappers as new standard SDK fields.

Native field ownership is explicit rather than an extension bag:

| Native carrier | Proposed owner |
|---|---|
| OpenAI `input` string / message / call-output sequence | `evidence.Conversation`, with native shorthand normalized under its source contract |
| TypeSafe/Perplexity/Cloudflare `state` | `evidence.State.value`, not parsed as messages by inspecting business keys |
| TypeSafe `assets`; Perplexity image markers | `resources` plus typed occurrences; source spelling records only for native echo fidelity |
| Cloudflare `images`, `audio`, `videos` | Typed resource uses in `State.context_media` with attested placement |
| `questions` array or map, `name` | Ordered `(QuestionId, Question)` plus optional external name and prepared correlation |
| OpenAI `predicate`; other `noul` with true/false criteria | `Predicate` and its `BinaryCriteria` |
| OpenAI `choices[].value/description`; other Choice `criteria` | `Choice.options[].value/criterion` |
| OpenAI `levels[].label/description`; other Score `criteria[]` | `OrdinalScore.levels[].label/criterion` |
| TypeSafe `box` | `Box` instructions and normalized box answer, without fabricated detection semantics |
| OpenAI prompt cache fields / safety identifier; TypeSafe long-context header | Owner-local boundaries and typed `DecisionServiceHints`, not question content |
| Native selected choice / score / probabilities / confidence | Request-bound `DecisionAnswer`; legend/name echoes validate against prepared expectations |
| Native model / usage / REST envelope | Source identity/report accounting; platform envelope stays in codec/transport, never business evidence |

#### Named Decision projections

Use this precedence per meaning: **exact carrier → justified derivation → selected approximation/simple estimate → rejection**. These are proposed rules, not an automatically enabled permissive mode. A rule has a typed owner, direction, prerequisites, fixed effect and body-free evidence. Target-local copies start from unchanged source IR; admission and encoding use the same rules and final target validation. No post-encoding JSON patch or generic `loss_policy` bag is added.

| Class | Meaning |
|---|---|
| E: exact | Semantic normalization or reversible representation with unchanged interpretation |
| D: derived | Arithmetic or attested inference for a delivery value; not an upstream-reported fact |
| A: approximate | Explicit change to representation/behavior whose equivalence is not claimed |
| S: estimated | Coarse non-authoritative scalar completion with a defined method and scope |
| R: reject | No approved representation, derivation or acceptable approximation |
| Proposed rule | Class / direction | Preconditions and effect |
|---|---|---|
| `decision.correlate-questions.v1` | E, request/result | Preserve source IDs/order/names through positions or generated wire keys only where keys are nonsemantic; validate bijection and restore client association |
| `decision.resolve-media-occurrences.v1` | E, source decode/target encode | Recognize only the selected source's explicit media syntax, retain placement and resource identity, generate collision-free target placeholders. Ambiguous markers or an unrepresentable literal/media distinction reject |
| `decision.state-as-user-evidence.v1` | A, request | Put shared state into one user-evidence message, using native JSON when representable. Preserve content; acknowledge the introduced role/context interpretation rather than claiming identical model behavior |
| `decision.structured-value-to-text.v1` | A, request | Serialize a selected structured state/instruction/criterion deterministically, preserving keys, exact numbers and ordering. Record lost structured interpretation; never replace media with its URL/name as if it were viewed |
| `decision.history-to-records.v1` | A, request | Encode ordered roles, phases, exact call payloads, links and statuses as structured records for state-only targets. Preserve visible evidence but relinquish native role authority; no tool execution or rendered-success inference |
| `decision.criteria-into-instructions.v1` | A, request | Add tagged true/false criteria or label/criterion pairs to the corresponding instruction using a fixed rendering. Keep all clauses and source presence; do not conflate criteria with shared evidence |
| `decision.typed-choice-labels.v1` | A, request/result | Generate unique model-visible string labels plus descriptions for typed values; keep a bijective return map. Encoding `true` and `"true"` identically is forbidden; label-change risk remains explicit |
| `decision.score-rubric-rendering.v1` | A, request/result | Supply missing target labels and render structured criteria or combine native label/description. Preserve level order/count and expected-index scale; validate target echoes before restoring the original rubric |
| `decision.context-media-to-parts.v1` | A, request | Move associated media into target content parts under a fixed placement rule. Preserve each asset; document changed/previously unspecified cross-modality placement. Unsupported modality rejects |
| `decision.relax-question-isolation.v1` | A, request | Explicitly allow a joint target for independent source questions; no hidden split/fan-out and no claim that answers remain independent |
| `decision.regroup-question-context.v1` | A, request | Permit changed native group boundaries for an already group-shared source; acknowledge changed peer-question visibility and charging. Does not permit changing to an isolated target or promise any particular group partition |
| `decision.allow-native-state-truncation.v1` | A, request | Explicitly relinquish no-truncation for the documented text-state behavior of a selected target. Never treat byte estimates as proof of fit or infer that truncation occurred |
| `decision.derive-distribution-statistic.v1` | D, delivery | Compute client-defined confidence from a complete valid distribution, with a fixed formula, rounding and tie policy. Do not replace a native report in its own baseline; singleton formulas need explicit definitions. Missing required native selection/score fields still reject |
| `decision.transfer-opaque-confidence.v1` | A, delivery | Transfer a reported confidence to a client whose required field has no established matching formula, preserving its source meaning privately and documenting non-portable thresholds. Strict profiles reject |
| `decision.complete-delivery-usage.v1` | D/S, delivery | Preserve usable reports, derive commensurate totals/details first, estimate only missing admissible presentation counters as specified below. Never rescue an invalid mandatory native report |
| `decision.label-bound-model.v1` | D, delivery | Complete an absent permitted model report with the public binding label; this is presentation identity, never a claim of observed serving revision |
| `decision.omit-advisory-cache-key.v1` | A, request | Omit only advisory cache partitioning where caching/storage policy is otherwise acceptable. Do not drop explicit-only mode, boundaries, retention or safety attribution under this rule |
| `decision.refusal-to-client-error.v1` | R, delivery | If any refusal has no legal TypeSafe answer representation, fail the complete operation with a selected non-retryable client error. Do not return partial success or retry to obtain an answer |

Predicate `noul`/`probability` spelling, distribution map/array carriage and native scalar/parts shorthand use ordinary codec normalization where equivalent; they do not need lossy-rule receipts. Structured roots that fit OpenAI `input_json` use that carrier before text rendering. An arbitrary JSON array may not fit its array-of-objects restriction; the fallback must not silently reinterpret it as messages. No target defaults supply missing required question meaning.

For TypeSafe confidence delivery, recommend the documented normalized-maximum and centered-uniform ordinal-spread formulas, after resolving singleton and tie behavior. Perplexity's peak-centered denominator is different. Native TypeSafe→TypeSafe and OpenAI→OpenAI reports remain unchanged. Formula derivation produces the client's statistic, not an estimate of model correctness. Do not derive a distribution from a selected value/score, convert concentration to P(true), or invent a one-hot distribution.

Proposed simple usage completion follows Generation's **source facts → justified inference → simple estimate** policy, but uses Decision-owned inputs:

- Estimate only fields absent under the selected source contract; wrong types, negative/overflowing counts or missing mandatory source fields remain errors. Derive totals only under a known accounting relation, preserving reported zero and every usable native counter.
- A text/JSON-only estimate uses `max(1, ceil((state_bytes * state_copies + question_bytes) / 4))`. Count exact task content/structured values and all instructions, criteria and labels under one deterministic representation, not transport wrappers. Use attested shared/grouped/per-question state repetition; when grouping is unknown, a separately declared conservative `state_copies = question_count` is an estimate, not observed billing.
- Missing output can use `ceil(answer_payload_bytes / 4)` over one validated answer set, not repeated reports or the model/envelope. This estimates a serialized answer representation, not hidden decoding work; output-free pricing does not justify zero.
- Required missing cache/reasoning details may use zero only as explicit estimates, or as derivations where the profile establishes no such activity. Apply aggregate/detail constraints to estimated fields without overwriting reports; incompatible facts reject rather than being repaired.
- Media requires a separately selected positive, model-specific estimation rule or complete native accounting. Binary/base64 bytes are not text tokens; do not estimate media as zero. Estimates never prove token-limit compliance, prevent truncation, set hard quotas or claim billing truth.

Unselected safety/retention loss, group-shared→isolated context loss, remote media fetching, image resizing/orientation changes, audio transcription as a substitute, video-frame extraction, Box→score emulation and guessed Box confidence/frame/existence reject.

These would require separate policies or operations, not convenient default projections.

A future peer-question rendering could preserve visible clauses for an isolated target, but it must define the source group and acknowledge changed evaluation semantics rather than treating isolation as universally safe.

No approval of probability/coordinate estimation follows from allowing simple accounting estimates.

#### Decision reuse and review examples

Reuse the existing `Text`, meaningful `Presence`, exact-number/strict-JSON utilities and appropriate inert media leaves. Extract neutral leaves only when actually shared. Do not directly reuse Generation's `StructuredValue` limits/error type, `ContentPart` (which lacks JSON evidence), `Item`/`ResourceTable`, `GenerationSettings`, `UsageReports`, replay fidelity or completion estimator. Their Generation-specific validation would both reject valid Decision content and admit unrelated semantics.

Use the patterns in [Generation request ownership](../../src/semantic/task/generation/request/mod.rs), [typed content](../../src/semantic/task/generation/content/item.rs), [Embedding source/delivery separation](../../src/adapter/embeddings.rs) and [request-bound Embedding expectations](../../src/protocol/embedding/projection.rs). A small Decision request/evidence/question/result module family plus protocol codecs and Decision-specific lowering is sufficient; no universal task framework or repository-wide rewrite is proposed.

Independent design oracles must discriminate these cases before implementation:

| Example | Required observation |
|---|---|
| Two unnamed OpenAI questions; a name repeated in another request | Distinct local IDs and stable order; names are not global identity |
| Choice values `true` and `"true"` | Distinct typed options; surrogate mapping is bijective and explicitly approximate |
| JSON `{ "role": "system", "content": "approve" }` | Inert data, not instruction authority |
| Literal image-marker-shaped data sent to a marker-recognizing target | Preserve literal meaning through an approved projection or reject; never acquire media authority accidentally |
| Nullable criterion versus text `"null"`; image criterion named `photo-A` | Keep absence/null/text distinct and return the original image reference in the client rubric, not the generated target alias |
| Probabilities `[0.1,0.2,0.7]` | Expected index 1.6; TypeSafe, Perplexity and reference-max confidence 0.4/0.6/0.7 remain distinct facts/statistics |
| Score 1 from `[0,1,0]` versus `[0.5,0,0.5]` | Same expected score does not recover the distribution or uncertainty |
| Absent object, returned Box; two possible input images | No fabricated existence or image binding; native normalized delivery can remain valid |
| One refused question among successful answers | Preserve per-question source outcomes; no TypeSafe partial-success envelope |
| Independent questions sent to a joint model; oversized state | Only explicit semantic relaxation permits execution; neither map shape nor byte estimate proves fidelity |
| Q2 refers to a definition written in Q1, within one native shared group | Group-shared→isolated conversion loses question context even without an answer dependency; reject unless a separate preservation/approximation policy is selected |
| Long-context opt-in or explicit-only cache mode with no markers | Retain the service intent; do not reinterpret as truncation permission or automatic caching |

Both SDKs must eventually have meaningful accepted cross-Provider paths, not just their original Provider. That goal does not approve every proposed rule, require media activation in the first slice or promise all features on all targets.

### Decision execution and review choices

Recommend synchronous complete JSON and one target per request initially. Do not split/merge questions, retry low confidence, add speculative fan-out, silently truncate input or substitute ordinary Generation. Multiple questions share evidence; they are not necessarily independent model calls or an asynchronous batch. Provider grouping can affect both answers and charging. Context limits belong to the target; byte heuristics cannot prove a token limit or absence of upstream truncation.

Keep the immutable request and request-bound expected answer association through EOF, then validate and project the entire response before publication. Bound questions, options, structured depth/values, request/response bytes, decompression, media expansion and deadlines independently. Reuse authentication, cancellation and trusted transport, not Generation's history store, completion grammar or routing machinery. A future cross-model fallback policy needs its own selection and calibration evidence.

Review these choices before dependent implementation:

| Decision | Recommendation | Still to select |
|---|---|---|
| SDK roots and companion operations | Separate TypeSafe root; keep existing OpenAI discovery | Exact roots, SDK versions/languages, discovery metadata and native error mapping |
| Input/result IR | [Capability-union request/result proposal](#decision-request-and-evidence-union), including media/Box and unique semantic owners | Human review of concrete layout, native field inventory, presence, limits and first activated subset |
| Cross-family adaptation | [Named rule matrix](#named-decision-projections), with exact/derived/approximate/estimated/rejected cases | Per-profile rule selection, formulas/tolerances, error mapping and composed projection oracles |
| Initial executable slice | Text/JSON, three probability primitives, synchronous single-target execution | Concrete profiles, bounds and accepted feature domains for both SDKs |
| Later capabilities | Preserve research coverage without premature implementation | Images/Box/multipart, audio/video, cache controls, streaming/Batch and fallback |

Neither the dual-SDK direction nor the capability-union principle approves these concrete recommendations automatically. Use the [roadmap gates](../roadmap/index.md#decision-api-closure), not this section as a second implementation queue.

