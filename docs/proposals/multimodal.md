# Multimodal Media Proposals

This document covers only **unselected** future media extensions beyond the delivered [static Images](../architecture/adaptation.md#native-static-images), Speech, and Transcription operations.

See [Proposals Framework](index.md) for shared boundaries and [Roadmap](../roadmap/index.md#stage-4-pending-scope) for Stage 4 scheduling and exit gates.

## Contents

- [Unselected Audio extensions](#unselected-audio-extensions)
  - [Audio translation](#audio-translation)
  - [Streaming and diarized audio](#streaming-and-diarized-audio)
  - [Optional Chat-carried native streaming](#optional-chat-carried-native-streaming)
- [Unselected Image extensions](#unselected-image-extensions)
  - [Masked editing, spooling, and native ratio controls](#masked-editing-spooling-and-native-ratio-controls)
  - [Image streaming lifecycle](#image-streaming-lifecycle)
  - [Image design review](#image-design-review)
- [Video API and implementation](#video-api-and-implementation)

## Unselected Audio extensions

Basic synchronous and chunked-SSE Speech (`POST /v1/audio/speech`) and Transcription (`POST /v1/audio/transcriptions` with `delivery.ffmpeg` normalization) are implemented in [API Reference](../usage/api-reference.md). The following extensions remain unselected proposals:

### Audio translation

If reopened beyond its deferred status, model `Translate` as a distinct operation variant rather than reusing `Transcribe` output encoding or chaining transcription + generation. Do not invent arbitrary target-language parameters over English-only translation contracts without pinned source support.

### Streaming and diarized audio

- **Progressive non-SSE Speech:** Avoid buffering entire binary streams when streaming upstream delivery is selected; keep audio encoding separate from transport framing.
- **Diarized / subtitle / timestamped ASR streams:** Require real upstream timing/speaker evidence rather than synthesizing timestamps from plain text; distinguish diarization/chunking controls from output formats.
- **Custom voice creation and consent:** Model voice cloning/design (`VoiceDesign`/`VoiceClone`) as separate resource operations with explicit authorization rather than overloading `SpeechSynthesis` fields.

### Optional Chat-carried native streaming

Chat-carried audio (such as MiMo audio codecs) currently adapts statically. Adding native upstream audio/text delta streaming requires task-specific delta mapping with cancellation and EOF verification rather than presenting post-buffer chunking as progressive upstream streaming.

## Unselected Image extensions

Static OpenAI Generate/Edit (`png`/`jpeg`/`webp` + `output_compression` via `delivery.ffmpeg`, `n = 1` on Gemini/Grok and `1..=6` on Qwen, completed-only static SSE, and GPT Image masks/`partial_images`) are implemented in [Native static images](../architecture/adaptation.md#native-static-images). The following extensions remain unselected:

### Masked editing, spooling, and native ratio controls

- **Bounded spooling / file references:** Extending beyond the 8 MiB in-memory multipart intake requires explicit content identity, external spool ownership, and cleanup guarantees outside IR (and [Files/Uploads](files-and-uploads.md) for `file_id` resolution).
- **Masked Edit & native controls on static targets:** Keep `mask` bound strictly to source index zero; do not map exact pixel `size` to native aspect-ratio/resolution tiers (expose separate typed controls if selected).

### Image streaming lifecycle

Any future native preview streaming beyond GPT Image requires task-specific `Preview { output_slot, preview_index, artifact, event_time, reported_settings }` and `Completed` events without inventing missing previews, mutating preview counts, forwarding URLs via server-side downloads, or treating `created_at` as static image creation time. Preview publication forbids replay.

### Image design review

| Decision | Unselected extension choices | Blocks |
|---|---|---|
| Additional native static controls & reports | Separate typed aspect-ratio/resolution-tier fields and per-artifact reports on non-GPT targets | Does not block static Generate/Edit |
| Spooling / file-ID Edit inputs | External content spooling ownership and target-scoped `file_id` resolution | Large-input / reference Edit |
| Non-GPT preview streaming | Task-local preview bounds (`0..=3`), slot association, and EOF-gated final usage | Non-GPT preview SSE |

## Video API and implementation

Async video generation remains pending [operation selection](../roadmap/index.md#stage-4-pending-scope), using native async job lifecycles (such as Grok `POST /v1/videos/generations` → `request_id` + `GET /v1/videos/{request_id}` polling with USD ticks and no cancellation endpoint) rather than synchronous waits or relabeling non-OpenAI jobs as OpenAI compatibility.
- Separate conversational video content from standalone video jobs; specify job submission identity, bounded polling, cancellation, artifact URL authorization (never unrestricted server-side fetching), and honest completion before implementation.
