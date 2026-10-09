# ADR 0010: Canonical Models and Fixed Pre-publication Fallback

## Status

Accepted.

## Decision

- Canonical model/task identity is independent of Provider aliases. Public admission may be narrower; compiled Routes contain only compatible fixed candidates.
- Each candidate projects the whole request from the same immutable IR, in declared order, under one permit, attempt budget and absolute deadline.
- Fallback is opt-in and advances only on eligible pre-publication failures. Endpoint/pool limits narrow the chain; no candidate is repeated, hedged or dynamically added.
- First downstream-frame publication freezes advancement. Handoff acknowledges commit; strict closure and final handoff establish completion. Late errors abort the body.
- Local unavailability, storage/security errors and upstream authorization/rate-limit scope are distinct. Unknown-scope rate limits do not authorize same-Provider credential switching.
- Canonical equality does not authorize opaque replay across auth/issuer scopes. Without explicit affinity ownership, multi-member ingress rejects source-bound opaque history and encrypted-output requests.

## Rationale and consequences

Fixed selection provides bounded recovery without dynamic routing or contract drift. Publication closes advancement races before acknowledged handoff. Fallback may duplicate upstream work/billing, and local cancellation is not proof of remote termination.

Owners: [topology](../../../src/topology/compile.rs), [plan](../../../src/execution/plan.rs), [fallback policy](../../../src/execution/fallback.rs), [exchange](../../../src/gateway/exchange.rs) and [body](../../../src/gateway/body.rs). Pool ownership: [ADR 0012](0012-grok-personal-credential-pool.md).
