# Codex 产品协议识别

MorphieCore 的 ChatGPT plan usage 登录采用独立 [SIWC](siwc-login.md)，不提供 Codex 产品 browser/device 登录或产品 client 别名。本页仅区分产品协议与公共授权，并保留来源定位；不是可执行登录指南。

## 固定来源

Codex 产品认证参考固定为 **Codex CLI 0.160.0**、`openai/codex@a956835d020762cb2b570053af06f643a11c0ecc`（[release][release]、[Apache-2.0][license]）。它不升级其他语义、codec 或客户端上下文基线，不授予第三方复用产品 client/backend 的资格。

官方 [Authentication][auth-doc] 与 [Access tokens][access-doc] 拥有产品用途；[Browser server][server]、[Authorization][authorization]、[Device auth][device]、[OAuth client][oauth]、[Default auth client][default-client] 和 [Revocation][revoke] 分别拥有产品 callback、设备交互、token、metadata 与撤销 wire。参考代码的采用须保留适用许可与 attribution。

## Codex 设备交互不是标准 device grant

产品设备步骤先取得 authorization code 与 PKCE 材料，再交换 token，不是 `urn:ietf:params:oauth:grant-type:device_code`。产品 polling 的 pending 状态不能推广为普通 OAuth 错误重试规则，也不用于 issued SIWC client。

## 授权隔离

产品 token、client、backend、账户 header 和 opaque history 不转换成 SIWC 权限，也不派生 Platform API key。公开 SIWC 必须独立 registration、验证 issuer/client/subject/nonce 与实际 scopes，并显式绑定公共 Responses 目标。代码不提供旧登录不代表已经清理本地旧材料或撤销远端 session；这些效果仍需分别授权。

通用文件存储与生命周期归 [凭据指南](../credentials.md)，产品 session/cache/turn 的独立来源归 [扩展与上下文](extensions-and-context.md)。

[release]: https://github.com/openai/codex/releases/tag/rust-v0.160.0
[license]: https://github.com/openai/codex/blob/a956835d020762cb2b570053af06f643a11c0ecc/LICENSE
[auth-doc]: https://learn.chatgpt.com/docs/auth
[access-doc]: https://learn.chatgpt.com/codex/enterprise/access-tokens
[server]: https://github.com/openai/codex/blob/a956835d020762cb2b570053af06f643a11c0ecc/codex-rs/login/src/server.rs
[authorization]: https://github.com/openai/codex/blob/a956835d020762cb2b570053af06f643a11c0ecc/codex-rs/login/src/oauth/authorization.rs
[device]: https://github.com/openai/codex/blob/a956835d020762cb2b570053af06f643a11c0ecc/codex-rs/login/src/device_code_auth.rs
[oauth]: https://github.com/openai/codex/blob/a956835d020762cb2b570053af06f643a11c0ecc/codex-rs/login/src/oauth/client.rs
[default-client]: https://github.com/openai/codex/blob/a956835d020762cb2b570053af06f643a11c0ecc/codex-rs/login/src/auth/default_client.rs
[revoke]: https://github.com/openai/codex/blob/a956835d020762cb2b570053af06f643a11c0ecc/codex-rs/login/src/auth/revoke.rs
