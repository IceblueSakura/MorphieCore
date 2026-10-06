# ADR 0001: Agent-first, Protocol-neutral Semantic Authority

## Status

Accepted design direction. Current codec admission and implementation gaps remain separately documented.

## Decision

MorphieCore has one independent Semantic Model for model API interactions; IR is its typed representation. Gateway and a future Agent share that authority. Generation owns ordered content, actions/results and continuation dependencies; Embedding has a distinct task contract, not a message variant. OpenAI Responses is the primary design reference; Google Gemini and Anthropic Messages provide supplementary semantic checks and projection targets. The IR remains independent: no protocol defines its expressiveness ceiling.

Keep ordered heterogeneous items, stable identities and explicit lifecycles where they express independent concepts. Do not clone a protocol union, SDK DTO, common denominator or mechanical field union. Shared concepts have one owner; scoped typed extensions preserve genuinely source-specific meaning without creating a Provider IR.

Public Generation targets standard OpenAI Responses; Embedding uses standard OpenAI Embeddings. Chat Completions is a required compatibility projection that may incur declared semantic loss under the [projection contract](../protocol-and-lowering.md#semantic-loss), without narrowing the core. Custom client extensions are bounded exceptions, not the default route to completeness. Missing semantic capacity must be reported with structural options before implementation; missing standard wire capacity must not be disguised as an IR value or an adapter workaround. The detailed decision boundary belongs to [semantic IR](../semantic-ir.md#4-ir-不足与标准载体缺口).

Same-protocol and cross-protocol paths decode, validate/transform, derive requirements, lower for a fixed target and encode. There is no Native bypass or post-encode semantic JSON mutation. Representability, executable capability and public admission remain separate.

## Rationale and consequences

MorphieCore is pre-release and in design exploration. Stability belongs to concepts, ownership and invariants, not current Rust shapes. New designs are evaluated against independent task contracts and selected consumers rather than incumbent implementation convenience. Future Agent planning, memory and tool orchestration remain outside model-interaction semantics. Realtime detailed design is deferred; no speculative session framework is required to stabilize the current request-based scope.

A narrow target must reject or explicitly convert unsupported meaning, not redefine the IR. Protocol neutrality does not erase existing Responses semantics or promise arbitrary lossless translation. Other tasks keep their own contracts; Agent-first representation does not authorize orchestration, storage or tools. Runtime secrets, selected targets and retry/commit state stay outside IR.

Detailed design owner: [semantic IR](../semantic-ir.md). Current implementation: [Generation](../../../src/semantic/task/generation/mod.rs), [adapters](../../../src/adapter/mod.rs); [gaps](../../implementation-status/generation.md). Sources: [reference index](../../references/README.md). Design changes do not silently change existing wire profiles.
