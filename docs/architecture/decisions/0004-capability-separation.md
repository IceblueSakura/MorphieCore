# ADR 0004: Separate Semantic, Representation, Execution and Public Capabilities

## Status

Accepted.

## Decision

Keep semantic, representation, execution and public contracts separate. Requirements come from final Task IR and delivery intent; no generic capability bitset is authoritative across all dimensions.

## Rationale and consequences

Standard expressiveness, target encoding, executable resources and public admission answer different questions. Topology compilation validates their relationships. Candidate compatibility checks the whole typed request, not ad-hoc JSON filters or unions of candidate capabilities.

Design: [capability and projection contract](../protocol-and-lowering.md#能力与固定目标). Owners: [semantic contract](../../../src/semantic/task/generation/contract.rs), [representation](../../../src/lowering/generation.rs), [topology](../../../src/topology/mod.rs).
