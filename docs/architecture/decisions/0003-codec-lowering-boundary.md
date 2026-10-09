# ADR 0003: Separate Protocol Codec from Endpoint Lowering

## Status

Accepted.

## Decision

Codecs own wire syntax, structural validation and declared mappings. Lowering checks representability and named conversion policies for one fixed target. Both remain pure: registry, credentials and I/O belong to other layers.

Projection starts from immutable semantics, validates the resulting view and its dependencies, then encodes. Unsupported meaning fails unless a named policy admits its loss.

## Rationale and consequences

This separation lets endpoints impose narrower constraints without redefining shared semantics. Same-protocol and cross-protocol paths use the same stages; post-encode JSON mutation cannot bypass them.

Owners: [protocol](../../../src/protocol/mod.rs), [lowering](../../../src/lowering/generation.rs) and [projection contract](../protocol-and-lowering.md).
