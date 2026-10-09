# ADR 0001: Agent-first, Protocol-neutral Semantic Authority

## Status

Accepted.

## Decision

- One independent Semantic Model serves Gateway and future Agent consumers; IR is its typed representation.
- OpenAI Responses is the primary Generation reference and client API. Google Interactions and Anthropic Messages supply supplementary semantic checks. Protocol DTOs do not define the core's expressiveness.
- Independent operations have distinct task contracts, including Embedding. Agent planning, memory and tool orchestration belong to the caller.
- Both same-protocol and cross-protocol requests use validation, requirements, fixed-target lowering and encoding. Runtime targets, credentials and retry/commit state stay outside IR.

## Rationale and consequences

A shared semantic authority permits low-loss mapping without maintaining Provider-specific cores. Concepts, ownership and invariants are stable; Rust shapes may change within an approved migration. Chat is a compatibility projection governed by the [loss contract](../protocol-and-lowering.md#semantic-loss). Missing IR concepts and missing target carriers are handled separately.

Owner: [Semantic Model](../semantic-ir.md). Implementation: [semantic](../../../src/semantic/task/generation/mod.rs) and [adapters](../../../src/adapter/mod.rs). Sources: [reference index](../../references/README.md).
