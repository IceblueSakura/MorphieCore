# 后续计划

## 当前主线

优先完善多个 Provider API → 共享 Semantic Model / IR → 标准 Responses 的功能与必要回传。Chat 是兼容路径；独立媒体使用各自标准 operation。总体职责归 [Semantic Model](../architecture/semantic-ir.md)，本页只维护未完成方向。

**先功能，后稳定性专项。** 保留现有认证、资源、取消和终态回归；新增工作从实际消费需求或最小反例出发，不扩张通用调度、重试或防御框架。

<a id="strict-verification"></a>

**Strict 验证暂限离线。** 保留 IR、codec/lowering 和固定 SDK synthetic 组合；strict Schema/function 的真实 Provider adherence 验证暂停，恢复时另定范围。非 strict 工具与媒体可独立验证。

## 推进顺序与退出条件

产品选型优先 OpenRouter 标准协议模型，次选 Token Plan。按具体 operation/profile 核对，产品优先级不用于请求内 fallback。

| 优先级 | 工作 | 退出条件 |
|---|---|---|
| 1 | Generation 的 reasoning、非 strict 工具/history、并行关联及图文组合 | 所选 wire→IR→标准 Responses→必要回传闭合 |
| 2 | 具体 Provider/model 的绑定与差异映射 | 原生 Responses 与所需 Chat→Responses 路径各自完成消费验证 |
| 3 | Images、Speech、Transcription 的实际需求增量 | 保持独立 task、目标准入和标准消费边界 |

### 未完成的指定目标

仅保留尚待选片的目标，不扩大默认 probe：

- Token Plan：`glm-5.3`、`deepseek-flash`、`qwen-image-3.0-pro`、`qwen-audio-3.0-tts-plus`、`qwen-audio-3.0-realtime-plus`。
- OpenCode Go：`claude-haiku-5-5`、`minimax-m3`。

Token Plan 的重名标签/上游 ID、图片 URL 产物，以及 Go 原生 Anthropic 与 Realtime 协议分别定稿。既有绑定按 catalog 查询，不创建占位项。

## 后续选片条件

从[实现缺口](../implementation-status/generation.md)选择可观察结果，在 [current-focus](current-focus.md)记录需求、独立失败例、非目标和验证边界。按最低 owner 修复，每片闭合即停止并清空 focus，不自动滚动实施下一片。实现与验证方法归[开发指南](../development.md)。

文件产品扩展在下一次相关选片前重评。恢复前维护既有 Responses user inline/URL 输入；纯资源 identity、用途与坐标完善可独立推进。

<a id="cross-target-history"></a>
### 跨 Provider/Model 历史投影

保留策略方向，实施延期：对送往不同 Provider 或 Model 的历史，在目标副本剥离不兼容 opaque，并将可见 thinking 转为 assistant text。保留真实文字、顺序、消息/part 边界和工具关联；原观察不变，同目标沿用既有 replay 条件。

恢复时先定来源证据、格式/owner、opaque-only/空 owner 处置、依赖重验与 typed 损失诊断。来源未知仍按现行边界处理。该策略与 ServerManaged、普通 response/event 交付分开；方法来源见 [pi 参考](../references/pi-provider-abstraction.md)。

## 延期目标与恢复条件

| 方向 | 恢复条件 |
|---|---|
| 跨目标 history 投影 | 按上节明确来源、消费者和依赖合同 |
| 文件扩展 | 具体消费场景与资源/issuer 合同；分别选择 ID、工具/生成文件、格式、Chat 投影或文件服务 |
| 高级图片 | 分别选择流式/预览、编辑/蒙版、参考图、URL 获取与资源服务 |
| Embedding 与其他媒体 operation | 定最小独立 task、输入/产物和资源范围；Embedding 面向标准 `/v1/embeddings` |
| Audio Realtime | 请求型范围收敛后独立确定双工协议、会话、事件与预算 |
| Interactions / Messages 原生接入 | 有具体目标需求后固定 operation/profile，再实现 codec 与执行主链 |
| ServerManaged / Gateway 短期会话 | 定历史权威、标准引用、并发/分支、账号绑定、期限/删除与缓存关系；与 history 投影分开 |
| Opaque 闭合后权威 | 按[待决问题](../implementation-status/open-questions.md#恢复选片所需证据)取得事件与消费者证据 |
| 丰富模型发现与调度 | 独立确定公开路径/schema 与选择策略；现有 Models 视图归[HTTP](../http-gateway.md#标准模型发现) |
| SIWC 远程持久化/部署 | 先确认[远程 host 条款](../references/siwc-login.md#远程-host-与分布式应用边界)，再按具体授权操作 |

当前拒绝与同目标 replay 合同保持有效。操作授权归 [AGENTS.md](../../AGENTS.md)，真实验证归[Probe](../probes.md)。
