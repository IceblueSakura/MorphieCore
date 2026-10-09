# pi Provider、上下文与回放来源

本页保留用于 MorphieCore 设计的固定实现入口；本项目决策归[ADRs](../architecture/README.md#架构决策)，合同归[Semantic Model](../architecture/semantic-ir.md)和[投影](../architecture/protocol-and-lowering.md)。

## 来源与版本

- Provider/API/Model 与 replay：pi `v1.0.2`，提交 [`cd32f7725fdbddbaecdff5b1e68491563394e0ca`](https://github.com/earendil-works/pi/tree/cd32f7725fdbddbaecdff5b1e68491563394e0ca)，[MIT](https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/LICENSE)。
- ClientManaged 上下文：pi `v1.0.4`，提交 [`7c10bd4337495ee613f2224843ecdf349b80d1df`](https://github.com/earendil-works/pi/tree/7c10bd4337495ee613f2224843ecdf349b80d1df)，[MIT](https://github.com/earendil-works/pi/blob/7c10bd4337495ee613f2224843ecdf349b80d1df/LICENSE)。
- Codex cache/header 专项仍使用[扩展与上下文](extensions-and-context.md)的独立固定来源；SIWC 登录来源见[SIWC](siwc-login.md)。

## Provider / API / Model

| 入口（v1.0.2） | 用途 |
|---|---|
| [models.ts](https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/ai/src/models.ts) | Provider 集合、认证、目录与分派；Provider 可组合多个 API |
| [types.ts](https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/ai/src/types.ts) | Model 描述、消息、options 与兼容类型 |
| [model-runtime.ts](https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/coding-agent/src/core/model-runtime.ts) | Coding Agent 的受信配置与 Provider 组合 |
| [OpenAI Chat](https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/ai/src/api/openai-completions.ts) / [Responses](https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/ai/src/api/openai-responses.ts) | transcript→wire、响应/事件归一化与目标特例 |
| [Responses shared](https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/ai/src/api/openai-responses-shared.ts) | item/终态消费、reasoning 与必要 replay 的保存/回传 |
| [event-stream.ts](https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/ai/src/utils/event-stream.ts) | partial 内容、block identity 与唯一终态 |
| [auth/resolve.ts](https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/ai/src/auth/resolve.ts) | 认证解析与 credential-store 刷新协调 |

pi 将 Provider 行为、可复用 API implementation 和 Model 描述分开。其 API implementation 同时执行 I/O；MorphieCore 的纯 codec/lowering 分层按 [ADR 0003](../architecture/decisions/0003-codec-lowering-boundary.md)处理。

<a id="client-managed-projection"></a>
## ClientManaged 上下文投影

v1.0.4 来源：

- [SessionManager](https://github.com/earendil-works/pi/blob/7c10bd4337495ee613f2224843ecdf349b80d1df/packages/coding-agent/src/core/session-manager.ts)：选择 active branch/compaction 范围，应用 context edits，保留 source entry。
- [AgentSession](https://github.com/earendil-works/pi/blob/7c10bd4337495ee613f2224843ecdf349b80d1df/packages/coding-agent/src/core/agent-session.ts)：从 canonical projection 重建上下文。
- [Agent loop](https://github.com/earendil-works/pi/blob/7c10bd4337495ee613f2224843ecdf349b80d1df/packages/agent/src/agent-loop.ts)：`transformContext` 先于 `convertToLlm`，工具执行由 Agent 负责。
- [Transcript helpers](https://github.com/earendil-works/pi/blob/7c10bd4337495ee613f2224843ecdf349b80d1df/packages/ai/src/utils/transcript.ts) / [Compaction](https://github.com/earendil-works/pi/blob/7c10bd4337495ee613f2224843ecdf349b80d1df/packages/coding-agent/src/core/compaction/compaction.ts)：有效配置重建、摘要与 retained range。

本项目历史选择、变换和依赖重验归[ClientManaged 合同](../architecture/interaction-contract.md#client-managed-context)，持久化与自动摘要由调用方负责。

## 跨模型回放

v1.0.2 的 [transformMessages](https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/ai/src/api/transform-messages.ts)用 Provider/API/Model 三者匹配判断同目标回放；跨目标会转 thinking 为 text、删除不兼容签名、重写工具 ID。它还可能用文本替代图片、合成缺失结果，并跳过 error/aborted assistant。

这些是客户端变换的来源事实。本项目允许损失归[投影合同](../architecture/protocol-and-lowering.md#semantic-loss)，未实施的跨目标方向归[next-goal](../implementation-plans/next-goal.md#cross-target-history)；不直接采用媒体占位或合成执行结果。
