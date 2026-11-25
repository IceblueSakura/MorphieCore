# Codecs, Dialects, and Lowering Guide (`morphiecore-protocol`, `morphiecore-lowering`)

Applies to `crates/morphiecore-protocol/` and `crates/morphiecore-lowering/` (inherits [crates/AGENTS.md](../AGENTS.md) and [AGENTS.md](../../AGENTS.md)).

## Purity and representation

- Keep `morphiecore-lowering` and `morphiecore-protocol` (including all adapters) strictly pure in-memory transformations with no network, clock, or filesystem I/O.
- Wire spelling stays inside codecs — never introduce vendor bags or duplicate semantic containers.

## Field handling and conversion ladder

- **Field handling:** Keep known fields strict and discard unknown client extension fields without IR retention or Provider passthrough ([field handling](../../docs/architecture/adaptation.md#field-handling)).
- **Conversion ladder ([conversion ladder](../../docs/architecture/adaptation.md#conversion-ladder)):**
  - Admit by default without requiring prior equivalence evidence.
  - Delegate unknown upstream behavior to upstream validation instead of pre-rejecting.
  - Reject only when omitting an inexpressible field would let the client mistake the response for satisfying its request.
  - Never weaken trust, bounds, malformed-known-value checks, identities, or terminal honesty.
  - Lowering a content field or explicit guarantee below rejection requires explicit user selection; same-semantic and lossy rules need a named rule and disposition-table entry (not per-rule confirmation).

## Public baselines and diagnostics

- Public APIs default to official baselines — add no public replay profiles or client-specific semantic retention extensions; Provider dialects remain upstream-only.
- Diagnostic output is optional, uses protocol-permitted extension positions, and never requires client echo.
