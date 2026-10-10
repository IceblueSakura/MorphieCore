# Agent 使用方案驱动的 Anthropic Messages 接入计划

**状态：当前优先 F 的 Gateway 自有续轮载荷与标准消费者；原生 SSE 和真实验证按依赖分别推进。当前小片归 [current-focus](current-focus.md)，所选静态语义与接口归 [Anthropic 合同](../architecture/anthropic-messages-profile.md)。**

指定准备目标仍为 `opencode-go/claude-haiku-5-5`。以 pi/OpenCode 的真实工具循环为使用基线，结合 new-api/RelayKit 的字段映射，先完成保留默认 adaptive thinking 的原生库级闭环，再单独解决标准 Responses 的交付、保存与回传。不从计划推定产品注册、账户资格或实例激活。

总体顺序归 [next-goal](next-goal.md)，语义权威归 [Semantic Model](../architecture/semantic-ir.md)，交互与投影分别归 [interaction](../architecture/interaction-contract.md) / [lowering](../architecture/protocol-and-lowering.md)。新规则迁入对应合同后才改变现行行为；实施前在 [current-focus](current-focus.md)记录当前小片的需求、独立失败例、非目标和停止点。完成的执行内容按[文档规则](../README.md#写作与维护)移除，不保留比较或验收档案。

## 1. 目标与方案来源

首个用途是：文本请求 → 模型选择普通客户端工具 → Agent 接收参数并返回结果或错误 → 模型继续回答；同时能保存并回传所选原生 thinking。格式无效、工具失败、模型截断和传输失败分别处理。IR 完整性服务于这条交互，消融减少重复表达，不以恢复全部原始 wire 为目标。

采用[固定源码与原生协议来源](../references/agent-protocol-adaptation.md)，不重新发明已有 Agent 用法：

| 来源 | 本计划参考的职责 | 不随之建设 |
|---|---|---|
| pi | 工具执行参数与 constrained sampling 分离；目标编码、参数消费、thinking 与历史投影 | Agent 调度、会话存储或完整 runtime |
| OpenCode | 普通 function/字符串工具、本地内容校验、工具错误反馈与后继请求、结果媒体的目标安排 | 权限系统、文件工具实现或自动修复 Agent |
| new-api / RelayKit | 所选方向的字段、工具选择、参数包装/解包、结果与 usage 映射及损失案例 | 渠道管理、计费、Chat 中转架构或全部宽松修补 |

模型特定 guide 与所选 schema/profile 共同决定原生映射；Agent 源码决定所参考的消费用法，不替代协议规范。直接复用现有库优先，跨语言先复用方案和算法，不默认引入进程服务；代码移植、测试资产与消费者升级遵循参考页许可及[开发指南](../development.md)的锁定检查。

## 2. 后续采用方向与待定的行为代价

首选原生用法、有限损失与独立 synthetic 例子只归 [Anthropic 合同](../architecture/anthropic-messages-profile.md)，不在计划重复采用结论。下表维护超出所选范围的取向，不预先批准每个有损规则；不用先完成整表才进入原生接入。

| 用途 | 优先复用的方式 | 定稿时必须回答 |
|---|---|---|
| 普通工具与约束生成 | 以 function/结构化参数为常用路径，参考 pi 分离执行输入和生成约束 | 来源 strict/default、要求与偏好如何表达；目标不支持时拒绝还是降级，执行校验仍由谁负责 |
| 文本/grammar 工具 | 需要时采用 pi/new-api 的单字符串 function 包装与逆转换；不是独立前置桥接工程 | 字符串是否原样到达；grammar 降为提示的影响；包装无效与工具内容无效如何分别反馈；历史及 SSE 如何恢复 |
| 工具选择与身份 | 参考已有具名工具选择和一致的 ID 映射 | 逻辑工具与目标别名的对应、碰撞及结果引用；不把“可经适配调用”标作原生 grammar 支持 |
| 工具结果/错误 | 参考 OpenCode/pi，保留正文和实际错误，允许 Agent 发出下一次修正请求 | 目标错误 carrier、未执行与执行失败的区别；Gateway 不执行工具、不猜修复参数或补造未知执行结果 |
| Reasoning/history | 同目标沿用适用原生回放；跨目标以继续任务为用途，来源见参考页 | 两种用途分开；跨目标规则按 [next-goal](next-goal.md#cross-target-history)另片恢复，不借它解除同目标必要 replay |
| Schema/消息表示 | 先找既有字段映射和有前提的归一化，再补本地差额 | 参数接受集合、指令作用位置、顺序和消费者行为是否改变；第三方省略或默认值不自动成为本项目规则 |

每个选中用法只需一条可验收的采用结论：**固定来源 → 采用行为 → 对 Agent 的代价 → 本地 owner/差额 → 独立反例**。区分 IR 无位置、目标无载体和未接线；有损可用性看用途，不用字段不等价直接否决，也不以 HTTP 成功代替任务完成。只有实际需要逆转换时，才补候选内的请求级映射记录，不预建通用转换计划或保真等级系统。

## 3. 原生范围与接入条件

原生文本、工具、错误、thinking、基本终态与计量的有限范围归 [profile](../architecture/anthropic-messages-profile.md)。动态目录、目标资格和模型特定控制/缺省在实施时复核，来源归参考页，不维护能力库存。

Go 的固定路径/认证依据归[原生来源](../references/agent-protocol-adaptation.md#anthropic-native)。准备显式发送 `anthropic-version: 2023-06-01`，使用真实 MorphieCore User-Agent；分组遵循[中性上下文的内部投影](../architecture/protocol-and-lowering.md#cache-affinity-projection)。Messages、Chat 与 Zen balance 分别绑定，不通过本地无 fallback 推定服务端不会消耗 Zen balance。只发送 synthetic 编码任务，不将订阅入口规划成任意流量代理。

## 4. 本地差额与结构选择

先复用共享 Generation IR、codecs/lowering、transport、凭据 loader 和 probe 守卫。下表只列本场景需要处理的边界，不要求一次性重构。

| 边界 / owner | 对接缺口 | 草案方案与待定点 |
|---|---|---|
| [工具](../../src/semantic/task/generation/tool.rs)、[结果](../../src/semantic/task/generation/tool_result.rs) | 标准消费者的新错误 carrier 与投影尚未闭合 | 按[结果错误合同](../architecture/interaction-contract.md#client-tool-result-errors)选择有限载体或损失，不从正文推断执行事实，不先合并全部工具类型 |
| [请求投影](../../src/adapter/request.rs)、[lowering](../../src/lowering/generation.rs)、[delivery](../../src/execution/delivery.rs) | 选用 custom 包装等规则时，响应需要知道本次请求的映射 | 按实际规则返回有界投影依据并传到对应 attempt；transport 不解释它，IR 不承载运行时状态。预检与发送使用同一规则，不能只改请求不闭合响应/history |
| [能力合同](../../src/semantic/task/generation/contract.rs)、[规划](../../src/execution/plan.rs) | 原生能力与经批准适配后的可用性需要区分 | 仅为选定规则检查原请求准入、策略前提及投影后 requirements；不把原生 capability 标成 true，不重写通用能力系统 |
| [Replay](../../src/semantic/task/generation/replay.rs)、[依赖](../../src/semantic/task/generation/dependency.rs) | 已有 Anthropic 格式，但无原生 parser/encoder 与逐格式绑定闭环 | 实现 signature intake、finalization、编码与实际 prefix/configuration 依赖；不以普通 fingerprint 声称跨请求认证 |
| [Progress](../../src/semantic/task/generation/progress.rs)、[response](../../src/semantic/task/generation/response.rs) | 基本进度已有 owner，更广 stop/refusal detail 未闭合 | 基本分支映射 Outcome/Progress；更广事实比较最小 typed 扩展与收窄 profile。未定稿值明确拒绝，不放入 fidelity JSON 掩盖 |
| [Bootstrap](../../src/gateway/bootstrap.rs)、[Gateway config](../../src/gateway/config.rs) | 上游协议登记与客户端入口激活相关联 | 原生库绑定、受控 probe 与公开客户端激活分别声明；新增 Messages binding 不自动启用下游 `/v1/messages` 或 Responses |

工具生成约束、输入包装、错误文本和有限历史重组需要的新规则，归 [protocol/lowering](../architecture/protocol-and-lowering.md) 与 [interaction](../architecture/interaction-contract.md)；Schema 约束/default 的改变归 [Schema profile](../architecture/schema-profile.md)。先定行为和反例，再选择 Rust 形状；不新增 Provider IR、raw-JSON 请求袋、万能参数修复器或私有 `_openbridge`。

## 5. 实施顺序与停止点

以下是同一主线的依赖顺序，不是要求一次实施全部的任务包。所选采用规则以 profile 为准；D–E 闭合原生流式与真实执行，F 解决标准消费者，可先推进其独立静态边界。每个获选小片在 current-focus 固定退出条件，闭合后停止，不自动进入下一片。复用[静态语义/接口](../architecture/anthropic-messages-profile.md)，不把离线类型、映射或绑定当成运行证明。

### D. SSE 与静态一致性

复用 [SSE framing](../../src/transport/sse.rs)，单独实现 `message_start`、block start/delta/stop、`message_delta`、`message_stop` 及所选 ping/error grammar。

- JSON 参数按原生分片合同构造，初始 `{}` 不证明完成；截断不提升为完整参数。
- Signature 按 block closure 最终化，不套用 Responses item-done 或闭合后权威规则。
- 累计 usage 更新不求和；message terminal 不替代严格 HTTP EOF，不要求 OpenAI `[DONE]`。
- 缺 closure、error、取消及尾随非法数据不可修补为成功；终态 snapshot 不补缺失必要事件。

退出条件：静态、语义分片和字节切分共享独立 typed 预期，失败状态不可恢复成功。

### E. 显式目标绑定与受控原生验证

- 绑定指定目标、固定 origin/path、有限能力和单来源凭据；保持现有 loader 与凭据域。
- 按第 3 节提供显式 session 和请求头；不通过公共客户端字段临时拼凑来源。
- 扩展现有受控 probe 的原生选择与 collector，复用账本、预算、源码指纹、脱敏及清理。
- 保持 raw capture 关闭，不以临时直连脚本绕过守卫；原生绑定不宣称 Gateway/binary 已激活。

退出条件：文本、工具成功/错误反馈、所选 thinking/signature 的原生交付与回传有对应证据。未报告 signature 只表示该回传分支未覆盖；该门槛不是实际 pi/OpenCode 或标准 Responses 消费者验收。

### F. 标准 Responses 与固定消费者闭环

- 先解决[标准载体选择](#responses-closure)，再接线下游；不拿跨目标 history 策略代替同目标必要 signature。
- 使用固定 pi 消费与工具循环；OpenCode 用法作为参数校验、错误恢复的独立对照，需要其实际兼容声明时另运行对应固定消费者。复用现有 gate，最小补足接口，不写第二个近似 Agent 当 oracle。
- 只有消费者实际需要 custom/grammar、namespace 或错误文本转换时，才实现第 2 节选定规则。请求、响应和下一轮 history 使用同一映射，失败与缓冲代价一并验收。
- 每个候选从同一原请求投影，重验要求与依赖；成功投影报告贯通已有 adapter/delivery 入口，不建立另一套诊断权威。

退出条件：所选标准客户端实际交付→执行或错误反馈→保存→回传成立，JSON/SSE 及规则开关的结果与合同一致。有限模型调用只证明选定场景；格式错误率、修复轮次和任务质量另作有界评估，不从序列化或单次成功推断。

## 6. 验证范围

验证方法归 [conformance](../references/conformance-baseline.md)、[development](../development.md)与 [probes](../probes.md)。下表只列本片需要的失败类别，不复制完整用例清单。

| 最小反例 | 必须保护的结果 |
|---|---|
| Message 没有 timestamp | 保持未报告；不生成伪上游时间 |
| 同一 Go 下的两种 wire/auth | Messages 使用固定 API-key header；既有 Chat 不受全局 auth 切换影响 |
| 精确数字、重复 JSON key、未闭合 input delta | 独立校验、精度保留；重复键或截断不能得到完整参数 |
| 工具结果有正文且 `is_error:true`；参数/工具内容校验失败 | 实际错误能反馈并供下一轮处理；Gateway 不执行工具、不把工具失败变成无正文或模型成功 |
| Signature-only；修改 system/tools/早期 history | 原签名保留；依赖改变后拒绝旧绑定，不从来源记录恢复删除值 |
| 同一 usage 累计报告从 7 更新到 9 | 最终为 9，不是 16；缺分项保持未知 |
| 缺 block-stop/message-stop、error 或终态后非法数据 | 不输出伪成功，不前移候选，不修补 partial |
| 选定有损规则；尚未支持的 replay/progress/参数载体 | 前者断言允许的行为改变及仍需保持的调用用途，后者明确拒绝；不靠私有 carrier 或一律拒绝掩盖缺口 |

采用文本包装时另覆盖引号/换行/反斜杠/Unicode/空输入、普通 function 不被误还原、选择与 ID 映射、参数闭合、名称冲突和下一轮历史。先验证外层包装，再由消费者处理内容合法性；不把外层 JSON 正确当作 grammar 正确。

每片先最低 owner synthetic 回归，再执行受影响开发基线。固定 SDK/Agent loopback 属于单独显式 gate；研究源码修订不是已安装运行依赖，版本和锁文件须在 gate 中固定。动态能力与模型质量不用离线输出猜测，真实验证补充而不替代独立预期。

### 待实施的原生 live 矩阵

| 场景 | 请求上限 |
|---|---:|
| 必要的显式模型 discovery | 1 |
| 文本 JSON / SSE | 2 |
| 工具两轮：JSON→SSE、SSE→JSON | 4 |
| 向实际工具调用返回 synthetic 错误，最多一次修正调用与结果后回答 | 3 |
| 实际 thinking/signature 两轮回传：两种交付顺序 | 4 |
| 有界截断 | 1 |

原生计划上限为 15 请求；单次输出 cap≤2048、接收≤2 MiB、exchange≤120 秒、run 总截止≤1800 秒。具体事件/JSON/总状态上限沿用并固定现有受控入口守卫。串行、零传输 retry/fallback/redirect、单来源凭据；前置模型/协议失败不发送依赖轮次。已声明的 synthetic 工具错误是该场景输入，其后继模型请求不是隐式重试；不扩成无界修复循环。

只发送 synthetic 编码任务和固定工具结果，不执行模型产生的任意脚本。取消沿用进程/连接清理；关闭不证明 Provider 停算。只保留有界脱敏计数与状态，原始内容及 signature 不落日志。F 的消费者、grammar 回退或质量对照须另定有限矩阵，不自动叠加到本计划。

执行前重新查询目标与能力，使用受控 loader/STORE；discovery 也占请求额度。当前 probe 的登记与选择能力不能由此矩阵推定，本文不提供可直接执行该目标的命令。低 cap 导致的合法截断独立报告，不通过放宽 guard 或无诊断价值重试取得通过。一次回传成功不证明聚合层账号稳定或隐藏 reasoning 确被使用。Strict live 暂停边界继续按 [next-goal](next-goal.md#strict-verification)执行。

<a id="responses-closure"></a>
## 7. 标准载体选择与原生停止点

标准 Responses 仍是最终 Generation 主接口。以下待决点阻塞相应标准消费分支，不阻塞独立原生库片；取舍不足时报告可行选项和行为代价，不自行发明扩展或把当前拒绝当永久设计：

1. Anthropic signature 的标准交付与必要回传采用 [Gateway 自有认证加密载荷](../architecture/anthropic-messages-profile.md#gateway-自有续轮载荷)。下一步闭合 reasoning `encrypted_content` 的显式 carrier dispatch、来源容器恢复与固定消费者；纯库签发/验证不启用公开路径，不恢复独立 `_openbridge`。
2. 固定消费者自动携带[中性 conversation header](../http-gateway.md#conversation-context)，验证首次请求、无 thinking 与模型切换；上游分组规则在 Gateway 内部完成，不将 Provider session 参数或 key 作用域要求交给使用者，也不将分组绑定进加密 reasoning。
3. Structured tool input 的字符串参数投影、保存与回传，以及工具错误报告的 carrier；参考现成用法定有限转换，而非要求所有来源形状原样恢复。
4. Reported progress、stop/refusal detail 的目标表达；明确哪些事实影响 Agent 是否调用工具、继续请求或停止，再决定允许的省略。
5. 上游未报告 timestamp 时，下游本地响应时间的 owner、来源和标准含义；本地 envelope metadata 不冒充上游报告。
6. Message boundary 投影与必要 replay/prefix 依赖的兼容条件。

D–E 原生范围闭合即停止并移除已完成计划内容：原生 JSON/SSE、typed IR、工具成功/错误及 signature/history 成立，既有协议和安全边界不回归；F 的未决方向保留，不宣称标准 Responses 或完整 Agent 已可用。

## 8. 随使用收敛的 IR 消融

消融不单独排在原生接入之前。所选适配暴露重复维护时，逐项比较“复用现结构”和“合并共同骨架”，一次只改变一个责任边界：

- 工具身份/选择的重复变体；function/custom 是否只是目标编码差异，还是仍有独立输入和失败合同。
- `CallContext` 中不同节点的职责，以及 presence 标志与子值同步产生的非法组合。
- 某个 wire 的必填 metadata、具体能力上限是否被提升成通用 IR 限制；活动语义与仅用于拒绝未支持字段的占位项分别归属。

接受条件是：同一用途的规则和修改点减少，生成约束的变化可解释，参数/结果关联、错误反馈和历史闭环仍满足所选合同；行数或类型数量减少不是单独判据。保留一份当前值和必要来源，不以 raw JSON 或第二套 Agent IR 转移复杂度。

图片/文件、server tools、cache breakpoint/TTL、redacted thinking、更广 stop/safety/usage、多候选、compaction、Batch 和跨目标 history 不因本计划自动进入首片。Agent runtime、会话服务、通用桥接/策略/验证框架也不是前置；后续选片与恢复条件只归 [next-goal](next-goal.md)。
