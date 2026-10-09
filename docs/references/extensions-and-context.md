# 扩展与上下文来源

项目所有权归 [ADR 0007](../architecture/decisions/0007-stateless-cache-affinity-and-extensions.md)，Provider cache 策略归 [ADR 0011](../architecture/decisions/0011-stable-admission-provider-cache.md)。本页只保存固定外部投影与生命周期差异。

## Codex 固定来源

上下文参考为 `openai/codex@d25c114d494ddb693290b76bf5e5f64ecbdb38fc` 的 [client][codex-client]、[metadata][codex-metadata]和 [Responses endpoint][codex-responses]。SDK/codec 的独立版本见[upstream-sync](upstream-sync.md)，认证来源见[Codex](chatgpt-login.md)与[SIWC](siwc-login.md)。

| 外部事实 | 固定投影 |
|---|---|
| Logical session | `client_metadata.session_id` 与 turn metadata |
| Cache affinity | root agent 的 `session-id` 使用 effective prompt cache key；non-root 使用 logical session |
| Thread | `thread-id`，该产品的 `x-client-request-id` 也使用 thread identity |
| Context window | `x-codex-window-id`；compaction 后可变化 |
| Turn state | 服务器签发的 `x-codex-turn-state`，同 turn 重放 |
| Canonical metadata | body `x-codex-turn-metadata` 与 HTTP/WS 的投影 |
| Credential/account | 认证与合规 headers，属于认证 owner |

`prompt_cache_key()` 按显式 override、source+parent-thread、logical session 选择。root agent 可以有不同的 logical session 与 cache affinity；这不是公共 Responses 或 SIWC 的默认别名规则。

HTTP 与 WS 的位置不同，新增 turn 或 auth ownership 改变会影响 sticky state。准确映射查固定源码，本项目当前公开 carrier 归 [cache codec](../../src/protocol/cache.rs)。

## 缓存、存储与连接状态

- Provider prompt cache 复用兼容前缀计算；亲和 key 不替代必要历史，也不保证命中。
- Response storage 独立于 prompt cache，`store:false` 不等于关闭缓存。
- 连接级 continuation 可依赖 WS、previous response 与增量前缀；使用范围取决于连接/auth 与设置/历史校验。
- Turn state 由服务器签发；token rotation、principal 改变和 logical session 分别判断。

这些分类用于识别协议事实；本项目前缀依赖和状态范围归[Semantic Model](../architecture/semantic-ir.md#7-控制计量与缓存)与[后续计划](../implementation-plans/next-goal.md#延期目标与恢复条件)。

## pi 的固定投影

pi-ai `0.99.2` / `005af57d88ee23b33778f343a9595b32e67ff788`（[MIT][pi-license]）：

- [Codex adapter][pi-codex-api]从 `sessionId` 选择 cache key，SSE 映射 session/request headers，WS 按 session/account 管理连接与 previous-response 前缀。
- [Responses adapter][pi-responses-api]保留所选 key，并省略公开 SIWC 不接受的控制。
- [Cache helper][pi-cache-key]截断 key 到 64 字符；这是客户端政策，目标映射需独立处理碰撞与长度。

其他 pi Provider 与 ClientManaged 来源见[pi 导航](pi-provider-abstraction.md)。本项目扩展 schema、scope、变换与准入归[交互](../architecture/interaction-contract.md)和[投影合同](../architecture/protocol-and-lowering.md)。

[codex-client]: https://github.com/openai/codex/blob/d25c114d494ddb693290b76bf5e5f64ecbdb38fc/codex-rs/core/src/client.rs
[codex-metadata]: https://github.com/openai/codex/blob/d25c114d494ddb693290b76bf5e5f64ecbdb38fc/codex-rs/core/src/responses_metadata.rs
[codex-responses]: https://github.com/openai/codex/blob/d25c114d494ddb693290b76bf5e5f64ecbdb38fc/codex-rs/codex-api/src/endpoint/responses.rs
[pi-license]: https://github.com/earendil-works/pi/blob/005af57d88ee23b33778f343a9595b32e67ff788/LICENSE
[pi-codex-api]: https://github.com/earendil-works/pi/blob/005af57d88ee23b33778f343a9595b32e67ff788/packages/ai/src/api/openai-codex-responses.ts
[pi-responses-api]: https://github.com/earendil-works/pi/blob/005af57d88ee23b33778f343a9595b32e67ff788/packages/ai/src/api/openai-responses.ts
[pi-cache-key]: https://github.com/earendil-works/pi/blob/005af57d88ee23b33778f343a9595b32e67ff788/packages/ai/src/api/openai-prompt-cache.ts
