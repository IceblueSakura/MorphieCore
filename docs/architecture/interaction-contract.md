# Generation 交互合同

本页细化 [Semantic Model](semantic-ir.md)中的请求型 Generation，约束工具、结果、响应进度、replay 与报告；不规定新客户端 API，不是 Agent 调度器或 Realtime 设计。具体优先级归[计划](../implementation-plans/next-goal.md)，现有准入归 [Responses](responses-text-profile.md)、[Chat](chat-text-profile.md)和[客户端边界](client-generation-profile.md)。

## 表示与唯一权威

Generation request 拥有有序 history、指令和生成意图；response 拥有有序输出、操作结果与 reported facts；context、delivery 和来源/依赖记录分别归属。Message、Reasoning、Call、Result 和有合同的控制项不能压成一种 role message，也不克隆某家 steps/blocks union。

原始字符串参数、完整结构化 JSON 与尚未闭合的构造片段是不同权威形式。Raw string 的 parsed view 只能派生；structured value 不另存可独立修改的同义字符串。严格解析保留精确数字和合同要求的对象顺序，并在构造 Value 前限制 bytes、depth/nodes 与 duplicate keys。

完整字符串不证明 JSON 有效，JSON 有效不证明符合参数 Schema，符合 Schema 不授予工具执行权限。Custom/grammar input 不强行按 JSON 解释。Partial builder 完成须验证，不能用初始空对象、缺字段默认或终态 snapshot 修补截断内容。

<a id="client-managed-context"></a>
## ClientManaged 上下文与配置

客户端拥有历史并明确选择本次使用的记录；核心从选中历史、新输入/结果和显式配置快照构造请求，不读取 session 文件、选择分支或维护第二套 AgentHistory。初次请求是空历史加新输入，不要求独立 Fresh 状态机。ServerManaged 的实施与恢复条件归[计划](../implementation-plans/next-goal.md#延期目标与恢复条件)，不以空壳变体预建。

```text
调用者选中的 typed 历史 / 已接受观察 + 配置快照 + 显式编辑或追加
  → 纯上下文构造 → 关联解析、依赖重验、预算检查
  → 最终 GenerationRequest + 变换诊断 / 未满足要求
  → 目标投影 → 现有执行边界
```

已接受观察与派生请求是不同语义对象。原观察不可被投影原地改写，最终请求只拥有一份当前 typed 值；来源、索引和变换报告不保存可以覆盖它的第二正文。纯库直接构造请求不必建立 session tree。Gateway 不因调用此能力而持有跨请求会话状态。

- 每次请求显式提供本次所需的历史、指令、工具与控制；不依赖 `previous_*_id`、conversation 或连接级增量历史。无状态历史、response storage、原生 cache 和 opaque replay 是不同机制，不以其中一个开关推导其他机制或 ZDR 保证。
- 配置与工具定义有可引用的修订。[配置 owner](../../src/semantic/task/generation/configuration.rs)以调用者显式声明的 scoped revision 关联不可变完整快照，客户端工具定义由原修订与限定 kind/namespace/name 唯一定位；没有原始关联时不从当前工具列表补猜。历史调用保留原定义关联，当前同名定义不重新解释旧参数；同修订不同内容是冲突，改变历史调用的绑定遵守新调用身份规则。修订标签不提供认证，也不要求建设全局 registry。只有触及已声明依赖的变化才使该依赖失效，不以全历史/全配置变化一律拒绝。
- 历史指令保留 authority、scope 与 phase。不能将所有中途变更无条件折叠为当前顶层指令；目标没有必要载体时按投影合同处理。原生 ConfigurationUpdate 等观察不自动修改受信请求配置。
- 显式选择、插入、删除、替换和重排返回新值并重验依赖；所有承诺返回合法请求的修改方法遵守与构造器相同的验证保证。调用者已验证过原值不免除修改后的校验。
- 外部提供的摘要是有来源的派生内容，不是被替换的上游观察，不继承其 opaque、计量或 instruction authority。裁剪不能孤立工具结果或破坏必要 replay；缺失返回诊断，不补造 `No result provided` 或自动调用模型修复。

本轮作用范围由完整当前配置与有序历史位置区分，沿用现有 message phase；不凭空创建通用指令 phase 或策略语言。[变换 owner](../../src/semantic/task/generation/context_transform.rs)提供原子显式编辑、source-ID 诊断及分阶段错误；保留的指令不能跨越其他保留项改变作用位置，同 owner 的 authority/role/phase 不能改写。外部摘要只选择连续、普通完整文本，使用新的 item/part 身份与 user authority；不把工具、refusal、媒体、opaque 或带 phase 的内容自动摘要成文本。更广摘要策略仍归调用者，并须另有受保护变换合同。

稀疏选择不是编辑源响应：原 usage、outcome、progress 留在原观察，进度只作为有来源的选择记录。Provider 后继要求由调用者/profile 显式声明并检查选定配置依赖，不从 `NeedsContinuation` 推断，也不与 pending client results 合并为执行队列。固定修订的依赖同时绑定内容，不能靠重建相同普通标签绕过检查；这些进程内检查仍不是认证或持久化许可。

Pi 的[上下文投影参考](../references/pi-provider-abstraction.md#client-managed-projection)提供历史与请求视图分离的方法，不决定本项目的 role、损失或信任规则。自动摘要、裁剪策略、分支存储、工具执行与续轮调度仍属于调用方。

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

<a id="provider-tool-observations"></a>
## Provider 工具观察与分层验证

Requester、执行责任、执行进度、结果正文与产物生命周期分别表达。执行责任来自受信定义/profile 和观察来源，不由模型参数自称；provider-executed 调用不能进入客户端结果要求或被重建为客户端待执行任务。调用被报告不证明已经执行，未报告结果不补成功或失败。已报告的取消/结果未知可以独立于正文存在，不强迫制造空 payload。

同一响应可以同时报告客户端调用 C 与 Provider 调用 S，只有 C 等待客户端结果；后续响应可以只报告 S 的结果。该结果在所选 ClientManaged 历史中解析到 S，不在新响应内伪造一次调用，也不要求 ServerManaged。需要保持相同工具配置的暂停续轮显式依赖其修订；Provider continuation 是后继请求要求，不是本地工具执行命令。

原生表示不一定有分离的 call/result block：某些工具将 action/status/result 放在同一 item，另一些把答案与引用放在独立 message。只提取实际报告的事实；不能按邻接、名称或“工具已完成”补造独立结果及因果边。结果正文、来源证据和对答案的引用各有 owner。

Provider 复合观察采用共享 Generation items 中的独立 typed 分支，不将普通 function/custom 调用与 Provider action 重写成同一参数节点。共享身份、引用、执行报告、结果值和资源基础类型；报告调用事实的观察与引用先前操作的观察分别表达，单一原生 item 不强拆成虚构 call/result。来源域标签用于关联而非认证或执行权限，纯核心构造不证明真实 Provider 执行。

具体动作内容归[Provider action owner](../../src/semantic/task/generation/provider.rs)，不是任意 JSON 参数袋。动作未报告与已报告但参数未报告分开；单查询、有序查询列表及其空值不互相归一化、去重或补猜，URL/查找模式仅是观察内容，不触发 I/O。工具名称不推导动作或执行责任。引用结果只指向原操作，不复制其动作正文；原生 sources、引用与 replay 附件须按各自 owner 另行准入，不能因动作可表达而静默丢失。选中来源与附件域区别见[动作证据](../references/upstream-sync.md#provider-actions)。

修改动作遵守调用身份规则，复用显式修订与派生关系；Provider 修订不能变成客户端调用，也不能携带原操作的进度、执行、正文或产物报告。旧结果不自动重关联；删除或显式修复悬空关系后才可构成新请求。新动作纳入内容依赖而非脱敏 Debug 的占位文本；脱敏不应使不同参数共享证明。规范事件只接收完整 typed 动作；原生分片在有相应 parser 合同之前不能作为已闭合值输入，后继报告也不修改已终止的观察。

验证保证分层，而不是放宽原有严格入口：

1. **局部观察合法性**：验证值、presence、局部 identity、生命周期和预算；允许明确未解析的外部调用引用，不抹掉已收到的合法观察。
2. **关联解析**：使用调用者提供的历史与协议引用域，返回唯一解析、缺失或歧义；未知 native ID 不伪造为已解析内部引用。
3. **最终请求/history 合法性**：检查选中记录、配置和必要依赖；未解析观察不能自动成为合法回放或执行就绪证明。
4. **目标可表示性与实例准入**：由既有 profile/lowering 与受信产品边界分别检查；核心表达力不自动激活 HTTP 分支。

同一调用的新报告可以作为后继响应中的新观察追加，不重开前一个已终止 reducer，也不擦除旧事实。静态与事件应能产生同一观察；final snapshot 不能补造缺失结果。现有工具、结果和事件源码仍有[实施缺口](../implementation-status/generation.md)，本合同不声明新分支已经接线。

## 身份、分组与依赖

- Local item/part/call reference、wire ID、call ID、stream index、response/turn/resource identity 各有范围。Native alias 包含来源、声明作用域和 ID kind；结果的引用域由协议决定，不能直接使用结果所在 response 的 scope。不同域同值 ID 可以共存；同引用域冲突或多个可匹配祖先须诊断，不选择最近一个。未知上游身份不合成成 reported fact；纯构造所需本地 scope/ID 分配由调用者明确提供，不在 decoder 隐式随机生成。
- 重排保持 surviving identity；编辑产生新修订。修改已观察调用的参数形成新的调用提案/identity，并保留派生来源，旧结果不能移挂。原观察可保留，但不覆盖当前派生值；本地修订不充当持久化编码或 issuer 认证。
- Message ownership、operation association 与共同 replay group 是不同关系。异构组可以包含文本、reasoning、call/result；不同关系可重叠。每类关系只有一个可写 owner，成员引用不复制正文，反向索引派生；声明顺序须与该关系合同一致，目标要求的连续性由目标另验，不从相邻、role 或名称推断。不能同时独立写 group membership 与调用的反向 message owner，或结果引用与第二张 ResultOf 表。
- 显式消息容器由独立的 scoped identity、role、有序 item 引用及必要的空容器位置组成，不把某段文本当成所有协议的消息容器。Chat 的 assistant 文本/function 归属是其窄形状；Anthropic assistant 的 thinking/text/tool-use/Provider 观察及 user 的 tool-result/text 可组成异构容器。内容仍只有原 typed owner，role 必须相容；容器既不产生 result-of 边，也不授予执行责任。相邻但未分组的 Responses items / Interactions steps 不补来源分组。
- 消息容器成员不重叠，ReplayGroup 可重叠；两者不可互换。保留成员相对顺序，编辑删除成员后重验，空容器用显式身份锚点或末尾位置表示，不用数组坐标附到新值。必要依赖绑定容器声明及其所选内容；公共 wire 的边界损失和目标重组只按[具名投影](protocol-and-lowering.md#message-envelope-projection)发生，不回写原观察。
- 同一 item 可以被多个显式 ReplayGroup 引用；各组有独立 identity、成员顺序和依赖。共享成员的相关编辑分别重验所有受影响组，不合并 opaque，也不以一组通过替代另一组通过。成员顺序与最终 items 的相对顺序一致，不要求核心成员连续；具体格式/目标可收紧结构。Message ownership 仍只有一个 assistant owner，不从 replay membership 推导 message ownership 或执行权限。
- 插入、重排、删除、替换及设置变化须维护 owner，并重验内容、成员/顺序、选定 prefix、工具/Schema、有效设置和资源依赖。能力合同选择依赖范围，业务 JSON 不提供任意 selector 或降低证明范围。
- 悬空关系须修复或拒绝，不能将旧 metadata 附到同坐标的新 owner。跨协议丢失关系只可能由明确的[有损合同](protocol-and-lowering.md#semantic-loss)处理，且必须保护实际续轮依赖；现行 profile 的拒绝不因设计许可自动解除。

所有关系只表达当前交互需要的有限依赖，不建设通用可执行图。共享方法不意味着缓存证明、replay scope 与 turn identity 可以混用。

## Typed replay 与信任

Reasoning opaque 是由上游提供、绑定于特定交互项的协议专属回放值。客户端可以保存、搬运和按协议重新序列化，但不解释、不自行改写或根据可见 reasoning 重建它；回放保留原值及协议要求的所属关系。“不可自行改写”不等于“从首次出现起永不变化”，协议规定的片段组装与最终化也不属于客户端擅自改写。

Replay attachment 拥有明确格式、唯一值、owner、partial/final、可见性与预算；绑定记录拥有可信兼容 scope 和依赖证明，不复制正文。其基础边界为：

附件采用 **owner-local typed 载荷**：只归实际承载它的 item/part，组只有存在独立载荷证据时才拥有值，否则只表达 replay 依赖。没有独立的 request/response 可写附件表；统一查询/绑定键只定位节点，不成为第二个载荷位置。非 reasoning 的调用、Provider 观察与文本/图片 part 分别保留各自 owner，不伪造 reasoning item。格式与节点组合由验证器限定；删除节点或其 typed 值后，来源记录不能恢复它。

所选格式依据[Replay 来源](../references/upstream-sync.md#replay-attachments)。绑定分离节点自身依赖与受信 intake 显式声明的历史/组/配置依赖；不能重新捕获编辑后的值来弱化原绑定。非 reasoning 的规范最终化事件只在 item 尚未闭合、适用的可见值/参数已闭合时接收完整附件，重复最终化或闭合后事件拒绝；这不是通用 replay 更新服务。纯库中的 assistant 图片观察不启用标准 Generation 输出 carrier。

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

原生同名 input 可能表示总输入或未缓存输入，必须声明它与 cache read/write 的关系，不能无条件套子集公式。采用 typed 计数关系和具名归一化 view；原生已报告值保持权威，缺失必要分项时派生值未知。编辑正文或上下文不改写原操作的 usage，也不将其冒充编辑后内容的计量。

Schema 结构/方言/引用、adherence 意图与目标 strict/配额分开。Reasoning mode、effort、预算、显示意图与 replay 分开。请求设置不是响应事实，不能回显补齐；共享时间/identity 不受某个 wire 的必填形式反向限制。

有证据的 mode 与 effort/预算按组合规则验证，例如 adaptive 与 effort 不因示意枚举而被强制互斥。硬上限遇到只有偏好式 effort 的目标不能宣称约束已满足。外部 Schema 引用只使用明确提供的解析内容，不联网；Schema 引用图与输入 JSON nesting 分别预算。

有限控制组合归 [reasoning owner](../../src/semantic/task/generation/reasoning.rs)，方言、资源引用与 strict 规则归 [Schema profile](schema-profile.md)；依据为 [S2 固定来源](../references/upstream-sync.md#client-managed-s2)。Budgeted 必须有数值预算，不提供裸 enabled；soft target 不超过同时声明的 hard limit，省略显示不声称省略内部推理。新 mode/预算/显示、明确方言与外部引用资源没有现行公开 carrier 时保留 typed 值并拒绝目标，不偷偷改为 effort、展开 Schema 或输出私有字段。

引用同时依赖输出 claim 和源资源坐标，单位必须明确；不能近似转换落在 UTF-8 中间的 offset。源编辑使引用重验，wire 索引由最终顺序投影。Configuration update 与 compaction 有作用范围/替代关系，不是普通摘要或可执行设置 patch；当前实现不得因有 union 分支而扩大执行能力。

## 客户端交付与验收

标准 Responses 与兼容 Chat 分别验收，客户端边界不提供独立私有 attachment。必要 replay/关联不能依赖 SDK 偶然保存 unknown fields；客户端丢弃字段后是否仍可安全续轮必须由对应投影合同决定，而不是由 HTTP 200 判断。

有回传要求的切片验证真实交付→保存→追加结果→回传，再检查 owner、值权威、依赖、目标准入与失败边界。IR 级工具结果齐备不是执行就绪证明。独立反例覆盖插入/替换/删除/重排、结构化精度、partial/invalid 值、错 call kind/identity、错误 scope/format、累计计量与发布后失败。

无需把完整工具/opaque/turn 场景作为每个媒体或 Embedding 切片的前置。最低 owning layer 保护不变量，检查方法归[验收基线](../references/conformance-baseline.md)；运行结果不写入合同。

## 来源

主要标准由 [OpenAI 固定基线](../references/upstream-sync.md)定位。补充概念参照：[Google Interactions v1](https://ai.google.dev/api/interactions-api-v1)、[thinking](https://ai.google.dev/gemini-api/docs/thinking)、[streaming](https://ai.google.dev/gemini-api/docs/interactions/streaming)、[stateless 示例](https://ai.google.dev/gemini-api/docs/quickstart.md.txt)，以及 [Anthropic Messages](https://platform.claude.com/docs/en/api/messages)。参考不证明原生接入，也不预定其实现优先级。

ClientManaged 与 Provider 工具的相关出处：[Responses 手动历史](https://developers.openai.com/api/docs/guides/conversation-state)、[Responses web search 输出与引用](https://developers.openai.com/api/docs/guides/tools-web-search)、[Interactions 历史与存储边界](https://ai.google.dev/gemini-api/docs/interactions-overview)、[Messages server tools 与混合调用](https://platform.claude.com/docs/en/agents-and-tools/tool-use/server-tools)。这些来源用于界定语义反例，不承诺三套原生实现已存在。

采用具体协议前固定 API/schema/SDK/profile，解决 required/optional 与事件合同差异，不拼接动态示例。Google 文档为 CC-BY-4.0、示例为 Apache-2.0；保留来源，不复制真实会话或 SDK 实现。账号管理、实时会话、工具执行和真实请求不由这些参考授权。
