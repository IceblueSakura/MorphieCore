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
| 文档收敛为 README / architecture / roadmap / usage / development / references，精简 AGENTS.md | 完成 |
| 移除 probe 框架（Python/TS 工具、live 示例），保留 SDK loopback gate | 完成 |
| 上游响应改为开放解码：已知字段严格、未知字段忽略；删除随之多余的方言开关 | 下一步 |
| Opaque reasoning 改用带标签信封；删除 AEAD 载荷、前缀 receipts 与依赖证明 | 待做 |
| `gateway::diagnostics` 的 probe 关联（`x-morphiecore-probe-id`）随 probe 移除一并清理 | 待做 |
| 可选：`morphiecore-auth` 并入主程序 `morphiecore auth …` | 待定 |

退出条件：离线基线全绿；被删测试覆盖的失败类型确认已有其他测试承担或已不再适用。

## 后续阶段

### 阶段 1 — 协议模块化，接通 Anthropic 上游

- lowering / adapter 从 `openai::Profile` 解耦：每个协议一个双向模块，每个上游一份 `TargetCaps`；方言规则收进协议模块。
- Anthropic Messages 上游进入 Gateway 执行链（请求头、响应解码、候选准备）。
- Anthropic SSE → `StreamEvent` 增量映射，替代先聚合后交付。
- Chat 下游的 `reasoning` / `reasoning_details` 扩展字段与 opaque 信封接通。

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
- Opaque 信封是否附加 HMAC。
- 切换 Provider/模型时，可见 thinking 是否转为 assistant 文本继续提供给新目标。
