# Usage

This guide covers local setup and client requests. For the build environment, see [Development](development.md#environment).

## Contents

- [Prepare credentials](#prepare-credentials)
- [Configure the gateway](#configure-the-gateway)
- [Start the gateway](#start-the-gateway)
- [Send a request](#send-a-request)
- [Stored Responses](#stored-responses)
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
install -d -m 700 "$HOME/.local/share/morphiecore" "$HOME/.local/share/morphiecore/credentials"
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

All authorized members remain selectable; pool `max_attempts` is a runtime ceiling per endpoint, not a startup membership cutoff. With `fallback: false`, one member may be tried, but affinity/health can make a later member the first choice. Expanded candidates remain bounded to 64 per entry. Unknown-scope rate limits cool the configured pool at that deployment/model across requests; unrelated pools are not globally disabled.

An OAuth member uses `{"kind":"oauth","profile":"openai","alias":"personal"}`. An `openai` subscription pool must have one member, no fallback and `max_attempts: 1`.

That pool limit does not disable cross-Provider fallback on a mixed model route. Such a route may contain one SIWC endpoint; its public contract stays shared, while requests containing unsupported SIWC controls skip that candidate. The model route's `max_attempts` bounds all attempts, and publication ends fallback. There is no separate service-wide attempt field.

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

Create private `gateway.json` and `models.json` files in `$HOME/.local/share/morphiecore` (use `%USERPROFILE%` on Windows), or select a different service file with `--config`. Their parent directories must also be private on Unix. Keep configuration outside the credential store. Each file is bounded to 64 KiB and rejects unknown fields and duplicate JSON keys. Changes take effect at startup; there is no hot reload.

`gateway.json` owns service settings. This example uses a synthetic ingress token; replace it with a private random token of 32–4096 bytes:

```json
{
  "client_key": "synthetic-local-ingress-token-00000001",
  "bind": "127.0.0.1:8080",
  "models_file": "models.json",
  "credentials_dir": "credentials"
}
```

| Field | Meaning |
|---|---|
| `client_key` | Required ingress token, separate from upstream credentials |
| `bind` | Loopback listener; default `127.0.0.1:8080` |
| `models_file` | Model configuration path; default `models.json` beside this file |
| `credentials_dir` | Optional credential-store path; omission retains the independent home-relative default |
| `proxy` | Outbound proxy |
| `concurrency` | Concurrent exchanges; default 16, range 1–256 |
| `exchange_timeout_ms` | Service-wide exchange ceiling; default 3600000, range 1–86400000 |
| `delta_coalescing` | Optional bounded text delivery batching; omitted or `null` disables it. See [Delta coalescing](#delta-coalescing) |
| `diagnostics` | Path for a new private diagnostics file; no bodies are recorded |
| `capture` | Optional private exchange capture; omitted or all-disabled selections record no content |

Paths in the service file resolve relative to that file, not the process working directory. CLI paths remain explicit process-relative or absolute paths.

### Delta coalescing

To opt in, add this field to `gateway.json`:

```json
{
  "delta_coalescing": {
    "max_delay_ms": 10,
    "max_pending_bytes": 16384
  }
}
```

Both fields are required. `max_delay_ms` must be 1–100 milliseconds; `max_pending_bytes` must be 1024–1048576 bytes. Invalid bounds or unknown fields fail startup. Omission preserves immediate delivery. Rust embedding uses the same optional `Limits::delta_coalescing` policy.

Only adjacent nonempty text deltas with the same projected owner, part and encoding context, and no probabilities, are merged. The first content batch is not intentionally delayed. Other events and context changes flush pending text in order. A fixed deadline flushes during upstream silence; later fragments do not extend it. The delay bounds intentional buffering, not client backpressure or end-to-end latency.

The byte bound includes conservatively accounted retained metadata, fidelity and text storage/escaping, not process RSS. Contexts or fragments that cannot fit pass through; reported instruction-message trees also bypass buffering. Upstream intake limits remain unchanged. Cancellation drops pending state; successful terminals still require validated upstream EOF. `stream:false`, including a declared forced-SSE upstream, uses existing final-response materialization without a coalescing buffer.

This service policy adds no HTTP request fields, changes no routing rules and reduces neither upstream traffic nor token usage. Optional diagnostics report the peak accounted pending bytes observed when an attempt returns, without content. Synthetic measurements demonstrate overhead reduction only for their fixture; SDK, real-client and Provider acceptance remain separate gates.

### Model configuration

`models.json` owns explicit public-model selection and ordered upstream candidates:

```json
{
  "implicit_cache_affinity": true,
  "models": {
    "configured-public-model": {
      "max_attempts": 2,
      "total_timeout_ms": 120000,
      "candidates": [
        {
          "binding": "existing-catalog-endpoint",
          "base_url": "https://upstream.example/v1",
          "upstream_model": "upstream-model-name",
          "credential": "configured-pool",
          "attempt_timeout_ms": 60000,
          "reasoning_as_text": false
        }
      ]
    }
  }
}
```

Replace the placeholders with a registered public label, one of its catalog endpoint IDs and a pool in that endpoint's Provider domain. Binding definitions are in [the catalog](../src/topology/catalog.rs). `credential` is a pool reference, never an inline key; manage its key/account members through the existing credential CLI. Omitted deployment fields retain the selected binding's values.

| Scope | Field | Meaning |
|---|---|---|
| File | `implicit_cache_affinity` | Best-effort prefix affinity; default `true` |
| Model | `candidates` | Nonempty ordered array of existing bindings for this public model |
| Model | `max_attempts` | Optional total attempt ceiling, including the first attempt; range 1–64, otherwise the route default |
| Model | `total_timeout_ms` | Optional exchange deadline after request intake; bounded by the service ceiling |
| Model | `thinking_tokens` | Optional six-level numeric table shared by this model's candidates |
| Candidate | `binding` | Required catalog endpoint ID; fixes protocol, dialect, canonical model and authentication kind |
| Candidate | `base_url` | Trusted origin plus API prefix, without the operation suffix |
| Candidate | `upstream_model` | Wire spelling only; does not change canonical identity or model capabilities |
| Candidate | `credential` | Pool reference; defaults to the binding's catalog pool |
| Candidate | `attempt_timeout_ms` | One upstream attempt's timeout, further bounded by the remaining exchange deadline |
| Candidate | `reasoning_as_text` | Permit the existing named history conversion; default `false` |
| Candidate | `thinking_conversion` | `budget_to_effort` or `effort_to_budget`; omission performs no numeric conversion |

Timeout values accept 1–86400000 milliseconds. Attempt ceilings span all endpoint and credential candidates; they do not multiply each other or schedule extra retries of an exhausted candidate list. Beginning local credential resolution counts as an attempt; skipped candidates do not. Array order remains stable, except for an available conversation preference and health gating. Selection scans past ineligible successors while preserving every prior failure restriction. Publication always prohibits replay.

Cooling targets are not tried before their deadline, even as a last resort. An expired scope admits one foreground recovery probe, held through final delivery. All otherwise-legal targets cooling/probing returns `503 upstream_unavailable` with `Retry-After`; no background probes or long waits are added. Standard upstream delays up to one day are retained. Without one, transient failures back off from 1–60 seconds and credential/quota/rate failures from 60–1800 seconds. Cancellation releases probe ownership; a late success cannot erase a newer cooldown.

All Generation client protocols use the same configured candidate array. To send Responses clients through Chat, select a Chat binding; there is no separate `responses_via_chat` switch. The old service-file `models`, `failover` and attempt fields are rejected, not automatically migrated.

`base_url` includes the API prefix: `/v1` plus a Chat binding produces `/v1/chat/completions`; a Messages binding produces `/v1/messages`. Google generateContent appends `/models/{upstream_model}:streamGenerateContent`, while Interactions appends `/interactions`. HTTPS is required except for literal loopback HTTP. Userinfo, query strings, fragments and ambiguous path escapes are rejected. Business requests cannot override these trusted targets.

Replay fingerprints include the normalized deployment origin/path. Changing either makes existing opaque replay foreign; the URL itself never reaches clients. This also replaces the former Provider/model-only fingerprints. Required compacted history cannot migrate across that boundary.

For `thinking_tokens`, provide all six positive values, for example:

```json
{
  "minimal": 1024,
  "low": 2048,
  "medium": 3072,
  "high": 8192,
  "xhigh": 16384,
  "max": 24576
}
```

Place this object on the public model, alongside `candidates`. At least one candidate must select a conversion when a custom table is supplied. A conversion without a custom table uses the [shared preset](architecture.md#thinking-budget-quantization). Duplicate token values are allowed; nearest-budget matching and highest-level ties retain the existing algorithm.

`budget_to_effort` uses the target's declared effort scope: response-wide for Messages, reasoning-only for the other current protocols. `effort_to_budget` accepts reasoning-scoped effort and currently requires a generateContent binding. An existing qualitative effort calibration cannot be combined with a numeric conversion. Unsupported target levels, incompatible controls and over-ceiling budgets still reject; the table does not widen model admission or change dialects.

`reasoning_as_text` controls conversion of history reasoning into assistant text, not visibility of new reasoning output. With the default `false`, a candidate that needs that conversion rejects rather than deleting readable content. Valid native reasoning/replay carriers remain available.

Only listed models are activated. Frozen Images, Speech, Transcription and Embeddings entries use an empty object (`"configured-frozen-model": {}`); they retain their existing single catalog target and pool constraints, without Generation overrides.

The file supports at most 64 models and 64 candidates per model; expanded credential candidates retain the existing runtime bound. Unknown bindings, mismatched model identities, duplicate deployments, missing pools and invalid bounds fail startup. See [service loading](../src/gateway/startup/bootstrap.rs) and [model assembly](../src/gateway/startup/models.rs).

### Private exchange capture

Capture is separate from body-free diagnostics and disabled by default. To enable it, add a `capture` object to the private gateway configuration:

```json
{
  "capture": {
    "path": "/path/to/private/exchanges.jsonl",
    "client_request": {"headers": true, "body": true},
    "upstream_request": {"headers": true, "body": true},
    "upstream_response": {"headers": true, "body": true},
    "client_response": {"headers": true, "body": true},
    "max_unit_bytes": 262144,
    "max_exchange_bytes": 4194304,
    "max_total_bytes": 67108864
  }
}
```

Each header/body switch is independent and defaults to `false`. The parent directory must already be private; the writer exclusively creates a new file with Unix mode `0600` and never replaces an existing file. Choose a new path for a new run. Capture may contain private business data: never commit it, print it, use it as a fixture or replay input, or export it remotely.

| Bound | Default | Accepted range |
|---|---|---|
| Serialized unit | 256 KiB | 1 KiB–1 MiB; raw JSON/SSE parsing conservatively uses half |
| Exchange, shared across attempts | 4 MiB | At least one unit, at most 64 MiB |
| Aggregate file retention | 64 MiB | At least one exchange, at most 1 GiB |

The writer queue holds at most 32 sanitized units. Authentication headers/cookies, credential query values and known authentication body fields are redacted before enqueueing. There is no redaction-disable switch. SSE is captured as normalized application units, not exact wire bytes. Unsafe argument fragments, URI headers and unsupported binary/multipart units are omitted or marked incomplete; large content is explicitly truncated rather than retained unboundedly.

Authenticated client boundaries and Generation upstream I/O are instrumented. Frozen standalone-task upstreams are not. A prepared upstream request is not proof of network transmission, client body handoff is not remote receipt, and `source_eof` proves neither semantic success nor durable full retention. `dropped_before` reports observed exchange-local drops; the library's `Gateway::capture_status()` exposes queue/retention drops and writer failure. Status records can themselves be lost when storage is full or unavailable.

## Start the gateway

```sh
cargo run --locked --offline --bin morphiecore
```

Override paths or proxy when needed:

`--credentials-dir` overrides the service file's `credentials_dir`; when both are omitted, changing `--config` does not relocate the default credential store. The credential CLI's `--store` selects only its credential directory. Legacy configuration is not searched or migrated.

```sh
cargo run --locked --offline --bin morphiecore -- \
  --credentials-dir /path/to/store \
  --config /path/to/gateway.json \
  --proxy http://127.0.0.1:7890
```

Proxy precedence is CLI → config → environment proxy settings. A failing explicit proxy does not fall back to a direct connection.

Ctrl-C shuts down the listener and cancels in-flight upstream work.

For Rust embedding, `Gateway::new` takes a compiled topology and one `TaskEntries` value with separate Generation, Images, Speech, Transcription and Embeddings lists. Empty lists activate nothing. Credentials and limits remain explicit; the embedded constructor does not opt into environment proxies. See [startup configuration](../src/gateway/startup/config.rs).

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

Use `"stream": true` for SSE. Send the full history each turn, or use [stored Responses](#stored-responses). Chat Completions and Messages remain full-history APIs.

Return reasoning carriers unchanged in later history. Chat clients must preserve `reasoning_details`, not just readable text. See [Reasoning replay](architecture.md#reasoning-replay).

For custom-capable targets, Chat JSON uses `{"type":"custom","custom":{"name":"sql","format":{"type":"text"}}}` definitions and `{"type":"custom","custom":{"name":"sql"}}` choices. Calls retain raw `custom.input`; return their ID through an ordinary `role:"tool"` message. Preserve the preceding typed call in history so the result cannot be mistaken for a function result. Grammar definitions use the nested carrier described in [protocol sources](refs/2026-10-16-chat-custom-tools.md).

The pinned Chat SSE contract has no custom-call delta carrier. Chat upstream streaming that enables custom selection and downstream custom-call deltas reject explicitly; use an admitted Responses streaming path instead. Custom history and declarations remain usable for a later stream when tool choice excludes custom output. Codec support does not activate models or execute tools.

Chat accepts the deprecated `max_tokens` as the output cap, but not together with `max_completion_tokens`. Chat and Responses codecs carry `verbosity`; native Messages and Google projections use selected, recorded omission rules. A codec carrier does not establish model-effective support or binding admission. Chat `stop` and Messages `stop_sequences` decode into ordered stop controls, but unchanged default binding admission does not enable them merely because the codecs have carriers.

Active Chat `functions`, `function_call` and `web_search_options`, and Messages `container`, `mcp_servers`, `context_management` and `compaction` remain unsupported and rejected. Other unrecognized fields are ignored by name. Messages `thinking.display` is preserved. Messages responses with citations or missing/ambiguous final usage fail projection rather than dropping annotations or inventing zero counts; a failure after streaming starts aborts delivery without a success terminal.

### Image input

Select an activated public model that admits image input. A tool-result image also needs a representable tool-media carrier on the selected upstream.

| Client protocol | User image | Tool-result image |
|---|---|---|
| Chat Completions | `image_url` | No tool-message carrier; clients may send a separate user image themselves |
| Responses | `input_image` | `function_call_output.output` array |
| Messages | `image` with a base64 or URL source | `tool_result.content` array |

Responses tool images require a Responses or native upstream carrier. All Generation client protocols use the model's configured candidates, so a tool image still fails on a Chat upstream. The gateway does not move tool media to another role, because standard Chat has no tool-media carrier.

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

Formats, encoded sizes and counts are target constraints, not a promise that every IR image fits every upstream. Preserve explicit per-use `detail` through an exact carrier; omission requires proven target-default equivalence, and other approximations require an evidence-backed selected rule. Without one, an unrepresentable detail rejects rather than disappearing. The example's explicit `auto` is not equivalent to absence and is not universally portable to native targets. Any approved projection changes only the candidate copy, not the source request. See [Media](architecture.md#media), [IR resource budgets](../src/semantic/task/generation/resources/mod.rs) and [Messages media limits](../src/protocol/anthropic/media.rs).

## Stored Responses

Storage defaults to true, matching OpenAI Responses. Omitted or null `store` retains a Responses exchange in gateway memory; set `store: false` to opt out. Upstream requests always carry complete history and disable Provider storage where supported.

1. Create a response with `store` omitted or set to `true` and consume the full JSON body or SSE stream. A gateway-owned response ID becomes readable only after validated upstream EOF and final body handoff.
2. Send that ID as `previous_response_id` with only the new input. The gateway prepends the stored request input and response output. Send current instructions, tools and controls explicitly: top-level settings are not inherited, while instruction items in history remain.
3. The successor is also stored by default. Set `store: false` if it need not be retrievable or usable as another predecessor; it may still read an existing predecessor.

```json
{"model":"configured-public-model","input":"Remember the label cedar.","store":true,"max_output_tokens":64}
```

Use the returned `id` in the next request:

```json
{"model":"configured-public-model","previous_response_id":"<returned-id>","input":"What was the label?","store":true,"max_output_tokens":64}
```

For tool continuation, send only the new `function_call_output` or other admitted result. The stored call and reasoning remain in history. Existing carrier and replay checks still apply; a response reference does not bypass model admission or grant cross-Provider replay.

| Operation | Behavior |
|---|---|
| `GET /v1/responses/{response_id}` | Retrieve the final JSON snapshot, including responses created with SSE |
| `DELETE /v1/responses/{response_id}` | Return `{"id":"…","object":"response.deleted","deleted":true}` |
| `GET /v1/responses/{response_id}/input_items` | List the complete resolved input, excluding this response's output; `limit` is 1–100 (default 20), `order` is `asc` or `desc` (default `desc`), and `after` is a returned item ID |

All operations require the same ingress authentication. Unknown, expired, deleted and unstored IDs return `404` with `response_not_found`. Concurrent requests may branch from one predecessor; they do not mutate it. Deleting an ancestor leaves already stored descendants usable.

Snapshots use a fixed TTL from final delivery, not a sliding TTL. Defaults are 30 minutes, 1,024 entries and a 256 MiB accounted payload/owner budget; transient request and body limits still apply. See [Limits](../src/gateway/startup/config.rs) for embedded configuration and bounds. Capacity exhaustion rejects storage with `history_capacity_exceeded` before a successful terminal; an already started SSE body aborts. Unexpired snapshots are not evicted to make room.

History is process-local and is lost at shutdown or restart. A declared conversation header still controls routing/cache affinity independently; response IDs do not select credentials. There is no disk persistence, Conversations API, background generation or event log. Retrieval is JSON only: `stream=true` and `starting_after` are rejected. Retrieval and input listing accept only `include[]=reasoning.encrypted_content` as an optional include; other query fields are rejected.

## HTTP API

| Method and path | Purpose |
|---|---|
| `GET /v1/models` | List activated public models |
| `GET /v1/models/{model}` | Retrieve an activated public model |
| `DELETE /v1/models/{model}` | Denied: 403 for an activated model, 404 otherwise |
| `GET /v1/morphiecore/capabilities` | Inspect partial gateway contracts for activated Generation clients |
| `POST /v1/responses` | Responses, JSON or SSE |
| `GET /v1/responses/{response_id}` | Retrieve a stored response |
| `DELETE /v1/responses/{response_id}` | Delete a stored response |
| `GET /v1/responses/{response_id}/input_items` | Cursor-paginated resolved input |
| `POST /v1/chat/completions` | Chat Completions, JSON or SSE |
| `POST /v1/messages` | Messages, JSON or SSE, using the model's configured candidates |

Frozen endpoints require explicit model activation:

| Method and path | Purpose |
|---|---|
| `POST /v1/embeddings` | Text input, float vectors |
| `POST /v1/images/generations` | Static inline images |
| `POST /v1/audio/speech` | Binary audio |
| `POST /v1/audio/transcriptions` | One WAV/MP3 multipart upload |

The [OpenAPI summary](openapi.json) describes envelopes and selected constraints, not full vendor API conformance. Codecs define exact admission. Business input cannot choose an upstream origin, credential or auth header; inbound headers are not forwarded.

### Capability discovery and adaptation notice

`GET /v1/morphiecore/capabilities` uses the ordinary ingress token. It returns a startup-immutable `version: 1` snapshot with `scope: "gateway_contract"` and `coverage: "partial"`. Each entry names an activated public model and client protocol, with separate `request`, `delivery` and `history` case lists. Frozen standalone tasks are not assessed. No upstream request or request-body collection occurs; HEAD omits the body. The complete snapshot must fit both `response_bytes` and 256 KiB or startup fails.

Each case names a `feature`, a limited `domain`, and `faithful`, `approved_adaptation` or `rejected` treatment. Approved adaptations include stable `rules` IDs. Unlisted cases are `not_assessed`, not implicitly supported or rejected. Statements concern that feature/domain only, assuming all other validation succeeds; neither model-effective settings nor full client loops are asserted.

The first request slice lists explicit public-admission rejection of safety policy, cached-context references and normalization-default tools where applicable; absence does not establish support. Delivery lists selected incidental-settings omissions, protected safety rejection and completion-cause carriers equally for JSON/SSE. Natural or sequence-matched completion differs from ambiguous stop, and these cases exclude pending tool continuation. Responses history preserves a successfully delivered stored projection within its TTL, not facts omitted before delivery or a promise of compatible replay to every target.

Generation JSON/SSE responses carry `X-MorphieCore-Adaptation: possible` and `Link: </v1/morphiecore/capabilities>; rel="describedby"`. This is deliberately conservative, including when no adaptation occurs and when Responses returns a pre-frame failure stream. It is not an actual-loss receipt, a success assertion or proof of Provider-effective behavior. Clients may not display it. Pre-response auth/admission errors, discovery, history operations and standalone tasks do not receive the notice; missing headers do not prove exactness. Standard response bodies and SSE are unchanged; actual receipts remain internal diagnostics.

## Conversation and cache affinity

Use `X-MorphieCore-Conversation-Id` for one logical conversation. Keep it stable across turns and model changes. Use a new ID for a new conversation.

The ID must be 1–256 visible ASCII characters, without commas, in one header. If it is absent, ingress checks these agent headers in order:

1. opencode: `X-Session-Id`
2. pi: `session_id`
3. Codex: `session-id`
4. pi Chat/Messages: `x-session-affinity`

For pi's ordinary API-key Chat and Messages integrations, enable `compat.sendSessionAffinityHeaders: true`; automatic session headers also require `cacheRetention` other than `none`. Responses normally sends `session_id` without that compatibility flag. Installed pi 1.3.0 transport carrier checks corroborate the pinned-source behavior; they do not establish full-client or Provider acceptance. Existing header precedence is unchanged; an invalid selected header rejects rather than falling through. `x-client-request-id` is not a conversation source.

This ID groups routing and cache affinity; it does not store history. Body `session_id` is rejected. Standard `prompt_cache_key` is a separate cache-grouping intent. Without a declared conversation, the optional prefix index offers best-effort cache reuse, not executor affinity or replay permission.

## Errors and limits

OpenAI routes return `{"error":{"message","type","param","code"}}`. Messages handler errors use Anthropic's `{"type":"error","error":{"type","message"}}` envelope. Unclassified Messages errors put the OpenAI `code` in the message, for example `The request could not be completed (upstream_error).` Authentication fails before the handler and uses the OpenAI envelope on every route.

| HTTP | Meaning |
|---|---|
| 400 | Invalid or unrepresentable request, context overflow |
| 401 | Missing, wrong or ambiguous ingress credential |
| 404 / 405 | Unknown model, route or stored response / unsupported method |
| 408 / 413 / 415 | Body timeout / body too large / unsupported media type |
| 429 | Gateway busy, history capacity exhausted, upstream rate limit or exhausted quota |
| 502 / 504 | Upstream or projection failure / upstream timeout |
| 503 / 500 | Shutdown, unavailable credentials/targets or upstream overload / local fault |

Messages maps exhausted quota to 402 and upstream overload to 529. Reported delays and local `upstream_unavailable` recovery delays may appear as `Retry-After`; they are not promises of recovery.

A Responses stream with classified context overflow or exhausted quota may return HTTP 200 with `response.failed`. HTTP 200 alone does not mean success. After body publication, errors can only report failure or abort delivery; no candidate replay or fabricated success terminal is allowed.

Resource defaults and bounds are in [Limits](../src/gateway/startup/config.rs). Configuration fields above do not expose every embedded-library limit.
