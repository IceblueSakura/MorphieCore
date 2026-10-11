# 路线

本页是方向与当前切片的唯一来源。完成的条目直接删除（历史查 Git）；设计原则见[架构](architecture.md)。

## 冻结范围

只做重构所需的维护，不扩展功能：

- 独立媒体与向量任务：Images、Speech、Transcription、Embedding。
- 新增 Provider 或模型绑定（Google 协议接入阶段除外）。
- 订阅凭据（SIWC、Grok）的新功能。
- 有状态 Responses、文件服务、Realtime。

## 当前焦点：阶段 0 — 减负

| 项 | 状态 |
|---|---|
| `gateway::diagnostics` 的 probe 关联（`x-morphiecore-probe-id`）随 probe 移除一并清理 | 待做 |
| 继续删减只为转发厂商诊断/计费字段的开关（`routing_extras`、`reported_request_id`、`chat_stop_diagnostics`、`response_extras`、`responses_inactive_*`、`chat_inference_response_shape` 的 null-only 部分）及对应 fidelity 记录 | 待评估 |
| 可选：`morphiecore-auth` 并入主程序 `morphiecore auth …` | 待定 |

开放解码的遗留边界：OpenAI 响应解码目前用线程局部作用域（`openai::upstream_shapes`）区分上游与请求，阶段 1 改为显式方向；OpenAI SSE 的未知事件类型仍拒绝；冻结的媒体解码器未改动。

退出条件：离线基线全绿；被删测试覆盖的失败类型确认已有其他测试承担或已不再适用。

## 后续阶段

### 阶段 1 — 协议模块化，接通 Anthropic 上游

- lowering / adapter 从 `openai::Profile` 解耦：每个协议一个双向模块，每个上游一份 `TargetCaps`；方言规则收进协议模块。
- Anthropic Messages 上游进入 Gateway 执行链（请求头、响应解码、候选准备）。
- Anthropic SSE → `StreamEvent` 增量映射，替代先聚合后交付。
- 将 Anthropic 原生 thinking 接入现有下游 reasoning 信封；上游 Chat `reasoning.text` 项的 `signature` 目前被忽略，需要建模，并验证跨协议的 message/block 顺序。

退出条件：`Responses / Chat × Anthropic` 的 JSON 与 SSE 覆盖文本、工具往返、工具错误、thinking 回传、截断与上游错误，有 synthetic 测试，并用一次有界真实调用验证（opencode-go `claude-haiku-5-5` 或其他可用目标）。

### 阶段 2 — 跨协议一致性矩阵

每个场景每种协议一份独立 fixture，用 synthetic upstream 跑全部"下游 × 上游"组合：文本、单个/并行工具、工具错误、reasoning 回传、usage、截断、上游错误、提交后断流。之后接入协议主要是补 fixture。

### 阶段 3 — Google generateContent 上游

需要先确认的 IR 问题：

- `thoughtSignature` 挂在 part（含 `functionCall`）上，确认 `Part.replay` / `CallContext.replay` 能直接承载。
- `functionCall.id` 可能缺失：网关需生成 call ID 并按名称与顺序回映射，确认 `NativeCallAlias` 是否足够，不够则作为 IR 缺口先报告方案。
- 流式响应是 `GenerateContentResponse` 片段，形态接近 Chat chunk。
- 更多 `finishReason`（SAFETY、MALFORMED_FUNCTION_CALL 等）与 usage 分项（`thoughtsTokenCount`、`cachedContentTokenCount`）。
- Schema 采用 OpenAPI 子集或 `parametersJsonSchema`。

### 阶段 4 — Google Interactions 上游

以无状态方式提交完整历史，不依赖服务端存储；复用 opaque 信封。接入前固定官方 API 版本。

### 阶段 5 — 下游 Anthropic Messages

复用阶段 1 的双向 Anthropic 模块，补请求解码、响应与 SSE 编码。

### 之后

有状态 Responses（网关自管限时历史）、文件服务、媒体任务恢复扩展、Realtime。

## 待决问题

- Chat 扩展字段的完整列表：除 reasoning 外，工具错误、refusal 细节等是否需要扩展载体。
- 下游请求中未知字段的处理：忽略并记录，还是对标准中存在但未实现的字段拒绝。
- 切换 Provider/模型时，可见 thinking 是否转为 assistant 文本继续提供给新目标。
