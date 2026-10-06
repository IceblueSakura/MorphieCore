# Generation 交互合同

本页细化 [Semantic Model](semantic-ir.md)中的请求型 Generation，约束工具、结果、响应进度、replay 与报告；不规定新客户端 API，不是 Agent 调度器或 Realtime 设计。具体优先级归[计划](../implementation-plans/next-goal.md)，现有准入归 [Responses](responses-text-profile.md)、[Chat](chat-text-profile.md)和[客户端边界](client-generation-profile.md)。

## 表示与唯一权威

Generation request 拥有有序 history、指令和生成意图；response 拥有有序输出、操作结果与 reported facts；context、delivery 和来源/依赖记录分别归属。Message、Reasoning、Call、Result 和有合同的控制项不能压成一种 role message，也不克隆某家 steps/blocks union。

原始字符串参数、完整结构化 JSON 与尚未闭合的构造片段是不同权威形式。Raw string 的 parsed view 只能派生；structured value 不另存可独立修改的同义字符串。严格解析保留精确数字和合同要求的对象顺序，并在构造 Value 前限制 bytes、depth/nodes 与 duplicate keys。

完整字符串不证明 JSON 有效，JSON 有效不证明符合参数 Schema，符合 Schema 不授予工具执行权限。Custom/grammar input 不强行按 JSON 解释。Partial builder 完成须验证，不能用初始空对象、缺字段默认或终态 snapshot 修补截断内容。

## Response outcome and continuation

以下事实分别表达，并验证合法组合：

| 事实 | 不代表什么 |
|---|---|
| Response 生命周期闭合 | 不代表 transport EOF 正确或逻辑 turn 已结束 |
| 产物完整/截断/失败/取消 | 不代表工具执行成功；refusal 内容也不是传输故障 |
| 报告的交互进度 | 未报告不能推定完成；等待结果必须有相应 call |
| 根据最终 history 派生的 pending results | 结果齐备不证明所有依赖满足、目标可表示或下一请求获准 |
| 后继 response 关系 | 不重新打开前一个终止的 reducer，不绕过提交后禁止 fallback |

工具结果分别拥有 call 关联、结果值、execution report 和 artifact lifecycle。执行失败可以携带有效正文/结构化诊断；错误不是排除正文的另一种 payload，也不自动升级为 Generation failure。结果名称不能替代 call identity；名称、种类与已知调用冲突时拒绝。

文本、结构化值、有序媒体 parts 及其空值保持各自合同，不隐式 stringify、拼接或把结果装入 user message 后丢掉结果身份。Client-executed、upstream-executed 和 program/custom 分支不互为别名；表示并不授权执行。

Continuation 是要求/依赖而不是动作命令。当前 [pending view](../../src/semantic/task/generation/continuation.rs)与[本地后继检查](../../src/semantic/task/generation/turn.rs)只提供有界事实，不替代真实 upstream turn identity、跨请求完整性或执行授权。

## 身份、分组与依赖

- Local item/part/call reference、wire ID、call ID、stream index、response/turn/resource identity 各有范围。未知上游身份不合成成 reported fact。
- Message ownership、operation association 与共同 replay group 是不同关系；membership 只有一个权威位置，views 派生，不从相邻、role 或名称推断。
- 插入、重排、删除、替换及设置变化须维护 owner，并重验内容、成员/顺序、选定 prefix、工具/Schema、有效设置和资源依赖。能力合同选择依赖范围，业务 JSON 不提供任意 selector 或降低证明范围。
- 悬空关系须修复或拒绝，不能将旧 metadata 附到同坐标的新 owner。跨协议丢失关系只可能由明确的[有损合同](protocol-and-lowering.md#semantic-loss)处理，且必须保护实际续轮依赖；现行 profile 的拒绝不因设计许可自动解除。

所有关系只表达当前交互需要的有限依赖，不建设通用可执行图。共享方法不意味着缓存证明、replay scope 与 turn identity 可以混用。

## Typed replay 与信任

Reasoning opaque 是由上游提供、绑定于特定交互项的协议专属回放值。客户端可以保存、搬运和按协议重新序列化，但不解释、不自行改写或根据可见 reasoning 重建它；回放保留原值及协议要求的所属关系。“不可自行改写”不等于“从首次出现起永不变化”，协议规定的片段组装与最终化也不属于客户端擅自改写。

Replay attachment 拥有明确格式、唯一值、owner、partial/final、可见性与预算；绑定记录拥有可信兼容 scope 和依赖证明，不复制正文。其基础边界为：

- 原样保留的是 opaque 值及必要关联，不要求外围 JSON 的空白、键顺序或等价转义写法逐字节相同。不截断、补造或拼接独立 opaque 值；不能从 summary 生成替代值。
- 值不能移挂到另一 owner；删除 owner 或 typed 值不能从 fidelity 恢复。可见 summary 与 opaque 的依赖由具体格式规定，不预设整个 reasoning 对象的每个字段都被签名绑定；已声明的依赖约束仍须保留。
- 上游未报告、客户端丢失和目标无载体是不同事实。缺失不补造，也不一律判作错误；必要 replay 缺失时不能宣称保持同等续轮语义。Opaque-only 内容合法不等于回放条件已经满足。
- Responses encrypted content、Google thought signature 与其他 opaque 格式不能因为都是字符串而互换，也不是认证凭据、稳定内容 ID 或跨目标通行证。可见 reasoning 不能替代必要 opaque；跨协议可继续发送请求不证明原 reasoning 状态得到保持。

完成与回放分三个维度判断，不要求据此新增字段或状态机：

| 维度 | 需要区分的事实 |
|---|---|
| 值完整性 | 构造中的值与按所选格式完成的值；非空不等于 final |
| 交互闭合 | owner completed、response closed 与严格 transport EOF 分别成立，不能互推 |
| 回放条件 | 格式、目标准入、可信 scope 与必要历史依赖是否满足；满足本地条件仍不证明上游接受 |

每种格式分别规定最终化和回放条件；Responses 的 item-done 规则不外推给其他协议。`Final` 不表示永不过期、不可重新签发或真实性已验证。此处澄清不放宽当前 profile，也不要求新增严格校验；已闭合 item 的迟到信息与权威问题集中在[待决状态](../implementation-status/open-questions.md#reasoning-opaque-的闭合后权威)。

三层证明不可互推：

1. 当前进程内编辑前后的依赖一致性；
2. 客户端交付、保存、回传后的完整性；
3. issuer 对值与用途的真实性验证。

普通 hash、client label、wire ID 或内部 scope 只够其声明的用途。原样保存不证明上游仍接受；对已修改 history 重新 hash 不证明原始签发内容。若需要跨请求证明，须另定 authenticated carrier 或受信状态，不能借现有 sidecar 声称完成，也不因此预建防篡改系统。Scope 不选择上游、账号或 credential。

## 报告、控制与引用

Usage report 声明 scope、basis、unit 和计数关系。Operation、item、session 的报告不混加；delta、cumulative snapshot 与 final 不混同。累计值更新而非重复求和，缺失不补零，同一事实只有一个权威；精确派生需要命名公式及完整前提。事件不能撤回已发布报告或把未知补成计费事实。

Schema 结构/方言/引用、adherence 意图与目标 strict/配额分开。Reasoning mode、effort、预算、显示意图与 replay 分开。请求设置不是响应事实，不能回显补齐；共享时间/identity 不受某个 wire 的必填形式反向限制。

引用同时依赖输出 claim 和源资源坐标，单位必须明确；不能近似转换落在 UTF-8 中间的 offset。源编辑使引用重验，wire 索引由最终顺序投影。Configuration update 与 compaction 有作用范围/替代关系，不是普通摘要或可执行设置 patch；当前实现不得因有 union 分支而扩大执行能力。

## 客户端交付与验收

标准 Responses 与兼容 Chat 分别验收，客户端边界不提供独立私有 attachment。必要 replay/关联不能依赖 SDK 偶然保存 unknown fields；客户端丢弃字段后是否仍可安全续轮必须由对应投影合同决定，而不是由 HTTP 200 判断。

有回传要求的切片验证真实交付→保存→追加结果→回传，再检查 owner、值权威、依赖、目标准入与失败边界。IR 级工具结果齐备不是执行就绪证明。独立反例覆盖插入/替换/删除/重排、结构化精度、partial/invalid 值、错 call kind/identity、错误 scope/format、累计计量与发布后失败。

无需把完整工具/opaque/turn 场景作为每个媒体或 Embedding 切片的前置。最低 owning layer 保护不变量，检查方法归[验收基线](../references/conformance-baseline.md)；运行结果不写入合同。

## 来源

主要标准由 [OpenAI 固定基线](../references/upstream-sync.md)定位。补充概念参照：[Google Interactions v1](https://ai.google.dev/api/interactions-api-v1)、[thinking](https://ai.google.dev/gemini-api/docs/thinking)、[streaming](https://ai.google.dev/gemini-api/docs/interactions/streaming)、[stateless 示例](https://ai.google.dev/gemini-api/docs/quickstart.md.txt)，以及 [Anthropic Messages](https://platform.claude.com/docs/en/api/messages)。参考不证明原生接入，也不预定其实现优先级。

采用具体协议前固定 API/schema/SDK/profile，解决 required/optional 与事件合同差异，不拼接动态示例。Google 文档为 CC-BY-4.0、示例为 Apache-2.0；保留来源，不复制真实会话或 SDK 实现。账号管理、实时会话、工具执行和真实请求不由这些参考授权。
