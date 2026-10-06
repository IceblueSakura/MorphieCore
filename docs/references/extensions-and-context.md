# 扩展语义、上下文与来源约束

本页拥有 session/cache/turn 的来源与生命周期边界，不记录调查或执行结果。[固定标准/codec 基线](upstream-sync.md)保持原用途；下面的 Codex 上下文参考采用 `d25c114d494ddb693290b76bf5e5f64ecbdb38fc` 的 [client][codex-client]、[metadata][codex-metadata]和 [Responses endpoint][codex-responses]，不隐式升级 SDK、codec 或本地准入。登录与 credential 合同分别归 [ChatGPT](chatgpt-login.md)和 [Grok](grok-login.md)参考。

本页区分外部事实与设计约束；接受的 IR 所有权与[客户端扩展决策](../architecture/semantic-ir.md#3-客户端-api-目标与扩展边界)由 semantic IR 维护。内部 scoped 语义不自动授权新的公开 API，固定产品的私有 carrier 也不替代标准 Responses。扩展不是裸 `extra_body` 或 `extra_headers` 透传口。

## 1. 扩展的三种用途

| 用途 | 示例 | 不能误归为 |
|---|---|---|
| Scoped 任务能力 | 尚有特定 profile 含义的媒体控制、tool signature、特殊输出控制 | 已有共享字段的第二份拷贝 |
| 请求/会话上下文 | Codex logical session、thread/window、agent lineage | prompt 文本、全局万能 SessionId |
| 受限 opaque replay | Provider 签发 token、signature、turn-state | 可自由跨目标复制的用户 JSON |

已理解的 `phase`、reasoning context、hosted tools 等能力保留 typed owner，不因目标 codec 不支持而退回 JSON。共享语义与 scoped extension 的边界依据含义和生命周期，而非哪家先定义字段。不是所有 `x-*`、body 私有字段都属于同一扩展；位置不是语义所有权。

## 2. Codex session_id 的实际含义

这里的 logical session 不指 OAuth renewable session、client registration 或安装/host identity；登录得到的 client ID、`ext_agent_host_id`、账户标识和 token 均不是缓存 key 的默认来源。固定 Codex 产品快照区分以下事实：

| 事实 | Wire 投影与生命周期 | 基线约束 |
|---|---|---|
| Logical session | `client_metadata.session_id` / turn metadata | 真实会话来源提供；不以请求 UUID 或缓存 hash 冒充 |
| Cache affinity | root agent 的 `session-id` 采用 effective prompt cache key | 与 logical session 分开；non-root agent 使用 logical session |
| Thread | `thread-id`；当前 `x-client-request-id` 同样用 thread identity | 不解释为每 HTTP attempt 唯一 ID |
| Context window | window identity / `x-codex-window-id` | compaction 后可变化，不等于 thread |
| Turn sticky state | server-issued `x-codex-turn-state`，同 turn 重放 | 不可派生；新 turn 或 auth ownership 变化清除 |
| Canonical metadata | `client_metadata["x-codex-turn-metadata"]` | flat body keys 和 HTTP/WS headers 是同一事实的投影 |
| Credentials/account | Authorization、account/compliance headers | 不进入普通扩展；由安全 binding owner 在执行时处理 |

`prompt_cache_key()` 的优先级为显式 override、internal session 有 parent thread 时的 source+parent-thread、logical session；root `responses_session_id()` 采用该 effective cache key，non-root 使用 logical session。root agent 因而可以同时有不同的 logical session `S` 和 cache affinity `K`：

```text
body.prompt_cache_key      = K
HTTP session-id            = K
client_metadata.session_id = S
```

这是固定 Codex 产品 profile 的投影，不是公共 Responses、公开 SIWC 或 MorphieCore body `session_id` 的默认别名规则。MorphieCore 公开 cache grouping 的准入与投影归 [HTTP 合同](../http-gateway.md)和 [cache carrier](../../src/protocol/cache.rs)，不因这里的外部映射自动改变。

HTTP header 与 WebSocket handshake/message 的位置不同；turn-state 还可能进入 WS message client metadata。扩展 schema 应描述事实与生命周期，再由 profile encoder 投影，不能同时保留多个可矛盾的 canonical 值。`protocol::extensions` 有低层 carrier 不代表 Gateway 已拥有 turn、接线私有 headers 或管理上游连接。

### 缓存、存储与连接状态

- **Provider prompt cache**复用实际兼容前缀的计算；key 是亲和 hint，不替代 instructions、tools/Schema 和必要历史。稳定 key 不证明命中、成本或延迟改善，效果需要独立且获准的对照。
- **Response storage**与 prompt cache 不同；`store:false` 不意味着关闭 prompt cache，也不证明所选 transport 没有短期状态。
- **连接级 continuation**可以依赖持有的 WebSocket、previous response 及增量前缀；它不是 prompt cache，也不是可跨连接、账户或 Endpoint 任意查询的持久 response。采用前必须选定 transport owner、来源 scope、前缀/设置验证及断连、取消和 ownership 变化时的失效行为。
- **Turn sticky state**只由服务器签发，在拥有的同一 turn 原样重放；新 turn 和 auth ownership 变化须清除。token rotation 与身份/ownership 改变分别判断，不能把每次 refresh 都当作新 logical session，也不能仅凭账户名称相同保留所有 opaque state。

稳定前缀与现有依赖检查归 [semantic cache](../../src/semantic/cache.rs)和 [ADR 0011](../architecture/decisions/0011-stable-admission-provider-cache.md)：追加前缀之后的历史与修改前缀、工具顺序、有效设置或可信 scope 不等价。`CachePrefixProof` 的内部 digest 不是上游 cache key 或 cache-hit 事实。其他客户端的 prefix hash 只能作为独立策略的来源线索；不自动哈希整轮 body、生成 session/thread/turn identity 或恢复网关粘性路由。

### pi 的选定客户端投影

来源固定为 pi-ai `0.99.2`、`earendil-works/pi@005af57d88ee23b33778f343a9595b32e67ff788`（[MIT][pi-license]），不是全部消费者或上游服务的通用合同：

- [Codex API adapter][pi-codex-api]从显式 `sessionId` 选择 cache key；SSE 把所选值投影到 `session-id` / `x-client-request-id`，与官方 Codex 的 thread identity 规则分别核对。WebSocket 连接缓存按 session/account 分组，增量请求只有在设置及已消费历史前缀匹配时使用 `previous_response_id`；失败会清除 continuation，不能由 key 相同推定可重放。
- [公共 Responses adapter][pi-responses-api]保留所选 cache key，但公开 SIWC 分支省略 retention/options 等不接受的控制。它与 Codex backend 的 headers、连接状态和字段准入分别定稿；客户端省略 key 不等于对所有 Provider 显式禁用缓存。
- [Cache-key helper][pi-cache-key]将 key 限制并截到 64 字符。这是固定客户端的本地选择，不替代 MorphieCore 的公开接入预算或目标限制。直接移植截断可能合并不同 key；目标的长度、编码与拒绝/映射合同需明确，不能默默截断来宣称无损。

这些投影不授予登录、账户发现、连接复用或业务请求 retry 权限；不采用参考客户端的自动 fallback 作为 Gateway 默认策略。

## 3. 扩展准入要求

每个可编码扩展至少需要明确：

- namespace、kind、schema version 和固定来源；
- attachment owner：request context、item、part、resource 或 event；
- typed payload／明确 schema 的 bounded opaque value；
- origin 与有效 scope、生命周期、是否 final/replayable；
- 信任来源、上下游可见性、日志/隐私级别；
- 可表示目标与跨 profile/issuer 的映射或拒绝条件；
- 对 requirements、删除/替换、终态、重试/fallback 的影响。

这是一份编译期合同，不要求先建动态插件系统。没有已验证 codec/目标 scope 的扩展不得因名字匹配就转发；未知数据可作为有界不可执行诊断，但不能无条件进入未来请求。

## 4. 信任与数据流

标准 `metadata` 是业务数据，不是扩展注册表；任意字符串不能选择 route、endpoint、credential 或脚本。扩展 namespace 可以声明语义来源，但不能授予对该 Provider 的访问权。真实 source scope 必须由可信边界绑定，不能接受客户端声称“我是某 issuer”。

hosted MCP 标准 schema 含 server URL、headers 与 authorization 等敏感入口。支持它的语法不等于解除网关安全边界：未来执行仍需受信目标准入和独立 credential binding，秘密不能塞进可日志化 IR。远程媒体 URL 也不授予 codec 下载权限。

session/thread/cache/turn 值即使不是密码，也可能敏感且高基数；默认不作为 metrics label 或普通日志字段，不在文档/fixture 保存真实值。

## 5. 共享语义演进与兼容

当 scoped 能力可以被独立、稳定地定义，或参考协议发生标准化变化时，先核对含义、presence、生命周期与依赖是否等价，再迁入唯一共享 owner。另一家出现同名字段不自动证明可合并；相同事实不能同时保留两份权威。旧 wire spelling 如仍需接受，由显式 profile codec 处理，不在 IR 保留 legacy alias。

现有非标准形式及其准入只由 [Responses profile](../architecture/responses-text-profile.md#control-message-and-annotation-admission-details)维护；本页不将其归为官方标准或批准进一步扩展。

## 6. 需要单独定稿的事项

现有低层 carrier 为 `protocol::extensions` 的 typed 生命周期字段与受限 opaque 值，凭据/传输类 fail-closed；它不代表新设计的所有语义 owner 或下游 carrier 已定稿，见 [ADR 0007](../architecture/decisions/0007-stateless-cache-affinity-and-extensions.md)。以下仍需解决：

- namespace 命名与版本协商；response 侧自定义段与观察类 header 的 typed 化；
- Codex 上下文是由客户端可信 adapter 提供、透明转发，还是由未来 Gateway 管理 turn；当前不自动生成身份或 sticky token；
- 外部 opaque state 的可信 scope 构造、失效、principal 隔离；
- 特殊多模态的具体 Provider/operation/schema；没有固定事实不预造字段全集。

这些事项不妨碍确定 Agent-first、协议中立和分层扩展基线，但在实现相应 wire 接口或 state owner 前必须解决。已有 Responses encrypted reasoning 的 carrier 不自动适用于其他 signature 或 redacted 内容。

[codex-client]: https://github.com/openai/codex/blob/d25c114d494ddb693290b76bf5e5f64ecbdb38fc/codex-rs/core/src/client.rs
[codex-metadata]: https://github.com/openai/codex/blob/d25c114d494ddb693290b76bf5e5f64ecbdb38fc/codex-rs/core/src/responses_metadata.rs
[codex-responses]: https://github.com/openai/codex/blob/d25c114d494ddb693290b76bf5e5f64ecbdb38fc/codex-rs/codex-api/src/endpoint/responses.rs
[pi-license]: https://github.com/earendil-works/pi/blob/005af57d88ee23b33778f343a9595b32e67ff788/LICENSE
[pi-codex-api]: https://github.com/earendil-works/pi/blob/005af57d88ee23b33778f343a9595b32e67ff788/packages/ai/src/api/openai-codex-responses.ts
[pi-responses-api]: https://github.com/earendil-works/pi/blob/005af57d88ee23b33778f343a9595b32e67ff788/packages/ai/src/api/openai-responses.ts
[pi-cache-key]: https://github.com/earendil-works/pi/blob/005af57d88ee23b33778f343a9595b32e67ff788/packages/ai/src/api/openai-prompt-cache.ts
