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

现有 ClientManaged 语义、Responses/Chat 转换、模型发现及请求型媒体是维护输入，不再作为待建基础层排期。具体承载、接线与消费者缺口查[实施边界](../implementation-status/generation.md)，所选 profile 的合同和回归随片维护；不以整个标准、其他模态或账户资格作为纯库修复的前置。

**产品选型顺序：优先 OpenRouter 上采用标准 OpenAI 协议的模型，次选 Token Plan 相关模型。** 这是接入优先级，不是请求内自动 fallback，也不限定模型研发者必须是 OpenAI。按具体 operation 核对协议、默认值、控制与产物；“OpenAI-compatible”不能替代逐项合同。必要差异仅在受信 profile 中具名映射，保持标准下游与共享 IR，不把不支持的控制静默丢弃。Token Plan 的套餐准入和所需原生协议分别核对，不与普通按量端点或凭据互换。来源入口见 [OpenRouter](../references/providers/README.md#openrouter) 与 [Model Studio](../references/providers/README.md#alibaba-cloud-model-studio)；此顺序不授权真实调用或私有 activation。

| 优先级 | 目标与产出 | 退出条件与非目标 |
|---|---|---|
| **1. Responses 核心功能** | 维护文本/function/history、reasoning、Structured Outputs 与已有图片/基础文件输入回归；仅按具体新消费差异扩展 | 所选功能形成 wire→IR→标准输出→必要回传闭环，不以接口接通或拒绝测试通过代替功能完成；不一次补齐完整 union |
| **2. 多 Provider 标准消费** | 代表性原生 Responses 场景作为接入回归；新增目标或实际差异按最小功能独立选片，差异归受信 adapter/profile | 除模型及明确支持的控制外，消费者不需要 Provider 专用解析或 history 重写；不按 Provider/model 数量铺测试矩阵 |
| **3. 非原生上游转换** | 维护现有 Chat→Responses 工具闭环，新的消费差异按[current-focus](current-focus.md)独立选片 | 静态、事件与回传一致；具名投影不自动授权其他损失，不以私有字段或静默丢关系绕过决策 |
| **4. 稳定性专项** | 核心功能收敛后，按实际使用问题选择异常、负载与长期运行边界 | 不提前扩张；正常功能中已遇到的阻断问题可随片修复，现有防线始终保持 |

**依赖关系：**第 1、2 项按实际功能交错推进，不等待完整标准；第 3 项的设计论证可独立进行，行为实施必须先闭合所选映射合同。类型化消费与必要回传从首片开始，纯库检查不等待账户资格。标准模型发现保持维护，不重新建设；SIWC 采用按下节独立处理，不阻塞 API-key 路径。其他资源与媒体按具体消费需求独立选片，不作为本表前置。

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
| 文件扩展 | 按上文选片前重评；先有具体消费场景与资源/issuer 合同，不为基础输入补建完整文件服务 |
| 高级图片 | 维持 `/v1/images/generations` 基础静态生成与有序产物；SSE、预览/最终产物事件、编辑、蒙版、参考图、URL 下载和文件服务延期。恢复时独立选片，先完善所选 API 再追加模型验证，不自动扩大测试集合 |
| Embedding、其他请求型音频与独立媒体 operation | 基础 Speech/Transcription 按现有合同维护；其他分支分别确定最小 task、输入/产物及资源范围。Embedding 保持独立 `/v1/embeddings` 目标；独立任务不强塞 Responses |
| Audio Realtime | 明确要实现；在请求型范围收敛并独立选片后展开协议与生命周期设计，不预建状态机或把音频重构塞入近期切片 |
| Google Interactions / Anthropic Messages 原生实现 | 当前仅作为设计来源与 typed 合同反例，不新增 codec、adapter、SDK 或网络接线；后续有具体目标与消费者需求时固定 profile 独立选片 |
| ServerManaged 上下文 | 待后续评估；当前只实施 ClientManaged，不预建 previous-ID、conversation、连接级历史或不可用模式。现有主动状态化分支拒绝继续保持 |
| Reasoning opaque 闭合后权威 | 只按[待决问题的恢复证据](../implementation-status/open-questions.md#恢复选片所需证据)重评，不作为其他任务前置，不预建更新事件、严格校验器或回放服务 |
| 丰富模型发现与调度 | 更丰富 `/models` 的路径/schema 另定；标准目录不引入价格、成本路由、动态 registry、负载均衡或自动 credential refresh |

<a id="每个切片的执行与验收"></a>

执行与验收统一遵循 [AGENTS.md](../../AGENTS.md)、[开发指南](../development.md)及[具名投影合同](../architecture/protocol-and-lowering.md#semantic-loss)，本页不另设执行清单。真实调用按[受控 probe](../probes.md)限定目标和诊断问题；计划不授予登录、部署、凭据操作或提交权限，也不记录动态模型库存与执行结果。
