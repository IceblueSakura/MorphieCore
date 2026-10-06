# 最小模型交互 HTTP 网关

`morphiecore` binary 与可嵌入的 `gateway::Gateway` 将共享 IR、双向 adapters、固定目标和实际 HTTP body 接通，不是完整标准实现或生产网关。配置层为 Responses 选择 Standard adapter，为 Chat 选择 MorphieCore 兼容 adapter；Provider 的具名规则不成为 Responses 下游扩展许可，选择 Standard 也不证明整个标准 union 已实现。规范客户端目标与扩展政策归[语义设计](architecture/semantic-ir.md#3-客户端-api-目标与扩展边界)，Chat 损失规则仍逐片定稿，不因方向许可自动丢字段。HTTP 决策归 [ADR 0009](architecture/decisions/0009-minimal-http-text-gateway.md)，公共接口摘要归 [OpenAPI](openapi.json)，模块接线归[架构](architecture.md)。

## 启动

启动只读取操作者指定的私有 JSON 配置与凭据目录，不读取环境 key/账户 alias、`.env`、旧 TOML 或第三方 auth cache。默认入口配置为目录内的 `gateway.json`，可用 `--config` 指定独立路径：

```sh
cargo run --locked --offline --bin morphiecore -- --credentials-dir /path/to/private-store
cargo run --locked --offline --bin morphiecore -- \
  --credentials-dir /path/to/private-store --config /path/to/private/gateway.json
```

配置结构、synthetic 示例、API key/OAuth 与 pool 操作由[凭据指南](credentials.md#gateway-access-绑定)维护；精确解析归 [bootstrap](../src/gateway/bootstrap.rs)。入口 `client_key` 不等于上游 key，listener 只允许 literal loopback，出站 proxy 必须显式受信配置，不继承环境代理。不得打印配置正文。

Pool 按编译 binding 启用候选；只有账户或 key、没有 pool，不激活推理。凭据绑定变化与并发管理的规则见[凭据指南](credentials.md)；本地可借用不证明上游授权、模型资格或额度。

仅启动不产生模型生成请求。真实登录、推理与付费测试需另行授权；默认验收只用 synthetic keys 和 loopback Provider。Ctrl-C 发起 graceful shutdown，取消在途上游并拒绝新业务请求，不证明 Provider 停算或停止计费。

## HTTP 合同

| 方法 / 路径 | 请求与交付 |
|---|---|
| `GET /v1/models` | 本实例已激活 public labels 的标准完整列表 |
| `GET /v1/models/{model}` | 与列表一致的单模型对象；未知或未激活标签返回 404 |
| `DELETE /v1/models/{model}` | 已激活模型返回 403；未知或未激活标签返回 404，不执行删除 |
| `POST /v1/chat/completions` | [单候选 Chat profile](architecture/chat-text-profile.md)；JSON 或 SSE |
| `POST /v1/responses` | [无状态 Responses profile](architecture/responses-text-profile.md)；JSON 或 SSE |
| `POST /v1/images/generations` | 显式激活的静态有序图片集合；JSON，inline 产物，图片模型须明确选择 |
| `POST /v1/audio/speech` | 显式激活的独立 TTS；严格 EOF 后交付有界二进制音频，binary 模型须明确选择 |
| `POST /v1/audio/transcriptions` | 显式激活的独立识别；有界 WAV/MP3 multipart 上传，严格 EOF 后返回标准 JSON |

- 唯一的 `Authorization: Bearer …` 在应用层 body 收集或目录查询前校验；重复或错误认证拒绝，其他 header 不替代它。除 Transcription 使用 multipart 外，生成请求使用 UTF-8 JSON Content-Type，均不接受 Content-Encoding；严格 JSON 拒绝重复 key，绑定 public model/task 后检查目标准入。Models 无请求 body 或必需查询参数。
- `model` 仅接受已激活的 public label，不带 `provider/` 前缀。顶层 `provider` 字段即使为 null 也拒绝。目标 URL/path、上游 model、auth、adapter 与 scope 都来自受信绑定；入站 headers 不透传。
- 每个 `(public model, client protocol)` 显式激活编译 Route 成员。保持 Route 和 pool 顺序，从同一最终 IR 独立预检每个固定 `(endpoint, credential)`；无兼容成员在 I/O 前失败。注册、Chat 激活与 Responses 激活不互相推定，查询方法见 [AGENTS](../AGENTS.md#current-provider-model-and-compatibility-information)。
- 仅在 Public Model 准入输出 token 控制时，将 operator 缺省上限写入最终 IR，再派生 requirements/admission/lowering；显式超限拒绝，不静默裁剪。SIWC 不支持该上游参数：省略时不补值，显式请求拒绝。本地 bytes/events/deadline 预算不证明上游停算或费用上限。响应 reported facts 不从请求补齐。
- 文本数组、工具选择/结果、概率、Schema 与 reported context 的精确接受/拒绝由上述 profiles 和 owning code 维护，不因路由存在而扩大 Public Model/Endpoint 合同。当前跨协议不可表示时仍明确拒绝；设计允许的 Chat 有损规则尚需逐片实现，不能提前按该方向丢字段换取成功。
- user 与 Responses 工具结果的 URL/inline 图片有独立准入，见[图片输入](architecture/responses-text-profile.md#user-image-input)与[工具图片结果](architecture/responses-text-profile.md#tool-image-results)。Responses 另有[标准 user inline 文件输入](architecture/responses-text-profile.md#user-inline-file-input)及[URL 文件输入](architecture/responses-text-profile.md#user-file-url-input)，仍需 public model 与 endpoint 显式文件准入；库 codec 不自动激活文件模型。不下载、解析文档、转码或放宽 body 预算；file ID、工具文件、Chat 工具图片、图片输出与资源服务不因此启用。
- HTTP envelope/item 上的独立 `_openbridge` 字段不准入，包括 null、空对象及版本化 attachment，也不输出该字段。结构化值、执行报告、message membership、progress/scoped usage 和 replay 的 typed owner 不因此删除；无标准载体的 history/目标投影明确拒绝。请求拒绝发生在上游 I/O 前；不可交付的静态输出失败，已发布 SSE 只能中止，不伪造终态或前移。普通正文、raw arguments/output 与用户 metadata 中的同名业务数据不被当成协议字段。详见[客户端边界](architecture/client-generation-profile.md)。
- Responses 拒绝非标准 `session_id`（包括 null），标准 identity/cache hints 保留各自 owner。Chat 兼容入口的 `session_id` body 扩展仍按声明的目标投影，不提供网关会话或粘性路由，不从 cache key 派生，也不透传 session headers；精确 carrier 归 [adapter request](../src/adapter/request.rs)与[cache projection](../src/protocol/cache.rs)。未声明 carrier 的 advisory cache hint 可按合同省略，行为控制与 identity/session 要求不能随之静默丢弃。
- 标准 Responses 无位置的具名 image/text 计量明细不输出，也不静默删除后继续成功；静态投影失败，已发布 SSE 中止。IR 中的实际报告仍保留，不能继承独立 Images 的计量损失许可。
- [Continuation](architecture/responses-text-profile.md#response-outcome-and-continuation)库视图不增加 HTTP 字段、执行就绪证明或自动 Agent loop。低层 CustomSections/CodexHeaders 也不等于 HTTP 接线；仅开放表中路由；状态资源、WebSocket、hosted-tool/program 执行等[缺口](implementation-status/generation.md)仍独立。

### 标准模型发现

Models 列表采用 OpenAI 的 `{"object":"list","data":[…]}`，不分页；单模型查询返回同一标准对象。目录包含实际启动激活的各 task public labels，跨协议和候选同名去重，按标签排序，不暴露未激活注册项。出现于目录仅说明有本地入口，不证明每个协议/控制、真实账户或上游推理可用。

`id` 是 public label，`object` 为 `"model"`；`created` 统一使用研发者模型发布时间，只有发布日期时按该日期 UTC 00:00 转换成 Unix 秒，不宣称精确发布时刻；`owned_by` 是模型研发者名称，不是推理服务商。未报告的可选 `shutdown_date` 省略。具体事实及来源归各 task 的 [catalog](../src/topology/catalog.rs)，不使用 OpenRouter 收录时间、Gateway 启动时间或账户 metadata 补值。

可嵌入调用方在编译各 task identity 后，通过 `CompiledTopology::with_model_metadata` 为 canonical identity 提供经过验证的 [ModelMetadata](../src/topology/model_metadata.rs)，别名共用同一来源。纯 topology/语义消费者可不提供；Gateway 要求每个已激活 identity 都有元数据，缺失时拒绝启动，不隐藏该模型。启动时有界构建完整只读视图，不读取上游目录、凭据文件或请求内刷新；数量/字节硬边界归 [Models owner](../src/gateway/models.rs)，同时受 `Limits.response_bytes` 限制。空列表编码不解除 Gateway 至少有一个业务入口的要求。

查询共享认证、sanitized errors、shutdown 与 `Cache-Control: no-store`，不采集请求 body，也不占用上游生成 permit。OpenAI 的 DELETE operation 只删除有权限的 fine-tuned 模型；本实例没有这类所有权或管理能力，因此对已激活标签返回 `403 / model_deletion_forbidden`，对未知或未激活标签返回 `404 / model_not_found`。不转发删除、改动 registry/activation 或伪造 `deleted:true`。

### 最小请求示例

`configured-public-model` 是占位符，不是已注册模型。先按查询指南替换为目标实例已激活、准入相应协议的 public label；示例不授予真实调用权限。

SIWC 的独立目标限制归 [adapter](../src/adapter/siwc.rs)：请求省略不支持的输出 token/sampling 控制，使用标准 [namespace 工具分组](architecture/responses-text-profile.md#tool-namespaces)，不能发送 flat tools 或 system message item。上游强制 SSE，静态下游仍通过有界终态聚合交付；单一 registration，不重试或 fallback。以下带输出 token 控制的示例仅适用于准入该控制的目标，不用于 SIWC。

Responses 请求发送到 `/v1/responses`，增加 `"stream":true` 请求 SSE：

```json
{"model":"configured-public-model","input":"Reply with exactly pong.","max_output_tokens":64}
```

Chat 请求发送到 `/v1/chat/completions`：

```json
{"model":"configured-public-model","messages":[{"role":"user","content":"Reply with exactly pong."}],"max_completion_tokens":64}
```

图片示例仅使用 synthetic URL，不表示上游可获取或模型已准入：

```json
{"model":"configured-public-model","input":[{"role":"user","content":[{"type":"input_text","text":"Describe the image."},{"type":"input_image","image_url":"https://example.test/synthetic.png"}]}],"max_output_tokens":64}
```

Chat 图片 part 使用 `{"type":"image_url","image_url":{"url":"https://example.test/synthetic.png"}}`。实际图片与资源限制仍由目标合同检查，不把媒体当普通字符串。

固定 SDK 的 `base_url` 指向本机 `/v1`，`api_key` 使用入口 token；不把上游 key 交给客户端。

### 独立图片生成

图片生成使用独立 [ImageGeneration task](../src/semantic/task/image_generation.rs)，不是 Responses hosted tool 或 Chat assistant message。请求承载数量及类型化尺寸、质量、背景、格式、压缩与 moderation 意图，user 属于独立身份上下文；具体准入、required/null、组合约束、预算和结果字段由 [OpenAPI](openapi.json) 与 [codec](../src/protocol/openai/images.rs)维护。结果格式、尺寸、背景、quality 和 usage 只保留上游报告，不从请求或 auto/default 补造；明确报告与请求矛盾时拒绝。Base64 验证不解析像素，不证明图像格式或生成质量。

公开数量采用本地 Exact 交付要求：只在实际有序产物数等于请求数量、全部产物合法且严格 EOF 后返回成功。OpenRouter 的数量参数属于上限合同，允许一次尝试，但其合法少图仍因不满足本地要求而整包失败；这不是上游畸形，也不宣称 OpenAI 官方保证足量。不拆单、不补图、不返回已验证前缀。逐图报告保留在 IR，标准顶层属性须与集合每项一致，不能用首图代表异构或部分缺失的报告；计量属于整次生成，不按数量分摊或复制。

嵌入方通过 `CompiledTopology::with_images` 编译明确的 Provider operation 路径、image profile、计量投影策略与单 endpoint `ImageRoute`，再由 `Gateway::new_with_images` 和 `ImageEntry` 显式激活；只允许单 API key 来源，无 retry/fallback。Binary 从[图片 catalog](../src/topology/catalog/images.rs)解析绑定，只有操作者在 `models` 中显式选定图片标签且配好对应单来源、禁 fallback 的 API key pool 才激活；省略 `models` 不自动增加图片入口。旧 `Gateway::new` 不启用图片绑定；未激活模型返回 `model_not_found`。输入标签不能选择 origin、路径或凭据，也不从 Chat/Responses 激活推定 Images 支持。

OpenRouter 的独立 Images profile 使用受信固定路径和 Provider 限制，不走 Chat image carrier；目标未声明的尺寸/格式等控制在 I/O 前拒绝，不把 aspect ratio 当作精确像素大小，也不从缺省产物格式推定可控。允许的 Provider-specific moderation 只映射到固定 Provider 的受信 carrier，客户端不能提交任意 provider options。该绑定使用[具名计量投影](architecture/protocol-and-lowering.md#独立-images-的计量投影)：IR 保留稀疏报告与费用，标准输出仅在完整可表示时提供 usage，否则省略 usage；费用不输出，严格策略仍拒绝。规则不放宽产物、错误或预算验证。

请求型例子中的模型是占位符，不代表注册或调用授权；示例控制还需目标支持，具体 profile 限制查对应 codec：

```json
{"model":"configured-image-model","prompt":"A blue square on a white background.","n":1,"stream":false,"quality":"high","background":"opaque","output_format":"jpeg","output_compression":85}
```

不支持图片编辑、URL 产物、文件服务或流式图片。图片请求不注入对话输出 token 上限；仍共享认证、并发、取消、严格 EOF、实际 body handoff 与绝对 deadline 约束。请求/响应分别受 operator、endpoint 和 codec 的硬预算限制。Images 响应有独立的 JSON 硬上限，不改变普通对话解析；`Limits.image_bytes` 对解码后的单图字节另设上限，`Limits.images_bytes` 独立限制集合累计字节，均不随数量放大，也不替代 response_bytes 或 endpoint 的整包限制。嵌入方须同时满足各层预算，不能通过调大其中一个绕过其他限制。Binary 使用默认 Limits，本片不增加私有配置格式。

### 独立语音生成

Speech 使用独立 [SpeechSynthesis task](../src/semantic/task/speech_synthesis.rs) 和[标准 Speech profile](architecture/speech-profile.md)，不复用 Chat `GeneratedAudio` 的引用/transcript/expiry 必填组合。精确字段准入、presence 和格式由 [codec](../src/protocol/openai/speech.rs) 与 [OpenAPI](openapi.json)维护。请求控制不成为输出报告，未声明格式的二进制响应保持 `application/octet-stream`。

嵌入方通过 `CompiledTopology::with_speech` 编译明确的 Provider operation、受信目标及单 endpoint `SpeechRoute`，通过 `Gateway::new_with_media` 的 `SpeechEntry` 显式激活。目标声明 voice、格式与控制准入；要求单 API key 来源、无本地 retry/fallback，以及 canonical publication metadata。`Gateway::new` / `new_with_images` 不自动启用 Speech。

Binary 从 [Speech catalog](../src/topology/catalog/speech.rs)解析绑定；只有 `models` 显式选定 Speech public label，且存在匹配的单来源、禁 fallback API key pool 才激活。省略 `models` 不自动增加 Speech，即使已有同 Provider 的对话或图片凭据。共享 pool 若为多来源或启用 fallback，不能同时拿来激活该 Speech 入口；不读取或猜测其他账户。精确模型、upstream ID、voice 与发布时间归源码，不能把任意产品名加进 `models` 就视为可用。

OpenRouter 的具名 MP3 映射与控制拒绝归 [Speech profile](architecture/speech-profile.md#openrouter-mp3-target-mapping)。标准缺省格式在上游显式编码，避免 PCM 默认值改变请求；显式 `speed: 1` 或空 instructions 也不绕过目标拒绝。标准 codec 的格式 vocabulary 不意味着这个目标全部支持。

Token Plan 使用[原生 MP3 映射](architecture/speech-profile.md#token-plan-native-mp3-mapping)，不是 OpenAI 上游 wire。Gateway 聚合有界音频 SSE，不下载 JSON 中的 URL；无编码报告时仍返回 `application/octet-stream`。完整 native 报告保留在 IR，标准二进制仅按[具名音频投影](architecture/protocol-and-lowering.md#独立音频的附属报告投影)省略附属信息。套餐 key 和按量 key 不互换，官方[工具交互式使用限制](https://help.aliyun.com/zh/model-studio/base-url)不因本地 HTTP 转换解除。

以下请求发往 `/v1/audio/speech`，模型与 voice 都是占位符，须按所选 binding 替换；不授予真实调用权限：

```json
{"model":"configured-speech-model","input":"Hello.","voice":"configured-voice","response_format":"mp3","stream_format":"audio"}
```

成功返回原始音频 bytes，而非 JSON/Base64。上游可以分块，但本入口有界收集至严格 EOF 后才发布，不承诺首字节低延迟；SDK streaming-response 只是消费方式，不证明边生成边播放。Content-Type、长度及 transport 失败检查不证明音频可解码或内容正确。

`Limits.speech_bytes`、`Limits.response_bytes`、Endpoint 和音频值硬预算共同限制完整产物；取消、绝对 deadline 和实际 handoff 复用共享交付层。不注入文本 output-token cap，不把本地 bytes/time 上限称为费用上限。SSE、自定义声音 ID、转录、Realtime、播放器、转码、文件保留和引用回放不在此入口范围。

### 独立语音识别

Transcription 使用独立 [SpeechRecognition task](../src/semantic/task/speech_recognition.rs)与[标准 JSON profile](architecture/transcription-profile.md)。`POST /v1/audio/transcriptions` 接受单个 WAV/MP3 的 multipart `file`、public `model` 和可选 `language`，输出全文 `text` 与实际可映射的 duration usage。省略格式或显式 `response_format: json`、`stream: false` 属于同一同步分支；prompt、temperature、时间戳请求、其他输出格式、URL/ID 输入及流式均在 I/O 前拒绝。

嵌入方通过 `CompiledTopology::with_transcriptions` 编译可信 operation 与单目标路由，再向 `Gateway::new_with_media` 的独立 `TranscriptionEntry` 列表显式传入标签；空列表不激活识别。Binary 从 [Transcription catalog](../src/topology/catalog/transcription.rs)取绑定，须在 `models` 明确选定，并配置同认证域的单来源、禁 fallback pool。共享 Token Plan 凭据不自动激活 Speech 或 Transcription，账户准入与实际使用仍需另行验证。

原生 ASR 使用受信固定路径与 Base64 文件数据，不访问本地路径、上传存储服务或下载 URL。Multipart 的 envelope、字段、文件和上游 encoded bytes 分别有界；`Limits.transcription_request_bytes` 不放宽其他 JSON 请求的预算。文件名/MIME 仅声明格式，不证明音频可解码；时长及实际格式校验属于上游，本地截止时间不等于远端计费上限。

返回示例仅示意标准形状，不承诺上游一定报告 usage：

```json
{"text":"Hello.","usage":{"type":"duration","seconds":1}}
```

IR 保留实际末句/词时序及 channel；不从末句假造全文时序，或从请求语言假造检测结果。标准 JSON 的附属报告省略按[具名音频投影](architecture/protocol-and-lowering.md#独立音频的附属报告投影)执行。正文、闭合、错误与取消不受损失许可影响；上游读取或报告失败时不返回成功前缀。

### 默认资源边界

默认值与可嵌入调整范围只由 [`Limits`](../src/gateway/config.rs)维护：应用层并发、请求收集、上游/下游 bytes、图片与音频 bytes、单 SSE frame/事件数、输出 tokens 与绝对 exchange deadline 分别有界，且仍受 Endpoint/codec 预算约束。满载拒绝，不排无限队列；更紧的 Endpoint timeout 优先。媒体输入不自动放宽限制，背压不暂停 deadline。

这些是应用层边界，不是全部 HTTP 连接、实际计费或生产抗负载保证。

### 错误与流式边界

应用错误使用 `{"error":{"message":…,"type":…,"param":null,"code":…}}`；HTTP framing 拒绝归 server。原始 parser/Provider 错误、上游正文、auth 状态细节、origin 与 credential locator 不回显；下游 model 保持 public label。

| HTTP | 应用层原因示例 |
|---|---|
| 400 | 无效/未准入语义、重复 JSON key、输出上限超限 |
| 401 | 缺失、无效或重复入口认证；带 `WWW-Authenticate: Bearer` |
| 403 | 无模型删除权限；不执行本地或上游删除 |
| 404 / 405 | 未开放模型或路由 / 方法不支持 |
| 408 / 413 / 415 | 请求收集超时 / body 超限 / 媒体类型或编码不支持 |
| 429 | 应用并发满或上游 rate limit |
| 502 / 504 | 上游状态、协议、投影或预算失败 / 交付超时 |
| 503 / 500 | 服务关闭或绑定凭据不可用 / 本地运行故障 |

跨 Endpoint fallback 须显式 Route 策略，同 Endpoint 换凭据须显式 pool 策略；全部候选共用一次 permit、尝试上限与绝对 deadline，无同成员重试/竞速/运行时改序。允许失败与身份边界归 [ADR 0010](architecture/decisions/0010-canonical-model-fixed-fallback.md)和[凭据 fallback](credentials.md#同-provider-fallback)：auth/权限、重定向、协议/投影、安全故障不前移，未知 scope 的 429 不授权换凭据。多成员入口仍拒绝没有 affinity carrier 的 opaque replay/加密输出请求。

首次下游 frame 发布即保守冻结前移，body handoff 才确认 commit；HTTP response 已交出后不能改状态。Late error、取消、超时、错误/缺失终态中止 body，不合成成功 `response.completed` / `[DONE]`。模型 incomplete 等合法非成功语义终态与 transport 失败分开，不作为 fallback 理由。

上游强制 SSE 与下游交付独立：JSON 有界聚合至验证终态及严格 EOF，SSE 增量交付。只有具名 profile 可接受缺失 Content-Type 的固定 SSE，显式冲突仍拒绝。每步至多消费一个 frame，未 handoff 不推进后续语义处理；严格 EOF 后释放终态，最终 handoff 后完成。编码/排队不是 commit，server handoff 也不是客户端/TCP acknowledgement。Drop、shutdown 或消费者不 poll 时，取消/deadline 仍释放上游资源。

## 操作者诊断

路由定位使用 [Public Model → Route → Endpoint 查询流程](../AGENTS.md#current-provider-model-and-compatibility-information)，不按名称前缀猜测，也不读取/打印私有配置。静态注册与运行实例激活分开报告。

入口配置的 `diagnostics` 显式启用本地 probe 元数据，不是内容日志或生产观测。父目录由操作者准备，文件必须新建；精确权限、队列、文件预算与白名单归 [diagnostics owner](../src/gateway/diagnostics.rs)。无效路径/已存在文件拒绝启动，运行时写失败或队列满丢诊断，不改变业务响应。结束时 best-effort 有界 drain；缺少记录只能记为未知。

仅认证后的 POST 且唯一 `x-morphiecore-probe-id` 符合 owner 语法才关联；无效/重复 ID 禁用观察但不改变请求准入。ID 不进入 IR、不选择上游、不透传或回显。诊断只保存封闭阶段/结果、HTTP、规范化 Retry-After、字节与时间及候选观察，不保存正文、header 原文、URL、reasoning、opaque 或凭据。

Run/attempt 归属、指标解释与真实调用预算只由[Probe 指南](probes.md)维护；诊断不触发 retry/backoff，不把 Retry-After 透传下游。嵌入方可在共享 Gateway 前调用 `with_probe_diagnostics(path)`，结束时调用 `flush_probe_diagnostics().await`。

## 嵌入与验证

`Gateway::new` 接收编译 topology、显式 entries、凭据绑定与 limits，不读取环境。`router()` 的嵌入方维护 listener 安全边界；`serve()` 自行检查 loopback 并连接 shutdown，丢弃 serve future 也取消 owned workers。跨协议 entry 仍受相同 IR/投影约束。

独立 owning-layer、Router/binary 与固定 SDK loopback 检查的职责和命令归[开发指南](development.md)。真实外部验证须按[Probe 指南](probes.md)另行授权；局部检查不证明一般 Agent、TLS/网络、缓存收益、负载或生产稳定性。运行结果不保存在本页。
