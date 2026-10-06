# ADR 0008: One Semantic Core and Explicit Boundary Adapters

## Status

Accepted. Field-specific compatibility defaults are allowed under explicit rules; there is no single permissive global Chat/Responses profile.

## Decision

- Keep one independent, Agent-first semantic core, primarily informed by OpenAI Responses. A different wire spelling changes an adapter; a genuinely missing capability requires a reported IR gap and analysis of shared structural options before implementation. Do not create Provider-specific Generation or Usage types.
- Chat Completions compatibility may lose declared semantics under the [loss contract](../protocol-and-lowering.md#semantic-loss); it is not limited to exact mappings. Each rule defines direction, affected owners, consequences, dependency checks and independent static/event expectations. Existing strict behavior remains until a scoped implementation replaces it.
- No adapter may invent content, use arbitrary dropping to hide missing IR, impersonate media/vector as text, or smuggle payloads through fidelity/custom fields. Unapproved losses remain failures; protected controls, call/replay dependencies and truthful terminals cannot be sacrificed for compatibility. Standard wire compliance and semantic fidelity are separate claims.
- Trusted client and Provider adapters compose shared protocol codecs. Business JSON cannot select dialect, origin or target; no generic body hooks or dynamic plugin framework are introduced.
- Named rules may map aliases, validate/discard derived views, exactly derive values or supply field-specific compatibility defaults. Defaults are not reported observations: preserve valid reports, reject malformed values, retain absence unless the named rule explicitly permits normalization, and record provenance outside task semantics.
- Encoding consumes final semantics without reapplying intake defaults. Never fabricate reported call identities, arguments, a whole missing usage object or a successful terminal. Local identity allocation must remain distinct from upstream identity and cannot repair missing required wire fields.
- Usage mappings declare scope, overlapping/disjoint relationships and prerequisites for exact derivation; no universal arithmetic rule is inferred from wire field names. Derived views never compete with reported values, and cumulative snapshots are not summed as deltas.
- Fidelity is bounded, source/profile/owner-dependent representation, not a second task payload. Classified extras use the authoritative terminal snapshot; incompatible or stale extras cannot restore edited semantics. Opaque replay keeps its stricter rejection contract.
- SSE intake/projection remains incremental and bounded. One delivery binds to one execution attempt, not a reusable wire response ID. Encoding is not commit; late failures abort rather than splice another attempt or synthesize success.

## Rationale and consequences

Explicit adapters absorb real wire differences without weakening the standard codec or splitting semantic authority. Each normalization has a narrow contract and independent rejection cases. Target completeness checks may fail at terminal after earlier output; pre-first-byte completeness requires bounded complete-result delivery instead of incremental streaming.

Exact rule selection, defaults and allow-lists live in [Adapter](../../../src/adapter/mod.rs), [WireRules](../../../src/protocol/adaptation.rs) and owning codecs. [Fidelity](../../../src/protocol/fidelity.rs), [Attempt](../../../src/execution/attempt.rs) and [ResponseDelivery](../../../src/execution/delivery.rs) own dependency and lifecycle enforcement. Do not duplicate their field tables or test results here.
