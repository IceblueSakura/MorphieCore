# 架构

MorphieCore 是以统一 IR 为核心的模型路由网关：下游接收标准客户端协议，上游连接不同 Provider 协议，中间只经过一份类型化语义表示。本页记录设计原则和已定决策；行为细节以代码与测试为准，推进顺序见[路线](roadmap.md)。

## 1. 目标协议

| 方向 | 协议 | 状态 |
|---|---|---|
| 下游 | OpenAI Responses（主接口） | 已接入 |
| 下游 | OpenAI Chat Completions（兼容接口，允许声明的损失，可用扩展字段补充语义） | 已接入 |
| 下游 | Anthropic Messages | Responses/Chat 稳定后开放 |
| 上游 | OpenAI Responses、OpenAI Chat（含各兼容方言） | 已接入 |
| 上游 | Anthropic Messages | 编解码已有，尚未接入 Gateway 执行链 |
| 上游 | Google generateContent、Google Interactions | 计划中 |

独立的 Embeddings、Images、Speech、Transcription 已有标准下游入口，当前冻结，只做重构所需的维护。

## 2. 分层与数据流

```text
下游 HTTP（鉴权、有界收集）
  → 下游 codec：wire → IR 请求
  → 校验 / requirements
  → 固定 Route 的每个候选：lowering（按目标能力投影）→ 上游 codec：IR → wire
  → transport（受信 origin、认证、超时）
  → 上游 codec：JSON / SSE → IR 响应或 StreamEvent
  → 下游 codec：IR → JSON / SSE → body 交付
```

| 模块 | 职责 | 约束 |
|---|---|---|
| `semantic` | IR 类型、校验、requirements、事件 reducer | 不依赖 protocol / provider / execution |
| `protocol` | 各协议的 wire 类型与编解码、方言规则 | 纯函数；不访问网络、凭据或 registry |
| `lowering` | 判断最终 IR 能否由目标表达，应用具名损失规则 | 纯函数；只作用于候选私有副本 |
| `adapter` | 组合协议 codec 与方言规则，生成请求/响应投影 | 规则由受信绑定选择，业务输入不能选择 |
| `topology` | Public Model → Route → Endpoint 的静态编译 | Endpoint 绑定 origin/path/协议/上游模型/凭据域 |
| `execution` | 候选计划、请求准备、增量 intake、交付状态 | 不解释业务语义 |
| `transport` | HTTP I/O 与 SSE framing | 只接收已准备好的可信请求 |
| `credential` | 文件凭据 store、API key、OAuth driver、有序池 | 凭据不进入 IR |
| `gateway` | 鉴权、准入、预算、body 发布与取消 | 只监听 loopback |

**结构目标（阶段 1）**：lowering 与 adapter 现在以 `openai::Profile` 为键，Anthropic 只是旁挂分支。目标是每个协议一个双向模块（请求/响应编解码 + 上游 SSE → `StreamEvent` + `StreamEvent` → 下游 SSE），每个上游以数据声明 `TargetCaps`（可表达的 IR 特性与适用的损失规则）；方言差异收进所属协议模块的小结构，替代全局布尔开关集合。

## 3. IR 原则

以下是长期有效的不变量：

- **有序异构 items**：Generation 历史是有序 items（指令、消息、工具调用/结果、reasoning、Provider 工具观察等），Message 只是其中一种容器。调用与结果通过 call ID 显式关联，不靠相邻位置或名称猜测。
- **身份分离**：本地 item/part ID、wire ID、call ID、流索引各自独立；未报告的身份不伪造成上游报告。
- **精确 JSON**：工具参数与 Schema 保留数值精度和键顺序；原始字节解析时拒绝重复键并限制深度、节点和字节数。
- **按需区分 presence**：在含义不同的地方区分缺省、null、空值和默认值（例如工具参数、Schema、计量）；不为每个字段机械套用。
- **reasoning 分层**：可读内容（摘要/文本）与 opaque 续轮载荷分开；effort/预算/显示是控制，不等于实际产出。
- **计量诚实**：未报告不补零，累计快照不相加，派生值不另存成上游报告。
- **终态诚实**：响应闭合、产物完整、轮次进度（等待工具结果 / 结束）分开表达；失败、截断、取消不能变成成功。
- **无运行时状态**：已选目标、凭据、重试和提交状态不进入 IR。
- **上下文模式**：当前为客户端每次提交完整历史（ClientManaged）；网关自管的限时 `previous_response_id` 历史是终期目标，届时物化为同样的完整 IR 再走同一链路。

IR 不是任一协议的 DTO，也不是字段并集。发现 IR 无法表达的概念时，先给出最小反例和结构选项，再决定实现；不在 adapter 里用私有字段绕过。

## 4. 协议适配规则

### 字段三级策略

| 级别 | 范围 | 处理 |
|---|---|---|
| 必须保真 | 指令、消息顺序、工具定义、参数、call ID 与结果关联、必要的 opaque 续轮、stop 原因与核心计量 | 目标无法表达时拒绝请求 |
| 可声明降级 | 目标不支持的采样参数、`metadata`、`service_tier`、`logprobs`、附属计量明细等 | 按具名规则从候选副本移除并记录到诊断 |
| 未知字段 | 上游响应中未登记的字段 | 忽略；已登记字段照常严格校验 |

- **上游响应**：已知字段严格（类型、取值、关系），未知字段忽略。上游新增字段不能让网关失败。
- **下游请求**：标准中能表达的字段按上表处理；无法表达的关键语义返回 400，不静默丢弃。
- 损失只作用于每个候选的私有投影，原 IR 不变；静态与流式使用同一规则。

### Opaque reasoning 续轮

所有上游签发的 opaque 值（OpenAI `encrypted_content`、Anthropic thinking `signature`、Gemini `thoughtSignature`、Interactions step signature）在下游统一使用**带标签的信封**：

```text
{版本, 上游 Provider, 上游模型, 格式, 原始载荷}
```

- 交付给下游时编码进对应载体（Responses reasoning item 的 `encrypted_content`；Chat 见下节）。
- 回传时：Provider 与模型都与本次目标一致才还原原生载荷；否则丢弃该 opaque 值，并移除因此变空的 reasoning item。
- 当前载体为 `mc1.` 加 base64url JSON，包含 Provider/上游模型的稳定指纹、原生格式和载荷；公共模型别名、凭据和入口密钥不参与兼容性指纹。裸值、损坏或未知版本信封不回传上游。
- 信封不加密、不附加 HMAC、不做本地前缀证明：来源标签只是兼容性提示，不是身份认证；标签不能选择路由、地址或凭据。签名合法性由上游签发方校验，被篡改或不匹配时由上游拒绝并作为错误返回。
- 同目标没有相应载体时仍拒绝，不把必要 opaque 当成异目标值静默丢弃。调用方显式使用的历史依赖检查、owner 关联与缓存前缀工具独立保留。
- 当前接线覆盖 reasoning item 的 Responses `encrypted_content` 与 Chat `reasoning.encrypted.data`，静态和流式相同。原生 Anthropic/Google 接入及 part/call 级签名仍按路线推进。

### Chat 扩展字段

Chat Completions 无法承载 reasoning 等语义时，参照 OpenRouter 的现有形状增加扩展字段，而不是自创格式：

- assistant 消息（`message`）与流式 `delta` 上的 `reasoning`（可读文本）与 `reasoning_details`（有序数组）。数组元素的 `type` 为 `reasoning.text`（`text` + 可选 `signature`）、`reasoning.summary`（`summary`）或 `reasoning.encrypted`（`data`），共有 `id`、`format`、`index`。
- 签名与加密载荷使用上节的信封编码；客户端须原样回传整段 `reasoning_details`，网关按上节规则还原或丢弃。
- 其他扩展字段按需逐个加入并在此登记；不恢复旧的 `_openbridge` attachment。

## 5. 执行与交付

- 启动时编译 Public Model → Route → 有序 Endpoint；请求不能创建或重排候选。每个候选都从同一不可变 IR 独立投影。
- Fallback 必须显式开启，只在首次向下游发布内容之前发生，且只针对可前移的失败（连接/传输失败、超时、5xx、本地凭据不可用）。401/403、协议/投影错误不前移。
- 首次发布下游 frame 即冻结前移；body 交出视为提交；上游严格 EOF 且最终交出后才算完成。之后的错误只能中止 body，不拼接其他 attempt，不伪造成功终态。
- 上游 SSE 与下游交付方式独立：下游要 JSON 时有界聚合到终态；要 SSE 时增量交付。
- 所有 body、frame、事件数和时间都有上界；客户端断开、超时或关闭会取消上游请求。

## 6. 缓存亲和

- 下游可选 `X-MorphieCore-Conversation-Id` 表达 Provider 无关的逻辑会话；网关在内部派生各上游的 session/cache 载体，客户端不需要了解 Provider 参数。
- 标准 `prompt_cache_key` 是独立的缓存分组意图。
- 无会话标识时，网关可用短 TTL 的前缀摘要索引尽力复用缓存分组；索引可随时丢弃，未命中只损失缓存机会，不改变请求合法性。它不是历史存储，也不参与 replay 判断。
- 隐式缓存范围仍隔离入口密钥、凭据来源与公共模型；不复用 replay 的 Provider/模型兼容性指纹。

## 7. 凭据

- 凭据只来自应用自有的文件 store；不读取环境变量中的 key，不读取第三方 auth cache。
- API key 与 OAuth（Grok、ChatGPT 订阅 SIWC）由统一 manager 管理；推理使用显式配置的有序池，按固定 identity 借用短期 access 视图。
- 普通请求不触发登录或刷新；凭据、locator 与上游 origin 不下发给客户端。

## 8. 已定决策

| 决策 | 要点 |
|---|---|
| 统一语义核心 | 一个 IR 服务 Gateway 与未来的 Agent 库调用；Agent 规划、记忆、工具执行属于调用方 |
| 独立任务 | Generation、Embedding、Images、Speech、Transcription 各有任务合同，不共用万能消息容器 |
| 纯 codec / lowering | 不访问网络、凭据、registry；编码基于最终 IR，不在编码后改 JSON |
| 能力分层 | 语义能力、wire 表示、执行属性、对下游承诺分开声明；公共能力不取候选并集 |
| 固定候选与提交前 fallback | 见第 5 节 |
| 最小 HTTP 网关 | loopback 监听、单一 Bearer 入口鉴权、入站 header 不透传 |
| 上游开放解码 | 已知字段严格、未知字段忽略 |
| Opaque 信封 | 不加密、不做本地前缀证明；同 Provider+模型还原，否则丢弃 |
| Chat 扩展 | 参照 OpenRouter 形状承载 Chat 无法表达的语义 |
| 文件凭据与有序池 | 授权生命周期与推理借用分离；无自动刷新/负载均衡 |
