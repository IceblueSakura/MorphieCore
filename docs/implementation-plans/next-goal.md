# 后续计划

## 当前主线

目标是让 **Gateway 与未来 Agent 复用同一套独立 Semantic Model / IR，通过标准 API 完成可验证的模型交互**。当前优先进行[三协议约束下的 ClientManaged Generation IR 完善](client-managed-ir.md)，Generation 仍以规范 Responses 为主要客户端接口。请求型音频保持独立 Speech/Transcription task；已有 Text/Image/File、标准文本/function、类型化消费和独立媒体作为维护与回归边界。不以增加 Provider、模型数量或模态枚举作为进度。Chat Completions 是有限有损的兼容投影，不反向限制共享核心。

本页维护推荐优先级、依赖和选片条件，不是实现完成声明或操作授权。有效合同归 [Semantic Model](../architecture/semantic-ir.md)，已定稿行为切片及其当前状态只归 [current-focus](current-focus.md)，实际缺口归[实施边界](../implementation-status/generation.md)，等待证据的问题归[待决状态](../implementation-status/open-questions.md)。

## 推进顺序与退出条件

以下是**选片优先级，不是整块实施阶段**。每项按最小可观察场景拆分；前项只需闭合后项实际依赖的边界，不要求先补齐整个 Responses 标准。已有基础层和独立覆盖直接复用，不重复安排“重建 IR / Gateway / 凭据管理器”。

**当前优先覆盖：ClientManaged Generation IR。** Responses、Interactions、Messages 共同提供语义反例，按[剩余计划的 P6](client-managed-ir.md#execution-order)推进消费者闭环；既有具名投影、资源/引用、身份/上下文/配置/控制/计量、所选 Provider 动作、Replay 附件与纯核心组合场景作为输入与回归，不再重复排期。Google/Anthropic 原生实现、ServerManaged 和 Agent 执行/恢复不纳入本轮；不能因此删去 Provider 工具结果或必要生命周期语义。既有 Responses/Chat 随片迁移，不等待全部核心完成才修复 codec。

**请求型音频转为维护与后续独立选片。** 基础 task、标准请求、有界交付及目标映射按 [Speech](../architecture/speech-profile.md)和[Transcription profile](../architecture/transcription-profile.md)维护，嵌入/binary 激活归 [HTTP 合同](../http-gateway.md)。低延迟交付、下游 SSE、转录扩展与实际服务验收仍可按具体需求独立定稿，不重复建设已有接线，也不成为 IR 的前置。下表为既有接口维护与后续产品选片顺序，不把本轮三协议语义工作缩减为单 function 子集。

**产品选型顺序：优先 OpenRouter 上采用标准 OpenAI 协议的模型，次选 Token Plan 相关模型。** 这是接入优先级，不是请求内自动 fallback，也不限定模型研发者必须是 OpenAI。按具体 operation 核对协议、默认值、控制与产物；“OpenAI-compatible”不能替代逐项合同。必要差异仅在受信 profile 中具名映射，保持标准下游与共享 IR，不把不支持的控制静默丢弃。Token Plan 的套餐准入和所需原生协议分别核对，不与普通按量端点或凭据互换。来源入口见 [OpenRouter](../references/providers/README.md#openrouter) 与 [Model Studio](../references/providers/README.md#alibaba-cloud-model-studio)；此顺序不授权真实调用或私有 activation。

| 优先级 | 目标与产出 | 退出条件与非目标 |
|---|---|---|
| **1. Responses 标准客户端边界** | 从文本与单 function 工具交互开始，逐项分离标准请求/响应/事件与本地兼容形式；Provider 差异留在受信 adapter/profile | 所选分支具备独立 wire→IR、IR→wire、presence/拒绝和适用事件预期；标准下游不依赖私有字段或 SDK 宽松解析。每片只收敛一个有证据的边界，不同时修改全部控制、计量与事件 |
| **2. ClientManaged 交互与类型化消费** | 在同一场景验证交付→客户端保留→追加工具结果→回传，以及纯库构造、检查、编辑；覆盖 client/provider 工具观察与已选资源语义 | 按[交互合同](../architecture/interaction-contract.md#client-managed-context)保护 scoped identity、原始参数、关联、配置和 replay 依赖；不另建 AgentHistory、Agent loop、工具执行器或持久化服务 |
| **3. 标准模型发现** | 维护[标准 Models 的本实例 public-label 视图](../http-gateway.md#标准模型发现)及消费者边界；新差异单独选片，不重新建设目录基础层 | 认证、显式激活过滤、去重与标准响应有独立预期；不泄露 credential、upstream origin、账户 metadata 或价格。目录不承诺每个协议/控制都可调用；更丰富发现和调度仍延期 |
| **4. SIWC 实例准入与受控使用** | 按下节明确应用/账户资格、实际授权、模型准入与本地实例切换 | credential、权限与目标分别验证，不转换 Codex token 或复用产品 client。外部资格或授权阻塞只暂停此方向，不阻塞标准语义工作；源码、synthetic 检查或一次登录均不证明真实 Agent 闭环 |
| **5. 后续资源与媒体场景** | 依据实际消费者需求重新选择文件扩展、图片高级功能、Embedding 或请求型音频的最小标准 operation | 每次只固定一个 task/operation 的输入、产物、适用事件与资源边界；不一次铺开所有模态。Realtime 保留为明确后续目标，其详细设计不成为近期任务前置 |

**依赖关系：**第 1、2 项按场景交错推进，类型化消费与必要续轮验收从首片开始，不延到“标准全部完成”之后。第 3 项只依赖可信注册、实例激活与 HTTP 安全边界，不依赖 SIWC 上游发现；第 4 项只依赖它实际使用的标准交互，不等待全部文件或媒体能力。纯库、codec 与 synthetic 检查不等待真实账户或付费验证。

**文件重评节点：**在下一次文件产品行为选片前评估是否继续延期，而不是等待表中第 5 项或所有模态完成。没有新的恢复决定前，仅维护既有 Responses user inline/URL 基础输入及必要正确性、安全边界；重评不自动恢复 issuer-bound ID、工具文件结果、生成文件、更多格式/目标、Chat 文件投影或 `/v1/files` 服务。本轮 IR 中资源 identity、用途、locator 与引用坐标的纯语义完善不受这一产品延期阻塞，也不因此激活新文件承载。

<a id="下一片候选标准模型发现"></a>

## 后续选片条件

本轮首先按 [ClientManaged 计划](client-managed-ir.md)选择实际依赖已闭合的子片，在 current-focus 定稿输入、输出、非目标与独立失败预期；后续产品工作按上表选片。不把缺口清单直接当排期，也不以完整 Responses union、更多模态或 SIWC 为前置。

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
| Agent runtime 与恢复 | 工具执行、自动续轮、持久化/checkpoint、claim/CAS/fencing 和副作用恢复延期；纯工具观察、结果关联与请求型生命周期不因此删减 |
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
