# Credentials and Upstream Diagnostics

This guide covers credential storage, account declaration, OAuth renewal workflows via `morphiecore-auth`, and upstream model diagnostics via `morphiecore-upstream`.

See [Usage Index](index.md) for document map, [Gateway Configuration](gateway-configuration.md) for daemon setup, and [Client Requests](client-requests.md) for request execution.

## Contents

- [Prepare credentials](#prepare-credentials)
  - [Add an API key](#add-an-api-key)
  - [Configure accounts](#configure-accounts)
  - [Recovery defaults](#recovery-defaults)
  - [OAuth](#oauth)
- [Manual upstream diagnostics](#manual-upstream-diagnostics)
  - [List upstream models](#list-upstream-models)
  - [Probe an exact upstream model](#probe-an-exact-upstream-model)
  - [Credential selection and optional controls](#credential-selection-and-optional-controls)

## Prepare credentials

The gateway, `morphiecore-auth` and `morphiecore-upstream` share a default store:

- Unix: `$HOME/.local/share/morphiecore/credentials`
- Windows: `%USERPROFILE%/.local/share/morphiecore/credentials`

They do not discover credentials from the working directory, `XDG_DATA_HOME`, environment keys or third-party auth caches.

On Unix, the store directory must be `0700` and private files must be `0600`. Unsafe permissions are rejected, not repaired. Never commit, print or hand-edit credential files while the service runs.

Build the CLIs:

```sh
cargo build --locked --offline --bins
ROOT="$HOME/.local/share/morphiecore"
```

`ROOT` is the default application directory, so the `--accounts "$ROOT/accounts.json"` and `--store "$ROOT/credentials"` arguments below may be omitted when configuration lives there; keep them explicit for any other directory. A new explicit directory for the breaking schema is also valid, and existing deployments and stores remain operator-owned. Copy the [configuration templates](gateway-configuration.md#configuration-templates) before provisioning. The examples use `morphiecore-auth` as shorthand for `cargo run --locked --offline --bin morphiecore-auth --`.

### Add an API key

Select a Provider and its configured account name, not a credential alias. The [account declaration](#configure-accounts) resolves the private storage reference. Read the key from a private file, never a command-line value:

```sh
morphiecore-auth --accounts "$ROOT/accounts.json" --store "$ROOT/credentials" \
  key add --provider "$PROVIDER_ID" --account primary --secret-file /path/to/private-key
morphiecore-auth --accounts "$ROOT/accounts.json" --store "$ROOT/credentials" \
  status --provider "$PROVIDER_ID"
```

Commands print human-readable output by default; `--json` selects one structured document. Status includes unprovisioned accounts without hiding existing accounts' revisions. Updates require the current revision:

```sh
morphiecore-auth --accounts "$ROOT/accounts.json" --store "$ROOT/credentials" \
  key replace --provider "$PROVIDER_ID" --account primary --revision "$REV" --secret-file /path/to/private-key
morphiecore-auth --accounts "$ROOT/accounts.json" --store "$ROOT/credentials" \
  key disable --provider "$PROVIDER_ID" --account primary --revision "$REV"
```

`enable` and `remove` use the same identity and revision arguments. Re-read the revision after every change. Removing a key deletes the local secret; it does not revoke the upstream key.

A running gateway pins key identity at startup. Replacement, disablement and removal invalidate that binding; restart with the current configuration to bind replacement material. A replaced key never inherits the old key's replay or cache scope.

### Configure accounts

`accounts.json` is private configuration outside the credential store. It lists ordered named accounts and alias-only credential references, not tokens or verified principal metadata. The Provider catalog determines credential kind and OAuth profile; account-level `kind` and `profile` fields reject. Start with [`accounts.example.json`](../../config/accounts.example.json). A minimal API-key declaration is:

```json
{
  "version": 1,
  "providers": {
    "openai": {
      "accounts": [
        {
          "name": "primary",
          "enabled": true,
          "credential": {"alias": "platform-primary"}
        }
      ]
    }
  }
}
```

Names are unique within a Provider and stay stable when the list is reordered. Route candidate order precedes account declaration order. `enabled` controls routing only; configured Active OAuth sessions remain maintained when disabled or unreferenced. Stored credentials alone activate no route.

Account affinity groups trusted Agent identity, public model and an explicit conversation/session carrier. Only validated completed work, including tool calls and requested output-limit completion, can commit after final delivery. Failed, cancelled and other incomplete outcomes do not recover health or change the binding. Preferences are advisory: the last successful completion wins, including late completions after expiry or eviction. No routing probe or concurrent-commit ownership is reserved.

`gateway.json.routing` sets `affinity_ttl_ms` (default 3600000) and `affinity_capacity` (default 4096). TTL slides only on successful completion, not on lookups or failures. Expiry and eviction remove the preference; a later successful completion may recreate it. Zero capacity or TTL retains no bindings. Prompt-prefix hints remain independent.

`routing.max_wait_ms` (default 10000) bounds one pre-dispatch wait for a conversation's cooling bound target. Zero disables waiting. New conversations never wait for an unbound candidate; after waiting, lifecycle and cooldowns are checked again within the original request deadline.

Each Provider declaration accepts an optional `recovery` object with the timing fields in [`RecoveryPolicy`](../../src/provider/recovery.rs). Omitted fields use the [defaults below](#recovery-defaults). Settings do not change failure classes, health scopes or the fixed two-send ceiling. `recovery_timeout_ms` bounds the one same-target retry through readiness for deliverable content; zero disables that retry. The original request deadline still bounds all work.

An API-key account may set `base_url` to a complete trusted API root. The catalog appends an explicit operation-relative suffix: `https://relay.example/custom/v1` plus `responses` becomes `/custom/v1/responses`. Omission retains each operation's original catalog target. HTTPS is required except for literal loopback HTTP; userinfo, queries, fragments and unsafe paths reject. OAuth overrides reject before credential borrowing.

`openai` means OpenAI API-key access. SIWC is the separate `chatgpt` Provider, which derives OAuth profile `openai` from the catalog. Grok uses Provider/profile `grok`. Both reference credentials with the same alias-only shape. Model availability and fixed request controls belong to each Provider's catalog bindings, not the account file.

Inspect declarations without opening the credential store, or validate all local references without login, refresh or probes:

```sh
morphiecore-auth --accounts "$ROOT/accounts.json" explain
morphiecore-auth --accounts "$ROOT/accounts.json" --store "$ROOT/credentials" validate
morphiecore --config "$ROOT/gateway.json" --validate --json
```

Gateway validation also uses the real model/task loader and Agent authentication configuration, without a listener or observation-sink activation. Schema errors, duplicate names/subjects, incompatible references and unsafe storage fail closed. Provision every referenced account first.

### Recovery defaults

Provider `recovery` fields use milliseconds except for integer multipliers. Service-wide wait, affinity and OAuth settings belong to [gateway configuration](gateway-configuration.md#configure-the-gateway), not per-account overrides.

| Fields | Defaults |
|---|---|
| `recovery_timeout_ms` | `30000` |
| `transient_cooldown_ms`, `transient_multiplier`, `transient_max_cooldown_ms` | `1000`, `2`, `60000` |
| `rate_limit_cooldown_ms`, `rate_limit_multiplier`, `rate_limit_max_cooldown_ms` | `60000`, `2`, `1800000` |
| `reset_hint_floor_ms`, `rate_limit_reset_max_ms`, `quota_reset_max_ms` | `1000`, `3600000`, `604800000` |
| `quota_cooldown_ms`, `auth_cooldown_ms`, `permission_cooldown_ms` | `1800000` each |
| `failure_free_reset_ms` | `1800000` |

Policy values and their relationships remain operator-owned; ordinary strict parsing and security checks still apply. Zero recovery disables the extra send, never means unlimited execution and cannot relax identity, lifecycle or publication boundaries. [Health and recovery](../architecture/runtime-and-routing.md#health-and-recovery) owns failure classification and timing semantics.

### OAuth

Declare the named OAuth account before login. Login and revocation remain explicit operator actions; ordinary inference borrowing is network-free:

```json
{
  "version": 1,
  "providers": {
    "chatgpt": {
      "accounts": [
        {"name": "personal", "enabled": true, "credential": {"alias": "personal"}}
      ]
    }
  }
}
```

```sh
morphiecore-auth --accounts "$ROOT/accounts.json" --store "$ROOT/credentials" \
  login --provider chatgpt --account personal
morphiecore-auth --accounts "$ROOT/accounts.json" --store "$ROOT/credentials" status
morphiecore-auth --accounts "$ROOT/accounts.json" --store "$ROOT/credentials" \
  refresh --provider chatgpt --account personal
morphiecore-auth --accounts "$ROOT/accounts.json" --store "$ROOT/credentials" \
  logout --provider chatgpt --account personal
```

| Option | Scope |
|---|---|
| `--accounts`, `--store`, `--json` | Global auth-CLI options |
| `--proxy` | Accepted by `login`, `refresh`, `logout`, OAuth `remove` and every `key` subcommand. Key operations ignore it; OAuth removal remains local, not revocation. Not available on `status`, `validate` or `explain` |
| `--method` | ChatGPT/SIWC supports browser only; Grok defaults to device and also supports browser |
| `--callback-port` | Browser login only; explicit device selection rejects it |
| `--client-id` | Grok login only; SIWC rejects it |
| `--consent` | ChatGPT/SIWC login only; Grok rejects it for both browser and device methods |
| `--revoke` | `logout` only; requests upstream revocation |

Grok device links prefill `user_code`; confirm that it matches the CLI before approving. Browser login accepts either the HTTP callback or a full callback URL on stdin; the first valid result wins without a separate input mode.

An Active account rejects another login; use `refresh`, or explicitly sign out before returning sign-in or consent. Pending login rejects concurrent login. Reauthorization and login after logout retain registration identity. Another alias cannot publish the same verified subject, even with another issued client; disablement, logout and reauthorization-required states retain that reservation.

`remove --provider chatgpt --account personal --revision "$REV"` clears the local session and releases its subject reservation without requesting remote revocation. Use it before deleting the configuration declaration. Re-registration gets a fresh immutable identity with a monotonic revision; old running bindings remain invalid. Removing configuration alone does not revoke an upstream session.

Callbacks listen on `127.0.0.1`. Login checks issuer, client and subject. Gateway inference never starts login or refresh.

For an SSH/headless host, run the same browser login and open the printed authorization URL on your computer. If the browser cannot reach the loopback callback, copy its **full URL** from the address bar and paste it into the waiting remote CLI, then press Enter. No SSH port forwarding or special flag is needed.

Paste only the callback URL from this login, including its query.

Bare codes, API keys and access tokens are not accepted.

The URL must match the prepared callback origin, port and path; the existing listener validates `state`, duplicate fields and issued client identity.

PKCE, nonce, identity verification and the login deadline remain unchanged.

Input is an ordinary echoed line, bounded to 8192 bytes; the CLI does not manage terminal mode or partially typed input.

Closing stdin leaves the browser callback available; Ctrl-C cancels the login.

Check the final CLI result, not merely receipt of the URL.

Automatic renewal runs in the background when running `morphiecore` (`Gateway::serve`):
- Checks configured OAuth accounts independently of routes and enablement on startup and at `oauth.scan_interval_ms` (default 15000).
- Renews credentials within `oauth.refresh_ahead_seconds` (default 120).
- Bounds each renewal with `oauth.renewal_timeout_ms` (default 90000); zero dispatches no renewal and consumes no material.
- Spaces successful short-grant renewals with `oauth.minimum_renewal_interval_ms` (default 60000). Failed eligible work uses `oauth.failed_renewal_delay_ms` (default 60000), not an exponential retry loop.
- Persists rotated tokens in the private credential store.

All settings belong to `gateway.json`. Zero scan interval keeps the startup check but disables periodic scanning. Timing never restores uncertain refresh material. Current SIWC/Grok network and temporary-service failures provide no safe-consumption proof, so ambiguous dispatched refresh remains quarantined; the delay alone authorizes no retry.

Use `status --provider ID` for redacted state and `refresh --provider ID --account NAME` for manual renewal, with the same explicit account/store paths.

When replacement material was received but identity verification failed, `refresh` resumes that verification without repeating the token exchange.

Automatic renewal also retries it after a network/5xx or timeout failure, every `oauth.failed_renewal_delay_ms`; after a denial, identity mismatch or invalid response only manual `refresh` retries.

Pending material remains unavailable to inference until verification succeeds.

Renewal never detaches network work; an already-started atomic store transaction drains before renewal ownership is released.

Material already returned by the Provider is still published after a renewal timeout or shutdown.

## Manual upstream diagnostics

`morphiecore-upstream` uses the same [embedded Provider metadata](../architecture/index.md#shared-provider-metadata) as the gateway. It executes **one direct upstream request** without retries or failover.

Build with `cargo build --locked --offline --bin morphiecore-upstream`. Provider names are positional:

### List upstream models

```sh
morphiecore-upstream models deepseek --account primary \
  --accounts "$ROOT/accounts.json" --store "$ROOT/credentials"
```

No protocol argument is required or accepted. The Provider's `models` endpoint supplies its path and authentication. The command prints the complete upstream JSON response, preserving all fields rather than extracting IDs or wrapping it in a report. Pagination fields remain untouched; no further page is fetched. There is no `--cursor` option. A complete response does not necessarily contain the entire Provider catalog or prove model entitlement.

If the Provider has no declared `models` endpoint, the command fails before credential access; it never guesses a path. Confirm the endpoint and update the repository metadata before rebuilding. xAI's list API is documented for API keys; OAuth entitlement remains unverified and may reject.

### Probe an exact upstream model

```sh
morphiecore-upstream probe deepseek \
  --model deepseek-flash --protocol openai-chat --account primary \
  --accounts "$ROOT/accounts.json" --store "$ROOT/credentials"

morphiecore-upstream probe opencode-go \
  --model claude-haiku-5-5 --protocol anthropic --account primary \
  --accounts "$ROOT/accounts.json" --store "$ROOT/credentials"

morphiecore-upstream probe google \
  --model gemini-3.8-flash --protocol google --account primary \
  --accounts "$ROOT/accounts.json" --store "$ROOT/credentials"

morphiecore-upstream probe openai \
  --model gpt-6-astra --protocol openai-responses --account primary \
  --accounts "$ROOT/accounts.json" --store "$ROOT/credentials"
```

The protocol choices are `openai-chat`, `openai-responses`, `anthropic` and `google` (generateContent). The operation must be declared by the Provider, but the model name is an ordinary upstream string: it need not appear in the local catalog or a previous model list. The fixed prompt is `Reply with the single word OK.`; no user payload, tools or history are accepted.

Optional token limits are omitted. Anthropic Messages alone requires `max_tokens`, so its request uses a fixed `1024`. SIWC requires `store:false` and `stream:true`; other probes use static JSON. SIWC consumes SSE through strict EOF, including closed event-owned output when its final summary is empty. Local deadlines and byte bounds do not guarantee bounded remote computation or billing.

Success requires nonempty text plus an ordinary completion (`stop`, `completed`, `end_turn` or `STOP`), not merely HTTP 200 or an exact `OK` answer. Truncation, failed/incomplete terminals and malformed responses fail. Reports contain status, elapsed time, byte counts and whether OAuth refreshed, not generated text, raw errors, credentials or private identities. This is a basic service observation, not gateway compatibility or capability certification.

### Credential selection and optional controls

`--account NAME` is required. The tool loads that named declaration from the shared account file, composes its effective target through the same catalog binder, and borrows only its credential. It never scans accounts or selects another by availability. A credential alias is not an account selector.

| Option | Meaning |
|---|---|
| `--account NAME` | Required configured account name within the positional Provider |
| `--accounts PATH` | Shared private account configuration; default `accounts.json` in the home-relative application directory |
| `--store PATH` | Select another project credential store |
| `--proxy URL` | Override system/environment proxies for upstream HTTP and OAuth operations |
| `--timeout-seconds N` | Whole refresh/request deadline, default 60, range 1–300 |

Targets and authentication come from the Provider/account binder, not command-line endpoint or credential-locator overrides. Use `chatgpt` for SIWC and `openai` for Platform API keys.

OAuth pins the selected identity, refreshes locally expired access once or completes a pending renewal, and persists through the existing manager. There is no login, permission escalation, refresh after HTTP 401, account replacement or inference replay. Failed/interrupted refresh may require recovery with `morphiecore-auth`. Gateway inference still never refreshes credentials.

To add a Provider for manual diagnostics, edit `src/provider/providers.json` with its trusted origin, authentication, confirmed operation paths and override suffixes, then rebuild. Declare and provision its named account separately. No gateway model binding is required for a probe. There is **no runtime Provider file or hot reload**.

The HTTP response ceiling is 4 MiB; JSON keeps shared structural limits, and SIWC limits events to 1 MiB each/10,000 total. Responses are buffered; oversized responses fail rather than print a truncated result. Echoed authentication material is rejected before output. Like the gateway and auth CLI, upstream HTTP follows system/environment proxy settings by default; an explicit proxy overrides them without direct fallback. Ctrl-C cancels local work; socket release does not prove remote computation stopped.

