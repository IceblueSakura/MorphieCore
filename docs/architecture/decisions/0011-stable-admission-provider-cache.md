# ADR 0011: Stable Admission and Provider-owned Cache Affinity

## Status

Accepted. Refines [ADR 0007](0007-stateless-cache-affinity-and-extensions.md) and [ADR 0010](0010-canonical-model-fixed-fallback.md).

## Decision

- Register canonical models explicitly once, independently of Provider spelling. Client-visible model labels have no Provider prefix and select only a fixed Route, never a client-supplied Provider. Each Endpoint binds its own upstream model spelling to the shared canonical identity; different Providers may use different names for that identity. Provider bindings declare adapters/protocols rather than duplicating a bootstrap switch.
- Public semantic admission and the client response contract remain independent of upstream aliases and Endpoint replacement. Representation contracts reuse semantic vocabulary, not a second flag model. Context admission, identity, cache projection, media limits and execution policy remain separate owners.
- Library and gateway use one bounded pure candidate selector. It projects whole requests independently and emits closed rejection categories, never raw request values or credential locators.
- Cache affinity uses Provider automatic caching and declared carriers, not an MorphieCore answer cache, load balancer, session manager or sticky router. Preserve deterministic target projection, tool/Schema order and surviving history prefixes; every request still supplies required history.
- Standard cache hints and identity hints remain independent. Unsupported advisory hints may be omitted only by explicit projection policy; behavioral controls and scoped session carriers require target admission or rejection. Never derive one identifier from another or forward arbitrary session headers.
- Response usage and echoes are reported facts, not request copies. Prefix stability does not establish cache hits, cost savings or latency improvement.

## Rationale and consequences

Separating stable public admission from target details avoids contract drift when adding Providers. Provider-owned caching avoids inventing conversation ownership or cross-request routing state. Diagnostics remain bounded and content-free; sink failure cannot affect business responses.

Caller-owned [semantic prefix proofs](../../../src/semantic/cache.rs) can reject changed dependencies before a selected operation; they are not cache directives or proof of Provider hits. Candidate checks must use final binding/context and a compatible trusted representation scope, not just the public label.

Owners: [catalog](../../../src/topology/catalog.rs), [semantic contract](../../../src/semantic/task/generation/contract.rs), [selector](../../../src/execution/plan.rs), [cache projection](../../../src/protocol/cache.rs), [adapter request](../../../src/adapter/request.rs). Candidate advancement, upstream intake and downstream acknowledgement belong respectively to [exchange](../../../src/gateway/exchange.rs), [intake](../../../src/gateway/intake.rs) and [body](../../../src/gateway/body.rs). Exact field rules and budgets stay beside those owners.
