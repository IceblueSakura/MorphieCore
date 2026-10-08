# 后续计划

## 当前主线

目标是**正确接入多个 Provider API，经独立 Semantic Model / IR 向下游提供标准规范的 OpenAI Responses 接口，简化下游 Agent 的适配细节**。Gateway 与未来 Agent 复用同一语义权威；[ClientManaged 交互合同](../architecture/interaction-contract.md#client-managed-context)作为选片与回归基线。请求型音频保持独立 Speech/Transcription task；已有 Text/Image/File、标准文本/function、类型化消费和独立媒体作为维护与回归边界。不以增加 Provider、模型数量或模态枚举作为进度。Chat Completions 是有限有损的兼容投影，不反向限制共享核心。

Agent runtime 与恢复不列入本项目的实施或延期目标；职责边界归 [Semantic Model](../architecture/semantic-ir.md#1-语义权威与消费者)。纯工具观察、结果关联与请求型生命周期不因此删减。

**先功能，后稳定性专项。** 近期优先正常功能、标准消费与必要回传；不主动扩展上游 HTTP 失败、非标准 JSON/解压失败、断流或慢消费者等异常矩阵，不将其作为新增功能的前置。现有认证、预算、取消、严格终态及拒绝回归继续保留；实际阻断所选功能的问题按最低 owner 修复，不借机扩展通用重试、调度或诊断框架。

<a id="strict-verification"></a>

**Strict 约束的验证边界。** strict JSON Schema 与 strict function tools 暂仅执行静态/离线验证，包括 pure IR、codec/lowering 及固定 SDK 的 synthetic loopback 组合；不执行真实 Provider 的 strict 能力/adherence 验证，也不以其稳定支持作为当前退出条件。保留 strict/default、Schema 顺序/精度、参数/结果关联与未准入拒绝，不删除约束或改变公共合同。既有 strict live probe 保留但暂停执行；未来恢复须重新明确范围。普通文本、非 strict 工具/history 与媒体功能仍可按授权独立验证。

本页维护推荐优先级、依赖和选片条件，不是实现完成声明或操作授权。有效合同归 [Semantic Model](../architecture/semantic-ir.md)，已定稿行为切片及其当前状态只归 [current-focus](current-focus.md)，实际缺口归[实施边界](../implementation-status/generation.md)，等待证据的问题归[待决状态](../implementation-status/open-questions.md)。

## 推进顺序与退出条件

以下是**选片优先级，不是整块实施阶段**。每项按最小可观察场景拆分；前项只需闭合后项实际依赖的边界，不要求先补齐整个 Responses 标准。已有基础层和独立覆盖直接复用，不重复安排“重建 IR / Gateway / 凭据管理器”。

**ClientManaged 选片边界。** Responses、Interactions、Messages 共同提供语义反例；既有具名投影、资源/引用、身份/上下文/配置/控制/计量、所选 Provider 动作、Replay 附件与纯核心/消费者组合场景作为输入与回归，不再重复排期。当前没有自动进入的后继阶段；新差异按[选片条件](#后续选片条件)定稿。Google/Anthropic 原生实现与 ServerManaged 不自动恢复；不能因此删去 Provider 工具结果或必要生命周期语义。既有 Responses/Chat 随片迁移，不等待整个标准完成才修复 codec。

**请求型音频转为维护与后续独立选片。** 基础 task、标准请求、有界交付及目标映射按 [Speech](../architecture/speech-profile.md)和[Transcription profile](../architecture/transcription-profile.md)维护，嵌入/binary 激活归 [HTTP 合同](../http-gateway.md)。低延迟交付、下游 SSE、转录扩展与实际服务验收仍可按具体需求独立定稿，不重复建设已有接线，也不成为 IR 的前置。下表为既有接口维护与后续产品选片顺序，不把本轮三协议语义工作缩减为单 function 子集。

**产品选型顺序：优先 OpenRouter 上采用标准 OpenAI 协议的模型，次选 Token Plan 相关模型。** 这是接入优先级，不是请求内自动 fallback，也不限定模型研发者必须是 OpenAI。按具体 operation 核对协议、默认值、控制与产物；“OpenAI-compatible”不能替代逐项合同。必要差异仅在受信 profile 中具名映射，保持标准下游与共享 IR，不把不支持的控制静默丢弃。Token Plan 的套餐准入和所需原生协议分别核对，不与普通按量端点或凭据互换。来源入口见 [OpenRouter](../references/providers/README.md#openrouter) 与 [Model Studio](../references/providers/README.md#alibaba-cloud-model-studio)；此顺序不授权真实调用或私有 activation。

| 优先级 | 目标与产出 | 退出条件与非目标 |
|---|---|---|
| **1. Responses 核心功能** | 维护文本/function/history、reasoning、Structured Outputs 与已有图片/基础文件输入回归；仅按具体新消费差异扩展 | 所选功能形成 wire→IR→标准输出→必要回传闭环，不以接口接通或拒绝测试通过代替功能完成；不一次补齐完整 union |
| **2. 多 Provider 标准消费** | 代表性原生 Responses 场景作为接入回归；新增目标或实际差异按最小功能独立选片，差异归受信 adapter/profile | 除模型及明确支持的控制外，消费者不需要 Provider 专用解析或 history 重写；不按 Provider/model 数量铺测试矩阵 |
| **3. 非原生上游转换** | F5 先解决 Chat→Responses 工具分组与回传合同，再实施最小映射和显式接线 | 静态、事件与回传一致；现有 Chat 损失许可不自动适用于 Responses，不以私有字段或静默丢关系绕过决策 |
| **4. 稳定性专项** | 核心功能收敛后，按实际使用问题选择异常、负载与长期运行边界 | 不提前扩张；正常功能中已遇到的阻断问题可随片修复，现有防线始终保持 |

**依赖关系：**第 1、2 项按实际功能交错推进，不等待完整标准；第 3 项的设计论证可独立进行，行为实施必须先闭合所选映射合同。类型化消费与必要回传从首片开始，纯库检查不等待账户资格。标准模型发现保持维护，不重新建设；SIWC 采用按下节独立处理，不阻塞 API-key 路径。其他资源与媒体按具体消费需求独立选片，不作为本表前置。

**文件重评节点：**在下一次文件产品行为选片前评估是否继续延期，不等待所有模态完成。没有新的恢复决定前，仅维护既有 Responses user inline/URL 基础输入及必要正确性、安全边界；重评不自动恢复 issuer-bound ID、工具文件结果、生成文件、更多格式/目标、Chat 文件投影或 `/v1/files` 服务。本轮 IR 中资源 identity、用途、locator 与引用坐标的纯语义完善不受这一产品延期阻塞，也不因此激活新文件承载。

## 近期实施单元与停止点

以下编号仅定位剩余工作，不是完成记录。先核对已有覆盖；已经满足的单元复用证据，不为凑阶段重复改代码。每次只在 current-focus 定稿一个实际行为缺口，采用独立失败例→最低 owner 修复→受影响回归的顺序。

文本、单/多 function/history 与 reasoning 的基础交付—保存—追加—回传作为维护和后续输入，不重复排期。独立预期归 [history continuation](../../tests/semantic/history_continuation.rs)、[reasoning](../../tests/semantic/reasoning.rs)与[固定 SDK gate](../../tests/sdk_loopback.rs)；reported final opaque 与未报告 opaque 分别验收，现行 scope、finality 和编辑失效边界不变。

Structured Outputs 的 strict/default、Schema 顺序/精度与已有图片/基础文件组合复用 [Schema](../../tests/semantic/schema.rs)、[Provider profiles](../../tests/semantic/provider_profiles.rs)及 SDK 回归；strict 场景遵守上文[离线验证边界](#strict-verification)。正常功能 probe 的 JSON object、非 strict 工具 history 和固定基础色/文件场景归[probe 指南](../probes.md)。这些是可复用验收入口，不是动态能力表；真实目标与执行证据每次独立报告，不从某次通过推定完整标准、全部 Provider 或一般输出质量。

| 单元 | 输入、实施范围与 owning sources | 验收与停止点 |
|---|---|---|
| **F5 Chat→Responses 功能转换** | 以 [group projection](../../tests/semantic/group_projection.rs) 和 [Gateway 跨协议预期](../../tests/gateway.rs)为最小反例，分析 message-call membership、顺序、原始参数及回传。现行限制归 [Responses profile](../architecture/responses-text-profile.md#message-owners-and-cross-protocol-grouping) | 先决定合法映射及必要信息的权威位置，再补独立静态/事件/history 预期和最小接线。未定稿时保留拒绝并明确功能未闭合；不将拒绝通过写成转换完成，不自动激活所有 Chat-only 绑定 |

**执行准备与验证顺序：**

1. 核对分支、工作区与目标 diff；读取对应合同和最低 owner 的测试。用 [ClientManaged 用例清单](../../tests/fixtures/client_managed_cases.json)追溯已有覆盖，不复制测试清单或历史结果到计划。
2. 按[开发指南](../development.md#nix-开发环境)使用项目锁定工具链；Node、Python/SDK 版本分别以 manifests/locks 为准。先确认解释器及依赖可用，再执行相关 gate；缺少缓存不以降版本、解锁依赖或全局安装绕过。
3. 先执行所选纯库/codec 检查；行为变化按 TDD 同步实现、profiles、OpenAPI 与受影响 fixtures，再跑[规定基线](../development.md#rust-检查)。固定 SDK/其他外部依赖 gate 需单独取得相符执行授权，计划本身不替代授权。
4. live run 只通过[受控 probe](../probes.md#先计划后执行)。执行前现场固定目标、场景、请求上限、输出/资源上限、deadline、取消清理与脱敏范围；读取凭据仅走现有 loader。不在计划中保存动态模型库存、私有 activation 或运行结果；凭据生命周期与实例切换仍需独立授权。
5. 每个单元交付时分别报告功能、标准 wire、消费者、真实 Provider 的证据与未验收层；完成所选边界即停止并清理 focus，复审后续单元，不自动滚动进入整个列表。

Agent Loop/Runtime 仅在用户后续阅读 Pi `AgentSession`、明确复用边界后另行规划；本表不安排该调研或实现。固定 SDK/Agent 消费验收不等于建设 Agent runtime。

<a id="下一片候选标准模型发现"></a>

## 后续选片条件

后续依据具体消费者反例或实现缺口，按[交互合同](../architecture/interaction-contract.md)选择实际依赖已闭合的子片，在 current-focus 定稿输入、输出、非目标与独立失败预期和停止点；产品优先级按上表选片。不把缺口清单直接当排期，也不以完整 Responses union、更多模态或 SIWC 为前置。来源冲突或新的 IR 缺口只阻塞相关范围，扩大公共类型或迁移边界前先说明影响。

1. 标准文本/function、工具与 opaque 回传、类型化编辑、已有图片和基础文件输入随片回归；新发现的真实差异按最低 owner 修复，不把已有基础层重新列为待建设任务。
2. 模型发现的字段来源、激活视图与拒绝边界归[HTTP 合同](../http-gateway.md#标准模型发现)。后续消费者差异须有具体反例，不假定 Agent 会自动发现，也不直接透传上游/账户目录。
3. 丰富 `/models` 的路径/schema、价格目录、自动模型选择与路由调度仅在恢复决定后独立选片。公开信息范围仍有实质选择时先报告方案，不以 placeholder 或私有扩展掩盖缺口。

<a id="siwc-迁移的内部顺序"></a>

## SIWC 采用与实例切换

[SIWC 参考](../references/siwc-login.md#本项目采用与实施边界)拥有来源与采用边界；credential 生命周期与操作归 [凭据指南](../credentials.md)，独立请求准入归 [adapter](../../src/adapter/siwc.rs)，工具分组与限定选择边界归 [Responses profile](../architecture/responses-text-profile.md#tool-namespaces)。后续实例采用分别处理：

1. **资格与合同确认**：限定本人单用户、同一应用、本地 credential owner 与本地执行；核对适用条款、所选 flow、权限与预算控制。远程持久化/复制的条款疑问未解决前，不安排分布式 token 部署。
2. **实际 registration 与权限**：真实登录、refresh/revoke 分别取得相符授权，使用显式自有本地 store；identity-only 不发推理，不从产品 token 或第三方 auth cache 导入权限。合成验证不能代替 authority 接受或原生持久化验收。
3. **模型与消费闭环**：显式核对所选 registration 的模型/operation，分别检查标准输入、实际工具调用/结果回传、额度错误与 JSON/SSE 严格终态。账户模型发现按 [AGENTS.md](../../AGENTS.md#standing-authorization-for-live-provider-verification)的调用授权执行，不直接变成公共目录。真实 gate 需适合 SIWC 的独立预算合同，不能套用要求上游 output-token cap 的通用 probe。
4. **受控实例切换**：仅在取得具体授权后更新私有 activation。旧材料清理、远端 revoke 和私有格式迁移分别处理；不把代码迁移视作删除文件、操作凭据或部署授权。

## 延期目标与恢复条件

| 方向 | 当前边界与恢复条件 |
|---|---|
| 文件扩展 | 按上文选片前重评；先有具体消费场景与资源/issuer 合同，不为基础输入补建完整文件服务 |
| 高级图片 | 维持 `/v1/images/generations` 基础静态生成与有序产物；SSE、预览/最终产物事件、编辑、蒙版、参考图、URL 下载和文件服务延期。恢复时独立选片，先完善所选 API 再追加模型验证，不自动扩大测试集合 |
| Embedding、其他请求型音频与独立媒体 operation | 基础 Speech/Transcription 按现有合同维护；其他分支分别确定最小 task、输入/产物及资源范围。Embedding 保持独立 `/v1/embeddings` 目标；独立任务不强塞 Responses |
| Audio Realtime | 明确要实现；在请求型范围收敛并独立选片后展开协议与生命周期设计，不预建状态机或把音频重构塞入近期切片 |
| Google Interactions / Anthropic Messages 原生实现 | 本轮仅作为设计来源与 typed 合同反例，不新增 codec、adapter、SDK 或网络接线；后续有具体目标与消费者需求时固定 profile 独立选片 |
| ServerManaged 上下文 | 待后续评估；当前只实施 ClientManaged，不预建 previous-ID、conversation、连接级历史或不可用模式。现有主动状态化分支拒绝继续保持 |
| Reasoning opaque 闭合后权威 | 只按[待决问题的恢复证据](../implementation-status/open-questions.md#恢复选片所需证据)重评，不作为其他任务前置，不预建更新事件、严格校验器或回放服务 |
| 丰富模型发现与调度 | 更丰富 `/models` 的路径/schema 另定；标准目录不引入价格、成本路由、动态 registry、负载均衡或自动 credential refresh |

## 每个切片的执行与验收

1. **定稿范围**：只解决场景实际依赖的缺口；按 [IR 缺口规则](../architecture/semantic-ir.md#4-ir-不足与标准载体缺口)区分语义缺失、目标无载体与未接线，结构选择先报告 owner、合法状态、事件/依赖和迁移影响。
2. **独立失败预期与类型化消费**：先 TDD，再同步验证纯库构造/检查/编辑及适用的标准输入输出；不复制第二套 IR，不把内存 fingerprint 当持久化协议。
3. **贯通交付与必要回传**：在最低 owning layer 保护 presence、顺序、精度、身份、依赖、预算、取消和真实终态；适用时覆盖 JSON/SSE、实际 body handoff 与续轮。必要 opaque 分别验证“实际报告且回传”和“未报告”，后者不能冒充前者。
4. **必要 Chat 兼容随片完成**：只应用逐条定稿的有限损失规则，未定稿时维持拒绝；保护指令/工具行为、call identity、必要 replay 与终态。规则在对应兼容路径默认应用，不逐请求另加开关；`phase` 等 optional 信息不自动视为安全损失。静态/事件一致，投影不能污染核心或其他候选。
5. **同步与交付**：类型、序列化、profile、OpenAPI、fixtures 与消费者按实际影响一起更新；执行[开发基线](../development.md)和所选独立 gates，报告未验证层。闭合后清理 current-focus 与对应缺口，不保留完成日记。

允许在明确迁移范围内破坏性重写，不要求旧 API/类型兼容垫片，也不授权无关全库重写或数据丢弃。迁移不恢复独立 `_openbridge`、隐式兼容入口或同义私有字段；未来是否重建扩展须重新决定。Provider 原生缓存、计量、Schema、凭据与运行保障只按场景依赖维护，不借规划扩张基础设施。

真实登录、账户发现、付费推理、部署、凭据迁移与提交均需目标和效果相符的授权；代码或 synthetic 检查通过不证明外部准入、真实质量或生产就绪。计划不记录动态模型库存或执行结果。
