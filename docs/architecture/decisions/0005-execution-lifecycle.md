# ADR 0005: Execution Is Semantically Blind After Encoding

## Status

Accepted.

## Decision

Execution consumes compiled candidate plans and encoded representations. It owns credential binding, attempts, cancellation, transport and delivery lifecycle, not semantic mutation. Response bytes/events re-enter semantics through the selected adapter before downstream delivery.

Retry/fallback must remain bounded and pre-commit, preserving fixed route order and immutable semantic input. Encoding bytes does not constitute downstream commit.

## Rationale and consequences

Keeping execution out of field conversion prevents late mutations from bypassing validation. I/O owners explicitly acknowledge visibility and completion; failure or cancellation cannot become a successful terminal. HTTP publication may freeze candidate advancement earlier than commit, as required by [ADR 0010](0010-canonical-model-fixed-fallback.md).

Owners: [execution](../../../src/execution/mod.rs), [delivery](../../../src/execution/delivery.rs), [HTTP body](../../../src/gateway/body.rs). Detailed contracts: [execution model](../execution-model.md).
