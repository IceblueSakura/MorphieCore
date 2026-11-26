# HTTP API Reference and Errors

This document defines standalone HTTP endpoints, model discovery, gateway error mappings, and resource limits.

See [Client Requests](client-requests.md) for generation protocols and [OpenAPI](../openapi.json) for machine-readable schemas.

## Contents

- [HTTP API](#http-api)
  - [Static image generation](#static-image-generation)
  - [Speech synthesis](#speech-synthesis)
  - [Audio transcription](#audio-transcription)
  - [Embeddings](#embeddings)
  - [Decisions](#decisions)
  - [Generation Text model discovery](#generation-text-model-discovery)
  - [Capability discovery and adaptation notice](#capability-discovery-and-adaptation-notice)
- [Errors and limits](#errors-and-limits)

## HTTP API

| Method and path | Purpose |
|---|---|
| `GET /v1/models` | List activated OpenAI-side public models |
| `GET /typesafe/v1/models` | List activated TypeSafe Decision models in the native TypeSafe envelope |
| `GET /v1/models/{model}` | Retrieve an activated public model |
| `DELETE /v1/models/{model}` | Denied: 403 for an activated model, 404 otherwise |
| `GET /v1/morphiecore/models` | List activated Generation Text model/interface contracts |
| `GET /v1/morphiecore/models/{model}` | Retrieve the same enhanced text model descriptor |
| `GET /v1/morphiecore/capabilities` | Inspect partial gateway contracts for activated Generation and Embedding clients |
| `POST /v1/responses` | Responses, JSON or SSE |
| `GET /v1/responses/{response_id}` | Experimental and disabled: authenticated requests return 400 |
| `DELETE /v1/responses/{response_id}` | Experimental and disabled: authenticated requests return 400 |
| `GET /v1/responses/{response_id}/input_items` | Experimental and disabled: authenticated requests return 400 |
| `POST /v1/chat/completions` | Chat Completions, JSON or SSE |
| `POST /v1/messages` | Messages, JSON or SSE, using the model's configured candidates |

Standalone endpoints require explicit model activation:

| Method and path | Purpose |
|---|---|
| `POST /v1/embeddings` | Text or model-scoped token IDs, float or Base64 vectors |
| `POST /v1/decisions` | OpenAI Predicate, Choice, and Score with ordered answers |
| `POST /typesafe/v1/systemone` | TypeSafe Noul, Choice, and Score with keyed answers |
| `POST /v1/images/generations` | Static inline images or preview SSE |
| `POST /v1/images/edits` | Static multipart edits or preview SSE |
| `POST /v1/audio/speech` | Binary audio or audio delta SSE |
| `POST /v1/audio/transcriptions` | Multipart upload (`json`, `text`, or JSON SSE) |

The [OpenAPI summary](../openapi.json) describes envelopes and selected constraints, not full vendor API conformance. Codecs define exact admission. Business input cannot choose an upstream origin, credential or auth header; inbound headers are not forwarded.

### Static image generation

Enable `gpt-image-2.6-flare` or `gpt-image-2.6-sunburst` in `models.json` with a named `openai` Platform API-key account, or enable `qwen-image-3.0-pro`, `gemini-nano-banana-2.1` or `grok-imagine-image-2.0`, to serve standard OpenAI `POST /v1/images/generations` (JSON) and `POST /v1/images/edits` (`multipart/form-data`, using `image` or repeated `image[]`) without custom profile headers. All models return bounded inline Base64 artifacts defaulting to PNG, and support `output_format` (`png`, `jpeg`, `webp`) plus `output_compression` (`0..=100` for `jpeg`/`webp`). When an upstream native encoding differs or recompression is requested, the gateway normalizes via the operator-configured [FFmpeg executable](gateway-configuration.md#media-format-transcoding) without altering image dimensions (matching encodings with no recompression pass through without FFmpeg; if the upstream omits `created`, the gateway records preparation time, and native accounting is omitted). Direct-input edits accept up to 16 declared `image/png`, `image/jpeg` or `image/webp` sources within an 8 MiB aggregate raw input budget. On OpenAI GPT Image targets, preview SSE streaming (`stream: true`, `n: 1`, `partial_images: 0..=3`), PNG `mask` (< 4,000,000 bytes bound to source 0), `input_fidelity`, `quality`, `background` (`transparent`/`opaque`/`auto`), `moderation` (Generate-only) and `safety_identifier` work natively. On Qwen/Gemini/Grok, `stream: true` (`n: 1`) completes statically and returns a single `completed` SSE frame (without preview images or synthesized `usage`); `mask`, `background: "transparent"`, exceeding `n` limits (1 on Gemini/Grok, 6 on Qwen) and exact `size` on Gemini/Grok reject, while advisory `quality`, `background: "opaque"`, `moderation`, `input_fidelity`, `partial_images`, `safety_identifier` and Gemini/Qwen `user` hints are omitted. Neither `file_id`/JSON edit references, SVG, nor GIF is admitted.

See [Qwen model configuration](gateway-configuration.md#qwen-image-candidates) for activation and [native static images](../architecture/adaptation.md#native-static-images) for exact field rules.

### Speech synthesis

Enable `gpt-4o-mini-tts` (OpenAI), `qwen-audio-3.0-tts-plus` (`aliyun-tokenplan-cn`, voices `longanlingxin`/`longanlufeng`) or `mimo-v2.5-tts` (`xiaomi`, voices `mimo_default`/`冰糖`/`茉莉`/`苏打`/`白桦`/`Mia`/`Chloe`/`Milo`/`Dean`) for standard OpenAI `POST /v1/audio/speech` supporting all six `response_format` encodings (`mp3`, `opus`, `aac`, `flac`, `wav`, `pcm`) and both `stream_format` modes (`audio` and `sse`). Model voices are strictly checked per target; unsupported `speed` or `instructions` (on Aliyun) are omitted. Native-matching formats use direct passthrough or native incremental SSE (`gpt-4o-mini-tts` and `qwen-audio-3.0-tts-plus` MP3); other formats normalize statically via bounded FFmpeg and, when `stream_format: "sse"` is requested, emit chunked `speech.audio.delta` frames plus `speech.audio.done` (without fabricating token usage). Custom voice objects reject.

### Audio transcription

Enable `gpt-transcribe` (OpenAI), `qwen-audio-3.0-asr-flash` (`dashscope-qwen-asr` or `tokenplan-qwen-asr`) or `mimo-v2.5-asr` (`xiaomi`) for standard `POST /v1/audio/transcriptions` accepting direct uploads up to 4 MiB in all nine OpenAI containers (`wav`, `mp3`, `flac`, `mp4`, `mpeg`, `mpga`, `m4a`, `ogg`, `webm`), `response_format` (`json` or `text`) and `stream` (`false` or `true` with `json`). Native-streaming OpenAI forwards directly; Aliyun and Xiaomi transcode non-`wav`/`mp3` uploads to 128 kbps MP3 via bounded FFmpeg before dispatch, omit unsupported advisory controls (`languages[]`, `keywords[]`, `prompt`, `temperature`, and out-of-range `language`), and deliver `transcript.text.delta` + `transcript.text.done` when `stream: true` (omitting non-token `duration` usage). Subtitle/diarized/verbose formats, `chunking_strategy`, `include` and `timestamp_granularities` reject.

### Embeddings

Explicitly list the model in `models.json`, for example `"text-embedding-3-large":{"account":"primary"}`, and provision the named account for its bound Provider. Other enabled accounts are not task fallback targets.

| Model | Provider | Default width | Explicit `dimensions` |
|---|---|---:|---|
| `text-embedding-3-large` | `openai` (direct Platform API key, not `chatgpt` OAuth) | 3072 | 1–3072 |
| `qwen3.7-text-embedding` | `aliyun-dashscope-cn` | 1024 | 256, 512, 768, 1024, 1536, 2048 or 2560 |
| `gemini-embedding-2` | `google` | 3072 | 128–3072 |
| `text-embedding-3-small` | `openai` (direct Platform API key) | 1536 | 1–1536 |

Use the same binding and width for indexing and querying; matching widths do not make vector spaces interchangeable.

Send the ordinary OpenAI request envelope to `/v1/embeddings`, for example `{"model":"gemini-embedding-2","input":["first","second"],"dimensions":128,"encoding_format":"base64"}`. Google uses native synchronous requests internally, with explicit no-truncation. The downstream path is not Google's native REST interface.

`text-embedding-3-small` also accepts one token sequence or a batch of sequences under the declared ordinary `cl100k_base` vocabulary. Use IDs from that tokenizer only; special/control tokens, unknown IDs, mixed text/token batches and token inputs to the text-only bindings reject before dispatch. There is no detokenization or automatic splitting. The gateway caps each response at 60,000 vector values; at width 3072, at most 19 inputs fit in one request.

Compatible profiles preserve valid vectors even when ancillary reports are absent, while returning the complete OpenAI response shape including both usage counters.

A missing model report uses the bound public label.

Real usage takes precedence, followed by attested input-only derivation; remaining missing counters use the request-local [approximation rule](../architecture/index.md#google-embedding-compatibility) without fabricating all-zero defaults or altering reported zeros. Approximate counts do not reflect exact billing or quotas.

Unsupported `user` hints omit on Google/DashScope and forward on OpenAI bindings (which require matching upstream model and both reported usage counters). Because the pinned OpenAI Python SDK's strict validator types `embedding` as a float list, pass explicit `encoding_format: "float"` when `_strict_response_validation=True`; ordinary mode handles Base64 and float responses alike.

### Decisions

[Enable models in `decisions`](gateway-configuration.md#decision-models) and authenticate with a gateway Agent key, not a Provider key. Each profile uses one target per request, without switching or retries.

| SDK profile | Base URL | Qualified model | Evaluation and discovery |
|---|---|---|---|
| OpenAI Python | `http://127.0.0.1:8080/v1` | `gpt-6-luna` | `client.decisions.create`, `client.models.list`, `client.models.retrieve` |
| TypeSafe Python | `http://127.0.0.1:8080/typesafe` | `jev-1.14.0` | `client.system_one`, `client.models.list` |
| TypeSafe Python | `http://127.0.0.1:8080/typesafe` | `gemini-decision-1` | `client.system_one` (plain-text Noul only), `client.models.list` |
| TypeSafe Python | `http://127.0.0.1:8080/typesafe` | `gpt-6-luna-noul` | `client.system_one` (plain-text Noul only, OpenAI upstream), `client.models.list` |

OpenAI accepts text or user messages with ordered text and `input_json` parts; each JSON part is an object or array of objects. Instructions, choice descriptions and score labels/descriptions are text. TypeSafe accepts a string, object or array as `state`, text instructions, text or `null` Choice criteria, and ordered text Score criteria. Noul in this profile requires text instructions. TypeSafe accepts the SDK's default single-frame zstd request compression; auth, wire bytes, decoder window and expanded bytes are bounded before dispatch. Model discovery is local, account-free at request time, and protocol-specific. A known model on the wrong Decision protocol returns 404.

Both projected TypeSafe bindings accept a non-empty string `state` and named Noul questions with non-empty string `instructions` and no binary rubric. A batch containing Choice, Score or structured state rejects before dispatch. Each returns the original probability as `noul` and the reported input/output counts, without a `confidence` field. The result's `model` is the activated binding label.

- `gemini-decision-1` keeps role-less state, accepts up to 1,000 questions, and returns `422 decision_blocked` on a whole-request Gemini policy block.
- `gpt-6-luna-noul` sends the unchanged `state` text as one OpenAI `user` message and accepts up to 400 questions. If Luna refuses any question, the entire request returns `422 decision_refused`, without partial answers.

Decision results are complete JSON with reported usage and typed answers; native OpenAI refusals retain their answer slots. Media, non-user OpenAI history, cache or attribution controls, TypeSafe experimental opt-ins, stateful continuation and other cross-family mappings are not admitted. TypeSafe validation uses HTTP 422 with `detail`; other TypeSafe errors are sanitized under the same `detail` envelope.

### Generation Text model discovery

Use the ordinary gateway Agent key with `GET /v1/morphiecore/models`, or append a public model ID for one descriptor. The enhanced view includes only activated Generation Text interfaces (`chat_completions`, `responses`, `messages`), with assessed image-input domains where applicable. Standard `/v1/models` remains unchanged; Generation capabilities share the same compiled contract. Other-task discovery is [deferred](../roadmap/index.md#deferred-generation-semantics).

List responses use `object: "morphiecore.model_list"` with an array in `data`; retrieval uses `object: "morphiecore.model"` with the same descriptor in `data`.

Each descriptor has `id`, `name`, `owned_by`, `preferred_interface` and `interfaces`.

Each interface identifies its public protocol and delivery modes, plus `input`, `output`, planning `token_limits`, text `features` and reasoning history/activation/effort declarations under `generation`.

The [OpenAPI schemas](../openapi.json) define the complete shape and [Architecture](../architecture/adaptation.md#generation-text-model-discovery) defines the assessed domains.

`preferred_interface` is advisory: it recommends only the primary candidate's native corresponding interface after its assessed Agent-default requests qualify. An inactive, unqualified or unmapped native interface yields `state: "not_assessed"` and null `protocol`/`uniform`. An assessed preference includes its `basis`; `uniform` states whether all candidates qualify the same native interface. This is not a conversion-loss ranking or a routing instruction.

`generation.agent_defaults` names its pinned public shape source, request modes and separately scoped text/tool/reasoning assessments. It evaluates startup preflight, including the configured ingress body limit, not Provider execution or a complete Agent loop. Interface `delivery_adaptations` lists separate output-domain cases. `/capabilities` exposes these Generation default assessments as `agent_defaults` from the same compilation.

`input.text` and `output.text` carry capability states.

`input.image` is a bounded array of positive domains, each with `placement` (`user` or `tool_result`), `source` (`inline` or `url`), `detail` (`omitted` or `auto`), state/rules and, for inline sources only, `mime_types`.

An empty array is no positive declaration, not a global statement about the native model.

Do not combine domains into an unsupported cross-product.

Pi Responses needs inline PNG/JPEG/WebP/GIF with explicit `auto` at both placements; user-image support alone is insufficient.

Audio/video/document inputs and non-text output are not assessed by this runtime view.

Read capability `state`, not just field presence:

- `faithful`: the declared simple domain has a faithful carrier.
- `approved_adaptation`: the domain is admitted with possible named `rules`.
- `unavailable`: the assessed domain is rejected or has no effective control carrier.
- `non_uniform`: candidate differences prevent a common positive statement.
- `not_assessed`: insufficient evidence; missing declarations have the same meaning.

Positive effort/activation choices include `values`; nonpositive choices have no values.

Effort scopes stay separate: Chat/Responses use reasoning effort, Messages uses response-wide effort.

Effort `values` lists only route-common native qualitative names; `compatibility_values`, when present, lists other effective accepted source names that require mapping.

For example, a low/high/max model can accept medium for ordinary clients without advertising medium as an independent model setting.

Plugins use `values`, not the union.

Scope substitutions still carry named adaptations; numeric budget presets are not native qualitative settings.

`enabled` describes the documented minimal manual-thinking domain, not every numeric budget.

Function tools, strict schemas, parallel permission, JSON-object and JSON-Schema output are independent, scoped declarations—not all combinations, schemas or tool/replay loops.

Tool-selection and declaration-history features assess these additional finite source domains through the same candidate projector:

| Feature | Assessed domain |
|---|---|
| `serial_tool_calls` | One root function, automatic selection and an explicit serial flag |
| `named_tool_choice` | One root function and native named selection: exactly one call for Chat/Responses, one-or-more for Messages |
| `allowed_tool_subset` | Two root functions restricted to a required singleton flat subset; Messages has no source carrier |
| `ordered_tool_declarations` | One root function followed, after a user message, by one distinct inline function addition; Chat has no source carrier |

These witnesses do not assess arbitrary declaration replacement/removal, collision resolution, schemas or complete replay loops. In particular, the mixed root/inline domain may be unavailable on a binding that supports inline-only declarations. Approved active-set collapse and allowed-inventory filtering expose their named loss rules; native or equivalent carriage remains faithful. The capabilities endpoint carries the corresponding finite `domain` alongside each request feature.

`reasoning.history` reports the route-common [binding history fact](../architecture/history-and-replay.md#binding-reasoning-history-contract): `unsupported`, `ignored`, `accepted` or `required_with_tools`.

Differing assessed candidates yield `non_uniform`; any unassessed candidate yields `not_assessed`.

It is independent of wire carriers and full-loop acceptance.

Standard Chat thinking-enabled/default tools reject on required-history candidates; the gateway does not switch thinking off.

An explicit runtime-selected off request is assessed independently.

The default tool probe may therefore report `unavailable` even when an explicit off request is admissible.

Limits are planning hints; `null` means unknown, not unlimited.

Context/input capacities come from applicable trusted declarations, including explicit candidate configuration; missing or non-JSON-safe facts on any candidate leave the common value unknown.

The [capacity evidence](../refs/2026-10-26-model-discovery.md) distinguishes exact native source facts from unknown proxy/subscription limits.

A known output maximum is not a default; it does not guarantee the entire maximum can be visible output or combined with other maxima.

Discovery implements no token counting and never substitutes an input maximum for a context window.

The pi plugin uses advertised total context and output limits before global fallback settings, retaining explicit per-model caps. If total context is absent but an input maximum is known, that input capacity supplies pi's planning fallback; it is not a newly discovered total context and output is never added to it. A successful discovery refresh updates the current model at the next idle boundary and refreshes stale scoped selections. Native pi settings still own the chosen thinking level. Offline startup retains cached metadata; refresh online before expecting changed gateway declarations.

The view is startup-immutable, sorted, and complete without pagination or revision fields; auth, `no-store`, automatic `HEAD`, and shutdown behavior match standard Models. At most 64 text models publish, and both list and detail must fit `min(response_bytes, 1 MiB)` or startup fails; `GET` does no upstream I/O and returns `model_not_found` for unknown or non-text models.

### Capability discovery and adaptation notice

`GET /v1/morphiecore/capabilities` uses the ordinary ingress token and returns a startup-immutable `version: 1`, `scope: "gateway_contract"`, `coverage: "partial"` snapshot covering activated Generation and Embedding models (other standalone media tasks are excluded). It fits `min(response_bytes, 4 MiB)`, performs no upstream I/O, supports `HEAD`, and evaluates limited finite witness domains per `feature` as `faithful`, `approved_adaptation` (with stable `rules` IDs), `rejected`, `non_uniform`, or `not_assessed`. Unlisted cases (such as safety policy or cache retention) remain `not_assessed`; full details of witness rules, fallbacks (`generation.complete-delivery-usage.v1`, `responses.infer-required-settings.v1`, `chat.coarsen-completion-cause.v1`, `responses.omit-completion-cause.v1`, `generation.omit-delivery-message-phase.v1`), and [actual-loss reporting](../architecture/adaptation.md#public-capability-and-adaptation-notice) belong to [Architecture](../architecture/adaptation.md).

Embedding descriptors declare `protocol: "embeddings"` (JSON only), input kinds, optional tokenizer/token limits, dimension/default/count limits, native/client encodings (`float` for omitted raw HTTP; Base64 default in Python SDK), [binary32 rounding](../architecture/index.md#embedding-encoding-compatibility), and `usage_may_be_estimated`.

All Generation responses (JSON, SSE, and pre-frame failure streams) and successful Embedding responses return conservative headers `X-MorphieCore-Adaptation: possible` and `Link: </v1/morphiecore/capabilities>; rel="describedby"` (omitted on pre-response errors, discovery, history routes, and other standalone tasks). Generation envelopes may also carry optional safe-to-ignore `morphiecore` notices without changing error semantics, tool execution, or stateless continuation; Google ordinary-text/call signatures without a standard carrier still reject rather than synthesizing replay metadata. Native replay/phase carriers retain their [contract](../architecture/history-and-replay.md#reasoning-replay).

## Errors and limits

OpenAI routes return `{"error":{"message","type","param","code"}}`. Messages errors, including authentication failures before body polling, use Anthropic's `{"type":"error","error":{"type","message"}}` envelope. Unclassified Messages errors put the public error code in the message, for example `The request could not be completed (upstream_error).` Messages 401 errors use `authentication_error`.

When Standard Chat planning is blocked by required thinking history, HTTP 400 uses `error.type:"invalid_request_error"` and `error.code:"reasoning_history_required"`. Its fixed message explains the missing history carrier and suggests an explicit off request or a compatible model/API. The gateway neither dispatches the rejected request nor retries it with different thinking settings. This is a JSON error before SSE starts, including for `stream:true`; it needs no diagnostic extension. In mixed rejected routes, the first configured blocking candidate supplies the planning diagnosis.

| HTTP | Meaning |
|---|---|
| 400 | Invalid or unrepresentable request, context overflow |
| 401 | Missing, wrong or ambiguous ingress credential |
| 404 / 405 | Unknown model or route / unsupported method |
| 400 / `stateful_api_disabled` | Responses storage, reference continuation or stored-resource operation is experimental and disabled |
| 408 / 413 / 415 | Body timeout / body too large / unsupported media type |
| 429 | Gateway busy, upstream rate limit or exhausted quota |
| 502 / 504 | Upstream or projection failure / upstream timeout |
| 503 / 500 | Shutdown, unavailable credentials/targets or upstream overload / local fault |

Messages maps exhausted quota to 402, upstream overload to 529, gateway timeouts to 504 `timeout_error` and oversized bodies to 413 `request_too_large`. Reported delays and local `upstream_unavailable` recovery delays may appear as `Retry-After`; they are not promises of recovery. Standalone tasks pass only an upstream rate-limit or overload delay within `routing.retry_hint_horizon_ms`.

A Responses stream with gateway-classified context overflow or exhausted quota may return HTTP 200 with a standard standalone `error` event, not a fabricated `response.failed` object. Native upstream failed Responses remain a separate carrier. HTTP 200 alone does not mean success. After body publication, errors can only report failure or abort delivery; no candidate replay or fabricated success terminal is allowed.

Failure diagnostics carry the bounded `generation` execution report: checked conversion boundaries, typed failures, attempt observations and final request outcome. Public error classification is distinct from the actual upstream response head; failed attempts do not become successful-request errors. Private payloads and arbitrary error strings are excluded. See [typed failures](../architecture/adaptation.md#typed-conversion-failures); recording never changes routing or completion behavior.

Resource defaults and bounds are in [Limits](../../crates/morphiecore-gateway/src/startup/limits.rs). Configuration fields above do not expose every embedded-library limit.
