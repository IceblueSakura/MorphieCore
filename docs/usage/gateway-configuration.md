# Gateway Configuration and Deployment

This guide covers gateway configuration templates, Agent API keys, model definitions, failure diagnostics, delta coalescing, daemon startup, and reverse-proxy deployment with Nginx.

See [Credentials and Diagnostics](credentials-and-diagnostics.md) for auth setup, [Client Requests](client-requests.md) for client protocols, and [Architecture Runtime](../architecture/runtime-and-routing.md) for failover mechanics.

## Contents

- [Configure the gateway](#configure-the-gateway)
  - [Configuration templates](#configuration-templates)
  - [Agent API keys](#agent-api-keys)
  - [Failure diagnostics](#failure-diagnostics)
  - [Model configuration](#model-configuration)
  - [Delta coalescing](#delta-coalescing)
    - [Choosing a window](#choosing-a-window)
  - [Private failure context](#private-failure-context)
- [Start the gateway](#start-the-gateway)
  - [Binary-only SSH releases](#binary-only-ssh-releases)
- [Reverse-proxy deployment](#reverse-proxy-deployment)

## Configure the gateway

Create private `gateway.json`, `auth.json`, `accounts.json` and `models.json` files. The default application directory is `$HOME/.local/share/morphiecore` (`%USERPROFILE%` on Windows); use `--config` for a fresh explicit setup. Parent directories must be private on Unix: directory mode `0700`, file mode `0600`. Keep configuration outside credential storage. Each file is bounded to 64 KiB and rejects unsupported versions, unknown fields and duplicate JSON keys. Changes take effect at startup; there is no hot reload.

### Configuration templates

The repository's [`config/`](../../config/) directory contains public templates, not live configuration:

| Template | Purpose |
|---|---|
| [`gateway.example.json`](../../config/gateway.example.json) | All current service fields, default 50 ms / 16 KiB coalescing, disabled failure diagnostics |
| [`auth.example.json`](../../config/auth.example.json) | Two independent Agent identities with deliberately too-short key placeholders; the unchanged file cannot start a gateway |
| [`accounts.example.json`](../../config/accounts.example.json) | Ordered named API-key accounts referencing protected credential aliases, independently of model activation |
| [`models.example.json`](../../config/models.example.json) | Narrow starting point: Qwen 3.8 Flash through the Token Plan Chat binding with standard Chat delivery |
| [`models.advanced.example.json`](../../config/models.advanced.example.json) | Optional Generation controls: Messages/Google bindings, output limits, inherited-default clearing and both numeric thinking conversions |
| [`models.tasks.example.json`](../../config/models.tasks.example.json) | Optional existing Embedding, Images, Speech and explicit Transcription binding selection, without Generation overrides |
| [`pi-deepseek.example.json`](../../config/pi-deepseek.example.json) | Optional pi client model declaration for DeepSeek through the gateway; merge into pi's `models.json`, not gateway model configuration. See [Chat Completions](client-requests.md#chat-completions) |
| [`nginx.example.conf`](../../config/nginx.example.conf) | Optional TLS virtual host, gzip SSE, proxy safety controls and example pre-authentication connection/upload limits; include from `http {}` |

Choose one model template as `models.json`, or deliberately combine selected entries under one `models` object. The loader does not merge files, expand placeholders or discover templates. The advanced Messages candidate clears its output default, so uncapped requests require an explicit cap. Account targets remain independently configured.

For a **new** Unix installation, run from the repository root. This refuses to overwrite existing files or symlinks:

```sh
(
    set -eu
    config="$ROOT"
    for name in gateway auth accounts models; do
        if [ -e "$config/$name.json" ] || [ -L "$config/$name.json" ]; then
            printf 'Refusing to overwrite an existing configuration.\n' >&2
            exit 1
        fi
    done
    install -d -m 0700 "$config"
    for name in gateway auth accounts models; do
        install -m 0600 "config/$name.example.json" "$config/$name.json"
    done
)
```

Replace **each** Agent-key placeholder with an independently generated high-entropy key of at least 32 printable non-space ASCII characters; keep values out of source and shell output. Provision every referenced account with `--accounts "$ROOT/accounts.json" --store "$ROOT/credentials"`. The shipped account template declares `primary` for `aliyun-tokenplan-cn`, `opencode-go`, `google` and `openrouter`; each references its Provider-local `example` alias. Remove unused Provider declarations before validation if those accounts will not be provisioned. Task models explicitly select `primary`. Templates are configuration examples, not live acceptance evidence.

`proxy: null` leaves the documented environment-proxy fallback in effect; it does not force direct connections. To enable failure diagnostics, replace `null` with the trusted destination object below; private body context remains off unless explicitly selected. Upstream credential stores are managed formats, not hand-edited templates. Provider metadata remains compiled into the program; there is no runtime `providers.json` template.

Keep edited copies, credentials, certificates and captures outside the repository. Git ignores non-template files under `config/` as a secondary guard, not as permission to put secrets in public `*.example.*` files.

`gateway.json` owns service settings, not key values:

```json
{
  "version": 1,
  "auth_file": "auth.json",
  "bind": "127.0.0.1:8080",
  "accounts_file": "accounts.json",
  "models_file": "models.json",
  "credentials_dir": "credentials"
}
```

| Field | Meaning |
|---|---|
| `version` | Required current schema version, `1` |
| `auth_file` | Optional path, default `auth.json` beside this file; the resolved private Agent-key file must exist and validate |
| `bind` | Loopback listener; default `127.0.0.1:8080` |
| `models_file` | Optional path, default `models.json` beside this file; the resolved model configuration must exist and validate |
| `accounts_file` | Optional path, default `accounts.json` beside this file; the resolved account configuration must exist and validate |
| `credentials_dir` | Optional credential-store path; omission retains the independent home-relative default |
| `proxy` | Outbound proxy |
| `routing` | Global affinity, pre-dispatch wait and downstream retry-hint policy |
| `oauth` | Global credential-maintenance timing |
| `delivery` | Total request deadline and text batching |
| `resources` | Concurrent exchanges, body budgets and Responses history retention |
| `diagnostics` | Optional trusted OTLP Logs destination and bounded private-body policy; see [Failure diagnostics](#failure-diagnostics) |

Paths in the service file resolve relative to that file, not the process working directory. CLI paths remain explicit process-relative or absolute paths.

The configuration uses **version 1**. Policy fields belong to the following objects. Omit a group or field to use its default. Groups and numeric/boolean fields reject `null`; `delivery.delta_coalescing:null` explicitly disables batching. Optional paths, proxy and diagnostics retain their nullable form.

| Group | Fields and defaults | Bounds / effect |
|---|---|---|
| `routing` | `implicit_cache_affinity: true`, `affinity_ttl_ms: 3600000`, `affinity_capacity: 4096`, `max_wait_ms: 10000`, `retry_hint_horizon_ms: 60000` | Timing choices remain operator-owned. Zero TTL/capacity retains no bindings; zero wait disables waiting. The hint horizon limits credible **downstream** `Retry-After` values |
| `oauth` | `scan_interval_ms: 15000`, `refresh_ahead_seconds: 120`, `renewal_timeout_ms: 90000`, `failed_renewal_delay_ms: 60000`, `minimum_renewal_interval_ms: 60000` | Zero scan interval keeps the startup check only; zero renewal timeout disables renewal |
| `delivery` | `exchange_timeout_ms: 3600000` | 1–86400000 ms; begins **before body collection and admission**, covers execution/delivery, and bounds model-local deadlines |
| `delivery` | `delta_coalescing: {"max_delay_ms":50,"max_pending_bytes":16384}` | Delay 1–100 ms, pending bytes 1024–1048576; see [batching](#delta-coalescing) |
| `resources` | `concurrency: 16` | 1–256; body collection occupies a permit |
| `resources` | `request_bytes: 33554432`, `response_bytes: 67108864` | Each 1–67108864 bytes; intersect with endpoint/task limits. A budget too small to publish discovery rejects startup |
| `resources` | `body_timeout_ms: 60000` | 1–60000 ms; additional upload ceiling within the total request budget; whichever deadline expires first ends collection |
| `resources` | `history_entries: 1024`, `history_bytes: 268435456`, `history_ttl_ms: 1800000` | Entries 1–4096, bytes 1–1073741824, TTL 1–86400000 ms. TTL controls expiration; exhausted entry/byte budgets reject additional storage instead of evicting retained responses. No history-disable setting |

These are deployment budgets, not new protocol capabilities. Transcription retains its independent multipart upload ceiling; image/audio/event and internal queue ceilings remain code-owned. Compiled Generation attempts retain a one-hour ceiling and standalone task operations retain their own ceilings (currently 120 seconds). OAuth HTTP operations retain a 30-second ceiling and a 10-second connection timeout. Raising an outer deadline does not enlarge these inner ceilings.

Responses storage reserves capacity before final publication. A full history budget can therefore fail a stored response after upstream generation, rather than silently dropping history or claiming a successful stored result. Size these budgets for the intended stored-response workload.

Inspect available bindings without configuration or credentials, then validate or explain an explicit setup without starting a listener, renewal, diagnostics:

```sh
morphiecore --catalog
morphiecore --catalog --json
morphiecore --config "$ROOT/gateway.json" --validate
morphiecore --config "$ROOT/gateway.json" --explain --json
```

Explanation uses the production loader and requires valid local credentials. It reports resolved service defaults, Provider recovery, ordered account selections (including disabled candidates), inherited/configured output defaults, resolved task bindings, endpoint limits and destination-free diagnostics state. It never prints keys, credential aliases, private paths, custom upstream model strings or target URLs. Context/input capacities are planning metadata; output maxima clamp a present ceiling and do not impose a universal token/spending budget. Only Messages defaults fill absent output ceilings. Errors identify the configuration domain and safe field/index location without echoing rejected values; numbered Provider/model maps follow sorted key order.

### Agent API keys

`auth.json` has one versioned schema. Start from [`auth.example.json`](../../config/auth.example.json) and replace its intentionally invalid key placeholders; never deploy shared example keys.

| Field | Contract |
|---|---|
| `version` | Required integer `1`; unsupported versions reject |
| `agents` | Required array of 1–256 records |
| `agent_id` | Required unique, case-sensitive identity: 1–128 ASCII bytes, starts with a letter/digit, then letters/digits or `.`, `_`, `-` |
| `key` | Required globally unique secret: 32–4096 printable non-space ASCII bytes |
| `name` | Optional mutable display label; defaults to empty, at most 256 UTF-8 bytes, no control characters |
| `desc` | Optional maintenance description; defaults to empty, at most 2048 UTF-8 bytes |

Records reject unknown fields; supplied fields must have their declared types, not null. Both duplicate IDs and duplicate keys reject startup. The file must exist and pass the same private-file checks as other configuration; there is no environment-key or anonymous fallback.

Use the Agent's `key` in the SDK. Authentication determines `agent_id`; request headers, metadata and business content cannot choose it. Names and descriptions are private maintenance metadata, not authentication or public discovery fields. The runtime retains key digests, not plaintext keys; ordinary inference diagnostics record only the authenticated ID.

All configured Agents share the current model and stored-response access policy, including deletion; this is not a per-Agent ACL or quota system. Keep `agent_id` unchanged when renaming an Agent or replacing its key. Remove a record to revoke access, then restart: running gateways retain their loaded keys and in-flight requests are not forcibly cancelled. There is no persistent identity registry or deletion tombstone; operators must not reuse a retired ID for a different Agent. OAuth, remote registration and credential-management APIs remain deferred.

### Failure diagnostics

Diagnostics are opt-in. Set `diagnostics` to an explicit trusted OTLP/HTTP Logs destination:

```json
{
  "diagnostics": {
    "endpoint": "http://127.0.0.1:4318/v1/logs",
    "private_bodies": false,
    "body_bytes": 16384
  }
}
```

| Field | Contract |
|---|---|
| `endpoint` | Complete HTTP/HTTPS Logs destination URL with a host; no userinfo, query or fragment. The usual Collector path is `/v1/logs`; the application does not require that path |
| `private_bodies` | Default `false`; explicit opt-in for [private failure context](#private-failure-context) |
| `body_bytes` | Shared retained-body allowance in bytes; default `16384`, accepted `1024–65536` |

Omit `diagnostics` or set it to `null` to disable the pipeline. Export disables proxy inheritance and redirects; ambient `OTEL_EXPORTER_OTLP_HEADERS` and `OTEL_EXPORTER_OTLP_LOGS_HEADERS` are rejected rather than used as another authentication path. Use an existing trusted local Collector if backend authentication or persistent queuing is needed. This configuration does not deploy a receiver.

Event names are `gateway.failure`, optional correlated `gateway.failure.response`, and background `gateway.credential.failure`. Query severity WARN/ERROR, then `request_id`, `agent_id`, `operation`, `provider`, `account`, `model`, `upstream_model`, `attempt`, `stage`, `error.type` and `retry.planned` when present. IDs are admission-generated runtime nonces/sequences, never caller identity/session/trace headers. `http.response.status_code` is the observed upstream head; `gateway.error.status_code` is the selected local error presentation. `client.handoff_complete:true` describes acknowledged local delivery of a failed task, not remote receipt or successful inference.

Normal successful requests have no archive entry. A failed attempt selected for recovery is WARN, not proof of recovery. Source failures inside HTTP 200 remain failures; ordinary disconnect/shutdown alone does not create an ERROR. Closed categories and source boundaries remain useful without bodies. [Architecture](../architecture/runtime-and-routing.md#observability) owns classification.

The SDK queue holds 64 records, batches at most four and schedules a 100 ms flush. HTTP export has a two-second timeout, no retries and a 1 MiB request bound. Saturation/export/flush failures issue fixed body-free stderr warnings; inference does not wait or change behavior. Shutdown flush is best effort; last-owner drop does not wait for export. The receiver owns retention, access and durability. No local files, rotation, tail recovery, metrics pipeline or mandatory trace exporter are configured.

### Model configuration

`models.json` owns explicit public-model selection and ordered upstream candidates. Start with [`models.example.json`](../../config/models.example.json); the separate [advanced](../../config/models.advanced.example.json) and [standalone-task](../../config/models.tasks.example.json) templates demonstrate optional configuration shapes without expanding the default activation.

Use `morphiecore --catalog` to select a registered public label and its catalog endpoint IDs. Each candidate expands that Provider's enabled accounts in declaration order; account names, targets and credential references belong only in [accounts.json](credentials-and-diagnostics.md#configure-accounts). Binding definitions are in [the catalog](../../src/topology/catalog.rs). Omitted model controls inherit the binding's values.

`deepseek-v4.1-flash-1017` is the public DeepSeek ID. Native upstream requests use `deepseek-flash`; DashScope uses its exact dated upstream name. Select Provider `deepseek` with `deepseek-chat` or `deepseek-responses`: their default paths are `/beta/chat/completions` and `/beta/responses`, using the same account domain. Non-strict requests stay non-strict. A complete account API-root override supplies its own prefix; use `https://api.deepseek.com/beta` to retain the default beta paths. The public label does not pin the native service's moving alias to an immutable revision.

| Scope | Field | Meaning |
|---|---|---|
| File | `version` | Required current schema version, `1` |
| Model | `candidates` | Nonempty ordered array of existing bindings for this public model |
| Model | `total_timeout_ms` | Optional model-local total deadline from the same request start, bounded by the service ceiling; route resolution cannot restart the clock |
| Standalone task | `account` | Required enabled API-key account name in the selected binding's Provider |
| Model | `thinking_tokens` | Optional six-level numeric table shared by this model's candidates |
| Candidate | `binding` | Required catalog endpoint ID; fixes protocol, dialect, canonical model and authentication kind |
| Candidate | `upstream_model` | Wire spelling only; does not change canonical identity or model capabilities |
| Candidate | `reasoning_as_text` | Permit the existing named history conversion; default `false` |
| Candidate | `thinking_conversion` | `budget_to_effort` or `effort_to_budget` fallback; exact model-supported controls take precedence. Omission performs no numeric conversion |
| Candidate | `allowed_effort` | Optional nonempty array of distinct `minimal`, `low`, `medium`, `high`, `xhigh`, `max` levels; narrows the catalog declaration when present |
| Candidate | `context_window_tokens` | Optional positive `u64` total-context planning capacity; narrows a known declaration or explicitly supplies an unknown one |
| Candidate | `maximum_input_tokens` | Optional positive `u64` independent input planning maximum; never inferred from context/output and cannot widen a known declaration |
| Candidate | `maximum_output_tokens` | Optional positive `u64` model-output maximum, not a default; cannot exceed a catalog-declared maximum |
| Candidate | `default_output_tokens` | Messages only: omit to inherit the binding default, set a positive `u64` to replace it, or `null` to clear it and require explicit request caps |

Omitted model-value fields inherit the binding's declaration. Effort/output limits apply during candidate projection, including numeric/qualitative effort conversions and retained per-message updates. Output ceilings above a known maximum clamp to it; unsupported effort levels resolve to the nearest supported level, ties upward, with named loss receipts. These rules do not grant missing protocol carriers or weaken explicit constraints. Context/input capacities are discovery planning metadata only, not local token counting or a new inference check. Replacing the binding's origin, endpoint path or upstream model clears inherited context/input evidence; explicit declarations can supply the new deployment's facts. See [model-effective values](../architecture/adaptation-and-dialects.md#model-effective-values) and [discovery ownership](../architecture/adaptation-and-dialects.md#generation-text-model-discovery).

The Go/Haiku Messages binding declares a `32000` default; generic Messages targets do not. `default_output_tokens` is independent of `maximum_output_tokens`: it may be raised or lowered, but a resolved default above the effective maximum rejects startup. For example, narrowing the maximum to `8192` requires overriding the inherited `32000` with a value at most `8192`, or clearing it with `null`. Zero, malformed values and this field on a non-Messages candidate (including `null`) reject. Defaults fill absence only; explicit request caps still take the model-maximum clamp above. An uncapped Messages candidate with no effective default rejects locally. Defaults are neither a promise to generate that amount nor a model-limit guarantee. See [missing output ceilings](../architecture/adaptation-and-dialects.md#missing-output-ceilings).

Account selection happens before dispatch. Generation may retry one explicitly eligible unpublished network/server failure on that exact target, with the same credential material and wire request. Rate/quota/auth/permission/request failures, malformed output, trust/local faults and ambiguous post-dispatch timeouts do not authorize the additional send. A returned failure never causes another account or Provider to be dispatched in that request.

Cooling targets are never tried early. If every otherwise-legal target is cooling, the gateway returns 429 for rate/quota-only blocking and 503 otherwise. It supplies `Retry-After` only for a credible positive delay within `routing.retry_hint_horizon_ms`. [Health and recovery](../architecture/runtime-and-routing.md#health-and-recovery) defines cooldown scopes, delay bounds and cancellation.

All Generation client protocols use the same configured candidate array. To send Responses clients through Chat, select a Chat binding.

For `claude-haiku-5-5`, `opencode-go-haiku-messages` candidates natively carry Messages owner-local `cache_control` markers, caller-selected 5m/1h automatic policy, ordered function `input_examples` and per-use document `context`. Mixed routes no longer close public cache admission: each candidate preserves supported carriers or applies its selected named projection. Explicit strictness, media and output guarantees still require a carrier. The native profile also has ordered effort, block-binding and tool-change carriers; public intake restrictions remain listed [below](client-requests.md#messages-profile-and-cache-markers). Credentials alone never activate a binding. Trusted deployment overrides retain the selected binding's contract and do not establish support for a different model. [Bounded acceptance](../development.md#overall-acceptance-status) does not prove elapsed cache retention or invoices.

On an admitted Messages route, request-wide automatic caching uses root `"cache_control":{"type":"ephemeral","ttl":"1h"}` or `"ttl":"5m"`; omitting `ttl` means 5m. Missing root policy retains the binding's marker-free default, not a client disable instruction. Explicit owner markers, four-marker limits and TTL conflicts retain their [existing rules](../architecture/adaptation-and-dialects.md#caller-selected-automatic-cache-admission).

Account-level API roots apply consistently to Generation, model-list diagnostics and standalone task operations. Google generateContent resolves its model/action suffix from the selected operation; no caller-controlled path or API-version guessing is introduced. See [target composition](credentials-and-diagnostics.md#configure-accounts).

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

Place this object on the public model, alongside `candidates`. At least one candidate must select a conversion when a custom table is supplied. A conversion without a custom table uses the [shared preset](../architecture/adaptation-and-dialects.md#thinking-budget-quantization). Duplicate token values are allowed; nearest-budget matching and highest-level ties retain the existing algorithm.

The table is consulted only when exact carriage is unavailable. For example, a supported reasoning-level `medium` request stays qualitative on Google even when `effort_to_budget` and a custom `medium:7000` preset are configured. That unused preset does not invalidate a caller's `6000` output ceiling. Conversely, an actual numeric conversion must still fit the caller ceiling and target limits; the gateway never raises the ceiling to make it fit.

`budget_to_effort` uses the target's declared effort scope: response-wide for Messages, reasoning-only for the other current protocols. `effort_to_budget` accepts reasoning-scoped effort and currently requires a generateContent binding. An existing OpenAI qualitative level calibration cannot be combined with a numeric conversion. Candidate projection preserves explicit effort over a conflicting numeric budget, omitting an uncarriable budget with its named receipt; it quantizes a budget only when explicit effort is absent. Nearest-level projection and model-maximum clamping remain bounded; malformed source controls and explicit guarantees retain validation.

The existing Gemini 3.8 Flash bindings inherit the selected [response-effort scope approximation](../architecture/adaptation-and-dialects.md#google-response-effort-projection) without a new configuration field. Omitted `thinking_conversion` retains it; an explicit numeric conversion replaces it, never chains the two. Messages delivery uses [request-grounded end-turn derivation and the separately selected Google tool-stop projection](../architecture/adaptation-and-dialects.md#messages-terminal-projection). These adaptations do not promise equivalent Claude effort behavior or complete Google protocol support; further [Google API expansion is paused](../roadmap/backlog-and-frozen.md#google-api-pause).

Those reviewed Google bindings omit temperature/top-p/top-k that the model ignores, with field-specific adaptation evidence. Generic codecs and other models do not inherit that policy; request labels cannot select it. Interactions v1 has no carrier for those controls. See the [target-effective sampling contract](../architecture/adaptation-and-dialects.md#selected-request-approximations); this does not change routing or activate structured-result admission.

`reasoning_as_text` controls the explicit conversion of unattributed readable history reasoning into assistant text, not visibility of new reasoning output. With the default `false`, a candidate needing that conversion rejects. Separately, complete reasoning demonstrably belonging to another target is omitted under the named visible-history transfer policy, never promoted to an assistant answer. Visible messages and real call/results remain; partial reasoning, damaged compatible replay and required compacted/cached context cannot be repaired by omission. Valid native carriers remain available.

Only listed models are activated. Images, Speech and Embeddings select one account explicitly, for example `"text-embedding-3-small":{"account":"primary"}`. They retain single-target task execution without Generation overrides or multi-account recovery.

Transcription also requires `account`. A shared model requires an explicit model-level `binding`, not a Generation `candidates` list:

```json
{
  "version": 1,
  "models": {
    "qwen-audio-3.0-asr-flash": {
      "binding": "dashscope-qwen-asr",
      "account": "primary"
    }
  }
}
```

Select `dashscope-qwen-asr` for metered DashScope or `tokenplan-qwen-asr` for Token Plan. Both expose `qwen-audio-3.0-asr-flash`; trusted origins and account namespaces remain separate. Missing, disabled or mismatched selections reject at startup. Execution uses exactly the selected binding/account, even when other accounts are enabled.

Model optional fields reject explicit null except the named Messages default-clearing field. Standalone tasks reject Generation-only fields even when an array is empty. Candidate controls are validated for disabled accounts as well as enabled ones; enablement only selects routing participation.

The file supports at most 64 models and 64 candidates per model, with at most 64 expanded account targets per public-model/client entry. Unknown bindings, mismatched model identities, duplicate deployments and invalid account references fail startup. See [service loading](../../src/gateway/startup/bootstrap.rs) and [model assembly](../../src/gateway/startup/models.rs).

### Delta coalescing

Text-delta coalescing is configured at `gateway.json.delivery.delta_coalescing` and enabled by default. Omitting the field selects this policy; a complete object can override either bound:

```json
{
  "delta_coalescing": {
    "max_delay_ms": 50,
    "max_pending_bytes": 16384
  }
}
```

Both object fields are required. `max_delay_ms` must be 1–100 milliseconds; `max_pending_bytes` must be 1024–1048576 bytes. Invalid bounds, incomplete objects or unknown fields fail startup. Set `"delta_coalescing": null` for immediate encoding. Rust `Limits::default()` enables the same policy; set `Limits::delta_coalescing` to `None` to disable it.

Only compatible adjacent nonempty text deltas are merged. The first content batch is not intentionally delayed; a fixed deadline flushes pending text during upstream silence. The delay bounds intentional buffering, not client backpressure or end-to-end latency. The byte limit accounts for retained context and encoded text, not process RSS. `stream:false`, including a declared forced-SSE upstream, retains existing final-response materialization without a coalescing buffer.

See the [coalescing contract](../architecture/runtime-and-routing.md#optional-delta-coalescing) for merge eligibility, flush barriers, accounting, cancellation and EOF requirements. This service-only policy changes neither routing nor upstream traffic or token usage. Optional diagnostics report peak accounted pending bytes without content. Synthetic measurements do not establish SDK, real-client or Provider acceptance.

#### Choosing a window

Use eligible text-delta arrival cadence, not token rate alone. A Provider may put several tokens in one event, deliver bursts, or interleave barriers that prevent merging. A window shorter than the typical inter-delta gap can add latency without reducing event count. A 50 ms window is the default compromise; try 100 ms when fewer updates matter more than interactivity, or 10–20 ms when prompt updates matter more.

- First content is not intentionally delayed. Later eligible fragments wait at most the configured window **inside the coalescer**, unless a barrier or byte bound flushes earlier. Scheduling, backpressure, compression and the network add their own delay.
- For regularly spaced eligible fragments, the batch size is roughly the window divided by the inter-delta gap, rounded up and at least one. Sparse fragments can each wait the full window; the common half-window average only approximates sufficiently dense arrivals. Do not size the window from RTT alone.
- The byte bound includes retained context, text and encoding allowances, not just useful text. Increasing it does not force full buffers or improve ordinary small deltas; large contexts may bypass coalescing. Keep the 16 KiB default unless measured bypass or burst behavior justifies a change.
- Compare the same synthetic source stream with coalescing disabled and at 10/50/100 ms, then repeat through the actual proxy and client. Measure encoded and compressed bytes, first-content time, update gaps, cancellation and completion. Fewer events or handoffs are not proportional packet savings; HTTP/TLS packetization is independent.

Compression already removes repeated envelopes, so its benefit and coalescing's benefit are not additive. Larger windows can produce larger bursts and longer visible pauses. Coalescing reduces framing and delivery work; it does not reduce radio loss, repair disconnected requests, compress uploads or guarantee higher completion rates. See the [controlled timing check](../development.md#reverse-proxy-delivery-gate) and keep its synthetic scope explicit.

### Private failure context

Set `diagnostics.private_bodies:true` only when the trusted receiver is approved to receive sensitive business payloads. Bodies attach only to eligible failure/warning records; successful exchanges are not archived.

Four keys describe context: `client_request`, `upstream_request`, `upstream_response`, `client_response`. Each has bounded `snippet`, `observed_bytes`, `retained_bytes`, `truncated`, `incomplete`, `redacted`, `eof` and optional `omission` facts. The shared retained-text allowance is divided across four boundaries, with at most four recent safe units each. Parser buffers are separately bounded by the configured allowance. Safe complete units may be clipped; oversized, malformed or incomplete JSON is omitted rather than exported raw.

Headers/cookies are not captured. Known credential body fields and credential URL values are always redacted before retention/export; no redaction-disable switch exists. SSE snippets are normalized safe events, not original bytes; partial tool arguments are omitted, never reconstructed. Unsupported binary, multipart and plain-text content carries omission facts. Token/OAuth endpoint bodies are excluded.

Observation uses only already-read data and polled client output. It does not prefetch, drain, retry, wait for EOF or delay terminal publication. A later `gateway.failure.response` with the same request ID can add final/interrupted client output. Upstream context starts fresh per attempt; old observers cannot append to a later send. Standalone tasks return at a failed head without reading the error body, so that boundary remains metadata-only.

Snippets/EOF flags do not establish a complete exchange, remote delivery, execution counts or backend persistence. Never publish, commit, print or replay real private context, or use it as protocol fixtures. Use independent synthetic upstream hooks for protocol/count qualification and client-side measurements for visible timing.

## Start the gateway

```sh
cargo run --locked --offline --bin morphiecore -- --config "$ROOT/gateway.json"
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

For Rust embedding, `Gateway::new` takes a compiled topology and one `TaskEntries` value with separate Generation, Images, Speech, Transcription and Embeddings lists. Empty lists activate nothing. Credentials and limits remain explicit; the embedded constructor does not opt into environment proxies. Each Generation entry names one client protocol (Chat, Responses or Messages); file-configured models expose all three. See [task entries](../../src/gateway/startup/entries.rs) and [binding](../../src/gateway/startup/bind.rs).

### Binary-only SSH releases

Build on a Linux workstation with the same CPU architecture as the server:

```sh
nix develop .#release --command cargo build --release --bins --locked --offline
```

The Linux-only `release` shell uses the pinned Rust toolchain and musl compiler. On x86_64, outputs are in `target/x86_64-unknown-linux-musl/release/`; on aarch64, use `target/aarch64-unknown-linux-musl/release/`. These static executables need no remote Nix store, Cargo, compiler or source checkout. Ordinary `target/release/` binaries built in the development shell retain Nix runtime dependencies and are not standalone deployment artifacts. Nix may fetch the pinned build environment before the offline Cargo build.

Keep private configuration and credentials outside release directories. For an existing user service, inspect `systemctl --user show morphiecore.service -p ExecStart -p WorkingDirectory -p DropInPaths` before changing paths. Preserve its configuration path, credential overrides, proxy settings and lifecycle controls. The `llmapi` deployment uses this final override at `~/.config/systemd/user/morphiecore.service.d/30-binary-release.conf`:

```ini
[Service]
WorkingDirectory=%h/.local/lib/morphiecore
ExecStart=
ExecStart=%h/.local/lib/morphiecore/current/morphiecore
```

Omitting `--config` selects `$HOME/.local/share/morphiecore/gateway.json`, whose `accounts_file` and `credentials_dir` resolve inside that same directory. The CLIs share those defaults, so `morphiecore-auth` needs no path flags there; pass `--config`, `--accounts` and `--store` only for non-default installations.

After an authorized unit change, run `systemctl --user daemon-reload`. For noninteractive SSH, set `XDG_RUNTIME_DIR=/run/user/$(id -u)` when the login environment does not provide it.

From the local repository root, use Bash to upload only the three executables into a fresh release directory and compare transfer checksums:

```bash
set -euo pipefail
HOST=llmapi
OUT=target/x86_64-unknown-linux-musl/release
RELEASE="$(git rev-parse --short=12 HEAD)-$(date -u +%Y%m%dT%H%M%SZ)"
REMOTE_DIR=".local/lib/morphiecore/releases/$RELEASE"
BINS=(morphiecore morphiecore-auth morphiecore-upstream)

ssh "$HOST" "test ! -e '$REMOTE_DIR' && install -d -m 700 '$REMOTE_DIR'"
scp "${BINS[@]/#/$OUT/}" "$HOST:$REMOTE_DIR/"
diff <(cd "$OUT" && sha256sum "${BINS[@]}") \
     <(ssh "$HOST" "cd '$REMOTE_DIR' && sha256sum ${BINS[*]}")
```

After deployment authorization, validate through the production loader, atomically switch the release symlink and restart immediately. This uses the configuration path above; adjust it with the unit for other installations.

```bash
ssh "$HOST" bash -s -- "$RELEASE" <<'REMOTE'
set -euo pipefail
export XDG_RUNTIME_DIR="/run/user/$(id -u)"
cd "$HOME/.local/lib/morphiecore"
release="releases/$1"
chmod 755 "$release"/morphiecore{,-auth,-upstream}
"$release/morphiecore" --validate
ln -s "$release" current.next
mv -Tf current.next current
mkdir -p "$HOME/.local/bin"
for bin in morphiecore morphiecore-auth morphiecore-upstream; do
    ln -sfn "../lib/morphiecore/current/$bin" "$HOME/.local/bin/$bin"
done
systemctl --user restart morphiecore.service
systemctl --user is-active --quiet morphiecore.service
REMOTE
```

Check the running process's executable, loopback listener and authenticated client behavior before removing an older release. Rollback selects a retained release with the same atomic symlink switch and restarts the service. Never copy over a running executable in place. Initial migration may remove the former source/build directory only after successful activation and confirmation that configuration, credentials and service paths are independent of it. Former Nix builds can also leave project source copies in the store; identify those copies and remove only those paths with `nix-store --delete` after releasing their project-owned GC roots. Do not synchronize the checkout, use a broad `rsync --delete` against application state or run global garbage collection for this cleanup.

## Reverse-proxy deployment

Keep MorphieCore on loopback. A separately managed Nginx may expose the standard APIs over TLS; this is deployment guidance, not permission to publish a listener. The [delivery contract](../architecture/runtime-and-routing.md#reverse-proxy-delivery) owns cancellation, publication and trust boundaries.

Copy [`nginx.example.conf`](../../config/nginx.example.conf) to deployment storage and include it from Nginx's `http {}` context, not inside another `server {}`. Replace the `.invalid` domain and certificate paths. It targets the service template's `127.0.0.1:8080`; change both sides together when moving that listener.

The template includes example limits of 16 concurrent requests per IP and 32 for the virtual host. Review these for shared NATs and capacity; they are pre-authentication resource guards, not Agent quotas. Create its body-temporary directory with private service ownership on space-bounded storage, and protect access/error logs. The access format omits identifying headers, URLs and bodies; inherited Nginx error logs can still contain request URLs and must not be published. Do not enable debug logging for production traffic.

The active HTTP/2 directive requires a suitable module and Nginx 1.25.1 or newer. HTTP/3 is a commented opt-in after build/client/UDP verification, not an enabled default. Do not deploy the template unchanged or assume it provisions certificates, filesystem quotas or a firewall.

- **Compression:** Nginx owns response compression; clearing the backend `Accept-Encoding` does not clear the client's negotiation with Nginx. Start with gzip, measure incremental output, and only add br/zstd modules after client/build verification. SSE has no fixed `Content-Length`, so `gzip_min_length` is not an event-size threshold. Do not add `no-transform` while selecting proxy compression.
- **Buffering:** `proxy_buffering off` controls responses. MorphieCore also sends `X-Accel-Buffering: no`, which Nginx normally consumes rather than forwarding. Upload buffering remains on: the gateway's default 60-second body-collection deadline then applies to the local backend transfer, not the preceding wireless upload. Nginx may spool full histories to temporary files and receives them before gateway authentication; limit that pre-authentication resource exposure separately. If spooling is forbidden, review gateway ingress bounds before disabling request buffering.
- **Identity:** preserve `Authorization` or `x-api-key`, Messages version/beta headers and session carriers. Do not insert a shared key or accept a forwarded Agent identity. `underscores_in_headers on` preserves `session_id`; check the effective/default virtual-server settings when sharing listeners.
- **Deadlines:** the read/write timeouts above bound gaps between operations, not whole-request duration. Coordinate them with Agent idle timeouts and the gateway's finite exchange deadline. Longer proxy timeouts cannot override an earlier client cancellation; do not disable cancellation or invent heartbeat content.
- **Protocol versions:** the frontend may negotiate HTTP/2 or HTTP/3 independently of the HTTP/1.1 backend. Current Agent libraries must actually use them; an `Alt-Svc` advertisement or browser test is not evidence. Do not use replayable TLS early data for generation POSTs without a separately selected replay-safety contract.
- **Reporting:** record only bounded protocol/encoding, status, size and timing fields, not credentials, bodies, session IDs or raw URLs. `$gzip_ratio`, `$request_length` and `$body_bytes_sent` help compare configurations but do not count all TLS/TCP overhead or retransmissions. Backend handoff is not final Agent receipt.

Nginx references: [proxy controls](https://nginx.org/en/docs/http/ngx_http_proxy_module.html), [gzip](https://nginx.org/en/docs/http/ngx_http_gzip_module.html), [request limits and headers](https://nginx.org/en/docs/http/ngx_http_core_module.html). Run `nginx -t` against the actual deployment configuration and use the separate [proxy delivery gate](../development.md#reverse-proxy-delivery-gate) before claiming client acceptance. That gate does not deploy Nginx or certify TLS, HTTP/2/3, radio loss or request compression.

