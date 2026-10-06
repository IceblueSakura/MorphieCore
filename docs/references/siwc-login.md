# SIWC 登录与 ChatGPT plan usage 参考

本页集中维护公开 Sign in with ChatGPT（SIWC）的官方来源、授权与推理边界，以及 MorphieCore 的采用依据。它不是完整上游 schema 的副本，不维护模型库存、账号资格、实例启用或测试结果，也不声明项目已获 OpenAI 用途批准。实现事实归 [credential driver](../../src/credential/siwc.rs)、[目标准入](../../src/adapter/siwc.rs)与 [凭据指南](../credentials.md)。

共用 OAuth/OIDC 标准归[来源索引](README.md#oauth-standards)，当前 credential 实现、授权与操作边界归[凭据指南](../credentials.md)，共享语义和标准下游 API 归 [Semantic Model](../architecture/semantic-ir.md)。本页独立描述公开 SIWC；SIWC 与 Codex 产品的 token、client 和 backend 不能互换。

## 官方来源与许可

| 来源 | 负责的事实 |
|---|---|
| [Quickstart][quickstart]、[OSS Overview][overview] | Identity 与 plan usage 分离、集成类型、client/host 生命周期 |
| [Registration and sign-in][sign-in] | 动态注册、callback、code exchange、ID-token 验证、持久化 |
| [Accounts and sessions][sessions]、[Token reference][tokens] | 账户选择、granted scopes、expiry、refresh rotation、退出 |
| [Models and inference][inference]、[Preview limitations][limitations] | 账户模型目录、公共 Responses、字段/工具/输入/transport 限制 |
| [Errors and recovery][errors]、[UI/UX guidelines][ux] | 未授权、额度/资格/路由错误、用户控制与品牌呈现 |
| [Self-hosted VMs][vms]、[Codex app-server][app-server] | 远程 host 的 credential 转移、应用内 app-server 集成 |
| [Website OIDC][website]、[Request a client ID][request-client] | 单独的商业 identity 集成与 ID-token 验证示例；不是 OSS 注册的替代流程 |
| [OIDC discovery][discovery]、[JWKS][jwks] | 生产 issuer、端点、签名算法和签名 key 来源 |
| [SIWC Terms][terms]、[Service Terms][service-terms]、[Terms of Use][terms-of-use]、[Usage Policies][usage-policies] | 应用、用户、用途、存储、隐私、安全与禁止行为 |
| [官方集成 Cookbook][cookbook] | 本地应用通过受保护主进程连接身份、registration、模型与实际任务 |

官方网页没有永久版本；实施时核对所选 flow 和适用条款，不用本地文档编辑刷新外部验证日期。SIWC Terms 页面标示的发布日期为 2026-09-29；该日期不是账号或部署核验日期。

官方 DevKit 的参考固定为 `openai/sign-in-with-chatgpt-devkit@f723814abdccec135b519c451fb6e1992ee5e933`，定位 [README][devkit-readme]、[credential security][devkit-security] 与 [LICENSE][devkit-license]。OpenAI-authored code/documentation 采用 **Sign-in with ChatGPT DevKit Noncommercial License v1.0**，不是 MIT/Apache；为雇主、客户或商业优势开展开发/测试，即使没有收费，也不自动满足其 Noncommercial Purpose。独立编写的软件不会仅因通过接口通信变成 Modified Work，但该例外不授予额外服务访问权。采用/分发 DevKit 代码、修改、品牌资产分别核对许可，不将本项目 MIT 扩张为第三方材料的许可。

MorphieCore 独立使用公开 OAuth/OIDC 与 Responses 合同，不直接引入 DevKit 或 Node sidecar。其受保护 pending-rotation、账户与主进程隔离设计可作为来源，不等于可以复制源码或已完成原生安全验收。

## 应用用途与资格

SIWC 包含两项不同权限：

- **Identity**：以已验证的 OpenAI 身份登录；identity scopes 不授予模型推理、API key、ChatGPT 对话、memory 或账户资源访问。
- **ChatGPT plan usage**：用户另行授权其 registration 使用符合资格的套餐/credits 完成获准请求；有效 ID token、模型目录或同名模型均不证明这项权限。

OSS/local plan-usage flow 不需要 client secret 或 partner API key。商业网站 identity、收费或远程托管应用分别查 [资格入口][request-client] 与 [interest form][interest]，不按“项目开源”推定所有用途获准。

[SIWC Terms][terms] 要求如实使用自己的应用名称、只通过支持的登录流程取得 token，不索取密码/session cookie；合理保护数据，处理前提供适用隐私告知与必要控制，不暗示 OpenAI endorsement。其 plan-usage 约束包括：

1. 请求来自用户本地 runtime，或只由该用户控制的远程 runtime。
2. 请求为已认证用户服务，来自本人活动或明确授权的自动化；后台使用须明确同意，其他用户的活动不得触发该账户的请求。
3. 只服务用户连接的应用；不得为其他工具或无关请求提供 general-purpose API access。
4. 用户无需付费给应用或升级付费版本才能通过 SIWC 使用套餐。
5. 不汇集、转售、赠送或共享套餐/token，不通过多账户、拆分或轮换绕过额度；不为另一用户消费同一人的订阅。
6. 不以接口接入规避条款中的安全、欺诈、滥用、SIWC 软件修改/逆向或转许可限制；DevKit 许可与服务条款的适用范围分别确认。

MorphieCore 的用途限定为本人单用户、同一自研分布式 Agent 应用的内部测试与模型执行组件，worker 只完成本人授权的集群运维/部署任务，项目保持开源。标准 OpenAI HTTP API 的形式不改变这一用途边界，也不能单凭内部网络、入口 key 或客户端自报名称证明请求属于该应用。复用 Pi 类 Runtime 不自动允许向任意独立应用提供套餐出口；身份与任务归属由可信应用边界约束，不把这一设计描述当作 OpenAI 批准。

## MorphieCore 非秘密参数与依据

名称、协议常量、callback 与验证归 [driver](../../src/credential/siwc.rs)与 [browser](../../src/credential/browser.rs)；provider/binding 的精确 ID 归 [provider catalog](../../src/provider/catalog.rs)与 [subscription bindings](../../src/topology/catalog/subscriptions.rs)。这些声明不是已注册 client、已启用实例或实际账户准入证明。

### 项目固定名称

应用展示名称与首次 `agent_name_hint` 如实采用同一自有名称，不冒用 Pi、Codex CLI 或 OpenAI；返回登录省略 hint。自有 UA 使用实际 package version，名称不是 OAuth client ID 或身份认证材料。description 取 [package metadata](../../Cargo.toml)，官方公开授权参数没有 description 字段，不凭推测发送额外参数。Sign-in 标签按 [UI/UX][ux] 使用 `Continue with ChatGPT`，不复制未经许可资产。

公共 model label 与 canonical model、upstream slug 分别绑定，不把 Provider/profile 塞进语义 identity。多个 binding 的后缀语法、私有文件布局和未来 `/models` 扩展 schema 在相应切片定稿；本页不提前声明可用模型或 legacy alias。

### 固定协议值与 callback 策略

| 用途 | 协议值 / 采用 |
|---|---|
| Trusted issuer / authority | `https://auth.openai.com` |
| Discovery | `https://auth.openai.com/.well-known/openid-configuration` |
| Authorization | `https://auth.openai.com/api/accounts/authorize` |
| Token | `https://auth.openai.com/api/accounts/oauth/token` |
| Revocation | `https://auth.openai.com/api/accounts/oauth/revoke`；从受信 discovery 核对 |
| JWKS | `https://auth.openai.com/.well-known/jwks.json` |
| Identity 签名策略 | 生产 discovery 的 `RS256`；固定 allowlist，不从 JWT `alg`/key URL 扩张信任 |
| First registration client | `dynamic_agent_client`，仅首次注册入口 |
| Public-client token auth | `none`，不发送 client secret |
| Response type / PKCE | `code` / `S256` |
| Resource | `https://api.openai.com/v1`，authorize、exchange、refresh 一致 |
| 请求 scopes | `openid profile email offline_access resource.invoke chatgpt.tokens.use.direct` |
| Callback host / path | literal `127.0.0.1` / `/auth/callback`，初次注册起固定 |
| Callback port | 首片默认 bind port `0`，由 OS 选择可用端口；实际 redirect URI 使用分配后的端口 |
| 推理与发现 | `POST https://api.openai.com/v1/responses` / `GET https://api.openai.com/v1/models` |
| HTTP 推理 transport | JSON request / SSE response，`store:false`、`stream:true` |
| Usage 管理 | `https://chatgpt.com/settings/usage` |

选择 OS 分配端口避免与 Pi/Codex 的 1455 listener 竞争；这是 MorphieCore 策略，不是 authority 固定端口。后续授权只能改变 port，不能改变 callback scheme/host/path；同一 attempt 的 authorize 与 exchange 必须使用准确相同 URI。显式端口占用即失败，不取消其他进程或偷偷换地址。首片只提供 browser flow，不移植 Codex 私有设备轮询。

### 不能预先写死的值

- **Issued `client_id`**：OpenAI 在首次 callback 返回，通常为 `oaiapp_...`；每个 registration 保存它，重登/refresh/revoke 复用。不能生成假值、使用 `dynamic_agent_client` 交换 code，或复制产品 client。
- **`ext_agent_host_id`**：首片每个明确 host 生命周期生成一次 UUIDv4，保存 `urn:uuid:<UUID>`；重启和切换账户复用，不按请求或每个临时 worker 随机生成。其他 host 有独立 ID；不写入用户、邮箱、IP 或账户信息。
- 官方还接受 `urn:ietf:params:oauth:jwk-thumbprint:…`（推荐的公钥派生格式，见 [RFC 9278](https://www.rfc-editor.org/rfc/rfc9278.html)）和 `did:key:`。当前它们仅作标识，不构成私钥持有证明；首片 UUID 选择不预建密钥管理系统。
- **`state`、nonce、PKCE verifier/challenge**：每次 attempt 分别生成 32 bytes 安全随机材料；state/nonce 使用无 padding base64url，verifier 符合 RFC 7636，challenge 为 `base64url(SHA256(verifier))` 且无 padding。绑定原始 callback 与单次事务，不能跨登录复用。
- **Code、tokens、granted scopes、expiry、verified subject**：来自本次被验证的 callback/authority 响应，不从本页示例、邮箱、alias 或 access-token payload 补造。
- **Task/thread/turn/request/cache identity**：由各自真实 owner 管理，不能由 OAuth client、host ID、token 或账户标签派生。

## 注册、返回登录与 consent

依据 [sign-in][sign-in]：

1. 先选择账户/待注册连接与 host，保留当前 active grant；新 attempt 不先覆盖旧账户。启动 loopback listener，保存 state、nonce、verifier 和准确 redirect URI。
2. 新注册使用入口 client、应用名称与 host ID。返回登录使用已保存 issued client、同一 host ID，省略 `agent_name_hint`。返回登录可以使用同账户保留的 `id_token_hint` 与 optional email `login_hint`；hint 可已过期，但不是本次身份验证、有效 session 或 workspace 改选权限。普通返回登录没有 hint 时显示账户选择，有 hint 时按官方流程跳过选择；都须验证新身份。
3. 在接受 code 或 OAuth error 前验证 state 和单次事务。Callback 必须有界校验 method/path/Host、唯一参数和准确 URI，不接受裸 code 作为绕过 state 的手动恢复。
4. 首次 callback 必须含 issued client；返回登录可省略，此时使用 pending attempt 的已有 client。如果返回另一 client，拒绝而不是替换选中 registration。Callback `scope` 不是最终授权证据。
5. 先以待完成 registration 保留 issued client，再用原 PKCE verifier、同 URI/resource 交换 code；未验证的 pending registration 不可借用来推理。Exchange `invalid_grant` 丢弃 code，用保留的 issued client 重新授权，不盲目重发 code 或再次 dynamic registration。
6. 验证 ID token 与实际 granted scopes 后原子发布连接。取消、失败、late callback 或身份不匹配不覆盖原 active grant；callback 页面只说明已收到回调，不提前声称登录/持久化成功。

没有 `chatgpt.tokens.use.direct` 时，合法 identity 登录可以保留，但 plan usage disabled，不发套餐推理。仅在用户明确请求启用时，用保存 client 和完整 scope 集重授权。`prompt=consent` 是官方现有 consent 机制；`force_reconsent=true` 只有 OpenAI 确认该 integration 已部署后使用。两者是 OAuth 参数，不是 Responses body 的 `prompt`。普通重登不强制反复 consent。

含 `id_token_hint` 的授权 URL 含敏感 token，必须从日志、diagnostics 和普通 CLI 输出脱敏；access/refresh token 不进入 URL。首片可省略 hints，让用户在浏览器选择，再验证返回身份，不能为了方便保留完整 URL 日志。

## Identity、账户与权限绑定

依据 [sign-in][sign-in] 与 [Website OIDC][website] 验证签名、受信 issuer、issued-client audience、expiration 和本次 nonce；使用成熟 JWT/JWK 库，限制算法、时钟容差与 key 集合。未知 `kid` 可触发有界受信 JWKS 更新，不能从 JWT header 的 `jku`/`x5u`/inline key 选择 authority。

账户 owner 至少区分 issuer、issued client 与 verified `sub`。Email/alias 只帮助显示，不能凭相同 email 合并账户或推断 workspace；registration 已绑定所选 workspace，不从 opaque auth metadata 反解或编造 workspace 标识。重登必须匹配原 registration identity，scope 变化须重新准入。ID token 用于身份，access token 用于推理，refresh token 用于续期，三者不能互代。

保留必要账户显示信息、验证身份、registration、tokens、实际 scopes 和 expiry，遵守最小化与隐私告知。Refresh 未返回新 ID token 时，仅按该已验证 registration 的 renewal 合同继承原身份，不补造新身份；若返回新 ID token，验证其身份一致后才发布为可用。

Access-token claims 的完整字段参考归 [Token reference][tokens]。其 `https://api.openai.com/auth` 内的 salt/encrypted metadata 视为 opaque，不能解释成 Codex account header、可移植 replay scope 或权限证明。

## Token、持久化与 refresh

[Token reference][tokens] 给出 access token 一小时、refresh token 三十天；成功 rotation 返回 replacement refresh token，替代值取得新的三十天 lifetime。它们是所选 flow 的合同，不是跨 authority 默认值；实现按实际 `expires_in`/expiry 调度，不硬编码 lifetime 代替响应或机械复制 Pi 的提前余量。

Token 响应包含 access、refresh、ID token、token type、expiry、scope，以及 `earliest_refresh_at` 的来源入口。该 reference 未充分解释后者的编码单位与调度语义；采用前核对，不能自行当作某种 Unix timestamp 或用其缺失补造时间。

Refresh 用 form encoding，发送 `grant_type=refresh_token`、issued client、当前 refresh token 和相同 resource，省略 scope。同 session 的 refresh 串行化，锁内重读 generation，避免多进程消费旧 token；文件事务不跨网络。Replacement tokens、expiry 和 scopes 同 generation 原子发布，不向数据面暴露 refresh/ID token。

MorphieCore 采用以下安全与恢复边界，不照搬“任何失败都重试”：

- 发出前的本地错误与已可能消费 token 的错误分别处理。
- 已收到 replacement、但新 ID-token 验证暂时受阻时，将新材料保存为受保护的 pending-verification 状态；禁止借用，允许后续恢复验证，不再次消费旧 refresh token。
- Authority/网络/发布结果不确定时隔离自动复用，不从旧备份或 recovery marker 复活可消费 token；保留 registration 和可恢复的受保护状态，不因暂时故障直接删除账户。
- Terminal refresh 错误需要重授权；确定未消费的错误是否允许重试必须有明确合同，不能把任意 timeout/5xx 当作证明。
- 数据面只借用 principal 固定的短期 access snapshot，普通请求不登录/refresh；后续续期调度由显式应用 auth owner 选片，不预建 daemon。

路径/权限、single writer、原子发布与不确定结果隔离归 [credential store 合同](../architecture/decisions/0012-grok-personal-credential-pool.md)；动态 registration、identity-only 状态和 pending renewal 的实际状态机归 [manager](../../src/credential/manager.rs)，操作与恢复归 [凭据指南](../credentials.md)。

Unix 文件/目录 owner-only 权限与原子写入是官方要求的一部分；`0600` 不等于加密。DevKit 的 OS-backed encryption、无 plaintext fallback 是它的 [SDK 存储合同][devkit-security]，独立 Rust store 需按自身威胁模型单独选定，不能声称已满足原生 keyring/ACL/断电验收。

## 退出、撤销与切换

[Sessions][sessions] 指定通过受信 discovery 的 revocation endpoint 发送 form `token=<refresh token>`、`token_type_hint=refresh_token`、issued client。空 HTTP 200 是协议层成功，包括 token 已无效；不等于删除 registration、终止已经发出的推理或确认停止计费。

先停止该 session 的新请求/借用，隔离仅用于 revoke 的材料，按显式操作预算尝试撤销，再清除本地 access/refresh/ID token。Network/5xx 的 bounded backoff 只有所选退出操作策略允许时执行；取消或远端失败也不恢复 active grant。不能确认撤销时明确区分 local sign-out 与 remote unconfirmed，提示用户可在 ChatGPT Settings disconnect。保留账户/registration 映射及 host ID，重登复用，不反复注册。

切换账户由本人明确选择，完成 identity/scopes 验证后才成为 active；模型选择随 registration 重新核对。不同 registration 的 client、tokens、identity、replay scope 不拼接。多个保存账户不形成轮换额度池。

## 模型发现与 HTTP Responses

[Models and inference][inference] 指定用所选 access token 查询公共 `/v1/models`。该 flow 的文档响应是 `models` 数组，UI 显示 `display_name`、推理使用 `slug`，可筛选 `visibility:"list"` 并保留顺序；不能直接套普通 Platform Models 的 `data` 形状或把目录当 entitlement proof。Credential-bearing 发现需要独立授权，不在默认离线验证、codec 或列表刷新中隐式执行。

推理使用公共 `/v1/responses` 和 Bearer access token，不能改发 `chatgpt.com/backend-api`。以下为不含真实 credential 的表示示例，不是可执行 probe 或本地模型注册声明：

```json
{
  "model": "<selected-account-model-slug>",
  "instructions": "Reply briefly.",
  "input": [{"role": "user", "content": "Reply with exactly pong."}],
  "store": false,
  "stream": true
}
```

必须完整消费 SSE，收到真实 `response.completed` 才是完成；`response.incomplete`、failed、断流、cancel 与完成分开。HTTP 200、收到 delta 或非空文本不证明完成。额度/使用检查错误可在开始 streaming 后到达，不能补造 completed、换账户拼接或 post-commit replay。

上游强制 SSE 与下游交付不同：标准非流式下游可由 [有界交付 owner](../../src/execution/delivery.rs)聚合并验证终态，不能把整个无界 stream 缓存在 codec，不能因为上游 streaming 而改成私有下游协议。

## 所选 preview 的准入边界

精确字段/工具来源归 [Preview limitations][limitations]，不是完整 Platform API 的能力声明：

- HTTP 使用 `store:false`、`stream:true` 和 array `input`，每次带所需历史；使用 `instructions` 或 developer messages，显式 `{type:"message",role:"system"}` item 被拒绝。等价投影须保护 instruction authority，不能只改 role 让请求通过。
- 不发送 `background`、`conversation`、`max_output_tokens`、`max_tool_calls`、`metadata`、`moderation`、`multi_agent`、`prompt`、`prompt_cache_retention`、`safety_identifier`、`temperature`、`top_logprobs`、`top_p`、`truncation`、`user`。
- HTTP 不发送 `previous_response_id`；WS continuation 只引用同 authenticated connection 的 response，不提供持久对话存储。WS 不进入首片，不能把它解释为 Realtime。
- Function/custom tools 使用 namespace 分组，或通过 `additional_tools` input items 提供。Web search 仍受模型/用户/workspace policy 限制。
- 不支持 image generation、file search、Code Interpreter、native computer use、hosted MCP/connectors 或 Responses `tool_search`；客户端执行工具不使这些 hosted carrier 获支持。顶层 tools 不接受 `programmatic_tool_calling`。
- Text/image/file 输入仍须所选模型与本地来源准入；audio/video 输入、Files upload API、transcription API 不属于此 flow。不隐式下载/上传或文本化媒体。

上游不支持的**显式行为控制**在最终 typed 值上拒绝，不先删除再让 sampling/header/body override 加回，也不缩减共享 IR。Operator 默认输出控制、真实上游 max-token 限制与本地 bytes/events/deadline 预算分开；本地取消不证明 Provider 停算或停费。

工具 namespace 的定义、qualified reference、call/result 与 history 共享 [typed owner](../../src/semantic/task/generation/tool.rs)；固定标准具名选择的载体缺口按 [Responses profile](../architecture/responses-text-profile.md#tool-namespaces)拒绝，不在 encoder 外硬包 namespace、合并同名工具或新增私有 attachment 来掩盖。

## Headers、cache 与应用内 app-server

SIWC 不继承 Codex 产品 account locator、CLI 身份或 private turn-state：不自动发送 `chatgpt-account-id`、`codex_cli_rs` originator/UA、产品实验 beta 或 `x-codex-turn-state`/turn metadata。不按相同 header 名证明两个 backend 具有相同效果；业务请求不能覆盖 auth、trusted origin、proxy 或账户选择。

公共 [API request-ID 合同][api-overview] 将 `X-Client-Request-Id` 定位为每请求唯一的追踪 ID（ASCII、至多 512 字符）；它不是通用 thread identity。`session-id` 与 `session_id` 是不同名字，不能因某客户端重复使用 session 值就推定 SIWC cache/turn sticky 语义。Host、OAuth session、logical session、thread、turn、request、prompt cache key 分别有 owner，具体 cache/连接状态归 [扩展与上下文](extensions-and-context.md)。`store:false` 不等于关闭 prompt cache。

MorphieCore 直接 HTTP 首片不自动发送自定义 session/turn/originator header；未定稿的 carrier 不透传。自有 UA 归 driver，但 UA 本身不验证应用归属。Pi `1.0.2`（固定 `earendil-works/pi@cd32f7725fdbddbaecdff5b1e68491563394e0ca`，[MIT][pi-license]）的 [SIWC module][pi-siwc]、[Responses adapter][pi-responses] 和 [resolver][pi-resolver] 只作实现来源导航，不替代官方验证、registration 和最终准入要求，也不升级其他固定基线。

若以后使用官方 [Codex app-server][app-server] 作为同一应用的子组件：

- 应用自己取得 SIWC access token，显式提供给子进程；不另外执行产品 Codex login，不把 refresh/ID token 交给工具或 renderer。
- 配置公共 Responses base URL、`requires_openai_auth=false`、`supports_websockets=false`；具体配置和 RPC 命令仍由官方页面拥有，MorphieCore 不因此依赖 app-server。
- `initialize.clientInfo` 如实采用 `name:"MorphieCore"`、`title:"MorphieCore"`、实际 version，与首次 `agent_name_hint` 保持应用名称一致；这是应用 attribution，不冒用 Codex CLI。SIWC 因而不是一概禁止 originator，只是不自动继承另一个产品身份。
- 子进程通过 stdio/RPC 使用自己的 thread/turn；只有 `turn/completed` 且 status completed 才算成功。模型 RPC 目录可能是 bundled catalog，不能证明资格。
- 应用负责续期；env-key 模式要用新 access token 重启子进程并 resume 其本地 thread。自有子进程的显式 token 注入不改变 MorphieCore 默认从显式文件读取 credential 的边界。

## 用户体验与隐私控制

依据 [UI/UX][ux] 与 [Terms][terms]：提供可识别的 `Continue with ChatGPT` 入口，处理数据前呈现适用隐私告知，不索取 API key 来完成 SIWC 登录。第一次成功启用 plan usage 后确认“正在使用 ChatGPT plan”，普通重登不反复弹首次确认。

账户选择使用各 registration 独立稳定标签，不按 email 合并；应用内显示选中的连接和 plan-use enabled/disabled 状态。任务/模型选择处明确标示 `Using ChatGPT plan`，保留 `Manage usage` 到 ChatGPT Usage 的入口；额度失败以管理使用为主要操作，不声称有新额度或自动切换付费/API-key 路径。CLI 对这些状态提供等价清晰提示，不为文档预建图形界面。

后台活动须独立明确同意其范围、期限、预算和停止方式。退出/取消不声称停止远端计算；应用自己的费用与 OpenAI 套餐/credits 分开。本项目不引入收费解锁、价格表、费用展示或购买 credits UI；将来改变商业模式需重新核对条款。

## 错误与恢复来源

依据 [Errors and recovery][errors] 保留 status、受限机器 code/param 和 request ID，检查响应形状；admission 可能只有 `{"detail":"..."}`，其文字不是稳定 machine code。原始敏感正文不进入日志、CLI 或下游；未知错误不套入已知恢复策略。

| 来源错误 | 官方恢复方向 / MorphieCore 边界 |
|---|---|
| Direct admission 401 / 403 | 检查 identity/direct permission 或 policy/region；不换账户或计费路径 |
| Direct admission 503 | Direct routing 暂不可用或未启用；保留 credential，不把所有此类失败判为可重试 |
| `subscription_sharing_user_not_eligible`（403） | 用户/workspace/policy 不符合；停止，不循环 OAuth |
| `subscription_sharing_usage_limit_exceeded`（429） | 暂停该 plan 的新请求，链接 Usage；不能猜整个套餐耗尽或 reset time |
| `subscription_sharing_usage_unavailable` / `subscription_sharing_user_unavailable`（503） | 临时检查失败；保留 credential，只有显式策略和预算才允许后续 backoff |
| `subscription_sharing_unsupported_capability`（400） | 检查 param；改正不支持的目标能力，不盲目重发或静默删语义 |
| `subscription_sharing_route_not_supported`（403） | 检查准确 method/path，其他 SDK/client 有支持不代表此 route 有权限 |
| `subscription_sharing_invalid_user`（401） | 保留 request ID 诊断；确认 revoke 或 terminal refresh 错误后重登 |
| `chatpass_v2_scope_not_authorized` / `chatpass_v2_invalid_authorization_context`（403） | 修正 registration/grant context，不改计费绕过 |
| Refresh `invalid_grant`、`invalid_refresh_token`、`token_expired`、`refresh_token_expired`、`refresh_token_invalidated`、`refresh_token_reused` | 旧 refresh 不可用，清理其复用权限，以保存 issued client 重授权 |
| `invalid_client` | 修正 client 配置，不解释为用户账户额度故障 |

这是恢复来源，不是完整错误枚举或本地公共错误 schema。官方不静默切换计费；MorphieCore 首片单选 registration、单成员绑定、`fallback=false`、`max_attempts=1`，无同请求重试。多个 worker 共用应用总预算，不形成多个套餐 allowance。连接断开目前没有服务端通知；只有请求/refresh 确认撤销才停止使用该 token set，不因临时服务错误删账户。

## 远程 host 与分布式应用边界

同一用户/workspace 的不同 host 可按 [Overview][overview] 复用 issued registration 和应用使用设置，各 host 保留独立 ID；这不产生独立额度，也不意味着同一个 rotating session 可以多主刷新。

[VM 指南][vms] 描述：先生成 VM 自己的稳定 host ID，在本地浏览器用同一工具/用户/workspace 完成 OAuth，经 SSH 等受保护通道转移所选 registration 的 credential，保留 VM host ID，由 VM 成为后续 refresh owner。该指南同时说明，转移 session 的 host-specific usage attribution 与 plan-access revocation 尚不可用。

但 [SIWC Terms][terms] 对持久化写明 local/user-controlled、not in a remote or managed environment，而 sessions/VM 指南包含 self-hosted 存储与转移。**不能自行采用宽松解释消除这项边界；远程持久化/复制前确认适用部署合同。** 通用托管、多用户服务和 token 共享没有由 VM 指南获得默认许可。

MorphieCore 的采用边界为本地 credential owner + 本地模型执行；未来自研 worker 只通过同一应用内部的受信通道提交本人任务，不取得 refresh/ID token，也不对任意第三方工具开放套餐出口。该结构降低 token 复制和 rotation 风险，不构成用途合规或网络部署批准。后台巡检/部署推理需明确授权范围、期限、预算和停止控制；模型请求权限不授予集群变更权限。不因 worker 扩容注册更多 client 或轮换账户绕过限制。

## 本项目采用与实施边界

- 登录采用独立 SIWC profile，不是转换 Codex token、重命名既有 backend 或搜索第三方 auth cache。新 registration 经过验证后显式切换绑定；旧材料清理、远端 revoke 和私有格式迁移需另外授权，不自动执行。
- 保持标准 OpenAI 下游 API 与共享语义，不把内部用途改成必须专用 RPC；SIWC 限制归独立目标合同。Chat 损失仍归 [投影合同](../architecture/protocol-and-lowering.md#semantic-loss)，新扩展不因迁移自动获准。
- 不保留计价、价格目录、金额估算或成本路由；真实 token usage、resource budgets 与额度错误不是计价。不能因为 native view 名含 billing 就丢 token facts，也不为客户端费用 UI 伪造零价格。
- 后续模型发现先维护标准 `/v1/models` 的 public-label 视图；更丰富 `/models` 的路径/schema 另定，不直接透传 SIWC 上游目录或账户 metadata，不提供价格字段。目录、准入、实例激活和实际完成分别验收；不能假定 Pi 自动发现该端点。
- 已有文件安全、JWT、access snapshot、增量 SSE 和有界 JSON 交付复用现有 owner。Dynamic registration/identity-only/pending renewal、独立请求准入、工具分组与错误分类分别归对应 owner；不为登录预建 Agent/k8s/WS runtime。
- 方向与实施顺序归 [next-goal](../implementation-plans/next-goal.md)，新行为只有获准后才记入 [current-focus](../implementation-plans/current-focus.md)。本页不覆盖当前未完成切片，不修改运行配置，也不授予登录、refresh/revoke、模型发现、推理、部署或提交权限。

实施验收遵循 [development](../development.md)：独立 callback/JWT/registration oracle，编辑与拒绝，rotation/取消/崩溃/pending 恢复，跨 principal/profile 隔离，最终参数不回流，namespace 的定义/调用/结果一致，以及 JSON/SSE 的真实终态、预算和 post-commit 边界。文档检查不证明行为；真实 gate 需明确应用、账户/workspace、模型、矩阵、请求/资源预算与脱敏输出。SIWC 不支持 max-output 参数，不能以发送该参数声称有硬上游 token cap；本地截止与取消也不是实际费用/停算保证。

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
[interest]: https://openai.com/form/sign-in-with-chatgpt-interest/
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
