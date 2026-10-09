# ADR 0006: Reasoning and Source-bound Replay Ownership

## Status

Accepted.

## Decision

- Controls, readable reasoning/summary, usage and opaque continuation are independent values.
- Replay has a format-specific typed owner: reasoning, part, call, resource or declared group. Origin/dependency evidence stays separate from its sole payload.
- Replay requires compatible trusted scope, surviving dependencies and value/owner finality. Editing dependencies invalidates replay unless an explicit preservation rule applies.
- Each format defines finalization, including replacement/removal of partial values. Static, event and history mappings preserve the same dependencies through client delivery and return.

## Rationale and consequences

Format-bound ownership supports continuity without treating different issuers' values as universal tokens. Block closure, response termination and logical-turn progress remain distinct. Source labels and local dependency checks are not cryptographic verification; missing required replay makes continuation unrepresentable.

Owners: [interaction contract](../interaction-contract.md#typed-replay-与信任), [replay values](../../../src/semantic/task/generation/replay.rs), [reducer](../../../src/semantic/task/generation/event.rs) and [fidelity](../../../src/protocol/fidelity.rs). Current wire admission: [Responses](../responses-text-profile.md) and [Chat](../chat-text-profile.md).
