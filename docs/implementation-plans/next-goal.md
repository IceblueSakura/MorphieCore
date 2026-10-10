# 后续计划

## 当前主线

当前以 **客户端每次提交完整请求/历史的无状态 API** 为主线。以 pi/OpenCode 的实际 Agent 工具循环为使用基线，结合 new-api/RelayKit 的协议转换方案，完成必要 IR 收敛与 Anthropic Messages 原生接入；随后扩展 Embedding、图片/音频及文件消费，终期闭合本网关限时历史的有状态 Responses API。Generation 继续使用多个 Provider API → 共享 Semantic Model / IR → 标准 Responses 的主链，Chat 是兼容路径；独立 task 使用各自标准 operation。总体职责归 [Semantic Model](../architecture/semantic-ir.md)，本页只维护未完成方向。

**优先复用方案，不先建设通用框架。** 从工具选择、参数、执行结果/错误和历史续轮出发，参考[固定 Agent 与转换源码](../references/agent-protocol-adaptation.md)，只补 MorphieCore 与所选用法之间的差额。完整性用于覆盖交互需要，消融用于减少重复表达与维护分支；不以原始 wire 全部可恢复为目标，也不压成协议最小公分母。具体有损行为在 owning contract 定稿后实施，现行拒绝不因本计划自动放宽。

首个落地场景及分步验收归 [Anthropic 接入计划](anthropic-messages-draft.md)：先文本、普通 function、工具错误反馈及默认 thinking 的原生库级闭环，再解决标准 Responses 的交付、保存与回传。custom/grammar→function 是按消费者需要选择的适配规则，不是独立桥接工程或 Anthropic 的前置任务。

下游只表达[中性逻辑 conversation](../http-gateway.md#conversation-context)，上游 session/cache 规则归[内部投影](../architecture/protocol-and-lowering.md#cache-affinity-projection)，不要求消费者按 Provider 传参。必要 replay 按真实来源与历史依赖验证，不默认绑定客户端 conversation ID 或 cache key；具体载荷合同归 [Anthropic profile](../architecture/anthropic-messages-profile.md#gateway-自有续轮载荷)。

<a id="api-context-goals"></a>
### API 与缓存目标

- 当前保持 ClientManaged 完整输入；显式会话 ID 是稳定分组方案，隐式缓存只求尽力，不追求绝对命中率或跨 Provider 缓存。
- 下一候选实现片是[短 TTL 亲和索引的最小闭环](cache-affinity-draft.md)，不保存历史正文，不以它阻塞或替代未完成的 Anthropic/F 消费链。
- 网关自管 `previous_response_id` 历史服务是[确定的终期目标](#stateful-api)，不是仍待决定是否实现。亲和索引与历史存储分开，权威和失败合同归[交互](../architecture/interaction-contract.md#context-authority)。

**先功能，后稳定性专项。** 保留现有认证、资源、取消和终态回归；新增工作从实际消费需求或最小反例出发，不扩张通用调度、重试或防御框架。

每片保留独立 synthetic 回归和受影响开发基线；与该片直接相关的固定消费者闭环随功能验证，按独立 gate 执行。大范围 SDK、真实 Provider 组合、质量和稳定性集中验收在所选功能清单接近完善后进行。用户指定目标可做有限接入验证，不据此扩张默认矩阵。

<a id="strict-verification"></a>

**Strict 验证暂限离线。** 保留 IR、codec/lowering 和固定 SDK synthetic 组合；strict Schema/function 的真实 Provider adherence 验证暂停，恢复时另定范围。非 strict 工具与媒体可独立验证。

## 推进顺序与退出条件

产品选型优先 OpenRouter 标准协议模型，次选 Token Plan。按具体 operation/profile 核对，产品优先级不用于请求内 fallback。

| 优先级 | 工作 | 退出条件 |
|---|---|---|
| 1 | 无状态主线：缓存亲和小片、Agent 兼容与 Anthropic 原生接入 | 按[缓存计划](cache-affinity-draft.md)闭合最小隐式优化后停止扩张；保留并继续原生/标准消费者的独立验收，不以缓存或原生成功代替完整回传 |
| 2 | Embedding 与图片/音频扩展 | 按实际需求选择编码、输入、产物或独立 operation；不从已有分支推定更广支持 |
| 3 | 文件操作与 Responses 文件消费 | 定服务操作、存储/访问/删除生命周期及 Gateway ID 与 issuer-bound ID 的关系，再闭合所选消费链 |
| 4 | 终期网关自管有状态 Responses API | 按[终期计划](#stateful-api)闭合限时历史、标准引用、工具续轮与 JSON/SSE；不依赖上游会话存储 |
| 5 | 集中兼容性验收，再稳定性专项 | 围绕已实现功能清单补固定消费者、真实 Provider、组合和运行证据 |

其他 Generation 增量按实际需求选片。工具结构消融随所选适配的重复分支评估，不要求先统一全部工具类型、补齐所有协议字段或完成下表延期项。

### 未完成的指定目标

仅保留尚待选片的目标，不扩大默认 probe：

- Token Plan：`glm-5.3`、`deepseek-flash`、`qwen-image-3.0-pro`、`qwen-audio-3.0-tts-plus`、`qwen-audio-3.0-realtime-plus`。
- OpenCode Go：`claude-haiku-5-5`、`minimax-m3`。

Token Plan 的重名标签/上游 ID、图片 URL 产物，以及 Go 原生 Anthropic 与 Realtime 协议分别定稿。既有绑定按 catalog 查询，不创建占位项。

## 后续选片条件

从所选 Agent 用法和[实现缺口](../implementation-status/generation.md)选择可观察结果：先定位现成实现与字段映射，说明对参数、生成约束、结果、历史和交付的影响，再区分 IR 缺口、目标限制与未接线。采用结论只迁入对应合同，不另存全协议比较矩阵；缺口列表不等于实施清单。

在 [current-focus](current-focus.md)记录需求、独立失败例、非目标和验证边界。按最低 owner 修复，每片闭合即停止并清空 focus，不自动滚动实施下一片。能直接使用的库优先复用；跨语言方案不以新增进程服务为默认实现。方法和依赖/许可检查归[开发指南](../development.md)及[固定来源](../references/agent-protocol-adaptation.md)。

文件服务模型由具体消费需求选择，不默认复用 Embedding 模型。相关切片前定稿服务范围及资源/issuer 合同；维护既有 Responses user inline/URL 输入，不以新增服务为由自动放宽 ID、工具或生成文件准入。

<a id="cross-target-history"></a>
### 跨 Provider/Model 历史投影

切换 Provider 或 Model 时不续传 opaque reasoning，即使某些上游声明格式兼容；原观察不变。可见 thinking 转为 assistant text 的方向仍待所选消费者切片定稿。转换用于继续任务，不恢复原模型内部推理状态，也不能替代同目标工具续轮的必要 replay。保留真实文字、顺序、消息/part 边界和工具关联，更广重组另定。

有具体消费者需要跨目标续轮时恢复，先定来源证据、格式/owner、opaque-only/空 owner 处置、依赖重验与有界行为诊断。来源未知仍按现行边界处理。该策略与 ServerManaged、普通 response/event 交付分开；方法来源见 [Agent 历史转换](../references/agent-protocol-adaptation.md#history)。

<a id="stateful-api"></a>
## 终期有状态 API 实施方向

在无状态请求/响应与所选消费者闭环稳定后实施；不以 Realtime、分布式存储或所有延期协议完成为前提。目标合同归[ServerManaged](../architecture/interaction-contract.md#server-managed-context)，当前活动引用仍拒绝。

1. 定稿 Gateway response ID、标准 `store`/`previous_response_id` envelope 与解析边界；替换当前拒绝用占位，而不是仅放宽 parser。同步 Schema、SDK 预期和本地身份/来源归属。
2. 实现单实例、有授权与容量界限的限时历史存储及完整上下文物化；固定 TTL、过期/删除、重启、祖先依赖和容量不足的合同，亲和索引不能承担此责任。
3. 接回同一 validation/lowering/execution 链，闭合工具结果续轮、并发分支、必要 replay、JSON/SSE 引用就绪与保存失败；运行独立回归和固定消费者 gate。

停止在所选短期历史服务，不顺带实现标准 Conversations 全套资源、自动摘要、Agent loop 或永久记忆。持久化/多实例共享按实际需求另定。

## 延期目标与恢复条件

| 方向 | 恢复条件 |
|---|---|
| 跨目标 history 投影 | 按上节明确来源、消费者和依赖合同 |
| 更广工具与 Schema 转换 | 具体消费者需要 custom/grammar、namespace、托管工具或方言适配时，采用已有方案并定执行输入、生成约束和回传代价；不先建设转换框架 |
| 文件扩展 | 具体消费场景与资源/issuer 合同；分别选择 ID、工具/生成文件、格式、Chat 投影或文件服务 |
| 高级图片 | 分别选择流式/预览、编辑/蒙版、参考图、URL 获取与资源服务 |
| 更广 Embedding 与其他媒体 operation | 分别选择 Base64、token 输入、稀疏或多模态向量等增量；不从文本向量主链推定实现 |
| Audio Realtime | 请求型范围收敛后独立确定双工协议、会话、事件与预算 |
| Google Interactions 原生接入 | 有具体目标需求后固定 operation/profile，再实现 codec 与执行主链 |
| Opaque 闭合后权威 | 按[待决问题](../implementation-status/open-questions.md#恢复选片所需证据)取得事件与消费者证据 |
| 丰富模型发现与调度 | 独立确定公开路径/schema 与选择策略；现有 Models 视图归[HTTP](../http-gateway.md#标准模型发现) |
| SIWC 远程持久化/部署 | 先确认[远程 host 条款](../references/siwc-login.md#远程-host-与分布式应用边界)，再按具体授权操作 |

当前拒绝与同目标 replay 合同保持有效。操作授权归 [AGENTS.md](../../AGENTS.md)，真实验证归[Probe](../probes.md)。
