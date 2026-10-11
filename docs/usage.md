# 使用

## 启动

```sh
cargo run --locked --offline --bin morphiecore
cargo run --locked --offline --bin morphiecore -- --credentials-dir /path/to/store --config /path/to/gateway.json
```

- 凭据 store 默认为 `~/.local/share/morphiecore/credentials`（Windows 用 `USERPROFILE`），`--credentials-dir` 覆盖；不读取当前目录、`XDG_DATA_HOME` 或环境变量中的 key。
- 入口配置默认为 store 内的 `gateway.json`，`--config` 覆盖。
- 出站代理优先级：`--proxy` > 配置 `proxy` > 环境变量（`HTTPS_PROXY` / `HTTP_PROXY` / `ALL_PROXY` / `NO_PROXY`）。显式代理失败不回退直连。
- 只监听 loopback；Ctrl-C 优雅关闭并取消在途上游请求。

### gateway.json

```json
{
  "client_key": "synthetic-local-ingress-token-00000001",
  "bind": "127.0.0.1:8080",
  "models": ["configured-public-model"],
  "max_attempts": 2
}
```

| 字段 | 含义 |
|---|---|
| `client_key` | 下游入口 Bearer token，与上游 key 无关 |
| `bind` | loopback 地址，默认 `127.0.0.1:8080` |
| `proxy` | 出站代理 |
| `models` | 显式启用的 public model；省略时启用已配置 pool 对应的默认对话绑定。媒体与 Embedding 模型必须显式列出 |
| `max_attempts` | 全局尝试上限 |
| `responses_via_chat` | `models` 的子集：这些模型的 Responses 入口改走其 Chat 上游 |
| `implicit_cache_affinity` | 隐式缓存亲和索引，默认 `true` |
| `diagnostics` | 本地诊断元数据文件路径（必须新建，不记录正文） |

未知字段、未知/重复模型或没有匹配 pool 的模型都会拒绝启动。可用模型、协议与上游绑定以 `src/topology/catalog*` 与 `src/gateway/bootstrap.rs` 为准。

## HTTP 接口

| 方法 / 路径 | 说明 |
|---|---|
| `GET /v1/models`、`GET /v1/models/{model}` | 已启用的 public model；`DELETE` 对已启用模型返回 403 |
| `POST /v1/responses` | Responses，JSON 或 SSE |
| `POST /v1/chat/completions` | Chat Completions，JSON 或 SSE |
| `POST /v1/embeddings` | 文本 float 向量 |
| `POST /v1/images/generations` | 静态图片生成 |
| `POST /v1/audio/speech` | TTS，返回二进制音频 |
| `POST /v1/audio/transcriptions` | WAV/MP3 multipart 识别 |

- 鉴权：唯一的 `Authorization: Bearer <client_key>`，在收集 body 前校验。
- `model` 只接受已启用的 public label；上游 origin、模型、认证都来自受信绑定，入站 header 不透传。
- 可选 `X-MorphieCore-Conversation-Id`（1–256 个可见 ASCII 字符，不含逗号，最多一个）：同一逻辑对话保持同一 ID，切换模型无需修改；网关据此派生上游缓存/会话载体。它不代表服务端保存了历史，每轮仍需提交完整历史。
- Body 中的 Provider `session_id` 字段被拒绝；`prompt_cache_key` 作为独立缓存分组保留。
- 公共接口摘要见 [openapi.json](openapi.json)。

最小请求：

```json
{"model":"configured-public-model","input":"Reply with exactly pong.","max_output_tokens":64}
```

```json
{"model":"configured-public-model","messages":[{"role":"user","content":"Reply with exactly pong."}],"max_completion_tokens":64}
```

SDK 使用时 `base_url` 指向 `http://127.0.0.1:8080/v1`，`api_key` 填 `client_key`。

### 错误

错误体为 `{"error":{"message","type","param","code"}}`，不回显上游正文、凭据或 origin。

| HTTP | 原因 |
|---|---|
| 400 | 请求无效或语义无法由目标表达、重复 JSON key、超出输出上限 |
| 401 | 入口鉴权缺失、错误或重复 |
| 404 / 405 | 模型或路由未开放 / 方法不支持 |
| 408 / 413 / 415 | 收集超时 / body 超限 / 媒体类型不支持 |
| 429 | 并发已满或上游限流 |
| 502 / 504 | 上游状态、协议或投影失败 / 超时 |
| 503 / 500 | 正在关闭或凭据不可用 / 本地故障 |

流式响应在首个 frame 发出后出错只能中止连接，不会补发成功终态。资源上限见 `src/gateway/config.rs` 的 `Limits`。

## 凭据

凭据通过 `morphiecore-auth` 管理。store 中的 JSON 含 secret：不要提交、打印或手工在服务运行时覆盖；Unix 下目录/文件须为 `0700/0600`，权限不安全时拒绝而不自动修复。

```sh
install -d -m 700 "$HOME/.local/share/morphiecore/credentials"
cargo build --locked --offline --bin morphiecore-auth
```

### API key

key 只从私有文件读取，不放进命令行参数：

```sh
morphiecore-auth api-key add     --domain "$PROVIDER_ID" --alias "$ALIAS" --secret-file /path/to/key
morphiecore-auth api-key list    --domain "$PROVIDER_ID"
morphiecore-auth api-key replace --domain "$PROVIDER_ID" --alias "$ALIAS" --revision "$REV" --secret-file /path/to/key
morphiecore-auth api-key disable|enable|remove --domain "$PROVIDER_ID" --alias "$ALIAS" --revision "$REV"
```

每次修改后重新读取 revision；过期 revision 拒绝写入。`remove` 只清除本地 secret，不撤销上游 key。

### 凭据池

只有配置了 pool 的凭据才会用于推理。pool 文件只含引用与策略：

```json
{
  "members": [{"kind": "api_key", "alias": "primary"}, {"kind": "api_key", "alias": "backup"}],
  "fallback": true,
  "max_attempts": 2
}
```

```sh
morphiecore-auth pool set --provider "$PROVIDER_ID" --binding "$BINDING_ID" --revision 0 --file /path/to/pool.json
morphiecore-auth pool list
```

OAuth 成员写作 `{"kind":"oauth","profile":"…","alias":"…"}`。`fallback` 默认关闭；开启后仅在提交前的连接失败、超时、5xx 或本地凭据不可用时换下一个成员，401/403 与未知范围的 429 不换。

### OAuth

```sh
morphiecore-auth grok login --account personal [--method browser]
morphiecore-auth openai login --account personal [--consent]   # ChatGPT 订阅（SIWC），不是 Platform API key
morphiecore-auth list
morphiecore-auth openai refresh|logout --account personal [--revoke]
```

- 回调只监听 `127.0.0.1`；登录结果须通过 issuer / client / subject 校验。
- 普通推理请求不登录、不刷新；刷新与撤销是显式操作。
- Grok 默认 device 流程，SIWC 只支持 browser；`--callback-port` 指定回调端口，Grok 可用 `--client-id`。
- 所有子命令都支持 `--store` 覆盖默认目录；login / refresh / logout 支持 `--proxy` 覆盖环境代理。
