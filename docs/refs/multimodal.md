# Multimodal carriers

Research notes on how each protocol carries image, audio, video and document input, tool-result media, Provider file references and generated media. Collected on 2026-10-12. Vendor pages are unversioned; recheck a row before relying on it. Code defines what MorphieCore actually accepts.

## Contents

- [Sources](#sources)
- [User message input](#user-message-input)
- [Tool-result media](#tool-result-media)
- [Provider file APIs](#provider-file-apis)
- [Generated media](#generated-media)
- [Client behavior](#client-behavior)
- [Consequences for MorphieCore](#consequences-for-morphiecore)
- [Open questions](#open-questions)

## Sources

| Source | Revision | Used for |
|---|---|---|
| Gemini API discovery (`generativelanguage.googleapis.com/$discovery/rest?version=v1beta`) | revision `20261010` | `Part`, `Blob`, `FileData`, `FunctionResponse`, `File` schemas |
| [Gemini file input methods](https://ai.google.dev/gemini-api/docs/file-input-methods), [Files](https://ai.google.dev/gemini-api/docs/files), [Video understanding](https://ai.google.dev/gemini-api/docs/video-understanding), [Thought signatures](https://ai.google.dev/gemini-api/docs/generate-content/thought-signatures) | unversioned | Limits, URL rules, retention |
| [Interactions API v1](https://ai.google.dev/api/interactions-api-v1), [v1beta](https://ai.google.dev/api/interactions-api) | unversioned | Content and step unions |
| anthropic-sdk-python | [`b4b7916`](https://github.com/anthropics/anthropic-sdk-python/tree/b4b7916deaf4e5570cd627b1ae7ee4394a4de39f) (1.13.0) | Block and source types |
| [Anthropic Files API](https://platform.claude.com/docs/en/build-with-claude/files), [Vision](https://platform.claude.com/docs/en/build-with-claude/vision), [PDF support](https://platform.claude.com/docs/en/build-with-claude/pdf-support) | unversioned | Limits, scope, retention |
| openai-python | [`5e7a23e`](https://github.com/openai/openai-python/tree/5e7a23e) (3.29.0) | Chat and Responses content types, Files types |
| [OpenAI file inputs](https://developers.openai.com/api/docs/guides/file-inputs), [Images and vision](https://developers.openai.com/api/docs/guides/images-vision), [Files create](https://developers.openai.com/api/reference/resources/files/methods/create) | unversioned | Limits and per-API methods |
| Open Responses | [`7078a8f`](https://github.com/openresponses/openresponses/tree/7078a8f) | `input_video` placement |
| [OpenRouter video](https://openrouter.ai/docs/guides/overview/multimodal/videos), [audio](https://openrouter.ai/docs/guides/overview/multimodal/audio) | unversioned | Chat and Responses extensions |
| [DashScope Chat](https://help.aliyun.com/zh/model-studio/qwen-api-via-openai-chat-completions), [Zhipu Chat](https://docs.bigmodel.cn/api-reference/%E6%A8%A1%E5%9E%8B-api/%E5%AF%B9%E8%AF%9D%E8%A1%A5%E5%85%A8), [vLLM multimodal inputs](https://docs.vllm.ai/en/latest/features/multimodal_inputs.html) | unversioned | Chat-compatible extensions |
| MiniCPM-V `docs/api.md` | [`6ada8e8`](https://github.com/OpenBMB/MiniCPM-V/blob/6ada8e8/docs/api.md) | ModelBest Chat video |
| pi, opencode | revisions pinned in [References](../references.md) | Shapes real clients send |

## User message input

| Protocol | Image | Audio | Video | Document | Remote sources |
|---|---|---|---|---|---|
| OpenAI Chat | `image_url` with URL or data URL; `detail` `auto/low/high/original` | `input_audio` base64, `format` `wav` or `mp3` | None | `file`: PDF only, `file_data` data URL or `file_id`; no `file_url` | Image URLs |
| OpenAI Responses | `input_image`: `image_url` or `file_id`, `detail` | None (`input_audio` exists only in evals types) | None | `input_file`: PDF, text, office and spreadsheet types; `file_data`, `file_url` or `file_id`; `filename`, `detail` (PDF only); 50 MB per request | Image and file URLs |
| Anthropic Messages | `image`: `image/jpeg/png/gif/webp`, source `base64`, `url` or `file`; 10 MB base64, 8000 px | None | None | `document`: PDF via `base64`, `url` or `file`; `text` source (`text/plain`); `content` source (blocks); optional `title`, `context`, `citations` | Anthropic fetches `url` sources |
| Gemini generateContent | `inlineData` (`mimeType`, `data`) for any supported type | Same, `audio/*` | Same, `video/*`; per-part `videoMetadata` (fps, offsets) and `mediaProcessing` | Same; PDF and text types | `fileData` (`fileUri` required, `mimeType` optional): Files URI, `gs://`, public HTTPS ≤15 MB (not Gemini 2.0), YouTube |
| Interactions v1 | `image`: png, jpeg, webp, heic, heif, gif, bmp, tiff; `resolution` | `audio`: 13 MIME types, `channels`, `sample_rate` | **None** (v1beta has `video`) | `document`: `application/pdf`, `text/csv` | `data` or `uri`; `mime_type` optional |

Gemini inline data is limited to 100 MB per request (50 MB for PDFs). The video guide also says inline requests should stay under 20 MB.

Chat-compatible extensions:

| Shape | Accepted by | Extras |
|---|---|---|
| `{"type":"video_url","video_url":{"url":…}}`, URL or data URL | OpenRouter, DashScope, Zhipu, vLLM, ModelBest (MiniCPM-V 4.5/4.6, MiniCPM-o 4.5) | OpenRouter `video_url.processing`; DashScope `fps`, `min_pixels`, `max_pixels`; vLLM `uuid` |
| `{"type":"video","video":[frame URLs]}` | DashScope | Frame list, not a video file |
| `{"type":"audio_url","audio_url":{"url":…}}` | vLLM | — |
| `input_audio.data` as URL or data URL | DashScope | Not OpenAI's base64 + `format` form |
| `input_audio` formats beyond wav/mp3 | OpenRouter (`aiff`, `aac`, `ogg`, `flac`, `m4a`, `pcm16`, `pcm24`) | Provider-dependent |
| `{"type":"input_video","video_url":…}` | OpenRouter Responses user messages | `processing` |

Anthropic Messages has no audio or video carrier. OpenRouter's Messages endpoint states it has no video input.

## Tool-result media

| Protocol | Carrier |
|---|---|
| OpenAI Chat | None. `role: tool` content is text parts only |
| OpenAI Responses | `function_call_output.output` array: `input_text`, `input_image`, `input_file`. Open Responses adds `input_video` here (not in user messages) |
| Anthropic Messages | `tool_result.content`: `text`, `image`, `document`, `search_result` |
| Gemini generateContent | `functionResponse.parts[].inlineData` only (no `fileData`); Gemini 3 models. The description refers to `$ref` and `inline_data.display_name`, but `FunctionResponseBlob` has no `displayName` field |
| Interactions v1 | `function_result.result` array: `text` and `image` only |

## Provider file APIs

| Provider | Scope | Retention | Size | Reference form |
|---|---|---|---|---|
| Anthropic (GA, no beta header) | Workspace: any key in the workspace can read any file. Docs: "Never accept `file_id` values from end users" | Until deleted, or `expires_in_seconds` 1 h–90 d | 500 MB per file, 1 TB per org | `source: {type: file, file_id}` on `image` and `document`; `container_upload` |
| Gemini | Project | 48 hours; `gs://` registration up to 30 days | 2 GB per file, 20 GB per project | `fileData.fileUri` (generateContent), `uri` (Interactions); file must be `ACTIVE` |
| OpenAI | Project (storage limits are per project; access rules are not stated) | `expires_after` seconds from creation; otherwise none | 512 MB per file, 2.5 TB per project | `file_id` on `input_image`, `input_file` and Chat `file` |

A Provider file reference names data owned by the credential's workspace or project. It is credential-bound state, not a portable value.

## Generated media

| Protocol | Output carrier |
|---|---|
| OpenAI Chat | `message.audio` (already in the IR as generated audio) |
| OpenAI Responses | `image_generation_call` items (a Provider tool), no image message part |
| Anthropic Messages | None; Claude does not generate images |
| Gemini generateContent | `responseModalities` `TEXT`/`IMAGE`/`AUDIO`; candidate parts carry `inlineData`. The final part, including `inlineData`, may carry a `thoughtSignature` that the API does not enforce |
| Interactions v1 | `model_output` content `image`/`audio`/`document`; streamed as `image`/`audio` deltas; `response_format` for image, audio and video output |

None of the downstream protocols has a standard carrier for generated images in an assistant message.

## Client behavior

| Client | Sends |
|---|---|
| pi | Images only, always inline base64. Messages: `image` in user and `tool_result`. Responses: `input_image` with `detail: "auto"` in user and `function_call_output`. Chat: `image_url` in user; tool-result images become a following user message (client-side). Gemini 3: `functionResponse.parts` without `displayName` |
| opencode | Image, audio, video and PDF when the model declares the modality. Chat: `image_url` and inline PDF `file`; other media fail in the client. Responses: non-image media become `input_file` with an `audio/*` or `video/*` data URL; video in tool results becomes `input_video`. Messages: `image` and `document` |

## Consequences for MorphieCore

1. Video needs its own resource kind. A Chat `video_url` URL has no media type, so it cannot be a File.
2. A File whose inline type is `video/*` (opencode over Responses) stays a File; targets that dispatch by MIME may still carry it.
3. URL sources need no IR media type: Gemini `fileData.mimeType` and Interactions `mime_type` are optional.
4. Client-supplied Provider file IDs and file URIs are rejected. Forwarding one would read the gateway credential's workspace or project.
5. Without a gateway file service, large media are limited to inline data within the gateway body and IR budgets, or public URLs.
6. Detail, resolution, filename, title, fps and similar per-use options do not translate across protocols. Targets without the option drop it through a named rule; inbound options with no IR slot that change visible content are rejected.
7. Generated media stays out of scope until a downstream carrier exists.

## Open questions

Each needs one bounded real call before it is relied on:

1. Gemini `fileData` with a public HTTPS URL and no `mimeType`.
2. Interactions v1 `uri` with a public HTTPS URL or a Files API URI.
3. Gemini 3 `functionResponse.parts` without `displayName` or `$ref`.
