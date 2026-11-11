# Roadmap

This page owns current scope, priorities, dependencies, and exit gates. [Architecture](architecture.md) owns accepted design, [API expansion design](api-expansion-design.md) owns unselected proposals, and [Development](development.md) owns verification procedures. Code and tests define behavior. Completed slices are removed; Git preserves history.

## Contents

- [Current status](#current-status)
- [Unscheduled issues](#unscheduled-issues)
  - [Tool guarantees](#tool-guarantees)
  - [History and replay](#history-and-replay)
  - [Content and client qualification](#content-and-client-qualification)
  - [Protocol boundaries](#protocol-boundaries)
  - [Routing qualification](#routing-qualification)
- [Deferred protocol follow-ups](#deferred-protocol-follow-ups)
  - [Conditional Stage-3 follow-ups](#conditional-stage-3-follow-ups)
    - [Tool verification and scope gates](#tool-verification-and-scope-gates)
  - [Retained rejections](#retained-rejections)
- [Planned order](#planned-order)
  - [Execution sequence and work-package rules](#execution-sequence-and-work-package-rules)
  - [Stage 4 scope](#stage-4-scope)
    - [Decision API closure](#decision-api-closure)
    - [Stage-4 operation readiness](#stage-4-operation-readiness)
    - [Grok Imagine adaptation](#grok-imagine-adaptation)
  - [Stage 5 backlog](#stage-5-backlog)
- [Test supplementation](#test-supplementation)
  - [Stage-4 reuse and acceptance](#stage-4-reuse-and-acceptance)
- [Phase rules](#phase-rules)
  - [Observability follow-up](#observability-follow-up)
  - [Agent authorization and diagnostics follow-up](#agent-authorization-and-diagnostics-follow-up)
  - [Temporary routing freeze](#temporary-routing-freeze)
  - [IR and codec migration](#ir-and-codec-migration)
  - [Verification and integration boundary](#verification-and-integration-boundary)
- [Integration handoff](#integration-handoff)
  - [Partial-delivery exit](#partial-delivery-exit)
  - [Public route activation review](#public-route-activation-review)
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

## Current status

**Current status (2026-11-11):** the Generation review baseline has converged under the [partial-delivery exit](#partial-delivery-exit). Configuration/diagnostic cleanup, maintenance simplification and lifecycle convergence are delivered under [Observability](architecture.md#observability), with remaining production observability/reliability/privacy gates deferred in the [cleanup plan](convergence-cleanup-plan.md). Accepted contracts live in [Architecture](architecture.md); dated [qualification evidence](development.md#provider-qualification-evidence) and current-Agent checks live in Development. Residual limitations and prerequisites share the [unscheduled issue register](#unscheduled-issues), without a separate review repair queue. **User-selected 2026-11-11:** no pre-Stage-4 gate remains; R09 and Agent session storage no longer gate Stage 4. No deployment or new API activation is implied.

| Priority | Remaining work | Exit |
|---|---|---|
| Conditional tool follow-ups | [S3-T5–S3-T7](#conditional-stage-3-follow-ups): namespace aliases, custom-text wrappers and examples projection | Separate policy selection and full affected-loop acceptance; not default handoff prerequisites |
| API expansion | Select and implement Stage-4 operation contracts | Per-operation lifecycle, protocol and SDK acceptance |
| Downstream integration | Opt-in [per-Agent JSONL session storage](architecture.md#agent-session-files); unscheduled and not a Stage-4 prerequisite | Select isolation and lifetime bounds before implementation |
| Production preparation | Select real diagnostic receiver/storage and deferred cluster observability scope | Explicit deployer selection; cleanup implementation does not deploy a Collector or backend |

## Unscheduled issues

This is the single register for unresolved Generation/client boundaries and qualification gaps. None is an active repair package or a prerequisite for the delivered baseline. Reopen only after a concrete unmet contract, target/profile, prerequisite and bounded acceptance plan are selected. Review IDs remain traceability labels, not a second phase queue. [Development](development.md#provider-qualification-evidence) owns dated verdicts; [Architecture](architecture.md) owns accepted contracts and service limitations.

R02/R06's pi-first stateless carrier was selected on 2026-11-10; remaining client/Provider qualification stays here. R04/R08 and Bailian structured-output variability remain excluded; do not substitute prompt/history rewriting or deduplication work. Accepted Bailian forcing and Grok content limitations are not reopened. R09 moved here from its former pre-Stage-4 slot on 2026-11-11.

### Tool guarantees

| Issue | Current boundary | Re-entry / decision |
|---|---|---|
| Google strict function guarantees (R05) | The [qualified subset](architecture.md#google-strict-function-arguments) includes nullable fields, bounded local references and no-call equivalence. Enabled-call length/pattern/general-composition and serial guarantees still reject; full current-Agent profiles remain unqualified. | A concrete current-Agent need and native carrier evidence first. Gateway argument validation or a call-publication barrier needs explicit selection of failure semantics and streaming latency before implementation. No argument repair, automatic retry, schema deletion or strictness disabling. Broader reference semantics need their own bounded design. |
| Effective output ceilings and recovery | Non-Qwen Bailian reply-ceiling enforcement and Haiku same-process cap-switch recovery remain unqualified; preparation receipts and GLM JSON smoke do not close them. | Select the exact client/model/control transition and a bounded real enforcement/recovery story; preserve independent byte/time bounds and honest length terminals. |

### History and replay

| Issue | Current boundary | Re-entry / decision |
|---|---|---|
| Google stateless ordinary-signature qualification (R02/R06) | **Pi-first carrier delivered, 2026-11-10.** The selected [Responses profile](architecture.md#responses-owner-replay) and pi save/resume/tool loop have synthetic acceptance. Standard unadapted clients still lack an ordinary text/call carrier. | OpenCode adaptation is deferred. Select the exact client/Provider lane and bounded real qualification separately; synthetic acceptance does not qualify real signed-call issuance or endpoint behavior. No signature deletion, synthetic reasoning owner or new Google API. |
| Real Haiku protected output (R01) | Current OpenCode's original Chat loop and an explicit adaptive/medium discriminator produce no protected owners; existing carrier repair remains verified. | Resume when the same profile actually yields protected output. Missing activation alone does not justify another repair or automatic effort-to-activation mapping. |
| Interactions signed-call replay | No signed-call sample was independently observed in the retained profile qualification; thought replay does not close that cell. | Obtain an actual signed-call sample for the selected profile and verify exact feedback/continuation; do not infer it from a generic managed-loop success. |

### Content and client qualification

| Issue | Current boundary | Re-entry / decision |
|---|---|---|
| Messages literal-label recall (R13) | Scoped upstream omission: the unchanged five-request audit preserves ordered instructions/history/tools/effort, and complete native text equals delivered/client text on the failed recall. | No further recall sweep or prompt/history rewrite. Re-enter only on concrete new project-owned divergence; preserve the exact-label failure and distinguish incomplete captures from verified attribution. |
| Luna router endpoints | Endpoint contracts remain unqualified. Existing registered-binding and Agent evidence does not qualify additional router endpoints. | Select the exact endpoint/profile and independent full-loop acceptance; no new binding or activation is implied. |

### Protocol boundaries

The [2026-10-28 source and feasibility recheck](refs/2026-10-27-client-ir-boundaries.md#follow-up-source-and-feasibility-review-2026-10-28) supports these retained dispositions. Current strict parsing, resource, replay and terminal guarantees remain in force.

| Issue | Current disposition | Reopen or complete when |
|---|---|---|
| Chat custom-tool SSE | Retain standard rejection; investigate upstream intake and downstream delivery separately | Official event contract resolves the disagreement, or a separately selected dialect/client contract supplies independent wire/consumer evidence |
| SIWC Responses inline-tool continuation | Real endpoint acceptance for inline declarations and direct call/result history remains unqualified | Select the endpoint/profile and a bounded full-loop run; retain root namespace requirements |
| Chat upstream final usage | Duplicate nonnull final reports remain unqualified at the emitter | A concrete upstream profile and independently observed report sequence |
| Nonempty Messages tool opener plus nonempty fragments | Retain rejection, including a complete-fragment negative regression; complete opener plus zero-byte deltas remains supported | Authoritative replacement/merge semantics exist |
| Bounded token-budget adjustment | Model-maximum clamp selected; broader formulas deferred; native effort and existing validation preserved | An authorized target plus failing request, limit provenance, useful reserve, finite maximum and infeasible behavior qualify a target-local rule |
| `user:null`, Chat refusal requiredness, Responses image-detail default | Preserve current intake; image `detail:"auto"` already takes L2 | Service contract clarifies the discrepancy or a selected strict consumer exposes a concrete failure |
| Legacy function carriers | Retained rejection | A real client needs a constrained association/default/conflict/history contract |
| Broader settings echo and richer output grouping | Case-specific ladder application | A concrete failure has an exact mapping or a bounded approximation; preserve report authority and content-coordinate ownership |
| Messages custom-input wrapping | Conditional Stage-3 follow-up S3-T6; not a core exit prerequisite | Declaration, arguments, return/history mapping and client execution expectations are selected together |
| Token scope and SDK drift | Investigate only the selected target/field; no automatic source-pin promotion | A bounded budget rule or reviewed version change has independent expectations and a reachable target path |

### Routing qualification

The routing freeze and credential-lifecycle authorization remain unchanged. Additional cache-affinity test items await user supplementation; do not invent or launch that broader suite.

| Issue | Current boundary | Re-entry / decision |
|---|---|---|
| **R09 — Real account-compatible failover** | [Real stateless JSON/SSE replay isolation passes](development.md#replay-isolation-evidence). Earlier loader evidence reports one enabled declared account per Provider; no independent compatible pair is qualified. Not a Stage-4 prerequisite since 2026-11-11 | With independent compatible accounts on one authorized same-model route, verify A success/affinity, a bounded A failure without switching inside that request, B selection on a new request with A-private replay omitted, then B success/stickiness. Define fault injection and budgets first; provisioning new credentials requires separate approval. Same-key aliases, different-model switching and synthetic authorities do not qualify this real-account gate |

## Deferred protocol follow-ups

Conditional tool packages and retained rejection boundaries are collected here; all unscheduled qualification issues live in the [shared register](#unscheduled-issues). Apply the accepted [conversion ladder](architecture.md#conversion-ladder), [field handling](architecture.md#field-handling), [projection principles](architecture.md#clientir-projection-principles) and [stream normalization](architecture.md#semantic-stream-normalization).

### Conditional Stage-3 follow-ups

These existing-tool compatibility improvements belong to Stage 3, not modality expansion or hosted execution. They remain **conditionally scheduled after the core**, requiring separate policy selection. They do not block Stage 4 or optional Agent qualification by default; if a selected acceptance lane requires one, make that package an explicit prerequisite for that lane. Deferred packages remain here rather than being silently activated or treated as complete.

| Package / entry | Scope | Exit check |
|---|---|---|
| **S3-T5 — Namespace aliases** / alias-policy selection | Reversible leaf names through request, selection, return and history; reuse candidate/delivery ownership | Duplicate leaf names, reorder/append/remove stability, original client identity, candidate isolation and JSON/SSE/retained agreement |
| **S3-T6 — Plain custom-text wrapping** / wrapper-policy selection; S3-T5 only for namespaced wrappers | Grammar-free custom text through function wrappers and inverse delivery; no grammar-to-prose fallback | Exact completed text and original call kind, bounded buffering, honest partial/cancelled states and lawful client delivery; no invented custom SSE |
| **S3-T7 — Tool examples projection** / content-policy selection | Bounded labelled examples in descriptions when no native examples carrier exists | Example order/content and absent-versus-empty meaning remain deliberate; schema, selection and strictness unchanged; token/cache/salience loss recorded |

#### Tool verification and scope gates

Verification is layered rather than every field multiplied by nine:

- Extend existing semantic and SDK tool-history fixtures with independent wire→IR and IR→wire expectations. Nine ordinary loop cells cover route composition; family-specific tests cover distinct losses and failures, not redundant permutations.
- Check JSON/SSE and retained-view agreement, partial arguments, raw-string authority, distinct item/call IDs, error results, cancellation, bounds, EOF and replay isolation. New alias/wrapper mapping must finish before publication and cannot depend on clients returning diagnostics.
- Reuse `tests/sdk/openai_tool_history.rs`, `tests/sdk/messages_tool_loop.rs`, existing semantic tool/schema/stream cases and installed-client drivers. Keep official baseline assets and real SDK consumers; do not create a parallel fixture framework.
- Current installed pi/OpenCode versions are recorded at execution time, without downgrade. Preserve successful default tool/history loops, including strict Chat and Responses inline declarations. Client-history normalization belongs in the codec; fixtures and transparent forwarders must not repair request bodies.
- Reuse the [offline baseline](development.md#offline-baseline) and [SDK gate](development.md#sdk-loopback-gate) per implementation slice. Real Provider/model qualification is separately selected and bounded; synthetic acceptance does not certify endpoint behavior.

Risks that change the plan:

- Unknown root/inline precedence prevents computing a faithful source active set for colliding definitions; scope that approximation to known cases, while preserving native shapes when possible.
- Missing custom-SSE carriers or lawful returned-role replay prevent complete advanced-family loops. Keep those boundaries explicit; ordinary function work need not wait for a speculative transport.
- Explicit strict, selected-tool, cardinality or serial guarantees with no target carrier remain rejected unless that concrete guarantee downgrade is selected. Do not repair by schema deletion, automatic prompting or a new silent fallback.
- S3-T5–S3-T7 require their own policy selection and affected-loop acceptance; textual tool-history annotations, media relocation and grammar-to-prose remain outside the scheduled packages.

Tool discovery, hosted/programmatic execution, new endpoints and Google expansion remain outside these tool packages. Client-history annotations use their separate [L2 contract](architecture.md#semantic-equivalence-compatibility). Public capability metadata must describe conditional mappings honestly; general protocol or SDK presence is not a per-model promise.

### Retained rejections

These L5 boundaries and invariant guards constrain future work. Endpoint-specific entry conditions live in the [issue register](#protocol-boundaries).

| Boundary | Retained |
|---|---|
| Chat → IR | `n > 1`, `web_search_options`, nonempty `logit_bias`, `store:true`, `moderation`, legacy `functions`/`function_call`/`role:function` |
| Responses → IR | Hosted tools and items, unselected `include` families, background execution, active conversation/prompt/moderation and context-management operations, item references outside the managed store. Local `store`/`previous_response_id` stay supported through [managed ingress](../src/gateway/ingress/admission.rs) |
| Messages → IR | `between_tools`, message-local null/reset and unselected format/task-budget controls, `clear_at`, root task budgets pending selection, `container`, `mcp_servers`, context management and compaction, server tools and non-direct callers, block-binding `"error"`, `inference_geo`, enabled citations, null root output effort/format pending field evidence |
| Request → target | Media, tools or explicit content/schema constraints without a carrier; explicit strict guarantees without a carrier or selected relaxation; `stop` without a carrier; compaction without a carrier; explicit history-read intent that cannot be carried. Output ceilings and [Bailian function strictness](architecture.md#shared-aliyun-billing-dialect) follow their selected loss dispositions |
| IR → client | Chat custom-call SSE; Responses SSE cancellation; unknown Chat/Messages stop causes without an evidence-backed map; non-object Messages tool input other than an empty string; call ID or name lengths beyond the client limit; blocking safety reports; retraction of published accounting |
| Invariants | Credential/origin/trust selection, replay origin and format trust, client file IDs, resource bounds, malformed known values, terminal honesty, incomplete tool input never executable |

The [pinned Anthropic SDK](../tests/upstream/sources.lock.json) types already include `between_tools`, optional-nullable output configuration and zero-token cache prewarming; their rejections are capability-selection gaps, not invalid syntax. Nonempty Messages tool openers followed by nonempty fragments concern native upstream SSE intake, not Client request intake.

## Planned order

The Generation review baseline is delivered under the [partial-delivery contract](#partial-delivery-exit). Stage 4 has no remaining entry gate; [unscheduled issues](#unscheduled-issues), including R09, do not become implicit prerequisites. Stage-4 families retain equal priority and advance as contracts are selected. Conditional tool packages keep their own policy gates; Stage 5 remains unscheduled.

| Stage | Goal | Exit evidence |
|---|---|---|
| **3 — Conditional compatibility follow-ups** | [S3-T5–S3-T7](#conditional-stage-3-follow-ups): namespace aliases, plain custom wrappers and examples projection | Separate policy selection and affected full-loop verification; not default blockers for Stage 4 or optional Agent qualification |
| **4 — Modalities, Decision & file resources** | Decision, Embedding extensions, Audio (Speech/Transcription/Translation), Images, Files/Uploads, and Video at equal priority | Verified request/result contracts, media bounds, resource isolation, cancellation, and SDK loops |
| **5 — Unscheduled follow-ups** | Citations, tool discovery, hosted tools, server-side compaction, Batch, and Realtime | Requires explicit per-slice design selection and scheduling prior to implementation |

### Execution sequence and work-package rules

Prefer small, independently decidable repairs before complex integration; diagnostic global → protocol → Provider/model layers are not a mandatory work queue or cross-product sweep. Reuse existing schema, lowering, execution, typed history and isolated current-client drivers. Every selected work package follows this lifecycle:
1. **Entry:** Pin source revisions, dialect rules, target profiles, and unresolved decisions. Document protocol findings in `docs/refs/`.
2. **Delivery:** Implement one coherent request/result/stream/history slice. Resolve unselected structures with a minimal counterexample and options before proceeding.
3. **Invariants:** Maintain immutable source IR and fresh per-attempt decoder state. Native/equivalent history projections preserve earlier turns; selected lossy projections record rendered-prefix/cache changes under the [candidate-local contract](architecture.md#candidate-local-projection).
4. **Evidence:** Provide independent directional tests and focused failure assertions at the responsible layer. Classify each field's ladder level and give rejections their L5 reason.
5. **Exit:** Run the offline baseline and relevant SDK gates before commit and push. Remove completed roadmap packages and record decisions in [Architecture](architecture.md).

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
| Images Generate | Grok static codec implemented; public contracts under review | Select public default encoding (PNG vs Base64) and response metadata |
| Images Edit & Stream | Research complete; mask boundaries and stream formats unselected | Await static Generate closure |
| Audio (Speech/ASR) | Static paths exist; streaming and multipart unselected | Select bounded single-operation profile |
| Files & Uploads | Safety requirements defined; storage authority unselected | Select storage authority and ID authorization |
| Video | Upstream API research available; job lifecycles unselected | Select job status and retrieval contract |

#### Grok Imagine adaptation

Before public Generate activation, resolve dimensions, quality tiers, creation timestamps, default encoding and response metadata using the existing static codec. OAuth must use the shared account source without inline login. Video integration requires a native asynchronous job contract rather than a synchronous facade.

### Stage 5 backlog

All Stage 5 tasks are currently unscheduled:
- **Citations & source attribution:** Unified citation models ([1C proposal](api-expansion-design.md#1c-design-messages-citations)).
- **Tool discovery:** Dynamic tool discovery and native `web_search`.
- **Hosted tool execution:** Separate contracts for Provider-executed tools and observations; ordinary declarations, aliases and custom-text adaptation remain in the Stage-3 tool work.
- **Server-side compaction:** Provider-executed `/responses/compact` operations.
- **Batch:** Asynchronous job processing and reconciliation.
- **Realtime:** Bi-directional streaming sessions, interruption, and media transport.

## Test supplementation

The test strategy uses pinned upstream assets and standard SDK consumers rather than hand-maintained mock matrices.

### Stage-4 reuse and acceptance

1. Identify applicable official SDK fixtures for each selected operation.
2. Verify MorphieCore request preparation and response decoding against upstream test cases.
3. Preserve the full offline baseline while adding operation-specific lifecycle and safety checks.

## Phase rules

### Observability follow-up

Application-side [failure diagnostics](architecture.md#observability), bounded private body context and custom-file removal are delivered. Real receiver/backend activation, centralized cluster topology, public diagnostic schema stabilization and metrics aggregation still await production packaging.

Safe-to-ignore diagnostic extensions may be added without dedicated extension tests. That exception does not waive the selected logger/capture replacement's behavior, redaction, bounds, cancellation and exporter checks. Further aggregation remains deferred; existing standard Agent-loop and authoritative evidence checks remain required.

### Agent authorization and diagnostics follow-up

- **Session design:** Opt-in [per-Agent JSONL session storage](architecture.md#agent-session-files) is unscheduled; select boundary isolation and lifetime bounds before implementation.
- **Deferred scope:** Dynamic OAuth authorization servers, cross-agent token exchange, and cluster IAM remain unscheduled.

### Temporary routing freeze

The [accepted account contract](provider-account-redesign-plan.md) and [routing decisions](architecture.md#routing-and-security-decisions) constrain future work. Further implementation does not authorize changing deployed credentials or activation.

The [stable routing baseline](architecture.md#failover) remains frozen unless its own change is explicitly selected. Unrelated codec and protocol work must not alter:
- Account membership, health probes, or target ordering.
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
| Development slice | Focused owner/codec tests pass; regressions isolated | Continue authorized implementation within its scope; unrelated work stays under the routing freeze |
| Commit and push | Full [offline baseline](development.md#offline-baseline) passes for implementation; [documentation checks](development.md#documentation-checks) for docs only | Commit completed slices; publication follows project authorization |
| Integration acceptance | Connected SDK scenarios and client loops pass | Declare client/protocol compatibility |

## Integration handoff

Carrier availability in codecs does not imply public route admission. Route admission is controlled by explicit target binding policy in [topology compilation](../src/topology/compile.rs).

### Partial-delivery exit

**User-approved 2026-11-07:** principal workflows are usable, selected low-loss conversions record their losses, affected paths are verified, and remaining difficulties have explicit scope for next-stage analysis.

1. Preserve the representative real tool/history paths across Haiku Messages, DeepSeek readable Chat and Grok Responses, plus the existing Qwen Responses coding path. Use installed pi/OpenCode for their affected client boundaries; reuse still-applicable evidence rather than requiring every client × model × protocol combination.
2. Verify each selected repair before claiming it complete. A new ambiguity may defer its dependent subcase without delaying unrelated slices.
3. Apply required offline and applicable SDK gates before publishing implementation. Recheck changed real workflows, including honest incomplete/cancelled delivery and continuation where affected. No broad re-sweep is required merely to preserve test counts.
4. Classify each remaining observation as resolved, a scoped limitation, next-stage analysis, or an unavailable prerequisite. A recorded deferral is sufficient disposition; closing every R identifier is not an exit condition.
5. Unscheduled issues may be selected alongside relevant Stage-4 work; it does not make Stage 5 features or production activation implicit.

### Public route activation review

Prerequisites for activating newly implemented codec carriers on public routes:
1. Classify each field on the conversion ladder: exact, equivalent, approximate, omitted or rejected.
2. Verify preflight/preparation agreement on candidate bindings.
3. Validate routing stability under failure and reconnection.
4. Complete offline baseline and connected SDK acceptance before client validation.

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

Further general Google API expansion remains paused. Maintenance of qualified existing paths needs a concrete selected defect; residual strict/replay boundaries follow the [unscheduled issue register](#unscheduled-issues). The delivered pi-first stateless carrier leaves OpenCode adaptation and real-Provider qualification deferred. Preserve the existing [terminal-report exceptions](architecture.md#service-policies-and-reports) and [native text Embedding path](architecture.md#independent-embedding-path). General multimodal input, native client intake, custom tools, unified error parsing and broader probability reports remain excluded.

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
Discovery for other task families (Embeddings, Media, Decision) remains schema-only until those operations are publicly activated. Extend the existing [Generation discovery contract](architecture.md#generation-text-model-discovery) only with selected operation contracts.

### Messages citations
Unified citation delivery ([1C proposal](api-expansion-design.md#1c-design-messages-citations)) remains unscheduled. Requests requiring citation output are rejected.

## Deferred acceptance

| Target | Condition for acceptance |
|---|---|
| Non-image multimodal real checks | Stage-4 modality slice implementation |
| Current-client image replay | Selected image client profile verification |
| Real multi-provider failover | [R09](#routing-qualification) prerequisites: dedicated credential pairs and reproducible fault injection |
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
