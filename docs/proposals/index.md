# API Expansion Proposals and Design Framework

This page owns proposed operation, resource, and advanced Generation designs. The requested scope is confirmed; structural recommendations remain proposals unless already established in [Architecture](../architecture/index.md). No new interface, binding, resource service, or execution policy is activated by this document.

<a id="operation-contracts"></a>
The expansion proposals are modularized by operational domain:

| Domain | Proposal document | Scope & Target Stage | Status |
|---|---|---|---|
| **Decision API** | [Decision API Proposal](decisions-api.md) | Stage 4: OpenAI Decisions + TypeSafe System One dual-SDK support | Ready for review |
| **Multimodal Operations** | [Multimodal Media Proposals](multimodal.md) | Stage 4: Audio (Speech/Transcription), Images (Generate/Edit), Video (Grok H3) | Pending review |
| **Tasks & Counting** | [Standalone Tasks and Token Counting](tasks-and-counting.md) | Stage 4: Responses input token counting, Messages token count, Embeddings expansion | Pending review |
| **Files & Uploads** | [Files and Uploads Resource Proposals](files-and-uploads.md) | Stage 4: Files management, multipart uploads | Pending review |
| **Stage 5 Backlog** | [Stage 5 Unscheduled Proposals Backlog](stage5-backlog.md) | Stage 5: Native batch, Realtime, Context compaction, Rauthy OAuth, Citations | Unscheduled |

[Architecture](../architecture/index.md) defines accepted principles, domain contracts, and architectural decisions. [Roadmap](../roadmap/index.md) defines scope, priorities, and acceptance gates. Configuration and operational commands belong to [Usage](../usage/index.md); development conventions and overall acceptance status belong to [Development](../development.md).

## Contents

- [Status and authority](#status-and-authority)
- [Requested scope](#requested-scope)
- [Shared design boundaries](#shared-design-boundaries)
  - [Proposed ownership](#proposed-ownership)
  - [Generation boundary review](#generation-boundary-review)
    - [Retain legitimate conversation semantics](#retain-legitimate-conversation-semantics)
    - [Independent output ceilings (unselected)](#independent-output-ceilings-unselected)
    - [Google output report owners (unselected)](#google-output-report-owners-unselected)
    - [Review concrete ownership coupling](#review-concrete-ownership-coupling)
    - [Integration constraints to revisit](#integration-constraints-to-revisit)
  - [Three-protocol tool conversion design](#three-protocol-tool-conversion-design)
    - [Candidate approximation policies](#candidate-approximation-policies)
    - [Ownership and return path](#ownership-and-return-path)
  - [Resource and execution safety](#resource-and-execution-safety)
- [Open decisions](#open-decisions)
- [Execution handoff](#execution-handoff)
- [Verification](#verification)
- [Protocol references](#protocol-references)

## Status and authority

This page owns proposed operation, resource and advanced-Generation designs. The requested scope is confirmed; structural recommendations remain proposals unless already established in [Architecture](../architecture/index.md). No new interface, binding, resource service or execution policy is activated by this document.

[Roadmap](../roadmap/index.md#planned-order) owns remaining execution order and the current routing freeze. Stage 3 closed its selected projection/loop scope with `count_tokens` deferred. [Unscheduled qualification issues](../roadmap/unscheduled-issues.md#unscheduled-issues), including R09 account failover, require individual re-entry selection and do not gate Stage 4. Decision, Embedding/audio/image/video and Files/multipart Uploads then advance as their contracts are selected. Server-side compaction, Batch and Realtime join complete citations/discovery/hosted tools in stage 5. **All stage-5 tasks are currently unscheduled:** scope placement is not a start or delivery commitment. Detailed operation designs remain proposals; stage-4/5 work is not an exit condition for earlier stages.

Code and tests describe current behavior, not authority over official contracts. The accepted [standards reset and Generation redesign](../architecture/decisions.md#scope-and-activation-decisions) governs further modality implementation and supersedes conflicting legacy mapping assumptions here. This document records unselected operation/resource designs, not a catalog or conformance claim. Provider and binding availability belongs to [the catalog](../../src/topology/catalog.rs) and [startup](../../src/gateway/startup/bootstrap.rs).

Under the [stage-4 scheduling decision](../roadmap/index.md#stage-4-scope), Decision, Embedding, Audio, Images, Files/Uploads and Video have equal priority; only their own contract selection and actual dependencies constrain a slice. The proposals below do not approve new IR layouts, delivery dialects, comprehensive coverage or bindings.

The subsequent Embedding selections are recorded in Architecture: [four-way encoding](../architecture/index.md#embedding-encoding-compatibility), sample-centered inputs and [report/identity projection](../architecture/index.md#google-embedding-compatibility) are implemented. Complete downstream usage with explicit request-based approximation supersedes missing-usage delivery; it does not reopen the deferred counting endpoint. This document retains only unselected Embedding extensions.

With [dual-SDK Decision access](../architecture/index.md#decision-api-direction) selected, its design, implementation and acceptance closure belongs to stage 4. Both TypeSafe and OpenAI are required downstream families, rather than competing alternatives. The [Decision section](decisions-api.md#decisions) proposes the detailed contract; selecting that direction does not approve its pending layouts, mappings or implementation.

## Requested scope

The user selected OpenAI-first downstream contracts, broad coverage of current audio and image APIs, and no compatibility work for historical protocols. The subsequent five-stage plan explicitly adds video API/implementation adaptation to stage 4. OpenAI-first does not mean OpenAI-only upstream execution; video operations and concrete endpoints remain to be selected from current protocol evidence.

| Requested surface | Expansion goal |
|---|---|
| OpenAI Decisions and TypeSafe System One, including SDK companion operations | Stable standard-SDK access through one Decision IR; adapt initially researched OpenAI, TypeSafe, Perplexity and Cloudflare upstreams. Concrete namespaces and compatibility profiles remain under review |
| `POST /v1/messages/count_tokens` | Stage 3, deferred: count a structured model input under the selected target/projection contract, without generation |
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

The companion surface `POST /v1/responses/input_tokens` is recommended for the OpenAI-first counting path, but remains unselected. Image generations and edits are both included by the [stage-4 scope](../roadmap/index.md#stage-4-scope); their concrete contracts and implementation slices still need selection.

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

Accepted production decisions live in Architecture: [tool examples/document context](../architecture/index.md#tool-examples-and-document-context), [citation point anchors](../architecture/index.md#citation-point-anchors), [automatic TTL admission](../architecture/adaptation-and-dialects.md#caller-selected-automatic-cache-admission) and [explanatory reports](../architecture/adaptation-and-dialects.md#explanatory-terminal-reports). They are selected local contracts, not unresolved alternatives below or claims of implementation. The remaining proposals do not inherit that approval.

#### Retain legitimate conversation semantics

The existing [Generation request](../../src/semantic/task/generation/request/mod.rs) owns conversation content and settings. Independent [Embedding](../../src/semantic/task/embedding.rs), [SpeechSynthesis](../../src/semantic/task/speech_synthesis.rs), [SpeechRecognition](../../src/semantic/task/speech_recognition.rs) and [ImageGeneration](../../src/semantic/task/image_generation.rs) contracts already exist. They are not standalone operations incorrectly hidden inside Generation.

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

The Bailian review selected [existing-IR-only adaptation](../architecture/adaptation-and-dialects.md#shared-aliyun-billing-dialect), not a budget-scope extension. Qwen Chat's reply-only `max_tokens:128` plus `thinking_budget:64` is a minimal counterexample: replacing them with total `192` permits an oversized reply when fewer thoughts are used; total `128` can truncate an otherwise valid 128-reply/64-thought result. Neither summation nor clipping is an exact mapping.

A future independent-ceiling representation would need answer/thought/total ownership, precedence, contradiction rules and directional mappings. No such representation or summation rule is selected. The [budget direction](../architecture/adaptation-and-dialects.md#bounded-token-budget-adaptation) permits bounded adaptation review without requiring that redesign.

The source/code recheck identifies two smaller candidates:

| Candidate | Applicability and trade-off | Selection still needed |
|---|---|---|
| Raise a gateway-provided output default | Client omitted its ceiling; a verified target supports numeric thinking. Preserves explicit thinking intent but may increase output cost. Go/Haiku already defaults to `32000` and disables manual thinking, so raising that default alone is not a demonstrated fix | Concrete target, finite maximum, meaningful answer reserve and bounded growth. `budget + 1` satisfies Messages syntax only, not useful answer capacity |
| Adjust a gateway-derived thinking preset | Exact native effort is unavailable and an already selected effort-to-budget fallback exceeds an explicit output ceiling. Preserves that ceiling but approximates effort more aggressively | Target-legal/useful budget, reserve and infeasible behavior; distinguish configured preset from applied allocation in evidence. Never silently reduce an explicit numeric budget |

Prefer an existing exact control or a reachable target-local rule over a universal multi-budget schema. Apply any selected adjustment before construction/validation of the projected settings; late wire rewriting cannot repair an already rejected IR. Preflight, encoding and discovery must use the same selected rule, with checked arithmetic and immutable source intent.

Current Messages intake requires a manual budget of at least `1024` and strictly less than `max_tokens`; that is source grammar, not a removable default. Startup currently exposes configured effort-to-budget conversion only for generateContent, whose general expansion remains paused. A synthetic generic conversion failure therefore establishes a policy edge, not a live Haiku defect or permission to activate Google targets. Current validation remains in force pending a concrete rule.

#### Google output report owners (unselected)

Probability and token-identity proposals remain paused under the [Google API pause](../roadmap/backlog-and-frozen.md#google-api-pause). Bounded terminal explanations and service-source review belong to the accepted [report contract](../architecture/adaptation-and-dialects.md#explanatory-terminal-reports); they did not select the owners below.

The [source counterexamples](../refs/2026-10-17-generation-contracts.md#google-foreground-contracts-2026-10-20) still require design review before broader probability carriage:

| Fact | Minimal proposed owner | Required decision |
|---|---|---|
| Reported average and sum log probabilities | One bounded output/candidate probability-report owner with independent optional numbers | Define chunk/checkpoint/final scope from evidence, replacement/conflict rules, edit dependencies and delivery. An aggregate may arrive without token reports; never recompute it as the reported value |
| Internal vocabulary token ID | Typed source-bound fidelity tied to the unchanged text/probability owner and chosen/alternative position | Select retention bounds and invalidation/carriers; it is neither portable token bytes nor replay authority |

A terminal-only aggregate-score profile remains unselected after the [streaming-scope research](../refs/2026-10-17-generation-contracts.md#streamed-score-scope-2026-10-20). Public sources define candidate averages/sums but do not establish ordinary SSE fragment-versus-cumulative coverage or repeated/missing-field reduction. A final finish reason alone does not establish whole-output score coverage. The SDK's cross-continuation-hop sum/latest-scalar merge is not a single-request stream oracle.

Recommendation after research: retain aggregate rejection until a concrete model/endpoint/API-version profile establishes coverage and report replacement. Do not select a universal response-score owner, infer totals from usage/token arrays, treat last checkpoint as final, or introduce an event archive merely to avoid that decision. The selected terminal explanation is independent of these score-scope questions.

Proposed token-identity records use signed int32 values and the existing token/top-candidate and fidelity budgets; bind them to source origin/model, readable owner, exact probabilities and chosen/alternative positions. Owner/probability edits invalidate them. Targets without an exact carrier reject by default; the selected explanation-only omission policy does not cover probability reports or token identity. No token-byte inference, probability-request admission or Google downstream endpoint is selected.

Prefer those local additions over a new generic report store. Existing `TopLogprob` already preserves absent alternative token/probability values and needs no new owner. Existing Provider-tool observations are the first candidate for future URL retrieval facts, but their Google mapping and delivery are not selected. Keep nonempty URL-context reports rejected without activating hosted tools or full citation support.

#### Review concrete ownership coupling

| Evidence entry point | Existing coupling | Proposed treatment |
|---|---|---|
| [Shared context](../../src/semantic/context.rs) | `ExecutionHints` mixes cache/identity metadata, `max_tool_calls`, storage/background controls and unit-valued unsupported-field placeholders | Separate Generation execution constraints, service intent and codec admission. Do not extend this envelope into file/job management |
| [Generation resources](../../src/semantic/task/generation/resources/mod.rs) | Reusable media locations depend on Generation errors, visibility and budgets | Extract only genuinely shared inert source/reference values; retain conversation resource tables and use-site interpretation |
| [Replay](../../src/semantic/task/generation/history/replay.rs) | Semantic validation names protocol-specific replay formats and carries origin compatibility data | Review carrier/provenance ownership when sharing compaction machinery; retain semantic purpose, owner linkage and all isolation checks |
| [Attempt preparation](../../src/execution/attempt.rs) | Independent tasks reuse prepared-request/error types located beside Generation-specific intake | Move necessary neutral execution values to an appropriate shared owner, without a framework rewrite |

These are boundary issues, not evidence that Generation already implements a scheduler. Actual Responses storage is outside the task IR; the shared context currently rejects active unsupported stateful execution. [Gateway history](../architecture/history-and-replay.md#stateful-history) is process-local conversation storage, not durable execution storage.

Do not remove opaque replay provenance to simplify types. Any reorganization must preserve carrier compatibility, completeness and deployment isolation. A protocol-specific representation is not a new semantic task.

The conversation [GeneratedAudio](../../src/semantic/task/generation/content/audio.rs) shape requires transcript/reference information that a standalone speech result need not have. Keep conversational replay requirements local; use suitable [shared audio artifacts](../../src/semantic/value/audio.rs) rather than fabricating those facts.

Existing cache options do not establish a complete cache-only operation. Preserve current handling and the separate [deferred prewarming decision](../roadmap/backlog-and-frozen.md#deferred-cache-prewarming); counting and compaction must not reopen it implicitly.

#### Integration constraints to revisit

The [transcription adapter](../../src/adapter/transcription.rs) and [preparation](../../src/execution/transcription.rs) show that a standard downstream transcription endpoint does not establish an OpenAI multipart upstream implementation. Include native upstream preparation, response decoding and stream intake in the audio plan.

Standalone topology has narrow operation and execution restrictions; see [image routes](../../src/topology/images.rs). Review model identity across operations, task-specific streaming admission and operation bindings during scheduled integration. Do not fabricate separate model identities to bypass a one-operation restriction.

Upstream response parsing must remain strict on known fields and ignore unknown fields. Review the shared closed-object helper in [OpenAI image/embedding codecs](../../src/protocol/openai/images.rs) when reopening those contracts; current rejection of unknown response fields is not the intended forward-compatibility policy.

### Three-protocol tool conversion design

**Ordinary nine-direction tool conversion and selected tool-set projections are delivered.** [Architecture](../architecture/index.md#ordered-tool-availability) owns their contract; [Development](../development.md#overall-acceptance-status) summarizes overall acceptance. This section retains unselected content projections only. [Roadmap](../roadmap/unscheduled-issues.md#conditional-stage-3-follow-ups) owns their conditional sequencing, not a second core implementation queue.

Assess each future projection through **client request → selected upstream → original client response → tool result → subsequent history**. The delivered ordinary function loops do not qualify custom tools, namespaces, result media or every dialect. The dated [assessment](../refs/2026-10-27-client-ir-boundaries.md#nine-direction-tool-assessment-2026-11-04) and [qualification](../refs/2026-10-27-client-ir-boundaries.md#tool-contract-qualification-2026-11-04) retain source findings and qualification distinctions.

#### Candidate approximation policies

| Candidate | Recommended treatment | Preserved intent and loss |
|---|---|---|
| Namespace aliases | Separate reversible target name per leaf, not a universal dispatch function | Preserve schema, call identity and selection; change model-facing grouping/names and namespace description placement. Names must be legal, bounded, stable and collision-checked; inverse mapping is mandatory |
| Custom text → `{"input": string}` function | Separate grammar-free wrapper package | Preserve completed text and original client call kind; change generation format, token cost and incrementality. Never weaken explicit Lark/regex grammar into prose |
| Examples → definition description | Separate bounded content projection | Preserve ordered labelled examples; change salience, escaping and token/cache behavior. Keep schema, choice and strictness separate |
| Historical change annotations | Leave unselected | Adds prompt content/authority questions without enforcing availability. Prefer native history or the accepted active-set projection; never invent absent schemas |
| Tool-result media → adjacent user content | Leave unselected | Moving intact media changes role and result attribution; stringifying a descriptor does not preserve media |
| Result schema or custom grammar → prose | Do not select as a general fallback | Output schemas and explicit grammar can promise more than guidance; neither becomes ordinary response-format configuration |

The accepted active-set, retained-history/current-set and inventory-filtering rules are not candidates here. Existing tool-error wrapping, nullable input-result handling and conditional strict defaults remain at their accepted owners. None of these proposals selects weaker strictness, cardinality, seriality or dispatch guarantees.

#### Ownership and return path

Reuse typed tool definitions/references, strict JSON, disposition tables, candidate projection and the shared Client delivery owner. No new schema compiler, tool executor, persistent alias service or pair-specific framework is proposed.

- Derive aliases outside semantic tool identity and bind them to the selected candidate/delivery context. Reorder, append, removal and replay must not rename old calls. Collision handling cannot silently truncate names or expose gateway aliases to the client.
- Restore original name, namespace and call kind before first publication, consistently across JSON, SSE and retained history. Historical calls keep their original raw arguments rather than today's schema.
- For wrappers, partial escaped JSON is not yet a custom-input string. Prefer bounded call-level buffering with declared incrementality loss; truncation, malformed completed input and cancellation remain non-executable.
- OpenAI raw argument strings remain authoritative. Parse only the target copy when a Messages object is needed; never copy SDK-style `{}` repair or invent original whitespace when serializing native objects.
- Target-native ordinary carriers remain preferable. Pinned official SDK consumers supply directional evidence; Vercel is a reference for typed consumer/result handling, not a replacement for ordered definitions, raw arguments or native replay. A universal dispatch wrapper would change every schema, choice and inverse path unnecessarily.

Before activating a candidate, select its named content-loss row and full return/replay acceptance under [S3-T5–S3-T7](../roadmap/unscheduled-issues.md#conditional-stage-3-follow-ups). Namespace aliases, plain custom-text wrappers and examples are independent packages, not prerequisites for delivered function conversion. Explicit grammar, media relocation and instruction-history synthesis remain excluded.

Unresolved source facts retain their own boundaries: Responses root/inline collision precedence, namespace-qualified choice spelling, non-developer returned-addition replay, Chat custom SSE and Messages repeated-definition grammar accounting. Preserve a native shape and delegate upstream validation when possible; reject a transformation needing an unknown winner or an unproved explicit guarantee, not native declarations merely lacking an observed result. Never promote tool/assistant declarations to developer authority. [Client-history empty annotations](../architecture/adaptation-and-dialects.md#semantic-equivalence-compatibility) is unrelated to proposed textual tool-history annotations.

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

## Open decisions

These recommendations have not been adopted merely by requesting this document.

| Decision | Options and recommendation | Blocks |
|---|---|---|
| Dual-SDK Decision contract | Both standard SDK families and shared IR/codec/dialect adaptation are selected. The [Decision review](decisions-api.md#decision-execution-and-review-choices) owns remaining roots, evidence/result types, projections, delivery and execution choices | Decision implementation and integration, not independent source research |
| Bounded token-budget rule | [Default expansion and derived-preset adjustment](#independent-output-ceilings-unselected) are candidates under the accepted adaptation direction; target, reserve, finite bound and loss provenance remain unselected | Budget implementation only; not Messages stream scheduling or unrelated Stage-4 design |
| Images contract | The [Image review](multimodal.md#image-design-review) owns pending structural choices. The selected Embedding contract has moved to [Architecture](../architecture/index.md#independent-embedding-path); future extensions remain unselected | Dependent S4-3 implementation and integration |
| Task grouping and shared content | Recommend independent TokenCount/Compaction, AudioToText variants, and Image Generate/Edit variants; share content without inheriting Generation execution | New task type and public Rust contract changes |
| Counting companion surface | Recommend Responses input-token counting; image generations/edits already belong to stage 4 | Adding the Responses counting surface to delivery scope |
| Counting profile and public scope | **Deferred scheduling; profile unselected.** When reopened, select the public-model/target profile, upstream reliability criteria, coarse estimation rule/input coverage and client-visible scope/provenance. [Upstream preference, explicit estimates for inaccurate reports, no gateway tokenizer counting and no inference side effects](../architecture/index.md#token-counting-policy) are selected, not open alternatives | Cross-protocol counting admission and client promises, only after reopening |
| File authority | Recommend upstream-native content plus gateway public IDs/durable mappings first. Gateway-owned content with target replicas improves portability but adds storage and replication ownership | Files/Uploads and resource-based failover |
| Retention and deletion | Select expiry, deletion with active jobs/history, cascade behavior and restart guarantees. Durable jobs and resource mappings cannot inherit process-local Responses TTL accidentally | Resource/job lifecycle implementation |
| Batch execution | Recommend native Provider batches. A gateway scheduler is a separate product with different billing, timing and cancellation semantics | Batch implementation |
| Batch composition | Select compatible model/deployment restrictions and supported endpoint set; recommend explicit rejection over implicit cross-target splitting | Batch admission |
| Custom voices | Recommend authorized voice references first; voice creation/consent management as a separate resource slice. Broader audio resource coverage may be selected explicitly | Voice-resource endpoints, not ordinary speech |
| Video contracts | Select current operations/endpoints, task versus content ownership, artifact access and any required job lifecycle before implementation | Video implementation, not stages 1–3 |
| Realtime design | Client scenario, session/transport and interruption/recovery contracts are unselected; keep stage 5 unscheduled | Realtime implementation |
| Later-operation selection | Generation Text closure and the selected Embedding design are complete; all stage-4 API families have equal priority, including Decision closure. Static Generate is an Images-internal recommendation; each operation needs just-in-time contract selection. All stage-5 tasks remain unscheduled | Per-operation design, activation and full-chain acceptance |

## Execution handoff

The [roadmap work packages](../roadmap/index.md#execution-sequence-and-work-package-rules) own sequence, dependencies and acceptance gates. This page does not maintain a second implementation queue. Before starting a package, select its request/result/event and lifecycle boundaries, record accepted choices in Architecture, and retain only unresolved alternatives here.

Extract shared values only when the selected operation needs them. Direct-input media do not depend on the complete file service; authorized file-reference paths do. Stage-4 native media jobs keep their own lifecycle without implying the general Batch API. Do not perform a repository-wide ownership rewrite or prebuild stage-5 infrastructure as a prerequisite for the first operation.

## Verification

Pin current official schemas and streaming grammars before implementation; documented carriers do not prove model-effective support. Use the existing [test design](../development.md#test-layout), [accepted design](../architecture/decisions.md#scope-and-activation-decisions) and [verification conventions](../development.md#verification-gates), including separately scoped [real Provider checks](../development.md#real-provider-checks).

Stage-4 implementation preserves the [text/SSE integration baseline](../development.md#overall-acceptance-status) and includes [operation-level upstream-test acceptance](../roadmap/index.md#stage-4-reuse-and-acceptance). Reuse the shared source/adaptation procedures rather than a separate modality harness; this does not select unresolved operation contracts.

Keep independent wire-to-IR and IR-to-wire expectations, covering success and intended rejection at the responsible owner. Extend existing tests rather than building per-model/Provider matrices. Resource/job checks must address authorization, recovery, cancellation and uncertain submission.

Compare the [OpenAPI summary](../openapi.json) with implemented ingress and codecs; never publish planned endpoints as implemented paths. Preserve safety assertions and frozen routing. Report uncovered layers: codec support is not public admission, compilation is not runtime acceptance, static results are not streaming support, and remote success is not crash recovery.

## Protocol references

These sources informed the proposal. Web references are mutable; recheck them and select pinned SDK revisions in [References](../references.md) when implementation starts. Source inspection is not a real Provider acceptance result.

| Contract | Official source |
|---|---|
| Decisions | [Dated four-Provider and SDK findings](../refs/2026-10-23-decision-contracts.md), with official schema/source links and unresolved discrepancies |
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
