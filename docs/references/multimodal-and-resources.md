# 多模态与资源语义基线

现有 Responses 映射的来源为[固定 Create 与 SDK 类型](upstream-sync.md)，其他协议参照查[官方入口](providers/README.md)。本页解释资源边界及既有映射出处，不定义 IR 的协议上限；详细概念归[semantic IR](../architecture/semantic-ir.md)。新模态、格式、source 类型或 operation 变化时重新核对一手合同。

## 1. 已有 Responses 来源与共享语义归属

| 语义域 | 当前公开依据 | 对应 owner |
|---|---|---|
| Responses 文本输入 | input_text、cache breakpoint | 标准 content part |
| Responses 图片输入 | URL/data URL/file ID；detail 含 auto/low/high/original | 标准 image part，source 与 detail 不压成 text |
| Responses 文件输入 | file_data/file_url/file_id、filename、detail、cache breakpoint | 标准 file part；source 与处理用途分离 |
| 工具多模态结果 | function/custom result 内容数组，computer screenshot、hosted image output 等各自 schema | 按对应 result/item 建模，不统一 stringify |
| Responses 音频 | 事件目录存在 audio/transcript delta/done；固定 SDK 输入 content union 仍为 text/image/file | 已声明事件是标准证据，但不能据此猜出完整音频 create→output 契约；需单独补齐来源与 profile |
| Provider 特殊音频/视频 | 需按具体 operation 查阅[官方来源](providers/README.md)，不由 Chat envelope 推断任务 | 经固定操作合同进入 task-specific typed extension，不伪装成普通 conversation |

共享图片/文件语义不能因当前实现没有 codec 就降格为“Provider 特殊能力”。上表只定位 Responses 来源，不将音频/视频永久限定为厂商扩展；共享内容值与仍具特定 profile 含义的控制应分别归属。同样，存在 audio 事件也不等于完整 request/response 合同或任意 Responses model 支持 audio input/output。

标准 Chat citations 与生成音频的具体控制、值、流和引用边界归 [Chat media profile](../architecture/chat-media-profile.md)，不由 Responses 的事件目录推导。

## 2. Resource value 需要表达什么

资源身份、来源、格式和用途分别表达：

- 来源：inline bytes / encoded inline data、remote URL、issuer-bound file/resource ID；source 的组合合法性按具体 part schema 验证。
- 描述：media type、编码/format、filename，必要的尺寸、采样率、channel 等只在有协议依据时加入。
- 用途：user perception、tool result、reasoning artifact、生成媒体或 voice reference；共享 source 不能抹掉用途。
- 选项：image/file detail、音频任务控制、mask、局部时间信息和 streaming chunk 关系。
- 依赖：稳定 item/part/resource identity、source issuer、access/retention 和跨目标 replay 约束。

Provider 返回的 file ID、音频 ID、container ID 和签名 URL 不是可移植正文；换 profile 不能猜测替换或自动重新上传。源 annotations、cache breakpoint、signature 随 owner 生存，不能按数组下标重新附着。

<a id="citation-coordinates"></a>
### 引用坐标来源

证据标识 `resource-citations-2026-10-07`，以下条目查阅于 **2026-10-07**；不升级其他来源基线，也不声明原生 Google/Anthropic 接入。

- 固定 OpenAI SDK 的 [response_output_text.py](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses/response_output_text.py) 将 URL/container citation 的 start/end 描述为输出消息内的字符索引；file citation 与 file path 的 `index` 是**文件列表序号**。不能将后者当作输出位置或源文本范围，也不能由字段名证明 bytes、Unicode scalar 与 UTF-16 可互换。
- [Google Interactions v1 reference](https://ai.google.dev/api/interactions-api-v1) 的所选 file citation 将输出起点明确为 **bytes**，结束位置排除；可报告 URI、文件名与页码等来源信息。不能沿用 Responses 的字符计数去验证 byte offset，也不从缺失的源正文推定页数。
- [Anthropic citations](https://platform.claude.com/docs/en/build-with-claude/citations) 将被支持的 claim 附着于 text block；源字符、页与自定义内容 block 坐标分开。字符和 block 索引从 0 开始，页码从 1 开始，结束索引排除；document index 是跨消息输入文档列表中的 0-based 位置，不是资源身份。title/context 不属于可引用的 source 正文。

本地资源身份不由这些索引或 locator 推断。已报告但缺少原文/边界的坐标保留其未知验证条件，不恢复丢失的源边界；映射只采用已定稿的单位与边界规则。Google 文档采用 CC-BY-4.0 attribution；固定 SDK 的许可见[来源基线](upstream-sync.md)，不复制外部实现或真实响应。

## 3. 不由纯 codec 承担的工作

下载、DNS/redirect、MIME sniffing、音视频转码、上传、扫描、缓存和资源授权属于显式资源/执行服务。纯 codec 不因收到 URL 就发网络请求，也不以 transcript 替代音频并声称无损。

encoded 与 decoded bytes、单资源与总请求、解压/解析深度、增量 chunk、capture 与 cancellation 分别有界。Base64、文件名、URL query、原始媒体及 transcript 都可能敏感，不能把它们自动放入日志或测试资产。

## 4. Task / modality / wire 不混同

- Responses hosted image generation 属于 Generation 的 tool 生命周期；独立 Images generation/edit 是独立 task/operation。
- 标准 Audio transcription/speech 与 Realtime session 各有独立 contract。
- 特殊 Provider 用 Chat envelope 提供语音时，wire 名称不决定任务类型。
- [File input guide](https://developers.openai.com/api/docs/guides/pdf-files)说明 Base64 data URL 示例及文件处理边界；具体字段仍以[固定 SDK union](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses/response_input_file_param.py)交叉核对，不能把上游解析能力移入纯 codec。
- [Embeddings](openai/embeddings-create.md)是 vector 输出的独立标准 operation 参考；数值结果、输入关联与维度不是文本 message，也不是 [Vector Store](openai/README.md#files) 的资源管理。
- Agent-first 的一套 IR 是共享原则与值类型下的 task family，不要求把 Embedding、VoiceDesign 等塞进 Generation 或任何协议的 message union。Responses 是主要参考，不提供其他 task 的默认 envelope；缺少标准载体按[设计决策规则](../architecture/semantic-ir.md#4-ir-不足与标准载体缺口)报告。

## 5. 后续验收的最小单元

先选一个明确 source+task+profile，建立独立 request/response/event 预期，再覆盖：

1. source 分支与互斥/缺失组合、detail/presence/filename；
2. IR 插入、替换、重排、删除后资源依赖不串位；
3. 资源 reference 同来源接受、异来源拒绝；
4. bytes、encoded/decoded 预算、分片、截断与取消；
5. 静态结果与增量 materialization 一致。

URL live probe 使用 W3C 的公开 [Dummy PDF file](https://www.w3.org/WAI/ER/tests/xhtml/testfiles/resources/pdf/dummy.pdf)，来源与许可为 [W3C Document License](https://www.w3.org/copyright/document-license-2023/)。Copyright © 2023 W3C®。Probe 的文本预期来自该公开测试文档；不把其 bytes 提交为仓库 fixture，不建立网关下载入口。外部 URL 无版本不变性，采用时需独立检查内容；本地检查不证明上游实际读取同一bytes。

小型 synthetic 媒体可证明表示和资源边界，不证明 OCR、音质、语音授权、模型质量或 Provider 下载行为。特定 endpoint 或样本的表现不能上升为通用多模态规则。
