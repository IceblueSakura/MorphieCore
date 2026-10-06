# 固定上游来源

这里固定设计所使用的来源，不记录同步过程、历史差异或测试结果。官方页面的既有基线日期为 **2026-09-23**；动态网页没有永久版本，本地文档整理不刷新该日期，也不代表重新核验上游。

## 版本与许可

| 来源 | 固定版本 | 用途与边界 |
|---|---|---|
| OpenAI Python SDK | [`be9d66628ad7377bd36fe5a76ae6d735843f0e76`](https://github.com/openai/openai-python/tree/be9d66628ad7377bd36fe5a76ae6d735843f0e76)，`3.19.0`，[Apache-2.0](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/LICENSE) | 交叉核对类型、required/nullable/union；不是运行验证 |
| Codex 既有基线 | [`a69d757cd8ef8310001186865911b69e4b4175e5`](https://github.com/openai/codex/tree/a69d757cd8ef8310001186865911b69e4b4175e5)，[Apache-2.0](https://github.com/openai/codex/blob/a69d757cd8ef8310001186865911b69e4b4175e5/LICENSE) | 既有语义/codec 来源，不是 OpenAI 公共 API；新的登录与客户端上下文来源分别由 [ChatGPT 参考](chatgpt-login.md)和 [扩展与上下文](extensions-and-context.md)固定，不由专项参考隐式升级本基线 |
| 本地消费者 gate | [pyproject.toml](../../tests/sdk/pyproject.toml) 与 [uv.lock](../../tests/sdk/uv.lock) | 可执行依赖由锁文件拥有，不由研究版本隐式升级；命令见[开发指南](../development.md#固定-openai-sdk-loopback) |

## 一手入口

- [Create a response](https://developers.openai.com/api/reference/resources/responses/methods/create)
- [Streaming events](https://developers.openai.com/api/reference/resources/responses/streaming-events)
- [Reasoning guide](https://developers.openai.com/api/docs/guides/reasoning)
- [Structured Outputs](https://developers.openai.com/api/docs/guides/structured-outputs)
- [WebSocket mode](https://developers.openai.com/api/docs/guides/websocket-mode)
- SDK [Responses types](https://github.com/openai/openai-python/tree/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses)：分别核对 create params、input/output item、response snapshot 与 stream event，不能用 input 简写放宽完整响应。
- SDK [Chat types](https://github.com/openai/openai-python/tree/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/chat)、[usage](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/completion_usage.py)、[reasoning](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/shared/reasoning.py)。
- Codex [client](https://github.com/openai/codex/blob/a69d757cd8ef8310001186865911b69e4b4175e5/codex-rs/core/src/client.rs)、[responses metadata](https://github.com/openai/codex/blob/a69d757cd8ef8310001186865911b69e4b4175e5/codex-rs/core/src/responses_metadata.rs)、[Responses endpoint](https://github.com/openai/codex/blob/a69d757cd8ef8310001186865911b69e4b4175e5/codex-rs/codex-api/src/endpoint/responses.rs)、[headers](https://github.com/openai/codex/blob/a69d757cd8ef8310001186865911b69e4b4175e5/codex-rs/codex-api/src/requests/headers.rs)、[items](https://github.com/openai/codex/blob/a69d757cd8ef8310001186865911b69e4b4175e5/codex-rs/protocol/src/models.rs)。

## 使用规则

公开 reference/guide 定义目标语义，SDK 用于交叉核对。Open Responses、Codex tolerant parser 和其他网关的兼容策略不是官方标准替代品。冲突须在受影响合同或 owning code 中明确分类，不默默选取更方便实现的一份。

新增子域时固定相关 schema/SDK/profile，并分别确认 request、response、event 和生命周期，不能从事件名猜出完整任务。按需保留必要来源和许可，不保存页面抽取日志、hash 清单或历史分析。字段准入看当前代码；Provider 可用性与 SDK/Agent 执行需要另外验证。
