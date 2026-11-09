# Roadmap

This page owns current scope, priorities, dependencies, and exit gates. [Architecture](architecture.md) owns accepted design, [API expansion design](api-expansion-design.md) owns unselected proposals, and [Development](development.md) owns verification procedures. Code and tests define behavior. Completed slices are removed; Git preserves history.

## Contents

- [Current status](#current-status)
- [Remaining Agent qualification](#remaining-agent-qualification)
  - [AR5 closure order](#ar5-closure-order)
  - [Remaining observation dispositions](#remaining-observation-dispositions)
    - [Tool invocation and argument guarantees](#tool-invocation-and-argument-guarantees)
    - [History and replay carriers](#history-and-replay-carriers)
    - [Routing and account failover](#routing-and-account-failover)
    - [Instruction following and content recall](#instruction-following-and-content-recall)
    - [Other qualification gates](#other-qualification-gates)
    - [Partial-delivery exit](#partial-delivery-exit)
- [Deferred protocol follow-ups](#deferred-protocol-follow-ups)
  - [Endpoint qualification and emitter follow-ups](#endpoint-qualification-and-emitter-follow-ups)
  - [Conditional Stage-3 follow-ups](#conditional-stage-3-follow-ups)
    - [Tool verification and scope gates](#tool-verification-and-scope-gates)
  - [Retained rejections](#retained-rejections)
  - [Remaining issue dispositions](#remaining-issue-dispositions)
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
  - [P7 boundary review](#p7-boundary-review)
- [Pending decisions](#pending-decisions)
  - [Ownership and projection decisions](#ownership-and-projection-decisions)
  - [Scheduling and product decisions](#scheduling-and-product-decisions)
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

**Current focus (2026-11-09):** Treat **AR5 as one consolidated problem set**, in order: global semantic and lifecycle boundaries, protocol contracts and carriers, then Provider/model dialects. Google and strict guarantees participate from the start; their earlier postponement only narrowed the temporary analysis surface and no longer sets priority. Follow the [AR5 closure order](#ar5-closure-order) and [function-grouped dispositions](#remaining-observation-dispositions), reusing the selected carrier repairs and completed qualification. R21 evidence participates in shared analysis, while its Provider-specific retest/repair remains deferred. External-client repairs remain excluded, and Stage-4 contract selection is not the current focus. The [partial-delivery exit](#partial-delivery-exit) does not require closing every observation. [AR5 Review](ar5-review.md) owns observations and passing evidence; production activation remains separately approved.

| Priority | Remaining work | Exit |
|---|---|---|
| AR5 first | Consolidate shared causes, resolve protocol boundaries, then isolate dialect-specific gaps | Verified repair or explicit attribution/disposition; preserve guarantees, replay guards and affected-path acceptance |
| Conditional tool follow-ups | [S3-T5–S3-T7](#conditional-stage-3-follow-ups): namespace aliases, custom-text wrappers and examples projection | Separate policy selection and full affected-loop acceptance; not default prerequisites for AR5 |
| API expansion | Select and implement Stage-4 operation contracts | Per-operation lifecycle, protocol and SDK acceptance |
| Downstream integration | Implement opt-in per-Agent JSONL session storage | Selected isolation and lifetime bounds |
| Production preparation | Decide diagnostic packaging and deferred observability scope | Explicit deployer selection |

## Remaining Agent qualification

The remaining AR5 boundaries are the current priority. [AR5 Review](ar5-review.md#pending-observations) owns their evidence; [Architecture](architecture.md) owns accepted contracts, and [Development](development.md#production-profile-acceptance) owns bounded qualification procedures. No broad Provider sweep, credential reprovisioning or account rewrite is scheduled.

### AR5 closure order

**User-selected analysis order, 2026-11-08:** Consolidate all retained AR5 observations before choosing another target-specific slice. Analyze global issues first, specific protocols second, and Provider/model dialects last. This supersedes the earlier Chat/Responses-before-Messages/Google tiers, strict deprioritization and next-Qwen-cell scheduling. Google and strict analysis are active, not conditional on discovering a carrier first. Semantic changes still require their boundary-specific selection.

| Order | Analysis scope | Required result |
|---|---|---|
| **1 — Global** | Across all nine IDs: tool identity/cardinality/seriality versus argument guarantees (R05/R17/R18); ordinary attachments versus protected reasoning and replay trust (R01/R02/R06/R09); instruction preservation, upstream content attribution and cancellation/feedback ownership (R13/R21) | Identify the authoritative IR/execution owner, first divergent boundary and shared cause or evidence gap. Keep genuinely different guarantees separate even when they share an investigation. |
| **2 — Protocol** | Compare Chat, Responses, Messages, generateContent and Interactions against their primary contracts, client-retainable carriers and existing SDK/client evidence, including Google and strict semantics | Record exact/equivalent/lossy/omitted/rejected dispositions and minimal counterexamples. Resolve shared carrier choices before adding target exceptions; unsupported mappings retain honest rejection pending selection. |
| **3 — Provider/model dialect** | Investigate only residual differences after the global/protocol review: Qwen/GLM forcing and strict behavior, real Haiku protected output, independent-account failover and remaining scoped upstream content limitations | Qualify the exact model/mode/binding with bounded discriminators, or record attribution/unavailable prerequisites. R21-specific retest/repair still needs separate selection; no repeated broad sweep. |

These are analysis layers, not three new implementation packages or mandatory all-cell test sweeps. Reuse the existing evidence across the functional dispositions below. A blocked live prerequisite does not prevent source/code analysis or work on another shared boundary. Completed carrier repairs and the Qwen thinking-off JSON/SSE qualification belong to [Architecture](architecture.md#google-strict-function-arguments) and [AR5 evidence](ar5-review.md), not another queued target sweep.

Reuse the existing lowering/disposition owners, codecs, pinned SDKs and isolated client drivers; no new dependency or test framework is planned. Fix each live run's target, budget, caps and deadline before sending under the bounds below. Run one coherent slice at a time, fixing faults at their shared owner before protocol or dialect specialization. If a carrier decision or live prerequisite is unavailable, record the precise blocked subcase and continue independent analysis; ordering does not authorize semantic loss. Implementation exits require directional regressions, the offline baseline, relevant SDK checks and affected real-client acceptance. Documentation-only disposition changes use documentation checks. Once the retained boundaries have verified repairs or explicit dispositions, use the partial-delivery exit rather than inventing more AR5 sweeps.

### Remaining observation dispositions

There are **nine retained observation IDs across four functional domains**, matching the [AR5 inventory](ar5-review.md#pending-observations). They form one analysis backlog under the global → protocol → dialect order above; the domains below locate detailed dispositions, not separate priority queues. Status and re-entry are attributes of each boundary, not separate copies of its plan. R18 has independent selection and strict-argument subcases; R02/R06 share one carrier problem across client protocols. The existing Readable Chat carrier extension is selected in [Architecture](architecture.md#chat-reasoning-extensions); this reorganization selects no deployment, new binding or additional content/signature loss.

**User-selected scope, 2026-11-08:** Bailian structured-output variability and its associated envelope/failed-stream observations are removed from AR5 follow-up. No further investigation, retest or repair is scheduled for that issue; existing format carriage and envelope normalization remain unchanged.

**User-selected removal, 2026-11-08:** Retire **R04/R08** and their external-client repair/attribution follow-ups from AR5. Do not substitute gateway omission, history equivalence or deduplication work. Existing history/replay assertions and test-authority forwarding ceilings remain; retirement is not a repair or broader compatibility qualification.

Use the existing lowering, replay/projection owners, reducers and client drivers for separately selected follow-ups. No new dependency, replay store, retry layer or diagnostic platform is planned.

**User-selected status, 2026-11-08:** All retained observations contribute to consolidated analysis, including Google, strict guarantees and existing R13/R21 attribution evidence. R21-specific Provider retest/repair remains deferred; R13 does not require another recall sweep. Analysis does not select weaker explicit guarantees or content/signature loss. The routing freeze remains. The general Google API expansion pause does not defer AR5 source, semantic or carrier analysis of existing paths.

Retain the exact profile, first divergent field/event, responsible owner and disposition for any follow-up. Use known field paths, event/block kinds, terminal classes and recognized error categories; private exchanges are not public fixtures. If fresh Provider evidence is necessary, fix target, operation, request/attempt budget, output/wire caps and deadline before each run, and run serially without hidden retries.

**User-selected follow-up bounds, 2026-11-09:** incomplete evidence may receive unlimited additional explicit Provider trials, superseding the temporary three-scenario/repeat ceilings. Each run still fixes finite requests, output/wire caps and deadlines before dispatch. Record each outcome independently; success does not erase earlier failures. OAuth renewal through the project manager/CLI remains authorized. This changes investigation scheduling, not gateway/SDK retry policy or the manager's quarantined-refresh lifecycle.

Stop the dependent live run or implementation when its prerequisite or semantic selection is unavailable; continue independent global/protocol analysis. A later implementation slice needs independent directional regressions, the required offline baseline, relevant SDK/client checks and affected real acceptance. Preserve credential/auth, bounds, cancellation, replay isolation and terminal honesty.

#### Tool invocation and argument guarantees

Invocation identity, seriality, argument-schema guarantees and terminal spelling are separate contracts. Preserve the accepted [strict-tool ownership](architecture.md#standard-request-boundary) and [Bailian tool policy](architecture.md#shared-aliyun-billing-dialect); passing one dimension does not reopen another.

| Boundary | Current disposition | Next work and exit / re-entry condition |
|---|---|---|
| **R18 — Forced invocation** | Scoped upstream limitation: known unsupported thinking profiles keep their selection rejection. Explicit thinking-off Qwen Responses forcing is qualified in JSON/SSE. | Re-enter the rejected profiles for a concrete contract/model change or new local divergence. Keep invocation, cardinality, seriality and arguments separate; the completed non-strict thinking-off SDK cell does not qualify default-pi strict admission. No repeated thinking/serial/call-terminal sweep. |
| **R17 / R18 — Strict arguments** | Scoped upstream limitations after shared carrier review; GLM Chat and Bailian Responses retain explicit-strict rejection. Google R05 has its separate qualified subset. | Re-enter for a concrete equivalent function carrier or changed target enforcement, qualifying arguments independently of tool identity/terminal spelling. Bailian's documented structured-output strictness is not a function-tool guarantee. Non-strict diagnostic variants are not strict opt-outs; no schema deletion or local repair/retry layer. |
| **R05 — Google strict profile** | [Closed-object argument subset selected](architecture.md#google-strict-function-arguments) on existing APIs; full original Agent profiles remain unqualified outside that schema/control subset. | Broaden only for a concrete supported constraint/carrier with independent evidence. Length/pattern/reference/composition constraints and Google serial intent remain rejected; do not opt out of strictness or delete schemas to qualify an Agent. Ordinary attachment carriage in R02/R06 remains separate. |

#### History and replay carriers

Ordinary content/call attachments and protected reasoning have distinct semantic owners. Follow [history and replay](architecture.md#history-and-replay); the selected Readable Chat reasoning transport does not authorize ordinary signature transport.

| Boundary | Current disposition | Next work and exit / re-entry condition |
|---|---|---|
| **R02 / R06 — Google ordinary attachments** | Scoped stateless limitation; the user selected existing managed Responses continuation, without a new stateless carrier or signature loss. | Use `store:true` / `previous_response_id` on the supported path. Stateless Chat/Responses/Messages retain honest attachment rejection. Reopen stateless support only with a selected client-retainable ordinary text/call carrier and client integration; never silently delete signatures or move ordinary content into reasoning. No new Google API activation. |
| **R01 — Real Haiku protected output** | Only the unobserved real protected-output cell remains; the existing-profile carrier repair is selected and verified. | Resume only when protected output is available on the same profile, preserving signatures, replay guards and original assertions. No new carrier/loss decision or repeat of completed synthetic/SDK/client carrier loops is needed. |

#### Routing and account failover

| Boundary | Current disposition | Next work and exit / re-entry condition |
|---|---|---|
| **R09 — Real account-compatible failover** | Real stateless JSON/SSE replay isolation is qualified. The project loader reports one enabled declared account per Provider; no independent compatible pair is qualified. | Resume with independent compatible accounts on one authorized same-model route. Verify A success/affinity, a bounded A failure without switching inside that request, B selection on a new request with A-private replay omitted, then B success/stickiness. Define fault injection and budgets first; provisioning new credentials or other unapproved lifecycle changes requires separate approval. No repeated transfer sweep; same-key aliases, different-model switching and synthetic authorities cannot close this real-account gate. |

#### Instruction following and content recall

Keep upstream content attribution separate from request preservation, tool policy, cancellation and feedback mechanics. Exact-label assertions remain; prompt/history rewriting is not selected.

| Boundary | Current disposition | Next work and exit / re-entry condition |
|---|---|---|
| **R13 — Messages literal-label recall** | Scoped upstream omission on complete native comparisons; reopened for bounded follow-up, not a scheduled recall sweep. | Re-enter only for a concrete new project-owned divergence. Reuse native/prepared/delivered evidence, keep the exact-label assertion and separate HTTP failures/incomplete captures. No prompting or history rewrite. |
| **R21 — Grok recovery content/tool behavior** | Existing evidence participates in global instruction/attribution and lifecycle analysis with R13; **Provider-specific retest/repair remains explicitly deferred**. | Reuse qualified cancellation/write-feedback mechanics and original content assertions to distinguish request preservation, execution ownership and upstream behavior. Resume Provider-specific work only when the user selects a concrete new local divergence, upstream contract change or explicit recovery-policy contract. Prompt-only no-tool text must not become `tool_choice:none`; no prompt/history rewrite or repeated shape/feedback-budget sweep. Deferral does not claim content compliance or block partial delivery. |

#### Other qualification gates

These previously retained, unnumbered profile gates are separate from the nine observation IDs. They are not new scheduled tasks or prerequisites requiring another broad sweep; resume only when separately selected.

| Function | Retained gate |
|---|---|
| Replay carriers | Interactions signed-call replay remains unqualified because no signed call was observed. Thought-replay acceptance does not close this call-signature cell. |
| Output ceilings and recovery | Effective non-Qwen Bailian reply-ceiling enforcement and Haiku same-process cap-switch recovery remain unqualified. Native-ceiling preparation receipts and GLM JSON smoke do not close them. |
| Endpoint contracts | Luna router endpoints remain unqualified; SIWC retains its [inline-tool continuation follow-up](#endpoint-qualification-and-emitter-follow-ups). No new binding or endpoint activation is selected. |

#### Partial-delivery exit

**User-approved 2026-11-07:** principal workflows are usable, selected low-loss conversions record their losses, affected paths are verified, and remaining difficulties have explicit scope for next-stage analysis.

1. Preserve the representative real tool/history paths across Haiku Messages, DeepSeek readable Chat and Grok Responses, plus the existing Qwen Responses coding path. Use installed pi/OpenCode for their affected client boundaries; reuse still-applicable evidence rather than requiring every client × model × protocol combination.
2. Verify each selected repair before claiming it complete. A new ambiguity may defer its dependent subcase without delaying unrelated slices.
3. Apply required offline and applicable SDK gates before publishing implementation. Recheck changed real workflows, including honest incomplete/cancelled delivery and continuation where affected. No broad re-sweep is required merely to preserve test counts.
4. Classify each remaining observation as resolved, a scoped limitation, next-stage analysis, or an unavailable prerequisite. A recorded deferral is sufficient disposition; closing every R identifier is not an exit condition.
5. Deferred AR5 analysis may be scheduled alongside relevant Stage-4 work; it does not make Stage 5 features or production activation implicit.

## Deferred protocol follow-ups

Only unresolved endpoint qualification and protocol compatibility follow-ups are listed here. Apply the accepted [conversion ladder](architecture.md#conversion-ladder), [field handling](architecture.md#field-handling), [projection principles](architecture.md#clientir-projection-principles) and [stream normalization](architecture.md#semantic-stream-normalization).

### Endpoint qualification and emitter follow-ups

| Boundary | Remaining work | Entry condition |
|---|---|---|
| SIWC Responses inline-tool continuation | Real endpoint acceptance for inline declarations and direct call/result history | Select the endpoint/profile and a bounded full-loop run; retain root namespace requirements |
| Chat upstream usage | Qualify duplicate nonnull final reports at the emitter | A concrete upstream profile and independently observed report sequence |

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

These L5 boundaries and invariant guards constrain future work. Endpoint-specific qualification follows the [remaining scope](#endpoint-qualification-and-emitter-follow-ups).

| Boundary | Retained |
|---|---|
| Chat → IR | `n > 1`, `web_search_options`, nonempty `logit_bias`, `store:true`, `moderation`, legacy `functions`/`function_call`/`role:function` |
| Responses → IR | Hosted tools and items, unselected `include` families, background execution, active conversation/prompt/moderation and context-management operations, item references outside the managed store. Local `store`/`previous_response_id` stay supported through [managed ingress](../src/gateway/ingress/admission.rs) |
| Messages → IR | `between_tools`, message-local null/reset and unselected format/task-budget controls, `clear_at`, root task budgets pending selection, `container`, `mcp_servers`, context management and compaction, server tools and non-direct callers, block-binding `"error"`, `inference_geo`, enabled citations, null root output effort/format pending field evidence |
| Request → target | Media, tools or explicit content/schema constraints without a carrier; explicit strict guarantees without a carrier; `stop` without a carrier; compaction without a carrier; explicit history-read intent that cannot be carried. Output ceilings instead follow their selected low-loss disposition |
| IR → client | Chat custom-call SSE; Responses SSE cancellation; unknown Chat/Messages stop causes without an evidence-backed map; non-object Messages tool input other than an empty string; call ID or name lengths beyond the client limit; blocking safety reports; retraction of published accounting |
| Invariants | Credential/origin/trust selection, replay origin and format trust, client file IDs, resource bounds, malformed known values, terminal honesty, incomplete tool input never executable |

The [pinned Anthropic SDK](../tests/upstream/sources.lock.json) types already include `between_tools`, optional-nullable output configuration and zero-token cache prewarming; their rejections are capability-selection gaps, not invalid syntax. Nonempty Messages tool openers followed by nonempty fragments concern native upstream SSE intake, not Client request intake.

### Remaining issue dispositions

The [2026-10-28 source and feasibility recheck](refs/2026-10-27-client-ir-boundaries.md#follow-up-source-and-feasibility-review-2026-10-28) supports these retained dispositions. Reopen only a selected target or field with a concrete unmet contract.

| Issue | Current disposition | Reopen or complete when |
|---|---|---|
| Chat custom-tool SSE | Retain standard rejection; investigate upstream intake and downstream delivery separately | Official event contract resolves the disagreement, or a separately selected dialect/client contract supplies independent wire/consumer evidence |
| Nonempty Messages tool opener plus nonempty fragments | Retain rejection, including a complete-fragment negative regression; complete opener plus zero-byte deltas remains supported | Authoritative replacement/merge semantics exist |
| Bounded token-budget adjustment | Model-maximum clamp selected; broader formulas deferred; native effort and existing validation preserved | An authorized target plus failing request, limit provenance, useful reserve, finite maximum and infeasible behavior qualify a target-local rule |
| `user:null`, Chat refusal requiredness, Responses image-detail default | Preserve current intake; image `detail:"auto"` already takes L2 | Service contract clarifies the discrepancy or a selected strict consumer exposes a concrete failure |
| Legacy function carriers | Retained rejection | A real client needs a constrained association/default/conflict/history contract |
| Broader settings echo and richer output grouping | Case-specific ladder application | A concrete failure has an exact mapping or a bounded approximation; preserve report authority and content-coordinate ownership |
| Messages custom-input wrapping | Conditional Stage-3 follow-up S3-T6; not a core exit prerequisite | Declaration, arguments, return/history mapping and client execution expectations are selected together |
| Token scope and SDK drift | Investigate only the selected target/field; no automatic source-pin promotion | A bounded budget rule or reviewed version change has independent expectations and a reachable target path |

## Planned order

Stage-4 API families have equal priority and advance as their operation contracts are selected. Conditional compatibility packages need their own policy selection and are not default prerequisites. Remaining Agent limitations may be qualified alongside relevant work; Stage 5 remains unscheduled.

| Stage | Goal | Exit evidence |
|---|---|---|
| **3 — Conditional compatibility follow-ups** | [S3-T5–S3-T7](#conditional-stage-3-follow-ups): namespace aliases, plain custom wrappers and examples projection | Separate policy selection and affected full-loop verification; not default blockers for Stage 4 or optional Agent qualification |
| **4 — Modalities, Decision & file resources** | Decision, Embedding extensions, Audio (Speech/Transcription/Translation), Images, Files/Uploads, and Video at equal priority | Verified request/result contracts, media bounds, resource isolation, cancellation, and SDK loops |
| **5 — Unscheduled follow-ups** | Citations, tool discovery, hosted tools, server-side compaction, Batch, and Realtime | Requires explicit per-slice design selection and scheduling prior to implementation |

### Execution sequence and work-package rules

Every work package follows this lifecycle:
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

Centralized OTel Collector forwarding, public diagnostic schema stabilization, and cluster metrics aggregation await production packaging.

Safe-to-ignore diagnostic extensions may be added without dedicated extension tests. Information organization/aggregation and those tests await the deployer's final production-preparation decision; existing standard Agent-loop, authoritative evidence and safety checks remain required.

### Agent authorization and diagnostics follow-up

- **Session design:** Implement the selected opt-in [per-Agent JSONL session storage](architecture.md#agent-session-files), including boundary isolation and lifetime bounds.
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

### P7 boundary review

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

### Scheduling and product decisions

| Area | Current policy | Scope |
|---|---|---|
| Target binding activation | Requires P7 review and verified preflight agreement | Target routes only |
| Models discovery extensions | Generation Text active; other modalities schema-only | Discovery endpoint only |
| Cache prewarming | Deferred; requires dedicated contract selection | Operation-specific |
| Token counting | Deferred; upstream-first policy selected | `POST /v1/messages/count_tokens` |
| API expansion | Stage-4 families have equal priority; Stage 5 remains unscheduled | Specific modalities |
| Files & multipart uploads | Stage 4 priority; requires storage authority selection | File resource endpoints |

## Deferred work

### Google API pause

Further general Google API expansion is paused; this is not a pause on consolidated AR5 analysis of existing Google paths, strict guarantees or replay carriers. Those boundaries follow the same global → protocol → dialect order as every other retained AR5 issue. Preserve the existing [terminal-report exceptions](architecture.md#service-policies-and-reports) and [native text Embedding path](architecture.md#independent-embedding-path). General multimodal input, native client intake, custom tools, unified error parsing and broader probability reports remain excluded from this work.

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
