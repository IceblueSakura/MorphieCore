# Usage

This guide owns local setup, configuration and client-facing procedures for implemented interfaces. [Architecture](architecture.md) explains the contracts; [Roadmap](roadmap.md) separates selected scope from acceptance. For tooling and verification, see [Development](development.md). Planned endpoints in the [API design](api-expansion-design.md) are not instructions for currently available APIs.

## Contents

- [Prepare credentials](#prepare-credentials)
  - [Add an API key](#add-an-api-key)
  - [Create a pool](#create-a-pool)
  - [OAuth](#oauth)
- [Manual upstream diagnostics](#manual-upstream-diagnostics)
- [Configure the gateway](#configure-the-gateway)
  - [Model configuration](#model-configuration)
  - [Delta coalescing](#delta-coalescing)
  - [Private exchange capture](#private-exchange-capture)
- [Start the gateway](#start-the-gateway)
- [Send a request](#send-a-request)
  - [Responses](#responses)
  - [Chat Completions](#chat-completions)
  - [Messages](#messages)
  - [Shared request rules](#shared-request-rules)
  - [Messages profile and cache markers](#messages-profile-and-cache-markers)
  - [Chat custom tools](#chat-custom-tools)
  - [Unsupported controls and delivery](#unsupported-controls-and-delivery)
  - [Image input](#image-input)
- [Stored Responses](#stored-responses)
- [Conversation and cache affinity](#conversation-and-cache-affinity)
- [HTTP API](#http-api)
  - [Capability discovery and adaptation notice](#capability-discovery-and-adaptation-notice)
- [Errors and limits](#errors-and-limits)

## Prepare credentials

The gateway, `morphiecore-auth` and `morphiecore-upstream` share a default store:

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

Callbacks listen on `127.0.0.1`. Login checks issuer, client and subject. Gateway inference never starts login or refresh.

## Manual upstream diagnostics

`morphiecore-upstream` uses the same [embedded Provider metadata](architecture.md#shared-provider-metadata) as the gateway. It does not read `models.json`, register models or change routes. Each invocation makes **one upstream operation request**, without retries, failover or redirects. OAuth preparation may make separate authority requests through the existing credential manager.

Build with `cargo build --locked --offline --bin morphiecore-upstream`, then use `target/debug/morphiecore-upstream` (or an installed binary) in these examples. Provider names are positional; the old `--provider`, `--origin`, `--path` and `--auth` options are removed.

### List upstream models

```sh
morphiecore-upstream models deepseek
morphiecore-upstream models google
morphiecore-upstream models openai
```

No protocol argument is required or accepted. The Provider's `models` endpoint supplies its path and authentication. The command prints the complete upstream JSON response, preserving all fields rather than extracting IDs or wrapping it in a report. Pagination fields remain untouched; no further page is fetched. There is no `--cursor` option. A complete response does not necessarily contain the entire Provider catalog or prove model entitlement.

If the Provider has no declared `models` endpoint, the command fails before credential access; it never guesses a path. Confirm the endpoint and update the repository metadata before rebuilding. xAI's list API is documented for API keys; OAuth entitlement remains unverified and may reject.

### Probe an exact upstream model

```sh
morphiecore-upstream probe deepseek \
  --model deepseek-flash --protocol openai-chat

morphiecore-upstream probe opencode-go \
  --model claude-haiku-5-5 --protocol anthropic

morphiecore-upstream probe google \
  --model gemini-3.8-flash --protocol google

morphiecore-upstream probe openai \
  --model gpt-6-astra --protocol openai-responses
```

The protocol choices are `openai-chat`, `openai-responses`, `anthropic` and `google` (generateContent). The operation must be declared by the Provider, but the model name is an ordinary upstream string: it need not appear in the local catalog or a previous model list. The fixed prompt is `Reply with the single word OK.`; no user payload, tools or history are accepted.

Optional token limits are omitted. Anthropic Messages alone requires `max_tokens`, so its request uses a fixed `1024`. SIWC requires `store:false` and `stream:true`; other probes use static JSON. SIWC consumes SSE through strict EOF, including closed event-owned output when its final summary is empty. Local deadlines and byte bounds do not guarantee bounded remote computation or billing.

Success requires nonempty text plus an ordinary completion (`stop`, `completed`, `end_turn` or `STOP`), not merely HTTP 200 or an exact `OK` answer. Truncation, failed/incomplete terminals and malformed responses fail. Reports contain status, elapsed time, byte counts and whether OAuth refreshed, not generated text, raw errors, credentials or private identities. This is a basic service observation, not gateway compatibility or capability certification.

### Credential selection and optional controls

Without an explicit alias, the tool loads the Provider's `credential_pool` from the project credential manager and selects **the first configured member only**. It ignores the pool's runtime fallback policy: a disabled, missing or unusable first member fails rather than selecting another. Missing pools must be created with `morphiecore-auth pool set`; the tool does not create pools, scan keys or choose an account by availability.

For example, `deepseek` defaults to `deepseek-api-key` and `openai` to `openai-oauth`. All default names are maintained in `providers.json`, not a second CLI table. The gateway uses these same binding defaults but retains its own routing/fallback policy.

| Option | Meaning |
|---|---|
| `--api-key ALIAS` | Bypass the default pool with one API-key alias in this Provider's domain; never a key value |
| `--account ALIAS` | Bypass the default pool with one OAuth account in the Provider's declared profile |
| `--store PATH` | Select another project credential store |
| `--proxy URL` | Override system/environment proxies for upstream HTTP and OAuth operations |
| `--timeout-seconds N` | Whole refresh/request deadline, default 60, range 1–300 |

The two credential overrides are mutually exclusive and must match the endpoint's credential kind/profile. They cannot change the destination or authentication scheme. To inspect a different API-key member without changing the pool:

```sh
morphiecore-upstream models deepseek --api-key secondary
```

OAuth pins the selected identity, refreshes locally expired access once or completes a pending renewal, and persists through the existing manager. There is no login, permission escalation, refresh after HTTP 401, account replacement or inference replay. Failed/interrupted refresh may require recovery with `morphiecore-auth`. Gateway inference still never refreshes credentials.

To add a Provider for manual diagnostics, edit `src/provider/providers.json` with its trusted origin, auth, default pool and confirmed endpoints, then rebuild. Provision its credentials/pool separately through `morphiecore-auth`. No gateway model binding is required for a probe. There is **no runtime Provider file or hot reload**. Existing gateway deployment overrides remain gateway configuration, not tool flags.

The HTTP response ceiling is 4 MiB; JSON keeps shared structural limits, and SIWC limits events to 1 MiB each/10,000 total. Responses are buffered; oversized responses fail rather than print a truncated result. Echoed authentication material is rejected before output. Like the gateway and auth CLI, upstream HTTP follows system/environment proxy settings by default; an explicit proxy overrides them without direct fallback. Ctrl-C cancels local work; socket release does not prove remote computation stopped.

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
| `diagnostics` | Private bounded diagnostics path, reusable after a clean restart; no bodies are recorded. See [Body-free diagnostics](#body-free-diagnostics) |
| `capture` | Optional private exchange capture; omitted or all-disabled selections record no content |

Paths in the service file resolve relative to that file, not the process working directory. CLI paths remain explicit process-relative or absolute paths.

### Body-free diagnostics

Diagnostics remain opt-in, separate from [private exchange capture](#private-exchange-capture). Use an existing trusted directory that other users cannot modify; mode `0700` is recommended on Unix, and created files use mode `0600`. No parent directories are created implicitly.

The configured path and `<path>.1` retain at most 1 MiB each. Rotation preserves complete JSONL records and discards only the oldest recognized archive. `<path>.lock` and `<path>.1.lock` are reserved zero-byte lock files; do not use either data slot as another active writer's base path. One writer holds both locks, including across rotation. Do not externally rotate, replace or edit these files while it runs.

Each line has `kind:"morphiecore_diagnostics"` and `version:1`. The `event` is `file` for an ownership marker, `request` for the existing metadata record, or `logging_status` for cumulative loss/failure/recovery counters. Readers counting requests must select `event:"request"`, not count all lines. Request `request_id` is a generated run nonce plus sequence; its candidate ordinals identify attempts, not user sessions or routing authority.

The 64-record queue never blocks request completion. Queue/oversize losses, failed writes and incomplete flushes are counted. Nonzero health is written to a healthy diagnostics file; when file output fails, the background worker issues rate-limited fixed stderr warnings without paths, I/O error text or record contents. Logging failures do not change client results. Flush is best effort with a two-second acknowledgement bound, not `fsync` durability or a promise that dropped records were written.

After the old process exits, reuse the same path. Valid records remain; a recognized incomplete final record is truncated and its lost byte count is reported. Unmarked, empty, aliased, oversized or unknown-version files are not adopted, including old-format diagnostics. Move those files aside or select a fresh path while stopped; an interrupted first ownership marker may also need this operator action. Persistent lock files need not be removed.

`first_upstream_bytes_ms` and `first_upstream_delta_ms` are request-relative monotonic timings. A decoded nonempty text/reasoning/tool-argument delta is not a client-receive event, token count or provider-only latency; earlier attempts can contribute. Client-visible first-content acceptance uses the separate [production-profile procedure](development.md#production-profile-acceptance).

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
| Candidate | `thinking_conversion` | `budget_to_effort` or `effort_to_budget` fallback; exact model-supported controls take precedence. Omission performs no numeric conversion |
| Candidate | `allowed_effort` | Optional nonempty array of distinct `minimal`, `low`, `medium`, `high`, `xhigh`, `max` levels; narrows the catalog declaration when present |
| Candidate | `maximum_output_tokens` | Optional positive `u64` model-output maximum, not a default; cannot exceed a catalog-declared maximum |
| Candidate | `default_output_tokens` | Messages only: omit to inherit the binding default, set a positive `u64` to replace it, or `null` to clear it and require explicit request caps |

Omitted model-value fields inherit the binding's declaration. These limits apply after target projection, including numeric/qualitative effort conversions and retained per-message updates; they do not rewrite conversion tables or grant missing protocol carriers. Values outside the effective target contract reject without clamping. See [model-effective values](architecture.md#model-effective-values).

The Go/Haiku Messages binding declares a `32000` default; generic Messages targets do not. `default_output_tokens` is independent of `maximum_output_tokens`: it may be raised or lowered, but a resolved default above the effective maximum rejects startup. For example, narrowing the maximum to `8192` requires overriding the inherited `32000` with a value at most `8192`, or clearing it with `null`. Zero, malformed values and this field on a non-Messages candidate (including `null`) reject. Explicit request caps are never replaced; an uncapped request with no effective default rejects locally. Defaults are neither a promise to generate that amount nor a model-limit guarantee. See [missing output ceilings](architecture.md#missing-output-ceilings).

Timeout values accept 1–86400000 milliseconds. Attempt ceilings span endpoint and credential candidates without multiplying or retrying an exhausted list. Beginning credential resolution counts as an attempt; skipped candidates do not. See [Advancement](architecture.md#advancement) for candidate ordering, failure restrictions and the publication boundary.

Cooling targets are never tried early. If every otherwise-legal target is cooling or probing, the gateway returns `503 upstream_unavailable` with `Retry-After`. [Health and recovery](architecture.md#health-and-recovery) defines cooldown scopes, delay bounds, foreground probes and cancellation.

All Generation client protocols use the same configured candidate array. To send Responses clients through Chat, select a Chat binding; there is no separate `responses_via_chat` switch. The old service-file `models`, `failover` and attempt fields are rejected, not automatically migrated.

For `claude-haiku-5-5`, selecting only `opencode-go-haiku-messages` candidates enables Messages owner-local `cache_control` markers, caller-selected 5m/1h automatic policy, ordered function `input_examples` and per-use document `context`. It also enables pi's built-in Haiku 5.5 profile: strict tools, mid-conversation system messages, per-message effort, block binding and tool changes. Each feature requires every selected candidate's native opt-in; including an OpenRouter candidate keeps this admission closed. No additional switch is needed, and credentials alone never activate a binding. Trusted deployment overrides retain the selected binding's contract and do not establish support for a different model. [Bounded acceptance](development.md#native-messages-input-acceptance) does not prove elapsed cache retention or invoices.

On an admitted Messages route, request-wide automatic caching uses root `"cache_control":{"type":"ephemeral","ttl":"1h"}` or `"ttl":"5m"`; omitting `ttl` means 5m. Missing root policy retains the binding's marker-free default, not a client disable instruction. Explicit owner markers, four-marker limits and TTL conflicts retain their [existing rules](architecture.md#caller-selected-automatic-cache-admission).

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

The table is consulted only when exact carriage is unavailable. For example, a supported reasoning-level `medium` request stays qualitative on Google even when `effort_to_budget` and a custom `medium:7000` preset are configured. That unused preset does not invalidate a caller's `6000` output ceiling. Conversely, an actual numeric conversion must still fit the caller ceiling and target limits; the gateway never raises the ceiling to make it fit.

`budget_to_effort` uses the target's declared effort scope: response-wide for Messages, reasoning-only for the other current protocols. `effort_to_budget` accepts reasoning-scoped effort and currently requires a generateContent binding. An existing OpenAI qualitative level calibration cannot be combined with a numeric conversion. Unsupported target levels, incompatible controls and over-ceiling budgets still reject; the table does not widen model admission or change dialects.

The existing Gemini 3.8 Flash bindings inherit the selected [response-effort scope approximation](architecture.md#google-response-effort-projection) without a new configuration field. Omitted `thinking_conversion` retains it; an explicit numeric conversion replaces it, never chains the two. Messages basic delivery additionally uses the separately selected [no-stop terminal profile](architecture.md#google-messages-terminal-projection). These adaptations do not promise equivalent Claude effort behavior or complete Google protocol support; further [Google API expansion is paused](roadmap.md#google-api-pause).

`reasoning_as_text` controls the explicit conversion of unattributed readable history reasoning into assistant text, not visibility of new reasoning output. With the default `false`, a candidate needing that conversion rejects. Separately, complete reasoning demonstrably belonging to another target is omitted under the named visible-history transfer policy, never promoted to an assistant answer. Visible messages and real call/results remain; partial reasoning, damaged compatible replay and required compacted/cached context cannot be repaired by omission. Valid native carriers remain available.

Only listed models are activated. Frozen Images, Speech and Embeddings entries use an empty object (`"configured-frozen-model": {}`); they retain their existing single catalog target and pool constraints, without Generation overrides.

Transcription also accepts an empty object when the model has exactly one catalog binding. Shared models require an explicit model-level `binding`, not a Generation `candidates` list:

```json
{
  "models": {
    "qwen-audio-3.0-asr-flash": {
      "binding": "dashscope-qwen-asr"
    }
  }
}
```

Select `dashscope-qwen-asr` for the metered DashScope pool or `tokenplan-qwen-asr` for the Token Plan pool. Both expose the same `qwen-audio-3.0-asr-flash` model; their trusted origins and credential domains remain separate. The former `qwen-audio-3.0-asr-flash-dashscope` alias is removed. Missing or mismatched selections reject at startup. Execution uses one binding and one credential source, without automatic Provider selection, failover or Generation overrides.

The file supports at most 64 models and 64 candidates per model; expanded credential candidates retain the existing runtime bound. Unknown bindings, mismatched model identities, duplicate deployments, missing pools and invalid bounds fail startup. See [service loading](../src/gateway/startup/bootstrap.rs) and [model assembly](../src/gateway/startup/models.rs).

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

Only compatible adjacent nonempty text deltas are merged. The first content batch is not intentionally delayed; a fixed deadline flushes pending text during upstream silence. The delay bounds intentional buffering, not client backpressure or end-to-end latency. The byte limit accounts for retained context and encoded text, not process RSS. `stream:false`, including a declared forced-SSE upstream, retains existing final-response materialization without a coalescing buffer.

See the [coalescing contract](architecture.md#optional-delta-coalescing) for merge eligibility, flush barriers, accounting, cancellation and EOF requirements. This service-only policy changes neither routing nor upstream traffic or token usage. Optional diagnostics report peak accounted pending bytes without content. Synthetic measurements do not establish SDK, real-client or Provider acceptance.

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

All three executables (`morphiecore`, `morphiecore-auth`, `morphiecore-upstream`) use the HTTP client's system/environment proxy defaults when no explicit proxy is selected. On Linux this includes `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`, `NO_PROXY` and their lowercase forms. Auth and upstream have no service configuration layer, so their precedence is CLI → system/environment. No proxy values or authentication are printed.

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

### Shared request rules

Use `"stream": true` for SSE. Send the full history each turn, or use [stored Responses](#stored-responses). Chat Completions and Messages remain full-history APIs.

Instruction content is plain text only: request-wide instructions, Messages `system`, instruction text in mid-conversation Messages system messages, and Chat/Responses system or developer messages cannot contain images, audio, video or files. Messages system messages may additionally carry the supported tool-change blocks and per-message effort described below. Media-bearing instructions reject at decoding rather than being dropped or moved to a user message. The same restriction applies to reported instruction echoes from an upstream.

Later system or developer instructions keep their position. Chat and Responses targets carry them natively; Messages uses mid-conversation system messages subject to native placement rules; Google targets reject them.

Return reasoning carriers unchanged in later history. Chat clients must preserve `reasoning_details`, not just readable text. See [Reasoning replay](architecture.md#reasoning-replay).

Responses assistant-history `output_text` may omit `annotations` under the [selected input shorthand](architecture.md#client-responses-history-shorthand). The gateway inserts an empty list with a named diagnostic receipt; explicit null or malformed annotations still reject, and supplied citations must be preserved. This does not relax upstream response validation or restore citations discarded by a client.

Chat accepts the deprecated `max_tokens` as the output cap, but not together with `max_completion_tokens`. An absent or null cap remains unspecified at Chat intake; bootstrap imposes no global default or token ceiling. Messages targets fill absence only from their configured `default_output_tokens`, under `messages.default-max-tokens.v1`; without one they reject. Native Messages intake still requires a positive `max_tokens`. Chat and Responses codecs carry `verbosity`; native Messages and Google projections use selected, recorded omission rules. A codec carrier does not establish model-effective support or binding admission. Chat `stop` and Messages `stop_sequences` decode into ordered stop controls, but unchanged default binding admission does not enable them merely because the codecs have carriers.

### End-user attribution

OpenAI Chat/Responses accept independent `safety_identifier` (up to 128 characters) and deprecated `user` (up to 256); native Messages accepts `metadata.user_id` (up to 512). Native OpenAI paths, including Chat↔Responses, retain both fields independently. They never automatically modernize `user` into safety/cache fields.

Cross-protocol targets with one attribution carrier prefer an explicit safety identifier, otherwise general user attribution. Messages uses `metadata.user_id`; OpenAI uses `safety_identifier`, not legacy `user`; generateContent uses only `labels.safety_identifier`. Google values allow at most 63 Unicode codepoints of lowercase/uncased letters, numeric characters, underscores and dashes. No generic label/metadata map or Interactions carrier is selected.

Explicit null blocks fallback, and a permitted empty string remains explicit. Google omits unrepresentable null with a named receipt rather than filling another ID. Invalid target values reject without truncation, lowercasing, hashing or substitution; for example, a 129-character native Messages ID cannot project to OpenAI's 128-character safety carrier.

`prompt_cache_key` remains independent. A projection can lose identity under `generation.omit-identity-hints.v1` and separately lose OpenAI legacy `user` cache bucketing under `cache.omit-legacy-user-bucketing.v1`; neither receipt includes identifier values. No identity is derived from credentials, prompts or conversation IDs. See the [attribution contract](architecture.md#end-user-attribution).

### Messages profile and cache markers

On an admitted native Messages configuration, tool definitions, system blocks and supported conversation blocks retain their own cache markers, including a `tool_result` wrapper. `{"type":"ephemeral"}` and explicit `ttl:"5m"` mean five minutes; `ttl:"1h"` means one hour. At most four explicit breakpoints are allowed, ordered with longer TTLs before shorter ones. The gateway does not move or drop markers, shorten TTLs or add a request-wide policy alongside them. Marker-free requests retain the existing automatic five-minute default. Caller-supplied top-level `cache_control` remains rejected on this binding, independently of owner-local markers. Cache hits and deployed proxy TTL behavior are not certified by admission.

The generic Messages profile keeps ordered-instruction and tool-change admission closed, while the [all-native configured Go/Haiku route](#model-configuration) enables the reviewed profile. Messages system messages can contain text and `tool_addition`/`tool_removal` blocks and carry per-message `output_config.effort`; a non-null effort on another role rejects. An effort-only system message may have empty content. Adaptive thinking can carry `thinking.block_binding`. Missing target carriers use the selected [named projections](architecture.md#request-rules); explicit block-binding `"error"` has no fallback.

### Chat custom tools

For custom-capable targets, Chat JSON uses `{"type":"custom","custom":{"name":"sql","format":{"type":"text"}}}` definitions and `{"type":"custom","custom":{"name":"sql"}}` choices. Calls retain raw `custom.input`; return their ID through an ordinary `role:"tool"` message. Preserve the preceding typed call in history so the result cannot be mistaken for a function result. Grammar definitions use the nested carrier described in [protocol sources](refs/2026-10-16-chat-custom-tools.md).

The pinned Chat SSE contract has no custom-call delta carrier. Chat upstream streaming that enables custom selection and downstream custom-call deltas reject explicitly; use an admitted Responses streaming path instead. Custom history and declarations remain usable for a later stream when tool choice excludes custom output. Codec support does not activate models or execute tools.

### Unsupported controls and delivery

Non-null Messages `clear_at`, Chat message `name` and Chat `store: true` remain rejected.

Active Chat `functions`, `function_call` and `web_search_options`, and Messages `container`, `mcp_servers`, `context_management` and `compaction` remain unsupported and rejected. Other unrecognized fields are ignored by name. Messages `thinking.display` is preserved; Messages `top_k` decodes, but the native Messages target rejects it. Messages responses with citations or missing/ambiguous final usage fail projection rather than dropping annotations or inventing zero counts; a failure after streaming starts aborts delivery without a success terminal.

Chat `moderation` remains a known unsupported semantic and rejects whenever present, before unknown-field filtering. Messages tool `input_examples` preserves ordered input objects in root and by-value definitions on admitted native routes; null lists, non-object entries and unsupported target carriers reject. Local checks bound structure/resources, not schema-instance validity. Document `context` preserves per-use absent/null/empty/text intent without prompt concatenation. Messages delivery requires terminal evidence: a finished turn plus a natural/matched cause, completed pending tool calls with no completion cause, or a supported incomplete reason. Pending calls can carry `tool_use` even when source disposition is unreported; this does not synthesize a source progress report. Unknown completed terminals without pending calls and explicit continuation reject in JSON and SSE; a completed Responses response alone does not establish a Messages `end_turn`.

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

Formats, encoded sizes and counts are target constraints, not a promise that every IR image fits every upstream. Preserve explicit per-use `detail` through an exact carrier; omission requires proven target-default equivalence, and other approximations require an evidence-backed selected rule. Without one, an unrepresentable detail rejects rather than disappearing. The example's explicit `auto` is not equivalent to absence and is not universally portable to native targets. Any approved projection changes only the candidate copy, not the source request. See [Media](architecture.md#media), [IR resource budgets](../src/semantic/task/generation/resources/mod.rs) and [Messages media limits](../src/protocol/anthropic/messages/media.rs).

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

## Conversation and cache affinity

Use `X-MorphieCore-Conversation-Id` for one logical conversation. Keep it stable across turns and model changes. Use a new ID for a new conversation.

The ID must be 1–256 visible ASCII characters, without commas, in one header. If it is absent, ingress checks these agent headers in order:

1. opencode: `X-Session-Id`
2. pi: `session_id`
3. Codex: `session-id`
4. pi Chat/Messages: `x-session-affinity`

For pi's ordinary API-key Chat and Messages integrations, enable `compat.sendSessionAffinityHeaders: true`; automatic session headers also require `cacheRetention` other than `none`. Responses normally sends `session_id` without that compatibility flag. Installed pi 1.3.0 transport carrier checks corroborate the pinned-source behavior; they do not establish full-client or Provider acceptance. Existing header precedence is unchanged; an invalid selected header rejects rather than falling through. `x-client-request-id` is not a conversation source.

This ID groups routing and cache affinity; it does not store history. Body `session_id` is rejected. Standard `prompt_cache_key` is a separate cache-grouping intent. Without a declared conversation, the optional prefix index offers best-effort cache reuse, not executor affinity or replay permission.

## HTTP API

| Method and path | Purpose |
|---|---|
| `GET /v1/models` | List activated public models |
| `GET /v1/models/{model}` | Retrieve an activated public model |
| `DELETE /v1/models/{model}` | Denied: 403 for an activated model, 404 otherwise |
| `GET /v1/morphiecore/capabilities` | Inspect partial gateway contracts for activated Generation and Embedding clients |
| `POST /v1/responses` | Responses, JSON or SSE |
| `GET /v1/responses/{response_id}` | Retrieve a stored response |
| `DELETE /v1/responses/{response_id}` | Delete a stored response |
| `GET /v1/responses/{response_id}/input_items` | Cursor-paginated resolved input |
| `POST /v1/chat/completions` | Chat Completions, JSON or SSE |
| `POST /v1/messages` | Messages, JSON or SSE, using the model's configured candidates |

Standalone endpoints require explicit model activation:

| Method and path | Purpose |
|---|---|
| `POST /v1/embeddings` | Text or model-scoped token IDs, float or Base64 vectors |
| `POST /v1/images/generations` | Static inline images |
| `POST /v1/audio/speech` | Binary audio |
| `POST /v1/audio/transcriptions` | One WAV/MP3 multipart upload |

The [OpenAPI summary](openapi.json) describes envelopes and selected constraints, not full vendor API conformance. Codecs define exact admission. Business input cannot choose an upstream origin, credential or auth header; inbound headers are not forwarded.

### Embeddings

Explicitly list `"gemini-embedding-2": {}` or `"text-embedding-3-small": {}` in the models configuration and bind the corresponding Google or OpenRouter single-member, nonfallback API-key pool. This does not activate either model automatically or change credentials. OpenAI's model currently uses OpenRouter; no direct OpenAI API binding is implied.

Send the ordinary OpenAI request envelope to `/v1/embeddings`, for example `{"model":"gemini-embedding-2","input":["first","second"],"dimensions":128,"encoding_format":"base64"}`. Google uses native synchronous requests internally, with explicit no-truncation. The downstream path is not Google's native REST interface.

`text-embedding-3-small` also accepts one token sequence or a batch of sequences under the declared ordinary `cl100k_base` vocabulary. Use IDs from that tokenizer only; special/control tokens, unknown IDs, mixed text/token batches and token inputs to Google/DashScope reject before dispatch. There is no detokenization or automatic splitting.

Compatible profiles preserve valid vectors even when ancillary reports are absent, while returning the complete OpenAI response shape including both usage counters. A missing model report uses the bound public label. Real usage takes precedence, followed by attested input-only derivation; remaining missing counters use the request-local [approximation rule](architecture.md#google-embedding-compatibility). No default all-zero usage or custom usage fields are emitted. Reported zeros are preserved. Approximate counts can be inaccurate across languages and models; do not treat them as exact billing or hard quota measurements. Unsupported `user` hints are omitted on Google/DashScope and forwarded on the current OpenAI-model binding. These adaptations do not claim upstream reporting or attribution.

The pinned OpenAI Python SDK's strict response model still types `embedding` as a float list, so use explicit `encoding_format: "float"` with `_strict_response_validation=True`. Its ordinary mode handles Base64 and SDK-default decoding. This SDK type limitation is separate from complete usage and does not authorize changing a requested encoding; the wire schema supports both representations.

### Capability discovery and adaptation notice

**Migration status:** the following describes the implemented partial snapshot and possible-adaptation header. The accepted [actual-loss reporting contract](architecture.md#public-capability-and-adaptation-notice) replaces internal-only receipts during the standards redesign; no new response extension is implemented by the dependency reset. Legacy manifest-based phase claims below are not official protocol conformance.

`GET /v1/morphiecore/capabilities` uses the ordinary ingress token. It returns a startup-immutable `version: 1` snapshot with `scope: "gateway_contract"` and `coverage: "partial"`. Each entry names an activated public model and client protocol, with separate `request`, `delivery` and `history` case lists. Other standalone media tasks are not assessed. No upstream request or request-body collection occurs; HEAD omits the body. The complete snapshot must fit both `response_bytes` and 256 KiB or startup fails.

Embedding entries use `protocol: "embeddings"` and JSON-only delivery. The descriptor declares input kinds, optional tokenizer/token limits, dimension/default/count budgets, native and client encodings, and `usage_may_be_estimated`. Limits intersect with runtime/body ceilings rather than promising every maximum-sized combination fits. Omitted dimensions use trusted defaults without adding a wire control. Raw HTTP encoding omission means float; the Python SDK normally sends Base64 on caller omission. The [numeric policy](architecture.md#embedding-encoding-compatibility) preserves source precision and permits named binary32 rounding only on delivery copies. Request and delivery rule lists describe compatible projections without exposing credentials or source payloads.

Each case names a `feature`, a limited `domain`, and `faithful`, `approved_adaptation` or `rejected` treatment. Approved adaptations include stable `rules` IDs. Unlisted cases are `not_assessed`, not implicitly supported or rejected. Statements concern that feature/domain only, assuming all other validation succeeds; neither model-effective settings nor full client loops are asserted.

Generation delivery declares `response.delivery.usage` with `generation.complete-delivery-usage.v1`: reported counts take priority, followed by accounting derivation and a text-only byte-based estimate. Usage can therefore be approximate and must not be treated as billing or quota truth, especially for media-bearing input. Responses additionally declares `responses.infer-required-settings.v1` for missing mandatory tool-setting echoes. See [the fallback rules and limitations](architecture.md#downstream-schema-completion); no nonstandard fields are inserted into SDK response objects.

The first request slice lists explicit public-admission rejection of safety policy, cached-context references and normalization-default tools where applicable; absence does not establish support. Delivery lists selected incidental-settings omissions, assessment-only safety-report omission, protected blocking-safety rejection and completion-cause carriers equally for JSON/SSE. Natural or sequence-matched completion differs from ambiguous stop, and these cases exclude pending tool continuation. Chat lists `chat.coarsen-completion-cause.v1` and Responses `responses.omit-completion-cause.v1` where a cause has no exact carrier. Responses history preserves a successfully delivered stored projection within its TTL, not facts omitted before delivery or a promise of compatible replay to every target.

Phase cases describe exact metadata carriers: native Responses assistant labels, or Chat/Messages text phases inside the bounded manifest. The history cases require returned native labels or an unchanged matching manifest. They do not promise phase-aware UI, client retention of opaque fields, or successful conversion of other content/terminals. `generation.return-message-phase.v1` records transport through a carrier, not permission to discard phase. Request-side phase transfer and cache-retention policies remain unassessed by this partial route-wide view.

Messages request cases distinguish `generation.cache` domain `valid_owner_local_breakpoints_5m_or_1h` from `request_wide_automatic_policy`. The former is faithful only under the selected all-native admission contract; mixed configurations publish rejection. These declarations do not promise cache creation/hits, billed TTLs, storage retention or signed-thinking replay.

Selected Google Messages entries additionally declare `generate_content_stop_without_configured_sequences` or `interactions_completed_without_configured_sequences` with their terminal rules. The general ambiguity rejection is scoped outside that selected no-stop profile. These cases still require valid outcome/progress/call shape, representable usage and successful EOF; they do not admit configured stops, unknown failure reasons or unsupported reports.

Generation JSON/SSE and successful Embedding responses carry `X-MorphieCore-Adaptation: possible` and `Link: </v1/morphiecore/capabilities>; rel="describedby"`. This is deliberately conservative, including when no adaptation occurs and when Responses returns a pre-frame failure stream. It is not an actual-loss receipt, a success assertion or proof of Provider-effective behavior. Clients may not display it. Pre-response auth/admission errors, discovery, history operations and other standalone tasks do not receive the notice; missing headers do not prove exactness. The notice adds no response-body fields or SSE events; the existing replay/phase carriers have their own [contract](architecture.md#reasoning-replay). Actual receipts remain internal diagnostics.

## Errors and limits

OpenAI routes return `{"error":{"message","type","param","code"}}`. Messages errors, including authentication failures before body polling, use Anthropic's `{"type":"error","error":{"type","message"}}` envelope. Unclassified Messages errors put the diagnostic `code` in the message, for example `The request could not be completed (upstream_error).` Messages 401 errors use `authentication_error`.

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

Body-free diagnostics distinguish known Messages codec projection failures as `projection_failure: "terminal"` or `"usage"`, including nested adapter/SSE errors. Unclassified errors remain `"codec"`; arbitrary error strings, token values and payloads are not included. Classification changes no client error envelope or completion behavior.

Resource defaults and bounds are in [Limits](../src/gateway/startup/config.rs). Configuration fields above do not expose every embedded-library limit.
