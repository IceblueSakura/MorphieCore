# ClientManaged Generation IR 剩余实施计划

**修订：v0.12。当前没有已定稿的剩余行为切片。** 本页保留选片边界、回归输入与延期条件；后续从具体消费者反例或实现缺口选片，不将既有能力重新排期。

语义归 [Semantic Model](../architecture/semantic-ir.md)与[交互合同](../architecture/interaction-contract.md)，映射与损失归 [protocol/lowering](../architecture/protocol-and-lowering.md)。[next-goal](next-goal.md)拥有产品优先级，[current-focus](current-focus.md)只登记实际启动的行为切片。文档修订不启动代码行为或授予操作权限。

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
所选 Replay 的输入归[owner-local 合同](../architecture/interaction-contract.md#typed-replay-与信任)、[格式来源](../references/upstream-sync.md#replay-attachments)、[附件回归](../../tests/semantic/replay_attachments.rs)及用例清单的 `replay-protection` 领域。组只作依赖，库级值/绑定不代表原生 parser、issuer 验证或公开 carrier；资源双端条件和实际消费者回传分别检查，不能由一种格式推定其他格式已准入。

<a id="execution-order"></a>
## 执行顺序

没有自动进入的后继阶段。新切片先固定依赖、最小可观察结果和停止点，再串行实施；后继工作须在获准范围内重新定稿焦点。来源冲突或新的 IR 缺口只阻塞相关范围；必须扩大公共类型或迁移边界时先说明影响，不在实现中不断叠加任务。

纯核心组合与受保护现有接口分别验收；后续资源/投影不代替核心及 Replay 回归。验收关联具体用例、执行证据和未闭合条件，不用单个“阶段完成”标签替代。仅有库级证据不能宣称接口、消费者或真实 Provider 通过。

## 资源、投影与消费回归输入

<a id="p4"></a>
<a id="p4-citation-contract"></a>
资源与引用输入归[唯一资源/引用合同](../architecture/semantic-ir.md#5-内容产物与引用)、[citation](../../src/semantic/task/generation/citation.rs)、[资源回归](../../tests/semantic/resource_table.rs)、[引用组合回归](../../tests/semantic/citation_bindings.rs)和用例清单的 resources/cache 领域。原生条件准入、缺失源正文恢复、更多坐标/媒体、文件服务与外部权限不由本地条件匹配或引用范围验证推出；现有标准引用与严格拒绝作为回归输入，不重新实施资源层。显式断点退休不证明远端缓存收益，原 usage 保留不生成派生正文计量。

<a id="p5"></a>
投影输入归 [semantic-loss 合同](../architecture/protocol-and-lowering.md#semantic-loss)、[具名规则与诊断](../../src/lowering/projection.rs)、[lowering](../../src/lowering/generation.rs)及用例清单的 `projection` 领域。只采用已定稿损失，不合并消息/part；新的规则单独选片。不可变源、复用重验、累计损失与受保护依赖继续作为回归输入，不重新建设投影层。

<a id="p6"></a>
消费输入归[Responses/Chat codecs](../../src/protocol/openai/mod.rs)、[事件 lowering](../../src/lowering/events.rs)、[transport](../../tests/transport.rs)、[固定消费者 gates](../../tests/sdk_loopback.rs)及用例清单的 `consumer` 领域。实际 wire/消费者回传只覆盖已实现的 Responses/Chat；三来源纯组合不代表 Google/Anthropic 原生接入。每片已执行且仍有效的回归直接复用，不从头复制所有测试。

保留交付→保存→追加结果→回传、编辑/配置/资源双锚点/Replay/原 usage 的组合，以及字节切分与语义分批的独立静态预期；无 carrier 的值保留纯库正例和明确拒绝。非法/partial 参数、合法末尾 opaque、累计计量、unknown event、深度/总缓冲、真实重复文本、严格 EOF、取消/失败及发布后禁止 fallback 继续由最低 owner 验证，不能依赖 SDK unknown fields 或快照修补。固定消费者 gate 缺授权/环境时阻塞相应消费声明；通过也不证明一般 SDK/Agent、负载、缓存收益或生产就绪。

## 共用执行与验收规则

- 行为实施前在 current-focus 定稿本片的可观察结果、失败例、非目标、类型迁移和验证边界，再做独立反例 TDD；新的 IR 缺口按[结构决策规则](../architecture/semantic-ir.md#4-ir-不足与标准载体缺口)报告，不能藏进 adapter、fidelity 或任意 JSON。
- 每片复用既有 identity/group/configuration owner，禁止第二份可写正文/关系。局部观察→关联解析→最终请求→目标表示→产品准入分别检查；受信 target、凭据和 attempt 留在外壳，来源标签不自授权。
- 按触点检查 presence、顺序、精度、partial/final、引用域、B−1/B/B+1 和累计预算；未知数据不能覆盖核心或自动执行/转发。观察出现不证明已执行，未知副作用不授权 fallback。
- 同步受影响 re-export、调用点、codec、序列化、OpenAPI 和 fixtures；新 typed 值不放宽严格入口。代码检查及文档检查只按[开发指南](../development.md)执行，不为通过检查提高预算或绕过锁定依赖。
- 每片交付说明合同/owner、实际符号与非零执行、正反例、红绿证据、迁移影响、最终 diff 和未验收层。结果只留当次交付或获准 ignored artifacts；本页不记录运行 SHA、测试计数或完成日记。完成的建设任务从本页移除，回归仍保留。

D（确定性）、P（有界性质）、W（已实现 wire）、K（固定消费者）、C（受控实际服务）是不同证据层。D/P/W 与代码基线按改动执行；K 缺失阻塞相应消费声明，C 不能替代其他层。Synthetic opaque 不证明服务端接受。

真实验证授权统一见 [AGENTS](../../AGENTS.md#scope-and-authorization)：Provider 受控验证适用 standing grant，无逐调用确认或金额上限，但须遵守[有限矩阵、资源/deadline、取消清理和脱敏守卫](../probes.md)。SDK loopback、部署、凭据生命周期和真实工具副作用不继承该授权，分别需要相符授权。

## 追溯与范围维护

[用例清单](../../tests/fixtures/client_managed_cases.json)以稳定 `case_key` 和语义 `area` 组织回归，不绑定执行阶段号。保留输入、人工预期、owner、实际 `target::symbol`、正反例/变换/预算、禁止副作用和适用范围；`sample_kind` 区分 `abstract_contract` 与已实现的 `native_wire`。纯抽象样本注明不适用 native schema，原生分支固定 source/profile/evidence 修订。符号存在不等于已执行，一个 case 也不代表整个来源 ID 通过。

T/A/E 是来源追溯号，不随 P 阶段重排改写其含义：T 来自《主流大模型协议深度剖析与跨厂商统一语义 IR 设计方案：审计修订版 v0.2》，A/E 来自《MorphieCore：IR 完善与面向 Agent 的测试、实施总计划 v0.1》；后者引用的《面向 Agent 的核心合同调整提案 v0.1》仅作已采纳合同的来源。当前规范归架构 owner，Rust 示意不是模板，不要求执行源材料的全部任务。

已实现断言只保留在 owning contracts、源码和测试清单中，不在计划重复全量 T/A/E 表或实现目录。跨阶段需求保留其尚未闭合部分，既有测试作为回归输入；历史排期查 Git，不另建完成档案。来源与许可复用[固定上游来源](../references/upstream-sync.md)、[交互合同来源](../architecture/interaction-contract.md#来源)和 [Pi 固定上下文参考](../references/pi-provider-abstraction.md#client-managed-projection)，不因整理刷新核验日期。

## 明确延期

ServerManaged/主动 previous-ID/conversation、远端操作恢复、Agent 执行与自动续轮、持久化/checkpoint/journal、claim/CAS/fencing、OpaqueStore、Google/Anthropic 原生接入及其他媒体扩展继续按 [next-goal](next-goal.md#延期目标与恢复条件)处理；不预建空壳，也不作为既有核心或消费者回归的前置。原 A25/A26/A28/A29 与 E03/E05 的远端/持久化部分不在本轮，延期不等于通过。

Reasoning opaque 的闭合后权威遵守[待决状态](../implementation-status/open-questions.md#reasoning-opaque-的闭合后权威)。可选 Responses Provider 工具输出观察须另行选片，不阻塞纯核心工作，也不自动开放 hosted 请求或工具执行。更广控制/Schema 的具体目标映射不从已有库级表达推定启用。
