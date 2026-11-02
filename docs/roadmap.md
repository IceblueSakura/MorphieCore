# Roadmap

This page owns current scope, priorities, dependencies, and exit gates. [Architecture](architecture.md) owns accepted design, [API expansion design](api-expansion-design.md) owns unselected proposals, and [Development](development.md) owns verification procedures. Code and tests define behavior. Completed slices are removed; Git preserves history.

## Contents

- [Current status](#current-status)
- [Standard request boundary repair](#standard-request-boundary-repair)
- [Three-protocol fidelity repair](#three-protocol-fidelity-repair)
  - [Agent-default production findings](#agent-default-production-findings)
  - [Conversion-ladder migration](#conversion-ladder-migration)
  - [Retained rejections](#retained-rejections)
  - [Remaining issue dispositions](#remaining-issue-dispositions)
  - [Closure boundary](#closure-boundary)
- [Standards reset and redesign](#standards-reset-and-redesign)
- [Planned order](#planned-order)
  - [Execution sequence and work-package rules](#execution-sequence-and-work-package-rules)
  - [Stage 1 work plan](#stage-1-work-plan)
  - [Stage 2 work plan](#stage-2-work-plan)
  - [Stage 3 work plan](#stage-3-work-plan)
  - [Generation Text production closure](#generation-text-production-closure)
  - [Stage 4 scope](#stage-4-scope)
    - [Decision API closure](#decision-api-closure)
    - [Stage-4 operation readiness](#stage-4-operation-readiness)
    - [Embedding integration targets](#embedding-integration-targets)
    - [Grok Imagine adaptation](#grok-imagine-adaptation)
  - [Stage 5 backlog](#stage-5-backlog)
- [Test supplementation](#test-supplementation)
  - [Current text and SSE gate](#current-text-and-sse-gate)
  - [Stage-4 reuse and acceptance](#stage-4-reuse-and-acceptance)
- [Phase rules](#phase-rules)
  - [Generation design review gate](#generation-design-review-gate)
  - [Observability follow-up](#observability-follow-up)
  - [Agent authorization and diagnostics follow-up](#agent-authorization-and-diagnostics-follow-up)
  - [Temporary routing freeze](#temporary-routing-freeze)
  - [IR and codec migration](#ir-and-codec-migration)
  - [Verification and integration boundary](#verification-and-integration-boundary)
- [Integration handoff](#integration-handoff)
  - [P7 boundary review](#p7-boundary-review)
  - [Controlled routing acceptance](#controlled-routing-acceptance)
- [Pending decisions](#pending-decisions)
  - [Client/IR stabilization decisions](#clientir-stabilization-decisions)
  - [Ownership and projection decisions](#ownership-and-projection-decisions)
  - [Scheduling and product decisions](#scheduling-and-product-decisions)
- [Deferred work](#deferred-work)
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

## Current status

**Current focus (2026-11-02):** Slices R0–R3 are complete. Active focus is **R4 (Unknown facts and history)**, followed by R5 (Cleanup and discovery) and installed-Agent default-profile acceptance. Stage 4 resumes once R4–R5 exit gates pass.

| Area | State | Next step |
|---|---|---|
| Routing | Stable routing baseline frozen; controlled synthetic recovery verified across Router, SDK, and installed clients | Real-failover production evidence required for multi-credential routes |
| Generation IR / Codec | R0–R3 complete; shared request/delivery owners, qualified streaming intake, Messages version admission | R4: history delegation and visible-history transfer; then R5 |
| Client compatibility | Default native Messages loops pass with installed pi and OpenCode without overrides | Complete three-protocol and cross-protocol exit loops after R4–R5 |
| Integration | R1a/R1b/R2 loops, controlled recovery and production closure passed | Revalidate changed paths against established gates |
| Cross-protocol conversion | Stage 3 closed for selected projections; `count_tokens` deferred | Stage-4 Embedding paths active; Google API paused |
| Decision APIs | Dual standard-SDK direction selected (TypeSafe/OpenAI) | Contract selection and Stage-4 implementation |
| Embedding & Media | Operational (Google native, OpenAI via OpenRouter, DashScope); Grok image codec implemented | Stage-4 contract selection for Images, Audio, Files, and Video |
| Downstream integration | Agent API key authentication and diagnostic attribution implemented | Per-Agent JSONL session storage design |
| Test baseline | Pinned upstream test baseline closed; strict SDK accumulators verified | Maintain offline baseline (`cargo test --locked --offline`) |

## Standard request boundary repair

Completed on 2026-10-26. [Architecture](architecture.md#standard-request-boundary) defines standard-priority contracts; see [dated findings](refs/2026-10-26-standard-request-boundaries.md#repair-disposition) for specific resolutions.

## Three-protocol fidelity repair

This workstream repairs semantic fidelity across standard Chat Completions, Responses, and Messages via the [conversion-ladder migration](#conversion-ladder-migration). Authoritative contracts: [conversion ladder](architecture.md#conversion-ladder), [field handling](architecture.md#field-handling), [projection principles](architecture.md#clientir-projection-principles), and [stream normalization](architecture.md#semantic-stream-normalization).

### Scope and boundaries

- **Protocols:** Covers all three same-protocol paths and six cross-protocol directions across request decoding, IR ownership, target request encoding, upstream decoding, delivery, and history replay.
- **Domain:** Generation Text (instructions/roles, text/refusal, function tools/results, structured output, reasoning/replay, presence/settings, usage, errors, and terminal events).
- **Extension properties:** Unknown extension fields are accepted and discarded at legal positions; known fields follow the conversion ladder; unscheduled capabilities keep [retained rejections](#retained-rejections).
- **Precedence:** Plain-text token-cap output may omit item status under named rules; Responses metadata echoes the admitted request only when upstream metadata is omitted.
- **Budget direction:** Bounded relaxation allows only the model-maximum clamp ([Architecture](architecture.md#bounded-token-budget-adaptation)).

### Reference strategy

| Reference family | Target contracts |
|---|---|
| OpenAI and Anthropic official specs & SDKs (Python / TypeScript) | Request/history unions, presence vs null defaults, tool arguments, signatures, stream accumulation, and terminal events |
| WHATWG HTML EventSource & RFC 8259 | SSE framing, multi-line events, UTF-8/JSON boundaries, EOF, and resource bounds |
| LiteLLM & CLIProxyAPI | OpenAI/Anthropic bridge mechanics, output-index mapping, block scheduling, and usage provenance |
| Vercel AI SDK & OpenAI Agents SDK | Typed stream consumers, incremental tool assembly, and history materialization |
| Installed pi and OpenCode | Real-world CLI/TUI serialization, SDK event consumption, and signature replay |

### Agent-default production findings

Captured pi 1.4.0 shapes run through the permanent [production-construction gate](development.md#sdk-loopback-gate). R1 and R2 resolved earlier lanes; unresolved lanes are listed below:

| Lane | Current gate disposition | Ladder resolution | Slice |
|---|---|---|---|
| Chat client → DeepSeek/Zhipu Chat tools and reasoning | `admission.client_representation` from `required_with_tools` | Deployer `readable_reasoning`; otherwise thinking-off fallback (L3) | R4 |
| Chat client → TokenPlan Chat, all profiles | `admission.client_representation` from unassessed history | Delegation | R4 |
| Responses client → OpenAI subscriptions, flat tools | Flat tools reject (requires namespace definitions and qualified calls) | Retain selected tool contract until endpoint evidence qualifies carrier | Endpoint qualification |
| Chat upstream usage | Duplicate nonnull final reports | Qualify at emitter | Outside slices |

### Conversion-ladder migration

Each slice migrates rejection sites onto the conversion ladder through unified request and delivery owners ([Architecture](architecture.md#conversion-ladder)).

#### Completed slices (R0–R3)

| Slice | Completed | Key Deliverables & Verified Behavior |
|---|---|---|
| **R0 Intake & Reachability** | 2026-10-30 | Permanent construction gate; qualified non-Chat streaming intake with named Chat JSON exception; Go/Haiku metadata preservation. |
| **R1 Request Disposition** | 2026-10-31 | Unified request disposition owner ([`lowering/disposition.rs`](../src/lowering/disposition.rs)); scalar `Option` migrations; conditional strictness; cache and report omissions. |
| **R2 Delivery Projection** | 2026-11-01 | Single delivery projection owner ([`ClientAdapter`](../src/adapter/downstream.rs)); unified event stream projection across JSON and SSE views. |
| **R3 Messages Intake** | 2026-11-02 | Version header admission; legacy sampling/block-binding omission; verified default native tool/history loops for installed pi and OpenCode. |

#### Active and pending slices (R4–R5)

| Slice | Work | Exit gate |
|---|---|---|
| **R4 Unknown facts and history** *(Active)* | **1. Delegation**: Send carriable history for unassessed binding facts (`ReasoningHistory::None`), letting upstream validate; discovery publishes `not_assessed`.<br>**2. `required_with_tools` fallback**: For Standard Chat + tools + thinking, allow deployer `readable_reasoning` or fallback to disabling thinking under L3 loss `chat.fallback-thinking-off.v1`; reject at planning only if thinking cannot be disabled.<br>**3. Generalized visible-history transfer**: Omit uncarried/foreign reasoning, citations, and obsolete markers under L3 while preserving visible text and tool links. | TokenPlan Chat, DeepSeek/Zhipu Chat tools, and two-turn Bailian Responses lanes accept; replay trust checks unchanged. |
| **R5 Cleanup and discovery** | Remove test-only strict-complete reporting policy, obsolete `mca1.` marker handling, and unreachable delivery guards. Run the `FidelityRecords` probe. Convert remaining `Presence` fields. Derive `/capabilities` and discovery (S10) from disposition tables. | Discovery publishes gate-observed facts; `Presence` remains only on contract-defined null fields; `FidelityRecords` outcome recorded. |
| **Exit** | Current-client characterization, then default reasoning/tool/history gateway loops with installed pi and OpenCode across all three client protocols and selected cross-protocol lanes. | Gateway execution and replay pass within documented profiles. |

Dependencies: R0–R3 are complete. R4 resolves delegation and fallback in planning, and visible-history transfer in request lowering. R5 runs the `FidelityRecords` probe against the single delivery projection.

### Retained rejections

These L5 boundaries and invariant guards remain after the migration. Current additional blockers are tracked in the [findings table](#agent-default-production-findings) with their slice.

| Boundary | Retained |
|---|---|
| Chat → IR | `n > 1`, `web_search_options`, nonempty `logit_bias`, `store:true`, `moderation`, legacy `functions`/`function_call`/`role:function` |
| Responses → IR | Hosted tools and items, unselected `include` families, background execution, active conversation/prompt/moderation and context-management operations, item references outside the managed store. Local `store`/`previous_response_id` stay supported through [managed ingress](../src/gateway/ingress/admission.rs) |
| Messages → IR | `between_tools`, per-message configuration, `clear_at`, task budgets pending selection, `container`, `mcp_servers`, context management and compaction, server tools and non-direct callers, block-binding `"error"`, `inference_geo`, enabled citations, null output effort/format pending field evidence |
| Request → target | Media, tools or explicit output constraints without a carrier; explicit strict guarantees without a carrier; `stop` without a carrier; compaction without a carrier; explicit history-read intent that cannot be carried |
| IR → client | Chat custom-call SSE; Responses SSE cancellation; unknown Chat/Messages stop causes without an evidence-backed map; non-object Messages tool input other than an empty string; call ID or name lengths beyond the client limit; blocking safety reports; retraction of published accounting |
| Invariants | Credential/origin/trust selection, replay origin and format trust, client file IDs, resource bounds, malformed known values, terminal honesty, incomplete tool input never executable |

The [pinned Anthropic SDK](../tests/upstream/sources.lock.json) types already include `between_tools`, optional-nullable output configuration and zero-token cache prewarming; their rejections are capability-selection gaps, not invalid syntax. Nonempty Messages tool openers followed by nonempty fragments concern native upstream SSE intake, not Client request intake.

### Remaining issue dispositions

The [2026-10-28 source and feasibility recheck](refs/2026-10-27-client-ir-boundaries.md#follow-up-source-and-feasibility-review-2026-10-28) supports these dispositions; the ladder migration owns the active work.

| Issue | Current disposition | Reopen or complete when |
|---|---|---|
| Chat custom-tool SSE | Retain standard rejection; investigate upstream intake and downstream delivery separately | Official event contract resolves the disagreement, or a separately selected dialect/client contract supplies independent wire/consumer evidence |
| Nonempty Messages tool opener plus nonempty fragments | Retain rejection, including a complete-fragment negative regression; complete opener plus zero-byte deltas remains supported | Authoritative replacement/merge semantics exist |
| Bounded token-budget adjustment | Model-maximum clamp selected; broader formulas deferred; native effort and existing validation preserved | An authorized target plus failing request, limit provenance, useful reserve, finite maximum and infeasible behavior qualify a target-local rule |
| `user:null`, Chat refusal requiredness, Responses image-detail default | Preserve current intake; image `detail:"auto"` already takes L2 | Service contract clarifies the discrepancy or a selected strict consumer exposes a concrete failure |
| Legacy function carriers | Retained rejection | A real client needs a constrained association/default/conflict/history contract |
| Broader settings echo and richer output grouping | Case-specific ladder application | A concrete failure has an exact mapping or a bounded approximation; preserve report authority and content-coordinate ownership |
| Messages custom-input wrapping | Defer as an end-to-end tool-interface adaptation | Declaration, arguments, return/history mapping and client execution expectations are selected together |
| Token scope and SDK drift | Investigate only the selected target/field; no automatic source-pin promotion | A bounded budget rule or reviewed version change has independent expectations and a reachable target path |

### Closure boundary

Reclose Client/IR stabilization when the corrected R0 gate is permanent; every same-protocol lane and each selected cross-protocol lane accepts or cites a [retained rejection](#retained-rejections) or invariant; R1–R5 each have verified completion; discovery publishes the same facts; and the exit loops pass.

Default reasoning/tool/history gateway success on all three client protocols is required for closure. Deployer-selected public model profiles (e.g. `readable_reasoning`) are configuration, not client overrides.

## Standards reset and redesign

Accepted on 2026-10-23 and closed on 2026-10-24. Pinned upstream release sources, standard delivery dialects, and canonical IR instruction mapping are settled in [Architecture](architecture.md#protocols) and verified in [Development](development.md#upstream-test-sources). Safe-to-ignore diagnostic extensions are permitted without dedicated extension tests pending final production packaging.

## Planned order

| Stage | Goal | Exit evidence |
|---|---|---|
| **1 — Same-protocol lossless IR codecs** | Preserve protocol semantics across same-protocol request, response, streaming, and history paths. Prioritize known fields, presence/defaults, reasoning/replay, and terminals | Independent wire→IR and IR→wire tests in each direction; JSON/SSE agreement; history round-trip preservation |
| **2 — Basic agent tool interactions** | Function-tool declarations, schema intent, call IDs/arguments, streaming results, and multi-turn history for pi/OpenCode | Verified same-protocol tool loops preserving source data, reasoning signatures, and cache controls |
| **3 — Cross-protocol stability & counting** | Cross-protocol request, response, and history conversion via exact carriers or approved adaptations. `count_tokens` deferred | Directional conversion tests; rejection of unrepresentable semantics; candidate-local isolation |
| **4 — Modalities, Decision & file resources** | Decision, Embedding, Audio (Speech/Transcription/Translation), Images, Files/Uploads, and Video at equal priority | Verified request/result contracts, media bounds, resource isolation, cancellation, and SDK loops |
| **5 — Unscheduled follow-ups** | Citations, tool discovery, hosted tools, server-side compaction, Batch, and Realtime | Requires explicit per-slice design selection and scheduling prior to implementation |

### Execution sequence and work-package rules

Every work package follows this lifecycle:
1. **Entry:** Pin source revisions, dialect rules, target profiles, and unresolved decisions. Document protocol findings in `docs/refs/`.
2. **Delivery:** Implement one coherent request/result/stream/history slice. Resolve unselected structures with a minimal counterexample and options before proceeding.
3. **Invariants:** Maintain immutable source IR, fresh per-attempt decoder state, and history-prefix stability during turn appending.
4. **Evidence:** Provide independent directional tests and focused failure assertions at the responsible layer. Classify each field's ladder level and give rejections their L5 reason.
5. **Exit:** Run the offline baseline and relevant SDK gates before commit and push. Remove completed roadmap packages and record decisions in [Architecture](architecture.md).

### Stage 1 work plan

**Status: Closed.** Replaced by the standards-reset baseline. Chat, Responses, and Messages maintain independent directional expectations within the [contract inventory](refs/2026-10-23-stable-protocol-contracts.md). Google generation has no downstream client pair.

### Stage 2 work plan

**Status: Closed.** Verified client interaction loops:
- Chat tool/reasoning history ([R1a gate](development.md#sdk-loopback-gate)).
- Responses native history and lifecycle reports ([R1b gate](development.md#sdk-loopback-gate)).
- Native Messages signed history and cache controls ([R2 gate](development.md#sdk-loopback-gate)).

### Stage 3 work plan

**Status: Closed for Generation Text projections; token counting deferred.** Implemented contracts: [model-effective guards](architecture.md#model-effective-values), [trusted missing-cap defaults](architecture.md#missing-output-ceilings), [attribution](architecture.md#end-user-attribution), [foreground Google intake](architecture.md#google-foreground-intake), and [Messages terminal projection](architecture.md#messages-terminal-projection).

#### Google API pause

Further general Google API expansion is paused. Implemented exceptions:
1. **Terminal reports:** Bounded `finishMessage` and [processing-class reports](architecture.md#service-policies-and-reports).
2. **Embedding:** Standalone native `gemini-embedding-2` text endpoint under Stage 4.

General multimodal input, native client intake, and custom tools remain excluded from the Google path.

### Generation Text production closure

**Status: Completed.** Bounded offline, SDK, installed-client, and Provider gates pass within documented profiles:
- **Baseline profiles:** Initial production profile verified with `pi` 1.4.0 and `opencode` 2.0.32 across reasoning, tool calls, and recovery.
- **Contract enforcement:** Bounded explanatory reports, automatic TTL cache controls, and numeric thinking budgets enforced.
- **Sustained operation:** Verified cancellation, stopped readers, budget resets, and bounded logging.
- **Exclusions:** Native `file_citation` insertion units and cross-account real failover remain excluded from production claims.

### Stage 4 scope

Stage 4 API families have equal priority and advance as their contracts are selected:

| Package | Scope | Exit criteria |
|---|---|---|
| **S4-0 Per-operation design** | Pin official contracts, select targets, define resource lifecycles, and apply the conversion ladder to each family's fields | Explicit contract decisions classifying fields by ladder level, without speculative shared abstractions |
| **S4-6 Decision closure** | Complete dual-SDK design, typed answers, and synchronous execution | Verified independent oracles, both SDK loops (TypeSafe and OpenAI), and bounded Provider evidence |
| **S4-2a Speech** | Text-to-speech with static binary and optional streaming delivery | Bounded buffering, cancellation safety, and verified audio format carriage |
| **S4-2b Transcription** | Audio-to-text recognition with multipart upstream preparation | Accurate text/timing carriage, format rejection, and bounded resource handling |
| **S4-2c Translation** | Audio-to-text translation as a distinct native operation | Preserved language intent, output format validation, and direct execution |
| **S4-3 Images** | Generate, Edit, and preview/final streaming (internal sequence: Generate first) | Selected defaults, format encoding, mask boundaries, and artifact identity |
| **S4-4a Files** | File storage, public ID authorization, and content access | Isolated namespaces, bounded spooling, deletion lifecycle, and durable mappings |
| **S4-4b Multipart Uploads** | Chunked upload sessions, part ordering, and cleanup | Ordered part assembly, cancellation cleanup, and bounded memory use |
| **S4-4c File-reference integration** | Connect resolved file resources to dependent task inputs | Target-scoped references, stale reference rejection, and direct-input fallback |
| **S4-5 Video** | Async video generation, job status tracking, and artifact access | Bounded polling, status reconciliation, cancellation, and artifact delivery |

#### Decision API closure

Selected direction: Dual-SDK support (TypeSafe and OpenAI) backed by a unified Decision IR.
1. Pin SDK schemas and resolve differences in discovery, error envelopes, and URL handling.
2. Complete capability union review: evidence representation, rubrics, confidence metrics, and refusal behavior.
3. Define bounded synchronous execution profile and input validation.
4. Validate both official SDK clients against gateway endpoints.

#### Stage-4 operation readiness

| Family | Readiness | Next step |
|---|---|---|
| Decision | Dual-SDK direction selected; IR drafts under review | Finalize contract selection and implement codecs |
| Text Embedding | Operational (DashScope, OpenRouter, Google) | Maintain contract; multimodal embeddings remain separate |
| Images Generate | Grok static codec implemented; public contracts under review | Select public default encoding (PNG vs Base64) and response metadata |
| Images Edit & Stream | Research complete; mask boundaries and stream formats unselected | Await static Generate closure |
| Audio (Speech/ASR) | Static paths exist; streaming and multipart unselected | Select bounded single-operation profile |
| Files & Uploads | Safety requirements defined; storage authority unselected | Select storage authority and ID authorization |
| Video | Upstream API research available; job lifecycles unselected | Select job status and retrieval contract |

#### Embedding integration targets

Implemented on `POST /v1/embeddings`:
- **DashScope:** Text embedding models.
- **OpenRouter:** `text-embedding-3-small` with trusted scoped tokens.
- **Google:** Native `gemini-embedding-2` with 100-input ceiling and 60,000-value budget.

#### Grok Imagine adaptation

Pure static image codec implemented (`grok-imagine-image-2.0`) with Base64 delivery. Video integration is deferred to native asynchronous APIs rather than a synchronous facade.
- OAuth uses the shared single-account Grok pool without inline login.
- Generate contract requires explicit resolution of dimensions, quality tiers, and creation timestamps before public activation.

### Stage 5 backlog

All Stage 5 tasks are currently unscheduled:
- **Citations & source attribution:** Unified citation models ([1C proposal](api-expansion-design.md#1c-design-messages-citations)).
- **Tool discovery:** Dynamic tool discovery and native `web_search`.
- **Server-side compaction:** Provider-executed `/responses/compact` operations.
- **Batch:** Asynchronous job processing and reconciliation.
- **Realtime:** Bi-directional streaming sessions, interruption, and media transport.

## Test supplementation

The test strategy uses pinned upstream assets and standard SDK consumers rather than hand-maintained mock matrices.

### Current text and SSE gate

- **Baseline:** Validated in `tests/upstream` using pinned sources and locked assets.
- **Scope:** Chat Completions, Responses, and Messages request/response contracts, SSE framing, EOF handling, and stream accumulation.
- **Invariants:** Upstream test updates are reviewed and locked; candidates remain unpromoted until reviewed.

### Stage-4 reuse and acceptance

1. Identify applicable official SDK fixtures for each selected operation.
2. Verify MorphieCore request preparation and response decoding against upstream test cases.
3. Preserve the full offline baseline while adding operation-specific lifecycle and safety checks.

## Phase rules

### Generation design review gate

The standards-reset architecture baseline was accepted on 2026-10-23. Implemented invariants:
- **Semantic ownership:** Ordered content, controls, and reported facts maintain separate typed owners in IR.
- **Source parsing & projection:** Source input is immutable; target projection operates on private copies.
- **Tools & schemas:** Tool identity, schema constraints, and explicit strictness are preserved; conditional defaults relax only through named ladder rules.
- **Delivery & terminals:** Output streaming preserves phase ordering; terminals require validated upstream EOF.
- **History & replay:** Resumed history reflects actual delivered output, preserving reasoning signatures and cache markers.

### Observability follow-up

- **Domain evidence:** Authoritative, body-free diagnostic receipts and typed error classifications are active.
- **Local telemetry:** File and terminal traces are operational with log-rotation and queue bounds.
- **Deferred scope:** Centralized OTel Collector forwarding, public diagnostic schema stabilization, and cluster metrics aggregations await production packaging.

### Agent authorization and diagnostics follow-up

- **Current state:** Startup-configured Agent API keys with diagnostic attribution are active.
- **Session design:** Per-Agent JSONL session storage is selected as the downstream integration architecture; implementation requires storage opt-in, boundary isolation, and lifetime bounds.
- **Deferred scope:** Dynamic OAuth authorization servers, cross-agent token exchange, and cluster IAM remain unscheduled.

### Temporary routing freeze

The separately requested [Provider account redesign plan](provider-account-redesign-plan.md) has an **accepted draft direction with configurable policy parameters as of 2026-10-30**. It remains a design draft awaiting final confirmation; implementation details, rewrite slices and scheduling are deferred. It grants no implementation authorization and adds no active work package. Confirm the design and select the rewrite scope explicitly before changing this baseline.

The [stable routing baseline](architecture.md#failover) is frozen. Codec and protocol work must not alter:
- Pool membership, health probes, or target ordering.
- Conversation cache affinity or cooldown thresholds.
- Route selection based on projected loss or codec capability.

### IR and codec migration

- Preserve immutable source IR across retries and failover attempts.
- Enforce strict JSON parsing and bounded resource limits on all decoders.
- Require independent directional expectations for all new codec mappings.
- Maintain honest terminal reporting: errors and cancellations must never fabricate success.

### Verification and integration boundary

| Milestone | Requirements | Permitted actions |
|---|---|---|
| Development slice | Focused unit and codec tests pass; regressions isolated | Continue local edits under routing freeze |
| Commit and push | Full offline baseline passes (`cargo test --locked --offline`) | Commit and push to `origin/main` |
| Integration acceptance | Connected SDK scenarios and client loops pass | Declare client/protocol compatibility |

## Integration handoff

Carrier availability in codecs does not imply public route admission. Route admission is controlled by explicit target binding policy in [topology compilation](../src/topology/compile.rs).

### P7 boundary review

Prerequisites for activating newly implemented codec carriers on public routes:
1. Classify each field on the conversion ladder: exact, equivalent, approximate, omitted or rejected.
2. Verify preflight/preparation agreement on candidate bindings.
3. Validate routing stability under failure and reconnection.
4. Complete offline baseline and connected SDK acceptance before client validation.

#### First reconnection slices

Selected loops (R1a, R2, R1b) and Google basic reconnection are completed under their recorded evidence. Further reconnection follows the [public capability contract](architecture.md#public-capability-and-adaptation-notice) and model-level preflight validation.

### Controlled routing acceptance

Controlled synthetic recovery is verified:
- Router/SDK recovery across simulated network faults and expired credentials.
- Sticky conversation continuation with signed-history preservation.
- All-cooling fail-safe recovery.
Real cross-account failover requires distinct, advanceable production credentials and remains subject to production deployment gates.

## Pending decisions

### Client/IR stabilization decisions

The user accepted principles P1–P5 and selections S1–S11 on 2026-10-28, generalized S2 to semantic-equivalence compatibility, and on 2026-10-30 accepted the conversion ladder with its selected applications and the IR/codec simplifications. [Architecture](architecture.md#conversion-ladder) owns their content; this table maps scheduling ids to their remaining migration work. Request strictness, presentation and cache selections are implemented.

| Id | Accepted rule | Owner | Slice |
|---|---|---|---|
| L | Conversion ladder and its 2026-10-30 applications | [Conversion ladder](architecture.md#conversion-ladder) | R4–R5 |
| C | Remaining IR/codec simplification and experiment-gated `FidelityRecords` folding | [Delivery and evidence decisions](architecture.md#delivery-and-evidence-decisions) | R5 |
| S5 | Generalized visible-history transfer | [Reasoning replay](architecture.md#reasoning-replay) | R4 |
| S10 | Agent-facing discovery | [Generation Text model discovery](architecture.md#generation-text-model-discovery) | R5 |
| S11 | Binding reasoning-history contract, with delegation for unassessed facts | [Binding reasoning-history contract](architecture.md#binding-reasoning-history-contract) | R4 |

### Ownership and projection decisions

| Issue | Status | Action |
|---|---|---|
| Complete citations | Stage 5 | Retain local point-anchor owner; defer full citation model |
| Execution controls | Paused | Enforce pre-dispatch validation; reject unsupported controls |
| Google error envelopes | Paused | Maintain per-API classification; avoid speculative unified parsing |
| Google probability reports | Paused | Keep existing text-report exceptions; defer broader token scores |

### Scheduling and product decisions

| Area | Current policy | Scope |
|---|---|---|
| Target binding activation | Requires P7 review and verified preflight agreement | Target routes only |
| Models discovery extensions | Generation Text active; other modalities schema-only | Discovery endpoint only |
| Cache prewarming | Deferred; requires dedicated contract selection | Operation-specific |
| Token counting | Deferred; upstream-first policy selected | `POST /v1/messages/count_tokens` |
| API expansion | Stage 4 active at equal priority; Stage 5 unscheduled | Specific modalities |
| Files & multipart uploads | Stage 4 priority; requires storage authority selection | File resource endpoints |

## Deferred work

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
Generation Text model discovery is operational, including route-common binding-history declarations from the first S11 migration. Its broader agent-facing extension (preferred interface, adaptation rules, agent-default probe forms) is slice R5 of the [conversion-ladder migration](#conversion-ladder-migration), with S10 pending. Discovery for other task families (Embeddings, Media, Decision) remains schema-only until those operations are publicly activated.

### Messages citations
Unified citation delivery ([1C proposal](api-expansion-design.md#1c-design-messages-citations)) remains unscheduled. Requests requiring citation output are rejected.

## Deferred acceptance

| Target | Condition for acceptance |
|---|---|
| Non-image multimodal real checks | Stage-4 modality slice implementation |
| Current-client image replay | Selected image client profile verification |
| Real multi-provider failover | Dedicated production credential pairs and reproducible fault injection |
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
