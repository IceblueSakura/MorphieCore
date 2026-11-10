# Convergence cleanup plan

**Status: implementation in progress, 2026-11-10.** Configuration-error simplification is delivered; the remaining tasks below implement OTel WARN/ERROR diagnostics with request/response context, without requiring JSONL storage. This document does not authorize deployment, credential changes or private-data export.

[Roadmap](roadmap.md#convergence-cleanup) owns priority and stage dependencies. [Architecture](architecture.md#observability) owns implemented contracts and the selected diagnostic goal. [Development](development.md#verification-gates) owns verification procedures and dated acceptance. Keep the detailed cleanup sequence here rather than duplicating it in those documents. Remove completed tasks during execution; retain meaningful decisions in Architecture and concise results in Development.

## Goal and exit

Make a failed operation diagnosable from a small number of OTel log records: identify the request, executing target, attempt, failure stage and reason, then compare relevant client/upstream payload context. Reduce the concepts, state and operational surfaces maintained by the application.

The planned baseline is **Logs-first OTLP export**, native structured OTel records, bounded opt-in private body context and external storage ownership. Full trace trees, per-success execution reports and local JSONL persistence are not prerequisites. Use the existing SDK and body observation points, not a new logging framework.

Exit when:

- WARN/ERROR records expose actual request/task failures, including recovered attempt failures and HTTP-200 failed tasks. Expected cancellation is not misclassified as a system error.
- Enabled private diagnostics provide bounded, sanitized request/response context with explicit omission/truncation/completeness. Disabled mode retains no diagnostic bodies.
- Export failure, saturation and shutdown cannot change inference results, replay decisions, delivery timing or credential lifecycle.
- Application-owned diagnostic/capture files, rolling recovery, reverse OTel serialization and diagnostic-only handoff proof are removed with their exclusive tests and dependencies.
- Current-schema configuration errors remain useful at domain/reason level without a maintained schema-path inventory or rejected values.
- Relevant offline, SDK and installed-client checks pass against the affected tree. Backend delivery is separately demonstrated with synthetic records, not inferred from an in-memory exporter.

## Scope and constraints

### Selected outcomes and planned tradeoffs

| Area | Planned result | Boundary |
|---|---|---|
| Configuration errors | Domain plus fixed safe reason; no precise Serde path | Remove `FIELDS`, path sanitization and its exclusive dependency; never replace them with raw Serde errors |
| Diagnostic transport | One runtime-owned logger provider, standard bounded batch processor and OTLP Logs exporter | No custom durable queue or file fallback; startup/export failures have a simple body-free stderr path |
| Event population | Failures, actionable warnings and background maintenance problems | No default successful-request archive, usage aggregation, metrics pipeline or mandatory trace export |
| Body context | Explicitly enabled private, bounded snippets at existing observation points | No complete exchange archive, binary dump, per-token logs or tool-argument reconstruction |
| Storage | Existing trusted OTLP receiver/backend owns retention and search | No mandatory new Collector/backend product; no application log rotation, adoption or tail repair |
| Adaptation | Preserve public notice and semantic conversion evidence | Remove diagnostic delivery proof, not source facts, public notice bounds or actual acknowledgments |

The old body-free logs and separate capture remain the implemented baseline until their replacement lands. Private diagnostic content must be explicitly enabled and authentication material redacted before export. Business input cannot select exporters, headers, credentials or destinations. OAuth/token endpoint bodies remain excluded.

Keep routing order, same-target retry policy, health/material/affinity fences, source immutability, strict known-field parsing, replay isolation, terminal honesty, cancellation and bounded resources. The selected per-Agent session-file feature is unrelated to diagnostic JSONL removal. R09 remains a separate pre-Stage-4 gate; this plan does not provision its accounts or qualify real failover.

### Remaining gates

These gates stop only their dependent implementation or acceptance:

| Gate | Default / required resolution |
|---|---|
| Export destination | Use test-owned loopback/in-memory exporters during development. Select the trusted receiver, transport and access before external acceptance; production activation requires separate approval |
| Rust exporter compatibility | Verify the current stable `opentelemetry-otlp` release against the locked SDK and chosen client/runtime. Pin the compatible release and features; do not refresh unrelated dependencies |
| Malformed/raw content | Safe baseline emits parser facts, omission markers and last safe SSE units. Verbatim malformed/HTML/plain-text capture needs a separately selected redaction policy; no parse-failed-to-raw fallback |
| Standalone failed-response bodies | Current task receiver returns at a failed head without reading its body. Before claiming task error-body coverage, select a finite byte/time read policy and verify cancellation, or explicitly retain metadata-only coverage |
| External Rust API consumers | Check callers before removing public report/setter APIs. No compatibility shim is required merely for internal/test convenience; report any established external consumer before changing its contract |

## Ownership and file map

| Files | Responsibility after cleanup |
|---|---|
| `src/configuration.rs`, `Cargo.toml`, `Cargo.lock` | Strict bounded intake and coarse, value-free configuration errors; dependency pruning |
| `src/gateway/diagnostics.rs`, `diagnostics/telemetry.rs` | Small current-request/attempt context, event classification and native log emission; no complete historical report or trace-tree owner |
| `src/gateway/diagnostics/export.rs`, `diagnostics/writer.rs` | Delete custom local exporters/reverse conversion and file persistence after their consumers move |
| `src/gateway/capture.rs`, `capture/redaction.rs` | Replace file recording with bounded private context; reuse useful sanitization and safe event framing, not archive sequencing |
| `src/gateway/ingress/http.rs`, `generation/{exchange,intake}.rs`, `tasks/exchange.rs` | Observe consumed client input, actual prepared upstream requests, received responses and polled client output; preserve per-attempt ownership |
| `src/gateway/body.rs`, `src/execution/{report,delivery}.rs` | Keep real handoff/lifecycle/history behavior; separate public notice accumulation from removable diagnostic proof |
| `src/execution/report/usage.rs`, `report/tests.rs` | Remove exclusive diagnostic observations/proof tests; preserve independent semantic accounting and notice checks at their owners |
| `src/gateway/renewal.rs`, `startup/{bootstrap,settings,inspection}.rs`, `runtime.rs`, `mod.rs` | Shared logger lifecycle, current configuration and background failure attribution |
| `config/gateway.example.json`, `src/bin/morphiecore.rs` | Current configuration/example and bounded startup/shutdown behavior, without old-schema aliases |
| `tests/gateway/bootstrap.rs`, `tests/clients/`, `tests/sdk/`, `tests/support/` | Behavioral and exporter acceptance; stop using diagnostic files as the execution oracle |

Inspect complete callers before deleting fields. In particular, `Report::notice/preview_notice` supplies client-visible adaptation notices, and `report.attempt().send_started` participates in retry handling. Move execution state to execution and preserve a bounded current-attempt notice owner before removing report machinery.

## Delivery sequence

Each task is a coherent verified slice. Update affected tests and documentation with that task, not in a final catch-up phase. Do not introduce a legacy/new runtime mode or retain production JSONL solely for test readers.


### 2. Deliver one actionable OTel failure end to end

- **Files:** diagnostics owners, startup/runtime, gateway binary/config, relevant gateway tests and existing client/SDK diagnostic consumers.
- **Consumes:** existing typed failures and trusted request/attempt identity.
- **Produces:** one standard OTLP Logs pipeline and compact event fields: event name, severity, request/Agent, operation/protocol, public/upstream model, Provider/account, attempt, stage, error category/code, HTTP status and retry disposition.
- Start with a synthetic upstream 400/503 through the real gateway. Emit native OTel records; do not serialize a whole `RequestReport` into `AnyValue`. Use scalar attributes and a small structured Body. Keep body snippets as bounded text rather than recursively converting arbitrary business JSON.
- Cut over metadata logging and its test consumers, then delete the local writer/exporters and request/attempt span machinery. Use SDK `InMemoryLogExporter` for owner assertions and an independent loopback OTLP consumer for export acceptance. Existing authority hooks remain the request/attempt-count oracle.
- **Verify:** focused `gateway::diagnostics`, `gateway::startup` and gateway integration tests. Receiver unavailable, queue full and bounded shutdown must not alter the response, send count or cancellation. Inspect a real exported synthetic record, not only a queued record.

### 3. Attach bounded private request/response context

- **Files:** capture/redaction, ingress middleware, Generation intake/exchange, task exchange, startup config and capture tests.
- **Consumes:** Task 2 logger/context and existing four body hooks.
- **Produces:** explicitly enabled request-owned input/output context and fresh attempt-owned upstream context, emitted only with eligible WARN/ERROR records.
- Reuse already available complete JSON where safe; sanitize before extracting a textual prefix. For SSE retain recent complete sanitized events within a shared byte/event budget. Include observed/retained size, incomplete/truncated state and omission reason. Do not duplicate full large request buffers just to obtain a small snippet.
- Keep observation passive: no prefetch, draining, hidden request, argument reassembly or delay to wait for the terminal. Do not treat malformed or split chunks as independently complete JSON. Distinguish safe event retention from an original-wire claim.
- Handle final client output at its middleware/body owner: `Trace::drop` can precede generation of that output. Correlate an additional client-output failure event only when needed; do not add a universal whole-request completion barrier.
- Remove capture-file path, total file quota, writer/channel/flush protocol and archive-only identifiers. Preserve auth redaction, trust and capture memory bounds. Resolve the standalone failed-body gate before promising that coverage.
- **Verify:** focused `gateway::capture`, Generation intake, task exchange and gateway integration checks. Cover split JSON, malformed/truncated input containing synthetic credential fields, late SSE failure, failed first send followed by recovery, and dropped client body. Disabled mode must retain no body context.

### 4. Complete failure semantics and remove diagnostic report state

- **Files:** diagnostics, renewal, execution report/delivery, Generation exchange/intake, body and corresponding tests.
- **Consumes:** compact events and bounded context from Tasks 2–3.
- **Produces:** consistent failure classification plus a minimal public adaptation-notice owner, independent of optional diagnostic output.
- Move `send_started` to attempt execution state. Preserve intake/source/upstream/output evidence selection, notice limits and truncation. Remove historical report snapshots, diagnostic-only usage/event collectors, `Association`/`BatchProgress`, abandonment proof and per-effect delivered-frame counting.
- Keep the actual oneshot acknowledgment, publication fence, delivery commit and history reservation behavior unchanged.
- Route background renewal through the same runtime logger with Provider/account and a closed error category; never log grants, token responses or raw third-party errors.
- **Verify:** `cargo test --locked --offline --lib execution::report`, `cargo test --locked --offline --lib gateway::body`, `cargo test --locked --offline --lib gateway::generation`, `cargo test --locked --offline --lib gateway::renewal`, and `cargo test --locked --offline --test gateway`. Adapt owner selectors if the removed module's meaningful tests move; never retain an empty module just for a selector.

#### Event rules

| Situation | Default diagnostic behavior |
|---|---|
| Client admission rejection; upstream request/rate/quota/permission rejection | WARN with actual boundary/cause; HTTP status alone does not assign blame |
| Failed attempt selected for same-target recovery | WARN with planned retry and attempt identity; never claim recovery before it occurs |
| Unrecovered service/transport failure; malformed response or invalid terminal | ERROR with preserved original evidence |
| HTTP-200 failed task | WARN/ERROR by source failure class, separately recording successful delivery if applicable |
| Ordinary cancellation, disconnect or shutdown | No default ERROR; preserve an independently observed upstream failure |
| Normal length stop, refusal, accepted lossy conversion, ignored unknown fields | No default warning flood |
| Temporary renewal failure | WARN with account/profile/category |
| Identity failure, unusable grant or uncertain persistence | ERROR without authentication payloads |

### 5. Converge acceptance, dependencies and documentation

- **Files:** affected test consumers/helpers, manifests, examples and the fixed document set.
- **Consumes:** passing slices and their surviving public behavior.
- **Produces:** one current configuration, one logger lifecycle and no dormant writer/trace/proof branch.
- Delete tests of withdrawn file formats, tail repair, rolling ownership and diagnostic proof with their exclusive fixtures/helpers. Keep request counts, recovery isolation, cancellation, credential redaction, usage semantics and notice correctness at real owners. Do not recreate successful execution archives just to satisfy old test scripts.
- Remove exclusive dependencies and features after caller checks; retain libraries still used for credential storage or semantic accounting. Update Architecture, Usage, Development and examples together. Preserve dated historical acceptance as historical, not current configuration guidance.
- **Verify:** full [offline baseline](development.md#offline-baseline), all [Python suites](development.md#python-checks), [serial SDK loopbacks](development.md#sdk-loopback-gate), and affected [installed-client synthetic workflows](development.md#installed-client-synthetic-recovery) using current safe launchers. Migrating shared client drivers requires their full synthetic suite. No real Provider sweep is required solely for logging storage removal.
- Record separate verdicts for in-memory log semantics, OTLP delivery, SDK/client behavior and any later real backend/Provider check. Finish with documentation links and `git diff --check`.

## Deferred cleanup candidates

These are findings for later selection, not extra exit requirements: direct enum evidence APIs instead of internal string parsing; constructor-fixed encoder contracts; removal of dedicated historical-store detection; typed credential-store maps; consolidated Responses normalization; remaining SDK-only self-tests. They must not delay the selected diagnostics/configuration work or silently activate a broader rewrite.

Raw malformed payload capture, Windows support removal, complete exchange archives, metrics, trace trees, durable app queues, a new observability backend and production Collector deployment are outside this baseline. Backend persistence can use standard Collector facilities later without restoring application-owned files.

## Implementation references

Checked **2026-11-10** during planning; validate exact crate release/features before implementation.

- [OTel Logs data model](https://opentelemetry.io/docs/specs/otel/logs/data-model/): structured Body, severity and optional trace context. A request ID is sufficient for local diagnostic correlation; do not fabricate an exported trace.
- [Common limits](https://opentelemetry.io/docs/specs/otel/common/): attribute limits do not automatically bound log Body; application snippets and serialized records still need limits.
- [Rust BatchLogProcessor 0.33.1](https://docs.rs/opentelemetry_sdk/0.33.1/opentelemetry_sdk/logs/struct.BatchLogProcessor.html): standard bounded queue/batches and lifecycle. Match exporter client features to its dedicated-thread/runtime requirements; do not put network export in a synchronous request-path simple processor.
- [Rust OTLP exporter](https://docs.rs/opentelemetry-otlp/latest/opentelemetry_otlp/): use the compatible official Logs exporter, not a custom OTLP client. Pin the selected release rather than depending on `latest`.
- [Tracing appender mappings](https://docs.rs/opentelemetry-appender-tracing/latest/opentelemetry_appender_tracing/): useful for scalar events, but `Debug` values become strings. Prefer the already-used native log API for private structured diagnostics; avoid double emission.
- [HTTP semantic conventions](https://opentelemetry.io/docs/specs/semconv/http/http-spans/): body content is opt-in and sensitive; this informs capture policy, not a requirement to retain HTTP spans.
- [Collector resiliency](https://opentelemetry.io/docs/collector/resiliency/): receiver/backend ownership of optional persistent queues. Application export remains best effort; logging failure must not become inference failure.
