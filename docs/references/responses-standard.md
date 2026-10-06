# Responses 标准语义基线

- 来源/版本：2026-09-23 官方 Create、streaming events、reasoning、Structured Outputs 与 WebSocket 文档；官方 Python SDK `3.19.0` 源码快照。精确 URL、commit 和许可见[固定上游来源](upstream-sync.md)。
- 范围：Generation 相关 create/input/output/event、状态与工具；不是全部 OpenAI 产品 API 的复制。
- 证据边界：公开 schema 与客户端类型，不证明账号、模型或第三方 Provider 执行能力；SDK 字段存在不自动解决文档歧义。
- 重核：官方 schema、SDK 类型、model-dependent 默认值或 transport 版本变化。

## 1. “标准”的具体含义

此处指 OpenAI 官方 Responses API 的固定公开快照，不是 MorphieCore 共享 IR 的上限或跨厂商通用标准。Open Responses 是独立开放规范；Codex backend 行为是产品 profile；二者不能覆盖或默默补写 OpenAI 标准。其他协议参照与本地语义决策由[设计基线](../architecture/semantic-ir.md)分别处理，不能为迁就 Chat 而削减已表达语义。

协议目标允许分阶段实现，但未实现字段必须标成实现缺口或 profile 拒绝，不能因 codec 缺失而抹掉相应语义。Responses 已公开的 `phase`、`configuration_update`、hosted tools 不是 Codex 私有字段；它们在共享 IR 中保持唯一 typed owner，而非形成第二套 OpenAI 语义。

## 2. Create request 的完整域

| 域 | 标准成员/关系 | 内部所有权要求 |
|---|---|---|
| 模型绑定 | `model` | 请求绑定上下文；公开标签不是上游 URL 或 credential |
| 指令与历史 | `instructions`、string / ordered `input[]` | 任务语义；保留 instruction authority、位置、message phase 与 item status |
| 生成控制 | `temperature`、`top_p`、`max_output_tokens`、`truncation` | typed 控制；输出 token 上限包含 reasoning，不能仅按可见文本解释 |
| 文本输出 | `text.format`、verbosity、`top_logprobs`、对应 `include` | text / JSON mode / schema 不混同；logprobs include 和 top-k 控制分别表达 |
| Reasoning | effort、summary、context、mode；deprecated `generate_summary` | 控制与 readable content、opaque replay 分离；允许值与目标模型支持分开 |
| 工具 | `tools`、`tool_choice`、`parallel_tool_calls`、`max_tool_calls` | 声明/选择/调用限制；`max_tool_calls` 是内建工具总调用上限，不是普通 client function loop 预算 |
| 状态 | `previous_response_id`、`conversation`、`store`、`background` | 标准状态意图与引用；不能永久建模为 `Presence<()>`；状态解析/存储不在纯 codec |
| 模板、上下文、治理 | `prompt`、`context_management`、`moderation` | 独立 typed 子域；模板不是 instructions 的字符串别名 |
| Cache/service/safety | `metadata`、service tier、safety identifier、user、prompt cache key/options（含 prewarm）/retention | 标准请求上下文；请求 hint 与响应 effective/reported fact 分开 |
| 交付 | `stream`、`stream_options.include_obfuscation` | delivery，不混入 conversation；WS 有独立创建与控制 envelope |

`conversation` 与 `previous_response_id` 互斥。使用 previous response 续轮时，旧 request 的 `instructions` 不自动继承到下一 request。无状态完整历史、服务端 conversation 和 response reference 是不同模式。

## 3. Ordered item 与内容

| 家族 | 必须保留的语义 |
|---|---|
| Message | role、content 顺序、id/status；assistant `phase: commentary | final_answer` 与 status 独立 |
| Input content | `input_text`、`input_image`、`input_file`；source、detail、文件名与 cache breakpoint，见[媒体基线](multimodal-and-resources.md) |
| Output content | `output_text`、refusal；annotations/citations、logprobs 与原正文的依赖 |
| Reasoning | summary 与 readable reasoning text；encrypted content 单独承载并绑定 owner/origin |
| Function/custom call/result | item ID、call ID、name/namespace、caller、原始 arguments/input、文本或内容数组 result、状态；不得互换 ID |
| Hosted / execution items | search、computer、code interpreter、image generation、MCP approval/call/list、shell、apply patch；专有输入/结果/进度不是普通 function 的别名 |
| 动态工具与编程调用 | namespace/tool search、additional tools、program/program_output、async/deferred caller 关系；按固定 union 分别准入 |
| 历史与状态变更 | item reference、compaction、compaction trigger、`configuration_update`；有序配置更新影响后续响应，不是普通 message |

`configuration_update` 当前 SDK 类型只声明 reasoning effort 更新。不能因有扩展入口便允许任意执行参数写入配置项。program replay fingerprint 等 opaque 数据不能被普通代码重建。

Input/output union 不完全相同，request 允许的简写也不能放宽完整 response 的必填性。独立构造 IR 可以显式分配新的 wire identity，但收到的上游 message 缺少必填 id/status 时不能靠自动补全掩盖错误。

`output_text`、`parsed`、`parsed_arguments` 等 convenience/derived view 与权威 output/arguments 分开；具体是否出现在 SDK serialization 由固定 SDK 核对。不得让派生视图覆盖正文或调用参数。

## 4. Tools 与 Schema

function JSON Schema、custom text/grammar、标准 hosted tool 是独立分支。标准工具的 SDK union 涵盖 function、custom、namespace、tool search、web/file search、computer、MCP、code interpreter、programmatic calling、image generation、local shell/shell、apply patch；类型存在不授权 Gateway 运行它们。

Schema baseline 是 **OpenAI Structured Outputs 文档化子集**，不是随意指定某个完整 Draft 后宣称兼容。最少分开：

1. 原始 JSON 结构有效性及有界解析；
2. schema 关键字/子结构和 reference 图；
3. strict 规则与 model/profile 额外限制；
4. 模型返回内容的 schema adherence（这是另外一层验证）。

固定基线要求 strict 根为 object 且不能在根使用 `anyOf`、对象明确 `additionalProperties:false`、字段全部 required；可用 nullable value 表达语义上的可空字段。支持 `$defs`、本地 `$ref` 与递归 schema，不能靠无限展开实现验证。输出遵循 schema key 顺序，因此 schema 的属性顺序不能默认当作无关表示而排序。

该来源基线的限制为总计 5000 个 object properties、10 层 nesting、相关 schema strings 合计 120000 characters、总计 1000 enum values；这些是有日期的 API profile 限制，不是所有任务 IR 的永久常量。部分 string/number/array 限制对 fine-tuned 模型不同。不支持的 composition 关键字如 `allOf`、`not`、`if/then/else` 需按选定标准 profile 拒绝，不默默删除。

function strict 的省略默认与显式 false/true、response format strict 的规则分开；任何 normalization 都需要命名与可验证前提。core 不为了“严格”擅自改变用户 schema。

## 5. Static response 与事件

响应保存 id、model、created/completed time、output 顺序、status、error/incomplete details、reported usage 和 settings/execution echoes。请求 service tier/reasoning context 与实际返回值可能不同，不能直接复制 request 冒充 response facts。

`queued/in_progress/completed/incomplete/failed/cancelled` 是 response 状态；不意味着每个状态都有同名标准 SSE event。固定 SDK 事件 union 含 `response.queued`（携带完整 response snapshot 与 sequence_number）而没有 `response.cancelled` 事件；本地兼容准入归 [Responses profile](../architecture/responses-text-profile.md#control-message-and-annotation-admission-details)，不改变标准事件名。

事件家族包括 response 生命周期、output item、content part、text/refusal、reasoning summary/text、function/custom input、annotations、工具执行进度/结果、audio/transcript、compaction progress、shell command/output 等。必须按 event 分支验证 required/nullable 字段、sequence、身份和 snapshot，而不是对全部 event 使用宽松字段超集。

- 完整 `Response` 的 `output` 是必填数组，显式空数组不等于缺省；固定 SDK 的标准 typed SSE events（如 `ResponseCreatedEvent`）要求 `sequence_number`。顺序校验不能代替 presence 校验，低层简写也不能放宽完整 snapshot。
- `output_item.done` 不等于 response terminal。
- encrypted reasoning 的最终可回放值来自 item done；item added 的值可能未完成。
- 当前官方说明无状态 reasoning 默认可返回 encrypted content，legacy include 仍被接受；不要将 include 存在作为唯一 replay ownership 条件。
- 顶层 `error`、response.failed、非 2xx、解析错误、EOF 和取消分别处理。
- 当前 content-part schema 可出现 reasoning text，不能把某个 SDK fixture 的事件组合当成唯一标准语法。

### 与 Chat 的纯文本交叉证据

同一固定 SDK 的 Chat `CompletionUsage` 在 `prompt_tokens_details` 定义 `cached_tokens`、`cache_write_tokens`、`text_tokens`，在 completion details 定义 reasoning、text 与 prediction token 细分；字段可选不意味着收到时可以丢弃。cache-write 并非 Responses 独有。Chat choice 也有 `content_filter` 终态，与 refusal 内容和输出长度上限是不同事实。具体来源链接见[上游同步](upstream-sync.md)，本地是否映射由[Chat profile](../architecture/chat-text-profile.md)说明。

## 6. Transport 与资源服务

HTTP JSON、HTTP SSE、Responses WebSocket、Realtime 是不同 transport/operation 合同，不能从单 response reducer 推定双向交互。WebSocket/Realtime 当前只保留[官方来源导航](openai/README.md#deferred-operations)，详细状态机、并发和后继关系待相应阶段定稿，不作为请求型主线前置。

retrieve/delete/cancel/input-items、conversation、compaction、文件与 container 服务需要独立资源/执行合同，不自动由 create codec 提供；也不能因为当前 runtime 未实现就从标准目标中删除其引用语义。
