# Convergence cleanup plan

**Status: selected implementation delivered, 2026-11-10.** Configuration errors, native failure Logs, bounded private context and diagnostic report/proof removal are complete. Finished tasks are removed; Git retains the implementation sequence.

[Architecture](architecture.md#observability) owns accepted contracts and deliberate losses. [Usage](usage.md#failure-diagnostics) owns current configuration. [Development](development.md#diagnostics-behavior-checks) owns procedures and dated acceptance. [Roadmap](roadmap.md#planned-order) owns priority and the independent R09 pre-Stage-4 gate. This document grants no deployment, credential or private-data export authorization.

## Remaining boundaries and gates

| Boundary | Current disposition |
|---|---|
| Production receiver and access | Development uses test-owned in-memory/loopback consumers. Select the trusted receiver, authentication boundary and retention before external acceptance; deployment requires separate approval |
| Backend persistence | Application export is best effort. Durable queues, indexing and access belong to the selected receiver/backend, not an application file fallback |
| Raw malformed content | Retain parser/omission facts and last safe events. Verbatim malformed/HTML/plain-text context needs a separately selected redaction policy |
| Standalone failed-response bodies | Keep metadata-only coverage and immediate failed-head return. Reading error bodies needs a separately selected finite byte/time/cancellation policy |
| Out-of-tree Rust consumers | Repository callers of removed report/capture APIs are migrated. No external consumer inventory or compatibility promise is claimed |

These boundaries do not reopen the delivered application cleanup. Complete successful exchange archives, exact live-wire/attempt attestation, metrics and mandatory trace export remain outside the selected baseline. Synthetic authorities and client-visible outcomes replace successful logs as verification oracles.

## Deferred cleanup candidates

The following findings need later selection, not completion of this cleanup:

- Direct enum evidence APIs instead of internal string parsing.
- Constructor-fixed encoder contracts.
- Removal of dedicated historical-store detection.
- Typed credential-store maps.
- Consolidated Responses normalization.
- Remaining SDK-only self-tests.

They do not activate a broader rewrite, new APIs, Windows removal, durable application queues or a new observability backend.

## Implementation references

Checked **2026-11-10**; implemented Rust SDK/exporter pins are **0.33.1** in `Cargo.toml`/`Cargo.lock`.

- [OTel Logs data model](https://opentelemetry.io/docs/specs/otel/logs/data-model/): structured Body, severity and optional trace context; request IDs need no fabricated trace.
- [Common limits](https://opentelemetry.io/docs/specs/otel/common/): attribute limits do not bound arbitrary log Body.
- [Rust BatchLogProcessor](https://docs.rs/opentelemetry_sdk/0.33.1/opentelemetry_sdk/logs/struct.BatchLogProcessor.html): bounded batching and lifecycle; explicit off-worker flush and non-waiting last-owner shutdown.
- [Rust OTLP exporter](https://docs.rs/opentelemetry-otlp/0.33.1/opentelemetry_otlp/): official Logs exporter and dedicated-thread blocking HTTP client.
- [HTTP semantic conventions](https://opentelemetry.io/docs/specs/semconv/http/http-spans/): opt-in sensitive body content, without requiring HTTP spans.
- [Collector resiliency](https://opentelemetry.io/docs/collector/resiliency/): optional persistent queues belong to the receiver/backend.
