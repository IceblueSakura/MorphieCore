# ADR 0002: Task IR, Identity, Presence and Extensions

## Status

Accepted.

## Decision

Generation uses ordered item/content semantics with explicit, bounded typed relations for ownership, grouping, calls/results and resources; it is neither a universal Message container nor an executable workflow graph. Other tasks retain their own request/response/event contracts. Wire family does not determine task identity, and wire role does not determine tool-result identity.

Separate task semantics, typed context, delivery intent, scoped extensions and bounded fidelity. Shared concepts have one owner, not protocol-specific copies. Local identities are distinct from wire IDs and indexes; unknown turn/group identities are not invented as reported facts. Presence is field-specific, and contradictory outer absence/inner values are invalid.

Response closure, artifact completeness and logical-turn progress are distinct semantic facts with constrained combinations. Continuation requirements describe dependencies for a subsequent operation; they do not authorize automatic execution or reopen a terminal response.

Extensions require explicit attachment, schema, trusted origin, lifecycle, visibility and target requirements. Representation-only preservation stays in fidelity; runtime authority never comes from an extension.

## Rationale and consequences

Stable ownership prevents reordering or transforms from attaching metadata to the wrong value. Dependencies may span a content group or prefix, not just a single owner. Replacing/deleting an owner invalidates dependent values; dangling references require explicit repair or rejection. Normalization must not erase Schema ordering, resource meaning or context lifetime. Opaque replay follows [ADR 0006](0006-reasoning-ownership.md).

Design: [semantic IR](../semantic-ir.md). Implemented types: [Generation](../../../src/semantic/task/generation/mod.rs), [context](../../../src/semantic/context.rs), [fidelity](../../../src/protocol/fidelity.rs), [extension carriers](../../../src/protocol/extensions.rs). These entry points do not imply that every designed task or carrier is wired into the gateway.
