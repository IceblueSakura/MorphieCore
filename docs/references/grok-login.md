# Grok Build / xAI 账户登录参考

本页维护 Grok Build 登录的来源、协议差异和采用边界，不是 MorphieCore 的已实现认证合同、模型清单或运行报告。共用标准归 [OAuth/OIDC 来源](README.md#oauth-standards)，授权与存储边界归[凭据指南](../credentials.md#授权与安全边界)；推进方向归 [next-goal](../implementation-plans/next-goal.md)。登录协议、账户/订阅权限和推理表示必须分别确定。

## 来源与版本

官方产品说明优先于第三方客户端的便利实现；固定源码用于定位具体流程，网页无永久版本，实施前需重新核对相关合同。

| 来源 | 固定版本 / 入口 | 用途 |
|---|---|---|
| Grok Build 官方文档 | [Authentication][grok-auth-doc]、[Enterprise deployments][grok-enterprise]、[CLI reference][grok-cli-doc] | 登录入口、企业策略与网络边界 |
| Grok Build 官方源码 | `xai-org/grok-build@2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8`，[Apache-2.0][grok-license] | [默认 client/scopes][grok-config]、[浏览器编排][grok-browser]、[OIDC 协议][grok-oidc]、[设备授权][grok-device]、[HTTP metadata][grok-http]、[版本注入][grok-version]、[存储][grok-storage]、[刷新][grok-refresh] |
| Grok 客户端发布 metadata 参考 | 官方 [`@xai-official/grok@1.0.46`][grok-release-metadata]；[stable 指针][grok-stable-pointer]提供发布导航 | 固定 UA/version header 的版本参考；与 wire 源码提交独立，不声称该提交就是此发布的构建 revision，不在运行时查询或自动升级 |
| pi coding agent / pi-ai | `0.99.2`，`earendil-works/pi@005af57d88ee23b33778f343a9595b32e67ff788`，[MIT][pi-license] | [内置 xAI OAuth][pi-xai-oauth]、[设备轮询][pi-device]、[刷新协调][pi-resolve]、[xAI Provider][pi-xai-provider] |
| pi-xai-oauth，补充参考 | `1.6.0`，`BlockedPath/pi-xai-oauth@f2408b0dc108103e0568e6ab02ee0b30644254bb`，[MIT][plugin-license] | [浏览器/刷新][plugin-oauth]、[OIDC 校验][plugin-oidc]、[有界设备授权][plugin-device]、[proxy headers][plugin-wire] |
| 公开 authority 元数据 | [OIDC discovery][xai-discovery]、[JWKS][xai-jwks] | authority 声明；不是账户授权或成功推理的证据 |

本页不改变 [upstream-sync](upstream-sync.md) 的 OpenAI/Codex 语义版本。参考代码未成为本项目依赖；采用代码时另行保留其许可证与 attribution。

## 登录方式与产品边界

- **浏览器登录**：`grok login` 默认进入 xAI OAuth/OIDC；`--oauth` 显式选择同一路径。
- **设备码登录**：`grok login --device-auth`，别名 `--device-code`，用于本机浏览器不可达的环境；仍需要人完成远端授权。
- **企业 OIDC**：操作者配置企业 IdP、合法 public client、callback、scope/audience，使用该 IdP 的 discovery 和 PKCE。它不是任意业务请求可选的 authority。
- **外部认证程序**：受信操作者配置 `auth_provider_command`，程序的 stdout 返回裸 token，或含 `access_token` 及可选 `refresh_token` / `expires_in` 的 JSON；stderr 用于交互提示。官方以 `GROK_AUTH_EXPIRED=1` 区分无人值守刷新，交互 sign-in 则允许用户完成授权；后台刷新不能等待输入。普通请求不授权脚本执行。
- **API key**：`XAI_API_KEY` 或受信 per-model 配置提供独立 API-key 接入；不是可刷新的订阅 session。

官方解析还区分 per-model key、per-model env key、活动 session 与全局 API key。企业 `disable_api_key_auth` / `force_login_team_uuid` 可进一步限制登录方式及 team principal。不要在网关复制一套隐式凭据 fallback，也不要把个人登录、team 登录、SuperGrok、X Premium 或企业资格视为互相等价。

上面的命令是官方客户端入口，不是 MorphieCore 命令；执行登录、保存或撤销凭据仍需明确授权。

## xAI authority 与 public client

第一方 authority 为 `https://auth.x.ai`。对应公开入口：

- discovery：`/.well-known/openid-configuration`；
- authorization：`/oauth2/authorize`；
- device authorization：`/oauth2/device/code`；
- token / refresh：`/oauth2/token`；
- JWKS：`/.well-known/jwks.json`。

Discovery 声明 authorization-code、refresh 和 `urn:ietf:params:oauth:grant-type:device_code`，PKCE 为 `S256`；当前第一方 ID-token 签名策略由 discovery/JWKS 核对，不能从任意 JWT header 接受算法、key 或 key URL。浏览器进入的账户 UI 可以位于 `accounts.x.ai`，不因此改变 token authority。

固定官方 CLI 与 pi 内置实现使用同一公开 client ID：`b1a00492-073a-47ea-816f-4c329264a828`。本项目选定它作为默认产品 client，并保留显式 override；已有账户不自动换绑。它不是 client secret；源码公开或第三方使用不证明本项目已获准复用此 registration、所有 redirect URI 或全部订阅用途。xAI 第三方 client registration / 接入资格仍须确认。

pi 内置请求的 scope 为 `openid profile email offline_access grok-cli:access api:access`，`referrer` 为 `pi`。官方 Grok Build 个人默认 scope 在此基础上还包含 conversations/workspaces 的 read/write；team 分支的 scope 与 principal 另有合同。第三方插件也有自己的选定 scope。以具体用途选择最小权限，不机械复制官方 CLI 的完整工作区权限；`grok-cli:access` 与 `api:access` 不互为别名。

官方 [HTTP owner][grok-http] 由 origin product/version、`grok-shell` agent product/version 和 OS/architecture 构造 UA；generic origin 与 agent 相同且版本一致时合并为一个 product token。官方 [VERSION][grok-version] 由发布构建的 `GROK_VERSION` 注入，开发 crate 的 package version 不能冒充发布版本。本项目选定固定 generic UA 与 `referrer=grok-build`，不追加项目名或读取 origin/version 环境 override；采用边界与精确值归 [metadata owner](../../src/credential/grok_metadata.rs)。浏览器 code exchange 发送版本 header，但 refresh/UserInfo/JWKS/revoke 不泛化设备 surface 或 exchange 版本 header。产品 metadata 是兼容参考，不证明上游准入。

## 浏览器 authorization-code + PKCE

固定官方流程由 [login][grok-browser] 和 [protocol][grok-oidc] 分工：

1. 从受信 issuer 取得 discovery，生成随机 PKCE verifier/challenge、state 和 nonce。
2. 在 literal loopback `127.0.0.1` 上启动 callback listener。生产默认由 OS 分配端口，callback path 为 `/callback`；固定开发端口不是生产要求。
3. 打开 authorization URL，发送 `response_type=code`、client ID、scope、准确 redirect URI、`code_challenge_method=S256`、challenge、state、nonce；受信配置可预选 principal，预选不替代结果检查。
4. 接收 browser callback；以 form-encoded `grant_type=authorization_code`、code、client ID、相同 redirect URI 和 verifier 交换 token，没有内置 client secret。
5. 个人 OIDC 分支校验 ID-token 签名、issuer、client audience、expiry 和 nonce，然后保存 credential；team 分支可能没有个人 ID token，按具体 principal 合同处理，不能强造个人身份。
6. 结束 listener、输入等待和临时授权状态。浏览器页面收到 code 不等于 exchange、身份校验和持久化已经成功。

官方快照提供 loopback 与手动粘贴两条路径，并允许裸 authorization code 在没有 state 时跳过 state 检查。补充插件的浏览器路径只接受带匹配 state 的完整 redirect URL，并固定第一方 discovery/JWKS 与 ES256/S256 策略。MorphieCore 的 [callback owner](../../src/credential/callback.rs)不接受裸 code 或手动 URL 绕过；错误 callback 也先验证其事务绑定，不覆盖另一登录尝试。PKCE、state、nonce 分别约束不同边界，不能相互替代。

## 标准设备授权

这是 **RFC 8628 device grant**，与 [Codex 私有设备交互](chatgpt-login.md#codex-设备交互不是标准-device-grant)不同。

```text
受信 CLI → device endpoint：申请 device_code / user_code
人 → verification URI：认证、选择 principal、批准
受信 CLI → token endpoint：按 interval 轮询 device grant
受信 CLI → 自己的 credential store：保存 token 与生命周期
```

- 申请使用 form-encoded POST，包含 client ID、scope 和 referrer；固定官方 CLI 在申请和轮询均发送 `x-grok-client-version`、`x-grok-client-surface`。非 TUI CLI 的 surface 按 stderr 是否为 TTY 选择 `cli`/`headless`，不是根据登录授权结果或网关业务 JSON 选择。
- `device_code` 是只供 token polling 使用的秘密，`user_code` 是人需要确认的短码。只展示可信验证网址及 user code，不将 device code 放入浏览器 URL、日志或诊断。
- polling 使用 `grant_type=urn:ietf:params:oauth:grant-type:device_code`、client ID 和 device code，成功直接返回 token，不再交换 authorization code。
- `authorization_pending` 继续等待；`slow_down` 调整后续 polling interval；拒绝、过期、取消和不可恢复错误结束当前尝试。第一轮等待、整体 deadline、单次 HTTP timeout 和响应预算分别控制，不能靠无界轮询等待人完成授权。
- 404 表示所选部署的设备入口可能未启用，不意味着自动获准另一 authority 或登录方法。

pi 的 [xai OAuth][pi-xai-oauth]采用此 grant，支持 `verification_uri_complete`、取消和 pending/slow-down。其验证网址检查主要限制 HTTPS；MorphieCore 还需要第一方 origin 白名单、禁止重定向泄露、长度/JSON/body 预算和每次网络操作的 deadline，不能直接将最小客户端实现当作网关安全边界。

官方 device 分支仅解码 ID token 用于显示，未执行浏览器分支的签名校验，源代码明确区分这两个场景。不得把这种 decoded claim 当作网关本地身份认证或客户端自证权限；需要本地可信身份时，采用相应验证或经过认证的 identity API，推理权限仍由 issuer/backend 判断。

## Refresh、持久化与退出

刷新向受信 token endpoint POST `grant_type=refresh_token`、对应 client ID 和 refresh token。返回的新 access token、expiry 与 rotating refresh token 必须作为同一 credential generation 更新；xAI/pi 分支允许 refresh 响应省略 refresh token，此时保持旧 token，不套用其他 authority 的规则。

- 官方存储记录 issuer/client、实际 principal 与 token 生命周期；保护本地文件，Unix owner-only 权限为 `0600`。多进程刷新需协调 rotating token，不能同时消费旧 refresh token。
- 官方 `expires_at` 优先于其缺省 TTL；源码的 30 天 fallback 不是所有 xAI access token 的服务端期限。pi xAI 模块缺少 `expires_in` 时采用一小时 fallback 并调整刷新余量；两种本地政策不能合并成 Provider 合同。
- 固定 pi [resolver][pi-resolve] 通过 credential-store `modify` 在锁内重读、判断有效期、限定 refresh timeout 并持久化；已存 OAuth credential 的刷新失败不会静默回退环境 API key。xAI 的 adjusted expiry 与通用 minimum-validity 窗口分别理解，不机械叠加余量。一次 `modify` 调用不证明多主机协调或事务 durability。
- 官方刷新源码含 reload/rotation/recovery 策略；只参考职责和不确定结果处理，不照搬其自动 retry、诊断 token 片段或恢复整个 CLI 运行时。
- xAI 本地 sign-out、远端 token revocation 与撤销账户/应用授权分别确定。固定 pi `/logout` 只清理自身 [stored credential][pi-store]，不撤销 Provider credential，也不移除环境变量或其他配置认证；不证明 issuer session 已撤销。

`~/.grok/auth.json` 是官方客户端 store 的源码定位，不是 MorphieCore 的调查、测试或默认导入路径。独立存储与迁移授权归[凭据指南](../credentials.md#授权与安全边界)。

## pi 内置与补充插件

固定 pi 内置 `/login xai` 提供订阅 device flow，认证数据由 pi 自身持有；`xai` Provider 使用公共 `https://api.x.ai/v1` Responses，access token 在内部 `apiKey` 字段中传递不使它变成静态 API key。

补充 `pi-xai-oauth` 注册 `xai-auth`，提供浏览器/设备方法及 session-token CLI proxy 路由。其参考价值在 account-bound catalog、受控 headers、truthful client attribution 及严格浏览器校验，不是建立第二套 Provider IR。插件另提供读取官方 CLI credential 的便利路径，不能作为 MorphieCore 默认依赖。

插件 `1.6.0` 的 peer range 不包含 pi `0.99.2`；它是固定源码参考，不是本项目安装建议或当前兼容声明。以后使用其他版本需重新核对，而不是从包目录或 README 推定能运行。

## 登录与推理的隔离

[Grok Build 官方文档][grok-enterprise]把 `cli-chat-proxy.grok.com` 列为 session inference/settings 入口，公共 API-key 路径另有 `api.x.ai`。固定客户端里还存在 client version/mode、request/conversation/session/model 等 metadata 投影，属于受信执行/adapter 边界。

- 相同 authority、client ID 或 token 字符串不证明两条推理路径、全部模型、额度或计费等价。
- 账户模型发现是独立的 credential-bearing 请求，不属于登录完成的隐式许可；结果只描述所选账户的上游准入，不直接成为 MorphieCore 注册或激活。
- 固定产品 client/metadata 只作为显式兼容参考，不授予订阅资格，也不允许伪造 UI surface 绕过资格/版本检查。必要 header 与 origin 由受信 profile 选择，业务 JSON 不能覆盖；Gateway 接线与部署用途仍须独立确认。
- CLI 请求中的 conversation/session metadata、Provider prompt cache、credential session 和服务器签发的 continuation 不互为别名；不能从相同 header 名复制 Codex 生命周期或生成上游身份。共同分类归 [扩展与上下文](extensions-and-context.md#缓存存储与连接状态)，xAI 的具体 carrier 仍需自己的固定 profile。

## 采用前需要定稿的事项

1. xAI 对 MorphieCore 的合法 client/redirect/scope、第三方客户端及网关用途的接入合同。
2. MorphieCore 选定个人账户、公共 Responses 方向；设备与显式浏览器 wire、nonce/ES256/UserInfo 一致性与总期限由 [Grok authority](../../src/credential/grok.rs)拥有，callback 注册资格仍需操作者确认；自有 store 与生命周期由 [共用池 ADR 0012](../architecture/decisions/0012-grok-personal-credential-pool.md)及 [credential owner](../../src/credential/mod.rs)拥有，按认证类型与 Codex 隔离，不实现 team 或数据面账户调度。UserInfo identity 不证明订阅 inference contract，不能只用登录 UI 文案判断权限。
3. Gateway 的固定 access 借用由 [ADR 0012](../architecture/decisions/0012-grok-personal-credential-pool.md)拥有；进一步的订阅准入与 inference/cache carrier 扩展仍需自己的执行切片；共用存储、验证和操作授权归[凭据指南](../credentials.md#授权与安全边界)，不重复建立通用运行时。

外部来源不能替代 MorphieCore 的采用决策，也不补齐当前 [Generation 缺口](../implementation-status/generation.md)中的 Gateway 接线与执行合同。

[grok-auth-doc]: https://github.com/xai-org/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-pager/docs/user-guide/02-authentication.md
[grok-enterprise]: https://docs.x.ai/build/enterprise
[grok-cli-doc]: https://docs.x.ai/build/cli/reference
[grok-license]: https://github.com/xai-org/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/LICENSE
[grok-config]: https://github.com/xai-org/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-login/src/config.rs
[grok-browser]: https://github.com/xai-org/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-login/src/oidc/login.rs
[grok-oidc]: https://github.com/xai-org/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-login/src/oidc/protocol.rs
[grok-device]: https://github.com/xai-org/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-login/src/device_code.rs
[grok-storage]: https://github.com/xai-org/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-login/src/storage.rs
[grok-refresh]: https://github.com/xai-org/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-login/src/refresh/oidc_refresher.rs
[grok-http]: https://github.com/xai-org/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-http/src/lib.rs
[grok-version]: https://github.com/xai-org/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-version/src/lib.rs
[grok-release-metadata]: https://registry.npmjs.org/@xai-official/grok/1.0.46
[grok-stable-pointer]: https://x.ai/cli/stable
[pi-license]: https://github.com/earendil-works/pi/blob/005af57d88ee23b33778f343a9595b32e67ff788/LICENSE
[pi-xai-oauth]: https://github.com/earendil-works/pi/blob/005af57d88ee23b33778f343a9595b32e67ff788/packages/ai/src/auth/oauth/xai.ts
[pi-device]: https://github.com/earendil-works/pi/blob/005af57d88ee23b33778f343a9595b32e67ff788/packages/ai/src/auth/oauth/device-code.ts
[pi-resolve]: https://github.com/earendil-works/pi/blob/005af57d88ee23b33778f343a9595b32e67ff788/packages/ai/src/auth/resolve.ts
[pi-store]: https://github.com/earendil-works/pi/blob/005af57d88ee23b33778f343a9595b32e67ff788/packages/coding-agent/src/core/auth-storage.ts
[pi-xai-provider]: https://github.com/earendil-works/pi/blob/005af57d88ee23b33778f343a9595b32e67ff788/packages/ai/src/providers/xai.ts
[plugin-license]: https://github.com/BlockedPath/pi-xai-oauth/blob/f2408b0dc108103e0568e6ab02ee0b30644254bb/LICENSE
[plugin-oauth]: https://github.com/BlockedPath/pi-xai-oauth/blob/f2408b0dc108103e0568e6ab02ee0b30644254bb/extensions/xai/oauth.ts
[plugin-oidc]: https://github.com/BlockedPath/pi-xai-oauth/blob/f2408b0dc108103e0568e6ab02ee0b30644254bb/extensions/xai/oidc.ts
[plugin-device]: https://github.com/BlockedPath/pi-xai-oauth/blob/f2408b0dc108103e0568e6ab02ee0b30644254bb/extensions/xai/device-auth.ts
[plugin-wire]: https://github.com/BlockedPath/pi-xai-oauth/blob/f2408b0dc108103e0568e6ab02ee0b30644254bb/extensions/xai/wire.ts
[xai-discovery]: https://auth.x.ai/.well-known/openid-configuration
[xai-jwks]: https://auth.x.ai/.well-known/jwks.json
