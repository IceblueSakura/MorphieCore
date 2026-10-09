# ADR 0002: Task IR, Identity, Presence and Extensions

## Status

Accepted.

## Decision

- Generation uses ordered heterogeneous items and bounded typed ownership, grouping, call/result and resource relations. Other tasks retain their own request, result and event contracts.
- Local identities, wire IDs and indexes are distinct. Presence is field-specific.
- Task semantics, context, delivery, scoped extensions and fidelity have separate owners.
- Response closure, artifact completeness, logical-turn progress and continuation requirements are distinct.
- Extensions declare schema, attachment, origin, lifecycle, visibility and target requirements. Fidelity retains representation evidence, not a second payload.

## Rationale and consequences

Explicit ownership keeps edits and reordering from attaching data to the wrong value. Changes invalidate affected dependencies; dangling references require repair or rejection. Normalization preserves required order, precision, resource meaning and lifetime. Replay follows [ADR 0006](0006-reasoning-ownership.md).

Owners: [Semantic Model](../semantic-ir.md), [Generation types](../../../src/semantic/task/generation/mod.rs), [context](../../../src/semantic/context.rs) and [fidelity](../../../src/protocol/fidelity.rs).
