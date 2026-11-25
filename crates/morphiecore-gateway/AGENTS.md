# Gateway and Runtime Trust Guide (`morphiecore-gateway`, `morphiecore-runtime`, `src/bin`)

Applies to `crates/morphiecore-gateway/`, `crates/morphiecore-runtime/`, and `src/bin/` (inherits [crates/AGENTS.md](../AGENTS.md) and [AGENTS.md](../../AGENTS.md)).

## Gateway/runtime boundary

- Apply [system boundaries](../../docs/architecture/index.md#system-boundaries) — reject target limitations requiring execution-strategy changes rather than silently disabling thinking or repairing a session.
- Enforce the [stateful boundary](../../docs/architecture/history-and-replay.md#stateful-history): reject downstream storage, reference-based continuation, and stored-resource operations; keep omitted/null/false `store` stateless while preserving full-history replay.

## Runtime trust invariants

- Preserve loopback-only ingress, fail-closed authentication, and trusted target selection:
  - Business data never selects origins, credentials, or auth/proxy headers.
  - Credential locators and trusted origins never reach clients.
- Enforce resource and time bounds across intake, upstream transport, media normalization, and telemetry export.
- Release upstreams promptly on downstream cancellation, never replay a request after client publication has begun, and never fabricate successful termination or missing usage.

## Routing and failover

- Keep routing strictly within [failover boundaries](../../docs/architecture/runtime-and-routing.md#failover): request-boundary cache-affine selection with at most one pre-publication same-target retry, never post-dispatch target switching or load balancing.
