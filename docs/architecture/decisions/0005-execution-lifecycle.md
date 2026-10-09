# ADR 0005: Execution Is Semantically Blind After Encoding

## Status

Accepted.

## Decision

Execution owns compiled attempts, credential binding, cancellation, transport and delivery. Encoded requests are not mutated; responses re-enter semantics through the selected adapter.

Retry/fallback uses immutable input, fixed candidate order and bounded pre-publication advancement. Encoding, publication, acknowledged handoff/commit and completion are distinct stages.

## Rationale and consequences

Explicit I/O ownership prevents late mutation from bypassing validation. Validated closure and final handoff establish completion; failures abort rather than fabricate success. [ADR 0010](0010-canonical-model-fixed-fallback.md) defines candidate advancement.

Owner: [execution model](../execution-model.md). Implementation: [execution](../../../src/execution/mod.rs), [delivery](../../../src/execution/delivery.rs) and [HTTP body](../../../src/gateway/body.rs).
