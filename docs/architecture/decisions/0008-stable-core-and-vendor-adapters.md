# ADR 0008: One Semantic Core and Explicit Boundary Adapters

## Status

Accepted.

## Decision

- Trusted adapters compose shared codecs and named Provider rules; business input cannot select dialect, origin or target.
- Rules may map aliases, validate derived views, perform exact derivations or apply field-specific defaults with provenance. Defaults preserve valid reports; malformed values fail validation.
- Encoding uses final semantics. Fidelity is bounded and source/profile/owner-dependent, not a competing payload or a way to restore deleted values.
- Compatibility loss follows the [projection contract](../protocol-and-lowering.md#semantic-loss). Instruction authority, controls, call/result association, necessary replay and truthful terminals remain protected.
- Usage mappings declare scope and count relationships; cumulative snapshots are not deltas. Incremental delivery stays bound to one attempt.

## Rationale and consequences

Named boundary rules accommodate real wire differences without splitting the semantic core. Each rule has explicit prerequisites and independent rejection cases. Consumers needing validation before any delivery use bounded complete-result delivery; streaming may discover an error after partial output.

Owners: [Adapter](../../../src/adapter/mod.rs), [WireRules](../../../src/protocol/adaptation.rs), [fidelity](../../../src/protocol/fidelity.rs) and [ResponseDelivery](../../../src/execution/delivery.rs).
