# Development Roadmap

This page owns active scope, planned execution order, pending decisions, deferred work, explicit non-goals (`Not planned`), and release/integration gates. Known qualification gaps and limitations live in [Unscheduled issues](unscheduled-issues.md).

[Architecture](../architecture/index.md) owns accepted design, [API expansion design](../proposals/index.md) owns unselected proposals, and [Development](../development.md) owns shared testing conventions and overall acceptance status. Completed slices are removed; Git preserves history.

## Contents

- [Current status](#current-status)
- [Planned order](#planned-order)
  - [Stage 4 pending scope](#stage-4-pending-scope)
  - [Stage 5 backlog](#stage-5-backlog)
- [Pending decisions](#pending-decisions)
- [Deferred work](#deferred-work)
  - [Deferred Generation semantics](#deferred-generation-semantics)
  - [Deferred acceptance](#deferred-acceptance)
  - [Only if needed](#only-if-needed)
- [Not planned](#not-planned)
- [Integration handoff](#integration-handoff)
  - [Partial-delivery exit](#partial-delivery-exit)
  - [Public route activation review](#public-route-activation-review)

## Current status

- **Stateful API freeze:** Upstream and downstream stateful APIs (`store: true`, reference-based continuation, stored-resource operations) remain [experimental and disabled](../architecture/history-and-replay.md#stateful-history). Exclude their acceptance from model adaptation and issue repairs until explicitly enabled by the user.
- **Delivered Stage 4 slices:** Independent Embeddings (`POST /v1/embeddings`), Static Images Generate & Edit (`POST /v1/images/generations`, `POST /v1/images/edits` with `png`/`jpeg`/`webp` normalization and `completed`-only SSE on static targets), Audio Speech (`POST /v1/audio/speech`), and Audio Transcription (`POST /v1/audio/transcriptions`) are implemented; remaining work in those families covers only unselected extensions below.
- **Pending Stage 4 & Stage 3 items:** Decision API, Files & Uploads, Video, conditional Stage-3 tool follow-ups, [unscheduled issues](unscheduled-issues.md#unscheduled-issues), R09 real-account failover, and opt-in Agent session files advance independently as scheduled.

## Planned order

| Stage | Goal | Exit evidence |
|---|---|---|
| **3 — Conditional compatibility follow-ups** | [Conditional packages](unscheduled-issues.md#conditional-stage-3-follow-ups) | Separate policy selection and affected-loop verification; not default Stage-4 blockers |
| **4 — Pending modalities, Decision & Files** | Decision API, Files/Uploads, Video, and unselected Speech/Transcription/Image extensions | Verified request/result contracts, media bounds, resource isolation, cancellation, and SDK loops |
| **5 — Unscheduled follow-ups** | Citations, tool discovery, hosted tools, server-side compaction, Batch, and Realtime | Explicit per-slice design selection and scheduling prior to implementation |

### Stage 4 pending scope

Every pending slice follows: pin official contracts → implement one coherent slice over immutable source IR → supply independent directional wire↔IR tests & L5 failure checks → run offline baseline & SDK gates → remove completed slices from this page.

| Package | Scope | Exit criteria |
|---|---|---|
| **Decision API** | Dual-SDK (`TypeSafe` + `OpenAI`) backed by unified Decision IR | Resolved schema/discovery envelopes, capability union, bounded execution, and both SDK client loops |
| **Files & Uploads** | File storage/ID authorization, bounded chunked uploads, cleanup, and file-reference resolution | Isolated account/project namespaces, bounded spooling, cancellation cleanup, and target-scoped ID checks |
| **Video** | Async video generation (`grok-imagine-video` / native jobs), polling, and artifact delivery | Selected async job contract, bounded polling, cancellation, and honest completion |
| **Image & Audio extensions** | Optional OpenAI `mask` on Edit, `partial_images` preview SSE on static targets, native ratio/resolution tiers; Audio Translation, progressive non-SSE binary Speech, and subtitle/diarized ASR | Explicit contract selection and conformance per extension; existing static behavior stays protected |

Decision execution order: complete the initial text/JSON SDK profile (Choice, Score, bounded TypeSafe zstd, and native discovery) before service activation. Qualify Gemini native execution and each cross-family direction independently; grouped/truncating targets, media, and sibling primitives do not gate the initial public profile. See the [Decision proposal](../proposals/decisions-api.md#decisions) for the SDK boundary and per-capability oracles.

### Stage 5 backlog

All Stage-5 work remains unscheduled. [Stage 5 proposals](../proposals/stage5-backlog.md) own citations, tool discovery/web search, hosted execution, compaction, Batch, and Realtime designs; inclusion is not activation approval.

## Pending decisions

| Issue | Status | Next step |
|---|---|---|
| Conditional tool adaptations | `generation.alias-tool-namespaces.v1` and `generation.wrap-plain-custom-tools.v1` delivered; remaining [Stage-3 follow-ups](unscheduled-issues.md#conditional-stage-3-follow-ups) unscheduled | Select policy and affected-loop acceptance if `S3-T7` (tool examples) is requested |
| Ordered control null/reset | Optional-null reset/default meanings unqualified | Qualify field-specific semantics before broadening admission |
| Counting estimate precision | Coarse byte fallback vs. open proposal profile | Reconcile scope, input coverage, and provenance before activating counting |
| Execution controls & probability reports | Unselected | Keep pre-dispatch validation, existing text-report contracts, and per-API error classification |

## Deferred work

### Deferred Generation semantics

- Audio translation (`POST /v1/audio/translations`; OpenAI deprecated `whisper-1` and `/v1/audio/translations` on 2026-08-26 for shutdown on 2027-02-26 in favor of Realtime translation).
- Turn-scoped system messages (`clear_at`), tokenizer-unbound `logit_bias`, multi-candidate (`n > 1`) execution, and generated audio/video inside Chat beyond basic static audio.
- Experimental stateful persistence (`store: true`, reference continuation, stored-resource operations) until explicitly enabled.
- Native no-output cache prewarming.
- `POST /v1/messages/count_tokens` (returns `404`; reopening requires upstream reliability criteria and coarse estimation provenance).
- Non-Generation discovery (`GET /v1/morphiecore/models` covers Generation Text and assessed image inputs; standard `/v1/models` and `/v1/morphiecore/capabilities` cover activated standalone tasks and Embeddings).
- Unified citation delivery ([1C proposal](../proposals/stage5-backlog.md#1c-design-messages-citations)) and hosted tool discovery/web search ([Stage 5](../proposals/stage5-backlog.md#deferred-tool-discovery-and-web-search)).

### Deferred acceptance

| Target | Condition for acceptance |
|---|---|
| Non-image multimodal real checks | Real-service qualification per Audio/Video slice |
| Current-client image replay | Selected image client profile verification |
| Real multi-provider failover | [R09](unscheduled-issues.md#routing-qualification) prerequisites: independent compatible account pairs and fault injection |
| Claude Code acceptance | Beta headers, counting, and citation contracts |
| `pi`, `opencode`, and `codex` maintenance | Primary verified clients; parser/stream/replay behavior remains the client-integration oracle |

### Only if needed

- SSE keep-alive comments during prolonged delays.
- Conversation prefix-derived affinity without conversation IDs.
- Caller-configured first-content timeout thresholds.

## Not planned

- Direct inference-engine integration (llama.cpp, SGLang, vLLM).
- Raw wire passthrough, post-encoding JSON monkey-patching, or weakened schemas.
- Synthetic tool result generation, orphan role remapping, or automatic capability inference from upstream errors.
- Conversation hashing, LLM-based prompt routing, ingress load balancing, concurrency leasing, or token-rate reservation.
- Refusal retries across alternative models, sub-turn switching, or dynamic effort modulation.
- Gateway-hosted plugin execution or Provider-managed persistent background generation outside selected async job contracts.
- Unregistered Provider bindings, subscription-token guesswork, or historical protocol compatibility shims.

## Integration handoff

Carrier availability in codecs does not imply public route admission; route admission is governed explicitly in [topology compilation](../../crates/morphiecore-catalog/src/topology/compile.rs) and the [failover baseline](../architecture/runtime-and-routing.md#failover).

### Partial-delivery exit

1. Preserve verified primary client workflows (Haiku Messages, DeepSeek Chat, Grok Responses, Qwen Responses, Google Interactions, and Codex CLI Responses/Messages/Chat tool loops) using isolated current `pi`/`opencode`/`codex` drivers.
2. Verify each selected slice before claiming completion; deferring an unresolved subcase does not block independent slices.
3. Run the offline baseline and applicable SDK gates before committing; classify remaining observations in [Unscheduled issues](unscheduled-issues.md#unscheduled-issues) (`R*` tracking labels are traceability only, not a release checklist).

### Public route activation review

Before activating new public route carriers: classify every field on the [conversion ladder](../architecture/adaptation.md#conversion-ladder), verify preflight/preparation agreement, confirm routing/cancellation invariants, and pass offline baseline + SDK acceptance.
