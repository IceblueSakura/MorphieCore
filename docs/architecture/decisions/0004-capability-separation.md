# ADR 0004: Separate Semantic, Representation, Execution and Public Capabilities

## Status

Accepted.

## Decision

Semantic, representation, execution and public contracts are separate. Requirements derive from final Task IR and delivery intent; topology compilation validates the relationships between contracts.

## Rationale and consequences

Model meaning, wire encoding, executable resources and public promises answer different questions. Candidate admission checks the whole typed request rather than a generic bitset, JSON filter or capability union.

Owner: [capability contract](../protocol-and-lowering.md#能力与固定目标). Implementation: [semantic contract](../../../src/semantic/task/generation/contract.rs), [representation](../../../src/lowering/generation.rs) and [topology](../../../src/topology/mod.rs).
