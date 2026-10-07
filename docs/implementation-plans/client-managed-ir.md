# 三协议约束下的 ClientManaged Generation IR 实施计划

**交付修订：v0.4。范围与设计约束不变；S2 的上下文、控制、Schema 与计量由纯核心 owner 承载，M1/M2 仍按完整退出条件判定，原生接入不随库级实现启用。**

本页是实施切片与验收需求的唯一维护入口，不是第二份语义合同、运行报告或操作授权。概念归 [Semantic Model](../architecture/semantic-ir.md)，交互规则归 [Generation 合同](../architecture/interaction-contract.md)，映射与损失归 [protocol/lowering](../architecture/protocol-and-lowering.md)。[next-goal](next-goal.md)拥有优先级，[current-focus](current-focus.md)拥有实际行为切片。

输入为《主流大模型协议深度剖析与跨厂商统一语义 IR 设计方案：审计修订版 v0.2》及《MorphieCore：IR 完善与面向 Agent 的测试、实施总计划 v0.1》。保留原 T01–T32、A01–A32、E01–E06 的追溯编号，按本轮范围拆分验收，不要求执行旧计划中的全部 Agent/协议接入任务。源材料的 Rust 片段是示意，不直接作为实现模板；本页与 owning contracts 足以开展实施，不依赖个人桌面路径。

T 编号来自协议调研；AGC/S 编号沿用旧总计划所引用的《MorphieCore：面向 Agent 的核心合同调整提案 v0.1》；A/E 编号由旧总计划新增。保留来源与编号不表示接受这些材料的全部实施范围，具体取舍以下文及现行合同为准。

## 1. 范围与交付层次

| 领域 | 本轮必须交付 | 不随本轮实施 |
|---|---|---|
| 三协议 | Responses、Google Interactions、Anthropic Messages 的请求型 Generation 差异共同约束 IR；固定相关正反例 | Google/Anthropic codec、adapter、SDK、网络接入和实例激活；generateContent 仅作独立反例，不成为接入目标 |
| 上下文 | ClientManaged 的显式历史选择、配置快照、编辑、关联与依赖检查 | ServerManaged、previous-ID/conversation 续接、连接级增量历史；不预建空壳模式 |
| 工具 | client/provider 执行责任、调用观察、结果、执行报告与跨响应关联 | 工具执行器、自动续轮、审批/许可服务、工具副作用核对引擎 |
| 生命周期 | 参数/内容/item/response/交付/交互进度分离，静态与事件一致 | checkpoint、持久化 journal、恢复游标、远端 get/resume/cancel/delete 接线、claim/CAS/fencing |
| 变换与保护 | replay、资源用途/locator、引用、usage/cache 与配置依赖的纯语义完善 | OpaqueStore、文件服务、自动上传/下载/转码、自动摘要/裁剪/重试 |
| 现有接口 | Responses/Chat 受影响的 codec/lowering、序列化、OpenAPI、fixtures 与消费者同步迁移 | 新私有 carrier、完整 Responses union、默认启用 hosted tools 或放宽现有产品准入 |
| 其他 task | 独立图片、Speech/Transcription 和共享基础值的受影响回归 | 将独立 task 并入万能 Generation，恢复 Realtime 或其他延期媒体 |

本轮“无状态”指请求显式携带所需客户端历史，不等于没有本地历史、原生 cache、opaque 或 Provider 内部工具状态，也不证明服务端无存储或 ZDR。历史投影保持完整依赖，不要求发送客户端全部原始日志。

### 交付层次

- **M1：纯交互核心**——S0–S2 的本轮断言闭合，ClientManaged 与混合工具/跨响应结果可构造、检查和编辑，无工具或模型 I/O。
- **M2：受保护的现有接口闭环**——S3 与前序回归闭合，replay/资源/报告/具名投影形成闭环，现有 Responses/Chat 受影响路径同步验证。
- 原 M3（恢复）与 M4（新协议接入）延期，不是 M1/M2 的前置。仅有库级通过时不声明公共接口或消费者通过；必需消费者 gate 缺授权/环境时，明确阻塞该消费子项，不宣称完整 M2 验收。

## 2. 已选设计与 owner

实施遵循下列已选方向；具体 Rust 命名与文件拆分在子切片中确定，不再重新选择三协议范围、ClientManaged 或是否建设 Agent runtime。

| 合同 | 已选方向及权威入口 | 首要落点 |
|---|---|---|
| AGC-01 | [scoped identity、native alias、修订与派生](../architecture/interaction-contract.md#身份分组与依赖)；参数改写形成新调用，旧结果不移挂 | `request.rs`、`turn.rs`、`dependency.rs`、source records |
| AGC-02 | 有序异构 items 与有限 typed group；每类关系唯一 owner，成员不复制正文，反向 view 派生 | `group.rs`、`validate.rs`、`protocol/fidelity.rs` |
| AGC-03 | [Provider 工具观察](../architecture/interaction-contract.md#provider-tool-observations)采用独立 typed 分支；requester、执行责任、进度、结果和产物分离 | `provider.rs`、`tool_result.rs`、`continuation.rs`、`event.rs` |
| AGC-04 | [ClientManaged](../architecture/interaction-contract.md#client-managed-context)；调用者选定历史，显式配置修订，要求集合不代表自动执行 | `client_managed.rs`、`request.rs`、`semantic/context.rs`、`turn.rs`、`progress.rs` |
| AGC-05 | [replay/编辑失效](../architecture/interaction-contract.md#typed-replay-与信任)、[资源与引用](../architecture/semantic-ir.md#5-内容产物与引用)；唯一载荷、按格式依赖，不复活删除值 | `reasoning.rs`、`replay.rs`、`dependency.rs`、`resource.rs`、`text.rs`、`usage.rs` |
| AGC-06 | 本轮只实现请求型观察、严格事件/终态与已有交付边界；后继报告不重开旧 reducer | `event.rs`、既有 decoder、`execution/`、`transport/` |
| AGC-07 | 局部观察、关联解析、最终请求、目标表示和产品准入分层；[具名投影](../architecture/protocol-and-lowering.md#semantic-loss)不授权弱化硬约束 | `validate.rs`、`request.rs`、`lowering/`、`protocol/` |
| AGC-08 | 持久化、attempt 恢复、CAS/claim/fencing 全部延期；不把内存 proof 当持久化协议 | 不新增实现 |

未写全的 Generation 文件名位于 `src/semantic/task/generation/`。现有 paths 是定位入口，不要求保留不合适的 Rust shape，也不因新增类型自动扩大其公共 carrier。

### Pi 参考的采用边界

以 [Pi v1.0.4 固定上下文参考](../references/pi-provider-abstraction.md#client-managed-projection)补充细节：借鉴原观察与请求投影分离、每请求重建上下文、配置/工具声明明确、内容变换先于 wire 表示。客户端负责历史选择；核心不复制 SessionManager、JSONL tree、Agent loop 或 compaction 策略。

不移植 Pi 的合成工具结果、thinking 转文本、图片占位、删除签名或任意 content-only 编辑。外部摘要不提升 authority、不继承旧 replay/计量；显式裁剪不允许悬空结果。模式设计只提供 ClientManaged，不建立暂不可用的 ServerManaged 入口。

### 不照搬调研示意的结构

- 受信 target、route、policy、凭据和 attempt 留在外壳，不进入业务语义请求。
- Reasoning mode、effort、预算和显示采用有证据的组合规则，不能将 adaptive 与 effort 错设为互斥。
- Usage 采用 typed 计数关系与具名归一化 view，不把各协议 input/cache 同名字段直接套入固定子集关系。
- 工具失败可以有正文；无正文执行观察不补空结果。调用观察不证明已执行，单个原生 item 也不强拆成虚构 call/result。
- 保留严格 JSON envelope parser；非法/partial 参数观察不要求重建通用 JSON AST 或重复键归档系统。
- Typed opaque 是唯一载荷，fidelity 只绑定来源/依赖；受控 raw capture 可缺省，不建设存储服务。

## 3. 证据、测试与迁移纪律

### 三种不同的证据

1. **设计证据**：固定官方 API family/version、资料定位、相关 schema/事件修订与适用分支。原生样本只说明为何需要该语义，不等于已实现 decoder。
2. **共享核心测试**：用 synthetic typed 值和规范事件，验证人工审定的语义预期。Google/Anthropic 在本轮停在此层，不写“测试专用”parser 或假 codec 绕过延期。
3. **已实现 wire 测试**：现有 Responses/Chat 的独立 wire→IR、IR→wire、静态/流式、失败与消费回传。原生输入、语义预期、目标表示/失败预期独立编写，不能只做 round trip。

D 表示确定性最低 owner 检查，P 表示有界可复现性质检查，W 表示已实现 wire 边界的 D/P，K 表示显式固定消费者 gate，C 表示受控实际服务验证。K/C 不是默认离线检查，也不能互相替代。Synthetic opaque 只证明本地绑定与隔离，不证明服务端接受。

### 最小追溯机制

S0 先核对已有符号，再建立最小静态 ID→合同→owner→测试符号映射，可复用邻近测试组织或小型 fixture manifest；不先建设测试管理平台。每个新增或复用子用例至少给出：

```text
case_key / 原 T、A、E ID / 子切片
sample_kind（abstract_contract 或已实现的 native_wire）
source/profile/evidence 修订；纯抽象样本注明不适用 native schema
owner / 实际 target::symbol
输入 / 人工语义预期 / 目标表示或诊断阶段
正例 / 反例 / 相关变换、预算与禁止副作用
范围：本轮必需或明确延期及恢复条件
```

实现后核对符号确实注册、选择器非空且真实执行。不得用空测试、无断言占位、忽略失败或 N/A 掩盖缺失。运行 SHA、命令、计数和 Passed/Failed/Blocked 留在当次交付或获准 ignored artifacts；本页不维护测试结果、完成日记或动态支持矩阵。

### 每片共同门槛

- 先在 current-focus 写明可观察结果、需求、失败例、非目标、类型迁移与验证边界；以独立反例 TDD。已有覆盖直接复用。
- 局部观察与合法请求使用不同保证，不能仅去掉 `response` 的结果拒绝或放松外部 validate。参数里的 executor、source/profile 标签不提供信任。
- Presence、身份、顺序、精度、引用域、partial/final、预算和日志脱敏按触点覆盖。B−1/B/B+1、深 nesting、未闭合参数/opaque、多小资源的总量分别检查。
- 先验证观察，后解析关系，再验证最终上下文和目标要求。目标投影不能恢复删除值、弱化硬约束或污染另一候选。
- 既有 Responses/Chat 调用点和独立预期随每片迁移，不把所有 codec 修复推迟到 S3。公共 schema、序列化、OpenAPI 与 fixtures 只按实际影响同步。
- 纯语义检查不依赖产品 catalog；产品准入、实际 Router 和消费者各在最低适合 owner 检查。只在有真实边界价值时使用 synthetic I/O 计数，不为纯函数添加无意义的“零调用”装饰断言。
- 涉及执行的未知事实不授权 fallback；本轮不启用需要新副作用恢复策略的 hosted 请求。保留既有提交前后、取消、deadline、backpressure 与清理防线。

## 4. 实施切片与退出条件

### 当前实施边界与剩余工作

下表用于定位下一步工作，不替代下文的完整退出条件，也不是运行结果或完成日记。实现事实以链接的源码与[用例追溯清单](../../tests/fixtures/client_managed_cases.json)为准；清单中的 ID 只覆盖各 case 声明的子范围，不能推定整个 A/T/E 已验收。

| 切片 | 当前实现落点 | 尚未闭合的要求 / 接续边界 |
|---|---|---|
| S0 | [validated 请求修改](../../src/semantic/task/generation/request.rs)包含 output/reasoning 重验；[独立反例](../../tests/semantic/request_edits.rs)和追溯入口已建立 | 三协议具体样本的固定证据仍随依赖子片核对，不能由 A01 修复推定所有来源已闭合 |
| S1.1 | [scoped identity/alias](../../src/semantic/task/generation/identity.rs)、[重叠 replay group 与单一 message owner](../../src/semantic/task/generation/group.rs)、调用修订与[续接关系检查](../../src/semantic/task/generation/turn.rs)已有实现 | 核心关系不授权 wire carrier；后续新增动作/参数必须接入相同身份、编辑和依赖保护，跨阶段验收仍须逐项核对 |
| S1.2（部分） | [独立 Provider 观察](../../src/semantic/task/generation/provider.rs)表达操作/引用、可缺省报告与结果；[规范事件](../../src/semantic/task/generation/event.rs)包含整值观察和文本/JSON 结果 builder；[反例](../../tests/semantic/provider_observations.rs)覆盖 C/S 区分与跨响应关联 | Provider 动作/参数的必要 typed 内容仍待具体样本核对；未闭合结果只在 builder 中观察，不能物化为闭合静态结果；不包含原生 Provider parser 或媒体增量 |
| S2.1 | [ClientManaged](../../src/semantic/task/generation/client_managed.rs)、[配置](../../src/semantic/task/generation/configuration.rs)与[变换](../../src/semantic/task/generation/context_transform.rs)拥有显式选择、原定义绑定、原子编辑与分层诊断；[续接要求](../../src/semantic/task/generation/continuation.rs)独立于客户端结果和 reported progress | 作用范围采用当前快照与有序历史位置，保留现有 phase；更广摘要/指令策略不由本片推定。Provider 动作另归 S1.2，逐格式 replay、资源双端依赖及 E02/E04/E06 跨片条件归 S3；修订/绑定没有公共 carrier 或持久化保证 |
| S2.2 | [reasoning](../../src/semantic/task/generation/reasoning.rs)表达 mode/effort/硬软预算/显示；[SchemaDocument](../../src/semantic/task/generation/schema_document.rs)拥有方言与显式引用资源，沿用 [validator](../../src/semantic/task/generation/schema.rs)的 strict/独立预算；[usage view](../../src/semantic/task/generation/usage_views.rs)按声明计数关系派生 | Schema 采用未声明方言的既有词汇与明确 2020-12 的有限子集，不声称通用 evaluator。新增控制/方言/资源的原生映射没有随纯核心启用，现有目标保持严格拒绝；依赖外部 profile 的最小预算、更多方言与效果校准另行选片 |
| S3.1–S3.3（待整体实施） | 复用既有 replay、资源、prefix、usage 和投影基础；当前 S1/S2 的共享修改不是零基础，也不等于 S3 交付 | 逐格式附件/依赖、资源双端引用与原报告归属、具名损失及消费者闭环仍按各片退出条件实施 |

**M1 不由 S2 单独判定**：仍须闭合 S0 所需具体样本与 S1.2 动作内容，并核对所有前序及 E01/E02/E04/E06 的 M1 子范围；S2 的组合场景不能覆盖其他子片的缺口。**M2 尚未闭合**：S3 与必要消费 gates 不因前序库级实现而视为完成。

验证层次不能混用：确定性 Core、现有 wire 回归与离线基线按开发指南执行；固定 SDK/其他外部消费者和真实 Provider 验证是单独门槛，没有对应执行证据不得标记通过。新 Provider 观察、未实现关系 carrier 和非标准 usage 口径在现有目标仍明确拒绝，测试存在不证明 HTTP 准入或上游接受。

### 依赖顺序

```text
S0 验证承诺 / 独立预期
  → S1.1 身份与关系 → S1.2 Provider 观察与关联
  → S2.1 ClientManaged / 配置 → S2.2 控制、Schema、Usage → M1
  → S3.1 Replay ──────────┐
    S3.2 资源、引用、cache ├→ S3.3 具名投影 / 消费闭环 → M2
                         ┘
```

证据整理可提前并行；公共 identity/group owner 未定稿前不并行重写互相依赖的类型。S0 的确定性验证修复不等待全部原生资料；每片只等待自己实际依赖的证据。既有 wire 回归贯穿所有阶段。

### S0：基线、验证承诺与源证据

- **进入**：读取实际 HEAD、分支、工作树和目标 diff；检查现行合同、工具链与锁定依赖。固定来源不是执行时最新 HEAD，不能自动回退仓库。
- **实现**：A01 检查 `with_output` / `with_reasoning` 与其他 validated 方法承诺一致；非法修改返回错误，合法修改保留 presence。按实际签名更新调用者，不能大面积用 `unwrap()` 掩盖输入错误。
- **证据**：先选混合 client/provider 工具、跨响应结果、空可见 reasoning、配置变化、usage/cache 口径与显式历史投影的代表性资料；有冲突的 native 子例暂缓，不伪造 wire。
- **退出**：A01 正反例先红后绿；若已修复则复核现有覆盖，不重复实现。A02 分层边界与适用 T 基线定位明确，执行受影响检查及开发基线，区分既有失败和本次回归。
- **非目标**：新协议、通用 manifest 平台、持久化、替换全部 JSON 类型、路由或凭据变更。

### S1.1：身份、修订与异构关系

- **前提**：AGC-01/02 的唯一 owner 与 native 引用域明确；本地 scope/ID 分配为显式输入。
- **实现**：typed scoped references、native alias、参数修改的新调用身份；异构组与有限关系；旧反向字段改为派生或迁出，不能维持双权威。
- **退出**：A03–A08 的本轮断言、T06/T16/T21/T28 相关部分；跨域同名不冲突，同引用域冲突诊断；重排保持身份，删除不复活，候选隔离。
- **迁移**：同步现有 decode 局部 ID 的确定性 lift、source records、静态/事件和分组投影；不激活多候选。

### S1.2：Provider 工具观察与跨响应关联

- **前提**：S1.1；局部观察与解析后保证分开，Provider 工具身份/责任来源已声明。
- **实现**：调用责任、可缺省执行报告、实际结果和 artifact 状态；R2 结果可解析到 R1；补足规范工具结果事件，不以终态 snapshot 补内容。
- **退出**：A09–A11 的纯观察部分、T11/T12/T18 与 E01 的结构/责任场景；provider call 不进入客户端结果要求，缺失/歧义不猜测。
- **非目标**：工具执行、自动下一请求、Google/Anthropic decoder、远端操作恢复。

### S2.1：ClientManaged 上下文与配置修订

- **前提**：S1 的引用与组结构；有限的配置依赖和显式历史选择合同。
- **实现**：纯上下文构造、完整本次配置快照、选定历史/追加结果/外部编辑、变换诊断与续接要求；直接 typed 构造不要求 session 对象。
- **退出**：A12–A15 的 ClientManaged 部分、T23/T24；E01 的后继请求构造、E06 的纯消费者路径。没有客户端结果要求不推导 turn finished；Provider continuation 不变成本地动作。
- **非目标**：ServerManaged/Fresh 状态机、session tree/JSONL、自动摘要、token 裁剪策略、自动继承配置。

### S2.2：控制、Schema 与计量

- **前提**：配置 owner；每个新增控制与计数关系有固定反例，不用“某厂支持”代替语义。
- **实现**：reasoning mode/effort/硬软预算/显示的组合；Schema 方言、strict 缺省、受控引用与独立预算；输入/cache 关系和具名 usage view。
- **退出**：A16–A18、T09–T12/T15/T20 的本轮部分；硬上限不能换 effort，未知不补零，累计不重复相加。外部 Schema `$ref` 不联网。交付 M1 的本轮范围。

### S3.1：Replay 与受保护编辑

- **前提**：身份/组/配置 owner；所选格式的附件位置、依赖与最终化规则明确。
- **实现**：item/part/group 的 typed 唯一载荷与绑定；格式、来源、scope、依赖和 partial/final 检查；合法闭合前末尾材料与闭合后权威区分。
- **退出**：A20/A22、T04–T08/T13/T27/T32 与 E02/E04 相关断言；依赖外编辑不误拒、依赖内编辑失效、删除不复活、公开诊断不泄漏。
- **非目标**：issuer 验签、OpaqueStore、跨请求认证证明服务、闭合后迟到 opaque 新规则。

### S3.2：资源、引用、cache 与原报告归属

- **前提**：依赖模型；Text/Image/File 的明确用途与坐标反例。
- **实现**：资源 identity/locator/用途、来源条件与双端引用；显式外壳快照用于权限/期限检查；cache breakpoint/prefix 依赖和报告来源。
- **退出**：A18–A21、T19/T28；保留原 usage 操作归属；源/输出编辑分别重验；相同 URL/key 不证明相同资源权限或 replay 资格。
- **非目标**：下载、上传、转码、病毒扫描、文件 ID 服务、自动跨 Provider 资源复制。其他媒体只作现有共享边界回归。

### S3.3：具名投影与现有接口闭环

- **前提**：前序实际依赖；每条新损失规则已在 owning contract 定稿。
- **实现**：返回投影值、规则/owner/方向/后果、剩余前提与修订；复用旧投影或绑定前重新核对。旧合法 Chat 规则保持，未知规则仍拒绝。
- **退出**：A23/A24 的纯计划部分、T20/T23/T25/T29，E02/E04/E06 的本轮范围；全部受影响既有 wire、失败与消费者门槛闭合，交付 M2。
- **非目标**：新审批/执行许可对象、资源准备 I/O、Google/Anthropic 接线、替换产品 catalog。

### S4 / S5：明确延期

S4 的 Agent 执行、持久化、checkpoint、claim/CAS/fencing、故障恢复全部延期；原子切片编号不再是本轮依赖。S5 的 Google Interactions、Anthropic Messages 及 generateContent 实现/激活也延期。恢复须另定目标、profile、责任与独立验收，不凭本文或资料存在开始实现。

可考虑在现有 Responses 中独立选取一个 Provider 工具**输出观察**分支，例如 web-search action/status 与答案引用，验证真实 wire→IR/事件与反向 carrier。它不是 M1 的前置；若未选片就维持拒绝，不能宣称本轮已有原生支持。即使实施该输出分支，也不自动开放请求工具配置、真实工具执行或产品激活。

## 5. T01–T32 的本轮归属

下表是验收需求，不是执行状态。每行必须按适用方向保留正例和最小反例；“延期”不等于通过或 N/A。Google/Anthropic 原生 W/K/C 一律不属于本轮，表中不逐行重复。

| ID | 本轮必要断言 | 归属 | 延期或限定 |
|---|---|---|---|
| T01 | Responses function call/result 类型、ID 与关联保持 | S0/S1，W | 不推定其他协议接入 |
| T02 | 连续同角色记录可表达；显式合并承认不可逆边界 | S1/S3 | 不实现 Messages 合并 codec |
| T03 | Interactions Step 与 generateContent Part 的语义来源/附件位置不混用 | S0/S3 | 仅设计证据与 typed 反例，不造两个假 codec |
| T04 | 空可见 reasoning 不删除实际报告的 opaque | S3.1 | 合成材料不证明服务器接受 |
| T05 | call/media part 上的 replay 保持原 owner | S3.1 | generateContent 仅独立反例 |
| T06 | 声明依赖内的内容/顺序编辑失效，依赖外编辑不过度拒绝 | S1/S3 | 不声称厂商密码学验签 |
| T07 | 未知跨模型/版本兼容不自动回放 | S3.1 | 实际兼容允许分支须精确证据 |
| T08 | 允许分支按明确兼容规则与 scope，未知则拒绝 | S3.1 | 缺证据的真实跨平台回放延期 |
| T09 | Raw 原文与 structured 精确数值各守权威 | S0/S2.2 | 字节保真只限实际捕获单位 |
| T10 | 非法参数可观察；完整 envelope/严格解析拒绝歧义键 | S0/S2.2 | 不建设重复键归档 AST |
| T11 | 完整但无效参数保留观察，不成为合法可用参数 | S1/S2 | 工具执行 gate 延期 |
| T12 | 合法参数分批组装一致，partial 不提前完成 | S1/S2/S3 | 字节 framer 与语义批次分别测 |
| T13 | 可见内容结束后、owner 合法闭合前的末尾材料保留 | S3.1 | 闭合后权威待决分支仍拒绝 |
| T14 | 流内 error/非法 EOF 不伪造成功 | S0/各事件触点，W | 不实施自动恢复 |
| T15 | 累计 10/20/30 按作用域最终为 30 | S2.2/S3.2 | 不用 token 推算账单 |
| T16 | 同名/同 alias 歧义不猜关联 | S1 | 不按最新/邻接匹配 |
| T17 | 结果未知作为独立观察，不改称失败或成功 | S1.2 | 真实工具超时、核对和重试延期 |
| T18 | Provider 已执行结果不变成本地待执行调用 | S1.2 | 不启用真实 hosted 请求 |
| T19 | 资源 scope/期限/权限快照不满足则拒绝表示或保留未满足要求 | S3.2 | 下载、上传和重建资源延期 |
| T20 | 硬 Schema 不能弱化后声称满足；具名允许损失明确报告 | S2.2/S3.3 | 不引入全局近似开关 |
| T21 | 候选作用域隔离、不拼接；限定单候选明确拒绝 | S1/S3 | 多候选产品启用延期 |
| T22 | 未知数据不覆盖核心、不自动执行或转发；可保留范围受预算约束 | S0/S3 | 不要求新通用 raw capture |
| T23 | authority/scope/phase 不提升、不静默弱化 | S2/S3 | 新业务意图不能沿用旧证明 |
| T24 | ClientManaged 不混入主动 previous-ID/conversation | S2.1，W | ServerManaged 合法分支延期 |
| T25 | 源值/目标/profile/配置改变使旧投影或绑定重验 | S3.3 | 工具执行许可和上传后授权系统延期 |
| T26 | 真实重复文本保留；现有有依据的帧规则独立回归 | S3/既有 transport | 跨重连游标/去重恢复延期 |
| T27 | opaque 不泄漏到 Debug/UI/普通日志或异源请求 | S3 | 不建设审计存储 |
| T28 | 显式组、来源和变换保持；没有来源映射不假称恢复原边界 | S1/S3 | 不以 SourceMap 重写正文 |
| T29 | 多段损失累积且端到端重验，候选互不污染 | S3.3 | 不放宽未批准规则 |
| T30 | 同一合法事件序列不同语义分批结果一致 | S1/S3 | 持久日志/checkpoint 重放延期 |
| T31 | 取消/失败后 reducer 不复活，现有交付竞争回归 | S3/既有 transport | 工具 worker 与恢复竞争延期 |
| T32 | 原文未保留或已脱敏时据实降低保真声明 | S0/S3 | 不为满足字节声明新增捕获系统 |

## 6. A01–A32 的本轮归属

| ID | 本轮必要断言 | 归属 | 延期或限定 |
|---|---|---|---|
| A01 | validated 修改统一重验，合法修改对照与 presence 保持 | S0 | 现有修复可复用，不重复改写 |
| A02 | 结构合法、目标表示、实例准入分别诊断 | S0/各片 | 新 IR 不自动激活入口 |
| A03 | scoped alias 跨域复用、同域冲突、缺失/歧义引用 | S1.1 | 恢复后的编码一致性延期 |
| A04 | 重排/插入保持 surviving identity，派生视图不改权威 | S1.1 | 持久化保存/读取延期 |
| A05 | 参数编辑产生新调用及派生来源，旧结果/证明不可移挂 | S1/S3 | 执行许可服务延期 |
| A06 | 删除 owner 后索引/fidelity 不恢复正文，悬空关系诊断 | S1/S3 | 不删除原始观察存储 |
| A07 | Text/Reasoning/client/provider call 异构组与 replay 关系共存 | S1/S3 | 目标连续性另验 |
| A08 | result reference、group membership 各有唯一可写 owner | S1.1 | 不保留双写兼容层 |
| A09 | 混合 C/S 只要求客户端提交 C，参数不能伪造责任 | S1.2 | 不构造执行队列或运行工具 |
| A10 | R2 的 S 结果在 ClientManaged 历史唯一解析；缺失/歧义分别诊断 | S1/S2 | 受信服务器历史分支延期 |
| A11 | 正文、执行报告、产物状态分离；未报告不补成功，未知后追加事实不抹旧观察 | S1.2 | 外部核对与重启延期 |
| A12 | 空历史首请求与完整选中历史使用 ClientManaged；主动远端引用拒绝 | S2.1 | Fresh 独立状态机与 ServerManaged 延期 |
| A13 | 配置/工具定义修订变更按声明依赖重验，不猜继承 | S2.1 | 恢复时默认配置变化延期 |
| A14 | 客户端结果要求、Provider continuation 与 reported progress 分开；空集不等于结束 | S2.1 | 部分提交只按已声明 profile |
| A15 | 指令层级/scope/phase 与来源不被拼接或内容文字提升 | S2/S3 | 没有等价载体则拒绝 |
| A16 | mode/effort/硬软预算/显示/presence 的合法组合与拒绝 | S2.2 | 不做预算效果校准 |
| A17 | Schema 方言、strict、受控引用、数据 nesting 分别验证 | S2.2 | 不联网解析 `$ref` |
| A18 | native 未缓存输入=10、缓存读=4096 的声明关系可表达；缺项不猜总量，编辑不改原报告 | S2.2/S3.2 | 不假设所有 input 都含 cache |
| A19 | cache hint、断点、前缀、TTL 意图与命中事实分开，删除不复活 | S3.2 | 不实现缓存服务或收益测试 |
| A20 | opaque-only、summary 内容、附件位置、依赖内外编辑与最终化 | S3.1 | 闭合后权威仍延期 |
| A21 | 工具/推理/产物用途、locator、坐标单位、源输出两端依赖与 supplied scope/期限 | S3.2 | 不执行资源准备 I/O |
| A22 | Typed 唯一载荷、raw 冲突隔离、删除不复活与脱敏输出 | S3.1 | capture 未启用不承诺原字节 |
| A23 | 精确/具名损失/未知规则、组合损失及候选隔离 | S3.3 | 不重做固定 route 选择 |
| A24 | 源/profile/配置/资源条件变化使旧投影或绑定不可复用 | S3.3 | 上传、执行许可、授权恢复延期 |
| A25 | 远端 get/resubscribe/continue/cancel/delete 运行合同 | 延期 | ServerManaged/远端操作另行选片 |
| A26 | 半参数/opaque/frame checkpoint 与持久水位恢复 | 延期 | S4 未恢复，不伪装进程内测试 |
| A27 | 真实重复文本不去重、累计量不重复相加 | S2/S3 | event-ID 重送恢复、动作重放延期 |
| A28 | attempt 发送前后崩溃、未知结果恢复 | 延期 | 需要工具执行与持久化范围 |
| A29 | 多 worker CAS/claim/租约/fencing | 延期 | 需要外部副作用与存储合同 |
| A30 | 同一 response 终态不可重开，矛盾终态有诊断 | S3/既有 transport | 跨进程迟到工具核对延期 |
| A31 | 既有发布/提交后禁止 fallback 的回归 | 既有 execution/transport | Provider 副作用恢复策略延期，相关请求不因观察支持而激活 |
| A32 | 敏感材料不因 Debug/诊断泄漏；普通 scope 不自授权 | S3 | checkpoint 版本、损坏恢复、principal 迁移延期 |

## 7. E01–E06 的本轮验收

| ID | 本轮范围 | 退出条件 / 延期部分 |
|---|---|---|
| E01 | 混合 client/provider 调用、ClientManaged 后继请求、跨响应结果 | 下述纯库闭环；无真实工具或模型 I/O，原持久化步骤延期 |
| E02 | 不可变源上的两个派生上下文，编辑/资源/replay/引用/usage 检查 | 一分支的删除、替换、重排和配置变化不污染另一分支；恢复步骤延期 |
| E03 | 远端操作读取、checkpoint 与取消恢复 | 整体延期；现有 EOF/取消单元回归仍由 T14/T31 承担 |
| E04 | 参数/opaque/usage 事件分批、合法末尾材料、错误与预算 | 人工静态 oracle 一致，无 snapshot 修补；持久恢复分支延期 |
| E05 | 工具副作用不确定性、多 worker 与真实重启 | 整体延期；纯观察未知状态由 A11/T17 承担 |
| E06 | 同一纯库消费者处理三协议来源的语义场景；现有接口回传 | Google/Anthropic 只验 typed 合同，不调用假 codec；新原生/SDK/服务验证延期 |

### E01 的最小闭环

1. 用明确配置修订与客户端历史构造请求，不发送。
2. R1 包含文本/reasoning、客户端调用 C、Provider 调用 S；保持 identity 和显式组，S 不据出现就视为已执行。
3. 检查只对 C 提出客户端结果要求；S 的状态/后继要求单独保留。
4. 由纯库构造已报告 Result(C)，明确来源，不能以构造行为证明真实工具执行。
5. 构造后继请求；历史、配置、replay、资源前提有缺失则诊断，不自动补结果或发送。
6. R2 仅报告 S 的结果；解析到 R1 的 S，不重建调用。对缺失历史、alias 歧义、错 kind、配置漂移分别给反例。

### E02 / E04 的重点

E02 同时检查源观察不可变、参数编辑新调用、结果不移挂、来源/输出双锚点、原 usage 的操作归属与依赖外编辑不过度失效。外部摘要必须是新派生内容，不能继承原 authority/opaque。

E04 将字节切分与语义事件分批分别验证；包含 UTF-8/转义截断、无效参数、严格 EOF、累计 usage、unknown event 和总缓冲超限。闭合前合法末尾 signature 与闭合后改写权威分开，后者维持当前拒绝。不得先发布成功终态再撤回。

### E06 的完整性判据

每个设计反例应有正例证明可表达，不得只靠拒绝宣称语义完整。对于已实现的 Responses/Chat，再独立验证实际编解码与适用消费回传；无标准 carrier 的新语义仍可在纯库保留并明确拒绝公开投影，不恢复 `_openbridge` 或依赖 SDK unknown fields 幸存。

## 8. Gates、迁移与交付

| Gate | 范围 | 阻断条件 |
|---|---|---|
| G0 | 合同/计划/fixture 的来源、编号、链接、许可与敏感信息 | 失效 owner、假来源、范围冲突、把规划写成已实现 |
| G1 | 受影响 D/P/W 与独立正反预期 | 零匹配、关键负例缺失、无声丢字段、越权或无界处理 |
| G2 | 代码变更的 locked/offline Rust、clippy all-targets、fmt、diff 基线 | 本次回归、无法解释失败、通过放宽预算/锁定绕过 |
| G3 | 受影响且明确需要的固定消费者 K、OpenAPI 或 TS/Python 检查 | 回传丢关联/replay、依赖私有字段、消费者结论无执行证据 |
| G4 | 按任务范围补充的 C | 目标/矩阵/限制或授权不满足；不能用 C 代替离线回归 |

精确命令只由[开发指南](../development.md)维护，不复制一套可能过期的命令。原生协议/实例启用不由 G1/G2 通过推定；G3 缺失只影响对应消费声明，不阻塞无依赖纯库工作。

真实 Provider 验证适用 [AGENTS standing authorization](../../AGENTS.md#standing-authorization-for-live-provider-verification)，不额外设置逐调用批准或金额上限；仍必须按 [probe operations](../probes.md)限定有限矩阵、请求/输出/资源、deadline、取消清理与脱敏。SDK loopback、部署、凭据生命周期和真实工具副作用不从这一授权继承权限。本文不要求为文档或纯库里程碑调用真实服务。

### 迁移与回滚

- 一次可编译变更更新受影响内部类型、显式 re-export、调用点和独立预期；不保留双权威旧字段或无必要兼容层。
- 保持现有严格公共入口，新增观察入口不作为宽松 HTTP 后门。新增结构不自动增加产品 capabilities。
- 没有实际持久化数据时不建设迁移平台；本轮不定义 checkpoint 格式。已有进程内 digest/Debug 不升级为保存协议。
- 不丢弃用户工作树，不因回滚代码重发未知状态的请求、删除观察或改变 activation。实际数据删除、部署和 Git 提交/推送分别受授权约束。
- 发生新 IR 缺口时给出最小反例、owner/状态/事件/迁移选项；只暂停依赖该缺口的子片，不用 adapter、generic JSON 或无断言测试绕过。

### 后续实施仍需固定的细节

| 事项 | 决策时点 | 未满足时的处理 |
|---|---|---|
| 所选 Interactions 版本、Messages 功能与事件证据 | 对应 S0 样本 / S1–S3 合同前 | 不混用 v1/v1beta 或 generateContent；仅阻塞依赖分支 |
| 新增动作/参数与既有 identity、关系 owner 的衔接 | S1.2 后续子片 | 复用既有 scoped identity 与唯一关系 owner，不私建第二权威 |
| Provider 动作内容与具体原生引用域映射 | S1.2 后续子片 | 独立观察分支与纯 alias 解析不代表原生映射；缺失/歧义不按名称猜，不启用相应请求 |
| 新 Provider 的配置依赖与变换条件 | 对应原生接入子片 | 复用 S2 的配置、原定义、变换诊断与显式续接要求；无原始关联不从当前定义推断，不将进程内 proof 当外部认证 |
| 控制/Schema 的具体目标映射 | 对应 profile 子片 | 复用 S2 owner 与固定来源；硬约束不近似满足，不删除 Schema 资源/方言来迁就目标，不扩大未选 native 分支 |
| 每格式 replay 依赖、资源用途/坐标与新损失规则 | S3 对应子片 | 维持当前拒绝，闭合后 opaque 争议独立延期 |
| 可选 Responses Provider 工具输出分支 | 独立行为选片 | 未选就不新增原生支持声明，不阻塞抽象核心 |

不需要为上述细节重复确认已定的 ClientManaged 范围；源码/API 命名可由实施者在合同内选择。新的行为取舍或来源冲突才需要明确决策。

### 单片交付内容

报告实际基线/最终工作树、合同和 owner、迁移影响、case→真实符号、红绿证据、已跑命令/计数、最终 diff、既有失败/环境阻塞与未测层。不得把引用存在、编译成功、目录项数量或少量 Provider 成功当完整验收。代码未实施时明确只交付文档。

## 9. 来源入口

- 三协议规范出处与引用许可：[Generation 合同来源](../architecture/interaction-contract.md#来源)、[固定上游来源](../references/upstream-sync.md)、[Provider 参考入口](../references/providers/README.md)。
- ClientManaged 方法：[Pi 固定上下文投影参考](../references/pi-provider-abstraction.md#client-managed-projection)，与旧 Provider/认证参考分版本维护。
- 验收方法：[conformance baseline](../references/conformance-baseline.md)、[development](../development.md)。
- 延期项与实际缺口：[next-goal](next-goal.md#延期目标与恢复条件)、[generation gaps](../implementation-status/generation.md)、[open questions](../implementation-status/open-questions.md)。

动态网页只作为固定证据的入口，不能将研究材料的核验日期当永久兼容保证。原 T/A/E 编号用于需求追溯，不在仓库保存旧计划全文或运行历史；接受的合同以各 owner 为准。
