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
  - [Token counting](#token-counting)
  - [Embeddings](#embeddings)
    - [Embedding chain and ownership](#embedding-chain-and-ownership)
    - [Embedding inputs and admission](#embedding-inputs-and-admission)
    - [Embedding results and encoding](#embedding-results-and-encoding)
    - [Google Embedding profile](#google-embedding-profile)
    - [Embedding design review](#embedding-design-review)
  - [Audio](#audio)
  - [Image production and editing](#image-production-and-editing)
    - [Image requests and target admission](#image-requests-and-target-admission)
    - [Image results and delivery compatibility](#image-results-and-delivery-compatibility)
    - [Image streaming lifecycle](#image-streaming-lifecycle)
    - [Image design review](#image-design-review)
  - [Video API and implementation](#video-api-and-implementation)
- [Resource services](#resource-services)
  - [Files and uploads](#files-and-uploads)
- [Unscheduled designs](#unscheduled-designs)
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

The subsequent Google-model, encoding and ancillary compatibility selections are recorded separately in Architecture: [four-way encoding](architecture.md#embedding-encoding-compatibility), including named standard binary32 rounding, and [Google report/identity projection](architecture.md#google-embedding-compatibility) are selected behavior. The layouts below remain proposed; the older exact-only conversion and strict-missing-usage recommendations are superseded.

## Requested scope

The user selected OpenAI-first downstream contracts, broad coverage of current audio and image APIs, and no compatibility work for historical protocols. The subsequent five-stage plan explicitly adds video API/implementation adaptation to stage 4. OpenAI-first does not mean OpenAI-only upstream execution; video operations and concrete endpoints remain to be selected from current protocol evidence.

| Requested surface | Expansion goal |
|---|---|
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

### Token counting

Selected for stage 3 with cross-protocol conversion and **deferred on 2026-10-21** ([roadmap](roadmap.md#deferred-token-counting)); the contract below is unstarted design reference. The Messages `count_tokens` surface is confirmed; the Responses counting companion still requires selection.

Model a count request from count-relevant conversation content: instructions, ordered history, tools, media and applicable settings. It is not generation with a zero output cap. A count result is not inference usage and must not contain fabricated generated output.

OpenAI documents `POST /v1/responses/input_tokens`; Anthropic documents `POST /v1/messages/count_tokens`. Reuse one task boundary while preserving distinct source semantics and result envelopes. Resolve any admitted gateway-owned history reference before target preparation; never forward a gateway ID as a Provider-owned reference.

The [upstream-first counting policy](architecture.md#token-counting-policy) is selected: prefer upstream counts, allow explicit coarse estimates for inaccurate upstream reports, and do not build a tokenizer-based exact-counting path. Identify the model/target, effective input projection and count source; a Messages-shaped request does not imply an Anthropic model or counting method.

Select the concrete profile's upstream reliability criteria, coarse estimation rule and supported input coverage. A character-based rule must account for the projected instructions, history and tool definitions/results, not just the latest user text. For heuristic fallback, media require an explicit estimate policy or rejection, not a zero-cost assumption. Select client-visible source/approximation reporting compatible with the result envelope; do not invent standard API fields or promise billing equality.

Exit evidence: independent request/result oracles cover upstream preference and the selected inaccurate-report fallback, count-relevant controls and media, explicit scope/provenance, no generation side effects and no tokenizer execution. Reject input outside the selected counting profile rather than silently omit it. One candidate's result still neither covers every failover candidate nor reserves the next generation route.

### Embeddings

Extend the [independent task](architecture.md#independent-embedding-path), not Generation. The current implementation accepts text and float JSON only. The [source review](refs/2026-10-22-embedding-image-contracts.md#embeddings) establishes four official input shapes and two output encodings; importantly, the pinned Python SDK requests Base64 when its caller omits encoding. A working explicit-float request is not ordinary SDK compatibility.

The encoding and [Google ancillary compatibility policies](architecture.md#google-embedding-compatibility) are selected behavior. The following is the converged implementation recommendation, not a claim that its structures or bindings exist. Keep the three-target scope in the [roadmap](roadmap.md#embedding-integration-targets); do not make Google-specific uncertainties block independent shared conversion work.

#### Embedding chain and ownership

Preserve the existing authenticated, single-target request chain:

```text
auth → bounded ingress / strict JSON → activated model → pure target preparation
  → one credential resolution / HTTP request
  → bounded native intake through EOF → source validation / request association
  → typed client projection + receipts → client encoding / size check
  → acknowledged body handoff → success
```

Preflight and actual preparation use one pure target-projection function, not independent mapping tables or post-serialization JSON patches. Carry a small operation-local expectation containing input count, effective dimensions, selected upstream encoding and downstream delivery policy to intake/projection. Retain the immutable client request; target copies resolve defaults and permitted omissions. Emit applied receipts once for the actual attempt, not again for a dry-run preflight.

| Responsible owner | Minimal adaptation |
|---|---|
| [Task values](../src/semantic/task/embedding.rs) | Ordered input alternatives, one numeric authority per vector, source usage with independent report presence/basis. No protocols, credentials or execution state |
| [Adapter](../src/adapter/embeddings.rs) | Client output preference, optional reported model/response ID and a trusted native profile. Keep bound/public identity outside the source report |
| [Lowering](../src/lowering/embeddings.rs) | Effective defaults and range-aware admission; typed target/client projection and bounded operation-local receipts. Reuse no Generation reducer or universal loss framework |
| [OpenAI codec](../src/protocol/openai/embeddings.rs) and proposed Google codec | Directional wire parsing/encoding, numeric wire layouts and native request-order association. Stop importing the image codec's closed-object reader for upstream responses; validate known fields and ignore unknown members locally |
| [Execution](../src/execution/embeddings.rs) and [gateway handler](../src/gateway/tasks/embeddings.rs) | Trusted path/auth, expected upstream representation, EOF validation, projection and bounded delivery. No retry, asynchronous job or hidden per-item fan-out |
| [Topology](../src/topology/embeddings.rs), [catalog](../src/topology/catalog/embeddings.rs) and startup | Fixed model-effective profile and one nonfallback API-key source; explicit activation. For the one selected Google model, declare its concrete operation path rather than adding a generic path-template/router framework |

The current gateway checks admission before resolving credentials, then re-prepares, receives JSON through EOF, validates against the request and projects. Retain its separate body/exchange deadlines, shutdown/client-drop cancellation, permit lifetime and handoff acknowledgement. Conversion failure must occur before any output is published. Do not extend time limits just because a request is a native synchronous batch.

Close integration gaps at their existing owners: retain the currently discarded response-ID-omission evidence; add body-free Embedding projection receipts to diagnostics; propagate the capture exchange handle to explicitly enabled upstream capture with auth redaction. Extend the existing authenticated capability snapshot/possible-adaptation notice to activated Embedding profiles, covering input kinds, effective dimension/default/count bounds, both encodings and selected report rules. `/v1/models` remains discovery, not a capability bag. A public optional-usage promise must not ship without that profile description.

Non-200 responses currently discard the error body. Add a small capped native-error read within the same deadline and release lifecycle; classify only established HTTP/status/reason facts into existing safe error codes, preserve bounded Retry-After where applicable, and never relay upstream messages/details. `INVALID_ARGUMENT` alone is not proof of token overflow; an unrecognized error remains `upstream_error`. Malformed, oversized or incomplete error bodies cannot turn into success.

#### Embedding inputs and admission

Propose one homogeneous, ordered input alternative per request:

```text
EmbeddingInput =
  Text(nonempty ordered strings)
  TokenIds(nonempty ordered token sequences, logical tokenizer scope)
EmbeddingRequest = { input, optional dimensions }
```

Scalar text and one token sequence normalize to one-element batches; wire shape is the codec's responsibility. Reject empty inputs, mixed text/token batches, negative/fractional/boolean/overflow token IDs and empty inner sequences. Propose bounded unsigned 32-bit IDs; tokenizer vocabulary and special-token restrictions belong to the selected tokenizer contract, not an assumption that every nonnegative ID is valid.

Token IDs acquire a logical tokenizer scope from the trusted public-model contract. The same integer sequence under another tokenizer can mean different text. A bounded codec input carrier may hold unbound token sequences while the adapter resolves that contract; only scoped values enter the task. Pure codecs receive explicit parsing context where needed, never consult topology or credentials. A target must attest the same tokenizer scope before encoding. Do not detokenize, retokenize, silently concatenate inputs, chunk text or split a batch into multiple requests. This introduces no gateway tokenizer execution or token-counting endpoint.

Keep model names, identity, downstream encoding, target selection and runtime limits outside the task. Extend the existing target contract rather than introducing a capability registry:

| Contract fact | Proposed use |
|---|---|
| Admitted input kinds and tokenizer scope | Reject unsupported token inputs before dispatch; public model aliases cannot imply tokenizer equivalence |
| Default dimensions | Resolve omitted dimensions for output-budget admission and subsequent result validation |
| Explicit dimension support | Separately express unsupported, a closed range or selected values; a fixed-size model need not accept an explicit control equal to its default |
| Per-input and aggregate token limits | Check token-sequence lengths exactly. Text byte limits are not token counts; leave text token enforcement upstream unless separately selected |
| Float/Base64 support and binary layout | Choose upstream representation on a typed target copy while retaining the client's delivery preference |
| Input count, body bytes, vector-value and time limits | Enforce independent local and target ceilings without hidden fan-out |

Initially retain the local 128-input, 256-KiB text, 8,192-dimension and 60,000-vector-value ceilings unless a concrete selected profile justifies a revision. Token inputs also need aggregate-ID and serialized-request byte limits; an integer vector is not charged as an empty text input. Official limits and local limits remain distinct. The source's ambiguous “any array … 2048 dimensions” wording needs profile-specific resolution, not silent application to every token sequence.

Keep current 256-KiB serialized upstream requests, 4-MiB upstream response and 120-second target deadline as initial ceilings, intersected with configured runtime limits. Text bytes, JSON wrappers/escaping, decoded numeric bytes/values and encoded client bytes are separate budgets. Base64 compression is not permission to exceed vector-value or expanded JSON delivery limits. No client-visible dimension reduction or automatic splitting repairs an over-budget request.

Preflight checked `input_count × effective_dimensions` for both explicit and default dimensions. Current lowering checks only explicit dimensions: 40 inputs at the catalog's 1,536-dimensional reference model produce 61,440 values, over the existing result budget, even when admission accepts omitted dimensions. Reject this predictable overflow before upstream work. If a target has no established default or finite output bound, it is not ready for that omitted-dimensions profile.

#### Embedding results and encoding

Retain ordered vectors, each with its reported input index. Require exactly one result per input, unique indexes covering `0..input_count`, consistent dimensions and agreement with admitted effective dimensions. Array position cannot replace the reported index; preserve response order. Do not truncate, pad, renormalize or substitute a nearby embedding space.

Keep exact JSON numbers authoritative for float reports. Base64 is a wire encoding, not an opaque semantic vector. Its selected binary layout must define finite binary32 elements and byte order; reject invalid padding/length, non-multiple-of-four payloads, nonfinite values and dimension/budget mismatches before retaining decoded vectors. SDK-native `float32` parsing is evidence of precision, not a portable byte-order specification.

**Representation recommendation for review:** use one numeric alternative per vector, `ExactJsonNumbers | FiniteBinary32`. Float reports keep the existing exact-number owner; validated binary reports retain finite binary32 values/bits. There are not two authorities for the same vector, and no unvalidated Base64 sidecar. This avoids exact decimal expansions and a general precision framework while preserving existing float-only callers. Do not switch all source reports to `f32`.

The [selected encoding policy](architecture.md#embedding-encoding-compatibility) replaces exact-only conversion rejection and owns all four conversion behaviors and precision/range rules. Request preparation selects upstream encoding independently from the client's output preference; the execution/delivery path carries both choices without changing task meaning.

Propose explicit little-endian IEEE-754 binary32 for the downstream OpenAI-compatible Base64 profile, standard alphabet/padding, and separate upstream layout declarations. Pin independent byte fixtures before implementation acceptance; SDK-native-endian code is not universal wire evidence. Numeric review cases include JSON `0.1`, Base64 decoded into binary64-client-visible JSON, signed zero, subnormals and a decimal just above a binary32 midpoint where intermediate binary64 would hide the difference. These exercise the selected policy rather than require exact decimal/binary32 equality.

Raw HTTP omission of `encoding_format` means float; SDK omission can arrive as explicit Base64. Preserve that distinction at ingress. Decoding validates the expected upstream representation, and conversion completes before response publication. A Base64 response is still one static JSON response, not an embedding stream.

Separate the native `Response` from the typed client delivery view. Recommend optional reported model/ID in the adapter; an explicit bound-model input to projection supplies the selected Google label without changing native provenance. Keep independent input/total report presence and a narrowly typed accounting basis in `EmbeddingUsage`, plus bounded modality details. Preserve the existing DashScope constructor's meaning: reported input-only total → input view is not Google's reported input → derived total. An empty/missing aggregate cannot become a constructor-supplied zero.

Strict OpenAI encoding still requires its complete report shape. The explicit Google client profile may encode the selected missing-usage view; other profiles do not inherit it. Known null/wrong-type/negative/overflow counters reject instead of being reclassified as missing. Preserve valid detail reports when the aggregate is absent, but do not sum them into an aggregate. With both scalar reports present, retain `total >= input` validation. Source usage remains reported facts; derived delivery counters and omission provenance belong to the projection result.

Use a closed Embedding receipt set for numeric rounding, selected model/accounting/user projection and existing response-ID omission. Include rule, direction and fixed action/presence only—no text, vector components, `user` value or arbitrary source keys. Give the existing ID-omission projection the fixed receipt name `embedding.omit-response-id.v1`; do not import Generation state or create an evidence service. Native errors and malformed known fields are validation failures, not allowed losses.

Keep one static attempt, bounded intake through EOF, deadlines and cancellation. No new credential fallback or cross-model failover is selected; equal dimensions prove neither tokenizer nor embedding-space compatibility. Retrieval intent, multimodal embeddings, sparse vectors, indexes and chunking remain outside this slice.

#### Google Embedding profile

The [selected target list](roadmap.md#embedding-integration-targets) now includes stable **`gemini-embedding-2`**, initially text-only. Its larger 8,192-token window and upstream normalization at reduced dimensions make it preferable here to `gemini-embedding-001` (2,048 tokens and caller normalization below its default). Neither model is a failover substitute for the other. The [dated Google sources](refs/2026-10-22-embedding-image-contracts.md#google-embedding-selection) own model observations and protocol disagreements.

Recommend native synchronous `POST /v1beta/models/gemini-embedding-2:batchEmbedContents` through the existing trusted Google origin and API-key loader. Pin the [REST/Discovery baseline](refs/2026-10-22-embedding-image-contracts.md#google-wire-source-reconciliation), not the SDK's older converted object. Google's OpenAI compatibility endpoint is not a shortcut: its guide still uses the preview slug, lacks the full parameter contract, and historical missing-usage evidence concerns 001 rather than stable Embedding 2.

Each downstream input becomes one native `requests[]` entry with one text `Content`; use the same bounded synchronous carrier for scalar and batch requests. Do not put separate inputs into one `Content.parts` array: Embedding 2 can aggregate those into one vector. Native response order supplies indexes only because this protocol explicitly guarantees request order; other protocols retain their reported indexes. A synchronous batch-shaped request is one HTTP attempt, not stage-5 Batch jobs or gateway fan-out.

Place `model:"models/gemini-embedding-2"`, `content.parts:[{text:...}]` and `embedContentConfig` inside **each** request entry. The batch envelope has no shared config. Use current nested `outputDimensionality` and candidate `autoTruncate:false`; never send deprecated top-level controls at the same time or retry with them after rejection. Omit role, task type, title, OCR and audio controls. Omitted dimensions stay omitted on the wire but resolve to 3,072 for admission/result validation.

The documented dimension range is 128–3,072, not `UpTo(3072)` from one; 768/1,536/3,072 are recommendations, not an exhaustive enum. Propose a conservative 100-entry local batch cap, based on historical SDK practice rather than a current model guarantee, further bounded by `min(128, floor(60000 / effective_dimensions))`. At default width only 19 inputs fit; 20 must reject before dispatch. A body-size ceiling may lower that count further. Do not raise the cap, shrink dimensions or split requests implicitly.

Current `ContentEmbedding.values` supplies dense numeric arrays. Require exact cardinality and width; zero/missing/shortened result lists reject the entire operation. The current schema's `shape` belongs to soft-token tensors, not dense Embedding output: validate a supplied shape's integer structure and reject nonempty tensor shape in this profile rather than silently flattening it. Known numeric fields remain strict; unknown members are ignored. No synchronous per-item error/partial-success union is selected.

Native vectors are numeric arrays, so Google float→Base64 will use the selected gateway conversion; Google need not provide Base64 natively. Native input admits text content, not pretokenized integer IDs. Retain text-only admission until a real tokenizer-compatible carrier exists; accepting OpenAI token IDs and guessing Google text would change the input. Embedding 2 does not support `taskType`: do not inject query/document prefixes or titles into caller text.

The [selected Google compatibility policy](architecture.md#google-embedding-compatibility) now resolves model/usage/`user` behavior; only its typed representation and activation evidence remain. Native usage is **batch-level** `usageMetadata.promptTokenCount` and `promptTokenDetails`, not a per-vector count and not Vertex's differently named report. Keep reported aggregate presence, derive only the selected client view, and explicitly advertise possible omission of the entire usage object. Do not confuse loose SDK parsing with strict OpenAI conformance.

**Remaining wire gate:** establish that nested config is honored and explicit `autoTruncate:false` prevents input loss for this Developer API model. REST and Discovery describe it, but both inspected SDKs locally reject it; neither a successful short request nor the Vertex default settles the issue. A specific over-limit rejection compared with an admitted short control is required. If unsupported or ineffective, stop activation and select another evidenced no-loss contract or seek explicit truncation approval; never silently remove the control. Byte admission does not prove a token limit. This gate does not block shared conversion, pure codec fixtures or existing-model repair.

The existing [publication workflow](roadmap.md#embedding-integration-targets) keeps account access and bounded Provider gates separate. Google API-key authentication uses `x-goog-api-key`; do not route through Vertex project/location endpoints or substitute an OpenAI Bearer request to bypass native-contract questions.

Compatibility covers realizable OpenAI features per model, not the intersection of their encodings. Text/batches, native dimensions and both client output encodings are the main path. Token-ID admission remains per tokenizer/carrier; native model restrictions are not losses that numeric-rounding approval can waive. Upstream normalization at selected dimensions is left intact, with no local vector rewriting or automatic task prefix.

#### Embedding design review

| Decision | Recommendation and alternative | Blocks |
|---|---|---|
| Token scope and input construction | Trusted logical tokenizer contract plus scoped token inputs; text-only profiles remain valid. Reject unknown scope instead of adding a local tokenizer | Token-ID admission, not text repairs |
| Numeric owner and binary layout | Four encoding combinations and standard rounding are selected. Recommend a local exact-JSON/binary32 alternative and explicit little-endian wire profile; review numeric structure and independent byte fixtures | Base64 codecs/delivery and ordinary SDK acceptance |
| Profile and bounds | Keep existing local ceilings; add trusted default dimensions and range-aware explicit controls. The [target list](roadmap.md#embedding-integration-targets) now includes Google; binding/account access remain separate | Implementation profile and end-to-end acceptance |
| Google wire baseline | Use nested config from pinned REST/Discovery; keep fixed per-input synchronous carrier and conservative local bounds. Verify no-truncation behavior directly | Google activation, not offline codec/shared conversion |
| Native reports and delivery view | Ancillary behavior is selected. Review optional reported model, independent usage presence/basis and typed client view; no anonymous zero/default injection | Google result implementation and its capability/notice disclosure |

Review evidence should include independent directional examples for all admitted input alternatives, all four encoding combinations, default/explicit-dimension parity, reordered indexes, exact decimal and binary fixtures, midpoint/subnormal/signed-zero/range cases, intended malformed-field failures and cancellation/EOF. Add Google per-input-versus-aggregation, response order, model/usage provenance, nonempty tensor-shape rejection and truncation cases. SDK acceptance must exercise omitted/float/Base64 calls through each selected profile, including a synthetic Base64-only upstream→float client and float-only Google-shaped upstream→SDK-default client. Check strict-client rejection of intentionally missing usage as a declared limitation, not a failing vector parser to bypass.

Use synthetic Router checks for missing/malformed reports, aggregate-plus-details versus detail-only receipt actions exactly once, permitted `user` omission, one transmitted request, local batch/default-width bounds, errors/Retry-After, source immutability, withheld EOF, cancellation/permit release, body-free receipts and opt-in capture redaction. Repair the startup test's assumption that every Embedding provider has an `API_KEY_BINDINGS` Generation entry; Google must activate independently, without enabling its Generation routes. Do not use compilation or catalog registration as account/access evidence.

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
| Grok public contract and native controls | Strict OpenAI baseline plus a separately selected explicit MorphieCore dialect; alternatively native surface or disconnected Grok | Grok public admission, defaults and binding, not its existing pure codec |
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
| Prioritized Embedding and Images | The [Embedding review](#embedding-design-review) and [Image review](#image-design-review) own the concrete pending choices; the user selected design work, not their structural alternatives | Dependent S4-1/S4-3 implementation and integration only |
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
| Later-operation selection | Generation Text closure is complete; Embedding and Images are the current design priority. Other stage-4 operation contracts still need just-in-time selection. All stage-5 tasks remain unscheduled | Per-operation design, activation and full-chain acceptance |

## Execution handoff

The [roadmap work packages](roadmap.md#execution-sequence-and-work-package-rules) own sequence, dependencies and acceptance gates, including the selected pre-stage-4 production closure. This page does not maintain a second implementation queue. Before starting a package, select its request/result/event and lifecycle boundaries, record accepted choices in Architecture, and retain only unresolved alternatives here.

Extract shared values only when the selected operation needs them. Direct-input media do not depend on the complete file service; authorized file-reference paths do. Stage-4 native media jobs keep their own lifecycle without implying the general Batch API. Do not perform a repository-wide ownership rewrite or prebuild stage-5 infrastructure as a prerequisite for the first operation.

## Verification

Pin current official schemas and streaming grammars before implementation; documented carriers do not prove model-effective support. Use the existing [test design](development.md#test-layout), [design-review gate](roadmap.md#generation-design-review-gate) and [verification procedures](development.md#verification-gates), including separately scoped [real Provider checks](development.md#real-provider-checks).

Keep independent wire-to-IR and IR-to-wire expectations, covering success and intended rejection at the responsible owner. Extend existing tests rather than building per-model/Provider matrices. Resource/job checks must address authorization, recovery, cancellation and uncertain submission.

Compare the [OpenAPI summary](openapi.json) with implemented ingress and codecs; never publish planned endpoints as implemented paths. Preserve safety assertions and frozen routing. Report uncovered layers: codec support is not public admission, compilation is not runtime acceptance, static results are not streaming support, and remote success is not crash recovery.

## Protocol references

These sources informed the proposal. Web references are mutable; recheck them and select pinned SDK revisions in [References](references.md) when implementation starts. Source inspection is not a real Provider acceptance result.

| Contract | Official source |
|---|---|
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
