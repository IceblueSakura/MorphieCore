# ClientManaged Generation IR 剩余实施计划

**修订：v0.10。只维护尚未闭合的工作，以 P5–P6 作为当前执行编号。** 已有能力作为执行基线，不重新排期；一次只执行一个有明确停止点的行为子片。

语义归 [Semantic Model](../architecture/semantic-ir.md)与[交互合同](../architecture/interaction-contract.md)，映射与损失归 [protocol/lowering](../architecture/protocol-and-lowering.md)。本页拥有剩余依赖与退出条件，[next-goal](next-goal.md)拥有产品优先级，[current-focus](current-focus.md)只登记实际启动的行为切片。文档修订不启动代码行为或授予操作权限。

## 范围与既有输入

- Responses、Google Interactions、Anthropic Messages 的请求型 Generation 差异共同约束 IR。Google/Anthropic 本轮只提供设计证据与 typed 反例，不实现 codec、adapter、SDK 或网络接入；generateContent 仅作独立反例。
- 既有[语义核心](../../src/semantic/task/generation/mod.rs)、[用例清单](../../tests/fixtures/client_managed_cases.json)和[实现缺口](../implementation-status/generation.md)是输入，不重新排期建设身份、配置、上下文、控制或计量基础。
- ClientManaged 显式选择历史，不意味着发送全部日志，也不证明无服务端存储或 ZDR。工具观察、上下文变换与后继要求不授权工具执行、自动续轮或自动摘要。
- 现有 Responses/Chat 的受影响调用点与独立预期随每片迁移，不集中拖到最后。新 typed 值不自动激活公共 carrier；没有合法映射仍拒绝，不恢复 `_openbridge` 或引入同义私有字段。

<a id="p1"></a>
Provider 动作及引用域的输入归[交互合同](../architecture/interaction-contract.md#provider-tool-observations)、[动作来源](../references/upstream-sync.md#provider-actions)与用例清单的 `provider-observations` 领域；不从库级表达推定原生 hosted 工具接线。

<a id="p2"></a>
纯核心组合输入归[ClientManaged 合同](../architecture/interaction-contract.md#client-managed-context)、[三来源组合回归](../../tests/semantic/client_managed_composition.rs)及用例清单中的配置、控制、Schema、计量、事件和现有 wire 正反例。每个 case 只覆盖声明的预期，不由核心表达推定接口、消费者或真实 Provider 验收。

<a id="p3"></a>
所选 Replay 的输入归[owner-local 合同](../architecture/interaction-contract.md#typed-replay-与信任)、[格式来源](../references/upstream-sync.md#replay-attachments)、[附件回归](../../tests/semantic/replay_attachments.rs)及用例清单的 `replay-protection` 领域。组只作依赖，库级值/绑定不代表原生 parser、issuer 验证或公开 carrier；资源双端条件和实际消费者回传仍属下方剩余范围。

<a id="execution-order"></a>
## 执行顺序

| 阶段 | 依赖 | 要交付的最小结果 |
|---|---|---|
| [P5 具名投影](#p5) | P4 与所选 Replay；已定稿损失规则 | 可检查的投影/损失结果及复用重验 |
| [P6 接口与消费者验收](#p6) | P5；前序受影响回归 | 现有 Responses/Chat 的交付、保存、追加和回传闭环 |

**默认按 P5 → P6 串行执行，一次只执行一个子片。** 子片满足退出条件后即停止扩展并交付，不自动串入下一子片。来源冲突或新的 IR 缺口只阻塞相关范围；必须扩大公共类型或迁移边界时先说明影响，不在实现中不断叠加任务。

纯核心组合与 P6 的受保护现有接口分别验收；后续资源/投影不代替核心及 Replay 回归。验收逐条引用阶段与条目（如 P5.2），关联用例、执行证据和未闭合条件，不用单个“阶段完成”标签替代。仅有库级证据不能宣称接口、消费者或真实 Provider 通过。

## 各阶段退出条件

<a id="p4"></a>
<a id="p4-citation-contract"></a>
资源与引用输入归[唯一资源/引用合同](../architecture/semantic-ir.md#5-内容产物与引用)、[citation](../../src/semantic/task/generation/citation.rs)、[资源回归](../../tests/semantic/resource_table.rs)、[引用组合回归](../../tests/semantic/citation_bindings.rs)和用例清单的 resources/cache 领域。原生条件准入、缺失源正文恢复、更多坐标/媒体、文件服务与外部权限不由本地条件匹配或引用范围验证推出；现有标准引用与严格拒绝作为 P5 的输入，不重新实施资源层。显式断点退休不证明远端缓存收益，原 usage 保留不生成派生正文计量。

<a id="p5"></a>
### P5：具名投影、组合损失与复用重验

**输入与 owner**：所选 Replay 与 P4 最终值及 [semantic-loss 合同](../architecture/protocol-and-lowering.md#semantic-loss)、[lowering](../../src/lowering/generation.rs)与[具名适配](../../src/protocol/adaptation.rs)。每条新损失规则先定稿方向、目标 profile、owner、前提与后果，不设全局近似开关。

**逐项验收**：

1. 返回可检查的投影值、规则/owner/方向/后果、剩余前提和修订；精确、具名损失、未知规则能区分，既有合法规则保持，未知规则拒绝。
2. 源值、目标/profile、配置或资源条件改变后，旧投影/绑定不可直接复用；变换后重新验证最终值、预算、依赖与 requirements。
3. 多段损失累积且端到端重验；各固定候选从同一不可变源开始，不互相污染、不取能力并集或重排 route。
4. 必要身份/关联/replay、指令 authority/scope/phase、硬 Schema/控制、真实失败/终态均不因兼容投影被弱化。连续同角色/多文本的合并只有在具名规则允许时才进行，并明确不可逆边界，不能借来源记录恢复丢失分组。

**追溯**：A15/A23/A24；T02/T20/T21/T23/T25/T28/T29；E02/E06 的投影部分。没有新审批/执行许可、资源准备 I/O 或产品 catalog 重建。

<a id="p6"></a>
### P6：现有接口与消费者闭环验收

**输入与 owner**：P5 及前序受影响路径；[Responses/Chat codecs](../../src/protocol/openai/mod.rs)、[事件 lowering](../../src/lowering/events.rs)、[transport](../../tests/transport.rs)与[固定消费者 gates](../../tests/sdk_loopback.rs)。每片已执行的有效回归直接复用，此处收口跨边界组合，不从头复制所有测试。

**逐项验收**：

1. 对有标准 carrier 的所选场景，独立验证 wire→IR、IR→wire、静态/流式及失败，完成交付→保存→追加结果→回传。无 carrier 的值保留纯库正例与明确拒绝，不依赖 SDK unknown fields 幸存。
2. E02 的两个派生分支联合覆盖编辑、配置、资源双锚点、replay 和原 usage；E06 同一纯消费者处理三协议语义反例，实际 wire/消费者回传只覆盖已实现的 Responses/Chat。
3. E04 分别检查字节切分与语义分批：UTF-8/转义截断、非法/partial 参数、合法末尾 opaque、累计 usage、unknown event、深 nesting 和总缓冲超限；独立静态 oracle 一致，不靠 snapshot 修补。
4. 流内 error、严格 EOF、真实重复文本、取消/失败后不可重开及发布/提交后禁止 fallback 均保留；不先发布成功终态再撤回。受影响序列化、OpenAPI、fixtures 和独立媒体边界同步核对。
5. 必需固定消费者 gate 有真实执行证据；缺授权/环境则明确阻塞对应消费子项，不能宣称 P6 完成。真实 Provider 验证只按实际需求补充，不能替代离线或消费者检查。

**追溯**：A02/A27/A30–A32；T14/T22/T26/T30/T31；E02/E04/E06 的跨边界部分。P6 通过也不证明一般 SDK/Agent、负载、缓存收益或生产就绪。

## 共用执行与验收规则

- 行为实施前在 current-focus 定稿本片的可观察结果、失败例、非目标、类型迁移和验证边界，再做独立反例 TDD；新的 IR 缺口按[结构决策规则](../architecture/semantic-ir.md#4-ir-不足与标准载体缺口)报告，不能藏进 adapter、fidelity 或任意 JSON。
- 每片复用既有 identity/group/configuration owner，禁止第二份可写正文/关系。局部观察→关联解析→最终请求→目标表示→产品准入分别检查；受信 target、凭据和 attempt 留在外壳，来源标签不自授权。
- 按触点检查 presence、顺序、精度、partial/final、引用域、B−1/B/B+1 和累计预算；未知数据不能覆盖核心或自动执行/转发。观察出现不证明已执行，未知副作用不授权 fallback。
- 同步受影响 re-export、调用点、codec、序列化、OpenAPI 和 fixtures；新 typed 值不放宽严格入口。代码检查及文档检查只按[开发指南](../development.md)执行，不为通过检查提高预算或绕过锁定依赖。
- 每片交付说明合同/owner、实际符号与非零执行、正反例、红绿证据、迁移影响、最终 diff 和未验收层。结果只留当次交付或获准 ignored artifacts；本页不记录运行 SHA、测试计数或完成日记。完成的建设任务从本页移除，回归仍保留。

D（确定性）、P（有界性质）、W（已实现 wire）、K（固定消费者）、C（受控实际服务）是不同证据层。D/P/W 与代码基线按改动执行；K 缺失阻塞相应消费声明，C 不能替代其他层。Synthetic opaque 不证明服务端接受。

真实验证授权统一见 [AGENTS](../../AGENTS.md#scope-and-authorization)：Provider 受控验证适用 standing grant，无逐调用确认或金额上限，但须遵守[有限矩阵、资源/deadline、取消清理和脱敏守卫](../probes.md)。SDK loopback、部署、凭据生命周期和真实工具副作用不继承该授权；本次计划整理不运行这些操作。

## 追溯与范围维护

[用例清单](../../tests/fixtures/client_managed_cases.json)以稳定 `case_key` 和语义 `area` 组织回归，不绑定执行阶段号。保留输入、人工预期、owner、实际 `target::symbol`、正反例/变换/预算、禁止副作用和适用范围；`sample_kind` 区分 `abstract_contract` 与已实现的 `native_wire`。纯抽象样本注明不适用 native schema，原生分支固定 source/profile/evidence 修订。符号存在不等于已执行，一个 case 也不代表整个来源 ID 通过。

T/A/E 是来源追溯号，不随 P 阶段重排改写其含义：T 来自《主流大模型协议深度剖析与跨厂商统一语义 IR 设计方案：审计修订版 v0.2》，A/E 来自《MorphieCore：IR 完善与面向 Agent 的测试、实施总计划 v0.1》；后者引用的《面向 Agent 的核心合同调整提案 v0.1》仅作已采纳合同的来源。当前规范归架构 owner，Rust 示意不是模板，不要求执行源材料的全部任务。

已实现断言只保留在 owning contracts、源码和测试清单中，不在计划重复全量 T/A/E 表或实现目录。跨阶段需求保留其尚未闭合部分，既有测试作为回归输入；历史排期查 Git，不另建完成档案。来源与许可复用[固定上游来源](../references/upstream-sync.md)、[交互合同来源](../architecture/interaction-contract.md#来源)和 [Pi 固定上下文参考](../references/pi-provider-abstraction.md#client-managed-projection)，不因整理刷新核验日期。

## 明确延期

ServerManaged/主动 previous-ID/conversation、远端操作恢复、Agent 执行与自动续轮、持久化/checkpoint/journal、claim/CAS/fencing、OpaqueStore、Google/Anthropic 原生接入及其他媒体扩展继续按 [next-goal](next-goal.md#延期目标与恢复条件)处理；不预建空壳，也不作为核心回归或 P6 前置。原 A25/A26/A28/A29 与 E03/E05 的远端/持久化部分不在本轮，延期不等于通过。

Reasoning opaque 的闭合后权威遵守[待决状态](../implementation-status/open-questions.md#reasoning-opaque-的闭合后权威)。可选 Responses Provider 工具输出观察须另行选片，不阻塞纯核心工作，也不自动开放 hosted 请求或工具执行。更广控制/Schema 的具体目标映射不从已有库级表达推定启用。
