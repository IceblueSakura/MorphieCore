# Grok Imagine contracts

Official sources checked on 2026-10-22. These mutable documentation observations do not establish OAuth account access or successful media generation. Scope and remaining gates belong to the [roadmap](../roadmap.md#grok-imagine-adaptation); selected execution policy belongs to [Architecture](../architecture.md#scope-and-activation-decisions).

## Sources

- [Image generation guide](https://docs.x.ai/developers/model-capabilities/images/generation)
- [Images REST reference](https://docs.x.ai/developers/rest-api-reference/inference/images.md)
- [Image-quality retirement](https://docs.x.ai/developers/migration/imagine-image-quality-nov-2)
- [Video overview](https://docs.x.ai/developers/model-capabilities/video/overview)
- [Videos REST reference](https://docs.x.ai/developers/rest-api-reference/inference/videos.md)
- [Video 1.5 lite model](https://docs.x.ai/developers/models/grok-imagine-video-1.5-lite)

## Images

The generation endpoint is `POST /v1/images/generations`. It accepts `model`, `prompt`, `n`, `aspect_ratio`, `resolution` and `response_format`; the latter selects URL or `b64_json`. Static results contain ordered `data` with URL/base64 and optional `mime_type`, plus optional usage. Do not manufacture the `created` timestamp expected by some other image contracts.

The guide explicitly gives `grok-imagine-image-2.0` quality values `low` and `medium`, defaulting to `medium`. The REST reference read on the same date omits that request field. It also lists `1.5k` resolution whereas the guide lists `1k` and `2k`. Pin the selected model's effective subset and resolve these differences rather than treating the generic schema as blanket model support.

The guide exposes moderation metadata, while the static REST result list does not describe that field. Confirm its wire shape before claiming faithful moderation projection. MIME/encoding and effective model reports must follow source evidence, not requested settings or model-name guesses.

The retirement notice says `grok-imagine-image-quality` retires on **2026-11-02**, following notice on 2026-09-02. Its slug continues to resolve, but generation/edit requests will be served by `grok-imagine-image-2.0` with `quality: "low"`; the notice says responses report the serving model. This is an upstream behavior change, not permission for a gateway alias or silent rewrite.

The broader API also exposes edits, deferred image jobs and file/storage/output destinations. Those carriers are not prerequisites for static generation.

## Video

`POST /v1/videos/generations` returns `request_id`, not a completed video. `GET /v1/videos/{request_id}` retrieves status/results. The generation reference specifies duration 1–15 seconds, default 8; resolution values include 480p, 720p and 1080p. An optional image accepts a URL/data URL or a Files ID; these forms have different trust/resource prerequisites.

The overview describes 1.5 and 1.5-lite text-to-video and image-to-video with generated audio. Full 1.5 additionally supports references, voices and pinned frames, and native 1080p; lite's 1080p is upscaled from 720p. A shared endpoint does not imply identical model capabilities.

The lite model page advertises 480p at USD 0.02 per generated second and 720p at USD 0.03, plus input-image charging. These are published prices, not measured account charges or an OAuth entitlement promise.

Polling includes `status`, optional `progress`, `error`, `model`, `usage` and `video`. A final video reports duration, moderation and an optional URL. The REST reference explicitly says a moderation-rejected video has an empty URL. Guide examples distinguish pending/done/failed and sometimes expired; do not accept an arbitrary status as a successful terminal or poll forever.

Usage can include USD ticks and token details. One dollar is 10,000,000,000 ticks; cached input tokens are a subset, not an additive count. Do not reinterpret monetary ticks as tokens or fabricate unreported totals.

The inspected Videos reference lists generation, edits, extension and result retrieval, but no cancellation endpoint. Local disconnect/deadline cancellation therefore cannot promise stopped remote computation or charges. POST submission uncertainty must not cause automatic resubmission.

## Evidence still needed

- The examples authenticate with API keys. The successful OAuth models-list request does not prove image/video generation access. Use the project credential loader for a separately bounded live check; do not extract tokens for curl.
- Confirm the selected image request/result discrepancies, including actual encoding, moderation and effective-model carriers.
- Independently verify wire-to-IR and IR-to-wire behavior, finite waiting/resource bounds, cancellation, terminal honesty and same-account polling.
- A returned artifact URL alone does not prove a usable video. Any later download verification must be separately bounded, avoid sending OAuth credentials to artifact hosts and avoid arbitrary server-side URL fetching.
