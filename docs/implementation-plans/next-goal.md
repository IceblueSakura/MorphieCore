# 后续计划

## 当前主线

目标是**正确接入多个 Provider API，经独立 Semantic Model / IR 向下游提供标准规范的 OpenAI Responses 接口，简化下游 Agent 的适配细节**。Gateway 与未来 Agent 复用同一语义权威；[ClientManaged 交互合同](../architecture/interaction-contract.md#client-managed-context)作为选片与回归基线。请求型音频保持独立 Speech/Transcription task；已有 Text/Image/File、标准文本/function、类型化消费和独立媒体作为维护与回归边界。不以增加 Provider、模型数量或模态枚举作为进度。Chat Completions 是有限有损的兼容投影，不反向限制共享核心。

Agent runtime 与恢复不列入本项目的实施或延期目标；职责边界归 [Semantic Model](../architecture/semantic-ir.md#1-语义权威与消费者)。纯工具观察、结果关联与请求型生命周期不因此删减。

**先功能，后稳定性专项。** 近期优先正常功能、标准消费与必要回传；不主动扩展上游 HTTP 失败、非标准 JSON/解压失败、断流或慢消费者等异常矩阵，不将其作为新增功能的前置。现有认证、预算、取消、严格终态及拒绝回归继续保留；实际阻断所选功能的问题按最低 owner 修复，不借机扩展通用重试、调度或诊断框架。

**本阶段以扩大实际支持面和测试约束设计。** 先选择具体 Provider API、标准功能或消费者组合，核对真实合同与最小输入/输出，再补负责层的缺口并验证闭环；只有实际反例证明现有 IR 不足时才调整语义设计。跨 Provider/Model 的 opaque 剥离与可见 thinking 转 text 保留为[后续方向](#cross-target-history)，本阶段不继续定型或实现，不为它预建来源体系、转换框架或会话服务。

<a id="strict-verification"></a>

**Strict 约束的验证边界。** strict JSON Schema 与 strict function tools 暂仅执行静态/离线验证，包括 pure IR、codec/lowering 及固定 SDK 的 synthetic loopback 组合；不执行真实 Provider 的 strict 能力/adherence 验证，也不以其稳定支持作为当前退出条件。保留 strict/default、Schema 顺序/精度、参数/结果关联与未准入拒绝，不删除约束或改变公共合同。既有 strict live probe 保留但暂停执行；未来恢复须重新明确范围。普通文本、非 strict 工具/history 与媒体功能仍可按授权独立验证。

本页维护推荐优先级、依赖和选片条件，不是实现完成声明或操作授权。有效合同归 [Semantic Model](../architecture/semantic-ir.md)，已定稿行为切片及其当前状态只归 [current-focus](current-focus.md)，实际缺口归[实施边界](../implementation-status/generation.md)，等待证据的问题归[待决状态](../implementation-status/open-questions.md)。

## 推进顺序与退出条件

以下是**选片优先级，不是整块实施阶段**。每项按最小可观察场景拆分；前项只需闭合后项实际依赖的边界，不要求先补齐整个 Responses 标准。已有基础层和独立覆盖直接复用，不重复安排“重建 IR / Gateway / 凭据管理器”。

现有 ClientManaged 语义、Responses/Chat 转换、模型发现及请求型媒体是维护输入，不再作为待建基础层排期。具体承载、接线与消费者缺口查[实施边界](../implementation-status/generation.md)，所选 profile 的合同和回归随片维护；不以整个标准、其他模态或账户资格作为纯库修复的前置。

**产品选型顺序：优先 OpenRouter 上采用标准 OpenAI 协议的模型，次选 Token Plan 相关模型。** 这是接入优先级，不是请求内自动 fallback，也不限定模型研发者必须是 OpenAI。按具体 operation 核对协议、默认值、控制与产物；“OpenAI-compatible”不能替代逐项合同。必要差异仅在受信 profile 中具名映射，保持标准下游与共享 IR，不把不支持的控制静默丢弃。Token Plan 的套餐准入和所需原生协议分别核对，不与普通按量端点或凭据互换。来源入口见 [OpenRouter](../references/providers/README.md#openrouter) 与 [Model Studio](../references/providers/README.md#alibaba-cloud-model-studio)；此顺序不授权真实调用或私有 activation。

| 优先级 | 目标与产出 | 退出条件与非目标 |
|---|---|---|
| **1. Generation 组合功能与标准消费** | 按具体缺口扩展 reasoning + 非 strict function/history、并行调用/结果关联、图文输入与工具往返，以及已准入的工具图片结果等组合 | 所选场景形成上游 wire→IR→标准 Responses→必要回传闭环；不重建已有工具基础层，不把 user 图片支持推定为工具图片准入 |
| **2. 多 Provider / 模型与上游差异** | 为具体消费需求增加受信绑定或 profile 映射，分别验证原生 Responses 与 Chat→Responses 路径的实际输入、报告和回传 | 区分注册、实例启用、wire 表示与上游接受；既有映射能满足时不重建 IR，不以兼容声明、模型数量或一次 HTTP 200 作为完成判据 |
| **3. 已有独立 task 的可用范围** | 对现有 Images、Speech、Transcription operation 的目标绑定、已定稿控制/产物及标准客户端消费按实际需求选最小增量 | 保持各 task 的标准接口，不强塞 Responses；新格式、事件、资源或新 task 若涉及延期范围，先单独重评，不由本表一并恢复 |

**维护基线：**Text 或 Image+Text 输入、纯文本最终输出、明文 reasoning 与同目标 opaque 回传沿用已有语义及消费者回归；Responses / Chat 是上游路径，下游当前验收目标仍是标准 OpenAI Responses。下游 Chat 保持既有合同，不扩大本轮验收。明文 content、summary、opaque 与最终答案继续分别判断，不将已有链路重新列为待建基础层，也不把模型视觉/算术质量等同于协议或 replay 正确性。

**每片推进方式：**先从[缺口名录](../implementation-status/generation.md)选择具体可观察场景，核对官方 operation/profile 与源码；区分语义不足、目标无载体、缺绑定/接线和缺验收。复用已有独立回归，仅补反例所需的 owning layer，再按[开发检查](../development.md)和[受控 probe](../probes.md)验证固定消费者及实际 Provider。按任务需要比较静态/事件、值与关联、同目标必要 replay，不复制逐模型矩阵；模型答案质量、协议正确性和 opaque 回传分别报告。来源冲突或真正的 IR 缺口只阻塞受影响范围，不能用 adapter 任意 JSON 或私有字段绕过。

**依赖与停止点：**第 1、2 项按真实功能交错推进，第 3 项不等待完整 Generation union。每次只在 [current-focus](current-focus.md)固定本片输入、产出、非目标、独立失败例和验证边界；闭合即收口，不自动进入下一片。同目标 replay 与现行拒绝继续维护，不能为扩大支持面提前启用跨目标 opaque/thinking 降级；严格约束实测、其他延期能力和稳定性专项不因本阶段方向自动恢复。

<a id="cross-target-history"></a>
### 后续保留：跨 Provider/Model 历史投影（本阶段不实施）

以下只保留未来恢复时的策略方向，不是近期排期，也不是本阶段扩展支持面的前置。须在用户明确恢复后，结合实际消费者反例将精确方向、格式/owner、依赖处置与诊断归入[交互合同](../architecture/interaction-contract.md)和[具名损失合同](../architecture/protocol-and-lowering.md#semantic-loss)，再选择行为切片；**当前合同、实现及拒绝边界不因这份计划改变**。

- **方向与触发**：只转换送往下一目标的 request/history。来源 Provider 或 Model 与目标不同，其 opaque 按不兼容处理，从目标副本直接剥离，同时将可见 thinking 转为普通 assistant text。Provider 指受信接入身份，不是模型研发者；模型别名等价需明确证据，不以字符串近似匹配。来源证据与目标运行信息的 owner 须先固定，不把路由、credential 或 attempt 放进 task 语义。
- **同目标与未知来源**：Provider/Model 相同不触发这条降级，但仍须满足格式/API、owner、finality、scope 和依赖检查，不保证跨账号或长期有效。来源未知不视为匹配或不匹配；首片限定来源已知的类型化历史，标准 HTTP 回传缺少可信来源时明确报告缺口，保持现行拒绝边界，不增设私有字段或复活 `_openbridge`。
- **可见内容与身份**：保留实际文字、顺序和独立消息/part 边界；summary 不冒充完整 reasoning content。仅有 opaque、无可见内容时不补占位正文；剥离后的空 owner 及其引用如何处理须在所选合同中明确。转换不提升为 system/developer authority，不合成 final-answer phase、成功终态或工具结果。
- **损失与依赖**：原始观察及其 reasoning/opaque/来源不变，各固定候选独立投影。区分主动放弃的隐藏推理续轮材料与仍须保护的工具 call identity、参数、结果关联、指令及资源关系；重验剩余依赖，不能靠重算证明或 fidelity 恢复删除值。损失须可由 typed 诊断检查，不输出密文或另存竞争正文。
- **与响应交付分离**：不将普通 response/event 的 reasoning 全局降为答案文本，不改写已交付内容或重开终止的 reducer。静态结果与事件物化结果构成 history 后应遵循相同转换；同目标原始回放继续使用原合同。

方法参考 [pi 跨模型回放](../references/pi-provider-abstraction.md)，仅采用上述选定方向，不继承图片占位、合成缺失工具结果或拒绝密文后自动剥离重试等其他客户端策略。跨目标降级是显式放弃原隐藏状态，不是将一种密文转换成另一种密文。

**恢复后的验收边界：**独立纯库预期应覆盖同目标保留、只换 Provider、只换 Model、可见/opaque-only/缺值、来源未知，以及删除、重排和受保护关联；验证原观察不变、候选间不污染，回到原目标仍须重新检查回放条件。再验证所选目标的独立 wire 预期与消费/必要回传，复用既有 JSON/SSE 和图片回归；不以 HTTP 200 证明隐藏状态延续，也不自动推进会话服务。

ServerManaged 与 Gateway 自有短期会话仍按下方恢复条件独立评估，不与上述未来投影捆绑，也不作为当前功能扩展前置。标准模型发现保持维护，SIWC 采用独立处理。

**文件重评节点：**在下一次文件产品行为选片前评估是否继续延期，不等待所有模态完成。没有新的恢复决定前，仅维护既有 Responses user inline/URL 基础输入及必要正确性、安全边界；重评不自动恢复 issuer-bound ID、工具文件结果、生成文件、更多格式/目标、Chat 文件投影或 `/v1/files` 服务。资源 identity、用途、locator 与引用坐标的纯语义完善不受这一产品延期阻塞，也不因此激活新文件承载。

<a id="近期实施单元与停止点"></a>
<a id="下一片候选标准模型发现"></a>

## 后续选片条件

后续依据具体消费者反例或实现缺口，按[交互合同](../architecture/interaction-contract.md)选择实际依赖已闭合的子片，在 current-focus 定稿输入、输出、非目标与独立失败预期和停止点；产品优先级按上表选片。不把缺口清单直接当排期，也不以完整 Responses union、更多模态或 SIWC 为前置。来源冲突或新的 IR 缺口只阻塞相关范围，扩大公共类型或迁移边界前先说明影响。完成所选边界即清理 focus，不自动滚动进入后继任务。

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
| 跨目标 opaque 剥离 / thinking 转 text | 仅保留[未来策略](#cross-target-history)，本阶段不继续定型或实现；用户明确恢复并有具体来源/目标/消费者场景后独立选片，不改变同目标 replay、当前拒绝或自动重试边界 |
| 文件扩展 | 按上文选片前重评；先有具体消费场景与资源/issuer 合同，不为基础输入补建完整文件服务 |
| 高级图片 | 维持 `/v1/images/generations` 基础静态生成与有序产物；SSE、预览/最终产物事件、编辑、蒙版、参考图、URL 下载和文件服务延期。恢复时独立选片，先完善所选 API 再追加模型验证，不自动扩大测试集合 |
| Embedding、其他请求型音频与独立媒体 operation | 基础 Speech/Transcription 按现有合同维护；其他分支分别确定最小 task、输入/产物及资源范围。Embedding 保持独立 `/v1/embeddings` 目标；独立任务不强塞 Responses |
| Audio Realtime | 明确要实现；在请求型范围收敛并独立选片后展开协议与生命周期设计，不预建状态机或把音频重构塞入近期切片 |
| Google Interactions / Anthropic Messages 原生实现 | 当前仅作为设计来源与 typed 合同反例，不新增 codec、adapter、SDK 或网络接线；后续有具体目标与消费者需求时固定 profile 独立选片 |
| ServerManaged 与 Gateway 自有短期会话 | 与跨目标 opaque/thinking 投影分开选片；先区分 Provider 管理的续接与 MorphieCore 自有状态，再决定仅保存亲和/来源还是保存完整历史。恢复前明确历史唯一权威、会话引用的标准 API、并发/分支、账号绑定、预算、过期/删除及缓存亲和边界；prompt cache key、session ID 和 opaque 不互为别名，稳定 key 不保证命中或密文可移植。当前保持 ClientManaged，不预建 previous-ID、conversation、状态服务或私有 carrier，现有主动状态化分支拒绝不变 |
| Reasoning opaque 闭合后权威 | 只按[待决问题的恢复证据](../implementation-status/open-questions.md#恢复选片所需证据)重评，不作为其他任务前置，不预建更新事件、严格校验器或回放服务 |
| 丰富模型发现与调度 | 更丰富 `/models` 的路径/schema 另定；标准目录不引入价格、成本路由、动态 registry、负载均衡或自动 credential refresh |

<a id="每个切片的执行与验收"></a>

执行与验收统一遵循 [AGENTS.md](../../AGENTS.md)、[开发指南](../development.md)及[具名投影合同](../architecture/protocol-and-lowering.md#semantic-loss)，本页不另设执行清单。真实调用按[受控 probe](../probes.md)限定目标和诊断问题；计划不授予登录、部署、凭据操作或提交权限，也不记录动态模型库存与执行结果。
