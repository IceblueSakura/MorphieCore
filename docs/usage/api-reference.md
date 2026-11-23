# HTTP API Reference and Errors

This document defines standalone HTTP endpoints (Embeddings, Model Discovery, Capabilities Notice) and gateway error mappings and resource limits.

See [Client Requests](client-requests.md) for generation protocols and [OpenAPI](../openapi.json) for machine-readable schemas.

## Contents

- [HTTP API](#http-api)
  - [Static image generation](#static-image-generation)
  - [Speech synthesis](#speech-synthesis)
  - [Audio transcription](#audio-transcription)
  - [Embeddings](#embeddings)
  - [Generation Text model discovery](#generation-text-model-discovery)
  - [Capability discovery and adaptation notice](#capability-discovery-and-adaptation-notice)
- [Errors and limits](#errors-and-limits)

## HTTP API

| Method and path | Purpose |
|---|---|
| `GET /v1/models` | List activated public models |
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
| `POST /v1/images/generations` | Static inline images |
| `POST /v1/audio/speech` | Binary audio |
| `POST /v1/audio/transcriptions` | One bounded audio multipart upload (`json` or `text`) |

The [OpenAPI summary](../openapi.json) describes envelopes and selected constraints, not full vendor API conformance. Codecs define exact admission. Business input cannot choose an upstream origin, credential or auth header; inbound headers are not forwarded.

### Static image generation

Enable `gpt-image-2.6-flare` or `gpt-image-2.6-sunburst` in `models.json` with a named `openai` Platform API-key account. `/v1/images/generations` returns bounded inline Base64 artifacts, defaulting to PNG. Reported creation time, supported image metadata and usage retain their standard meanings. Neither the retired OpenRouter `gpt-image-2.5-flare` label nor Image Edit, preview streaming, SVG or GIF is admitted.

`qwen-image-3.0-pro`, `gemini-nano-banana-2.1` and `grok-imagine-image-2.0` can use the default OpenAI response shape without extra headers. Omit `output_format` or set it to `png`; Gemini and Grok take one image and do not accept an exact `size`. Qwen already supplies PNG. Gemini and Grok require an operator-configured [FFmpeg executable](gateway-configuration.md#static-image-transcoding) for default delivery. If the upstream does not report creation time, `created` is the time the gateway prepared the artifact, **not** the original creation time. Native accounting is not included.

To keep the original encoding and bytes and leave unreported `created` absent, explicitly set `X-MorphieCore-Image-Profile: native-static-v1`. This mode is not a strict OpenAI Images response; use the SDK's `extra_headers` and read the raw JSON:

```python
import json

response = client.images.with_raw_response.generate(
    model="gemini-nano-banana-2.1", prompt="A flat blue circle on white",
    extra_headers={"X-MorphieCore-Image-Profile": "native-static-v1"},
)
result = json.loads(response.content)
artifacts, output_format = result["data"], result["output_format"]
```

See [Qwen model configuration](gateway-configuration.md#qwen-image-candidates) for activation and [native static images](../architecture/adaptation-and-dialects.md#native-static-images) for supported controls. Omit the header for default delivery and for GPT Image.

### Speech synthesis

Enable `gpt-4o-mini-tts` in `models.json` with a named `openai` Platform API-key account for bounded static binary delivery on `POST /v1/audio/speech`. It accepts OpenAI's 13 built-in voices (`alloy`, `ash`, `ballad`, `coral`, `echo`, `fable`, `onyx`, `nova`, `sage`, `shimmer`, `verse`, `marin`, `cedar`), `instructions`, `speed` (`0.25`–`4`), `response_format` (`mp3`, `opus`, `aac`, `flac`, `wav`, `pcm`) and `stream_format: "audio"`. Custom voice objects and `stream_format: "sse"` reject. Narrower `qwen-audio-3.0-tts-plus` (`aliyun-tokenplan-cn`, `mp3` only, `longanlingxin`/`longanlufeng`) and `mimo-v2.5-tts` (`xiaomi`, `wav`/`pcm`/`mp3`, no `speed`) bindings remain separately activatable.

### Audio transcription

Enable `gpt-transcribe` in `models.json` with a named `openai` Platform API-key account for static multipart transcription on `POST /v1/audio/transcriptions`. It accepts direct uploads up to 4 MiB in nine containers (`wav`, `mp3`, `flac`, `mp4`, `mpeg`, `mpga`, `m4a`, `ogg`, `webm`), `language` or `languages[]` (mutually exclusive), `keywords[]`, `prompt`, `temperature` (`0`–`1`), `response_format` (`json` or `text`) and `stream: false`. Subtitle/diarized/verbose formats, `stream: true`, `chunking_strategy`, `include` and `timestamp_granularities` reject. Narrower `qwen-audio-3.0-asr-flash` (`dashscope-qwen-asr` or `tokenplan-qwen-asr`) and `mimo-v2.5-asr` (`xiaomi`) bindings accept WAV/MP3 up to 4 MiB and `language`, reject `languages[]`/`keywords[]`/`prompt`/`temperature`, and also support downstream `response_format: "text"` projection.

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

Real usage takes precedence, followed by attested input-only derivation; remaining missing counters use the request-local [approximation rule](../architecture/index.md#google-embedding-compatibility).

No default all-zero usage or custom usage fields are emitted.

Reported zeros are preserved.

Approximate counts can be inaccurate across languages and models; do not treat them as exact billing or hard quota measurements.

Unsupported `user` hints are omitted on Google/DashScope and forwarded on the OpenAI bindings. The direct OpenAI bindings require the upstream model to match and both usage counters to be reported; they do not substitute or estimate missing reports.

These adaptations do not claim upstream reporting or attribution.

The pinned OpenAI Python SDK's strict response model still types `embedding` as a float list, so use explicit `encoding_format: "float"` with `_strict_response_validation=True`. Its ordinary mode handles Base64 and SDK-default decoding. This SDK type limitation is separate from complete usage and does not authorize changing a requested encoding; the wire schema supports both representations.

### Generation Text model discovery

Use the ordinary gateway Agent key with `GET /v1/morphiecore/models`, or append a public model ID for one descriptor. The enhanced view includes only activated Generation Text interfaces (`chat_completions`, `responses`, `messages`), with assessed image-input domains where applicable. Standard `/v1/models` remains unchanged; Generation capabilities share the same compiled contract. Other-task discovery is [deferred](../roadmap/backlog-and-frozen.md#models-discovery-interfaces).

List responses use `object: "morphiecore.model_list"` with an array in `data`; retrieval uses `object: "morphiecore.model"` with the same descriptor in `data`.

Each descriptor has `id`, `name`, `owned_by`, `preferred_interface` and `interfaces`.

Each interface identifies its public protocol and delivery modes, plus `input`, `output`, planning `token_limits`, text `features` and reasoning history/activation/effort declarations under `generation`.

The [OpenAPI schemas](../openapi.json) define the complete shape and [Architecture](../architecture/adaptation-and-dialects.md#generation-text-model-discovery) defines the assessed domains.

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

The view is startup-immutable, sorted and complete without pagination or version/revision fields. Authentication, `no-store`, automatic HEAD and shutdown behavior match standard Models. At most 64 text models are published; both list and detail must fit `min(response_bytes, 1 MiB)` or startup fails. Unknown/non-text labels return `model_not_found`; GET does no upstream I/O. Pi consumes this view through its separately managed provider extension; the gateway writes no client configuration and performs no model activation or health polling.

### Capability discovery and adaptation notice

The implemented surface combines a partial capability snapshot, a conservative possible-adaptation header and optional bounded `morphiecore` notices. The [actual-loss reporting contract](../architecture/adaptation-and-dialects.md#public-capability-and-adaptation-notice) separates these projections from the authoritative internal execution report. Notices are safe to ignore, not a stable public schema or a continuation carrier.

The accepted [safe-to-ignore diagnostic extension policy](../architecture/adaptation-and-dialects.md#public-capability-and-adaptation-notice) permits additional namespaced fields only where the selected protocol allows extensions, for example to explain actual data loss.

Ordinary SDK clients need not display, retain or return them.

SDK tolerance alone does not establish a permitted extension position.

Their layout and aggregation are provisional, and no new field spelling or consumer-retention guarantee is documented here.

Standard errors, tool execution and required continuation never depend on these diagnostics; notices cannot authorize an unapproved loss or repair invalid standard output.

`GET /v1/morphiecore/capabilities` uses the ordinary ingress token. It returns a startup-immutable `version: 1` snapshot with `scope: "gateway_contract"` and `coverage: "partial"`. Each entry names an activated public model and client protocol, with separate `request`, `delivery` and `history` case lists. Other standalone media tasks are not assessed. No upstream request or request-body collection occurs; HEAD omits the body. The complete snapshot must fit both `response_bytes` and 4 MiB or startup fails.

Embedding entries use `protocol: "embeddings"` and JSON-only delivery.

The descriptor declares input kinds, optional tokenizer/token limits, dimension/default/count budgets, native and client encodings, and `usage_may_be_estimated`.

Limits intersect with runtime/body ceilings rather than promising every maximum-sized combination fits.

Omitted dimensions use trusted defaults without adding a wire control.

Raw HTTP encoding omission means float; the Python SDK normally sends Base64 on caller omission.

The [numeric policy](../architecture/index.md#embedding-encoding-compatibility) preserves source precision and permits named binary32 rounding only on delivery copies.

Request and delivery rule lists describe compatible projections without exposing credentials or source payloads.

Each case names a `feature`, a limited `domain`, and `faithful`, `approved_adaptation`, `rejected`, `non_uniform` or `not_assessed` treatment. Approved adaptations include stable `rules` IDs. Unlisted cases are `not_assessed`, not implicitly supported or rejected. Statements concern that feature/domain only, assuming all other validation succeeds; neither model-effective settings nor full client loops are asserted.

Generation delivery declares `response.delivery.usage` with `generation.complete-delivery-usage.v1`: reported counts take priority, followed by accounting derivation and a text-only byte-based estimate bounded by known totals/details.

Usage can therefore be approximate and must not be treated as billing or quota truth, especially for media-bearing input.

Responses additionally declares `responses.infer-required-settings.v1` for missing mandatory tool-setting echoes.

See [the fallback rules and limitations](../architecture/adaptation-and-dialects.md#downstream-schema-completion); optional diagnostics never alter standard field meaning.

The current request slice assesses tools, tool selection, output formats and output caps. Unlisted safety-policy, cached-context-reference and cache-retention cases remain `not_assessed` by this partial discovery view; request validation and candidate-local projection still apply.

Delivery lists selected incidental-settings omissions, assessment-only safety-report omission, protected blocking-safety rejection and completion-cause carriers equally for JSON/SSE.

Natural or sequence-matched completion differs from ambiguous stop, and these cases exclude pending tool continuation.

Chat lists `chat.coarsen-completion-cause.v1` and Responses `responses.omit-completion-cause.v1` where a cause has no exact carrier.

Responses capabilities mark experimental stored projections as rejected. Returned native phase/history carriage remains stateless and does not activate storage.

Phase cases describe exact native Responses assistant labels. Chat and Messages preserve visible content/order but omit phase under `generation.omit-delivery-message-phase.v1`; returned phase is not supported there. These cases do not promise phase-aware UI, client retention, or successful conversion of other content/terminals. Request-side phase transfer and cache-retention policies remain unassessed by this partial route-wide view.

Selected Google Messages entries additionally declare `generate_content_stop_without_configured_sequences` or `interactions_completed_without_configured_sequences` with their terminal rules.

The general ambiguity rejection is scoped outside that selected no-stop profile.

These cases still require valid outcome/progress/call shape, representable usage and successful EOF; they do not admit configured stops, unknown failure reasons or unsupported reports.

Google ordinary-text/call signatures have no standard Chat/Messages/stateless Responses carrier and reject instead of creating a reasoning/thinking manifest.

Experimental managed continuation is disabled and supplies no alternative for those signatures; native Messages thinking/redacted-thinking signatures are unaffected.

Generation JSON/SSE and successful Embedding responses carry `X-MorphieCore-Adaptation: possible` and `Link: </v1/morphiecore/capabilities>; rel="describedby"`.

This is deliberately conservative, including when no adaptation occurs and when Responses returns a pre-frame failure stream.

It is not an actual-loss receipt, a success assertion or proof of Provider-effective behavior.

Pre-response auth/admission errors, discovery, history operations and other standalone tasks do not receive these headers; missing headers do not prove exactness.

Generation may additionally attach optional body diagnostics to existing JSON/SSE envelopes, never new receipt-only events.

Missing or truncated notices do not establish losslessness, and JSON/SSE parity or SDK retention of notices is not promised.

Native replay/phase carriers retain their independent [contract](../architecture/history-and-replay.md#reasoning-replay).

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

Failure diagnostics carry the bounded `generation` execution report: checked conversion boundaries, typed failures, attempt observations and final request outcome. Public error classification is distinct from the actual upstream response head; failed attempts do not become successful-request errors. Private payloads and arbitrary error strings are excluded. See [typed failures](../architecture/adaptation-and-dialects.md#typed-conversion-failures); recording never changes routing or completion behavior.

Resource defaults and bounds are in [Limits](../../crates/morphiecore-gateway/src/startup/limits.rs). Configuration fields above do not expose every embedded-library limit.
