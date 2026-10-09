# ADR 0007: Stateless Cache Affinity and Extension Carrier Ownership

## Status

Accepted.

## Decision

- Cache intent, identity hints and reported response context have separate typed owners. Session, thread, context window, turn and cache affinity are distinct identities.
- Behavioral cache controls declare typed attachment and prefix dependencies. Fidelity preserves representation only.
- Header/body placement projects an owned fact. Carrier mappings validate schema, scope and lifecycle on decode and encode.
- Credentials, account locators and transport authority stay outside context carriers; server-issued turn state has its own lifecycle.

## Rationale and consequences

Wire placement does not determine ownership. Separating identities preserves lifetime meaning and prevents ordinary context from selecting authentication or routing. Sensitive high-cardinality context stays out of ordinary logs and metric labels. Provider cache policy follows [ADR 0011](0011-stable-admission-provider-cache.md).

Owners: [context](../../../src/semantic/context.rs), [extension carriers](../../../src/protocol/extensions.rs) and [adapter request](../../../src/adapter/request.rs). Protocol evidence: [extensions and context](../../references/extensions-and-context.md).
