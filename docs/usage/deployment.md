# Gateway Deployment and Operations

This guide covers starting the gateway daemon, building and deploying binary-only SSH releases, fronting the loopback listener with Nginx, and running optional loopback telemetry receivers.

See [Gateway Configuration](gateway-configuration.md) for JSON configuration schemas and templates, [Credentials and Diagnostics](credentials-and-diagnostics.md) for credential provisioning, and [Architecture Runtime](../architecture/runtime-and-routing.md) for delivery and cancellation contracts.

## Contents

- [Start the gateway](#start-the-gateway)
  - [Binary-only SSH releases](#binary-only-ssh-releases)
- [Reverse-proxy deployment](#reverse-proxy-deployment)
- [Diagnostics receivers](#diagnostics-receivers)
  - [Loopback file receiver](#loopback-file-receiver)
  - [Indexed development query](#indexed-development-query)

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

For Rust embedding, `Gateway::new` takes a compiled topology and one `TaskEntries` value with separate Generation, Images, Speech, Transcription and Embeddings lists. Empty lists activate nothing. Credentials and limits remain explicit; the embedded constructor does not opt into environment proxies. Each Generation entry names one client protocol (Chat, Responses or Messages); file-configured models expose all three. See [task entries](../../crates/morphiecore-gateway/src/startup/entries.rs) and [binding](../../crates/morphiecore-gateway/src/startup/bind.rs).

### Binary-only SSH releases

Build on a Linux workstation with the same CPU architecture as the server:

```sh
nix develop .#release --command cargo build --release --bins --locked --offline
```

The Linux-only `release` shell uses the pinned Rust toolchain and musl compiler. On x86_64, outputs are in `target/x86_64-unknown-linux-musl/release/`; on aarch64, use `target/aarch64-unknown-linux-musl/release/`. These static executables need no remote Nix store, Cargo, compiler or source checkout. Ordinary `target/release/` binaries built in the development shell retain Nix runtime dependencies and are not standalone deployment artifacts. Nix may fetch the pinned build environment before the offline Cargo build.

Keep private configuration and credentials outside release directories. For an existing user service, inspect `systemctl --user show morphiecore.service -p ExecStart -p WorkingDirectory -p DropInPaths` before changing paths. Preserve its configuration path, credential overrides, proxy settings and lifecycle controls. For example, put a final override at `~/.config/systemd/user/morphiecore.service.d/30-binary-release.conf`:

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
: "${HOST:?Set HOST to the target SSH host or alias}"
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

Check the running process's executable, loopback listener and authenticated client behavior before removing an older release. Rollback selects a retained release with the same atomic symlink switch and restarts the service. Never copy over a running executable in place.

Initial migration may remove the former source/build directory only after successful activation and confirmation that configuration, credentials and service paths are independent of it. Former Nix builds can also leave project source copies in the store; identify those copies and remove only those paths with `nix-store --delete` after releasing their project-owned GC roots. Do not synchronize the checkout, use a broad `rsync --delete` against application state or run global garbage collection for this cleanup.

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

## Diagnostics receivers

### Loopback file receiver

The minimal receiver is official `otelcol` with its OTLP HTTP receiver, memory limiter and file exporter. [Release pin and source](../references.md#telemetry-dependency-selection) own the selected version. Use [collector configuration](../../config/otelcol.example.yaml) and [user service](../../config/otelcol.example.service); no container, Collector-to-backend forwarding, external listener or custom receiver is required.

Install the verified binary at `~/.local/lib/morphiecore-otel/0.165.0/otelcol`, create `~/.local/share/morphiecore-otel/logs` with mode `0700`, and copy the configuration to `~/.local/share/morphiecore-otel/collector.yaml`. Copy the unit to `~/.config/systemd/user/morphiecore-otel.service`. Keep configuration/unit files private. Validate from the configured working directory before enabling:

```sh
cd ~/.local/share/morphiecore-otel
~/.local/lib/morphiecore-otel/0.165.0/otelcol validate --config=collector.yaml
systemctl --user daemon-reload
systemctl --user enable --now morphiecore-otel.service
```

The template listens only on `127.0.0.1:4318` and disables Collector metrics serving. Point MorphieCore diagnostics there with `private_bodies:false`. The file exporter is alpha; verify reception, restart preservation, rotation, access and isolated write failures with synthetic records before relying on it.

The selected exporter creates mode-`0644` files on rotation despite the service umask. **The mode-`0700` parent directory is the access boundary**; do not move those files into a shared directory. Logs and Traces each retain one current file and at most eight 16-MiB backups. Cleanup may transiently retain extra backups. `max_days` applies only to rotated files and is not strict TTL. Neither HTTP acceptance nor successful file writes promise fsync durability or zero loss.

Files contain newline-delimited OTLP JSON **batches**, not one event per line. Query through SSH, bound the time/row range, and select metadata instead of printing bodies. For example, show the last 20 records' metadata with Python:

```sh
python3 - <<'PY'
import collections, json, pathlib
rows = collections.deque(maxlen=20)
root = pathlib.Path.home() / ".local/share/morphiecore-otel/logs"
for path in sorted(root.glob("generation*.jsonl"), key=lambda p: p.stat().st_mtime):
    with path.open() as source:
        for line in source:
            for resource in json.loads(line).get("resourceLogs", []):
                for scope in resource.get("scopeLogs", []):
                    for record in scope.get("logRecords", []):
                        rows.append({key: record[key] for key in
                                     ("timeUnixNano", "eventName", "severityText", "traceId", "spanId", "attributes")
                                     if key in record})
for row in rows:
    print(json.dumps(row))
PY
```

### Indexed development query

Use only synthetic traffic and a disposable trusted backend. In a private scratch copy of the [Collector example](../../config/otelcol.example.yaml), add an `otlp_http` exporter for the local development backend to both pipelines. Keep the Collector and the backend's published ports on loopback. The file-only receiver does not answer indexed queries.

With the Collector validated and running, use the existing gateway failure fixture. It disables private capture and rejects non-loopback export destinations:

```sh
MORPHIECORE_TEST_INDEXED_OTLP=http://127.0.0.1:4318 \
  cargo test --locked --offline --test gateway \
  bootstrap::emit_failure_for_indexed_query -- --ignored --exact
```

With the [pinned development backend](../references.md#telemetry-dependency-selection), query Loki with `{service_name="morphiecore"} | stage="intake"`, bounded to the fixture's time window and a small row limit. Confirm `error_type=recovery.request`; use the indexed log's `trace_id` to open the Tempo trace and its `span_id` to select `gateway.attempt`. Follow `parentSpanId` to `gateway.operation` and `gateway.request`. When querying the Tempo JSON API directly, normalize base64 IDs to hex before comparison. The test alone sends the fixture; the query verifies indexing and lineage. Stop the test backend and discard its scratch data afterward.
