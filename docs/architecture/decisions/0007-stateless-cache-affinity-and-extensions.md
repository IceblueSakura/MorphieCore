# ADR 0007: Stateless Cache Affinity and Extension Carrier Ownership

## Status

Accepted. Provider cache projection is refined by [ADR 0011](0011-stable-admission-provider-cache.md).

## Decision

- Cache intent, identity hints and reported response context have separate typed owners. Affinity hints, prefix breakpoints/policies and remote cache-resource references are not aliases. Behavioral cache controls require typed context/attachment and declared prefix dependencies, not representation-only fidelity. Codecs do not synthesize echoes or derive one identifier from another.
- Logical session, cache affinity, thread, context window and turn state are distinct facts. No universal session ID or fabricated sticky token substitutes for a real owner.
- Header/body placement is a protocol projection of typed context or scoped extension owners, not another semantic authority. Existing protocol-layer carriers do not define the architecture ceiling. Typed lifecycle fields and bounded opaque values require declared admission and target mappings; arbitrary header/body passthrough is forbidden.
- Credential, account-locator and transport fields never enter context carriers. Server-issued turn state is opaque and codec-inert; extending or replaying it across turns requires an explicit lifecycle owner.

## Rationale and consequences

Wire location does not determine semantic ownership. Conflating cache/session identifiers loses lifecycle meaning; generic headers create an authentication-injection path. Validate carriers on both decode and encode, including hand-built values. Sensitive high-cardinality context does not belong in logs or metric labels.

Carrier availability does not imply HTTP wiring or session management. Owners: [standard context](../../../src/semantic/context.rs), [extensions](../../../src/protocol/extensions.rs), [adapter request](../../../src/adapter/request.rs). Source provenance: [fixed Codex baseline](../../references/upstream-sync.md).
