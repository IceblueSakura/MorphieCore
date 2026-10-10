# ADR 0011: Stable Admission and Provider-owned Cache Affinity

## Status

Accepted.

## Decision

- Canonical models are explicitly declared. Public labels select fixed Routes; each Endpoint binds its own upstream spelling, adapter and contracts.
- Public semantic admission and client response contracts remain independent of Endpoint replacement. Context, identity, cache, media limits and execution retain separate owners.
- Library and Gateway share one pure bounded candidate selector that checks final whole requests and emits closed rejection categories.
- Cache affinity uses Provider automatic caching and declared carriers. Preserve deterministic projection, required history and surviving tool/Schema order.
- Stateless request correctness is independent of Gateway optimization state. A bounded, disposable prefix-affinity index may assist declared carriers within one Provider/model and compatible authorization scope; misses or index failures do not restore history or alter candidate/credential selection.
- Identity and cache hints are independent. Advisory omission requires a named policy; behavioral controls require target admission. Usage and echoes remain actual reports.

## Rationale and consequences

Stable admission avoids public-contract drift when Providers change. Provider-owned caching requires no answer cache or cross-request routing ownership. Explicit grouping is the stable option; implicit affinity is best-effort, not a promise of maximum hit rate or cross-Provider reuse. The Gateway's separately planned history service owns stateful correctness, never the affinity index. Prefix proofs check declared dependencies; cache hits and benefits require separate observation.

Owners: [catalog](../../../src/topology/catalog.rs), [selector](../../../src/execution/plan.rs), [cache projection](../../../src/protocol/cache.rs), [prefix encoding](../../../src/protocol/cache_affinity.rs), [disposable index](../../../src/gateway/affinity.rs), [prefix proof](../../../src/semantic/cache.rs) and [adapter request](../../../src/adapter/request.rs).

Lifecycle contracts: [implicit affinity](../protocol-and-lowering.md#implicit-cache-affinity) and [history authority](../interaction-contract.md#context-authority). Index activation does not implement or authorize the separately planned stateful API.
