# API expansion design

## Contents

- [Status and authority](#status-and-authority)
- [Requested scope](#requested-scope)
- [Shared design boundaries](#shared-design-boundaries)
  - [Proposed ownership](#proposed-ownership)
  - [Generation boundary review](#generation-boundary-review)
    - [Missing Messages output ceilings](#missing-messages-output-ceilings)
    - [Google output report owners (unselected)](#google-output-report-owners-unselected)
  - [Resource and execution safety](#resource-and-execution-safety)
- [Operation contracts](#operation-contracts)
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
  - [Token counting](#token-counting)
  - [Embeddings](#embeddings)
    - [Embedding chain and ownership](#embedding-chain-and-ownership)
    - [Embedding inputs and admission](#embedding-inputs-and-admission)
    - [Embedding results and encoding](#embedding-results-and-encoding)
    - [Google Embedding profile](#google-embedding-profile)
  - [Audio](#audio)
  - [Image production and editing](#image-production-and-editing)
    - [Image requests and target admission](#image-requests-and-target-admission)
    - [Image results and delivery compatibility](#image-results-and-delivery-compatibility)
    - [Static Generate review proposal](#static-generate-review-proposal)
    - [Image streaming lifecycle](#image-streaming-lifecycle)
    - [Image design review](#image-design-review)
  - [Video API and implementation](#video-api-and-implementation)
- [Resource services](#resource-services)
  - [Files and uploads](#files-and-uploads)
- [Unscheduled designs](#unscheduled-designs)
  - [Models discovery interfaces](#models-discovery-interfaces)
    - [Models envelopes and interface descriptors](#models-envelopes-and-interface-descriptors)
    - [Generation discovery schema](#generation-discovery-schema)
    - [Discovery aggregation and ownership](#discovery-aggregation-and-ownership)
    - [Enhanced model example](#enhanced-model-example)
    - [Discovery integration boundaries](#discovery-integration-boundaries)
  - [Centralized agent authorization](#centralized-agent-authorization)
  - [1C design: Messages citations](#1c-design-messages-citations)
  - [Deferred tool discovery and web search](#deferred-tool-discovery-and-web-search)
  - [Context compaction](#context-compaction)
  - [Native batch jobs](#native-batch-jobs)
  - [Realtime](#realtime)
  - [Deferred cache prewarming](#deferred-cache-prewarming)
  - [Model-name service-policy carriers](#model-name-service-policy-carriers)
- [Open decisions](#open-decisions)
- [Execution handoff](#execution-handoff)
- [Verification](#verification)
- [Protocol references](#protocol-references)

## Status and authority

This page owns proposed operation, resource and advanced-Generation designs. The requested scope is confirmed; structural recommendations remain proposals unless already established in [Architecture](architecture.md). No new interface, binding, resource service or execution policy is activated by this document.

[Roadmap](roadmap.md#planned-order) owns the five-stage sequence and the current routing freeze. Stage 3 closed its selected projection/loop scope with `count_tokens` deferred. The selected [Generation Text production closure](roadmap.md#generation-text-production-closure) precedes stage-4 Embedding/audio/image/video and Files/multipart Uploads. Server-side compaction, Batch and Realtime join complete citations/discovery/hosted tools in stage 5. **All stage-5 tasks are currently unscheduled:** scope placement is not a start or delivery commitment. Detailed operation designs remain proposals; stage-4/5 work is not an exit condition for earlier stages.

Code and tests remain the behavior specification. This document records ownership, constraints and proposed work, not a model catalog, compatibility certification or implementation progress log. Provider and binding availability belongs to [the catalog](../src/topology/catalog.rs) and [startup](../src/gateway/startup/bootstrap.rs).

On 2026-10-22 the user prioritized **Embedding and Images design**. The detailed proposals below are the current review surface, not approval of new IR layouts, delivery dialects, comprehensive coverage or bindings. Audio, Files and deferred video keep their existing scope; the [roadmap](roadmap.md#stage-4-scope) owns scheduling.

The subsequent Embedding selections are recorded in Architecture: [four-way encoding](architecture.md#embedding-encoding-compatibility), sample-centered inputs and [report/identity projection](architecture.md#google-embedding-compatibility) are implemented. Complete downstream usage with explicit request-based approximation supersedes missing-usage delivery; it does not reopen the deferred counting endpoint. This document retains only unselected Embedding extensions.

On 2026-10-23 the user selected [dual-SDK Decision access](architecture.md#decision-api-direction), with research/design before further modality work. Both TypeSafe and OpenAI are required downstream families, rather than competing alternatives. The [Decision section](#decisions) proposes the detailed contract; selecting that direction does not approve its pending layouts, mappings or implementation.

## Requested scope

The user selected OpenAI-first downstream contracts, broad coverage of current audio and image APIs, and no compatibility work for historical protocols. The subsequent five-stage plan explicitly adds video API/implementation adaptation to stage 4. OpenAI-first does not mean OpenAI-only upstream execution; video operations and concrete endpoints remain to be selected from current protocol evidence.

| Requested surface | Expansion goal |
|---|---|
| OpenAI Decisions and TypeSafe System One, including SDK companion operations | Stable standard-SDK access through one Decision IR; adapt initially researched OpenAI, TypeSafe, Perplexity and Cloudflare upstreams. Concrete namespaces and compatibility profiles remain under review |
| `POST /v1/messages/count_tokens` | Stage 3, deferred 2026-10-21: count a structured model input under the selected target/projection contract, without generation |
| `POST /v1/responses/compact` | Stage 5, unscheduled: execute native context compaction and return reusable context |
| `POST /v1/embeddings` | Complete the existing independent embedding contract |
| `POST /v1/audio/speech` | Complete standalone speech synthesis, including supported streaming |
| `POST /v1/audio/transcriptions` | Complete file-based transcription and its reported evidence |
| `POST /v1/audio/translations` | Add the current native audio-translation contract |
| `POST /v1/images/generations` | Complete the existing image-generation contract within the selected stage-4 image scope |
| `POST /v1/images/edits` | Add editing and complete the shared modern image-production contract |
| Video APIs, concrete operations/endpoints pending | Adapt selected video input/output or generation operations, including their execution lifecycle and artifact delivery |
| Batch APIs | Stage 5, unscheduled: submit and manage asynchronous batches of supported typed tasks |
| Files/multipart Uploads | Stage 4: manage reusable files and bounded multipart upload resources |
| Realtime APIs, concrete contracts pending | Stage 5, unscheduled: select client/session, transport, interruption and recovery contracts |

The companion surface `POST /v1/responses/input_tokens` is recommended for the OpenAI-first counting path, but remains unselected. Image generations and edits are both included by the [stage-4 scope](roadmap.md#stage-4-scope); their concrete contracts and implementation slices still need selection.

Optional scope: map task-specific upstream Chat Completions media payloads, such as MiMo TTS/ASR, to standard `/audio/*` endpoints. This must not block the OpenAI main path.

Completeness means covering the selected current operation contracts, not only the intersection supported by every model. Keep semantic expression, codec representation, model-effective admission and working integration as separate claims. Unsupported combinations reject explicitly; a field's presence in a schema does not establish effective model support.

Do not add Legacy Completions, old DALL-E-only behavior or image variations for historical parity. Existing current Chat Completions and Messages contracts are not deprecated merely by this expansion. A still-current endpoint's documented older-model restriction is not permission to invent a newer equivalent.

The earlier exclusion of video is superseded by the stage-4 modality scope; it does not select a particular video-generation or editing contract yet. Realtime/Live is an unscheduled stage-5 topic, not part of stage-4 media delivery. Google-native downstream endpoints, Vector Stores, Assistants and fine-tuning remain outside this expansion. Hosted tools have their own unscheduled stage-5 work. Custom voice creation and consent management remain an [open scope decision](#open-decisions). Standalone image support does not automatically implement the Responses image-generation tool.

The initial vendor comparison is a checklist, not a protocol oracle. Endpoint names do not determine task ownership: Gemini can synthesize speech through a generation endpoint, Responses can be stateless, and Anthropic permits consecutive same-role turns. Use the [official references](#protocol-references), not the comparison's historical restrictions.

## Shared design boundaries

### Proposed ownership

One typed semantic core does not require one universal `GenerationRequest`. Standalone operations must not be forced through ordinary Generation execution. Conversational media remains Generation content; select video operation ownership before adding a new task or lifecycle.

| Surface | Semantic or service owner | Execution owner | Reuse from Generation |
|---|---|---|---|
| OpenAI Decisions / TypeSafe System One | Independent Decision task; detailed layout proposed below | Independent evaluation and complete-result delivery | Evidence/content values where meanings agree, not Generation controls or execution |
| Messages and proposed Responses token counting | New `TokenCount` task | Independent counting operation | Ordered input, tools, media and count-relevant controls |
| Responses compaction | New `ContextCompaction` task | Independent compaction operation | Ordered context, item identity and opaque replay |
| Embeddings | Existing Embedding task | Existing independent path | Shared scalar/accounting values only where meanings match |
| Speech | Existing SpeechSynthesis task | Independent synthesis and delivery | Shared audio values, not a conversation response |
| Transcriptions/translations | Proposed AudioToText family with `Transcribe` and `Translate` variants | Shared audio-input infrastructure, distinct operation admission | Shared media values, not Generation events |
| Image generations/edits | Proposed Image family with `Generate` and `Edit` variants | Shared image execution infrastructure, distinct operation admission | Shared media values and artifacts |
| Video operations | Task/content and optional job owners to be selected per operation | Native synchronous/asynchronous lifecycle, as required by the selected API | Shared media, timing and artifact values without borrowing Generation completion |
| Files/Uploads | Resource-operation contracts | Resource service | Inert content references, not the conversation resource table |
| Batches | Job contract containing allowlisted typed task requests | Durable job coordination | Generation as one supported record type |

Names above describe ownership, not a required directory migration or public Rust API.

```text
shared semantic values
  conversation content: ordered items, links, resources and replay
  media sources/artifacts, timing and appropriate accounting values

task contracts
  Generation
  Decision
  TokenCount
  ContextCompaction
  Embedding
  SpeechSynthesis
  AudioToText { Transcribe, Translate }
  Image { Generate, Edit }

service contracts
  Files / Uploads
  Batch jobs
```

Share authentication, trusted transport, bounded I/O, cancellation primitives and error classification. Keep each task's event grammar, reduction, result validation and completion rules separate. Do not introduce a universal request bag, plugin registry or generic pipeline framework.

Extract a shared conversation-input aggregate only where counting and compaction need it. Reusing ordered content must not force those operations to inherit sampling, audio-output settings or ordinary assistant-response validation.

### Generation boundary review

The 2026-10-21 production decisions now live in Architecture: [tool examples/document context](architecture.md#tool-examples-and-document-context), [citation point anchors](architecture.md#citation-point-anchors), [automatic TTL admission](architecture.md#caller-selected-automatic-cache-admission) and [explanatory reports](architecture.md#explanatory-terminal-reports). They are selected local contracts, not unresolved alternatives below or claims of implementation. The remaining proposals do not inherit that approval.

#### Retain legitimate conversation semantics

The existing [Generation request](../src/semantic/task/generation/request/mod.rs) owns conversation content and settings. Independent [Embedding](../src/semantic/task/embedding.rs), [SpeechSynthesis](../src/semantic/task/speech_synthesis.rs), [SpeechRecognition](../src/semantic/task/speech_recognition.rs) and [ImageGeneration](../src/semantic/task/image_generation.rs) contracts already exist. They are not standalone operations incorrectly hidden inside Generation.

Keep these concepts in conversation semantics:

- Text-only instructions, plus user and tool-result media with their existing role and carrier restrictions. Media-bearing instructions remain rejected.
- Conversational audio output and necessary returned-history references.
- Compaction items consumed as ordered context.
- Tool calls/results, citations, and reported Provider-tool observations.
- Pure caller-owned history selection and context edits.

Minimal counterexamples distinguish content from operations:

| Scenario | Correct owner |
|---|---|
| Answer a question aloud | Generation with audio output |
| Read supplied text aloud | SpeechSynthesis |
| Describe an image | Generation with image input |
| Edit a supplied image under a mask | Image Edit |
| Consume an opaque compacted-history item | Generation input |
| Produce a compacted context window | ContextCompaction |
| Cite or consume a document | Conversation content |
| Upload, expire or delete a stored document | File service |

A future Responses hosted image-tool observation may contain image artifacts without making standalone image execution a Generation operation. Similarly, reported tool progress is not the gateway's Batch scheduler state.

#### Independent output ceilings (unselected)

The 2026-10-19 Bailian review selected [existing-IR-only adaptation](architecture.md#shared-aliyun-billing-dialect), not a budget-scope extension. Qwen Chat's reply-only `max_tokens:128` plus `thinking_budget:64` is a minimal counterexample: replacing them with total `192` permits an oversized reply when fewer thoughts are used; total `128` can truncate an otherwise valid 128-reply/64-thought result. Neither summation nor clipping is an exact mapping.

A future proposal would need independently owned answer/thought/total limits, precedence and contradiction rules, native source/target mappings and cross-protocol refusal behavior. No representation is selected here. Current codecs reject the unrepresentable combination and numeric allocations outside the existing IR relation; they do not widen IR or reinterpret effort as tokens.


#### Missing Messages output ceilings

The user selected the candidate-local configuration, explicit-cap-only generic constructors and Go/Haiku `32000` catalog default on 2026-10-20. The accepted [missing-output-ceiling contract](architecture.md#missing-output-ceilings) now owns these rules; [Usage](usage.md#model-configuration) owns implemented configuration. No alternative default framework or independent output-limit IR extension is selected here.

#### Google output report owners (unselected)

Probability and token-identity proposals remain paused under the [Google API pause](roadmap.md#google-api-pause). The later 2026-10-21 decision moved bounded terminal explanations and service-source review to the accepted [report contract](architecture.md#explanatory-terminal-reports); it did not select the owners below.

The [source counterexamples](refs/2026-10-17-generation-contracts.md#google-foreground-contracts-2026-10-20) still require design review before broader probability carriage:

| Fact | Minimal proposed owner | Required decision |
|---|---|---|
| Reported average and sum log probabilities | One bounded output/candidate probability-report owner with independent optional numbers | Define chunk/checkpoint/final scope from evidence, replacement/conflict rules, edit dependencies and delivery. An aggregate may arrive without token reports; never recompute it as the reported value |
| Internal vocabulary token ID | Typed source-bound fidelity tied to the unchanged text/probability owner and chosen/alternative position | Select retention bounds and invalidation/carriers; it is neither portable token bytes nor replay authority |

A terminal-only aggregate-score profile remains unselected after the [streaming-scope research](refs/2026-10-17-generation-contracts.md#streamed-score-scope-2026-10-20). Public sources define candidate averages/sums but do not establish ordinary SSE fragment-versus-cumulative coverage or repeated/missing-field reduction. A final finish reason alone does not establish whole-output score coverage. The SDK's cross-continuation-hop sum/latest-scalar merge is not a single-request stream oracle.

Recommendation after research: retain aggregate rejection until a concrete model/endpoint/API-version profile establishes coverage and report replacement. Do not select a universal response-score owner, infer totals from usage/token arrays, treat last checkpoint as final, or introduce an event archive merely to avoid that decision. The selected terminal explanation is independent of these score-scope questions.

Proposed token-identity records use signed int32 values and the existing token/top-candidate and fidelity budgets; bind them to source origin/model, readable owner, exact probabilities and chosen/alternative positions. Owner/probability edits invalidate them. Targets without an exact carrier reject by default; the selected explanation-only omission policy does not cover probability reports or token identity. No token-byte inference, probability-request admission or Google downstream endpoint is selected.

Prefer those local additions over a new generic report store. Existing `TopLogprob` already preserves absent alternative token/probability values and needs no new owner. Existing Provider-tool observations are the first candidate for future URL retrieval facts, but their Google mapping and delivery are not selected. Keep nonempty URL-context reports rejected without activating hosted tools or full citation support.

#### Review concrete ownership coupling

| Evidence entry point | Existing coupling | Proposed treatment |
|---|---|---|
| [Shared context](../src/semantic/context.rs) | `ExecutionHints` mixes cache/identity metadata, `max_tool_calls`, storage/background controls and unit-valued unsupported-field placeholders | Separate Generation execution constraints, service intent and codec admission. Do not extend this envelope into file/job management |
| [Generation resources](../src/semantic/task/generation/resources/mod.rs) | Reusable media locations depend on Generation errors, visibility and budgets | Extract only genuinely shared inert source/reference values; retain conversation resource tables and use-site interpretation |
| [Replay](../src/semantic/task/generation/history/replay.rs) | Semantic validation names protocol-specific replay formats and carries origin compatibility data | Review carrier/provenance ownership when sharing compaction machinery; retain semantic purpose, owner linkage and all isolation checks |
| [Attempt preparation](../src/execution/attempt.rs) | Independent tasks reuse prepared-request/error types located beside Generation-specific intake | Move necessary neutral execution values to an appropriate shared owner, without a framework rewrite |

These are boundary issues, not evidence that Generation already implements a scheduler. Actual Responses storage is outside the task IR; the shared context currently rejects active unsupported stateful execution. [Gateway history](architecture.md#stateful-history) is process-local conversation storage, not durable execution storage.

Do not remove opaque replay provenance to simplify types. Any reorganization must preserve carrier compatibility, completeness and deployment isolation. A protocol-specific representation is not a new semantic task.

The conversation [GeneratedAudio](../src/semantic/task/generation/content/audio.rs) shape requires transcript/reference information that a standalone speech result need not have. Keep conversational replay requirements local; use suitable [shared audio artifacts](../src/semantic/value/audio.rs) rather than fabricating those facts.

Existing cache options do not establish a complete cache-only operation. Preserve current handling and the separate [deferred prewarming decision](roadmap.md#deferred-cache-prewarming); counting and compaction must not reopen it implicitly.

#### Integration constraints to revisit

The [transcription adapter](../src/adapter/transcription.rs) and [preparation](../src/execution/transcription.rs) show that a standard downstream transcription endpoint does not establish an OpenAI multipart upstream implementation. Include native upstream preparation, response decoding and stream intake in the audio plan.

Standalone topology has narrow operation and execution restrictions; see [image routes](../src/topology/images.rs). Review model identity across operations, task-specific streaming admission and operation bindings during scheduled integration. Do not fabricate separate model identities to bypass a one-operation restriction.

Upstream response parsing must remain strict on known fields and ignore unknown fields. Review the shared closed-object helper in [OpenAI image/embedding codecs](../src/protocol/openai/images.rs) when reopening those contracts; current rejection of unknown response fields is not the intended forward-compatibility policy.

### Resource and execution safety

Keep runtime targets, credentials, retries and publication state outside task IR. Resource services and job coordination may need private persistent metadata, but never persist access tokens in semantic requests or expose credential locators.

| Failure scenario | Required boundary |
|---|---|
| File ID names a different account/project | Intersect authorized task targets with resource scope; reject incompatible use |
| Connection drops after remote upload completion or batch creation | Reconcile remote outcome before repeating a state-changing request |
| Gateway restarts after acknowledging a batch | Recover the accepted job, not merely its conversation history |
| Caller disconnects during synchronous speech/recognition | Cancel and release the upstream according to the task lifecycle |
| Caller disconnects after asynchronous job acceptance | Keep the job; cancellation requires an explicit operation |
| Stream ends before its required completion evidence | Fail or abort; do not fabricate success |
| Deletion/expiry races an active job or retained history | Apply a selected reference-retention policy rather than accidental dangling access |
| A large upload or JSONL input exceeds memory capacity | Use bounded streaming/spooling and finite retention, not Generation's whole-request buffer |

Existing conversation affinity does not replace resource ownership or durable job affinity. Long-lived jobs need a policy for unavailable credentials without adding implicit credential refresh, login or reassignment.

Keep diagnostics body-free. Apply the existing private-capture policy deliberately when new paths are instrumented; binary, multipart and fragmented content must not become unredacted fallback logging.

## Operation contracts

### Decisions

The [dual-SDK direction and capability-union principle](architecture.md#decision-api-direction) are selected. The concrete realization below remains proposed, not approval of semantic layout, dialect behavior, SDK coverage or bindings. The [dated source note](refs/2026-10-23-decision-contracts.md) owns protocol findings and discrepancies; the [roadmap](roadmap.md#decision-api-design) owns ordering and gates.

#### SDK surfaces and discovery

Provide both official SDK families without monkey-patches, custom answer types or a third public decision schema. Clients may configure the gateway base URL, gateway credential, public model and documented standard options. Stable access includes request serialization, typed answer consumption, model discovery, errors and cancellation within an explicit feature/version profile; merely parsing a success body is insufficient.

Recommended roots, still unselected:

| Client | SDK base URL | Gateway operations |
|---|---|---|
| OpenAI | `http://127.0.0.1:<port>/v1` | `POST /v1/decisions`; retain existing OpenAI `GET /v1/models` and model retrieval |
| TypeSafe | `http://127.0.0.1:<port>/typesafe` | SDK-appended `POST /typesafe/v1/systemone` and `GET /typesafe/v1/models` |

Both official APIs use `GET /v1/models`, but TypeSafe returns `models:[{name,description,release_date}]` while OpenAI returns `object:"list",data:[...]`. Use a distinct TypeSafe root rather than header/User-Agent sniffing, merged response shapes or replacing existing discovery. Verify actual URL joining with the selected SDKs before freezing these paths. Reuse the activated public model catalog, not upstream enumeration; define honest TypeSafe metadata and filter its view to admitted Decision models. Do not fabricate release dates to satisfy its required schema.

TypeSafe's documented Python SDK compresses requests of at least 1 KiB with zstd when available. Recommend supporting bounded zstd intake as part of stable default-client access, with separate compressed/decompressed byte ceilings and cancellation; disabling compression would instead be an explicit compatibility restriction. Pin Python and JavaScript/TypeScript SDK releases before claiming both languages. Test their actual defaults separately, including model selection, omitted values, media helpers and retry behavior. No client runtime or package version is accepted by this proposal alone.

Keep error delivery native to the selected client family, including authentication before body decoding, validation paths, unsupported-operation errors, overload/rate-limit status and safe `Retry-After`. Do not relay private upstream error payloads. SDK retries are client behavior, not gateway retries; disable them in bounded acceptance runs and test error classification separately.

#### Decision ownership and wire families

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

#### Decision semantics and compatibility

**Capability-union principle selected; concrete layout proposed.** Follow [Generation's unique-owner discipline](architecture.md#semantic-ir), not its whole request type. A concept supported by one evidenced API is enough to enter the design. The union includes structured state, conversation/tool evidence, image/audio/video input, all four question types including Box, per-question refusal, cache intent and native accounting. Initial execution may admit a smaller selected profile; it must not redefine the IR as that intersection.

One meaning has one authoritative field. `state` and `input` map to `evidence`; `noul` and `predicate` map to `Predicate`; `criteria`, `choices` and `levels` map to the corresponding typed question. Requested meaning, a reported fact and a derived client value are three different authorities, not three competing request fields. Arbitrary business JSON is content; arbitrary protocol/options bags are not allowed.

##### Decision request and evidence union

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

Only the source codec recognizes TypeSafe placeholders or Perplexity media markers; OpenAI `input_json` keys such as `role` or `image_url` remain data. Resolve recognized media into resource uses once. Do not retain a second editable placeholder map. Literal text and resource identity generate collision-free target placeholders, but cannot reconstruct a caller's original spelling by themselves. Keep bounded, immutable `MediaSpelling` records outside the task, bound to each source owner/occurrence and resource identity. They restore original references in TypeSafe rubric echoes after target validation; generated target aliases never escape into client `legend`. Edits invalidate the affected spelling, and no record restores deleted content or supplies hidden semantics. Reject ambiguous overlapping substitutions until native matching is pinned. Count both unique resource bytes and expanded uses, so repeated references cannot bypass budgets.

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

##### Decision questions and evaluation requirements

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

Evaluation requirements express requested/implicit source semantics, not observed runtime scheduling. `Isolated` exposes only the shared evidence and current question. `SharedWithinNativeGroups` additionally exposes other questions' content within the service's native evaluation group, not their answers; it does not guarantee one group for the entire request. `Unspecified` imposes neither guarantee and cannot replace a known grouped source contract. A TypeSafe profile requires isolation; Perplexity v1.2 documents group-shared visibility. Moving between those semantics requires an explicit approximation or rejection in either direction.

Actual group membership, overflow grouping and billing remain target/report facts. Exact grouped projection requires an attested compatible grouping contract, not just the shared visibility tag; unknown or changed partitions need a separate approximation. This preserves known question-context semantics without inventing caller-selected groups, a scheduler or per-question execution. The overflow requirement independently distinguishes rejection from permission for documented native text-state truncation only; neither permits dropping questions, media or arbitrary history. Source defaults resolve under the pinned source contract, not the selected upstream. A byte estimate cannot establish effective context limits, group membership or actual truncation.

##### Decision source results and accounting

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

`Probability` stores the exact reported JSON number with a finite `[0,1]` invariant. Distributions have one value per requested option/level in request order. `selected` and `expected_index` retain the reported selection/statistic rather than being recomputed at decode; they are distinct observations constrained by the distribution, not alternative editable rubrics. Validate argmax/ties, expected score and approximate sum with explicit source-rounding tolerances; never clamp or renormalize invalid native results. Codec-required fields remain required even if another delivery path could derive them.

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

##### Named Decision projections

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

Unselected safety/retention loss, group-shared→isolated context loss, remote media fetching, image resizing/orientation changes, audio transcription as a substitute, video-frame extraction, Box→score emulation and guessed Box confidence/frame/existence reject. These would require separate policies or operations, not convenient default projections. A future peer-question rendering could preserve visible clauses for an isolated target, but it must define the source group and acknowledge changed evaluation semantics rather than treating isolation as universally safe. No approval of probability/coordinate estimation follows from allowing simple accounting estimates.

##### Decision reuse and review examples

Reuse the existing `Text`, meaningful `Presence`, exact-number/strict-JSON utilities and appropriate inert media leaves. Extract neutral leaves only when actually shared. Do not directly reuse Generation's `StructuredValue` limits/error type, `ContentPart` (which lacks JSON evidence), `Item`/`ResourceTable`, `GenerationSettings`, `UsageReports`, replay fidelity or completion estimator. Their Generation-specific validation would both reject valid Decision content and admit unrelated semantics.

Use the patterns in [Generation request ownership](../src/semantic/task/generation/request/mod.rs), [typed content](../src/semantic/task/generation/content/item.rs), [Embedding source/delivery separation](../src/adapter/embeddings.rs) and [request-bound Embedding expectations](../src/lowering/embeddings.rs). A small Decision request/evidence/question/result module family plus protocol codecs and Decision-specific lowering is sufficient; no universal task framework or repository-wide rewrite is proposed.

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

#### Decision execution and review choices

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

Neither the dual-SDK direction nor the capability-union principle approves these concrete recommendations automatically. Use the [roadmap gates](roadmap.md#decision-api-design), not this section as a second implementation queue.

### Token counting

Selected for stage 3 with cross-protocol conversion and **deferred on 2026-10-21** ([roadmap](roadmap.md#deferred-token-counting)); the contract below is unstarted design reference. The Messages `count_tokens` surface is confirmed; the Responses counting companion still requires selection.

Model a count request from count-relevant conversation content: instructions, ordered history, tools, media and applicable settings. It is not generation with a zero output cap. A count result is not inference usage and must not contain fabricated generated output.

OpenAI documents `POST /v1/responses/input_tokens`; Anthropic documents `POST /v1/messages/count_tokens`. Reuse one task boundary while preserving distinct source semantics and result envelopes. Resolve any admitted gateway-owned history reference before target preparation; never forward a gateway ID as a Provider-owned reference.

The [upstream-first counting policy](architecture.md#token-counting-policy) is selected: prefer upstream counts, allow explicit coarse estimates for inaccurate upstream reports, and do not build a tokenizer-based exact-counting path. Identify the model/target, effective input projection and count source; a Messages-shaped request does not imply an Anthropic model or counting method.

Select the concrete profile's upstream reliability criteria, coarse estimation rule and supported input coverage. A character-based rule must account for the projected instructions, history and tool definitions/results, not just the latest user text. For heuristic fallback, media require an explicit estimate policy or rejection, not a zero-cost assumption. Select client-visible source/approximation reporting compatible with the result envelope; do not invent standard API fields or promise billing equality.

Exit evidence: independent request/result oracles cover upstream preference and the selected inaccurate-report fallback, count-relevant controls and media, explicit scope/provenance, no generation side effects and no tokenizer execution. Reject input outside the selected counting profile rather than silently omit it. One candidate's result still neither covers every failover candidate nor reserves the next generation route.

### Embeddings

The sample-centered text/scoped-token IR, native Google integration, immutable preparation and explicit ancillary/numeric delivery rules are implemented under the renewed 2026-10-23 authorization. Their authoritative contract is [Independent Embedding path](architecture.md#independent-embedding-path), not a pending proposal. This section retains only extension boundaries.

#### Embedding chain and ownership

Preserve the independent single-target chain, source/delivery separation and one immutable prepared plan. Future operations must establish their own admission and response expectations before activation; no generic provider-options bag, hidden fan-out or Generation failover is proposed. Native error-body inspection is not selected: status and Retry-After remain authoritative even if a body stalls.

#### Embedding inputs and admission

Future joint image/audio/video inputs need typed content owners, source validation and model-specific input processing controls. Keep independent samples distinct from joint parts within a sample. Do not infer joint semantics by concatenating text or reuse generation attachments without reviewing their contract.

Task purpose and explicit overflow/truncation controls would belong to each sample, not competing batch defaults. No task prefix injection or implicit truncation is selected. New tokenizer scopes need vocabulary, special-token, per-input and aggregate limits plus actual target-carrier evidence; the implemented ordinary `cl100k_base` subset does not attest other tokenizers.

#### Embedding results and encoding

Quantized int8/uint8/binary vectors and multi-representation output require an explicit numerical representation independent of transport encoding. Base64 alone cannot identify element width or dtype. Tensor/soft-token results require a separate shape/index contract, not flattening into ordinary dense embeddings. Contextualized chunks need a distinct shared-context relation rather than being represented as independent batch samples.

Embedding-space identity remains a trusted model contract. Equal widths, labels or a hash of request options are not sufficient for substitutability. Retrieval indexes, chunking, contextual grouping and migration/re-embedding remain outside the current path.

#### Google Embedding profile

The implemented target uses native synchronous `batchEmbedContents`, nested controls and explicit no-truncation; [dated source/protocol evidence](refs/2026-10-22-embedding-image-contracts.md#google-wire-source-reconciliation) distinguishes current REST from lagging SDK converters. The Google OpenAI-compatible endpoint, native downstream APIs, `gemini-embedding-001`, multimodal ingestion and tensors remain unselected. A future extension must characterize its own configuration and reporting rather than inherit text-only acceptance.

### Audio

#### Speech synthesis

Preserve supplied text, synthesis instructions, voice selection, speed and encoding intent. Distinguish a built-in voice from an authorized custom voice reference instead of relying on an unconstrained string.

Support the selected current binary and SSE delivery contracts. Progressive binary delivery must not require whole-output buffering; SSE needs task-specific audio events, reported usage and completion. Audio encoding and event transport are separate choices.

Use reported token, character or duration accounting only where the source provides it. Input text is not a fabricated audio transcript. Custom voice creation and consent are resource operations, not fields that cause synthesis to create resources.

Exit evidence: correct binary/SSE delivery, supported controls preserved, unsupported voice/format/control combinations rejected, bounded buffering and cancellation, and no success after truncated input.

#### Audio to text

Propose separate `Transcribe` and `Translate` request variants within one audio-to-text domain. The same recording may require an original-language transcript or an English translation; that difference is protected task intent, not a response encoding.

Complete the selected current contract for audio containers, source-language hints, prompting and applicable controls. Results may include full text, segments, words, timestamps, speaker attribution, log probabilities and token- or duration-based usage. Preserve reported coverage: the existing last-sentence report is not a complete alignment.

Keep plain text, JSON and subtitle encoders outside the core result. Formats that require timestamps or speaker information need real corresponding evidence; they cannot be manufactured from unannotated text. Diarization and chunking controls remain distinct from ordinary formatting.

An uploaded audio file with streamed transcription results is not a live microphone-input protocol. File transcription does not require creating a reusable File API object. Do not extend the existing multipart collector into an unbounded file-service buffer.

The currently documented OpenAI translation endpoint is English-targeted and Whisper-only. Recheck this restriction when pinning the implementation source; do not invent an arbitrary target-language parameter or replace it with an implicit transcription-plus-Generation chain.

Exit evidence: native multipart preparation, complete static and supported streamed results, faithful timing/speaker/usage reports, operation-specific restrictions, bounds and cancellation.

#### Optional Chat-carried audio

Separate downstream API, task meaning and upstream carrier:

```text
/audio/speech
  -> SpeechSynthesis
  -> task-specific MiMo Chat codec
  -> /chat/completions
  -> SpeechResult
  -> standard audio delivery

/audio/transcriptions
  -> AudioToText.Transcribe
  -> task-specific MiMo Chat codec
  -> /chat/completions
  -> TranscriptionResult
  -> standard transcription delivery
```

Neither path needs `GenerationRequest`, `GenerationResponse` or the Generation reducer. A documented TTS carrier placing supplied text in an `assistant` field is wire encoding, not a fabricated conversation turn in IR.

The existing [MiMo audio codec](../src/protocol/xiaomi_audio.rs), [speech adapter](../src/adapter/speech.rs) and [transcription adapter](../src/adapter/transcription.rs) already demonstrate this separation. Preserve and evolve that path; catalog registration does not establish activation or live acceptance.

Retain exact task intent and explicit target capabilities. Map supported media/control carriers, reject unsupported voice, speed, format, timing or speaker requirements, and preserve available accounting under the selected reporting policy. Do not reinterpret ASR as translation or substitute a nearby voice silently.

Current static adaptation is not proof of native streaming support. When scheduled, implement a task-specific mapping from upstream audio/text deltas to the selected downstream event contract, including cancellation and honest terminals. Do not present post-buffer chunking as verified progressive upstream delivery.

Treat this as a separate optional audio slice: migrate existing static mappings after the task contracts stabilize, then add native streaming and richer reports where justified. It must not narrow the OpenAI contract or block its completion.

### Image production and editing

The current task and execution path implement bounded static Generate, not Edit or streaming. The selected Grok codec does not establish a downstream contract or gateway binding. [Dated OpenAI evidence](refs/2026-10-22-embedding-image-contracts.md#images) and [Grok observations](refs/2026-10-22-grok-imagine.md#images) distinguish the official schema, model-effective controls and unresolved source discrepancies.

#### Image requests and target admission

Propose explicit operation variants, reusing existing controls and artifacts:

```text
ImageRequest =
  Generate { prompt, output_controls }
  Edit {
    prompt, ordered_sources,
    optional mask { source_index, content },
    optional input_fidelity,
    output_controls
  }
```

An edit of photograph A is not generation with a longer prompt; source identity/order, mask association and preservation intent are protected task meaning. Keep model, client identity, transport and runtime state in their existing envelopes. The family need not force a repository-wide rename or generic media-task framework.

Direct sources distinguish bounded image content from inert URL/data-URL references. Reuse only suitable neutral values, not Generation resource tables or replay owners. Multipart filenames are untrusted transport metadata, never local paths or target selectors. Preserve part order and MIME declarations without treating them as verified pixel/format facts.

Recommend immutable owned/shared bytes for the first direct-input slice, with deliberately small fixed aggregate memory limits; defer spooling. Pure multipart encoding consumes those bounded bytes and produces a bounded body; budget the intake, encoded body and output copies, not only source payload size. Content ownership lasts through preparation/upload and releases on completion, error or cancellation. This fits the current byte-owned execution request without promising full upstream file-size coverage. If larger inputs later justify spooling, first select request-local content identities, an external content owner and a pure multipart part plan; handles, I/O and cleanup never enter IR. Merely adding a spool limit would not establish that ownership seam.

For OpenAI, encode ordered multipart `image`/`image[]` or the selected JSON `images` reference form without changing meaning. JSON entries choose exactly one source form. File-ID forms remain rejected until authorized Files resolution exists. Remote URLs may be passed only to targets with an admitted native reference carrier; do not add gateway URL downloading, redirects or credential forwarding to make multipart-only targets work.

OpenAI applies a mask to the first image. Decode that into explicit source-zero association and reject other associations on encoding; never reorder images to conceal a mismatch. The mask is editing guidance, not a promise of pixel-exact preservation. Recommend retaining the accepted [inert-media boundary](architecture.md#media): validate source shape, declared type and encoded/decoded byte bounds locally; leave actual format, alpha and matching pixel dimensions to the native upstream validator for both inline and URL forms. A target without the required validation contract rejects. MIME declarations are not validation evidence. A future bounded metadata inspector would require an explicit standalone-Edit policy exception, resource limits and a declared-versus-observed fact owner; this proposal does not introduce image decoding or change Generation media handling.

The mask sources conflict: the pinned SDK specifies PNG below 4 MB and matching dimensions; the current guide describes matching format/size below 50 MB with alpha. Recommend an initial conservative masked profile with PNG-declared source and mask, mask below 4,000,000 bytes, and native validation of PNG/alpha/matching dimensions, clearly documented as a local subset rather than local pixel verification. Choose finite per-image, aggregate and materialized-body limits separately before implementation. “Up to 16 images below 50 MB each” is not permission to buffer 800 MB per request.

| Control | Proposed meaning and admission |
|---|---|
| Count | Exact final-artifact count, initially existing 1–10 bound; target restrictions can be narrower |
| Output encoding | PNG/JPEG/WebP intent, separate from Base64/URL carriage. OpenAI omission means PNG, not unconstrained target choice |
| Size | Keep Auto and exact pixels. Model profiles enforce multiples, aspect, pixel/edge limits where applicable; a schema string is not universal model support |
| Native ratio/resolution | Proposed separate typed aspect-ratio and resolution-tier intent for Grok, not an approximation of exact pixels. Public exposure and conflicting-control rules need selection |
| Quality/background | Preserve Auto versus explicit choices. Auto is delegated choice, not a measured result; xhigh/max or transparency admission is model-specific |
| Compression/moderation | Preserve supported explicit controls; JPEG/WebP compression is not PNG compression, and moderation choice is not a safety assessment |
| Input fidelity | Edit-only preservation effort, not byte identity. Unsupported explicit values reject; documented automatic-high behavior needs a named exact rule before omitting an explicit high |
| Streaming/previews | Delivery-specific stream selection and optional preview ceiling, not part of requested final count |

Resolve protocol defaults on typed target copies before encoding. Do not invent reported settings from those defaults. Preserve absent/null/explicit states only where their source contract permits them; null is not a universal reset. Both OpenAI identity inputs and reported effective safety identifiers need their own bounded envelope/report carriers, never output controls or inferred values.

Operation admission must distinguish Generate, Edit, static and streaming support on one model identity. The current one-operation/API-key-only topology restrictions need responsible-layer changes only when their selected integration begins; do not invent duplicate model names to bypass them. Grok has no inspected equivalent mask or input-fidelity carrier and no established OpenAI preview stream: those combinations reject.

#### Image results and delivery compatibility

Share ordered final artifacts and optional **reported** creation time, serving model and accounting while retaining operation identity at the task boundary. Each artifact owns data and its actual reports; a top-level static report is valid only when it describes every artifact. Retain current bounded Base64 delivery for the first static slice; URL-only artifacts do not authorize automatic downloading or caller-directed upload destinations.

Maintain three distinct facts: requested controls, stream-reported settings (which can remain Auto), and concrete artifact reports. Never put `quality:auto` into an effective `ImageQuality`, infer dimensions from a requested size, or treat event `created_at` as static image creation time. Absent, null and zero accounting are distinct. Monetary ticks remain money, cached tokens remain subsets, and native sparse usage cannot fill required OpenAI counters.

**Public compatibility is a design gate, not wiring:**

| Minimal counterexample | Consequence and recommendation |
|---|---|
| Grok returns only `data:[{b64_json:...}]` | Valid native output lacks OpenAI's required `created`. Keep strict OpenAI delivery; never insert the gateway clock or request start time |
| OpenAI request omits `output_format`; Grok returns JPEG | OpenAI's PNG default was not preserved. Reject the binding/profile before dispatch unless an explicitly different client contract or verified exact transformation is selected |
| Source reports serving model/native billing without standard carriers | Preserve the source; select explicit dialect carriage or a named allowed report omission. The existing accounting projection does not authorize model/time loss |
| Artifact reports disagree across outputs | Do not collapse them into a misleading top-level report; reject incompatible standard delivery or use a selected richer carrier |

Recommend keeping strict OpenAI Images as the standard dialect and designing an **explicit MorphieCore Images dialect** for optional native reports and ratio/tier controls if Grok activation remains desired. Its selection mechanism, field layout, defaults and SDK implications require human review. Alternatives are a separately selected native surface or leaving Grok disconnected. Do not silently relax the standard schema for one model, add fields to OpenAPI before implementation, or claim that a lenient SDK parser establishes conformance. Image transcoding is a separate capability, not a default repair.

#### Static Generate review proposal

**Unselected proposal, refined on 2026-10-23.** The [roadmap](roadmap.md#next-stage-4-slice) owns the recommended sequence. Start with Generate's existing task/artifact owners; neither an Image Generate/Edit type migration nor native ratio/tier controls are prerequisites for this review.

| Public-contract option | Benefit and cost | Recommendation |
|---|---|---|
| Explicit native-output profile on the existing Generate route | Reuses task execution and most request fields, but intentionally changes encoding defaults and result requirements. Needs explicit versioned client opt-in, capability disclosure and a typed delivery alternative | Preferred if exposing Grok now is valuable; select the opt-in mechanism and exact schema before implementation |
| Separate native image surface | Makes incompatibility unmistakable, but adds another public endpoint/schema and client integration for the same task | Use only if independent native API ownership is a product requirement |
| Keep standard-only delivery and Grok disconnected | No new public contract; cannot deliver native Grok success without established standard-compatible defaults/reports | Safe fallback if a nonstandard profile is not wanted |

The proposed native-output profile is narrower than a complete Grok API:

- **Selection and ownership:** default clients retain strict OpenAI delivery. Require explicit per-request profile selection, independent of model aliases and trusted target selection. The profile is an adapter/delivery fact, not a semantic task field, origin or credential selector. Recommend a versioned request header so JSON model/prompt input remains reusable; its name/value and response schema are still review decisions. Unsupported profile values reject before dispatch.
- **Request subset:** initially reuse prompt, count, supported quality and the existing identity envelope with bounded Base64 static delivery. Omitted image encoding delegates to the target, unlike standard OpenAI's PNG default. Explicit encoding, exact size and other unsupported controls reject before dispatch; do not silently map pixels to tiers. Native aspect ratio/resolution can be a later typed extension rather than a prerequisite for usable static output.
- **Result shape:** retain ordered Base64 artifacts. Require the admitted final count, but allow absent reported creation time and serving model. Carry optional per-artifact format reports and sparse native accounting in explicitly typed fields rather than forcing them into standard required counters. Reuse the current exact-money/token owners; exact field names and null rules must be selected, with absence/null/zero distinguished where the source admits them. No raw response bag or broad report-loss policy.
- **Defaults and evidence:** prepare a target-local effective expectation without mutating the source request. Standard omission must resolve to PNG and require an evidenced native guarantee; the proposed profile's omission means target-selected encoding, not guessed JPEG or PNG. Validate any reported contradiction after EOF. Missing MIME is unknown, not proof that the requested/default encoding was honored. A known incompatible target must reject before spending a request; a late contradiction fails delivery without replay.
- **Bounds and publication:** retain the local 1–10 count and 8-MiB aggregate decoded-final ceiling, intersected with configured per-image/body/time/concurrency limits. Budget Base64/JSON delivery independently. No URL fetching, transcoding, deferred jobs, alternate upload destinations or new retries. Complete static intake/validation/projection before publication.
- **Client consequence:** the profile is not strict OpenAI Images conformance. Python SDK header forwarding may be usable, but strict typed consumers can reject an absent `created` or ignore extension reports. SDK behavior needs its own acceptance and cannot justify inventing a timestamp.

Before implementation, review isolated directional examples: native `data` without `created`; a separate result with `created` plus serving `model`; a standard omitted-format request followed by a JPEG report; differing reports across two artifacts; and sparse usage with exact money. Each negative example must reach its own intended failure rather than first failing for a missing timestamp. Positive examples must independently specify the selected public representation, not only native→IR→native round trips.

One existing boundary repair also belongs in the selected static slice: OpenAI/OpenRouter upstream image parsing currently shares closed-object helpers and rejects unknown members, unlike the strict-known/ignore-unknown rule. Separate upstream tolerance from downstream admission; retain strict known fields, duplicate-key rejection and resource bounds. Do not accidentally admit unsupported client controls while fixing upstream extensibility.

Grok OAuth support and entitlement are subsequent independent integration questions. The API-key-only image topology/gateway checks need responsible-layer changes under the selected contract, not a weakened Provider check or a duplicate model identity. The existing pure codec remains disconnected until those gates pass.

#### Image streaming lifecycle

Propose a small task-specific event vocabulary, not the Generation reducer:

```text
Preview { output_slot, preview_index, artifact, event_time, reported_settings }
Completed { final_artifacts, event_time, reported_settings, reported_usage }
```

An internal output slot is association within one operation, not an invented upstream/public ID. The inspected OpenAI stream has `partial_image_index` but no final-output index. Recommend **one final output for the initial streaming profile**; then every preview belongs to slot zero. Multi-output static delivery remains independent. Do not guess multi-output associations or reuse preview indexes as final-image identity.

The requested 0–3 partial images are a ceiling, not guaranteed work or percentage progress. The inspected references do not establish omission's default. Recommend forwarding omission unchanged to the same native contract while enforcing a separate local maximum of three previews; do not silently turn omission into zero. Explicit null needs a source-backed profile rule (preserve, normalize to omission or reject), not a universal reset. Select that rule for Generate and Edit before admission.

Accept zero/fewer previews and a final image arriving early. Preserve reported preview indexes and arrival order; initially require nonnegative, unique increasing indexes within the effective local/explicit bound without inventing missing previews. Pin any stronger contiguity rule before implementation. Reject previews with an explicit zero ceiling, incompatible event types, duplicate completion, post-terminal data and malformed known settings.

Stream incrementally without retaining every preview; charge cumulative encoded/decoded bytes, per-event bytes, event count, final-artifact bytes, idle/total time and concurrency independently. The current 8-MiB aggregate final-image budget is not a new 8-MiB allowance for each preview. Select finite stream/upload limits before implementing those paths.

Success requires an operation-correct completed event, the requested final count and validated EOF. Preview publication forbids replay; on later failure abort/fail the existing delivery rather than replay or emit a successful terminal. Hold successful completion behind EOF, release upstreams on disconnect/deadline, and retain reported final usage once rather than summing preview observations. SSE `created_at` cannot automatically produce a static `created`; reject unsupported static↔stream conversions until a source-backed mapping is selected. Buffered static images split into chunks do not establish upstream progressive delivery.

#### Image design review

| Decision | Recommendation and alternative | Blocks |
|---|---|---|
| Operation/source ownership | Generate/Edit family, ordered neutral sources and explicit source-zero mask. Start with immutable byte-owned inputs and a total materialization budget; a later spool-backed part plan is a separate choice | Edit IR and multipart execution, not existing Generate maintenance |
| Static Generate public contract | [Explicit native-output profile proposal](#static-generate-review-proposal), separate native surface or disconnected Grok. Select defaults, report shape and client opt-in together; native ratio/tier exposure is independently selectable | Grok public admission, defaults and binding, not its existing pure codec or later Edit/stream review |
| Mask profile and validation | Conservative PNG-declared source/mask and byte subset, retaining native pixel/alpha validation. Local metadata inspection instead needs an explicit inert-media policy exception | Masked Edit admission, not unmasked direct-input design |
| Stream association and reports | Initially one final output; task-local preview/final lifecycle and separate settings/event-time owners. Select omission/null behavior without inventing a preview default | Streaming, not static Generate/Edit |

Independent directional oracles must cover Generate and Edit, multipart and JSON, source/mask ordering, defaults and target-specific restrictions, optional/native result reports and intended negative cases. Streaming needs independent preview/final fixtures, omitted/null/zero preview controls, fewer previews, Auto reports, errors/truncation/post-terminal data, cumulative bounds and cancellation without replay. SDK multipart/static and native SSE consumption are separate integration gates; a genuine incremental transport check is required in addition to an SDK accumulator. Real calls wait for a selected target and bounded acceptance plan. Files, hosted image tools, deferred jobs, video, cross-Provider failover and automatic media sweeps remain excluded.

### Video API and implementation

Video integration is explicitly [deferred](roadmap.md#grok-imagine-adaptation). On resumption, use Grok's native asynchronous API as the primary design reference; the earlier synchronous-wait choice is superseded. Public wire schemas, typed owners, job authorization and persistence remain unselected. Do not label a Grok job or synchronous wrapper as OpenAI Videos compatibility. For each operation, pin current official contracts before selecting input/output, generation or editing behavior. Separate conversational video content from standalone production operations; the wire endpoint name does not determine the task owner.

For each selected operation, specify supported input references, required media controls, output artifacts and reported accounting. If the native API is asynchronous, select submission identity, status, cancellation, artifact access and uncertain-submission handling explicitly. Do not reuse a completed text response as evidence of video completion or turn a returned artifact URL into unrestricted server-side fetching.

Exit evidence: independently specified request/result and applicable event/job contracts, bounded media I/O, correct artifact identity, cancellation and honest completion. Specific APIs, new task types, storage and job persistence remain design decisions, not permissions implied by this scope entry.

## Resource services

### Files and uploads

Files/multipart Uploads are selected for stage 4. They need typed resource operations, not an inference request or a larger Generation resource table.

| Method and path | Proposed operation |
|---|---|
| `POST /v1/files` | Create a file |
| `GET /v1/files` | List accessible files |
| `GET /v1/files/{file_id}` | Retrieve metadata |
| `GET /v1/files/{file_id}/content` | Read content |
| `DELETE /v1/files/{file_id}` | Delete a file |
| `POST /v1/uploads` | Create a bounded multipart upload |
| `POST /v1/uploads/{upload_id}/parts` | Add a part |
| `POST /v1/uploads/{upload_id}/complete` | Complete ordered parts into a usable file |
| `POST /v1/uploads/{upload_id}/cancel` | Cancel an incomplete upload |

The resource service owns public identities, access checks, metadata, purpose, retention, upload state and private upstream mappings. The conversation resource table remains request-local content ownership.

Choose authoritative storage before implementation. The initial Files-slice recommendation is upstream-native content storage with gateway-owned public IDs and durable private mappings. The alternative is gateway-owned content with per-target upstream replicas. Neither is selected yet.

Define finite storage/upload bounds, part order and byte-total validation, incomplete-upload cleanup and supported purposes. Do not adopt deprecated File status fields as a new processing state machine. Accepting a file purpose does not implement its associated fine-tuning or agent service.

Resolve resource authorization outside pure codecs. Task content may carry typed resource identity, but credentials, storage handles and trusted target mappings stay outside IR. Resource scope can restrict already authorized targets; it cannot authorize an arbitrary origin or credential.

Exit evidence: create/read/list/delete and upload lifecycle contracts, restart-consistent mappings, access isolation, expiry, deletion with active references, and bounded streaming rather than whole-file memory retention.

## Unscheduled designs

The [stage-5 backlog](roadmap.md#stage-5-backlog) has no implementation queue or delivery commitment. Native cache-only prewarming and model-name service-policy selection also retain separate scheduling decisions. These branches share no automatic approval or activation.

### Models discovery interfaces

**User-reviewed design draft; interface completion is tentative and unscheduled.** Retain the schema below for later review and implementation selection, not as an implemented or frozen wire contract. The [roadmap](roadmap.md#models-discovery-interfaces) owns scheduling and acceptance gates. This work has no assigned stage, does not change current priorities and is not a prerequisite for another slice.

Keep standard OpenAI discovery separate from enhanced Agent metadata:

| Surface | Responsibility |
|---|---|
| Existing `/v1/models` and `/v1/models/{model}` | Standard activated-model list/retrieval and existing deletion refusal; further conformance review remains pending |
| Proposed `/v1/morphiecore/models` and `/v1/morphiecore/models/{model}` | Read-only effective model/interface descriptions for Agent planning |
| Existing `/v1/morphiecore/capabilities` | Detailed request, delivery and history cases with stable adaptation rule IDs |

Do not turn standard model deletion into configuration management or add enhanced fields to its standard object. The existing [capability contract](architecture.md#public-capability-and-adaptation-notice) remains authoritative for implemented behavior. No new route, binding, formal schema artifact or `docs/openapi.json` entry is authorized by documenting this proposal.

#### Models envelopes and interface descriptors

The TypeScript below describes proposed JSON field names and unions, not internal Rust types. Named fields are required unless marked optional; `Partial` dictionaries may omit unassessed entries.

```ts
interface DiscoveryEnvelope {
  version: 1;
  catalog_revision: string;
  coverage: "partial";
  unlisted: "not_assessed";
  links: {
    capabilities: "/v1/morphiecore/capabilities";
  };
}

interface ModelListResponse extends DiscoveryEnvelope {
  object: "morphiecore.model_list";
  data: ModelDescriptor[];
}

interface ModelResponse extends DiscoveryEnvelope {
  object: "morphiecore.model";
  data: ModelDescriptor;
}

interface ModelDescriptor {
  id: string;
  name: string;
  created: number;
  owned_by: string;
  shutdown_date?: string | null;
  interfaces: ModelInterface[];
}

interface ModelInterface {
  task:
    | "generation" | "embedding" | "image_generation"
    | "speech" | "transcription";
  protocol:
    | "chat_completions" | "responses" | "messages" | "embeddings"
    | "images_generations" | "audio_speech" | "audio_transcriptions";
  delivery_modes: Array<"json" | "sse" | "binary">;
  generation: GenerationContract | null;
}
```

List and retrieval use the same descriptor from one immutable snapshot. `catalog_revision` is an opaque revision of the visible discovery contract, not a Git commit, startup time or private-configuration digest. Propose adding the same revision to the existing capabilities response; equal revisions must identify consistent descriptors, cases and rules. Clients join by revision, public model ID and interface protocol, not by array position. Missing detailed entries remain unassessed.

Only list activated public models visible to the caller, with activated interfaces and delivery modes. Task/protocol pairs must agree: Generation uses Chat/Responses/Messages; the other protocols correspond to their named standalone tasks. The type unions do not authorize arbitrary combinations or future operations. Keep `id`, `created`, `owned_by` and any `shutdown_date` consistent with the selected standard Models contract; do not invent dates or confuse temporary gateway deactivation with model retirement. `name` is a display label and may fall back to `id`.

The first detailed schema is Generation-only. Non-Generation interfaces require `generation: null`; a Generation interface may also use null when no enhanced declaration has been established. Null does not disable the interface. Existing Embedding detail stays in capabilities rather than being duplicated here.

#### Generation discovery schema

```ts
type Capability<T extends object = {}> =
  | ({ state: "faithful" } & T)
  | ({
      state: "approved_adaptation";
      rules: string[];
    } & T)
  | {
      state: "unavailable" | "non_uniform" | "not_assessed";
    };

interface GenerationContract {
  scope: "route_common";
  composition: "not_asserted";
  token_limits: TokenLimits;
  modalities: ModalityCapabilities;
  features: Partial<Record<GenerationFeature, Capability>>;
  reasoning: ReasoningCapabilities;
}

interface TokenLimits {
  purpose: "planning";
  context_window_tokens: number | null;
  max_input_tokens: number | null;
  max_output_tokens: number | null;
  output_token_accounting:
    | "includes_reasoning" | "excludes_reasoning" | "not_assessed";
}

type Modality = "text" | "image" | "audio" | "video" | "document";

interface InputForm {
  source: "inline" | "url" | "file_id";
  media_types: string[];
}

type InputModalityCapability = Capability<{ forms: InputForm[] }>;

interface ModalityCapabilities {
  input: Partial<Record<Modality, InputModalityCapability>>;
  output: Partial<Record<Modality, Capability>>;
  tool_result_input: Partial<Record<Modality, InputModalityCapability>>;
}

type GenerationFeature =
  | "function_tools" | "strict_tool_schema" | "parallel_tool_calls"
  | "json_object_output" | "json_schema_output" | "max_output_tokens"
  | "temperature" | "top_p" | "stop_sequences" | "verbosity";

type EffortLevel = "minimal" | "low" | "medium" | "high" | "xhigh" | "max";

interface ReasoningCapabilities {
  generation: Capability;
  activation: Capability<{
    modes: Array<"enabled" | "disabled" | "adaptive">;
  }>;
  effort: {
    response: Capability<{ values: EffortLevel[] }>;
    reasoning: Capability<{ values: EffortLevel[] }>;
  };
  budget_tokens: Capability<{
    minimum: number;
    maximum: number;
    relation_to_output_limit: "lt" | "lte" | "independent";
  }>;
}
```

Capability states describe a limited feature domain, not model quality, live availability or successful completion:

| State | Meaning |
|---|---|
| `faithful` | The declared common domain has a faithful carrier and evidenced effective support |
| `approved_adaptation` | The common domain is admitted with possible approved adaptations |
| `unavailable` | Public admission rejects the domain, or every relevant candidate/mode is known unable to provide it |
| `non_uniform` | Known candidate or delivery-mode differences prevent a common positive declaration |
| `not_assessed` | Evidence is insufficient; absent dictionary keys have this meaning too |

Positive statements cover the configured static candidates and listed delivery modes, independent of transient health. `approved_adaptation.rules` is a nonempty, duplicate-free list of stable rule IDs describing possible, not necessarily applied, changes. Never flatten an approved omission into effective parameter support. Nonpositive states carry no positive forms, ranges or values. Consumers must not interpret unknown enum values or fields as support.

`scope: "route_common"` excludes candidate unions or strongest-target advertising. `composition: "not_asserted"` means independently supported features do not promise every combination: tools and JSON Schema may work separately but not together. Existing request checks and detailed capability domains remain authoritative; no general constraint language is introduced.

Token limits are positive JSON-safe integers or null. Null means unknown or not honestly publishable, never unlimited. `context_window_tokens` uses a shared input/generated-content capacity; do not silently substitute a differently defined native limit. Publish common values only when source facts and accounting are comparable. `max_input_tokens` is an independent limit, not context minus output; `max_output_tokens` is a ceiling, not a default. Input, generated reasoning and output share applicable constraints, so maxima cannot necessarily be used together. An unassessed output accounting scope provides no visible-output or reasoning-budget guarantee.

`purpose: "planning"` explicitly withholds exact request-admission claims: client tokenization, protocol framing and target projection can change counts. These fields neither implement token counting nor reopen its deferred endpoint. The numeric output ceiling is distinct from `features.max_output_tokens`, which describes whether a caller can express and enforce that control through this interface.

Positive input capabilities require nonempty, duplicate-free forms and MIME lists. Each form publishes a usable common source/MIME subset, not a cross-product assembled from different candidates. `inline` uses the selected protocol's text/Base64/data-URL carrier. A URL form does not authorize gateway downloading; a `file_id` form requires an established public file-identity/resolution contract and never exposes Provider-owned identity. Declaring the enum does not activate Files. Document content is a modality; a file reference is a source form. Unlisted formats remain unassessed, and size/count/resource limits still apply. Ordinary input, output and tool-result input are separate claims.

The small feature dictionary is an Agent-facing summary, not a second protocol specification. Function tools do not imply strict schemas, parallel calls or media results. Parallel calls do not authorize gateway tool execution. JSON-object and JSON-Schema output remain separate, and neither promises every schema keyword. Accepted sampling fields are not proof of effective sampling. Detailed request/delivery/history limitations remain in capabilities.

Reasoning generation does not promise raw thinking disclosure. Activation modes, qualitative effort and numeric budgets are independent; disabling reasoning is not an effort level. Retain response-wide versus reasoning-only scopes. Values are semantic choices mapped by the client adapter to the selected protocol, not universal request field values. Derive allowed source-domain values through the selected projections, not by copying target-local `allowed_effort`. Positive modes/values are nonempty and unique; positive budgets require `1 <= minimum <= maximum` within JSON-safe integers. Check `lt`/`lte` against the effective output ceiling after defaults and conversions, without clipping or raising caps. If no common interval/relation can be established, publish a nonpositive state. No guessed default mode, effort or output budget is included.

#### Discovery aggregation and ownership

| Counterexample | Required publication |
|---|---|
| Effort sets `{low, high}` and `{high}` | Publish `{high}` as the common subset, not automatically `non_uniform` |
| One candidate accepts only image URLs, another only inline images | No common input form: `non_uniform` |
| All accept the same form, but one requires an approved projection | `approved_adaptation` with possible rule IDs |
| Native support exists but public admission rejects it | `unavailable` |
| One supports a feature; another is unassessed | No common positive claim; normally `not_assessed` |
| All relevant candidates reject the feature | `unavailable` |
| Comparable 128K and 1M context capacities | The common planning ceiling may be 128K |
| Incomparable token or reasoning accounting | Corresponding common numeric limits remain null |

These states do not change runtime admission. A request outside the published common subset may still pass existing candidate checks; `non_uniform` and `not_assessed` are not new rejection policies. Neither a positive descriptor nor an adaptation list proves an entire Agent loop.

Keep canonical publication facts, trusted binding limits and downstream effective contracts in their existing owners, outside the semantic request IR. Generate the enhanced summary and detailed capabilities from one compiled contract snapshot aligned with validation/lowering. Sharing a type name is insufficient: do not publish `GenerationSemanticContract::full()` or raw admission booleans as effective support, and do not maintain a second handwritten capability registry.

#### Enhanced model example

This synthetic example describes no real model or accepted binding. List responses place the same descriptor in `data: [...]` under `object: "morphiecore.model_list"`.

```json
{
  "object": "morphiecore.model",
  "version": 1,
  "catalog_revision": "example-revision-1",
  "coverage": "partial",
  "unlisted": "not_assessed",
  "links": {
    "capabilities": "/v1/morphiecore/capabilities"
  },
  "data": {
    "id": "example-model",
    "name": "Example Model",
    "created": 1704067200,
    "owned_by": "Example Lab",
    "interfaces": [{
      "task": "generation",
      "protocol": "responses",
      "delivery_modes": ["json", "sse"],
      "generation": {
        "scope": "route_common",
        "composition": "not_asserted",
        "token_limits": {
          "purpose": "planning",
          "context_window_tokens": 131072,
          "max_input_tokens": null,
          "max_output_tokens": 32768,
          "output_token_accounting": "includes_reasoning"
        },
        "modalities": {
          "input": {
            "text": {
              "state": "faithful",
              "forms": [{"source": "inline", "media_types": ["text/plain"]}]
            },
            "image": {
              "state": "faithful",
              "forms": [{"source": "inline", "media_types": ["image/png", "image/jpeg"]}]
            }
          },
          "output": {"text": {"state": "faithful"}},
          "tool_result_input": {
            "text": {
              "state": "faithful",
              "forms": [{"source": "inline", "media_types": ["text/plain"]}]
            },
            "image": {"state": "non_uniform"}
          }
        },
        "features": {
          "function_tools": {"state": "faithful"},
          "strict_tool_schema": {"state": "not_assessed"},
          "parallel_tool_calls": {"state": "faithful"},
          "json_schema_output": {"state": "unavailable"},
          "max_output_tokens": {"state": "faithful"}
        },
        "reasoning": {
          "generation": {"state": "faithful"},
          "activation": {"state": "unavailable"},
          "effort": {
            "response": {"state": "faithful", "values": ["low", "medium", "high"]},
            "reasoning": {"state": "unavailable"}
          },
          "budget_tokens": {"state": "unavailable"}
        }
      }
    }]
  }
}
```

#### Discovery integration boundaries

Reuse ingress authentication and bounded immutable discovery; GET performs no upstream request, credential refresh or automatic capability probing. Bound model/interface counts and serialized bytes before publication. The proposed v1 list is complete and unpaginated: oversize configuration must fail rather than silently truncate a successful response. Select exact bounds and HTTP caching behavior before implementation; do not infer them from the example.

Standard SDK conformance and enhanced-metadata consumption are separate acceptance questions. Do not assume pi or opencode reads arbitrary OpenAI `/models` extensions. A future minimal integration can export version-specific client model configuration from the enhanced snapshot: context/output limits, modalities, reasoning controls and verified transport compatibility. It must omit secrets, preserve user configuration and avoid treating unsupported tools as a guaranteed client-side tool-disable switch. Dynamic provider refresh is a later choice, not a generic plugin framework requirement.

Exclude public Provider/candidate identities, upstream aliases/origins, credentials, live health, price/performance rankings, SLA claims, inferred defaults and a duplicate native-versus-effective limit catalog. No metadata endpoint changes routing, adds exact token counting, opens Files, or activates a model. Before implementation, finalize the machine schema, version/unknown-value rules, finite bounds, revision generation, evidence-backed common subsets and selected client adapter. The roadmap owns that future selection; current Usage and OpenAPI remain unchanged.

### Centralized agent authorization

The converged 2026-10-23 [Agent identity and cluster diagnostics baseline](architecture.md#agent-identity-and-cluster-diagnostics) now owns the selected direction: one deployer, no business user/tenant model, credential-independent Agent attribution, shared history and sensitive diagnostics under mandatory authentication redaction. Do not reinterpret OAuth account identity as Agent identity or rebuild a separate user/ownership hierarchy here.

Unresolved integration choices are the authorization-server product, verified OAuth authorization-instance binding to `agent_id`, registry persistence/management, client correlation carriers, token permissions and revocation policy, and diagnostic storage/export/retention. Rauthy remains the first evaluation candidate; the [dated research](refs/2026-10-23-downstream-oauth.md) owns versioned evidence and limitations. [Roadmap](roadmap.md#agent-authorization-and-diagnostics-follow-up) owns the unscheduled dependency order and acceptance gates. This is not a stage-5 API assignment, implementation approval or change to current loopback ingress.

### 1C design: Messages citations

This is an unscheduled stage-5 proposal, not a prerequisite for stages 1–4. Citation-bearing requests/responses retain their existing rejection until the design is selected, scheduled and implemented. The selected document-context and point-anchor additions do not supply the missing citation enable flag, quote or request-document association below.

Counterexample: a request holds two PDF documents with `citations: {"enabled": true}`. The response cites `{"type":"page_location","document_index":1,"document_title":"B","cited_text":"…","start_page_number":2,"end_page_number":3}`, and the client returns that text block with its citation. The IR has page coordinates but cannot hold the per-document flag, the cited text or a reference to the second request document: `Citation.source` resolves only in its owner's resource table.

Protocol facts (pinned SDK; [Citations](https://platform.claude.com/docs/en/build-with-claude/citations) and [Search results](https://platform.claude.com/docs/en/build-with-claude/search-results), read 2026-10-13):

- `document_index` is 0-based over all document blocks in the request, across messages. `search_result_index` is 0-based over all `search_result` blocks, across messages and tool results.
- Character and block ranges are 0-based; pages are 1-based. All ends are exclusive.
- Citations are enabled on all or none of the documents, and likewise for search results. They are incompatible with structured output.
- Messages cites whole text blocks, splitting text at citation boundaries. `cited_text` is not billed.

Proposed 1C-a, documents only:

| Concept | IR change |
|---|---|
| Enable flag | `FileDescription.citations: bool` per use. Messages carries it; other targets reject `true` because it changes the output contract |
| Cited source | `Citation.source` becomes `CitationSource::Resource(ResourceId)` or `CitationSource::RequestDocument(ordinal)`, counting the request's file uses in order |
| Cited text | `Citation.quote: Option<Text>`, kept exactly |
| Location | Existing `SourceCoordinates` text, page and block ranges; the claim is `Whole` |
| Other targets | OpenAI has no request-document citation. Reject an incompatible target rather than drop required evidence |

Exit condition: one hand-written Messages oracle each way for a cited PDF, one streamed `citations_delta`, and an OpenAI target rejecting the flag. A bounded real call must confirm whether documents inside `tool_result` count toward `document_index`.

Later 1C-b: `search_result` input parts and `content` documents of text and image blocks; neither has a carrier outside Messages. `web_search_result_location` belongs to Provider web search. The alternative is to keep 1C rejected: pi never sends citations, and opencode sends them only from part metadata.

### Deferred tool discovery and web search

These belong to the unscheduled stage-5 backlog. Select IR ownership and execution/replay contracts and explicitly schedule a slice before implementation. Neither blocks stages 1–4, coverage or P7; stage placement is not a delivery commitment.

- **Tool discovery and ordered availability updates.** Complete `tool_search`, its call/output items and `additional_tools`, building on deferred-loading and namespace types. Later tools must not be retroactively added to earlier history through top-level settings. Exit condition: discovery results and availability changes keep order, definitions and references through decode, encode and replay. No gateway tool registry or search executor.
- **Native OpenAI `web_search`.** Complete the selected built-in path: declaration and controls, Provider-executed observations, reported sources and URL citations, streamed lifecycle and history, reusing observation and citation foundations. Exit condition: the Responses path preserves operations, results, citation associations and honest terminals in JSON and SSE; targets without a carrier reject. The gateway never fetches URLs or searches itself.

Unsupported discovery and search semantics stay rejected, not converted to text or ordinary function calls. These slices do not reopen generated media, file services, new bindings, Realtime or Embeddings.

### Context compaction

Stage-5 future work, currently unscheduled. Existing Generation compaction-item intake/replay is not deferred with this operation.

Use a distinct request and result. The result contains returned ordered context, resources, reported accounting and operation metadata. It is not a normal assistant response.

The official compact response can retain user messages followed by a compaction item. [Generation response validation](../src/semantic/task/generation/validate.rs) excludes user/instruction messages from ordinary output. Do not weaken that validation to reuse `GenerationResponse`.

Reuse typed conversation content and [compacted-history replay](architecture.md#compacted-history). Preserve the opaque result's compatible deployment/model scope; incompatible replay rejects rather than strips required history.

Keep three operations separate: caller-supplied plaintext context editing, Provider-executed compaction, and automatic compaction scheduling. Adding `/responses/compact` does not select automatic triggers, mutate stored conversations, enable Provider-held continuation or introduce background Generation.

Exit evidence: independently specified compacted context in both directions, faithful subsequent input replay, incompatible-target rejection and honest failure handling.

### Native batch jobs

Stage-5 future work, currently unscheduled. Stage-4 native media jobs retain their own lifecycle requirements without implying this general Batch API.

| Method and path | Proposed operation |
|---|---|
| `POST /v1/batches` | Submit a batch |
| `GET /v1/batches` | List accessible batches |
| `GET /v1/batches/{batch_id}` | Retrieve state |
| `POST /v1/batches/{batch_id}/cancel` | Request cancellation |

Recommend Provider-native batch execution with durable gateway coordination. A gateway scheduler running synchronous tasks is a different product and remains an open alternative, not an interchangeable implementation detail.

Decode each JSONL record through an allowlisted operation codec into its task contract. Validate method/path, public model, resource references and `custom_id`; project only to a trusted compatible native batch target. Re-encode typed requests rather than forwarding arbitrary JSONL to a selected origin.

Define mixed-model/deployment admission explicitly. Do not silently split a batch across credentials or Providers, alter record identity, or turn unsupported records into a local execution loop. Stream bounded validation and result processing rather than loading the full batch into Generation memory.

Preserve per-record outcome, request/result association, job state and output/error file references. Native batch endpoint support is a separate capability: image operations appear in the current OpenAI contract, but that does not establish native batch support for audio operations.

Select the durable acceptance point and crash-recovery contract. A submission timeout may follow successful remote creation; persist enough private identity to reconcile before retrying. Do not claim exactly-once submission without upstream evidence.

After acceptance, a disconnected client does not cancel the job. Explicit cancellation distinguishes requested, confirmed and already-completed work. A cancelled or expired job may still have valid completed records; job failure must not fabricate record outcomes.

Exit evidence: accepted-job recovery, lost submission acknowledgement, partial results, identity mapping, explicit cancellation, output/error file access and no accidental resubmission.

### Realtime

Stage-5 future work, currently unscheduled. No concrete session API, transport or event contract is selected. Choose the client scenario, media directions, interruption/cancellation, reconnection, session/resource ownership and bounded delivery before implementation. Ordinary streamed Generation and stage-4 audio/video operations do not establish Realtime support.

### Deferred cache prewarming

Native cache-only prewarming is not a prerequisite for first-stage completion, coverage or P7. Existing cached-context references keep their separate contract.

The [pinned pi cache warmer](https://github.com/earendil-works/pi/blob/4270522473e677ddba7b093caa0064a3df51d459/packages/coding-agent/src/core/cache-warmer.ts) replays a real request with a one-token cap and keeps refresh usage outside history. That is ordinary generation owned by the client strategy, not native cache-only support; no gateway-owned warmer is selected.

When reopened, select the cache-only operation and result contract from pi's strategy and official native carriers. Exit condition: exact codecs preserve the operation, usage and reported cache facts, distinguish prewarming completion from output and reject targets without carriers. `max_tokens: 0` is not a zero-length answer; never replace explicit no-output prewarming with one-token generation.

### Model-name service-policy carriers

Service-policy carriers may be upstream model names rather than a dedicated field. A trusted, evidence-backed binding declaration must associate an alias with the complete policy it represents; never infer a tier from a suffix or accept a client-supplied upstream selector. A different model version, quantization or context contract is not automatically a tier of the same model. The current [endpoint contract](../src/topology/endpoint.rs) fixes the upstream model at binding time; choosing predeclared variants during preparation is an integration design question, not a codec-only string rewrite.

When scheduled, add an independent alias-carrier oracle and an incompatible-policy rejection. Preparation must agree with preflight on the effective deployment, and cache/replay identity must follow the selected wire model. A selected alias proves request projection, not delivered service; record a reported tier only from an authoritative report.

## Open decisions

These recommendations have not been adopted merely by requesting this document.

| Decision | Options and recommendation | Blocks |
|---|---|---|
| Dual-SDK Decision contract | Both standard SDK families and shared IR/codec/dialect adaptation are selected. The [Decision review](#decision-execution-and-review-choices) owns remaining roots, evidence/result types, projections, delivery and execution choices | Decision implementation and integration, not independent source research |
| Prioritized Images | The [Image review](#image-design-review) owns pending structural choices. The selected Embedding contract has moved to [Architecture](architecture.md#independent-embedding-path); future extensions remain unselected | Dependent S4-3 implementation and integration |
| Task grouping and shared content | Recommend independent TokenCount/Compaction, AudioToText variants, and Image Generate/Edit variants; share content without inheriting Generation execution | New task type and public Rust contract changes |
| Counting companion surface | Recommend Responses input-token counting; image generations/edits already belong to stage 4 | Adding the Responses counting surface to delivery scope |
| Counting profile and public scope | **Deferred scheduling; profile unselected.** When reopened, select the public-model/target profile, upstream reliability criteria, coarse estimation rule/input coverage and client-visible scope/provenance. [Upstream preference, explicit estimates for inaccurate reports, no gateway tokenizer counting and no inference side effects](architecture.md#token-counting-policy) are selected, not open alternatives | Cross-protocol counting admission and client promises, only after reopening |
| File authority | Recommend upstream-native content plus gateway public IDs/durable mappings first. Gateway-owned content with target replicas improves portability but adds storage and replication ownership | Files/Uploads and resource-based failover |
| Retention and deletion | Select expiry, deletion with active jobs/history, cascade behavior and restart guarantees. Durable jobs and resource mappings cannot inherit process-local Responses TTL accidentally | Resource/job lifecycle implementation |
| Batch execution | Recommend native Provider batches. A gateway scheduler is a separate product with different billing, timing and cancellation semantics | Batch implementation |
| Batch composition | Select compatible model/deployment restrictions and supported endpoint set; recommend explicit rejection over implicit cross-target splitting | Batch admission |
| Custom voices | Recommend authorized voice references first; voice creation/consent management as a separate resource slice. Broader audio resource coverage may be selected explicitly | Voice-resource endpoints, not ordinary speech |
| Video contracts | Select current operations/endpoints, task versus content ownership, artifact access and any required job lifecycle before implementation | Video implementation, not stages 1–3 |
| Realtime design | Client scenario, session/transport and interruption/recovery contracts are unselected; keep stage 5 unscheduled | Realtime implementation |
| Later-operation selection | Generation Text closure and the selected Embedding design are complete; Decision design now precedes further modality work, with static Generate still the next Images review. Other stage-4 operation contracts need just-in-time selection. All stage-5 tasks remain unscheduled | Per-operation design, activation and full-chain acceptance |

## Execution handoff

The [roadmap work packages](roadmap.md#execution-sequence-and-work-package-rules) own sequence, dependencies and acceptance gates, including the selected pre-stage-4 production closure. This page does not maintain a second implementation queue. Before starting a package, select its request/result/event and lifecycle boundaries, record accepted choices in Architecture, and retain only unresolved alternatives here.

Extract shared values only when the selected operation needs them. Direct-input media do not depend on the complete file service; authorized file-reference paths do. Stage-4 native media jobs keep their own lifecycle without implying the general Batch API. Do not perform a repository-wide ownership rewrite or prebuild stage-5 infrastructure as a prerequisite for the first operation.

## Verification

Pin current official schemas and streaming grammars before implementation; documented carriers do not prove model-effective support. Use the existing [test design](development.md#test-layout), [design-review gate](roadmap.md#generation-design-review-gate) and [verification procedures](development.md#verification-gates), including separately scoped [real Provider checks](development.md#real-provider-checks).

Stage-4 implementation follows the [current text/SSE prerequisite](roadmap.md#current-text-and-sse-gate) and includes [operation-level upstream-test acceptance](roadmap.md#stage-4-reuse-and-acceptance). Reuse the shared source/adaptation procedures rather than a separate modality harness; this does not select unresolved operation contracts.

Keep independent wire-to-IR and IR-to-wire expectations, covering success and intended rejection at the responsible owner. Extend existing tests rather than building per-model/Provider matrices. Resource/job checks must address authorization, recovery, cancellation and uncertain submission.

Compare the [OpenAPI summary](openapi.json) with implemented ingress and codecs; never publish planned endpoints as implemented paths. Preserve safety assertions and frozen routing. Report uncovered layers: codec support is not public admission, compilation is not runtime acceptance, static results are not streaming support, and remote success is not crash recovery.

## Protocol references

These sources informed the proposal. Web references are mutable; recheck them and select pinned SDK revisions in [References](references.md) when implementation starts. Source inspection is not a real Provider acceptance result.

| Contract | Official source |
|---|---|
| Decisions | [Dated four-Provider and SDK findings](refs/2026-10-23-decision-contracts.md), with official schema/source links and unresolved discrepancies |
| Responses input counting | [Count input tokens](https://developers.openai.com/api/reference/resources/responses/subresources/input_tokens/methods/count) |
| Messages counting | [Count tokens](https://platform.claude.com/docs/en/api/messages/count_tokens), [counting semantics](https://platform.claude.com/docs/en/build-with-claude/token-counting) |
| Compaction and state | [Compact](https://developers.openai.com/api/reference/resources/responses/methods/compact), [conversation state](https://developers.openai.com/api/docs/guides/conversation-state) |
| Embeddings | [Create embeddings](https://developers.openai.com/api/reference/resources/embeddings/methods/create) |
| Speech | [Create speech](https://developers.openai.com/api/reference/resources/audio/subresources/speech/methods/create) |
| Transcription | [Create transcription](https://developers.openai.com/api/reference/resources/audio/subresources/transcriptions/methods/create) |
| Translation | [Create translation](https://developers.openai.com/api/reference/resources/audio/subresources/translations/methods/create) |
| Voice resources | [Audio resource reference](https://developers.openai.com/api/reference/resources/audio) |
| Images | [Generate](https://developers.openai.com/api/reference/resources/images/methods/generate), [edit](https://developers.openai.com/api/reference/resources/images/methods/edit) |
| Files | [File operations](https://developers.openai.com/api/reference/resources/files) |
| Uploads | [Upload operations](https://developers.openai.com/api/reference/resources/uploads), [completion](https://developers.openai.com/api/reference/resources/uploads/methods/complete) |
| Batch | [Batch operations](https://developers.openai.com/api/reference/resources/batches), [submission contract](https://developers.openai.com/api/reference/resources/batches/methods/create) |
| Chat-carried audio | [MiMo TTS](https://mimo.mi.com/docs/en-US/api/audio/tts), [MiMo ASR](https://mimo.mi.com/docs/en-US/api/audio/Speech-Recognition) |
| Comparison corrections | [Messages roles](https://platform.claude.com/docs/en/api/messages/create), [Gemini TTS through generation](https://ai.google.dev/gemini-api/docs/generate-content/speech-generation) |
