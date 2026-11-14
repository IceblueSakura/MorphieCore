# Multimodal Media Proposals

This document details proposed operation contracts and execution models for media-bearing modalities in Stage 4: Audio (Speech, Transcription & Translation), Image generation/editing, and asynchronous Video generation.

See [Proposals Framework](index.md) for shared boundaries and [Roadmap](../roadmap/index.md) for Stage 4 execution gates.

## Contents

- [Audio](#audio)
  - [Speech synthesis](#speech-synthesis)
  - [Audio to text](#audio-to-text)
  - [Optional Chat-carried audio](#optional-chat-carried-audio)
- [Image production and editing](#image-production-and-editing)
  - [Image requests and target admission](#image-requests-and-target-admission)
  - [Image results and delivery compatibility](#image-results-and-delivery-compatibility)
  - [Static Generate review proposal](#static-generate-review-proposal)
  - [Image streaming lifecycle](#image-streaming-lifecycle)
  - [Image design review](#image-design-review)
- [Video API and implementation](#video-api-and-implementation)

## Audio

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

### Optional Chat-carried audio

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

The existing [MiMo audio codec](../../src/protocol/xiaomi_audio.rs), [speech adapter](../../src/adapter/speech.rs) and [transcription adapter](../../src/adapter/transcription.rs) already demonstrate this separation. Preserve and evolve that path; catalog registration does not establish activation or live acceptance.

Retain exact task intent and explicit target capabilities. Map supported media/control carriers, reject unsupported voice, speed, format, timing or speaker requirements, and preserve available accounting under the selected reporting policy. Do not reinterpret ASR as translation or substitute a nearby voice silently.

Current static adaptation is not proof of native streaming support. When scheduled, implement a task-specific mapping from upstream audio/text deltas to the selected downstream event contract, including cancellation and honest terminals. Do not present post-buffer chunking as verified progressive upstream delivery.

Treat this as a separate optional audio slice: migrate existing static mappings after the task contracts stabilize, then add native streaming and richer reports where justified. It must not narrow the OpenAI contract or block its completion.

## Image production and editing

The current task and execution path implement bounded static Generate, not Edit or streaming. The selected Grok codec does not establish a downstream contract or gateway binding. [Dated OpenAI evidence](../refs/2026-10-22-embedding-image-contracts.md#images) and [Grok observations](../refs/2026-10-22-grok-imagine.md#images) distinguish the official schema, model-effective controls and unresolved source discrepancies.

### Image requests and target admission

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

Recommend immutable owned/shared bytes for the first direct-input slice, with deliberately small fixed aggregate memory limits; defer spooling.

Pure multipart encoding consumes those bounded bytes and produces a bounded body; budget the intake, encoded body and output copies, not only source payload size.

Content ownership lasts through preparation/upload and releases on completion, error or cancellation.

This fits the current byte-owned execution request without promising full upstream file-size coverage.

If larger inputs later justify spooling, first select request-local content identities, an external content owner and a pure multipart part plan; handles, I/O and cleanup never enter IR.

Merely adding a spool limit would not establish that ownership seam.

For OpenAI, encode ordered multipart `image`/`image[]` or the selected JSON `images` reference form without changing meaning. JSON entries choose exactly one source form. File-ID forms remain rejected until authorized Files resolution exists. Remote URLs may be passed only to targets with an admitted native reference carrier; do not add gateway URL downloading, redirects or credential forwarding to make multipart-only targets work.

OpenAI applies a mask to the first image.

Decode that into explicit source-zero association and reject other associations on encoding; never reorder images to conceal a mismatch.

The mask is editing guidance, not a promise of pixel-exact preservation.

Recommend retaining the accepted [inert-media boundary](../architecture/adaptation-and-dialects.md#media): validate source shape, declared type and encoded/decoded byte bounds locally; leave actual format, alpha and matching pixel dimensions to the native upstream validator for both inline and URL forms.

A target without the required validation contract rejects.

MIME declarations are not validation evidence.

A future bounded metadata inspector would require an explicit standalone-Edit policy exception, resource limits and a declared-versus-observed fact owner; this proposal does not introduce image decoding or change Generation media handling.

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

### Image results and delivery compatibility

Share ordered final artifacts and optional **reported** creation time, serving model and accounting while retaining operation identity at the task boundary. Each artifact owns data and its actual reports; a top-level static report is valid only when it describes every artifact. Retain current bounded Base64 delivery for the first static slice; URL-only artifacts do not authorize automatic downloading or caller-directed upload destinations.

Maintain three distinct facts: requested controls, stream-reported settings (which can remain Auto), and concrete artifact reports. Never put `quality:auto` into an effective `ImageQuality`, infer dimensions from a requested size, or treat event `created_at` as static image creation time. Absent, null and zero accounting are distinct. Monetary ticks remain money, cached tokens remain subsets, and native sparse usage cannot fill required OpenAI counters.

**Public compatibility is a design gate, not wiring:**

| Minimal counterexample | Consequence and recommendation |
|---|---|
| Grok returns only `data:[{b64_json:...}]` | Valid native output lacks OpenAI's required `created`. Keep strict OpenAI delivery; never insert the gateway clock or request start time |
| OpenAI request omits `output_format`; Grok returns JPEG | OpenAI's PNG default was not preserved. Reject the binding/profile before dispatch unless an explicitly different client contract or verified exact transformation is selected |
| Source reports serving model/native billing without standard carriers | Preserve the source; select explicit dialect carriage or a named allowed report omission. The existing accounting projection does not authorize model/time loss |
| Artifact reports disagree across outputs | Do not collapse them into a misleading top-level report; reject incompatible standard delivery or use a selected richer carrier |

Recommend keeping strict OpenAI Images as the standard dialect and designing an **explicit MorphieCore Images dialect** for optional native reports and ratio/tier controls if Grok activation remains desired.

Its selection mechanism, field layout, defaults and SDK implications require human review.

Alternatives are a separately selected native surface or leaving Grok disconnected.

Do not silently relax the standard schema for one model, add fields to OpenAPI before implementation, or claim that a lenient SDK parser establishes conformance.

Image transcoding is a separate capability, not a default repair.

### Static Generate review proposal

**Unselected proposal.** The [roadmap](../roadmap/index.md#stage-4-operation-readiness) owns scheduling and the Images-internal sequence. Start with Generate's existing task/artifact owners; neither an Image Generate/Edit type migration nor native ratio/tier controls are prerequisites for this review.

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

### Image streaming lifecycle

Propose a small task-specific event vocabulary, not the Generation reducer:

```text
Preview { output_slot, preview_index, artifact, event_time, reported_settings }
Completed { final_artifacts, event_time, reported_settings, reported_usage }
```

An internal output slot is association within one operation, not an invented upstream/public ID. The inspected OpenAI stream has `partial_image_index` but no final-output index. Recommend **one final output for the initial streaming profile**; then every preview belongs to slot zero. Multi-output static delivery remains independent. Do not guess multi-output associations or reuse preview indexes as final-image identity.

The requested 0–3 partial images are a ceiling, not guaranteed work or percentage progress. The inspected references do not establish omission's default. Recommend forwarding omission unchanged to the same native contract while enforcing a separate local maximum of three previews; do not silently turn omission into zero. Explicit null needs a source-backed profile rule (preserve, normalize to omission or reject), not a universal reset. Select that rule for Generate and Edit before admission.

Accept zero/fewer previews and a final image arriving early. Preserve reported preview indexes and arrival order; initially require nonnegative, unique increasing indexes within the effective local/explicit bound without inventing missing previews. Pin any stronger contiguity rule before implementation. Reject previews with an explicit zero ceiling, incompatible event types, duplicate completion, post-terminal data and malformed known settings.

Stream incrementally without retaining every preview; charge cumulative encoded/decoded bytes, per-event bytes, event count, final-artifact bytes, idle/total time and concurrency independently. The current 8-MiB aggregate final-image budget is not a new 8-MiB allowance for each preview. Select finite stream/upload limits before implementing those paths.

Success requires an operation-correct completed event, the requested final count and validated EOF.

Preview publication forbids replay; on later failure abort/fail the existing delivery rather than replay or emit a successful terminal.

Hold successful completion behind EOF, release upstreams on disconnect/deadline, and retain reported final usage once rather than summing preview observations.

SSE `created_at` cannot automatically produce a static `created`; reject unsupported static↔stream conversions until a source-backed mapping is selected.

Buffered static images split into chunks do not establish upstream progressive delivery.

### Image design review

| Decision | Recommendation and alternative | Blocks |
|---|---|---|
| Operation/source ownership | Generate/Edit family, ordered neutral sources and explicit source-zero mask. Start with immutable byte-owned inputs and a total materialization budget; a later spool-backed part plan is a separate choice | Edit IR and multipart execution, not existing Generate maintenance |
| Static Generate public contract | [Explicit native-output profile proposal](#static-generate-review-proposal), separate native surface or disconnected Grok. Select defaults, report shape and client opt-in together; native ratio/tier exposure is independently selectable | Grok public admission, defaults and binding, not its existing pure codec or later Edit/stream review |
| Mask profile and validation | Conservative PNG-declared source/mask and byte subset, retaining native pixel/alpha validation. Local metadata inspection instead needs an explicit inert-media policy exception | Masked Edit admission, not unmasked direct-input design |
| Stream association and reports | Initially one final output; task-local preview/final lifecycle and separate settings/event-time owners. Select omission/null behavior without inventing a preview default | Streaming, not static Generate/Edit |

Independent directional oracles must cover Generate and Edit, multipart and JSON, source/mask ordering, defaults and target-specific restrictions, optional/native result reports and intended negative cases.

Streaming needs independent preview/final fixtures, omitted/null/zero preview controls, fewer previews, Auto reports, errors/truncation/post-terminal data, cumulative bounds and cancellation without replay.

SDK multipart/static and native SSE consumption are separate integration gates; a genuine incremental transport check is required in addition to an SDK accumulator.

Real calls wait for a selected target and bounded acceptance plan.

Files, hosted image tools, deferred jobs, video, cross-Provider failover and automatic media sweeps remain excluded.

## Video API and implementation

Video has equal stage-4 priority but still needs [operation selection](../roadmap/index.md#grok-imagine-adaptation) before implementation.

For its design, use Grok's native asynchronous API as the primary design reference; the earlier synchronous-wait choice is superseded.

Public wire schemas, typed owners, job authorization and persistence remain unselected.

Do not label a Grok job or synchronous wrapper as OpenAI Videos compatibility.

For each operation, pin current official contracts before selecting input/output, generation or editing behavior.

Separate conversational video content from standalone production operations; the wire endpoint name does not determine the task owner.

For each selected operation, specify supported input references, required media controls, output artifacts and reported accounting. If the native API is asynchronous, select submission identity, status, cancellation, artifact access and uncertain-submission handling explicitly. Do not reuse a completed text response as evidence of video completion or turn a returned artifact URL into unrestricted server-side fetching.

Exit evidence: independently specified request/result and applicable event/job contracts, bounded media I/O, correct artifact identity, cancellation and honest completion. Specific APIs, new task types, storage and job persistence remain design decisions, not permissions implied by this scope entry.

