# ADR 0006: Reasoning and Source-bound Replay Ownership

## Status

Accepted design. The existing Responses replay implementation is one profile, not the universal opaque-value contract.

## Decision

- Reasoning controls, readable content/summary, reported usage and opaque continuation are independent owners. Mode, effort, numeric budget and display intent are not interchangeable controls.
- Replay values have a specific type and surviving attachment: reasoning, part, call, resource or a declared content group. Different issuers' encrypted content, signatures and redacted blocks are not interchangeable strings.
- One owner holds each authoritative value. Bounded source/fidelity records may prove origin and dependencies but cannot supply a competing payload or restore deleted values. Dependency scope includes order, grouping or settings where the owning contract requires it.
- Replay requires compatible trusted scope and finality of both value and owner. Unknown origin permits only explicitly admitted intake, not replay. A local dependency check is not issuer authentication or cryptographic verification.
- Final events replace partial values, including explicit removal. Each opaque type defines its own finalization boundary; the Responses item-done rule cannot be copied to unrelated protocols by assumption.
- Static/history/event mappings must preserve the same dependencies, including delivery through the client and return on the next request. Request-local retention alone cannot provide stateless continuation.

## Rationale and consequences

A reasoning-only replay slot cannot express all Agent continuity constraints. Separating common dependency rules from issuer-specific values preserves extensibility without inventing a universal token or a second Provider IR. Editing a dependency invalidates replay unless an explicit preservation rule proves otherwise; deleted owners cannot be recreated from fidelity.

Block closure, value validity, owner completion, response termination and logical-turn progress are distinct. A completed owner may survive a later incomplete response, but a terminated reducer cannot resume. Missing required replay makes continuation unrepresentable; it does not authorize signature fabrication, implicit history loss or cross-account fallback.

Detailed owner and transform contract: [semantic IR](../semantic-ir.md). Current implementation: [reasoning owners](../../../src/semantic/task/generation/reasoning.rs), [format-bound values](../../../src/semantic/task/generation/replay.rs), [reducer](../../../src/semantic/task/generation/event.rs), [fidelity](../../../src/protocol/fidelity.rs), [Chat carrier](../../../src/protocol/openai/chat_reasoning.rs). Existing admission remains in [Responses](../responses-text-profile.md) / [Chat](../chat-text-profile.md), with the [client Generation boundary](../client-generation-profile.md) owned separately; unimplemented domains remain [gaps](../../implementation-status/generation.md).
