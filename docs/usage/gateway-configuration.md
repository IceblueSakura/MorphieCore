# Gateway Configuration

This guide defines the private JSON configuration schemas (`gateway.json`, `auth.json`, `models.json`), public configuration templates, failure diagnostics settings, model activation schema, and streaming delta coalescing.

See [Credentials and Diagnostics](credentials-and-diagnostics.md) for `accounts.json` and credential provisioning, [Deployment](deployment.md) for daemon startup, SSH releases, Nginx, and loopback receivers, and [Architecture Runtime](../architecture/runtime-and-routing.md) for routing and failover mechanics.

## Contents

- [Configure the gateway](#configure-the-gateway)
  - [Configuration templates](#configuration-templates)
  - [Agent API keys](#agent-api-keys)
  - [Failure diagnostics](#failure-diagnostics)
  - [Model configuration](#model-configuration)
    - [Decision models](#decision-models)
    - [Qwen image candidates](#qwen-image-candidates)
    - [Media format transcoding](#media-format-transcoding)
  - [Delta coalescing](#delta-coalescing)
    - [Choosing a window](#choosing-a-window)
  - [Private failure context](#private-failure-context)

## Configure the gateway

Create private `gateway.json`, `auth.json`, `accounts.json` and `models.json` files. The default application directory is `$HOME/.local/share/morphiecore` (`%USERPROFILE%` on Windows); use `--config` for a fresh explicit setup. Parent directories must be private on Unix: directory mode `0700`, file mode `0600`. Keep configuration outside credential storage. Each file is bounded to 64 KiB and rejects unsupported versions, unknown fields and duplicate JSON keys. Changes take effect at startup; there is no hot reload.

### Configuration templates

The repository's [`config/`](../../config/) directory contains public templates, not live configuration:

| Template | Purpose |
|---|---|
| [`gateway.example.json`](../../config/gateway.example.json) | All current service fields, default 50 ms / 16 KiB coalescing, disabled failure diagnostics |
| [`auth.example.json`](../../config/auth.example.json) | Two independent Agent identities with placeholder keys (must be replaced before starting) |
| [`accounts.example.json`](../../config/accounts.example.json) | Ordered named API-key accounts referencing protected credential aliases, independently of model activation |
| [`models.example.json`](../../config/models.example.json) | Narrow starting point: Qwen 3.8 Flash through the Token Plan Chat binding with standard Chat delivery |
| [`models.advanced.example.json`](../../config/models.advanced.example.json) | Optional Generation controls: Messages/Google bindings, output limits, inherited-default clearing and both numeric thinking conversions |
| [`models.tasks.example.json`](../../config/models.tasks.example.json) | Optional existing Embedding, Images, Speech and explicit Transcription binding selection, without Generation overrides |
| [`pi-deepseek.example.json`](../../config/pi-deepseek.example.json) | Optional pi client model declaration for DeepSeek through the gateway; merge into pi's `models.json`, not gateway model configuration. See [Chat Completions](client-requests.md#chat-completions) |
| [`nginx.example.conf`](../../config/nginx.example.conf) | Optional TLS virtual host, gzip SSE, proxy safety controls and example pre-authentication limits; include from `http {}` |
| [`otelcol.example.yaml`](../../config/otelcol.example.yaml), [`otelcol.example.service`](../../config/otelcol.example.service) | Optional loopback diagnostics receiver and user unit; see [setup and retention](deployment.md#loopback-file-receiver) |

Choose one model template as `models.json`, or combine selected entries under one `models` object. The loader does not merge files, expand placeholders or discover templates. Unused Provider declarations in `accounts.json` must be removed before validation if those accounts are not provisioned.

For a **new** Unix installation from the repository root (refuses to overwrite existing files or symlinks):

```sh
ROOT="${ROOT:-$HOME/.local/share/morphiecore}"
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

Replace **each** Agent-key placeholder with an independently generated high-entropy key of at least 32 printable non-space ASCII characters, and provision every referenced account with `--accounts "$ROOT/accounts.json" --store "$ROOT/credentials"`.

`proxy: null` leaves environment-proxy fallback in effect rather than forcing direct connections. Provider metadata is compiled into the binary; there is no runtime `providers.json` template. Keep edited copies, credentials, certificates and captures outside the repository.

`gateway.json` schema:

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
| `auth_file` | Optional path, default `auth.json` beside this file; resolved private file must exist and validate |
| `bind` | Loopback listener; default `127.0.0.1:8080` |
| `models_file` | Optional path, default `models.json` beside this file; resolved file must exist and validate |
| `accounts_file` | Optional path, default `accounts.json` beside this file; resolved file must exist and validate |
| `credentials_dir` | Optional credential-store path; omission retains the independent home-relative default |
| `proxy` | Outbound proxy URL (or `null`) |
| `routing` | Global affinity, pre-dispatch wait and downstream retry-hint policy |
| `oauth` | Global credential-maintenance timing |
| `delivery` | Total request deadline, FFmpeg executable (`ffmpeg`), and text batching (`delta_coalescing`) |
| `resources` | Concurrent exchanges, request/response body budgets and upload deadline |
| `diagnostics` | Optional trusted OTLP Logs/Traces destination and bounded private-body policy; see [Failure diagnostics](#failure-diagnostics) |

Paths inside `gateway.json` resolve relative to `gateway.json`, not the process working directory. Policy groups omit to defaults and reject `null`, except `delivery.delta_coalescing: null` disables batching; `auth_file`, `models_file` and `accounts_file` reject `null`; `credentials_dir`, `proxy`, `delivery.ffmpeg` and `diagnostics` accept `null`.

| Group | Fields and defaults | Bounds / effect |
|---|---|---|
| `routing` | `implicit_cache_affinity: true`, `affinity_ttl_ms: 3600000`, `affinity_capacity: 4096`, `max_wait_ms: 10000`, `retry_hint_horizon_ms: 60000` | Zero TTL/capacity retains no bindings; zero wait disables waiting. Hint horizon bounds downstream `Retry-After` |
| `oauth` | `scan_interval_ms: 15000`, `refresh_ahead_seconds: 120`, `renewal_timeout_ms: 90000`, `failed_renewal_delay_ms: 60000`, `minimum_renewal_interval_ms: 60000` | Zero scan interval checks startup only; zero renewal timeout disables renewal |
| `delivery` | `exchange_timeout_ms: 3600000` | 1–86400000 ms; begins **before body collection and admission**, covering execution/delivery and bounding model deadlines |
| `delivery` | `delta_coalescing: {"max_delay_ms":50,"max_pending_bytes":16384}` | Delay 1–100 ms, pending bytes 1024–1048576; see [Delta coalescing](#delta-coalescing) |
| `delivery` | `ffmpeg: null` | Absolute path to trusted FFmpeg binary for non-native image/audio transcoding; see [Media format transcoding](#media-format-transcoding) |
| `resources` | `concurrency: 16` | 1–256; body collection occupies a permit |
| `resources` | `request_bytes: 33554432`, `response_bytes: 67108864` | Each 1–67108864 bytes; intersect with endpoint/task limits. Too small to publish discovery rejects startup |
| `resources` | `body_timeout_ms: 60000` | 1–60000 ms; upload ceiling within the total request budget |

Inner ceilings remain code-owned: compiled Generation attempts retain a 1-hour ceiling, standalone tasks retain 120 s (with Transcription's independent 4 MiB upload cap), and OAuth HTTP retains 30 s total / 10 s connect timeouts. Decoded static images default to 2 MiB per image / 2 MiB aggregate at the gateway (semantic ceiling 8 MiB); binary Speech defaults to 8 MiB (semantic ceiling 16 MiB). Raising an outer limit does not enlarge these inner bounds. See [Limits](../../crates/morphiecore-gateway/src/startup/limits.rs) and [stateful history](../architecture/history-and-replay.md#stateful-history).

Inspect available bindings without configuration or credentials, or validate/explain a setup without starting listeners:

```sh
morphiecore --catalog
morphiecore --catalog --json
morphiecore --config "$ROOT/gateway.json" --validate
morphiecore --config "$ROOT/gateway.json" --explain --json
```

`--explain` uses the production loader (requiring valid local credentials) and prints safe resolved configuration without echoing keys, aliases, private paths, custom upstream model strings, or target URLs.

### Agent API keys

`auth.json` has one versioned schema. Start from [`auth.example.json`](../../config/auth.example.json) and replace its invalid key placeholders.

| Field | Contract |
|---|---|
| `version` | Required integer `1`; unsupported versions reject |
| `agents` | Required array of 1–256 records |
| `agent_id` | Required unique, case-sensitive identity: 1–128 ASCII bytes, starts with letter/digit, then letters/digits or `.`, `_`, `-` |
| `key` | Required globally unique secret: 32–4096 printable non-space ASCII bytes |
| `name` | Optional display label; defaults to empty, up to 256 UTF-8 bytes, no control characters |
| `desc` | Optional maintenance description; defaults to empty, up to 2048 UTF-8 bytes |

Records reject unknown fields and unexpected `null` values; duplicate IDs or keys reject startup. Authentication binds `agent_id` strictly from `key` (via Bearer or `x-api-key`); headers/body cannot select or override it. There is no per-Agent ACL or quota system—keep `agent_id` stable when rotating a key, remove a record and restart to revoke access, and never reuse a retired `agent_id` for a different Agent.

### Failure diagnostics

Diagnostics are opt-in via `diagnostics`:

```json
{
  "diagnostics": {
    "endpoint": "http://127.0.0.1:4318",
    "level": "info",
    "private_bodies": false,
    "body_bytes": 16384
  }
}
```

| Field | Contract |
|---|---|
| `endpoint` | HTTP/HTTPS OTLP base URL with host (no userinfo/query/fragment). Appends `/v1/logs` and `/v1/traces` |
| `level` | `error`, `warn`, `info` (default) or `debug`; filters logs independently of traces |
| `private_bodies` | Default `false`; explicit opt-in for [Private failure context](#private-failure-context) |
| `body_bytes` | Shared retained-body allowance in bytes; default `16384`, accepted `1024–65536` |

Export disables proxy inheritance and redirects, and rejects ambient `OTEL_EXPORTER_OTLP*_HEADERS`. Two SDK queues hold up to 2,048 signals each (100 ms flush, batches of 4 logs or 128 spans, 2 s HTTP timeout, 1 MiB request bound, 5 s shared shutdown budget). Use [Deployment](deployment.md#diagnostics-receivers) for loopback collector and development backend instructions, and [Observability](../architecture/runtime-and-routing.md#observability) for field/span semantics. Key log/trace filters: `request_id`, `agent_id`, `operation`, `provider`, `account`, `model`, `upstream_model`, `attempt`, `stage`, `error.type`, `retry.planned`, `http.response.status_code`, `gateway.error.status_code`, `morphiecore.response.outcome`, `conversion.boundary`, `decode.operation`, `source.last_event.{type,ordinal,sequence}`, and `handling.error.type`. Set `MORPHIECORE_REVISION` at build time to populate `morphiecore.build.revision`.

### Model configuration

`models.json` activates explicit public models and ordered upstream candidates (`64` combined entries in `models` and `decisions`, `64` candidates per Generation model, `64` expanded account targets per entry). Use `morphiecore --catalog` for registered public model IDs and catalog `binding` IDs; account targets and credentials belong in [accounts.json](credentials-and-diagnostics.md#configure-accounts). See [the catalog](../../crates/morphiecore-catalog/src/topology/catalog.rs), [service loading](../../crates/morphiecore-gateway/src/startup/bootstrap.rs), and [model assembly](../../crates/morphiecore-gateway/src/startup/models.rs).

| Scope | Field | Meaning |
|---|---|---|
| File | `version` | Required current schema version, `1` |
| File | `models` | Required map of non-Decision model activations; use `{}` for a Decision-only gateway |
| File | `decisions` | Optional map of explicit native Decision activations |
| Model | `candidates` | Nonempty ordered array of Generation bindings for this public model |
| Model | `total_timeout_ms` | Optional model-local total deadline from request start, bounded by service ceiling |
| Standalone task | `account` | Required enabled API-key/task account name in the selected binding's Provider |
| Model | `thinking_tokens` | Optional six-level numeric table (`minimal`, `low`, `medium`, `high`, `xhigh`, `max`) shared by this model's candidates |
| Candidate | `binding` | Required catalog endpoint ID; fixes protocol, dialect, canonical model and auth kind |
| Candidate | `upstream_model` | Wire spelling override; does not change canonical identity or model capabilities |
| Candidate | `thinking_conversion` | `budget_to_effort` fallback; omitted performs no numeric conversion |
| Candidate | `allowed_effort` | Optional nonempty array of distinct `minimal`..`max` levels; narrows catalog declaration |
| Candidate | `context_window_tokens` | Optional positive `u64` total-context planning capacity; narrows known or supplies unknown |
| Candidate | `maximum_input_tokens` | Optional positive `u64` independent input planning maximum; cannot widen known |
| Candidate | `maximum_output_tokens` | Optional positive `u64` model-output clamp (not a default); cannot exceed catalog maximum |
| Candidate | `default_output_tokens` | Messages only: omit to inherit binding default, positive `u64` to replace, or `null` to clear |

Key inheritance and validation rules:
- **Omitted fields** inherit catalog defaults; model optional fields reject `null` except Messages `default_output_tokens`. Standalone tasks reject Generation fields even if empty.
- **Account overrides & replay:** Overriding an account's base URL, path, or `upstream_model` clears inherited context/input discovery evidence and alters the normalized deployment replay fingerprint ([history contract](../architecture/history-and-replay.md)).
- **Messages defaults:** `default_output_tokens` must not exceed `maximum_output_tokens` at startup; uncapped requests on Messages candidates without an effective default reject locally ([missing output ceilings](../architecture/provider-dialects.md#missing-output-ceilings)).
- **Thinking conversion:** Supply all six positive keys in `thinking_tokens` (requires at least one candidate selecting a conversion); `thinking_conversion: "budget_to_effort"` uses the [shared preset](../architecture/adaptation.md#thinking-budget-quantization) if `thinking_tokens` is omitted. Exact effort controls take precedence over numeric quantization; reviewed Gemini 4 bindings inherit response-effort scope projection unless overridden ([Google dialects](../architecture/provider-dialects.md#google-interactions)).
- **Capability & history semantics:** Output clamping, effort nearest-level narrowing, cache-control policies, and reasoning-history handling follow [Adaptation](../architecture/adaptation.md#model-effective-values), [Provider Dialects](../architecture/provider-dialects.md), and [Client Requests](client-requests.md).

Standalone tasks (`GPT Image`, `Gemini Nano Banana`, `Grok Imagine`, `Embeddings`, `Speech`, `Transcription`) configure a single `account` (and explicit `binding` when shared across origins, e.g. `qwen-audio-3.0-asr-flash` selecting `dashscope-qwen-asr` or `tokenplan-qwen-asr`) without multi-account recovery:

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

#### Decision models

Use the separate `decisions` map; a `gpt-6-luna` entry under `models` does not enable `POST /v1/decisions`. Both may be present in one file. Each Decision entry selects one existing enabled API-key account scoped to that binding's Provider; there are no candidates or model overrides. Omit any entry to leave that binding disabled.

```json
{
  "version": 1,
  "models": {},
  "decisions": {
    "gpt-6-luna": {"account": "platform"},
    "jev-1.14.0": {"account": "jev"},
    "gemini-decision-1": {"account": "google-platform"}
  }
}
```

Declare the selected accounts in `accounts.json` under `openai` (`platform`), `typesafe` (`jev`), or `google` (`google-platform`), then provision them through [the credential CLI](credentials-and-diagnostics.md#add-an-api-key). Substitute your configured names. Gemini is discoverable only on the TypeSafe surface and accepts only plain-text Noul. See [Decision API](api-reference.md#decisions) for the exact profiles and SDK base URLs.

#### Qwen image candidates

Use the ordered `candidates` array in [`models.tasks.example.json`](../../config/models.tasks.example.json) to activate `qwen-image-3.0-pro`. Each entry requires `binding` and an enabled API-key `account` owned by that binding's Provider. Configure either candidate alone or both; when both are present, `tokenplan-qwen-image` must precede `dashscope-qwen-image`. See [pre-dispatch image selection](../architecture/runtime-and-routing.md#standalone-image-candidates) and [static image generation](api-reference.md#static-image-generation).

#### Media format transcoding

Set `delivery.ffmpeg` in `gateway.json` to the **absolute path** of a trusted FFmpeg binary (the Nix devShell provides `ffmpeg-headless`; musl release binaries do not bundle it). Requests already matching the upstream native format pass through without spawning FFmpeg; non-native conversions (e.g., Gemini/Grok PNG or WebP/JPEG outputs, non-native Aliyun/Xiaomi audio formats) invoke FFmpeg with fixed bounded arguments, pinned container demuxers, pipe I/O (or bounded temporary files for seekable MP4/M4A), audio duration caps, and Linux address-space limits—never scaling images or passing URLs/credentials. Transcode-requiring requests fail pre-dispatch with `503 Service Unavailable` when `delivery.ffmpeg` is unset.

### Delta coalescing

Text-delta coalescing defaults to `{"max_delay_ms": 50, "max_pending_bytes": 16384}` under `gateway.json.delivery.delta_coalescing`; omit to enable, override both required fields (`1–100` ms, `1024–1048576` bytes), or set `null` to disable (in Rust embedding, configure `Limits::delta_coalescing`). Non-streaming (`stream: false`) requests bypass the coalescing buffer.

Only compatible adjacent nonempty text deltas merge; the first content batch is not intentionally delayed, and a fixed deadline flushes pending text during silence. The byte bound accounts for retained context and encoded text allowances rather than RSS. See the [coalescing contract](../architecture/runtime-and-routing.md#optional-delta-coalescing) for flush barriers and cancellation behavior.

#### Choosing a window

Size the window from eligible text-delta arrival cadence rather than RTT or token rate alone: try `100` ms when fewer SSE frames matter more than interactivity, `10–20` ms for low-latency UI updates, or keep `50` ms / `16 KiB` unless measured burst or context bypass justifies tuning. Proxy compression already deduplicates repeated envelopes, so coalescing savings reduce framing/delivery work rather than compounding compression ratios linearly. Benchmark using the [controlled timing gate](../development.md#reverse-proxy-delivery-gate).

### Private failure context

Set `diagnostics.private_bodies: true` only when the receiver is authorized for sensitive payloads. Capture records only eligible WARN/ERROR exchanges across four bounded views (`client_request`, `upstream_request`, `upstream_response`, `client_response`), each capped at one-quarter of `body_bytes` and at most four recent safe units (`snippet`, `observed_bytes`, `retained_bytes`, `truncated`, `incomplete`, `redacted`, `eof`, optional `omission`).

Headers, cookies, OAuth/token bodies, binary/multipart payloads, partial tool arguments, and unparsed JSON are excluded or omitted; known credential fields and URL tokens are always redacted before retention/export. Observation reads only already-polled buffers without prefetching or delaying failure delivery; standalone task errors record metadata only. Never print, commit, replay, or use private failure captures as fixtures.
