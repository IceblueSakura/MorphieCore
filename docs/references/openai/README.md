# OpenAI operation 来源导航

本页只定位官方 operation 与既有来源日期，不复制字段表、动态模型能力、实现清单或测试场景。Responses/Chat 的固定标准与 SDK 版本见[upstream-sync](../upstream-sync.md)，标准语义要点见[Responses baseline](../responses-standard.md)。API 优先级与可接受损失由[Semantic Model](../../architecture/semantic-ir.md)和[投影合同](../../architecture/protocol-and-lowering.md)决定。

## Adoption

- 方法、路径、请求编码、响应 transport、错误/终态、资源身份及权限需按具体 operation 固定；JSON、multipart、binary、SSE 和双向会话不是同一种能力。
- 网页日期是既有来源定位，不是当前在线版本或再次核验；较早资料不覆盖固定 codec 基线。冲突先核对，不拼接不同日期的字段或事件。SDK 是消费者/类型交叉证据，不替代官方 wire 合同。
- [OpenAPI repository](https://github.com/openai/openai-openapi)与[API Reference](https://developers.openai.com/api/reference)提供总入口。既有机器资料基线为 2026-07-18、OpenAPI 3.1.0 / info.version 2.3.0；既有目录核对日期为 2026-08-10。本地编辑不更新这些日期。
- 同一模态不代表同一任务：Responses hosted image tool、独立 Images operation、图像输入互不推定；Embedding vector 不是 Vector Store 资源。Voice 输入/输出不自动采用 custom voice 或 Realtime。
- 标准接口范围见[计划](../../implementation-plans/next-goal.md)。采用新 operation 前固定 schema/SDK/profile、许可、资源与消费者边界；存在官方 endpoint 不授予本地实现、账号资格或真实调用。
- 独立语义/字节/消费者/外部执行验证只由[验收基线](../conformance-baseline.md)维护。未来枚举不静默丢弃或任意透传；缺口按语义 owner 与目标合同处理。

## Embeddings

[Create embeddings](embeddings-create.md)保留向量任务的来源、输入关联、数值/编码与不可跨模型等价的必要约束。公开目标为独立标准 `/v1/embeddings`，不是 Responses 的私有 vector item。

## Responses

| Operation / 来源主题 | 官方入口 | 既有来源日期 |
|---|---|---|
| Responses Function tools | [Function calling](https://platform.openai.com/docs/guides/function-calling)、[Create a response](https://developers.openai.com/api/reference/resources/responses/methods/create) | 2026-08-03 |
| Responses 非流式响应 | [Create a response](https://developers.openai.com/api/reference/resources/responses/methods/create) | 2026-08-03 |
| Responses Create JSON 请求 | [Create a response](https://developers.openai.com/api/reference/resources/responses/methods/create)、[Reasoning models](https://developers.openai.com/api/docs/guides/reasoning) | 2026-08-03 |
| Responses resource lifecycle | [Responses API](https://developers.openai.com/api/reference/resources/responses)、[Delete a response](https://developers.openai.com/api/reference/resources/responses/methods/delete)、[Background mode](https://developers.openai.com/api/docs/guides/background) | 2026-08-10 |
| Responses continuation 与 state ownership | [Conversation state](https://developers.openai.com/api/docs/guides/conversation-state)、[Conversations API](https://developers.openai.com/api/reference/resources/conversations)、[Create a response](https://developers.openai.com/api/reference/resources/responses/methods/create) | 2026-08-10 |
| Responses typed SSE | [Streaming responses](https://developers.openai.com/api/docs/guides/streaming-responses)、[Streaming events reference](https://developers.openai.com/api/reference/resources/responses/streaming-events) | 2026-08-11 |
| Responses Structured output | [Structured Outputs](https://platform.openai.com/docs/guides/structured-outputs)、[Create a response](https://developers.openai.com/api/reference/resources/responses/methods/create) | 2026-08-03 |

Input/output/resource/stream 合同分别固定；SDK `output_text` 是派生 view，状态名也不推出同名 SSE event。必要标准区别以[固定标准基线](../responses-standard.md)为准。

## SIWC ChatGPT plan usage

公开 SIWC 的官方来源、条款、动态 client、身份/permission、renewal 与受限公共 Responses 合同集中在 [SIWC 参考](../siwc-login.md)。同一公共 API origin 不使 SIWC token 与 Platform API key 的 operation/参数准入相同；该参考也不升级本页既有标准/schema/SDK 日期或基线。

## Chat Completions

| Operation / 来源主题 | 官方入口 | 既有来源日期 |
|---|---|---|
| Chat Completions Function tools | [Function calling](https://platform.openai.com/docs/guides/function-calling)、[Create chat completion](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create) | 2026-08-03 |
| Chat Completions 非流式响应 | [Create chat completion](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create) | 2026-08-03 |
| Chat Completions JSON 请求 | [Create chat completion](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create) | 2026-08-03 |
| Stored Chat Completions resource | [Chat Completions API](https://developers.openai.com/api/reference/resources/chat/subresources/completions)、[Retrieve](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/retrieve)、[Update](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/update)、[Delete](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/delete)、[List](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/list)、[Messages](https://developers.openai.com/api/reference/resources/chat/subresources/completions/subresources/messages/methods/list) | 2026-08-10 |
| Chat Completions SSE | [Chat API reference](https://developers.openai.com/api/reference/resources/chat)、[Streaming responses](https://developers.openai.com/api/docs/guides/streaming-responses) | 2026-08-11 |
| Chat Completions Structured output | [Structured Outputs](https://platform.openai.com/docs/guides/structured-outputs)、[Create chat completion](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create) | 2026-08-03 |

## Image

独立静态 GPT Image generation 的公开合同为 [Create image](https://developers.openai.com/api/reference/resources/images/methods/generate)。类型与 nullable/presence 交叉依据固定 [SDK image_generate_params.py](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/image_generate_params.py)、[images_response.py](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/images_response.py) 与 [image.py](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/image.py)；版本与 Apache-2.0 许可归 [upstream-sync](../upstream-sync.md)。仅采用 GPT Image JSON 请求/响应分支，不采用 SDK 中的旧模型缺省或 DALL·E 专属选项；不从事件 union 推定流式准入。最小准入及拒绝边界归 [codec](../../../src/protocol/openai/images.rs) 和 [OpenAPI](../../openapi.json)，不复制动态模型库存。

| Operation / 来源主题 | 官方入口 | 既有来源日期 |
|---|---|---|
| Chat Completions 图片输入 | [Images and vision](https://developers.openai.com/api/docs/guides/images-vision)、[Create chat completion](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create) | 2026-08-04 |
| Images Edits 与 Variations | [Image generation](https://developers.openai.com/api/docs/guides/image-generation)、[Images API](https://developers.openai.com/api/reference/resources/images)、[Create image variation](https://developers.openai.com/api/reference/resources/images/methods/create_variation) | 2026-08-04 |
| Images Generations | [Image generation](https://developers.openai.com/api/docs/guides/image-generation)、[Images API](https://developers.openai.com/api/reference/resources/images) | 2026-08-22 |
| Responses hosted image generation | [Image generation](https://developers.openai.com/api/docs/guides/image-generation)、[Create a response](https://developers.openai.com/api/reference/resources/responses/methods/create)、[Responses streaming events](https://platform.openai.com/docs/api-reference/responses-streaming) | 2026-08-03 / 2026-08-04 |
| Responses 图片输入 | [Images and vision](https://developers.openai.com/api/docs/guides/images-vision)、[Create a response](https://developers.openai.com/api/reference/resources/responses/methods/create) | 2026-08-04 |

内容值、资源来源、用途与执行服务的区别见[媒体与资源](../multimodal-and-resources.md)，不从一个 operation 推断同族全部能力。

## Audio

| Operation / 来源主题 | 官方入口 | 既有来源日期 |
|---|---|---|
| Chat Completions 音频输入与输出 | [Audio and speech](https://developers.openai.com/api/docs/guides/audio)、[Create chat completion](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create) | 2026-08-08 |
| 自定义声音与 consent | [Create voice consent](https://developers.openai.com/api/reference/resources/audio/subresources/voice_consents/methods/create)、[List voice consents](https://developers.openai.com/api/reference/resources/audio/subresources/voice_consents/methods/list)、[Create voice](https://developers.openai.com/api/reference/resources/audio/subresources/voices/methods/create) | 2026-08-10 |
| Audio Speech | [Create speech](https://developers.openai.com/api/reference/resources/audio/subresources/speech/methods/create)、[Text to speech](https://developers.openai.com/api/docs/guides/text-to-speech) | 2026-08-10 |
| Audio Transcriptions | [Create transcription](https://developers.openai.com/api/reference/resources/audio/subresources/transcriptions/methods/create)、[Speech to text](https://developers.openai.com/api/docs/guides/speech-to-text) | 2026-08-08 |
| Audio Translations | [Create translation](https://developers.openai.com/api/reference/resources/audio/subresources/translations/methods/create)、[Speech to text](https://developers.openai.com/api/docs/guides/speech-to-text) | 2026-08-08 |

内容值、资源来源、用途与执行服务的区别见[媒体与资源](../multimodal-and-resources.md)，不从一个 operation 推断同族全部能力。

## Files

| Operation / 来源主题 | 官方入口 | 既有来源日期 |
|---|---|---|
| Chat Completions 文件输入 | [File inputs](https://developers.openai.com/api/docs/guides/file-inputs)、[Create chat completion](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create) | 2026-08-21 |
| Files content download | [Files API](https://developers.openai.com/api/reference/resources/files) | 2026-08-04 |
| Files Create | [Create file](https://developers.openai.com/api/reference/resources/files/methods/create)、[Files API](https://developers.openai.com/api/reference/resources/files) | 2026-08-04 |
| Files metadata、list 与 delete | [Files API](https://developers.openai.com/api/reference/resources/files) | 2026-08-04 |
| Responses File Search | [File search](https://developers.openai.com/api/docs/guides/tools-file-search)、[Create a response](https://developers.openai.com/api/reference/resources/responses/methods/create) | 2026-08-04 |
| Responses 文件输入 | [File inputs](https://developers.openai.com/api/docs/guides/file-inputs)、[Create a response](https://developers.openai.com/api/reference/resources/responses/methods/create) | 2026-08-21 |
| Uploads transaction | [Uploads API](https://developers.openai.com/api/reference/resources/uploads) | 2026-08-04 |
| Vector Stores | [Vector Stores API](https://developers.openai.com/api/reference/resources/vector_stores) | 2026-08-04 |

内容值、资源来源、用途与执行服务的区别见[媒体与资源](../multimodal-and-resources.md)，不从一个 operation 推断同族全部能力。

## Deferred operations

| Operation / 来源主题 | 官方入口 | 既有来源日期 |
|---|---|---|
| Moderations Create | [Create moderation](https://developers.openai.com/api/reference/resources/moderations/methods/create)、[Moderation guide](https://developers.openai.com/api/docs/guides/moderation) | 2026-08-10 |
| Realtime HTTP control plane | [Create client secret](https://developers.openai.com/api/reference/resources/realtime/subresources/client_secrets/methods/create)、[Create translation client secret](https://developers.openai.com/api/reference/resources/realtime/subresources/translations/subresources/client_secrets/methods/create)、[Realtime WebRTC](https://developers.openai.com/api/docs/guides/realtime-webrtc)、[Realtime translation](https://developers.openai.com/api/docs/guides/realtime-translation)、[Realtime SIP](https://developers.openai.com/api/docs/guides/realtime-sip)、[Realtime calls](https://developers.openai.com/api/reference/resources/realtime/subresources/calls/methods/accept) | 2026-08-10 |
| Realtime 双向 transport | [Realtime](https://developers.openai.com/api/docs/guides/realtime)、[WebRTC](https://developers.openai.com/api/docs/guides/realtime-webrtc)、[WebSocket](https://developers.openai.com/api/docs/guides/realtime-websocket)、[SIP](https://developers.openai.com/api/docs/guides/realtime-sip)、[Realtime translation](https://developers.openai.com/api/docs/guides/realtime-translation)、[Realtime transcription](https://developers.openai.com/api/docs/guides/realtime-transcription) | 2026-08-10 |
| Responses WebSocket | [WebSocket mode](https://developers.openai.com/api/docs/guides/websocket-mode)、[WebSocket events](https://developers.openai.com/api/reference/resources/responses/websocket-events) | 见固定基线 |
| Video characters | [Create character](https://developers.openai.com/api/reference/resources/videos/methods/create_character)、[Retrieve character](https://developers.openai.com/api/reference/resources/videos/methods/get_character)、[Video generation guide](https://developers.openai.com/api/docs/guides/video-generation) | 2026-08-10 |
| Videos Create | [Video generation](https://developers.openai.com/api/docs/guides/video-generation)、[Create video](https://developers.openai.com/api/reference/resources/videos/methods/create) | 2026-08-10 |
| 派生 Video jobs | [Video edits](https://developers.openai.com/api/reference/resources/videos/methods/edit)、[Video extensions](https://developers.openai.com/api/reference/resources/videos/methods/extend)、[Video remix](https://developers.openai.com/api/reference/resources/videos/methods/remix)、[Video generation guide](https://developers.openai.com/api/docs/guides/video-generation) | 2026-08-10 |
| Videos resource lifecycle | [Video generation](https://developers.openai.com/api/docs/guides/video-generation)、[Videos API](https://developers.openai.com/api/reference/resources/videos) | 2026-08-10 |

Realtime、WebSocket、Videos、Moderations 及其他管理/异步操作目前仅保留查询入口，不细化本地状态机或纳入当前 gate。既有 2026-08-10 来源将 Sora 2 Videos 标为 deprecated 并计划 2026-09-24 关闭；这些旧入口不证明当前可用，采用前需核对替代 API。

其他按需入口：[Models](https://developers.openai.com/api/reference/resources/models/methods/list)、[Batches](https://developers.openai.com/api/reference/resources/batches/methods/create)、[Evals](https://developers.openai.com/api/reference/resources/evals/methods/create)、[Fine-tuning](https://developers.openai.com/api/reference/resources/fine_tuning/subresources/jobs/methods/create)。它们不扩张当前产品范围。

外部资料保留发布方来源；SDK/Codex 固定提交和许可归[固定来源](../upstream-sync.md)。不复制上游实现、真实会话、媒体或敏感资源标识。实际注册/激活按 [AGENTS](../../../AGENTS.md#current-provider-model-and-compatibility-information)现场核对。
