# ClientManaged Generation IR 剩余实施计划

**修订：v0.5。只维护尚未闭合的工作，以 P1–P6 作为当前执行编号。** 已有能力不再单列建设任务；删除完成项不删除合同或回归，也不意味着整体验收通过。

语义归 [Semantic Model](../architecture/semantic-ir.md)与[交互合同](../architecture/interaction-contract.md)，映射与损失归 [protocol/lowering](../architecture/protocol-and-lowering.md)。本页拥有剩余依赖与退出条件，[next-goal](next-goal.md)拥有产品优先级，[current-focus](current-focus.md)只登记实际启动的行为切片。文档修订不启动代码行为或授予操作权限。

## 范围与既有输入

- Responses、Google Interactions、Anthropic Messages 的请求型 Generation 差异共同约束 IR。Google/Anthropic 本轮只提供设计证据与 typed 反例，不实现 codec、adapter、SDK 或网络接入；generateContent 仅作独立反例。
- 既有[语义核心](../../src/semantic/task/generation/mod.rs)、[用例清单](../../tests/fixtures/client_managed_cases.json)和[实现缺口](../implementation-status/generation.md)是输入，不重新排期建设身份、配置、上下文、控制或计量基础。
- ClientManaged 显式选择历史，不意味着发送全部日志，也不证明无服务端存储或 ZDR。工具观察、上下文变换与后继要求不授权工具执行、自动续轮或自动摘要。
- 现有 Responses/Chat 的受影响调用点与独立预期随每片迁移，不集中拖到最后。新 typed 值不自动激活公共 carrier；没有合法映射仍拒绝，不恢复 `_openbridge` 或引入同义私有字段。

<a id="execution-order"></a>
## 执行顺序

| 阶段 | 依赖 | 要交付的最小结果 |
|---|---|---|
| [P1 Provider 动作语义](#p1) | 既有核心；所选动作证据 | 有必要 typed 内容的动作观察及关联/编辑保护 |
| [P2 纯交互核心验收](#p2) | P1 | 核心组合场景和来源缺口闭合；不是新一轮基础设施实现 |
| [P3 Replay 保护](#p3) | P2；所选格式证据 | 逐格式附件、依赖与合法最终化 |
| [P4 资源、引用与报告归属](#p4) | P2；与 P3 对齐的依赖 owner | 双端引用、资源条件、cache 依赖与原报告归属 |
| [P5 具名投影](#p5) | P3、P4；已定稿损失规则 | 可检查的投影/损失结果及复用重验 |
| [P6 接口与消费者验收](#p6) | P5；前序受影响回归 | 现有 Responses/Chat 的交付、保存、追加和回传闭环 |

**默认按 P1 → P2 → P3 → P4 → P5 → P6 串行执行。** P3/P4 的证据整理可并行，但不并发重写共享 owner。每片只等待自己的实际依赖；来源冲突或新的 IR 缺口只阻塞相关子范围，不通过猜测或降低验收标准绕过。

P2 是纯交互核心的验收点，P6 是受保护现有接口的验收点。两者分开交付；验收逐条引用阶段与条目（如 P3.2），关联用例、执行证据和未闭合条件，不用单个“阶段完成”标签替代。仅有库级证据不能宣称接口、消费者或真实 Provider 通过。

## 各阶段退出条件

<a id="p1"></a>
### P1：Provider 动作语义与引用域

**输入与 owner**：[Provider 观察](../../src/semantic/task/generation/provider.rs)、[身份](../../src/semantic/task/generation/identity.rs)、[规范事件](../../src/semantic/task/generation/event.rs)及其现有回归。先固定本片所需的官方 API family/version、动作内容、参数、引用域和适用事件证据，不做无关的全协议调研。

**新增工作**：从代表性样本确定最小 typed 动作/参数结构，接入既有身份、修订、关系、依赖和预算。不能用 generic JSON、动作名称猜测或虚构 function call 替代缺失概念。

**逐项验收**：

1. 选中动作有独立正例与最小反例；Interactions Step 与 generateContent Part 的来源/附件位置不混用，来源冲突有明确诊断。
2. 新增动作内容的修改形成适当的新调用身份/派生关系，旧结果与证明不移挂；删除、重排与候选隔离复用既有保护。
3. 混合客户端 C / Provider S 仍只要求客户端提交 C；requester、执行责任、执行报告、正文和产物状态不混同，不从参数或 scope 标签取得信任。无报告不补成功，失败可有正文，后继事实不擦除旧观察。
4. R2 的结果在选中历史中唯一解析到 R1；缺失、歧义、错 kind 分别拒绝。静态与规范事件一致，partial 不物化成闭合结果，终态 snapshot 不补内容。
5. 新语义的现有目标拒绝和产品准入边界保留；不启用 hosted 请求、工具执行或新原生 parser。

**追溯**：A05、A09–A11；T03、T11/T12、T16–T18；E01 的动作/责任部分。新增用例与 `provider-observations` 领域的已有用例合并核对。

<a id="p2"></a>
### P2：纯交互核心组合验收

**输入与 owner**：P1 的类型/证据与[静态追溯清单](../../tests/fixtures/client_managed_cases.json)。只补真正缺失的断言或修复负责的 owner，不重新实现已存在的核心。

**逐项验收**：

1. 核对三协议设计样本与人工语义预期；既有配置、控制、Schema、usage 证据直接复用，只补实际缺口。每个拒绝反例有对应正例，不能靠“全部拒绝”宣称表达完整。
2. 将 P1 的具体动作带入配置化 C/S 后继请求：保留文本/reasoning、身份与显式组，追加已报告 C 结果，再将 R2 的 S 结果解析到 R1。历史/配置依赖缺失须诊断，不重建调用、补造结果或执行请求。
3. 在同一不可变源上构造两个派生上下文；插入、替换、删除、重排和配置漂移不相互污染。原定义、参数权威、指令作用位置/authority、phase 与原报告不因变换被改称新事实。
4. 客户端 pending results、显式 Provider 后继要求和 reported progress 保持分离；空结果要求不推导 turn finished。复用三来源纯消费者、规范事件分批及现有 wire 正反例，不造 Google/Anthropic 测试 codec。
5. 用例选择器非空、真实符号已执行；结构、关联、目标表示和实例准入分层结论明确。资源双端依赖、逐格式 replay 与实际消费者回传由 P3–P6 验收，不混入此处的通过声明。

**追溯**：E01、E02/E04/E06 的纯核心部分；复用清单中验证、身份/关系、上下文、控制/Schema/计量领域的适用 T/A 断言。P2 不新增外部消费者或真实服务门槛。

<a id="p3"></a>
### P3：逐格式 Replay 与受保护编辑

**输入与 owner**：[replay 合同](../architecture/interaction-contract.md#typed-replay-与信任)、[typed 值](../../src/semantic/task/generation/replay.rs)、[依赖](../../src/semantic/task/generation/dependency.rs)、[fidelity](../../src/protocol/fidelity.rs)。先固定所选格式的附件位置、兼容 scope、依赖和最终化条件。

**逐项验收**：

1. item/part/group 的实际附件各有 typed 唯一载荷；绑定只保留来源/依赖，不复制正文或混并 opaque。空可见 reasoning 不删除实际 opaque；call/media part 的附件不移挂到其他 owner。
2. 按格式检查来源、scope、partial/final 和必要依赖；跨模型/版本兼容有明确允许依据，未知则拒绝。普通 scope 或本地 hash 不充当 issuer 验签。
3. 依赖内的内容、顺序、组、prefix、工具/Schema、配置或资源修改失效；依赖外编辑不过度拒绝。删除 owner/typed 值后，索引、raw 或 fidelity 不恢复它。
4. 可见内容结束后、owner 合法闭合前的末尾材料保留；闭合后的权威改写继续遵守[延期边界](../implementation-status/open-questions.md#reasoning-opaque-的闭合后权威)，不扩大本片。
5. 独立静态 oracle 与事件分批一致；未闭合 opaque、单值/累计预算及脱敏分别验证。raw 未保留、冲突或已脱敏时据实降低保真声明，不泄漏到 Debug/UI/普通日志或异源请求。

**追溯**：A06/A07、A20/A22/A32；T03–T08、T13、T22、T27/T28/T30/T32；E02/E04 的 replay 部分。没有存储服务、认证证明服务或跨进程恢复。

<a id="p4"></a>
### P4：资源、引用、Cache 与原报告归属

**输入与 owner**：[资源与引用合同](../architecture/semantic-ir.md#5-内容产物与引用)、[resource](../../src/semantic/task/generation/resource.rs)、[text](../../src/semantic/task/generation/text.rs)、[cache](../../src/semantic/cache.rs)与[usage](../../src/semantic/task/generation/usage.rs)。范围为 Text/Image/File；其他 task 只做受影响回归。

**逐项验收**：

1. 区分资源 identity、locator 和输入/工具/推理/产物用途。外壳显式提供的 scope、权限、期限条件不足时拒绝表示或保留未满足要求；相同 URL/key 不证明相同资源、权限或 replay 资格。
2. 引用同时绑定输出 claim 与源资源坐标，单位明确；源/输出两端的删除和替换分别重验，重排只按合同重算目标坐标。坐标转换须有明确单位与来源依据，不假称恢复已经丢失的原边界。
3. cache hint、断点、prefix、TTL 意图和实际命中分开；删除不复活，配置/工具/Schema 与前缀依赖按声明重验，不声称缓存收益。
4. 编辑后保留原 usage 的操作归属，不冒充新正文计量；缺项不补零、累计报告不重复相加、具名派生保留完整前提。
5. 单资源、多个小资源总量、引用图与编码/解码预算分别验收；P3 的附件/依赖因资源变化受到影响时补组合回归。

**追溯**：A18/A19/A21/A27；T15/T19/T28；E02 的资源/引用/报告部分。无下载、上传、转码、病毒扫描、文件服务或自动跨 Provider 复制。

<a id="p5"></a>
### P5：具名投影、组合损失与复用重验

**输入与 owner**：P3/P4 最终值及 [semantic-loss 合同](../architecture/protocol-and-lowering.md#semantic-loss)、[lowering](../../src/lowering/generation.rs)与[具名适配](../../src/protocol/adaptation.rs)。每条新损失规则先定稿方向、目标 profile、owner、前提与后果，不设全局近似开关。

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

ServerManaged/主动 previous-ID/conversation、远端操作恢复、Agent 执行与自动续轮、持久化/checkpoint/journal、claim/CAS/fencing、OpaqueStore、Google/Anthropic 原生接入及其他媒体扩展继续按 [next-goal](next-goal.md#延期目标与恢复条件)处理；不预建空壳，也不作为 P2/P6 前置。原 A25/A26/A28/A29 与 E03/E05 的远端/持久化部分不在本轮，延期不等于通过。

Reasoning opaque 的闭合后权威遵守[待决状态](../implementation-status/open-questions.md#reasoning-opaque-的闭合后权威)。可选 Responses Provider 工具输出观察须另行选片，不阻塞 P2，也不自动开放 hosted 请求或工具执行。更广控制/Schema 的具体目标映射不从已有库级表达推定启用。
