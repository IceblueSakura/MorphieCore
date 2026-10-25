# MiniMax H3 video contracts

Official sources checked on 2026-10-22. These mutable documentation observations establish neither account access nor a successful generation. Video integration requires [operation selection](../roadmap.md#grok-imagine-adaptation) under the current stage-4 schedule; this note is evidence for the stage-4 video operation, not a design approval.

## Sources

- [Video Generation guide](https://platform.minimax.io/docs/guides/video-generation)
- [Create Video Generation Task](https://platform.minimax.io/docs/api-reference/video-generation-v2-create)
- [Query Task](https://platform.minimax.io/docs/api-reference/video-generation-v2-query)
- [List Tasks](https://platform.minimax.io/docs/api-reference/video-generation-v2-list)
- [Cancel or Delete Task](https://platform.minimax.io/docs/api-reference/video-generation-v2-delete)
- [Create H3-Context-IR Task](https://platform.minimax.io/docs/api-reference/video-generation-v2-h3-context-ir)
- [Create Video Regeneration Task](https://platform.minimax.cn/docs/api-reference/video-generation-v2-regeneration)
- [API Overview](https://platform.minimax.io/docs/api-reference/api-overview), [Rate limits](https://platform.minimax.io/docs/guides/rate-limits), [Pricing](https://platform.minimax.io/docs/pricing/overview)
- Legacy v1: [Text-to-video](https://platform.minimax.io/docs/api-reference/video-generation-t2v), [Query](https://platform.minimax.io/docs/api-reference/video-generation-query), [Retrieve File](https://platform.minimax.io/docs/api-reference/file-management-retrieve), [Upload File](https://platform.minimax.io/docs/api-reference/file-management-upload)
- CN mirror: [创建视频生成任务](https://platform.minimax.cn/docs/api-reference/video-generation-v2-create), [速率限制](https://platform.minimax.cn/docs/guides/rate-limits)

## Protocol shape

MiniMax H3 (`MiniMax-H3`, `MiniMax-H3-Max`) is **not** an OpenAI-compatible surface. The compatibility claims in the same documentation cover only chat (`/anthropic`, `/v1/chat/completion`) and hosted tools. Video is a proprietary asynchronous task protocol: one endpoint to create a task, one to read its state, plus list/cancel and two auxiliary create endpoints.

| Operation | Wire |
|---|---|
| Create generation | `POST /v2/video_generation` |
| Query task | `GET /v2/query/video_generation/{task_id}` |
| List tasks | `GET /v2/query/video_generation?page_num&page_size[&filter.*]` |
| Cancel or delete | `DELETE /v2/video_generation/{task_id}` |
| Create prompt-enhancement task | `POST /v2/h3_context_ir` |
| Create regeneration task | `POST /v2/video_generation` regeneration variant |
| Upload input asset | `POST /v1/files/upload` (`purpose=video_generation_input`) |

Authentication is `Authorization: Bearer <api_key>` only; no signature, no project/group scope in the path. Hosts: `api.minimax.io` (international) and `api.minimax.cn` (CN). The v2 OpenAPI description strings call the same API "video generation V2 (Hailuo-03)", so H3 is the Hailuo-03 line under a new model name.

### Content array

The request body carries `model`, `resolution`, `duration`, optional `ratio`, `extra` (H3-Max only), `callback_url`, `aigc_watermark`, and a required `content[]`:

- Items are distinguished by `type`: `text`, `image_url`, `video_url`, `audio_url`. Non-text items carry `{"<type>": {"url": …}}` plus a `role`.
- Exactly one non-empty `text` item is mandatory in every mode. Text-only content means text-to-video.
- Roles: `first_frame`, `last_frame`, `reference_image`, `reference_video`, `reference_audio`, `base_video` (regeneration only).
- First/last-frame and reference roles are mutually exclusive in one request; mixing is a parameter error.
- `resolution` is per-model: H3 `768P|2K`; H3-Max `480P|768P`, no `2K`. `duration` is an integer enum 4–15 (H3) or 5–15 (H3-Max).
- `ratio`: text-to-video requires a concrete ratio and rejects `adaptive`; image-to-video forces `adaptive` and silently ignores a concrete value; reference-to-video defaults to `adaptive` but accepts a concrete ratio. The effective ratio is reported back on the task.
- `url` accepts an `http(s)` URL, a `data:` URI, or `mm_file://{file_id}` from a `video_generation_input` upload, which is valid for 7 days. Body limit is 64 MB; the guide asks for public URLs instead of Base64 for large assets.
- Asset limits: images ≤ 30 MB, `[256, 5760]` px, aspect `[0.4, 2.5]`, ≤ 9 reference images; videos MP4/MOV, ≤ 50 MB, ≤ 3 clips, 2–15 s each and ≤ 15 s total; audio WAV/MP3, ≤ 15 MB, ≤ 3 clips; at most 12 mixed reference files.

`extra.prompt_expansion_mode` is documented as the only H3-Max extra, defaulting to `balanced` when absent. The documentation does not enumerate the accepted non-default values and explicitly forbids `balance`.

### Task object and status

Create returns only `{"task_id": "…"}` — no status, no echo of the request. Query returns `{"task": {id, model, status, created_at, updated_at, content, error?, resolution, duration, usage, ratio, task_type, modality}}`.

- `status`: `queued`, `running`, `succeeded`, `failed`, `cancelled`.
- On success `content.url` holds a time-limited video URL; an H3-Context-IR task instead returns `content.prompt` (an `integrated_multimodal_description` with shot timing and soundscape) and `modality: "text"`.
- `task_type`: `generation`, `h3_context_ir`, `regeneration`.
- `usage` appears populated only after success: `total_seconds`, `input_seconds`, `output_seconds`, `input_image_count`, `input_audio_seconds`, and token counts (`total_tokens`, `prompt_tokens`, `completion_tokens`).
- Query and list cover only the last 7 days (`[T-7d, T)`); outside that window the task ID is invalid.
- The guide recommends a 10 s poll interval. `failed` and `cancelled` are terminal.

`DELETE` is state-dependent and is both cancellation and record deletion: `queued → cancelled` (no charge), `succeeded|failed → deleted`, while `running` and `cancelled` return an error. A running task therefore cannot be cancelled at all, so a local deadline cannot promise stopped remote computation or stopped billing.

### Errors

Create/query transport errors use an envelope that resembles OpenAI's:

```json
{"type":"error","error":{"type":"rate_limit_error","message":"rate limit, please retry later (1002)","http_code":"429"},"request_id":"…"}
```

Mapping is by HTTP status and by `error.type` (`bad_request_error` 400, `authorized_error` 401, `insufficient_balance_error` 402, `unprocessable_entity_error` 422, `rate_limit_error` 429, `server_error` 500, plus an `overloaded_error` 529 in the CN reference). Two properties differ from OpenAI: the numeric MiniMax code is embedded at the end of the human-readable `message` (there is no `code` or `param` field), and `http_code` is a string. Task-level failures use a *second* carrier inside the task object: `task.error.error.code` (`"1026"` for sensitive content) plus `task.error.error.message`. Any adapter must parse both carriers and must not infer the numeric code from the envelope.

### Callback

`callback_url` is available on create, H3-Context-IR and regeneration. Verification is a challenge echo: MiniMax POSTs `{"challenge": …}` and the receiver must return the value unchanged within 3 s. Afterwards each status change POSTs a body structured like the query response, with `status` in the same five values. The documentation describes no signature, shared secret or source authentication for the callback, so a push cannot be trusted without separate verification against the query endpoint.

### Limits and accounting

Published limits disagree between the two documentation mirrors, checked the same day:

- EN [rate limits](https://platform.minimax.io/docs/guides/rate-limits): Video Generation V2, limit type `CONN` (max concurrent tasks), free tier 2, paid tier 15; legacy Hailuo v1 uses RPM 5/20.
- CN [速率限制](https://platform.minimax.cn/docs/guides/rate-limits): Video Generation V2, RPM 300 and 30 concurrent inflight tasks.

Do not hardcode either set; treat concurrency as plan-dependent and re-check before designing admission control. Billing is per output second ($0.08/s at 768P, $0.13/s at 2K, H3-Max $0.05/s at 480P) with separate input-image and input-video charges; prepaid Video Packages explicitly exclude H3.

## Legacy v1 envelope (Hailuo series)

`MiniMax-Hailuo-2.3`, `MiniMax-Hailuo-2.3-Fast`, `MiniMax-Hailuo-02`, `T2V-01-Director` and `I2V-01*` remain on a different, incompatible protocol:

- `POST /v1/video_generation` takes flat fields — `model`, `prompt` (≤ 2000 chars, `[command]` camera syntax), `duration` (6 or 10), `resolution` (`720P|768P|1080P`), `prompt_optimizer`, `fast_pretreatment`, `first_frame_image`, `last_frame_image`, `callback_url` — and returns `{"task_id", "base_resp": {"status_code", "status_msg"}}`.
- `GET /v1/query/video_generation?task_id=` returns `status` in `Preparing|Queueing|Processing|Success|Fail` (capitalised) plus `file_id`, `video_width`, `video_height` and `base_resp`. There is no URL.
- The artifact needs a third call: `GET /v1/files/retrieve?file_id=` → `file.download_url` with `purpose: "video_generation"`, again wrapped in `base_resp`.
- v1 callback statuses are lowercase `processing|success|failed`, i.e. a third spelling that matches neither the query enum nor the v2 enum.

v1 and v2 therefore differ in request body, status vocabulary, error envelope, artifact access (file-ID indirection versus direct URL) and namespace. They must not share one codec or one status mapping.

## Design inputs, not decisions

- This is a second, differently shaped async-task protocol alongside [Grok's](2026-10-22-grok-imagine.md) `request_id` + poll design, so the selected video task owner must not be shaped to Grok's response format alone.
- Artifact delivery is URL-only and time-limited; retention and re-fetch cannot rely on upstream task identity, which expires after 7 days.
- `mm_file://` introduces a Provider-owned file reference as an *upstream* input carrier for standalone operations. That is a separate contract from the conversational media carriers recorded in [Multimodal carriers](multimodal.md), and it implies upload, identity and expiry handling rather than inline bytes.
- Silent degradation exists on the wire (`ratio` ignored for image-to-video; unknown-field strictness not stated for `extra`), which needs an explicit recorded rule rather than an assumption of lossless forwarding.
- Terminal honesty: submission returns nothing but an ID, `usage` is only meaningful on success, and failure detail lives inside the task object.

## Evidence still needed

- A bounded live call under the project credential loader: text-only H3 submission, poll to a terminal state, one download check without sending credentials to the artifact host.
- Whether `mm_file://{file_id}` and `data:` URIs are actually accepted by the v2 `content` URLs (documented, not observed), and how expiry is reported.
- The real accepted values of `extra.prompt_expansion_mode` and the strictness of undeclared fields.
- Whether query/list use the same OpenAI-shaped error envelope as create, including `request_id`.
- Callback replay/forgery resistance, and whether the same push body ever carries `task.error`.
- Default and maximum `page_size`, and whether `filter.task_ids` is repeatable as documented.
- The actual concurrency limit for the account tier in use, given the two mirrors' disagreement.
