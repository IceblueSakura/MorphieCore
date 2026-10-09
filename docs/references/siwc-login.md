# SIWC 登录与 ChatGPT plan usage 来源

本项目操作与恢复归[凭据指南](../credentials.md)，架构归 [ADR 0012](../architecture/decisions/0012-grok-personal-credential-pool.md)，准确登录参数与推理准入分别归 [driver](../../src/credential/siwc.rs)和 [adapter](../../src/adapter/siwc.rs)。

## 官方来源与许可

| 来源 | 用途 |
|---|---|
| [Quickstart][quickstart]、[OSS Overview][overview] | Identity/plan usage、client 与 host 生命周期 |
| [Sign-in][sign-in]、[Sessions][sessions]、[Tokens][tokens] | 动态 registration、账户、权限、rotation 与退出 |
| [Models/inference][inference]、[Preview][limitations]、[Errors][errors] | 公共 Responses、发现、准入与恢复 |
| [UI/UX][ux]、[Cookbook][cookbook] | 应用呈现、用户控制与主进程集成 |
| [VM][vms]、[app-server][app-server] | 特定远程 host 与应用内子组件 |
| [Website OIDC][website]、[Client request][request-client] | 商业 identity 集成资格 |
| [Discovery][discovery]、[JWKS][jwks] | 受信 issuer/端点与签名 key |
| [SIWC Terms][terms]、[Service Terms][service-terms]、[Terms of Use][terms-of-use]、[Usage Policies][usage-policies] | 用途、数据、存储与禁止行为 |

SIWC Terms 页面发布日期为 2026-09-29；动态页面在相关操作前核对，本地整理不刷新验证日期。

DevKit 参考固定为 `f723814abdccec135b519c451fb6e1992ee5e933`：[README][devkit-readme]、[security][devkit-security]、[LICENSE][devkit-license]。其 **Noncommercial License v1.0** 与本项目 MIT 分开；为雇主/客户或商业优势开发测试不自动满足 Noncommercial Purpose。MorphieCore 独立实现公开合同，不引入 DevKit/Node sidecar；接口通信的独立软件例外不授予额外服务访问权。

## 应用用途与资格

Identity 与 ChatGPT plan usage 分开授予。Identity 登录不授予推理；plan usage 使用该 registration 的实际 granted scopes 和套餐资格。

[Terms][terms]要求：请求为已认证用户服务，由本人活动或明确授权自动化触发；只供连接的应用使用；不得共享/汇集/转售套餐或轮换账户规避额度；SIWC 不得作为应用收费解锁条件。如实呈现应用名称与隐私控制，不索取密码或 session cookie。

本项目用途与 credential ownership 归 [ADR 0012](../architecture/decisions/0012-grok-personal-credential-pool.md)。

## 注册与身份

- OSS flow 是动态 public-client registration。首次 callback 返回 issued client，后续登录/refresh/revoke 复用；host ID 有独立稳定生命周期。
- 每次授权绑定 state、nonce、PKCE 与准确 redirect URI。首次注册与返回登录的参数区别由 [sign-in][sign-in]拥有。
- Callback scope 是提示，最终权限来自验证后的 grant。ID-token 验证 issuer、issued-client audience、签名、expiry 和 nonce；重登匹配原 registration principal。
- Identity-only 可保存，但不可借用套餐推理。账户/workspace、client 与 scopes 不按 email/alias 合并。
- ID/access/refresh token 分别用于身份、推理与续期。Access 的 encrypted metadata 保持 opaque。

共用标准归 [OAuth/OIDC](README.md#oauth-standards)，本地 callback、账户和 pending 状态归[凭据实现](../../src/credential/manager.rs)。

## Token 与退出

[Token reference][tokens]定义该 flow 的 expiry 与 rotation；实现按实际响应处理。`earliest_refresh_at` 的单位/调度语义仍需明确后才采用。

Refresh 使用原 issued client、resource 与 refresh token，省略 scope。Replacement 原子发布；本地 token-consumption、pending verification 和不确定结果恢复以[凭据指南](../credentials.md#交互与身份)为准。

[Sessions][sessions]的 revoke 使用受信 discovery 端点。空 HTTP 200 是协议成功，local sign-out、远端 revoke、registration 删除和已经发出的推理分别处理。

## 模型发现与 HTTP Responses

[Models/inference][inference]使用公共 `api.openai.com/v1/models` 与 `/responses`。账户目录为 `models` 数组，显示名与 inference slug 分开；不直接套 Platform `data` 形状。

[Preview][limitations]采用完整 array history、`store:false`、`stream:true`，function/custom 工具按 namespace 分组。不支持的显式控制（包括上游输出 token cap）、hosted tool 或媒体分支在目标准入层拒绝。准确字段集合归 adapter，而非参考页副本。

完成依据真实终态与 SSE EOF；incomplete/failed/cancel 分开。JSON 下游由交付 owner 有界聚合。不会把 SIWC token 改送 Codex backend，或继承其 account/turn-state headers。

## Headers、cache 与应用内 app-server

[Request-ID 合同][api-overview]中的 `X-Client-Request-Id` 是请求追踪，不是 thread/session/cache identity。其他 carrier 来源归[扩展与上下文](extensions-and-context.md)。

[app-server][app-server]作为应用内子组件时使用自己取得的 SIWC access、公共 Responses 配置和如实 clientInfo；应用负责续期，refresh/ID token 不交给子组件。此集成不是 MorphieCore 的依赖。

## 错误与用户控制

[Errors][errors]区分授权/资格、额度、临时 usage 检查、能力与路由错误；具体分类归 driver/Provider。额度失败停止新请求并提供 [Usage](https://chatgpt.com/settings/usage)入口，临时错误不等于账户撤销。未知错误不自动换账户或计费路径。

[UI/UX][ux]规定 `Continue with ChatGPT`、plan-use 状态、用量管理与退出控制；后台活动需要明确范围、期限、预算和停止方式。

## 远程 host 与分布式应用边界

[VM 指南][vms]允许描述的单用户 credential 转移，要求独立 host ID 与后续 refresh owner；host attribution/revocation 仍有限制。[Terms][terms]的持久化条款与该指南存在适用范围疑问，**远程持久化或复制前确认具体部署合同**。同用户多 host 不产生独立 allowance，也不能多主消费 rotating token。

## 补充实现来源

pi `1.0.2` / `cd32f7725fdbddbaecdff5b1e68491563394e0ca`（[MIT][pi-license]）的 [SIWC module][pi-siwc]、[Responses][pi-responses]和 [resolver][pi-resolver]；仅为来源，不替代官方合同。

[quickstart]: https://developers.openai.com/siwc/quickstart
[overview]: https://developers.openai.com/siwc/token-sharing-open-source
[sign-in]: https://developers.openai.com/siwc/token-sharing-open-source/sign-in
[sessions]: https://developers.openai.com/siwc/token-sharing-open-source/profiles-and-sessions
[tokens]: https://developers.openai.com/siwc/token-sharing-open-source/token-reference
[inference]: https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference
[limitations]: https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations
[errors]: https://developers.openai.com/siwc/token-sharing-open-source/errors-and-recovery
[ux]: https://developers.openai.com/siwc/ui-ux-guidelines
[vms]: https://developers.openai.com/siwc/token-sharing-open-source/self-hosted-vms
[app-server]: https://developers.openai.com/siwc/token-sharing-open-source/codex-app-server
[website]: https://developers.openai.com/siwc/website
[request-client]: https://developers.openai.com/siwc/request-client-id
[discovery]: https://auth.openai.com/.well-known/openid-configuration
[jwks]: https://auth.openai.com/.well-known/jwks.json
[terms]: https://openai.com/policies/sign-in-with-chatgpt-terms/
[service-terms]: https://openai.com/policies/service-terms/
[terms-of-use]: https://openai.com/policies/row-terms-of-use/
[usage-policies]: https://openai.com/policies/usage-policies/
[cookbook]: https://developers.openai.com/cookbook/articles/sign-in-with-chatgpt
[devkit-readme]: https://github.com/openai/sign-in-with-chatgpt-devkit/blob/f723814abdccec135b519c451fb6e1992ee5e933/README.md
[devkit-security]: https://github.com/openai/sign-in-with-chatgpt-devkit/blob/f723814abdccec135b519c451fb6e1992ee5e933/docs/security.md
[devkit-license]: https://github.com/openai/sign-in-with-chatgpt-devkit/blob/f723814abdccec135b519c451fb6e1992ee5e933/LICENSE
[api-overview]: https://developers.openai.com/api/reference/overview
[pi-license]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/LICENSE
[pi-siwc]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/ai/src/auth/oauth/openai-chatgpt.ts
[pi-responses]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/ai/src/api/openai-responses.ts
[pi-resolver]: https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/ai/src/auth/resolve.ts
