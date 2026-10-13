# Usage

This guide covers local setup and client requests. For the build environment, see [Development](development.md#environment).

## Contents

- [Prepare credentials](#prepare-credentials)
- [Configure the gateway](#configure-the-gateway)
- [Start the gateway](#start-the-gateway)
- [Send a request](#send-a-request)
- [HTTP API](#http-api)
- [Conversation and cache affinity](#conversation-and-cache-affinity)
- [Errors and limits](#errors-and-limits)

## Prepare credentials

The gateway and `morphiecore-auth` share a default store:

- Unix: `$HOME/.local/share/morphiecore/credentials`
- Windows: `%USERPROFILE%/.local/share/morphiecore/credentials`

They do not discover credentials from the working directory, `XDG_DATA_HOME`, environment keys or third-party auth caches.

On Unix, the store directory must be `0700` and private files must be `0600`. Unsafe permissions are rejected, not repaired. Never commit, print or hand-edit credential files while the service runs.

Build the CLIs:

```sh
cargo build --locked --offline --bins
install -d -m 700 "$HOME/.local/share/morphiecore/credentials"
```

The examples below use `morphiecore-auth` as shorthand for `cargo run --locked --offline --bin morphiecore-auth --`. Replace placeholders with catalog identities. Provider definitions are in [src/provider/catalog.rs](../src/provider/catalog.rs); model and credential bindings are in [src/topology/catalog.rs](../src/topology/catalog.rs) and its modules.

### Add an API key

Read the key from a private file, not a command-line value:

```sh
morphiecore-auth api-key add --domain "$PROVIDER_ID" --alias "$ALIAS" --secret-file /path/to/private-key
morphiecore-auth api-key list --domain "$PROVIDER_ID"
```

Updates require the current revision:

```sh
morphiecore-auth api-key replace --domain "$PROVIDER_ID" --alias "$ALIAS" --revision "$REV" --secret-file /path/to/private-key
morphiecore-auth api-key disable --domain "$PROVIDER_ID" --alias "$ALIAS" --revision "$REV"
```

`enable` and `remove` use the same identity and revision arguments. Re-read the revision after every change. Removing a key deletes the local secret; it does not revoke the upstream key.

Every subcommand accepts `--store /path/to/store`.

### Create a pool

Only credentials referenced by a pool are used for inference. A pool file contains references and policy, not secrets:

```json
{
  "members": [{"kind": "api_key", "alias": "primary"}],
  "fallback": false,
  "max_attempts": 1
}
```

Use revision `0` to create a new pool:

```sh
morphiecore-auth pool set --provider "$PROVIDER_ID" --binding "$BINDING_ID" --revision 0 --file /path/to/pool.json
morphiecore-auth pool list
```

Members are ordered. To allow credential fallback, add members, set `fallback: true` and raise `max_attempts`. Exhausted quota, unavailable credentials, transport failures, timeouts and server failures may advance before publication. Authentication failures and rate limits of unknown scope do not rotate keys. See [Failover](architecture.md#failover).

An OAuth member uses `{"kind":"oauth","profile":"openai","alias":"personal"}`. An `openai` subscription pool must have one member, no fallback and `max_attempts: 1`.

### OAuth

Login, refresh and revocation are explicit operator actions:

```sh
morphiecore-auth grok login --account personal
morphiecore-auth openai login --account personal
morphiecore-auth list
morphiecore-auth openai refresh --account personal
morphiecore-auth openai logout --account personal
```

- `openai` is the ChatGPT subscription profile (SIWC), not an OpenAI Platform API key.
- Grok defaults to device login; `--method browser` selects browser login. SIWC supports browser login only.
- `--consent` explicitly requests ChatGPT plan consent during login.
- `--callback-port` sets the loopback callback port. Grok also accepts `--client-id`.
- `logout --revoke` requests upstream revocation.
- `login`, `refresh` and `logout` accept `--proxy`.

Callbacks listen on `127.0.0.1`. Login checks issuer, client and subject. Inference never starts login or refresh.

## Configure the gateway

Create a private `gateway.json` in the store, or select a different file with `--config`. This example uses a synthetic ingress token; replace it with a private random token of 32–4096 bytes:

```json
{
  "client_key": "synthetic-local-ingress-token-00000001",
  "bind": "127.0.0.1:8080",
  "models": ["configured-public-model"],
  "max_attempts": 2
}
```

Replace `configured-public-model` with a catalog label whose credential pool is configured.

| Field | Meaning |
|---|---|
| `client_key` | Required ingress token, separate from upstream credentials |
| `bind` | Loopback listener; default `127.0.0.1:8080` |
| `proxy` | Outbound proxy |
| `models` | Public labels to activate |
| `max_attempts` | Optional global attempt ceiling, 1–64 |
| `responses_via_chat` | Listed models whose Responses entry uses their Chat endpoint |
| `failover` | Map from a listed model to its ordered Provider IDs; each needs a pool |
| `implicit_cache_affinity` | Best-effort prefix index; default `true` |
| `diagnostics` | Path for a new private diagnostics file; no bodies are recorded |

Explicit `models` selection is recommended. If omitted, eligible generation bindings with pools are activated. Standalone media, Embeddings, audio-input and native Messages bindings require explicit selection.

For models covered by `failover`, only the named Providers are used. Otherwise available bindings follow catalog order. Configuration is not an upstream parameter bag: unknown fields, duplicate or unknown models, and models without a matching pool fail startup.

Exact activation rules are in [bootstrap.rs](../src/gateway/startup/bootstrap.rs).

## Start the gateway

```sh
cargo run --locked --offline --bin morphiecore
```

Override paths or proxy when needed:

```sh
cargo run --locked --offline --bin morphiecore -- \
  --credentials-dir /path/to/store \
  --config /path/to/gateway.json \
  --proxy http://127.0.0.1:7890
```

Proxy precedence is CLI → config → environment proxy settings. A failing explicit proxy does not fall back to a direct connection.

Ctrl-C shuts down the listener and cancels in-flight upstream work.

## Send a request

Set the SDK `base_url` to `http://127.0.0.1:8080/v1` and `api_key` to the ingress `client_key`. Anthropic clients use the bare origin, `http://127.0.0.1:8080`.

Send JSON with `Content-Type: application/json` and exactly one credential header:

- `Authorization: Bearer <client_key>`, or
- `x-api-key: <client_key>`.

Both forms work at ingress. Repeated headers or both forms together are rejected, even if they carry the same token.

### Responses

```json
{"model":"configured-public-model","input":"Reply with exactly pong.","max_output_tokens":64}
```

### Chat Completions

```json
{"model":"configured-public-model","messages":[{"role":"user","content":"Reply with exactly pong."}],"max_completion_tokens":64}
```

### Messages

```json
{"model":"configured-public-model","messages":[{"role":"user","content":"Reply with exactly pong."}],"max_tokens":64}
```

Use `"stream": true` for SSE. Send the full history each turn; the gateway does not store conversation history.

Return reasoning carriers unchanged in later history. Chat clients must preserve `reasoning_details`, not just readable text. See [Reasoning replay](architecture.md#reasoning-replay).

Nonempty Chat `stop` and Messages `stop_sequences` are unsupported and rejected. Messages `thinking.display` is preserved. Messages responses with citations or missing/ambiguous final usage fail projection rather than dropping annotations or inventing zero counts; a failure after streaming starts aborts delivery without a success terminal.

### Image input

Select an activated public model that admits image input. A tool-result image also needs a representable tool-media carrier on the selected upstream.

| Client protocol | User image | Tool-result image |
|---|---|---|
| Chat Completions | `image_url` | No tool-message carrier; clients may send a separate user image themselves |
| Responses | `input_image` | `function_call_output.output` array |
| Messages | `image` with a base64 or URL source | `tool_result.content` array |

Responses tool images require a Responses or native upstream carrier. Messages uses the model's Chat candidates, so a Messages tool image still fails on a Chat upstream. `responses_via_chat` has the same limitation. The gateway does not move tool media to another role, because standard Chat has no tool-media carrier.

For an inline Responses image, replace `<base64-image>` with valid base64 for the declared image type:

```json
{
  "model": "configured-public-model",
  "input": [{
    "role": "user",
    "content": [
      {"type": "input_text", "text": "Describe this image briefly."},
      {"type": "input_image", "image_url": "data:image/png;base64,<base64-image>", "detail": "auto"}
    ]
  }],
  "max_output_tokens": 256
}
```

For a tool image, append a `function_call_output` with the returned `call_id` and an `output` array containing `input_image`. Return the complete earlier history, including reported thinking and opaque replay values. Do not fabricate a tool call to substitute for model-issued history.

Image sources stay inert: the gateway does not fetch URLs, decode pixels, resize or transcode. Public URL availability and actual image validity remain upstream concerns. Client Provider file references are rejected, including Google Files API URLs disguised as ordinary image URLs. File services remain deferred.

Formats, encoded sizes and counts are target constraints, not a promise that every IR image fits every upstream. Per-use `detail` without a target slot is removed by a named rule on the candidate copy. The source request is unchanged. See [Media](architecture.md#media), [IR resource budgets](../src/semantic/task/generation/resources/mod.rs) and [Messages media limits](../src/protocol/anthropic/media.rs).

## HTTP API

| Method and path | Purpose |
|---|---|
| `GET /v1/models` | List activated public models |
| `GET /v1/models/{model}` | Retrieve an activated public model |
| `DELETE /v1/models/{model}` | Denied: 403 for an activated model, 404 otherwise |
| `POST /v1/responses` | Responses, JSON or SSE |
| `POST /v1/chat/completions` | Chat Completions, JSON or SSE |
| `POST /v1/messages` | Messages, JSON or SSE, using the model's Chat candidates |

Frozen endpoints require explicit model activation:

| Method and path | Purpose |
|---|---|
| `POST /v1/embeddings` | Text input, float vectors |
| `POST /v1/images/generations` | Static inline images |
| `POST /v1/audio/speech` | Binary audio |
| `POST /v1/audio/transcriptions` | One WAV/MP3 multipart upload |

The [OpenAPI summary](openapi.json) describes envelopes and selected constraints, not full vendor API conformance. Codecs define exact admission. Business input cannot choose an upstream origin, credential or auth header; inbound headers are not forwarded.

## Conversation and cache affinity

Use `X-MorphieCore-Conversation-Id` for one logical conversation. Keep it stable across turns and model changes. Use a new ID for a new conversation.

The ID must be 1–256 visible ASCII characters, without commas, in one header. If it is absent, ingress checks these agent headers in order:

1. opencode: `X-Session-Id`
2. pi: `session_id`
3. Codex: `session-id`

pi's Chat integration sends its session header only with `compat.sendSessionAffinityHeaders: true`.

This ID groups routing and cache affinity; it does not store history. Body `session_id` is rejected. Standard `prompt_cache_key` is a separate cache-grouping intent. Without a declared conversation, the optional prefix index offers best-effort cache reuse, not executor affinity or replay permission.

## Errors and limits

OpenAI routes return `{"error":{"message","type","param","code"}}`. Messages handler errors use Anthropic's `{"type":"error","error":{"type","message"}}` envelope. Unclassified Messages errors put the OpenAI `code` in the message, for example `The request could not be completed (upstream_error).` Authentication fails before the handler and uses the OpenAI envelope on every route.

| HTTP | Meaning |
|---|---|
| 400 | Invalid or unrepresentable request, context overflow |
| 401 | Missing, wrong or ambiguous ingress credential |
| 404 / 405 | Unknown model or route / unsupported method |
| 408 / 413 / 415 | Body timeout / body too large / unsupported media type |
| 429 | Gateway busy, upstream rate limit or exhausted quota |
| 502 / 504 | Upstream or projection failure / upstream timeout |
| 503 / 500 | Shutdown, unavailable credentials or upstream overload / local fault |

Messages maps exhausted quota to 402 and upstream overload to 529. Reported rate-limit resets may appear as `Retry-After`.

A Responses stream with classified context overflow or exhausted quota may return HTTP 200 with `response.failed`. HTTP 200 alone does not mean success. After body publication, errors can only report failure or abort delivery; no candidate replay or fabricated success terminal is allowed.

Resource defaults and bounds are in [Limits](../src/gateway/startup/config.rs). Configuration fields above do not expose every embedded-library limit.
