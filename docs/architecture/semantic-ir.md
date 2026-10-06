# Semantic Model 与 IR

MorphieCore 的核心是**独立、可复用的模型交互语义体系**：承载模型 API 交互，尽量降低跨 Provider 映射的语义损失，并通过标准接口服务下游。Gateway 是其运行边界；未来自研 Agent 复用同一语义体系，而非另建 Agent/Provider IR。项目未发布，稳定目标是概念、所有权与不变量，不是当前 Rust 类型；允许在明确迁移范围内大规模破坏性重写，不维持无必要的旧结构兼容。

本页拥有总体合同；Generation 专项归[交互合同](interaction-contract.md)，映射与损失归[protocol/lowering](protocol-and-lowering.md)，当前接线归[架构](../architecture.md)。[计划](../implementation-plans/next-goal.md)决定实施顺序，[缺口](../implementation-status/generation.md)区分设计与当前实现。

## 1. 语义权威与消费者

- **Semantic Model** 定义内容、操作、identity、关系、合法状态与变化的含义；**IR** 是其 typed 值、请求、结果及适用事件的实现。
- OpenAI Responses 是主要参考，不是 IR 的字段模板或表达力上限。Google/Anthropic 可补充检验概念，但不决定当前主线；不采用协议最小交集、机械字段并集或 raw SDK DTO。
- 每个概念只有一个 owner。Typed 能力域与 scoped 值属于同一权威；它们不自动成为自定义 HTTP 字段。Fidelity 仅保存有界表示/来源/依赖记录，不保留竞争的语义正文。
- Gateway 与未来 Agent 都经过 validation、requirements、目标投影与执行边界。Agent 可通过类型化库接口使用语义能力，不必先编码 Responses 再解码回来；这不是新增公开私有 API，也不绕过准入。
- Agent 的规划、记忆选择、工具执行、预算调度与循环策略属于 Agent runtime。IR 表达交互和执行报告，不成为任意可执行 DAG 或通用工作流引擎。

```text
标准 API 客户端 → client codec ─┐
                              ├→ 同一 Semantic / IR → 目标投影 → Provider codec
未来 Agent → 类型化语义入口 ──┘                ↕
                                      显式执行与交付边界
```

这是职责关系，不声明 Agent SDK、Embedding 或未来协议已经实现。

## 2. Task、内容、资源与协议

| 概念 | 责任与边界 |
|---|---|
| Task | 一种模型操作的请求、结果及适用事件合同；Generation 与 Embedding 共享基础值，不共享万能消息容器 |
| Content / artifact | 文本、图片、音频等输入内容与输出产物；用途、完整性和必要描述独立于 wire 位置 |
| Resource | 内容来源与可引用身份：inline、URL、issuer-bound ID；来源不抹平权限、时效或用途 |
| Protocol / operation | 请求编码、响应 envelope、事件语法及 profile 必填性；不决定共享语义所有权 |
| Delivery / execution | 交付意图与运行时实现分开；socket、已选目标、credential、attempt、retry/commit 不进入 task 数据 |

Generation 的 Agent-first Text/Image/File 与 Responses 合同不作为独立媒体 operation 的容器。请求型音频按独立 task 定稿，Embedding 也不作其前置。音频 Realtime 明确要实现，但其设计与实施后置，不预建类型/状态机，也不保证现有单 response reducer 可直接复用。具体顺序只归后续计划。

Generation 保留有序异构 items 与必要 typed 关系，Message 只是其中一种 owner。Embedding 的输入关联、向量数值、维度与结果属于独立 task，不使用 assistant role、tool loop 或虚构 SSE 终态。其他任务只在实际标准 operation 需要时定稿，不由 TaskKind 名称推定实现。

## 3. 客户端 API 目标与扩展边界

- **Generation：规范 OpenAI Responses 为主要接口。** 标准场景不得依赖私有字段；遵守所选官方 operation/schema，而非仅让 SDK 宽松解析成功。
- **Embedding：规范 OpenAI Embeddings 接口。** 与 Generation 共用语义体系和受信执行原则，不将 vector 塞进 Responses message。
- **Chat Completions：必须维护的兼容接口，允许部分语义损失。** 损失发生在有合同的目标投影，不削弱核心 IR；具体规则与不可损失边界归 [Semantic loss](protocol-and-lowering.md#semantic-loss)。不是承诺与 Responses 功能等价。
- 标准规范性、能力覆盖度和映射保真度分别判断。规范 wire 可以是有损投影；未实现分支、未批准的损失或缺失必要依赖仍须明确拒绝。
- 公开边界[不提供独立 `_openbridge` carrier](client-generation-profile.md)，不保留隐式兼容或替代字段。共享 typed 语义不因此删除；无标准载体且不属于已定稿 Chat 损失时仍拒绝。是否重建扩展在迁移完成后决定，新方案须说明标准缺口、IR owner 和消费者/回传后果。

独立 TTS 采用标准 `/v1/audio/speech` 的[有界音频分支](speech-profile.md)，语音识别采用标准 `/v1/audio/transcriptions` 的[文件上传/JSON 分支](transcription-profile.md)，独立图片生成采用标准 Images operation；其他音频端点与资源服务范围仍需另定。Responses 是 Generation 主接口，不意味着任意模态或独立任务都有 Responses carrier；没有标准载体时报告选择，不伪造字段或隐式新增端点。

## 4. IR 不足与标准载体缺口

先判断问题在哪一层：

1. **IR 无法承载**：给出最小反例、缺失概念、受影响 owner/关系，分析结构方案、合法状态、事件、迁移与验证影响，并向用户报告后定稿。不得用 adapter、fidelity 或任意 JSON 掩盖。
2. **IR 可表达、目标无位置**：进行目标可表示性判断；Chat 可采用已经定稿的有损兼容规则；独立任务只有明确限定的 [Images 计量投影](protocol-and-lowering.md#独立-images-的计量投影)与[音频附属报告投影](protocol-and-lowering.md#独立音频的附属报告投影)许可，其他情况报告并拒绝或另行选择合同，不反向删弱 IR。
3. **IR 与标准都可表达但未接线**：实现 codec、准入、执行或消费者闭环，不为局部缺口重建语义权威。

不存在统一的保真百分比或全局 best-effort 许可。区分精确映射、具名前提下的等价归一化、声明的有损投影与不可表示。媒体替换成说明文本/transcript 是内容变换，不是编码别名；标准 Base64 等保持 typed 含义的编码转换与此不同。

## 5. 内容、产物与引用

共享值应区分输入内容、工具结果、reasoning 内容与生成产物，而不是按 role 或媒体类型猜用途。Text、refusal、reasoning、结构化值、媒体和向量不能靠普通字符串互相冒充。

媒体的 bytes/encoding、MIME/format、必要描述、可选伴随文本、远端引用与 expiry 分别拥有明确含义。请求格式和 voice 是意图，不能补成输出报告；transcript 不替代音频。不得把某个 Chat 音频 envelope 的必填组合固化为全部音频产物的普遍要求。

File 是容器/资源，不等同于提取后的 text；文件名、来源、格式与处理用途须保留所选合同的含义。URL、inline data 与 issuer-bound ID 不互为可移植别名。下载、上传、redirect、转码、扫描、保留与权限解析属于显式资源/执行边界，纯 codec 不执行它们。

引用须同时表达输出 claim/owner 与来源坐标。Byte、字符、页、时间和 block 使用明确单位；源删除/替换使依赖重验或失效，重排只在合同允许时重算 wire 坐标。预算分别覆盖 encoded/decoded bytes、单资源、总状态和增量缓冲。来源导航见[媒体与资源](../references/multimodal-and-resources.md)。

## 6. Identity、presence 与变换

Local item/part/resource identity、wire ID、call ID、response/turn 身份与 stream index 分开；索引是坐标，未知身份不伪造为上游报告。分组必须显式声明，不从相邻、同 role 或名称猜测。关系只有一个权威位置，其他 views 借用派生。

Presence 逐字段定义：Absent、Null、空值、false、显式 default 不普遍等价。外层缺失不能隐藏有效子值。结构化 JSON/Schema 保留精度及合同要求的顺序；派生 parsed view 不成为可独立修改的第二正文。

| 变换 | 约束 |
|---|---|
| 插入 / 重排 | 保持 surviving identity，分配新 owner，重验 membership、顺序、prefix 和引用依赖 |
| 替换 | 使依赖旧正文、参数、资源或设置的值失效，除非有明确保持证明 |
| 删除 | 删除附着值，不从 fidelity 恢复；悬空关系需显式修复或拒绝 |
| 目标有损投影 | 只作用于投影值；声明丢失及其后果，重验剩余语义/依赖，不污染原始值或其他候选 |

合法事件 materialization 与对应静态语义一致。Partial 值、完整有效值、item 完成与 response 终态不能互推；事件失败后不可恢复成功，终态快照不能修补缺失的必要内容。

## 7. 控制、计量与缓存

- Schema 值/方言/引用、adherence 意图和目标 strict/配额分开；结构有效不等于生成结果遵循 Schema。不得删 required 或暗改约束迁就目标。当前准入归 [Schema profile](schema-profile.md)。
- Reasoning mode、effort、数值预算、显示/summary、实际可读内容与 opaque replay 分开。隐藏摘要不证明未推理，也不解除回放依赖；详细交互规则归[交互合同](interaction-contract.md)。
- Usage 的 scope、basis、单位、重叠/互斥关系、最终性和缺省须明确。未知不补零，累计快照不相加，命名派生 view 借用唯一报告，不另存可修改 total；降级后的缺省也不得冒充上游未报告。
- Provider 原生缓存意图与命中事实分开；前缀策略、亲和 hint、远端资源引用不互为别名。证明只检验已声明依赖，不证明 wire 字节相等、Provider 命中或收益。Gateway 不因表示 cache 就拥有回答缓存、跨请求粘性路由或 Agent 记忆。

## 8. 稳定化判据

稳定化看 selected task 的 owner、合法状态、依赖、presence、Static/Event、变换及映射能否形成独立可验收合同，不看字段数，也不等待所有 Provider/Realtime 设计完成。核心可由纯库消费者构造、检查和编辑，而无需网络、私有配置或协议 DTO。

涉及跨请求保存时另行固定版本、编码、完整性和恢复边界；当前内存 fingerprint、Rust Debug 或客户端标签不是持久化协议、issuer 认证或 Agent memory 格式。版本化公开 SDK、独立 crate 和进程部署均按实际消费需求定稿，不预建框架。

现行 profiles 与源码仍定义当前行为；设计变化不自动激活新能力。实现遵循 [current-focus](../implementation-plans/current-focus.md)和[验收方法](../references/conformance-baseline.md)，执行权限与安全边界不因兼容目标降低。
