# ADR 0011: Stable Admission and Provider-owned Cache Affinity

## Status

Accepted.

## Decision

- Canonical models are explicitly declared. Public labels select fixed Routes; each Endpoint binds its own upstream spelling, adapter and contracts.
- Public semantic admission and client response contracts remain independent of Endpoint replacement. Context, identity, cache, media limits and execution retain separate owners.
- Library and Gateway share one pure bounded candidate selector that checks final whole requests and emits closed rejection categories.
- Cache affinity uses Provider automatic caching and declared carriers. Preserve deterministic projection, required history and surviving tool/Schema order.
- Identity and cache hints are independent. Advisory omission requires a named policy; behavioral controls require target admission. Usage and echoes remain actual reports.

## Rationale and consequences

Stable admission avoids public-contract drift when Providers change. Provider-owned caching requires neither an answer cache nor cross-request session/routing ownership. Prefix proofs check declared dependencies; cache hits and benefits require separate observation.

Owners: [catalog](../../../src/topology/catalog.rs), [selector](../../../src/execution/plan.rs), [cache projection](../../../src/protocol/cache.rs), [prefix proof](../../../src/semantic/cache.rs) and [adapter request](../../../src/adapter/request.rs).
