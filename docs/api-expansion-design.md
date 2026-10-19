# API expansion design and implementation plan

## Status and authority

This is a standalone pre-implementation reference requested for the API expansion. The requested scope is confirmed; the architecture recommendations below are proposals unless already established in [Architecture](architecture.md). No new interface, binding, resource service or execution policy is activated by this document.

[Roadmap](roadmap.md#planned-order) owns the five-stage sequence and the current routing freeze. Embedding and other-modality API/implementation adaptation, including audio/image/video, are selected for stage 4, after cross-protocol stability and before stage-5 citations/discovery/hosted tools. This is not an additional exit condition for stages 1–3. Detailed operation designs and within-stage ordering remain proposals; record selected decisions and explicit slices before implementing new contracts or activating bindings. Counting, compaction, general Files/Uploads and Batch retain separate scheduling except for selected dependencies.

Code and tests remain the behavior specification. This document records ownership, constraints and proposed work, not a model catalog, compatibility certification or implementation progress log. Provider and binding availability belongs to [the catalog](../src/topology/catalog.rs) and [startup](../src/gateway/startup/bootstrap.rs).

## Contents

- [Requested scope](#requested-scope)
- [Proposed ownership](#proposed-ownership)
- [Generation boundary review](#generation-boundary-review)
- [Operation contracts](#operation-contracts)
- [Optional Chat-carried audio](#optional-chat-carried-audio)
- [Resource and execution safety](#resource-and-execution-safety)
- [Implementation sequence](#implementation-sequence)
- [Open decisions](#open-decisions)
- [Verification](#verification)
- [Protocol references](#protocol-references)

## Requested scope

The user selected OpenAI-first downstream contracts, broad coverage of current audio and image APIs, and no compatibility work for historical protocols. The subsequent five-stage plan explicitly adds video API/implementation adaptation to stage 4. OpenAI-first does not mean OpenAI-only upstream execution; video operations and concrete endpoints remain to be selected from current protocol evidence.

| Requested surface | Expansion goal |
|---|---|
| `POST /v1/messages/count_tokens` | Count a structured model input without generating a message |
| `POST /v1/responses/compact` | Execute native context compaction and return reusable context |
| `POST /v1/embeddings` | Complete the existing independent embedding contract |
| `POST /v1/audio/speech` | Complete standalone speech synthesis, including supported streaming |
| `POST /v1/audio/transcriptions` | Complete file-based transcription and its reported evidence |
| `POST /v1/audio/translations` | Add the current native audio-translation contract |
| `POST /v1/images/edits` | Add editing and complete the shared modern image-production contract |
| Video APIs, concrete operations/endpoints pending | Adapt selected video input/output or generation operations, including their execution lifecycle and artifact delivery |
| Batch APIs | Submit and manage asynchronous batches of supported typed tasks |
| File APIs | Manage reusable files and multipart upload resources |

Two companion surfaces are recommended, not silently added to the confirmed list: `POST /v1/responses/input_tokens` for the OpenAI-first counting path, and completion of the existing `POST /v1/images/generations` alongside edits.

Optional scope: map task-specific upstream Chat Completions media payloads, such as MiMo TTS/ASR, to standard `/audio/*` endpoints. This must not block the OpenAI main path.

Completeness means covering the selected current operation contracts, not only the intersection supported by every model. Keep semantic expression, codec representation, model-effective admission and working integration as separate claims. Unsupported combinations reject explicitly; a field's presence in a schema does not establish effective model support.

Do not add Legacy Completions, old DALL-E-only behavior or image variations for historical parity. Existing current Chat Completions and Messages contracts are not deprecated merely by this expansion. A still-current endpoint's documented older-model restriction is not permission to invent a newer equivalent.

The earlier exclusion of video is superseded by the stage-4 modality scope; it does not select a particular video-generation or editing contract yet. Google-native downstream endpoints, Realtime/Live, Vector Stores, Assistants, fine-tuning and general hosted-tool execution remain outside this expansion. Hosted tools have their own stage-5 work. Custom voice creation and consent management remain an [open scope decision](#open-decisions). Standalone image support does not automatically implement the Responses image-generation tool.

The initial vendor comparison is a checklist, not a protocol oracle. Endpoint names do not determine task ownership: Gemini can synthesize speech through a generation endpoint, Responses can be stateless, and Anthropic permits consecutive same-role turns. Use the [official references](#protocol-references), not the comparison's historical restrictions.

## Proposed ownership

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

## Generation boundary review

### Retain legitimate conversation semantics

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

### Review concrete ownership coupling

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

### Integration constraints to revisit

The [transcription adapter](../src/adapter/transcription.rs) and [preparation](../src/execution/transcription.rs) show that a standard downstream transcription endpoint does not establish an OpenAI multipart upstream implementation. Include native upstream preparation, response decoding and stream intake in the audio plan.

Standalone topology has narrow operation and execution restrictions; see [image routes](../src/topology/images.rs). Review model identity across operations, task-specific streaming admission and operation bindings during scheduled integration. Do not fabricate separate model identities to bypass a one-operation restriction.

Upstream response parsing must remain strict on known fields and ignore unknown fields. Review the shared closed-object helper in [OpenAI image/embedding codecs](../src/protocol/openai/images.rs) when reopening those contracts; current rejection of unknown response fields is not the intended forward-compatibility policy.

## Operation contracts

### Token counting

Model a count request from count-relevant conversation content: instructions, ordered history, tools, media and applicable settings. It is not generation with a zero output cap. A count result is not inference usage and must not contain fabricated generated output.

OpenAI documents `POST /v1/responses/input_tokens`; Anthropic documents `POST /v1/messages/count_tokens`. Reuse one task boundary while preserving distinct source semantics and result envelopes. Resolve any admitted gateway-owned history reference before target preparation; never forward a gateway ID as a Provider-owned reference.

Counting must identify the model/tokenizer and effective input projection. A Messages-shaped request does not automatically imply an Anthropic tokenizer if the public model denotes another target. Cross-protocol counting needs an explicitly selected, evidence-backed profile consistent with generation preparation.

One candidate's count is not an exact count for every failover candidate or a reservation of the next generation route. Upstream token counts may themselves be estimates; preserve their documented meaning rather than promise billing equality. Missing native counting support must not silently become a character-count estimate or an inference call.

Exit evidence: independent request/result oracles, count-relevant controls and media preserved, unsupported counting rejected, no generation side effects, and explicit scope for the returned count.

### Context compaction

Use a distinct request and result. The result contains returned ordered context, resources, reported accounting and operation metadata. It is not a normal assistant response.

The official compact response can retain user messages followed by a compaction item. [Generation response validation](../src/semantic/task/generation/validate.rs) excludes user/instruction messages from ordinary output. Do not weaken that validation to reuse `GenerationResponse`.

Reuse typed conversation content and [compacted-history replay](architecture.md#compacted-history). Preserve the opaque result's compatible deployment/model scope; incompatible replay rejects rather than strips required history.

Keep three operations separate: caller-supplied plaintext context editing, Provider-executed compaction, and automatic compaction scheduling. Adding `/responses/compact` does not select automatic triggers, mutate stored conversations, enable Provider-held continuation or introduce background Generation.

Exit evidence: independently specified compacted context in both directions, faithful subsequent input replay, incompatible-target rejection and honest failure handling.

### Embeddings

Extend the existing independent task rather than representing vectors as messages or tool output.

| Concern | Required distinction |
|---|---|
| Inputs | Text and token-ID inputs are different typed alternatives; token IDs need tokenizer/model scope |
| Output | Preserve input/result indexes, dimensions and exact reported vector values |
| Encoding | Float/base64 wire representation belongs to the codec/delivery boundary |
| Usage | Preserve reported accounting; absent is not zero |
| Routing | Equal vector dimensions do not establish embedding-space compatibility |

Retrieval task intent or multimodal embeddings may enter the task only when selected official requirements justify them. Do not silently detokenize token IDs or apply Generation's cross-model failover without an explicit embedding-space contract. Chunking, vector indexes and retrieval remain caller responsibilities.

### Speech synthesis

Preserve supplied text, synthesis instructions, voice selection, speed and encoding intent. Distinguish a built-in voice from an authorized custom voice reference instead of relying on an unconstrained string.

Support the selected current binary and SSE delivery contracts. Progressive binary delivery must not require whole-output buffering; SSE needs task-specific audio events, reported usage and completion. Audio encoding and event transport are separate choices.

Use reported token, character or duration accounting only where the source provides it. Input text is not a fabricated audio transcript. Custom voice creation and consent are resource operations, not fields that cause synthesis to create resources.

Exit evidence: correct binary/SSE delivery, supported controls preserved, unsupported voice/format/control combinations rejected, bounded buffering and cancellation, and no success after truncated input.

### Audio to text

Propose separate `Transcribe` and `Translate` request variants within one audio-to-text domain. The same recording may require an original-language transcript or an English translation; that difference is protected task intent, not a response encoding.

Complete the selected current contract for audio containers, source-language hints, prompting and applicable controls. Results may include full text, segments, words, timestamps, speaker attribution, log probabilities and token- or duration-based usage. Preserve reported coverage: the existing last-sentence report is not a complete alignment.

Keep plain text, JSON and subtitle encoders outside the core result. Formats that require timestamps or speaker information need real corresponding evidence; they cannot be manufactured from unannotated text. Diarization and chunking controls remain distinct from ordinary formatting.

An uploaded audio file with streamed transcription results is not a live microphone-input protocol. File transcription does not require creating a reusable File API object. Do not extend the existing multipart collector into an unbounded file-service buffer.

The currently documented OpenAI translation endpoint is English-targeted and Whisper-only. Recheck this restriction when pinning the implementation source; do not invent an arbitrary target-language parameter or replace it with an implicit transcription-plus-Generation chain.

Exit evidence: native multipart preparation, complete static and supported streamed results, faithful timing/speaker/usage reports, operation-specific restrictions, bounds and cancellation.

### Image production and editing

Propose explicit `Generate` and `Edit` variants, sharing output controls, artifacts and accounting. An edit requires ordered source images, a mask association where present, edit intent and applicable input-preservation controls. A prompt alone cannot express editing a particular photograph.

Complete current output-format, size, quality, background, compression and applicable safety controls without preserving obsolete model-only carriers for compatibility. Effective support stays binding-specific, not a universal promise derived from the schema.

Support current multipart and JSON-reference input forms where the selected operation admits them. Distinguish uploaded binary content, inert URLs/data URLs and authorized file references. Never turn an arbitrary Provider file ID into an authorized input.

Static and streamed delivery share final-artifact semantics. Partial images are previews, not final outputs or progress inferred from their count. Preserve preview ordering and final-image identity; do not satisfy a requested final count with previews.

Exit evidence: source/mask ownership, independent generate/edit wire oracles, correct artifact/usage projection, partial-versus-final streams and rejection of incompatible options. Direct-input support can precede File API integration.

### Video API and implementation

Video is selected as a stage-4 modality, not an already chosen endpoint family. Pin current official contracts before selecting concrete input/output, generation or editing operations. Separate conversational video content from standalone production operations; the wire endpoint name does not determine the task owner.

For each selected operation, specify supported input references, required media controls, output artifacts and reported accounting. If the native API is asynchronous, select submission identity, status, cancellation, artifact access and uncertain-submission handling explicitly. Do not reuse a completed text response as evidence of video completion or turn a returned artifact URL into unrestricted server-side fetching.

Exit evidence: independently specified request/result and applicable event/job contracts, bounded media I/O, correct artifact identity, cancellation and honest completion. Specific APIs, new task types, storage and job persistence remain design decisions, not permissions implied by this scope entry.

### Files and uploads

Files/Uploads need typed resource operations, not an inference request or a larger Generation resource table.

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

Choose authoritative storage before implementation. The first-stage recommendation is upstream-native content storage with gateway-owned public IDs and durable private mappings. The alternative is gateway-owned content with per-target upstream replicas. Neither is selected yet.

Define finite storage/upload bounds, part order and byte-total validation, incomplete-upload cleanup and supported purposes. Do not adopt deprecated File status fields as a new processing state machine. Accepting a file purpose does not implement its associated fine-tuning or agent service.

Resolve resource authorization outside pure codecs. Task content may carry typed resource identity, but credentials, storage handles and trusted target mappings stay outside IR. Resource scope can restrict already authorized targets; it cannot authorize an arbitrary origin or credential.

Exit evidence: create/read/list/delete and upload lifecycle contracts, restart-consistent mappings, access isolation, expiry, deletion with active references, and bounded streaming rather than whole-file memory retention.

### Native batch jobs

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

## Optional Chat-carried audio

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

## Resource and execution safety

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

## Implementation sequence

The roadmap's stage-4 placement is selected; the task-local sequence below is a proposal, not another numbering of the five roadmap stages. Embedding and audio/image/video work need not wait for unrelated counting, compaction or Batch work. Definition work does not reopen the [routing freeze](roadmap.md#temporary-routing-freeze) or activate bindings. Schedule each implementation slice after its boundary decisions.

| Slice | Work | Exit condition |
|---|---|---|
| Per-operation contract selection | Pin official API/SDK sources; select task and required resource/job ownership; extract only necessary shared values | Each operation has request/result/event/lifecycle boundaries and explicit exclusions; decisions are recorded |
| Stage 4: Embeddings | Extend the existing task and codecs | Input/result correspondence, dimensions, encoding, tokenizer scope and embedding-space admission hold |
| Stage 4: Audio | Complete speech delivery, then transcription, then translation; add native OpenAI preparation/intake | Supported static/streamed results preserve required evidence and reject unsupported combinations |
| Stage 4: Optional audio dialect | Migrate Chat-carried static audio; add native streaming/reporting separately | Standard downstream audio works without Generation execution; optional work does not block the main audio slice |
| Stage 4: Images | Complete selected Generate/Edit contracts, initially with direct inputs | Source/mask identity, output controls, static results and preview/final streams are correct |
| Stage 4: Video | Select concrete video operations, then implement task/content codecs and native execution/delivery | Required controls, artifacts and synchronous/asynchronous completion have independent evidence |
| Separately scheduled context operations | Implement counting and compaction as separate slices; include the recommended Responses counting surface if selected | No generation masquerading as counting; returned compacted context replays correctly without weakening Generation validation |
| Selected dependency or separate Files/Uploads slice | Add the selected resource service and connect admitted file references | Lifecycle, authorization, durable mappings and active-reference policies hold across restart |
| Separately scheduled Native Batch | Add job coordination over completed task contracts and file resources; open endpoint support incrementally | Submission uncertainty, recovery, cancellation and per-record results are handled without implicit retries |
| Per-slice integrated acceptance | Verify selected public bindings, OpenAPI summary, pinned SDKs and bounded real calls | IR, codec, admission, SDK and actual Provider claims have separate evidence |

Files must be designed early but need not block direct audio or image work:

```text
shared values -> counting / compaction / embeddings / audio
              -> direct-input images
              -> files/uploads -> image file references
                               -> batch resource lifecycle
completed task contracts ------> batch record execution
```

Before each implementation slice, state the goal and exit condition in the roadmap and present any new IR counterexample/options. Do not perform a repository-wide ownership rewrite as a prerequisite for the first working operation.

## Open decisions

These recommendations have not been adopted merely by requesting this document.

| Decision | Options and recommendation | Blocks |
|---|---|---|
| Task grouping and shared content | Recommend independent TokenCount/Compaction, AudioToText variants, and Image Generate/Edit variants; share content without inheriting Generation execution | New task type and public Rust contract changes |
| Companion surfaces | Recommend Responses input-token counting and completing image generations alongside edits | Declaring those additional surfaces part of the delivery scope |
| Counting meaning across protocols | Recommend an explicit public-model/target counting profile consistent with generation projection; reject unsupported counting rather than silently estimate. A separately exposed estimate would need its own policy | Cross-protocol counting admission and client promises |
| File authority | Recommend upstream-native content plus gateway public IDs/durable mappings first. Gateway-owned content with target replicas improves portability but adds storage and replication ownership | Files/Uploads and resource-based failover |
| Retention and deletion | Select expiry, deletion with active jobs/history, cascade behavior and restart guarantees. Durable jobs and resource mappings cannot inherit process-local Responses TTL accidentally | Resource/job lifecycle implementation |
| Batch execution | Recommend native Provider batches. A gateway scheduler is a separate product with different billing, timing and cancellation semantics | Batch implementation |
| Batch composition | Select compatible model/deployment restrictions and supported endpoint set; recommend explicit rejection over implicit cross-target splitting | Batch admission |
| Custom voices | Recommend authorized voice references first; voice creation/consent management as a separate resource slice. Broader audio resource coverage may be selected explicitly | Voice-resource endpoints, not ordinary speech |
| Video contracts | Select current operations/endpoints, task versus content ownership, artifact access and any required job lifecycle before implementation | Video implementation, not stages 1–3 |
| Within-stage scheduling | Stage 4 is selected for Embedding/audio/image/video; choose bounded implementation slices and necessary dependencies. Other expansion surfaces retain separate scheduling | Per-slice activation and full-chain acceptance |

## Verification

1. Pin and inspect current official schemas and streaming grammars before each codec implementation. Separate documented fields from model-effective support; do not maintain model lists here.
2. Use independent hand-written wire-to-IR and IR-to-wire expectations. Extend existing same-layer tests; preserve the lean suite and safety assertions. Follow the roadmap's phase-specific scope and coverage authorization.
3. Keep one positive and one negative case per contract at the lowest owner. Do not build per-model/Provider variant matrices. Resource/job safety assertions should address concrete authorization, recovery, cancellation and uncertain-submission scenarios.
4. Run affected checks, then the [full offline baseline](development.md#offline-baseline). Do not weaken checks or repair unrelated frozen routing to make a new slice appear integrated.
5. When connected, run the [SDK loopback gate](development.md#sdk-loopback-gate) and compare the [OpenAPI summary](openapi.json) with implemented ingress and codecs. Do not publish planned endpoints as implemented API paths.
6. Run separately scoped real Provider checks only when needed. Fix target, count, output cap and timeout; use the project credential loader and redacted reporting.
7. Record exact uncovered layers. A codec test is not public admission; compilation is not runtime acceptance; a static result is not streaming support; a remote success is not crash recovery.

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
