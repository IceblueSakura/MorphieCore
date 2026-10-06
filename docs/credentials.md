# 统一文件凭据管理器

`CredentialManager` 管理 API key 的本地生命周期、OAuth 授权生命周期，以及供推理使用的有序凭据池。凭据只来自应用自有文件 store；CLI 默认目录与覆盖方式见[自有文件目录](#自有文件目录)。CLI、Gateway 和 probe 不从环境变量读取上游 key 或账户 alias，不搜索第三方 auth cache。API key 不伪装成 OAuth grant，登录成功也不证明模型、订阅或额度资格。

合同归 [ADR 0012](architecture/decisions/0012-grok-personal-credential-pool.md)，执行前移归 [ADR 0010](architecture/decisions/0010-canonical-model-fixed-fallback.md)。OAuth 来源见 [Grok](references/grok-login.md)和 [SIWC](references/siwc-login.md)。ChatGPT plan usage 只提供独立 SIWC browser flow，不提供 Codex 产品登录、client override 或 token 转换；SIWC 也不是 Platform API key。

## 授权与安全边界

[OAuth/OIDC 标准](references/README.md#oauth-standards)与具体产品准入分开；登录、refresh、revocation、credential-bearing 发现与推理须分别取得目标、效果和必要预算授权。公开 metadata、固定源码与 synthetic 验证不能证明真实账户资格或上游接受。

凭据、locator、选定账户、registration 与 refresh/retry state 属于认证/执行 owner，不进入 Task IR；纯 codec/lowering 不访问凭据、registry 或网络。业务 JSON 不选择 authority、账户、认证 headers 或脚本。数据面只借用短生命周期、身份绑定的 access 视图，不获得完整 credential bundle。

不自动发现、读取、导入或修改其他客户端、浏览器或 OS 的真实 auth cache；凭据迁移、私有文件清理与远端撤销分别授权。不从参考客户端引入动态脚本、隐式认证 fallback 或普通请求内自动 refresh/retry；publication/commit 后禁止认证恢复重放。

## Gateway access 绑定

```sh
# 使用默认 store；仍须先准备 gateway.json 和显式 pool。
cargo run --locked --offline --bin morphiecore
# 覆盖 store。
cargo run --locked --offline --bin morphiecore -- --credentials-dir /path/to/private-store
# 入口配置可独立放置；不需要复制上游凭据。
cargo run --locked --offline --bin morphiecore -- \
  --credentials-dir /path/to/private-store --config /path/to/private/gateway.json
```

默认入口配置为目录中的 `gateway.json`，精确字段与资源上限归 [bootstrap](../src/gateway/bootstrap.rs)。该文件包含入口 `client_key`、可选 loopback `bind`、受信 `proxy`、诊断路径 `diagnostics`、显式模型过滤 `models` 和全局尝试上限 `max_attempts`。省略 models 使用已配置 pool 对应的默认对话绑定；[Images](http-gateway.md#独立图片生成)、[Speech](http-gateway.md#独立语音生成) 与 [Transcription](http-gateway.md#独立语音识别) 仍须分别显式选定，不因共享 pool 自动启用。空、重复、未知或未启用模型过滤拒绝。配置路径通过参数提供，不从 env 导入旧格式。

以下仅为 synthetic 结构示例；模型占位符须按 catalog 查询替换，不是实际准入声明：

```json
{
  "client_key": "synthetic-local-ingress-token-00000001",
  "bind": "127.0.0.1:8080",
  "models": ["configured-public-model"],
  "max_attempts": 2
}
```

Provider 文件中的 `pools` 使用已编译的 credential binding ID 为键。pool 明确引用本 Provider 的 API key alias，或指定 OAuth profile/alias；Provider ID 与授权 profile 不互为别名。Gateway 验证池成员与 Endpoint 的认证类型、profile、认证域，不因相同 Bearer 拼写允许混用。

- 每条固定候选是 `(endpoint, credential)`，顺序来自 Route 和池成员数组，不生成伪 Endpoint ID 或随机挑选账户。
- API key 借用固定 record identity / generation / epoch，并在内部绑定实际材料摘要（不输出到状态）；替换、启停、移除使旧绑定失效，重新启用不会复活旧绑定。显式重新构建 Gateway（binary 重启）才会绑定新 epoch。
- OAuth 固定 profile/client/verified principal（包括 workspace），每次借用当前 access snapshot；正常 token refresh 保持该身份。不向推理层提供 refresh/ID token，不在普通请求中登录或刷新。
- 已借用的单次快照可完成，不在推理 I/O 期间持有文件事务；禁用、退出或取消不证明上游停算、停止计费或远端撤销。
- API key 的 `enabled` 只是本地状态；未知 expiry、模型/额度和上游可用性不能补成有效事实。

### 同 Provider fallback

SIWC 是例外：单一显式 registration、单成员、`fallback:false`、`max_attempts:1`，不做账户轮换或同请求重试。下列可选 fallback 机制不能扩大 SIWC 的授权边界。

`fallback` 缺省为 false，必须在池配置显式开启。有效尝试数不超过池上限、入口配置上限和 Route 总上限；本地不可用检查也占候选位置。所有尝试共用总 deadline 和并发 permit，没有同候选重试、并行竞速、自动刷新或跨请求健康调度。

| 失败边界 | 同 Provider 凭据前移 |
|---|---|
| 本地 key 禁用/已移除/绑定失效、已有身份绑定的 OAuth 已退出/过期 | 在显式策略和剩余预算内可前移，不发认证恢复请求 |
| 提交前连接/传输失败、timeout、5xx | 在显式策略和剩余预算内可前移；可能重复计算/计费 |
| 401/403、未知作用范围的 429 | 停止；同 Provider 不证明凭据拥有独立配额，不能用换账户掩盖权限失败 |
| 损坏/不安全文件、身份改变、锁冲突、参数/协议/投影错误 | 停止，不把配置或安全错误变成凭据不可用 |
| 已发布下游内容后的错误 | 中止当前 body，禁止前移或伪造成功终态 |

不同凭据不共享 replay scope。多个候选的入口拒绝尚无亲和 carrier 的 encrypted-output 请求和 opaque history；不能去掉 signature 以继续，也不能凭 Provider/model 相同跨身份回放。池配置变更不热扩张正在运行的候选集合。

<a id="显式自有文件目录"></a>

## 自有文件目录

两个 binary 共用 [CLI 路径解析](../src/credential/directory.rs)：省略 `morphiecore-auth --store` 或 `morphiecore --credentials-dir` 时，使用当前用户 home 下的 `.local/share/morphiecore/credentials`。Unix home 来自 `HOME`，Windows 来自 `USERPROFILE`，仅用于非秘密路径选择；缺失、空或相对 home 拒绝，不搜索当前目录或旧 `config/`，不采用 `XDG_DATA_HOME` 等其他目录 fallback。显式 CLI 路径优先且不依赖 home。库构造仍由调用者传入路径，不自动选择 store。

Unix 首次使用可准备私有父目录；已有不安全权限不会自动修复：

```sh
install -d -m 700 "$HOME/.local/share/morphiecore"
install -d -m 700 "$HOME/.local/share/morphiecore/credentials"
target/debug/morphiecore-auth list
```

这只选择本应用的文件目录，不读取环境 secret 或默认账户。`--account`、pool 成员和 Gateway 激活仍须显式配置。目录迁移需独立授权并停止写入，整体保留 namespace 文档、host ID、锁文件和 recovery markers；程序不自动搬迁、转换或合并旧 store。

每个 Provider namespace 使用一个 JSON 文档，包含 `provider`、文档 `revision`、`api_keys`、`oauth` 和 `pools`。OAuth 文档按授权 profile namespace 管理，可被 Provider pool 显式引用，不复制 grant。每个条目的 secret、身份与生命周期仍有唯一 typed owner；精确结构、预算和拒绝规则归 [store](../src/credential/store.rs)、[API key](../src/credential/api_key.rs) 和 [pool](../src/credential/pool.rs)。保留字及旧布局拒绝，不自动读取、转换或删除旧私有文件。

**这些 JSON 包含 secrets，不能提交、打印、复制到诊断或发给客户端。** store 根目录仅用于 Provider 文档、gateway 配置和内部锁/marker；key/pool 导入文件另放私有输入目录。文件格式可读不等于允许在服务运行时用不遵守锁的编辑器覆盖凭据；在线修改使用 CLI/manager。字段重复、未知字段、类型或绑定不一致失败关闭。

- `fs-mistrust` 验证路径和 Unix 权限，显式禁用其环境绕过开关；Unix 新目录/文件为 `0700/0600`，不安全权限、链接和非普通文件拒绝，不自动修复。
- Windows 使用 `windows-permissions` 补充 ACL 检查，因为 fs-mistrust 本身不验证 Windows ACL。要求能持续继承给子目录/文件的私有 ACL，以及不可被不受信主体替换的父路径；不自动放宽 ACL 或接受 reparse point。管理员与当前进程主体属于受信边界。
- `atomicwrites` 负责完整临时文件、安全替换及平台持久化操作；标准库文件锁负责进程间协调。稳定锁文件不随 JSON 替换移动，短事务重读最新文档，只合并本次记录变更，不覆盖另一账户的更新。
- 同账户 operation lock 排斥竞争修改，事务锁不跨网络或人工交互。`busy` 返回调用者，不偷偷等待、重试或换账户。
- 每条记录的恢复 marker 在替换前持久化。标记文件保存 armed/cleared 状态，不手动删除；不确定发布隔离该条目，不能恢复旧 token。API key 显式 replace/remove、OAuth 显式 login/logout 才可恢复。

仅面向满足这些保证的本地文件系统；不是分布式 store。平台库存在或交叉编译不证明实际文件系统、ACL、断电或原生运行语义。`secrecy` 在自持有 secret 值释放时归零；`zeroize` 保护主要私有文件和 authority 响应字节缓冲。显式持久化仍会暴露给受保护文件；Serde、HTTP、分配器或系统可能另有副本。此方案不隔离恶意同用户/管理员进程，不保证全进程或存储介质的 secure erasure。

## 命令

CLI 由 `clap` 解析，错误不回显输入值。省略 `--store` 使用上述默认目录，显式参数可覆盖；父目录须由操作者准备，不读取默认账户或环境 key。以下命令保留显式 store，以便操作其他自有目录。

```sh
cargo build --locked --offline --bin morphiecore-auth
```

下列 `STORE`、`PROVIDER_ID`、`KEY_ALIAS`、`BINDING_ID`、`REVISION` 是非秘密操作者变量；真实 key 只放在已授权的私有输入文件中。不要把 secret 放入 argv 或 shell history。

### API key

```sh
target/debug/morphiecore-auth api-key add --store "$STORE" \
  --domain "$PROVIDER_ID" --alias "$KEY_ALIAS" --secret-file /path/to/private/key-input

target/debug/morphiecore-auth api-key list --store "$STORE" --domain "$PROVIDER_ID"
target/debug/morphiecore-auth api-key replace --store "$STORE" \
  --domain "$PROVIDER_ID" --alias "$KEY_ALIAS" --revision "$REVISION" \
  --secret-file /path/to/private/replacement-input

target/debug/morphiecore-auth api-key disable --store "$STORE" \
  --domain "$PROVIDER_ID" --alias "$KEY_ALIAS" --revision "$REVISION"
target/debug/morphiecore-auth api-key enable --store "$STORE" \
  --domain "$PROVIDER_ID" --alias "$KEY_ALIAS" --revision "$REVISION"
target/debug/morphiecore-auth api-key remove --store "$STORE" \
  --domain "$PROVIDER_ID" --alias "$KEY_ALIAS" --revision "$REVISION"
```

输入有界，允许一个结尾 LF/CRLF，不 trim 其他空白；不再提供 Unix FD 专用输入。每次修改后重新读取 revision；过期 revision 拒绝覆盖。replace 发布新材料并启用条目；remove 清除本地 secret 并保留 tombstone，不执行上游 revoke。首次中断发布仅有 marker 时，可显式 remove revision 0 后重新 add。

### 凭据池

先创建/登录被引用的凭据，再准备私有 pool 配置文件。示例只有引用和策略，不含 secret：

```json
{
  "members": [
    {"kind": "api_key", "alias": "primary"},
    {"kind": "api_key", "alias": "backup"}
  ],
  "fallback": true,
  "max_attempts": 2
}
```

OAuth 成员形状为 `{"kind":"oauth","profile":"configured-profile","alias":"chosen-account"}`，须符合具体 Endpoint 合同。不能因放在同一数组便使不兼容认证可互换。

```sh
target/debug/morphiecore-auth pool set --store "$STORE" --provider "$PROVIDER_ID" \
  --binding "$BINDING_ID" --revision 0 --file /path/to/private/pool.json
target/debug/morphiecore-auth pool list --store "$STORE"
```

创建新 pool 时传 revision 0，成功后返回 revision 1；更新已有 pool 使用当前 pool revision，而非文档或凭据 revision。没有 pool 的凭据只受管理，不启用推理。库入口是 `set_pool`、`bind_pool` 和 `Credentials::insert_pool`；静态内存注入仅供显式嵌入及 synthetic checks，不是 binary 的环境回退入口。

### OAuth

OpenAI 的对外 profile 与 Provider ID 统一为 `openai`，其登录方式是 **ChatGPT subscription / SIWC**，不是 Platform API key。这与 pi 按 Provider 选择登录方式的命名分层一致，但不引入 pi 的凭据格式、自动刷新、模型发现或 API-key fallback。协议 owner 仍为 [SIWC driver](../src/credential/siwc.rs)，编译绑定归 [catalog](../src/topology/catalog/subscriptions.rs)。

```sh
target/debug/morphiecore-auth grok login --store "$STORE" --account personal
target/debug/morphiecore-auth grok login --store "$STORE" --account personal --method browser
target/debug/morphiecore-auth openai login --store "$STORE" --account personal
# 身份已登录但未开启套餐权限时，由用户明确重新 consent。
target/debug/morphiecore-auth openai login --store "$STORE" --account personal --consent
target/debug/morphiecore-auth list --store "$STORE"
target/debug/morphiecore-auth openai list --store "$STORE"
target/debug/morphiecore-auth openai refresh --store "$STORE" --account personal
target/debug/morphiecore-auth openai logout --store "$STORE" --account personal
# 显式远端撤销需独立授权；本地清理先于该请求。
target/debug/morphiecore-auth openai logout --store "$STORE" --account personal --revoke
```

store 使用 `openai.json`。旧 `siwc` profile / `openai-siwc` Provider 不作为别名注册。已有旧 store 必须在停止所有写入并取得迁移授权后，同步 namespace、OAuth 记录的 profile、锁/恢复标记名称和相关 pool 引用；不能只改 JSON 文件名。迁移保留 issued client、host ID、tokens、身份、expiry 与恢复状态，不重新登录或消费 refresh token。若目标已存在则停止，不自动合并；程序不自动迁移其他目录。

- SIWC 缺省且仅支持 browser；Grok 缺省 device，browser 显式选择。失败不自动换 client/方法。Grok 可指定已获准 `--client-id`，SIWC 使用 callback issued client，不接受 override。auth 默认使用[环境代理](#出站代理)，`--proxy` 显式覆盖。
- SIWC 浏览器登录的 code exchange / JWKS 非 200 响应报告固定阶段、HTTP 状态和精确白名单错误码，并保留原错误作为 cause；未知码仅显示 `unknown_or_missing`。不输出原始响应、description、任意 header、code 或 tokens；授权页展示 plan usage 不证明最终授予的 scopes。Refresh/revoke 的错误分类不变。
- alias 不是已验证身份。登录结果必须通过 driver 的 issuer/client/subject/workspace 验证；不同 profile 的 token 不互换，不从 JWT header 选择可信 issuer/key URL。
- 同账户竞争返回 busy。失败/取消的 login 保留已有可用 session；ticket 防止过期登录覆盖退出或后续登录。
- refresh 发出前先持久化移除可复用 secrets；拒绝、取消或不确定消费结果需要重新登录，不能盲目重发 refresh token。SIWC 已收到的 replacement 在验证前持久化为 `renewal_pending`，禁止借用；再次 `refresh` 只验证这份材料，不再次发 token grant。身份不匹配不能发布，需显式 logout 后重新登录；不手动恢复旧备份。
- logout 先清理本地，再可选远端 revoke；远端不确定不恢复本地 tokens。Ctrl-C 取消本次操作，不声称上游终止。

## 出站代理

auth 的 login/refresh/revoke 无显式 `--proxy` 时使用 reqwest 的环境代理规则：按目标 scheme 选择 `HTTP_PROXY` / `HTTPS_PROXY`，以 `ALL_PROXY` 为 fallback，并遵守 `NO_PROXY`；支持对应小写变量。大小写并存和 CGI 场景按锁定 reqwest 的规则处理，建议同一变量只设置一种拼写。

主程序优先级为 `--proxy` > `gateway.json` 中的 `proxy` > 环境代理。显式 CLI/文件代理用于全部出站 HTTP(S)，不与环境代理或 `NO_PROXY` 混用。代理失败不会自动改走直连或重试。环境变量只来自操作者启动进程的配置，不从下游请求读取；不要打印代理 URL 或包含认证信息的环境值。

```sh
# 代理地址为示例；只配置选路，不执行登录或推理。
export HTTPS_PROXY=http://127.0.0.1:7890
export NO_PROXY=localhost,127.0.0.1,::1
```

`--proxy` 支持受信 HTTP/HTTPS 代理；不因此增加 SOCKS 支持。浏览器网络设置与 CLI 环境分别生效，浏览器授权成功不证明 CLI token exchange 使用相同出口。库级 `Gateway` / `HttpTransport::new` 与原 `Bootstrap::from_files` 保持无环境代理的显式策略；binary 通过 `Bootstrap::from_files_with_proxy` 选择环境默认。Synthetic authority 和隔离测试不继承真实环境代理。

## 交互与身份

设备入口只展示第一方验证 URL 与 user code，不展示 device secret；浏览器入口只展示第一方授权 URL 和准确 callback，不自动打开浏览器。

Callback 仅监听 literal `127.0.0.1`。Grok 使用 `/callback`，SIWC 使用 `/auth/callback`；缺省或 `--callback-port 0` 由 OS 分配。显式端口占用即失败，不停止其他进程。严格校验 method/path/Host、state、唯一参数与预算；接收 callback 不等于身份验证或持久化成功。

Grok browser 使用 ES256/OIDC nonce 与 UserInfo subject 对齐；device 验证固定 UserInfo。SIWC 使用 RS256、受信 issuer、issued-client audience、subject、expiry 和本次 nonce；不解释 opaque account metadata 或继承产品账户 header。稳定 host ID 属于所选本地 store，同 host 重登/切换账户复用；不同远程 host 的配置与 token 转移不在该 CLI 的自动职责内。

SIWC 的注册参数与应用名称归 [driver](../src/credential/siwc.rs)，description 取 package metadata。公开授权参数没有 description 字段，不发送猜测字段。CLI 在登录前告知本地数据存储与单用户应用用途，使用 `Continue with ChatGPT`；状态报告 `plan_usage_enabled`，没有实际套餐权限时不可借用推理。权限不证明所选模型可调用；usage 管理入口为 <https://chatgpt.com/settings/usage>。

新 SIWC 登录不会转换、撤销或删除旧 Codex 文件。为新路径选择显式 store，并按当前 catalog 配置新的单成员 Provider pool；旧 pool 不自动重命名或启用。私有文件清理与远端撤销须分别授权。

## 状态与验收

`list` 按 kind 输出本地状态；不输出 secret、主体/workspace、内部 replay identity 或 token 前后缀。API key 的 recovery 标记优先于 enabled；OAuth access 的 unknown 不等于 valid。Pool list 仅面向操作者显示引用和策略，不能作为下游凭据选择 API。

新增授权 profile 实现 [driver](../src/credential/driver.rs)并显式注册；不在 manager/store 堆叠 Provider 分支或可执行脚本。默认验证只使用 synthetic 本地文件/authority，命令见 [开发指南](development.md)；真实授权、迁移、推理和付费 probes 仍须单独授权。
