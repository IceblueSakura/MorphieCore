# Usage

## Startup

```sh
cargo run --locked --offline --bin morphiecore
cargo run --locked --offline --bin morphiecore -- --credentials-dir /path/to/store --config /path/to/gateway.json
```

- The credential store defaults to `~/.local/share/morphiecore/credentials` (`USERPROFILE` on Windows); `--credentials-dir` overrides it. The current directory, `XDG_DATA_HOME` and keys in environment variables are never read.
- The gateway config defaults to `gateway.json` inside the store; `--config` overrides it.
- Outbound proxy precedence: `--proxy` > config `proxy` > environment (`HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY`, `NO_PROXY`). A failing explicit proxy never falls back to a direct connection.
- Listens on loopback only. Ctrl-C shuts down gracefully and cancels in-flight upstream requests.

### gateway.json

```json
{
  "client_key": "synthetic-local-ingress-token-00000001",
  "bind": "127.0.0.1:8080",
  "models": ["configured-public-model"],
  "max_attempts": 2
}
```

| Field | Meaning |
|---|---|
| `client_key` | Downstream Bearer token; unrelated to upstream keys |
| `bind` | Loopback address, default `127.0.0.1:8080` |
| `proxy` | Outbound proxy |
| `models` | Public models to enable. If omitted, the default chat bindings with configured pools are enabled; media, Embedding, audio-input and native Messages models must be listed explicitly |
| `max_attempts` | Global attempt ceiling (1–64) |
| `responses_via_chat` | Subset of `models` whose Responses entry uses the model's Chat upstream instead |
| `failover` | Per listed model, its Providers in failover order, e.g. `{"glm-5.3": ["openrouter", "zhipu"]}`. Each needs a pool; unlisted Providers are not used. Without it, every Provider with a pool serves in catalog order |
| `implicit_cache_affinity` | Implicit cache-affinity index, default `true` |
| `diagnostics` | Local diagnostics metadata file (must be new; never records bodies) |

Unknown fields, unknown or duplicate models, and models without a matching pool all fail startup. Available models, protocols and upstream bindings are defined in `src/topology/catalog*` and `src/gateway/bootstrap.rs`.

## HTTP API

| Method and path | Purpose |
|---|---|
| `GET /v1/models`, `GET /v1/models/{model}` | Enabled public models; `DELETE` on an enabled model returns 403 |
| `POST /v1/responses` | Responses, JSON or SSE |
| `POST /v1/chat/completions` | Chat Completions, JSON or SSE |
| `POST /v1/messages` | Anthropic Messages for Claude Code, JSON or SSE; any public model with a Chat entry. The key may come as `x-api-key`; errors use Anthropic's envelope |
| `POST /v1/embeddings` | Text float vectors |
| `POST /v1/images/generations` | Static image generation |
| `POST /v1/audio/speech` | Text to speech, binary audio |
| `POST /v1/audio/transcriptions` | WAV/MP3 multipart transcription |

- Auth: exactly one `Authorization: Bearer <client_key>`, checked before the body is collected.
- `model` accepts only enabled public labels. Upstream origin, model and auth come from trusted bindings; inbound headers are not forwarded.
- Optional `X-MorphieCore-Conversation-Id` (1–256 visible ASCII characters, no commas, at most one): keep it stable for one logical conversation, across model switches. The gateway derives upstream cache/session carriers from it. It does not mean the server stores history; send the full history every turn. Without it, an agent's declared session header is read the same way: opencode `X-Session-Id`, pi `session_id` (pi's Chat API sends it only with `compat.sendSessionAffinityHeaders: true`), Codex `session-id`.
- Provider `session_id` in the body is rejected; `prompt_cache_key` is kept as a separate cache group.
- Public API summary: [openapi.json](openapi.json).

Minimal requests:

```json
{"model":"configured-public-model","input":"Reply with exactly pong.","max_output_tokens":64}
```

```json
{"model":"configured-public-model","messages":[{"role":"user","content":"Reply with exactly pong."}],"max_completion_tokens":64}
```

With an SDK, set `base_url` to `http://127.0.0.1:8080/v1` and `api_key` to `client_key`.

### Errors

Error bodies are `{"error":{"message","type","param","code"}}` and never echo upstream bodies, credentials or origins.

| HTTP | Cause |
|---|---|
| 400 | Invalid request, semantics the target cannot express, duplicate JSON keys, output cap exceeded |
| 401 | Ingress auth missing, wrong or repeated |
| 404 / 405 | Model or route not available / method not allowed |
| 408 / 413 / 415 | Body collection timeout / body too large / unsupported media type |
| 429 | Concurrency full or upstream rate limit |
| 502 / 504 | Upstream status, protocol or projection failure / timeout |
| 503 / 500 | Shutting down or credentials unavailable / local fault |

Once a stream has sent its first frame, an error can only abort the connection; no success terminal is sent. Resource limits are in `Limits` in `src/gateway/config.rs`.

## Credentials

Manage credentials with `morphiecore-auth`. Store files contain secrets: never commit, print or hand-edit them while the service runs. On Unix the directory and files must be `0700`/`0600`; unsafe permissions are rejected, not repaired.

```sh
install -d -m 700 "$HOME/.local/share/morphiecore/credentials"
cargo build --locked --offline --bin morphiecore-auth
```

### API keys

Keys are read from private files, never from command-line arguments:

```sh
morphiecore-auth api-key add     --domain "$PROVIDER_ID" --alias "$ALIAS" --secret-file /path/to/key
morphiecore-auth api-key list    --domain "$PROVIDER_ID"
morphiecore-auth api-key replace --domain "$PROVIDER_ID" --alias "$ALIAS" --revision "$REV" --secret-file /path/to/key
morphiecore-auth api-key disable|enable|remove --domain "$PROVIDER_ID" --alias "$ALIAS" --revision "$REV"
```

Re-read the revision after every change; stale revisions are rejected. `remove` deletes the local secret only and does not revoke the upstream key.

### Pools

Only credentials in a pool are used for inference. A pool file holds references and policy only:

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

OAuth members are `{"kind":"oauth","profile":"…","alias":"…"}`. `fallback` is off by default; when on, the next member is tried only for pre-commit connect failures, timeouts, 5xx or locally unavailable credentials. 401/403 and 429 of unknown scope do not advance.

### OAuth

```sh
morphiecore-auth grok login --account personal [--method browser]
morphiecore-auth openai login --account personal [--consent]   # ChatGPT subscription (SIWC), not a Platform API key
morphiecore-auth list
morphiecore-auth openai refresh|logout --account personal [--revoke]
```

- Callbacks listen on `127.0.0.1` only; login results must pass issuer, client and subject checks.
- Inference requests never log in or refresh; refresh and revocation are explicit.
- Grok defaults to the device flow; SIWC supports browser only. `--callback-port` sets the callback port; Grok accepts `--client-id`.
- Every subcommand accepts `--store` to override the default directory; `login`, `refresh` and `logout` accept `--proxy` to override environment proxies.
