# Backlog, Deferred Work, and Frozen Scope

This page records pending decisions, paused capability investigations, deferred operations, and explicit non-goals (frozen scope) to preserve architectural boundaries.

See [Roadmap Main](index.md) for active execution priorities and [Unscheduled issues](unscheduled-issues.md) for known limitations.

## Contents

- [Pending decisions](#pending-decisions)
  - [Ownership and projection decisions](#ownership-and-projection-decisions)
- [Deferred work](#deferred-work)
  - [Google API pause](#google-api-pause)
  - [Deferred Generation semantics](#deferred-generation-semantics)
  - [Deferred cache prewarming](#deferred-cache-prewarming)
  - [Deferred tool discovery and web search](#deferred-tool-discovery-and-web-search)
  - [Deferred token counting](#deferred-token-counting)
  - [Models discovery interfaces](#models-discovery-interfaces)
  - [Messages citations](#messages-citations)
- [Deferred acceptance](#deferred-acceptance)
  - [Target clients](#target-clients)
  - [Only if needed](#only-if-needed)
- [Frozen scope](#frozen-scope)
  - [Not planned](#not-planned)
  - [Maintenance rules](#maintenance-rules)

## Pending decisions

### Ownership and projection decisions

| Issue | Status | Action |
|---|---|---|
| Namespace aliases, custom text and examples | Conditional S3-T5–S3-T7 | Select each content/interface adaptation separately; do not make all three implicit core prerequisites |
| Ordered control null/reset | Optional-null reset/default meanings remain unqualified | Qualify field-specific semantics before broadening admission; retain current null rejections |
| Complete citations | Stage 5 | Retain local point-anchor owner; defer full citation model |
| Execution controls | Paused | Enforce pre-dispatch validation; reject unsupported controls |
| Google error envelopes | Paused | Maintain per-API classification; avoid speculative unified parsing |
| Google probability reports | Paused | Keep existing text-report exceptions; defer broader token scores |

## Deferred work

### Google API pause

Further general Google API expansion remains paused. Maintenance of qualified existing paths needs a concrete selected defect; residual strict/replay boundaries follow the [unscheduled issue register](unscheduled-issues.md#unscheduled-issues). Standard-only downstream APIs retain explicit ordinary-signature rejection; no private replay-profile or OpenCode adaptation is scheduled. Preserve the existing [terminal-report exceptions](../architecture/adaptation-and-dialects.md#service-policies-and-reports) and [native text Embedding path](../architecture/index.md#independent-embedding-path). General multimodal input, native client intake, custom tools, unified error parsing and broader probability reports remain excluded.

### Deferred Generation semantics
- Turn-scoped system messages (`clear_at`).
- Generated audio/video inside Chat (beyond basic static audio).
- Chat completion persistence (`store: true`); local managed Responses history is already supported.
- Tokenizer-unbound `logit_bias`.
- Multi-candidate (`n > 1`) execution.

### Deferred cache prewarming
Native no-output cache prewarming is unscheduled and remains separate from standard request execution.

### Deferred tool discovery and web search
Dynamic tool discovery and native Provider web search remain unscheduled Stage 5 tasks.

### Deferred token counting
`POST /v1/messages/count_tokens` is deferred. The gateway returns 404 rather than generating synthetic or estimated counts. Reopening requires defining upstream reliability criteria and coarse estimation provenance.

### Models discovery interfaces
Discovery for other task families (Embeddings, Media, Decision) remains schema-only until those operations are publicly activated. Extend the existing [Generation discovery contract](../architecture/adaptation-and-dialects.md#generation-text-model-discovery) only with selected operation contracts.

### Messages citations
Unified citation delivery ([1C proposal](../proposals/stage5-backlog.md#1c-design-messages-citations)) remains unscheduled. Requests requiring citation output are rejected.

## Deferred acceptance

| Target | Condition for acceptance |
|---|---|
| Non-image multimodal real checks | Stage-4 modality slice implementation |
| Current-client image replay | Selected image client profile verification |
| Real multi-provider failover | [R09](unscheduled-issues.md#routing-qualification) prerequisites: dedicated credential pairs and reproducible fault injection |
| Claude Code acceptance | Verification of beta headers, counting, and citation contracts |

### Target clients
`pi` and `opencode` are the primary verified clients. Their parser behavior, stream consumption, and history replay serve as acceptance oracles.

### Only if needed
- SSE keep-alive comments during prolonged processing delays.
- Conversation prefix-derived executor affinity when no conversation ID is provided.
- Caller-configured first-content timeout thresholds.

## Frozen scope

The following areas are frozen against incidental modification:
- Standalone media endpoints prior to their respective Stage 4 slices.
- Unregistered Provider bindings and subscription-token workflows.
- Server-side compaction, Batch, and Realtime prior to Stage 5 scheduling.

### Not planned
- Direct inference-engine integration (llama.cpp, SGLang, vLLM).
- Raw wire passthrough, post-encoding JSON monkey-patching, or weakened schemas.
- Synthetic tool result generation or orphan role remapping.
- Conversation hashing or LLM-based prompt routing.
- Gateway-hosted plugin execution or automatic capability inference from upstream errors.
- Provider-managed conversation persistence or background generation tasks.
- Ingress load balancing, concurrency leasing, or token-rate reservation.
- Refusal retries across alternative models.
- Sub-turn routing, branch switching, or dynamic effort modulation.

### Maintenance rules
Vendor-specific stream behaviors become named dialect rules only when emitted by a configured, bound upstream. Upstream in-stream errors are mapped to classified terminal errors. Standalone media paths are not modified during Generation Text maintenance.
