# Grok Build / xAI 登录来源

本项目登录与存储操作归[凭据指南](../credentials.md)，最终架构归 [ADR 0012](../architecture/decisions/0012-grok-personal-credential-pool.md)。本页只维护外部协议差异与固定来源。

## 来源与版本

| 来源 | 固定版本 / 许可 |
|---|---|
| 官方 [Authentication][grok-auth-doc]、[Enterprise][grok-enterprise]、[CLI][grok-cli-doc] | 产品登录、企业策略和网络合同 |
| Grok Build 源码 | `2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8`，[Apache-2.0][grok-license]；[config][grok-config]、[browser][grok-browser]、[OIDC][grok-oidc]、[device][grok-device]、[HTTP metadata][grok-http]、[refresh][grok-refresh] |
| 官方发布 metadata | [`@xai-official/grok@1.0.46`][grok-release-metadata]；与源码提交独立固定 |
| pi xAI | `0.99.2` / `005af57d88ee23b33778f343a9595b32e67ff788`，[MIT][pi-license]；[OAuth][pi-xai-oauth]、[polling][pi-device]、[resolver][pi-resolve] |
| 补充插件 | `pi-xai-oauth 1.6.0` / `f2408b0dc108103e0568e6ab02ee0b30644254bb`，[MIT][plugin-license]；[OAuth][plugin-oauth]、[OIDC][plugin-oidc]、[device][plugin-device]、[wire][plugin-wire] |
| Authority | [Discovery][xai-discovery]、[JWKS][xai-jwks] |

插件与 pi 固定版是独立参考，不是安装兼容建议。第三方 client/redirect/scope 复用资格由适用 authority 合同确认。

## 必要协议差异

- 第一方 authority 为 `https://auth.x.ai`，浏览器与标准 RFC 8628 设备授权是不同 flow；企业 OIDC、外部认证程序和 API key 是独立产品入口。
- 官方与 pi 固定 client 的公开 ID 相同；公开 ID 不等于接入资格。准确默认 client、最小 scopes、UA/version 与 referrer 由 [Grok driver](../../src/credential/grok.rs)和 [metadata owner](../../src/credential/grok_metadata.rs)维护。
- 浏览器 flow 使用 PKCE/state/nonce。官方快照的手动裸 code 路径可绕过 state，本项目 [callback](../../src/credential/callback.rs)要求事务绑定。
- 设备 flow 直接返回 token，不再交换 authorization code；按 interval、pending/slow-down 与截止处理。官方设备分支用于显示的 decoded ID token 不等于已验证身份。
- Refresh 可省略 replacement refresh token；原 token 的保留由该 authority 的合同决定。官方与 pi 的 expiry fallback/提前余量属于各自客户端政策。
- 官方产品 session inference proxy 与公共 `api.x.ai` 是不同目标；登录身份、模型目录和推理资格分别核对。
- 本项目不读取官方 CLI auth cache，当前个人账户范围、身份验证、access binding 与 pool 行为以代码及[凭据指南](../credentials.md)为准。

共用标准见 [OAuth/OIDC](README.md#oauth-standards)，session/cache/turn 来源见[扩展与上下文](extensions-and-context.md)。

[grok-auth-doc]: https://github.com/xai-org/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-pager/docs/user-guide/02-authentication.md
[grok-enterprise]: https://docs.x.ai/build/enterprise
[grok-cli-doc]: https://docs.x.ai/build/cli/reference
[grok-license]: https://github.com/xai-org/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/LICENSE
[grok-config]: https://github.com/xai-org/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-login/src/config.rs
[grok-browser]: https://github.com/xai-org/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-login/src/oidc/login.rs
[grok-oidc]: https://github.com/xai-org/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-login/src/oidc/protocol.rs
[grok-device]: https://github.com/xai-org/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-login/src/device_code.rs
[grok-refresh]: https://github.com/xai-org/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-login/src/refresh/oidc_refresher.rs
[grok-http]: https://github.com/xai-org/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-http/src/lib.rs
[grok-release-metadata]: https://registry.npmjs.org/@xai-official/grok/1.0.46
[pi-license]: https://github.com/earendil-works/pi/blob/005af57d88ee23b33778f343a9595b32e67ff788/LICENSE
[pi-xai-oauth]: https://github.com/earendil-works/pi/blob/005af57d88ee23b33778f343a9595b32e67ff788/packages/ai/src/auth/oauth/xai.ts
[pi-device]: https://github.com/earendil-works/pi/blob/005af57d88ee23b33778f343a9595b32e67ff788/packages/ai/src/auth/oauth/device-code.ts
[pi-resolve]: https://github.com/earendil-works/pi/blob/005af57d88ee23b33778f343a9595b32e67ff788/packages/ai/src/auth/resolve.ts
[plugin-license]: https://github.com/BlockedPath/pi-xai-oauth/blob/f2408b0dc108103e0568e6ab02ee0b30644254bb/LICENSE
[plugin-oauth]: https://github.com/BlockedPath/pi-xai-oauth/blob/f2408b0dc108103e0568e6ab02ee0b30644254bb/extensions/xai/oauth.ts
[plugin-oidc]: https://github.com/BlockedPath/pi-xai-oauth/blob/f2408b0dc108103e0568e6ab02ee0b30644254bb/extensions/xai/oidc.ts
[plugin-device]: https://github.com/BlockedPath/pi-xai-oauth/blob/f2408b0dc108103e0568e6ab02ee0b30644254bb/extensions/xai/device-auth.ts
[plugin-wire]: https://github.com/BlockedPath/pi-xai-oauth/blob/f2408b0dc108103e0568e6ab02ee0b30644254bb/extensions/xai/wire.ts
[xai-discovery]: https://auth.x.ai/.well-known/openid-configuration
[xai-jwks]: https://auth.x.ai/.well-known/jwks.json
