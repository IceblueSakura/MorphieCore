# ADR 0003: Separate Protocol Codec from Endpoint Lowering

## Status

Accepted.

## Decision

Protocol/profile codecs own wire syntax, structural validation and declared extension mappings. Lowering owns representability and explicit conversion policy for a fixed target. Neither performs registry lookup, credential resolution or network I/O.

Unsupported semantics must fail unless a named target policy explicitly admits their loss. Chat compatibility may use such bounded policies; codecs cannot silently decide omissions or fabricate facts. Projection preserves the immutable source, validates its resulting view and dependency effects, and never uses a post-encode body hook.

## Rationale and consequences

Separating syntax from target restrictions prevents a narrow Endpoint from redefining standard semantics. Same-protocol and cross-protocol paths share the same stages. Request shorthand, complete responses and SDK-derived views need distinct validation boundaries.

Owners: [protocol](../../../src/protocol/mod.rs), [lowering](../../../src/lowering/generation.rs), [adapters](../../../src/adapter/mod.rs). Cross-module contract: [protocol and lowering](../protocol-and-lowering.md).
