# Roadmap

This page owns current scope, priorities, dependencies and exit gates. Start with the next work package; it is not an approval record or implementation specification. [Architecture](architecture.md) owns accepted design, [API expansion design](api-expansion-design.md) owns unselected proposals, and [Development](development.md) owns verification procedures. Code and tests define behavior. Remove completed slices; Git keeps history.

## Contents

- [Current status](#current-status)
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

**Current handoff (2026-10-25):** the selected Generation Text implementation is closed after the overall correctness, lifecycle, evidence and redundancy review. Unified [domain evidence](architecture.md#domain-evidence-interface) and [local telemetry](architecture.md#observability) are implemented, not pending migrations. The [closure checks](development.md#text-and-sse-integration-baseline) and separately bounded [current-host/Provider reacceptance](refs/2026-10-17-generation-contracts.md#current-host-closure-recheck-2026-10-25) pass with explicit exclusions. Stage 4 now contains Decision, Embedding, Audio, Images, Files/Uploads and Video at equal priority; no further operation, routing feature or deployment is activated by this handoff.

| Area | State | Next step |
|---|---|---|
| Routing | Functional phase complete under the [stable routing baseline](architecture.md#failover); feature development closed. Selected [controlled acceptance](#controlled-routing-acceptance) passed through Router, SDK and installed clients | [Production closure](#generation-text-production-closure) requires separate real evidence for routes promising failover; no routing feature expansion |
| Generation IR/codec | Selected ownership/mapping corrections, standard delivery/continuation and unified evidence implemented within the [contract inventory](refs/2026-10-23-stable-protocol-contracts.md#official-contract-difference-inventory); final review repairs are verified | Preserve native-only replay, participant attribution, directional presence, sparse accounting and checked evidence; inventory exclusions and source disagreements remain explicit |
| Client compatibility | Rejection audit, [phase transfer/return](architecture.md#message-phase-transfer), guarded extended-retention omission and retained citation rejection complete. [Downstream schema completion](architecture.md#downstream-schema-completion) implemented on 2026-10-23 with strict pinned-SDK JSON/SSE and stored-resource checks | Preserve SDK schema, source-fact and terminal-honesty regressions; broader official semantics remain explicitly scoped |
| Integration | Selected R1a/R1b/R2 client loops and controlled routing acceptance passed their bounded scopes. R1b lifecycle reports and client history shorthand were explicitly selected on 2026-10-20 | [Production closure](#generation-text-production-closure): bounded capability and sustained-operation acceptance, not a full client/model matrix |
| Cross-protocol conversion and counting | Stage 3 closed its selected projection and loop scope; counting is deferred | Preserve that evidence; the [Google pause](#google-api-pause) has selected text-report and stage-4 Embedding exceptions, not general Generation reopening |
| Generation Text production readiness | Historical closure, standards-redesign reacceptance and final 2026-10-25 correctness/current-host review are complete within explicit exclusions | Preserve the [selected readiness boundary](#generation-text-production-closure); proceed to per-operation stage-4 selection, not broader production claims or deployment |
| Code simplification | The 2026-10-25 closure removes demonstrated redundant state, forwarding and unreachable checks; no scheduled cleanup remains | Preserve independent safety assertions and production/test boundaries; no count target, speculative rewrite or measured-performance claim |
| Decision APIs | [Dual standard-SDK direction](architecture.md#decision-api-direction) selected on 2026-10-23; initial TypeSafe/OpenAI/Perplexity/Cloudflare protocol research only | [Decision closure](#decision-api-closure) is stage-4 work at equal priority with the other API families; its design selection still precedes implementation and activation |
| Embedding, other modalities and file resources | Sample-centered Embedding, compatible delivery and native Google/OpenAI-model integration accepted on 2026-10-23. Generation prerequisites are closed within their scoped gates; Grok static codec still has no gateway/live acceptance | API families have equal priority under [stage-4 selection](#stage-4-scope); static Generate is an Images-internal review recommendation. Later slices inherit [operation-level test acceptance](#stage-4-reuse-and-acceptance); further Embedding modalities and video remain unselected |
| Stage-5 follow-ups | Citations, tool discovery, hosted tools, server-side compaction, Batch and Realtime are recorded but **unscheduled** | No implementation queue or delivery commitment; select each slice before starting it |
| Downstream integration | [Startup-only Agent API keys and minimal diagnostic attribution](architecture.md#agent-identity-and-cluster-diagnostics) implemented; [per-Agent JSONL sessions](architecture.md#agent-session-files) selected as a direction within the current downstream stage | Continue current-stage session design and the separately scoped model-discovery review; concrete migration, OAuth, managed registration, usage aggregation and cluster diagnostics remain unactivated |
| External protocol tests | Scoped [text/SSE gate](#current-text-and-sse-gate) closed on the verified release baseline: independent complex-history oracles, strict SDK accumulators, ingress budgets and transport safety. Latest candidates are visible, not silently promoted | Preserve that baseline and review source candidates separately; no comprehensive conformance, unified-report or live-acceptance claim |
| Real acceptance | Current pi/OpenCode Token Plan Chat reasoning loops pass three requests/attempts each; separate native Go/Haiku examples and current-host synthetic gates retain distinct evidence | Preserve delivery/replay versus raw-source fidelity limits, historical profile boundaries and [deferred acceptance](#deferred-acceptance); stronger claims need their own gates |

The [approved Generation baseline](#generation-design-review-gate) leaves inventory exclusions and binding boundaries unchanged. Other real acceptance gates remain open. Google value guards and Interactions snapshots belong to stage 3: no Google client endpoint forms a same-protocol pair. Provider-specific reset dialects, persistence and cooldown tuning need separate evidence and approval; no background probes, balancing or multi-round retries are scheduled.

Phase completeness covers the declared feature scope, not all official Generation semantics; explicit rejection is safer than silent loss but is not completeness. Chat Completions, Responses and Messages are equally important for same-protocol request, response, static delivery, streaming and history replay. Google generateContent and Interactions supply upstream request encoding and SSE intake only; record missing directions rather than implying a complete same-protocol pair.

## Standards reset and redesign

The user accepted the complete design review on 2026-10-23 and required a current-stable dependency/source reset before implementation. [Architecture](architecture.md#protocols) owns the new contracts; [Development](development.md#upstream-test-sources) owns procedures. This is a revision of Generation stages 1–3 and their test gate, not a sixth API stage.

Release reset and the bounded official-contract inventory/initial asset integration are implemented. Sources, reviewed fixtures and local extraction recipes are pinned; executable unmet requirements remain explicitly separate from regression. Standard delivery/continuation now uses official public dialects, native-only replay, typed managed Responses history and classified errors; fabricated manifest owners are removed. Preserve EOF/cancellation, source-binding and standard SDK checks through the remaining slices. This closes those scoped implementations, not all-standard compliance. Extend the inventory and external cases at each responsible owner below rather than restarting a parallel audit framework.

**Scoped closure (2026-10-24):** explicit readable Chat selection, private-field rejection, Qwen thinking-output preflight, independent JSON/SSE/history oracles and strict SDK accumulation pass. Full offline checks, all 16 SDK scenarios and 12 current-host synthetic Agent scenarios pass. Real pi 1.4.0 and OpenCode 2.0.32 each pass three requests/attempts through existing `aliyun-tokenplan-cn-chat` / `qwen3.8-flash`, selected by the user after DashScope's `Arrearage`. The bounded in-memory delivery observer replaces the impossible full response-capture prerequisite without weakening capture safety. Native Go/Haiku retains its independent three-request examples gate; Standard Chat remains a separate control. See [current-host evidence](refs/2026-10-17-generation-contracts.md#current-host-reacceptance-2026-10-24) for failed preparatory runs, exact bounds and exclusions. No raw Provider-to-delivery fidelity, real failover, additional profile or latest-candidate claim follows.

**Unified-report scheduling:** the initial 2026-10-24 deferral let the text/SSE and client/Provider gates close independently. The later same-day selection and implementation superseded that deferral for [internal evidence](architecture.md#domain-evidence-interface); local telemetry followed on 2026-10-25. This does not certify complete adaptation explanations or lift the diagnostic-extension deferral below.

**Diagnostic-extension scheduling decision (2026-10-24):** permit additional safe-to-ignore MorphieCore information now, including selected routing and actual-loss diagnostics for a future deployer-owned Agent Runtime. Do not add extension-specific tests or require a finalized schema, information organization/aggregation, JSON/SSE/retrieval diagnostic parity, SDK retention or UI display in this stage. The deployer will decide how to organize/aggregate that information and construct its tests when preparing the final production deployment; do not create a new prerequisite or activate that work now. This supersedes earlier extension-consumer acceptance requirements, not the execution-owned evidence contract or existing standard Agent-loop, privacy, bounds, cancellation and terminal-honesty checks. Current client/Provider reacceptance remains about the supported standard behavior, not extension certification.

The user resumed the six deferred correctness findings before this handoff. Owner-local repairs and independent regressions now protect effective call participant attribution, reject Messages null/empty normalization, reject unprojected empty Interactions envelopes and unsupported summary-null-to-display conversion, validate sparse accounting premises, and separate nullable report mode from request mode. Their source evidence and remaining exclusions live in the [contract inventory](refs/2026-10-23-stable-protocol-contracts.md#official-contract-difference-inventory), not an unfinished repair queue.

**Instruction review (2026-10-25):** the user separately selected the [DeepSeek parallel-tool policy](architecture.md#selected-request-approximations), superseding only that part of the pause. The [additional review](refs/2026-10-23-stable-protocol-contracts.md#instruction-conversion-review-2026-10-25) distinguishes root versus ordered authority, placement and continuation, and reproduces Messages string/block root asymmetry. Developer promotion and root normalization remain unselected; choose shared root authority/scope and preserve per-block cache ownership before implementing their conversion.

Upstream fixtures and real SDK consumers enter each owner/codec slice as it is rewritten, not as a comprehensive cleanup after implementation. Reuse existing valuable scenarios and safety assertions; classify each affected old expectation as retained, replaced by a source-backed contract, or removed with its obsolete behavior. Keep unresolved semantics visible rather than skipping them as passed.

The initial assets alone do not exhaust the selected gate; the closed [text/SSE baseline](#current-text-and-sse-gate) also exercises version-aware HTTP admission/failure envelopes, complex multi-call Router/SDK cases and deeper transport lifecycle coverage. Diagnostic-extension testing follows the deferral above. Historical success cannot certify updated releases. Runtime routing redesign, new bindings, native Google ingress, counting and stage-5 APIs remain unactivated. Further work follows equal-priority stage-4 selection; Decision is not a cross-family prerequisite.

## Planned order

| Stage | Goal | Exit evidence for a selected slice |
|---|---|---|
| **1 — Same-protocol lossless IR codecs** | Preserve the declared protocol semantics through client request → IR → same-protocol upstream request, and upstream response/events → IR → same-protocol client delivery/history. Prioritize known fields, presence/default meaning, ordered owners, reasoning/replay, usage and terminal facts over cross-protocol convenience | Independent wire→IR and IR→wire expectations in each applicable direction, JSON/SSE agreement and returned-history preservation. Report unsupported fields/directions and actual losses explicitly; round trips, HTTP success and named loss receipts are not lossless evidence |
| **2 — Basic agent tool interactions** | Ordinary function-tool declarations and schema intent, call IDs/arguments, results/errors, streaming, final answers and next-turn history for pi/OpenCode. The client executes its tools; no gateway tool executor is implied | Selected same-protocol tool-call → result → final-answer → replay loops, preserving source data and required reasoning/cache owners. Keep offline, SDK and bounded real-client/Provider evidence separate |
| **3 — Cross-protocol stability and token counting** | Stabilize selected request, response/event and history conversions through exact carriers or approved named adaptations. The selected projection/loop scope is closed; `count_tokens` is [deferred](#deferred-token-counting) | Independent directional expectations and selected loops; admission/preparation agreement and rejection of protected unrepresentable semantics. Deferred counting retains its separate upstream-first policy and exit gate |
| **4 — Decision, Embedding, modalities and file resources** | Close Decision, Embedding and audio/image/video APIs through selected design, task IR, codecs, execution, delivery and acceptance; implement Files and multipart Uploads as resource services | Verify Decision question/answer correspondence and both SDK families, media formats/dimensions, controls, results/events, bounds, cancellation, accounting and applicable SDK/real gates. File resources require authorization, bounded upload/part handling, identity, retention/deletion and restart-consistent mappings |
| **5 — Unscheduled follow-ups** | Citations/source attribution, `tool_search`, hosted tools, server-side compaction, Batch and Realtime | **Unscheduled:** select each scope/design and explicitly schedule its slice before implementation. No start/delivery commitment or prerequisite for stages 1–4 |

The five-stage order is unchanged. The selected [Generation Text production closure](#generation-text-production-closure) is a bounded handoff before stage 4, not a sixth stage, a reopening of stage-3 counting or a claim of complete official Generation coverage.

The selected [standards reset and redesign](#standards-reset-and-redesign) integrated and closed the text/SSE gate without restoring old mappings as the oracle. Preserve that closed baseline throughout stage 4. Its API families have equal priority; none activates stage 5.

“Lossless” means semantic fidelity within an explicit protocol revision and feature domain, not byte-identical JSON or retention of unknown fields. Equivalent wire normalization is allowed. Preserve protocol records in their existing bounded owners; do not introduce raw passthrough or an IR extension bag. A same-named API at a different Provider is not proof of equivalent semantics.

Audit existing same-protocol loss paths and distinguish codec loss from model/product policy. Basic tool-field fidelity belongs to stage 1; stage 2 adds the agent interaction loop. All previously approved conversions are reopened under the new hierarchy; current behavior is not normative merely because an old test asserts it. Deferred features remain explicit coverage gaps, not silently discarded content or complete protocol support.

Design review and focused coverage proceed within each stage, not as a requirement to finish every later-stage design first. Preserve security, bounds, cancellation, replay isolation and honest terminals while replacing obsolete mapping assertions. Existing media safety coverage remains; new modality APIs and execution paths belong to stage 4. Other-agent acceptance keeps its [separate scheduling](#deferred-acceptance).

The P7 boundary review becomes eligible after first-stage local completion, without waiting for deferred cache and tool slices, and is required before route-facing acceptance. Its analysis may proceed alongside design closure.

### Execution sequence and work-package rules

The packages below refine the selected scope, not design approval or binding activation. IDs name coherent deliverables, not a required commit count. Stage-4 package IDs and table order do not rank API families; exact operation contracts still need selection. Stage 5 has no execution queue.

Inference frameworks such as llama.cpp and SGLang are design references, not integration targets. Use their input-preparation, generation-constraint and output-interpretation designs to improve the existing gateway boundaries. This adds no backend work package, engine dependency or generic session/plugin framework; [inference-engine integration is not planned](#not-planned).

1. Stage-1 selected codec work is implemented; use its [inventory and boundaries](refs/2026-10-19-same-protocol-inventory.md) for review and per-loop readiness. No all-official-semantics or real-loop completeness is implied.
2. The selected R1a, R2 and R1b loops completed independently under their own carriers and approval/admission gates. A future loop waits for its own readiness, not every stage-1 package; no additional client/model matrix is scheduled.
3. R2 readiness concerns its implemented pi Haiku profile, not wider native manual-budget or sampling support. An observed defect in a carrier actually used by R2 must still be fixed before accepting it.
4. Complete selected stage-2 loops before routing acceptance and stage-3 conversion work. Keep separately scheduled real failover distinct from controlled synthetic recovery checks.
5. Preserve the closed stage-3 projection/loop evidence. Counting remains deferred; its future implementation waits only for its selected effective-input projection.
6. Apply the [standards reset and redesign](#standards-reset-and-redesign), then close the integrated text/SSE gate before further stage-4 implementation. Preserve safety evidence, not superseded mapping expectations; unrelated API activation remains separate.
7. In stage 4, select each equal-priority operation's contract just in time and apply [operation-level upstream-test acceptance](#stage-4-reuse-and-acceptance). Select file authority before dependent file-reference work, without blocking independent Decision or direct-input Embedding/media slices. File-reference integration waits for authorized resource resolution.

Every implementation package follows the same bounded workflow:

- **Entry:** name the pinned source revision, dialect, feature domain, relevant binding/client profile, existing owners and unresolved decisions. Extend sourced `docs/refs/` notes for protocol findings; do not create run-report documents.
- **Delivery:** implement one coherent request/result/event/history or resource-lifecycle slice. Present a minimal counterexample and options before introducing an unselected semantic structure; a decision blocks only dependent work.
- **Preparation and continuation:** preserve the gateway's existing retained binding for request preparation and response decoding, with fresh mutable decoder state per attempt. Keep candidate projections independent and preserve [history-prefix stability](architecture.md#candidate-local-projection) as turns are appended. These are invariants to maintain during each slice, not a new session/snapshot deliverable; change ownership only for a demonstrated gap.
- **Evidence:** write independent directional expectations and focused failure checks at the responsible layer. Separate equivalent normalization, exact carriage, approved loss, rejection and unassessed cases. Distinguish IR expression, protocol carriers, binding admission, model effectiveness and accepted client loops; a generic tool-support claim cannot establish required/named choice, strict schemas, parallel calls or streamed arguments. Scope new capability acceptance to the selected profile, including history continuation, without dropping existing regression assertions. Prepare the [human review](#generation-design-review-gate) evidence incrementally; no passing test or agent review closes that gate.
- **Exit:** apply the [publication and integration gates](#verification-and-integration-boundary). Report codec, admission, SDK and real-client/Provider status separately, including blocked checks. Commit/push only after the applicable checks; remove completed packages from this page and keep accepted decisions in Architecture.

### Stage 1 work plan

**Selected scope closed; the standards-reset baseline supersedes historical mappings.** Chat, Responses and Messages have independent request/result/event/history expectations within the [current contract inventory](refs/2026-10-23-stable-protocol-contracts.md#official-contract-difference-inventory). The [earlier inventory](refs/2026-10-19-same-protocol-inventory.md) retains its dated acceptance boundaries, not the current regression oracle. Google has no downstream/static pair.

Accepted ownership and projection rules live in [Architecture](architecture.md#protocols); executable coverage lives in the [text/SSE baseline](development.md#text-and-sse-integration-baseline). Unsupported official fields, named losses and source disagreements remain explicit. Neither rejection nor a codec carrier establishes lossless support, binding admission or a real client loop. No stage-1 implementation package remains scheduled.

#### Stage-1 to stage-2 handoff

Future loops require their own exact client shape, admitted binding, model-effective carrier evidence and independent static/SSE/history expectations. Current-host clients are tested independently of pinned protocol sources; preserve the selected [pi Haiku profile](refs/2026-10-17-generation-contracts.md#pi-messages-request-profile-2026-10-19), including adaptive-thinking flags. Every ignored request field must be classified, rather than counted as exact support.

### Stage 2 work plan

**Selected R1a/R1b/R2 loops and controlled routing acceptance are closed.** Their dated evidence records targets, request bounds, client versions, failures and exclusions:

- Chat tool/reasoning history: [R1a source and client evidence](refs/2026-10-17-generation-contracts.md), with later [current-host reacceptance](refs/2026-10-17-generation-contracts.md#current-host-reacceptance-2026-10-24).
- Responses native history and lifecycle reports: [R1b](refs/2026-10-17-generation-contracts.md#responses-lifecycle-reports-2026-10-19). Installed-client opaque replay is not established.
- Native Messages signed history and cache controls: [R2](refs/2026-10-17-generation-contracts.md#r2-native-client-and-replay-evidence-2026-10-19), with the separately selected [production profiles](development.md#production-profile-acceptance).

No further stage-2 loop or client/model matrix is scheduled. Retain tool-error, strict-EOF, cancellation and no-post-publication-replay checks. Synthetic recovery is not real cross-account failover; broader retention or replay claims need their own evidence. Follow the shared [bounded acceptance procedure](development.md#online-acceptance) when a later loop is selected.

### Stage 3 work plan

**Selected projection and cross-protocol loop scope is closed; counting is deferred.** The implemented contracts are [model-effective guards](architecture.md#model-effective-values), [trusted missing-cap defaults](architecture.md#missing-output-ceilings), [attribution](architecture.md#end-user-attribution), [foreground Google intake](architecture.md#google-foreground-intake) and the selected [Messages terminal projection](architecture.md#google-messages-terminal-projection). Candidate-local projection keeps source input immutable; transport does not reinterpret prepared semantics.

Independent Router/SDK JSON/SSE correction/history cases and bounded native/installed-client loops retain their distinct [Google evidence](refs/2026-10-17-generation-contracts.md#google-installed-client-acceptance-2026-10-21) and [Messages closure](refs/2026-10-17-generation-contracts.md#google-messages-basic-loop-closure-2026-10-21). Earlier failed probes remain failures, not acceptance. Native-only replay under the standards reset supersedes fabricated public manifest expectations; managed Responses continuation and native Messages signatures have separate contracts.

No stage-3 implementation package remains scheduled. Buffered basic loops do not establish incremental timing, every multi-call/client combination, accounting accuracy, complete native fields or cross-account failover. Native Google ingress/static intake, broader reports, background execution and model fallback remain excluded. The narrow text-report and standalone Embedding exceptions below do not reopen Google Generation or deferred counting.

#### Google API pause

The user selected basic-loop closure followed by a pause in further Google API improvement on 2026-10-21, accepting incomplete operation outside the selected profiles. Keep the accepted bindings, three-level effort mapping, terminal rules and replay protections; allow necessary correctness/security maintenance without expanding the feature scope.

A later decision on 2026-10-21 selects one [text-report exception](architecture.md#explanatory-terminal-reports): bounded `finishMessage` and the concrete [processing-class reports](architecture.md#service-policies-and-reports) are implemented. The [source recheck](refs/2026-10-17-generation-contracts.md#google-text-report-source-recheck-2026-10-21) establishes the selected v1 carrier and processing meaning; unproven defaults/transitions still reject. The exception adds no Google downstream/static direction or request policy.

On 2026-10-22 the user requested one Google Embedding target, then paused it for review. On 2026-10-23 the user explicitly resumed Google/OpenAI Embedding implementation with named lossy compatibility. This [standalone text exception](#embedding-integration-targets) does not reopen Google Generation, native downstream APIs, other models or multimodal ingestion. The broader Google API pause remains in force.

Probability/token-identity owners, general Generation usage approximations, native Google client/static directions, cache/strict-tool/dynamic-control expansion, new Generation models/modalities, hosted tools and background execution remain unscheduled. A passing basic loop is not full Google conformance. These gaps do not reopen stage 3. `count_tokens` remains independently [deferred](#deferred-token-counting).

### Generation Text production closure

**Selected on 2026-10-21; historical closure on 2026-10-22, redesign reacceptance on 2026-10-24 and final review closure on 2026-10-25.** The bounded offline, SDK, installed-client and Provider gates are complete within their distinct recorded profiles; procedures and exact evidence live in [Development](development.md#production-profile-acceptance) and the [dated notes](refs/2026-10-17-generation-contracts.md#current-host-closure-recheck-2026-10-25). This closes the Generation prerequisite before stage-4 operation work, not public hosting, all official fields, a Provider/model matrix or deployment authorization. Earlier version-specific acceptance remains valid only within its recorded scope; fresh Chat acceptance does not rerun every historical Responses/Messages capability.

Completed contracts:

- [Profile preparation](development.md#initial-production-profile-baseline), recovery, ordinary/split/retried summaries, rebuilt tool history and separate preferred-strict fallback/error characterization passed with installed pi 1.3.0/OpenCode 2.0.32. Auxiliary requests remain explicitly counted; this is not native `/responses/compact`.
- Protected [field handling](architecture.md#field-handling), bounded [explanatory reports](architecture.md#explanatory-terminal-reports) and concrete Google processing reports preserve their selected directional, closure and receipt contracts. Unowned budget/dispatch/execution/billing facts still reject.
- [Native input owners](architecture.md#tool-examples-and-document-context) and [automatic TTL](architecture.md#caller-selected-automatic-cache-admission) have bounded Go/Haiku examples/context/5m/1h write/read evidence and independent all-native admission. The separate [constrained profile](development.md#constrained-messages-profile-acceptance) covers numeric schemas, fixed adaptive thinking/effort, JSON/SSE history, honest truncation and strict required/named controls without weakening.
- [Sustained-operation checks](development.md#sustained-operation-acceptance) prove controlled consumer content before terminal/EOF, cancellation/recovery without replay, stopped-reader admission, small-budget history release/restart and bounded logging saturation/failure/recovery. A separate [one-request live timing](development.md#no-tool-text-timing) observes SDK text/terminal timing without tools; it is not a latency SLO.

The same-date audit closure adds root/by-value example presence and aggregate-node safeguards, target-copy report removal, post-drain recovery verdicts and a scheduling-safe held-content oracle. The strengthened constrained-profile gate checks conflicting prompts and exact prepared history; its bounded rerun passed. Updated recovery/lifecycle evidence uses pi 1.4.0/OpenCode 2.0.32; earlier summary evidence retains its own versions. These repairs do not reopen excluded carriers or add a broader profile claim.

Production exclusions and continuing rules:

- Native `file_citation` insertion units remain unresolved. [Point owners](architecture.md#citation-point-anchors) and independent JSON/SSE rejection tests remain; this is an explicit production exclusion, not a guessed conversion or a completed wire mapping.
- OpenCode `run --format json` reports completed text blocks. Its controlled first-content evidence uses the installed core's SSE delta, not CLI/TUI visibility. Socket release proves neither remote computation nor billing cancellation.
- Initial production claims remain [single-candidate/one-attempt](architecture.md#scope-and-activation-decisions). Real failover needs a separately reproducible advanceable failure, compatible real credential/Provider pair, returned-history/replay compatibility and sticky continuation. Controlled synthetic recovery is not that evidence; never strip signatures to manufacture it.
- Preserve [profile maintenance](development.md#production-profile-acceptance), [observability](architecture.md#observability) and per-slice [publication/integration gates](#verification-and-integration-boundary). Neither source-delta timing nor model-name/SDK support alone establishes an accepted consumer/binding combination.

`count_tokens`, complete citations, cache-only prewarming, hosted tools/discovery, server-side compaction, wider budget scopes, additional clients/models and other stage-5 work remain deferred. Responses history stays process-local TTL storage, not persistence. Production readiness is correct selected text/tools/structured output, history, cancellation, failure handling and diagnosable sustained operation—not accounting accuracy, arbitrary schemas, every real refusal/parallel-call combination or permission to deploy. Each stage-4 operation starts with its own design gate; new semantic policy still needs a focused selection.

### Stage 4 scope

The [API expansion design](api-expansion-design.md) supplies operation proposals, not implemented endpoints or approved type layouts. Keep conversational media in Generation where appropriate and independent operations in their own task contracts. Further implementation requires the completed [Generation Text production closure](#generation-text-production-closure), the [current text/SSE test gate](#current-text-and-sse-gate) and per-operation selection. The API families below have equal priority; stage-4 work does not reopen completed stage-1/2/3 gates.

**Scheduling decision (2026-10-25): Decision closure belongs to stage 4 alongside Embedding, Audio (Speech/Transcription/Translation), Images, Files/Uploads and Video, all at equal priority.** This supersedes Decision-first and Embedding/Images-first cross-family ordering. Select coherent slices by their contracts, readiness and real dependencies, not a fixed API ranking. Independent families need not wait for one another; equal priority neither requires simultaneous implementation nor approves unresolved contracts, bindings or calls. Existing Embedding acceptance remains valid; new Decision/media/resource implementations still require their own selection.

The current review exit for unselected operations is a coherent request/result and applicable event contract, source-backed model restrictions, bounded admission/execution/delivery and counterexamples for unresolved choices. The [accepted Embedding contract](architecture.md#independent-embedding-path) and [Image design choices](api-expansion-design.md#image-design-review) are distinct. Record newly selected structures in Architecture before dependent implementation or ordinary comprehensive coverage; earlier approvals do not extend to unresolved operations.

Every selected implementation slice also meets the shared [upstream-test acceptance conditions](#stage-4-reuse-and-acceptance). The table below adds operation-specific obligations; external tests do not replace local IR, resource or lifecycle evidence.

| Package | Dependency and work | Exit condition |
|---|---|---|
| S4-0 Per-operation design | Pin current official request/result/event contracts, choose one target and task/resource owners, and state exclusions. Select file authority before dependent references; select operation contracts just in time rather than requiring a complete redesign upfront | New semantic structures and execution-policy choices have explicit decisions. Existing owners/shared values are reused where appropriate; no universal task bag, repository-wide rewrite or implicit new binding |
| S4-6 Decision closure | Complete the [Decision design gate](#decision-api-closure), then selected IR/codecs, bounded synchronous execution, both standard SDK surfaces and companion operations | Independent directional oracles, complete typed answers, both SDK-family loops and bounded Provider evidence pass; no single-vendor or design-only closure |
| S4-2a Speech | Complete the selected static binary contract, then independently deliver progressive binary/SSE where supported. Preserve text, instructions, voice, speed, format and reported accounting | Static and streamed evidence remain separate; bounded buffering, cancellation and honest completion hold. No fabricated transcript/usage or implicit voice-resource creation |
| S4-2b Transcription | Complete file-based recognition with native multipart upstream preparation, static decoding and selected stream intake | Real reported text/timing/speaker/probability/usage facts survive; unsupported output formats reject instead of inventing timestamps. Large input and cancellation stay bounded; a reusable File object is not required |
| S4-2c Translation | Reuse the necessary audio-input infrastructure from S4-2b, but select and implement native translation as a distinct operation | Current model/language restrictions are rechecked; translation intent and results are preserved. No implicit ASR-plus-Generation substitute |
| S4-3 Images | Proposed Images-internal sequence: Generate/defaults/result-delivery closure; direct-input Edit; selected single-output preview/final streaming. Keep source order, mask association, controls and artifact identity | Review operation/source owners, Grok public delivery/defaults, mask/input bounds and stream association/reports first. Later delivery needs independent generate/edit oracles and task-specific lifecycle checks; previews do not satisfy final counts. Only file-reference forms wait for S4-4c |
| S4-4a Files | After selecting storage authority, public-ID authorization, private mappings and retention/reference policy, implement create/read/list/delete and content access | Access isolation, bounded streaming/spooling, deletion/expiry with active references and restart-consistent mappings are verified. No credential-owned upstream ID is accepted as public authority |
| S4-4b Multipart Uploads | Build on the selected file identity/lifecycle contract: create upload, add bounded parts, complete ordered parts, cancel and clean up incomplete uploads | Part identity/order, total bytes, expiry/cancel cleanup and uncertain remote completion are handled without blind replay or whole-file memory retention |
| S4-4c File-reference integration | Connect authorized resource resolution to each selected task outside pure codecs; retain the task's existing direct-input path | Resource scope can restrict trusted targets but cannot supply origins/credentials. Selected file-backed operations work across restart and reject stale, unauthorized or incompatible references |
| S4-5 Video | Select concrete current input/output or production operations before choosing task/content and optional native-job owners. Implement their codecs, execution and artifact access | Independent request/result and applicable event/job evidence covers bounds, cancellation, artifact identity and honest completion. Async APIs explicitly handle submission uncertainty/status/cancellation; only file-reference paths depend on S4-4c |

Files design can proceed alongside the first operation slices; implementation need not delay direct-input media. Decision, Embedding, Audio, Images, Files/Uploads and Video may advance independently once their own contracts are selected, but shared-owner changes must stay coordinated. Native video/media job lifecycle belongs to that operation, not the general Batch API.

Optional Chat-carried audio is a separate slice after its task contract stabilizes: preserve the existing static path, then verify genuine upstream streaming if selected. It neither blocks the main audio path nor establishes Realtime. For every operation, update the OpenAPI/usage surface only when implemented, keep admission and actual Provider evidence separate, and use the [stage-specific verification](api-expansion-design.md#verification).

#### Decision API closure

**Stage-4 closure work, placed here on 2026-10-25; the dual-SDK direction was selected on 2026-10-23.** Plan stable standard TypeSafe and OpenAI SDK access through one Decision IR and upstream codecs/dialects. The [accepted direction](architecture.md#decision-api-direction), [proposed contracts](api-expansion-design.md#decisions) and [dated evidence](refs/2026-10-23-decision-contracts.md) have separate owners. Research starts with TypeSafe, OpenAI, Perplexity and Cloudflare; no generic mainstream-Provider compatibility claim follows.

The user subsequently selected Generation-style capability-union ownership and consideration of named approximations/simple estimates before rejection. The [IR draft](api-expansion-design.md#decision-semantics-and-compatibility) now includes media/Box, service intent and source/delivery separation; its concrete fields and [projection matrix](api-expansion-design.md#named-decision-projections) remain under review. Union coverage is not first-slice activation or approval of each lossy rule.

Before dependent implementation, complete this bounded review:

1. Pin the SDK/schema baselines and resolve consequential source discrepancies. Include companion discovery, error handling, URL joining and request compression, not only inference JSON.
2. Review the full evidenced union's evidence/question/result and service owners, native preservation domains and directional cross-family rules. Show minimal counterexamples for names/order, structured rubrics, role evidence, Boolean choice values, confidence, refusal, media references and Box framing. Distinguish exact carriage, derivation, behavioral approximation, simple estimation and rejection; select concrete rules separately from the design principle.
3. Select a finite synchronous execution profile, bounds and target admission. Decide how to prevent or explicitly reject unsupported context/truncation behavior; do not inherit Generation fallback or silently split questions.
4. Obtain human selection of the remaining contract choices. Record accepted rules in Architecture and schedule a coherent implementation slice only then. Both SDK families are required; a working single-vendor path is not closure of the interoperability goal.

After the design gate, implement the selected Decision IR, codecs, bounded execution, standard-SDK companion operations and delivery as coherent slices. Closure requires both SDK families and applicable Provider acceptance; a design-only review or single-vendor implementation is not closure.

Acceptance for each authorized implementation must separate:

- **Offline contracts:** independent wire→IR and IR→wire expectations for each selected baseline/dialect and projection; exact question/answer correspondence, probabilities, refusal, strict JSON, bounds, cancellation and EOF/publication checks.
- **Standard SDK loops:** pinned official clients with configured base URL/key/model, without patches or custom response types; actual typed answer completeness, discovery, success/error behavior and selected defaults including compression. Exercise accepted cross-Provider paths for both families. SDKs that skip unknown answers must not make incomplete delivery pass.
- **Provider evidence:** fixed target/count/output bounds/timeout, serial calls through the project loader, no hidden retries and redacted reporting. Record model identity, grouping/usage and context behavior separately from protocol shape. Statistical quality/calibration and threshold portability require their own evidence, not a successful request.

This review does not activate endpoints, promise every modality in either SDK, reopen deferred routing/counting, or authorize comprehensive coverage of unresolved contracts. It blocks only dependent Decision work, not another stage-4 API family; equal priority does not preapprove unresolved contracts.

#### Stage-4 operation readiness

**Recommendation from the 2026-10-23 source/code review: close the static Generate public contract before widening Images.** This is an Images-internal recommendation, not priority over another stage-4 API family; it selects no Image dialect, IR structure or binding. Source detail belongs to the [dated recheck](refs/2026-10-22-embedding-image-contracts.md#static-generate-source-recheck-2026-10-23); the [review proposal](api-expansion-design.md#static-generate-review-proposal) owns the alternatives and minimal contract.

| API family / slice | Actual readiness | Next step within that family |
|---|---|---|
| Decision | Dual-SDK direction and capability-union principle selected; concrete contracts and projections remain under review | Complete its own design, implementation and acceptance gates without blocking other API families |
| Existing text Embedding | Selected numeric delivery, native Google/source reports and scoped OpenAI-model tokens passed offline, SDK and bounded Provider acceptance | Preserve the completed contract; new modalities, contextual output and native Google ingress need separate selection |
| Static Images Generate | Recent official/pinned-SDK notes, existing bounded task/executor and pure Grok codec. Missing creation time, PNG-default mismatch and native reports are concrete delivery blockers | Within Images, choose one public contract and effective-default/result rules, without waiting for Edit or Files |
| Direct-input Edit and image streaming | Substantial source research, but new source/mask ownership, finite input budgets and preview/final semantics remain unselected | Follow static closure under separate decisions; do not make the whole image family a prerequisite for Generate |
| Audio: Speech, Transcription and Translation | Existing static paths, but Audio has no equally detailed pinned operation/model review. Progressive delivery, complete alignment and native multipart are not established by those paths | Select one bounded operation/profile with its own source evidence; do not promise all Audio formats or stream modes at once |
| Files and Uploads | Safety requirements are written; storage authority, public-ID authorization, durable private mappings and retention remain unselected | Early authority review is useful, but complete Files implementation is not a dependency of direct-input media |
| Video | Dated Grok/MiniMax protocol research exists; public job contract and persistence are unselected | Select the operation and job contract before implementation; a well-documented upstream API is not a ready gateway contract |

For the Images static Generate slice, the review deliverable is deliberately small:

1. Choose standard-only/disconnected Grok versus an explicit native-output profile or separate native surface. Resolve default encoding, optional reports and client selection together; no fabricated time or implicit model-based schema relaxation.
2. State Generate-only owners, effective target defaults, pre-dispatch rejection and post-EOF validation. Keep broader ratio/tier exposure, Edit sources, previews and Files out of this first contract unless separately selected.
3. After human selection, record accepted decisions in Architecture and implement the coherent static slice. Preserve standard delivery regressions and independent native/delivery oracles; OAuth integration and account entitlement remain separate gates.

Exit from this review means a selected contract with finite bounds and explicit client/target profiles, not a successful native codec test. No new live call, binding activation or full image-API coverage follows from this recommendation.

#### Embedding integration targets

The 2026-10-23 user instruction explicitly supersedes the Embedding-only pause and authorizes implementation of Google/OpenAI Embedding with recorded compatible delivery. The broader [Google API pause](#google-api-pause) is unchanged.

Executable profiles and limits live in [the catalog](../src/topology/catalog/embeddings.rs); accepted owners and conversion policies live in [Architecture](architecture.md#independent-embedding-path). The selected public surface remains `POST /v1/embeddings`: DashScope text, OpenAI's `text-embedding-3-small` through OpenRouter with trusted scoped tokens, and Google's native synchronous `gemini-embedding-2` text path. Both client vector encodings are available without claiming identical native wire contracts.

The native Google gate established nested dimensions and explicit no-truncation before connection. Its 100-input ceiling matches the separately observed native 100-success/101-limit-error boundary; the 60000-value budget limits default-width requests to 19. Dated protocol and Provider evidence belongs in the [existing source note](refs/2026-10-22-embedding-image-contracts.md#embeddings), not a retained execution history here.

No image/audio/video embedding input, task-prefix injection, new tokenizer, tensor/quantized/contextualized result, async Batch or cross-space fallback is selected. Future proposals remain in [API expansion design](api-expansion-design.md#embeddings); they require separate semantic selection and per-target evidence.

#### Grok Imagine adaptation

**The 2026-10-22 scope revision implemented a static image codec without video integration.** Video now follows the equal-priority stage-4 selection rule; its concrete implementation is still unselected. The pure `grok-imagine-image-2.0` projection/response codec exists; no Grok image gateway binding or live generation acceptance is established. Do not add the retiring `grok-imagine-image-quality`. The user superseded the synchronous-video choice: later video integration should use Grok's native asynchronous API as its primary design reference, not a synchronous wrapper or assumed OpenAI video compatibility. See the [accepted decisions](architecture.md#scope-and-activation-decisions) and [dated sources](refs/2026-10-22-grok-imagine.md).

Remaining image gates:

1. **Public contract and broader controls.** The codec maps prompt/count, user and `low`/`medium`/`auto` quality with static Base64 results. Exact pixel sizes, encoding guarantees, background/compression/moderation controls and streaming reject. The [Image design](api-expansion-design.md#image-results-and-delivery-compatibility) now reviews typed native ratio/tier controls, missing `created`, serving-model/accounting carriers and the OpenAI-default-PNG mismatch. No delivery option is approved yet; a native codec is not complete OpenAI Images compatibility.
2. **OAuth and binding.** Add an explicit model binding and remove the image path's API-key-only assumption at the responsible layers without weakening Provider/profile checks. Use the existing single-account Grok OAuth pool, with no inference-triggered login/refresh or fallback.
3. **Acceptance.** Preserve independent request/response codec expectations and strict JSON, bounds, accounting and terminal-honesty checks. Before any live run fix target, count, output cap and timeout; call serially through the project loader with no hidden retries and redacted reporting. Model enumeration does not establish OAuth image entitlement.

The implemented Grok slice still excludes editing/deferred jobs/preview streams, Files references, caller-directed upload destinations, cross-Provider failover and automatic media sweeps. Broader Generate/Edit/streaming design belongs to S4-3 without cross-family precedence; that design does not widen the implemented Grok profile. Unselected operation implementations stay maintenance-only until their own slices are selected.

**Video selection gates:** retain `grok-imagine-video-1.5` and `grok-imagine-video-1.5-lite` as targets, with lite as the future default test model. Do not add endpoints, bindings, codecs, polling executors or synchronous facades now. Before implementation select native-style submission/status/artifact contracts, public task identity, authorization, persistence/expiry, bounded polling, cancellation limitations and uncertain-submission handling. A later minimal acceptance proposal is one text-only 1-second 480p lite generation; it is not a scheduled call or proof of the full model's account access.

### Stage 5 backlog

**Status: unscheduled.** This is a future-work bucket, not a promised implementation queue after stage 4. Each task needs a selected design and explicit scheduling before work begins.

Reopening a task requires its client scenario, minimum semantic/resource dependencies, design choice and bounded acceptance contract. Do not prebuild these systems as stage-3/4 infrastructure. Native cache-only prewarming and other separately deferred features retain their own scheduling decisions; they are not silently added to this queue.

- Citations/source attribution, `tool_search`/ordered discovery and hosted tools retain their separate [design proposals](api-expansion-design.md#unscheduled-designs).
- **Server-side compaction:** include the explicit Provider-executed `/responses/compact` operation. Receiving/replaying compaction items remains existing Generation scope. Automatic compaction triggers, mutation of stored conversations and background execution are not implied by the explicit operation.
- **Batch:** select supported typed tasks, file dependencies, submission reconciliation, durable job identity, cancellation and per-record outcomes.
- **Realtime:** select client scenarios, session/transport ownership, event/media contracts, interruption, recovery and resource bounds. Stage-4 audio/video adaptation does not implement Realtime.

## Test supplementation

**Integrated into the accepted breaking redesign on 2026-10-23.** Use current stable release assets to verify standard text, tool/history and SSE contracts during each implementation slice. Do not make the pre-reset implementation the expected-output oracle or first repair all old mappings. Extend the same sources/adapters to later selected operations; no separate hand-maintained protocol dataset or benchmark framework is selected.

Use the [source lock](../tests/upstream/sources.lock.json), [asset lock](../tests/upstream/assets.lock.json), [source directory](references.md#test-source-repositories) and [Development rules](development.md#upstream-test-sources). Official contracts supply normative semantics; SDK types/fixtures and Vercel consumers provide scoped evidence, not unconditional validity or repair-policy authority. Initial acquisition and thin adapters are implemented; WPT acquisition and broader fixture integration remain candidates for the relevant slices.

### Current text and SSE gate

**Scoped closure (2026-10-24), retained by final Generation Text review.** The [executable baseline](development.md#text-and-sse-integration-baseline) owns source/asset provenance, official SDK consumption, complex tool/history loops, framing mutations and transport lifecycle evidence. Preserve the full offline baseline and applicable connected SDK loops when changing those owners. Latest-source candidates remain visible but [unpromoted](refs/2026-10-23-stable-protocol-contracts.md#latest-candidate-review-2026-10-24); the locked baseline is not advertised as the newest release.

This finite gate covers selected Chat, Responses and Messages contracts, upstream dialects and existing Google text directions. It is not comprehensive official conformance, all upstream tests, raw Provider fidelity, Google ingress or a new modality activation. Importer failures, silently skipped selected cases and defects in claimed support block closure; explicit exclusions are not passing cases. Diagnostic-extension organization and dedicated checks remain deferred under the [scheduling decision](#standards-reset-and-redesign).

### Stage-4 reuse and acceptance

Use the same pinned sources, update mechanism and thin adapters rather than another multimodal harness. Apply these conditions per selected operation, after its design approval:

1. Identify the applicable official SDK/Vercel cases, source revisions and local entry points for that operation and target. Inspect operation-specific assets; text tests and a whole-repository checkout do not establish image, audio, file or video coverage.
2. Run selected cases through MorphieCore request preparation, codecs and delivery, with independent wire-to-IR/IR-to-wire expectations and actual SDK consumption where applicable. Exact preservation, approved adaptation and intended rejection need explicit assertions. Where no suitable upstream case exists, record the gap and retain or add a focused, source-backed local oracle; missing external coverage is not a passing skip.
3. Keep the current text/SSE baseline green and add the operation's resource, lifecycle and safety checks. SSE reuse applies only to SSE paths; static, binary, multipart and asynchronous-job contracts need their own evidence. External tests supplement, not replace, the offline baseline and applicable SDK/real acceptance gates.

For a selected static Images Generate slice, select image-model/API fixtures for controls, effective defaults, result encoding/counts and reported-field presence. Edit/mask/multipart and preview/final streams follow their own selected slices; do not treat static Generate or conversational image inputs as their acceptance. Apply the same per-operation rule to Decision, Embedding extensions, Audio, Files/Uploads and Video without implicitly activating them.

BFCL, tau-bench, ComplexFuncBench and ToolSandbox are not dependencies or gates for this plan. Automatic tool execution, malformed-call repair, new Provider activation and unresolved protocol semantics remain outside scope. Preserve the existing safety assertions; upstream source evolution does not authorize relaxing them.

## Phase rules

### Generation design review gate

**Current status: the standards-reset redesign was fully accepted on 2026-10-23.** Its ordering and scope are in [the integrated sequence](#standards-reset-and-redesign). No second blanket approval of those principles is required. Concrete unresolved carriers or new semantics still need source-backed selection; approval is not implementation or conformance evidence.

The earlier stage-1/2 approval at `26b63eb` on 2026-10-20 and the table below are historical scope evidence. They do not make old SDK releases, public manifests, internal-only receipts or mapping expectations authoritative for the redesign.

The six areas below summarize the approved stage-1/2 baseline, not a new specification or a claim of exhaustive coverage.

| Review area | Approved baseline | Contract and evidence entry points |
|---|---|---|
| Semantic ownership and bounds | Ordered content/resources, dependencies, requested controls and reported facts have separate typed owners. Construction and edits revalidate bounds and references; runtime selection and credentials do not become task state | [Semantic IR](architecture.md#semantic-ir), [request validation](../src/semantic/task/generation/request/mod.rs), [edit assertions](../tests/semantic/history/context_transforms.rs) |
| Source parsing and target projection | Validate the source before target-private projection. Exact carriage, named adaptation and rejection remain distinct; source IR is unchanged. Reported text absent/null/object states are preserved without admitting request null | [Adaptation](architecture.md#adaptation), [text presence](architecture.md#reported-text-configuration), [directional text oracles](../tests/semantic/text/text_profile.rs), [native request oracle](../tests/semantic/providers/dashscope_controls.rs) |
| Tools and schemas | Keep explicit strictness, non-strict defaults and Responses normalization defaults distinct. Preserve schema constraints and call/result identity; reject unsupported target subsets rather than weakening them | [Tool owners](../src/semantic/task/generation/tools/mod.rs), [strictness/schema assertions](../tests/semantic/tool_calls/tools.rs), [Messages profile](../tests/semantic/anthropic/client_fidelity.rs) |
| Streaming, terminal and reported evidence | Preserve bounded phase reports and selected transitions. Validate native facts before delivery loss; withhold success until validated EOF and final handoff. Publication forbids replay; failures/cancellation do not fabricate success | [Report contract](architecture.md#lifecycle-settings-and-service-reports), [delivery](architecture.md#streaming-and-delivery), [lifecycle oracles](../tests/semantic/text/lifecycle_reports.rs), [publication assertions](../tests/gateway/tool_publication.rs) |
| History and replay | Stored retrieval retains the delivered terminal projection, not an opening archive. TTL storage, affinity and replay compatibility are separate; owner edits and deployment isolation still govern replay | [Replay](architecture.md#reasoning-replay), [storage](architecture.md#stateful-history), [stored/stateless parity](../tests/gateway/responses_history.rs), [carrier assertions](../tests/semantic/reasoning_replay/replay_carrier.rs) |
| Public and integration boundary | Capabilities remain partial gateway contracts, not codec unions or model/full-loop guarantees. The notice means possible adaptation. Acceptance is confined to the selected client/binding profiles; later activation needs its own evidence | [Capability contract](architecture.md#public-capability-and-adaptation-notice), [public assertions](../src/gateway/tests/models.rs), [selected loops](#stage-2-work-plan), [integration handoff](#integration-handoff) |

Approval accepts this scoped baseline, not all official Generation semantics. Preserve the [inventory](refs/2026-10-19-same-protocol-inventory.md): named losses are not losslessness, rejection is not support, and ignored/unassessed cases are not passing evidence. In particular, Messages deferred controls/reports, hosted tools, broader citations and missing Google directions remain outside the selected exactness claims.

On 2026-10-20 the user additionally approved the [Interactions call-signature profile](architecture.md#reasoning-replay): existing call replay, a distinct call-only format, native input signature and current manifest/dependency rules. Approval covers focused directional/ownership/bounds/snapshot checks and Router/SDK return-loop coverage, followed by separately bounded native acceptance. It does not admit other signature owners or resolve report designs.

On 2026-10-21 the user selected the local input, cache, report and citation decisions linked from [production closure](#generation-text-production-closure). Those decisions satisfy the design-selection gate for their declared scope without reopening the earlier baseline. They do not resolve source-version/unit questions, approve unrelated owners, execute the queued work or certify any new full loop.

The reported text-container gap has a [source-backed local repair](refs/2026-10-17-generation-contracts.md#responses-text-container-presence-2026-10-20), verified by the full offline baseline, pinned SDK loopback and Python checks. No installed-client or real Provider run was repeated for this repair. Earlier real-client/Provider results remain scoped to their recorded runs; this review does not establish R1b installed-client opaque replay, cross-account compatibility or real failover. Stage-3 snapshot/counting choices and later operation designs stay in their own slices. [Observability follow-up](#observability-follow-up) is not an additional approval gate.

The sourced [contract boundaries](refs/2026-10-17-generation-contracts.md) retain deferred counterexamples. Test counts, round trips, agent audits and passing coverage runs establish neither semantic completeness, design approval nor integration; a scoped design approval is not a defect-free implementation claim.

### Observability follow-up

**Selected Generation scope is implemented:** unified domain evidence/typed failures on 2026-10-24 and scoped local trace/log wiring on 2026-10-25. [Architecture](architecture.md#observability) owns the interfaces, disclosure and lifecycle contracts; [Development](development.md#telemetry-ecosystem-checks) owns ecosystem, report and local-file checks. There is no pending runtime report migration or separate telemetry acceptance matrix.

Remaining work is deliberately unactivated. Public diagnostic field organization, JSON/SSE/retrieval parity, SDK retention and display await final production preparation. A future trusted Collector requires selected destinations/configuration isolation, visible export health and a network shutdown barrier; storage migration and [Agent/cluster authorization](#agent-authorization-and-diagnostics-follow-up) retain separate scope. No Collector deployment, content export, global registration, metrics reader or relaxed private-file guarantee is authorized.

Preserve existing authoritative-report, body-free privacy, bounded-file/queue, cancellation and standard Agent-loop checks. Broader instrumentation is driven by concrete diagnostic needs, not an exhaustive event catalogue.

### Agent authorization and diagnostics follow-up

**API-key admission and minimal diagnostic attribution are implemented under the 2026-10-25 selection.** Preserve the [identity/observation contract](architecture.md#agent-identity-and-cluster-diagnostics), [versioned private configuration](usage.md#agent-api-keys) and bounded [verification baseline](development.md#verification-gates). All admitted Agents retain the existing shared access policy. [Exchange capture](architecture.md#configurable-exchange-capture) owns sensitive diagnostics. The current downstream stage now includes Agent session management; this is cross-cutting integration work, not a sixth API stage or reopening of Generation.

Next reviews and separately deferred branches:

1. **Agent session management — current-stage design.** Follow the [selected direction](architecture.md#agent-session-files): per-Agent directories of JSONL sessions when storage is enabled, with explicit reading/copying for cross-Agent work. Select record/session identity, storage opt-in, creator/caller provenance, read-versus-copy semantics, bounds and private-file/publication safety before implementation. No concrete migration plan, disk backend, new endpoint or change to current Responses TTL/shared access is selected. Preserve native replay restrictions; a public transcript is not necessarily resumable typed history.
2. **Enhanced model discovery — next scoped review.** Continue the [reviewed draft](#models-discovery-interfaces) separately after minimal attribution. Session design belongs to this stage, but complete persistence/migration is not a discovery prerequisite. Select one effective snapshot/client integration without activating new models or operations.
3. **Usage rollups and cluster diagnostics — unscheduled.** Build only on the observed per-attempt source snapshots; verify cumulative replacement, missing/estimated values, failed/cancelled attempts and visible recording loss. Collector/storage selection must preserve redaction, bounded streaming and honest completeness. No statistics API, metrics reader or durable billing ledger is activated.
4. **OAuth and managed binding — deferred optional branch.** API-key attribution, diagnostics, sessions and model discovery do not wait for it. On resumption, prove a refresh-stable authorization-instance binding, independent revocation, trusted audience/permissions and actual client integration before selecting a server. Registry management/hot reload remain separate.
5. **Cluster activation — unscheduled.** Select HTTPS/trusted-proxy topology, explicit collector destinations, retention/capacity and revocation/in-flight behavior. Run bounded two-Agent authentication → inference → explicit handoff → revocation acceptance under the chosen credential lifecycle; OAuth refresh applies only if that branch is selected. Deployment still requires explicit authorization.

Do not prebuild multi-user IAM, tenant ACLs, automatic sub-Agent delegation, financial billing or strict quota settlement. Authorization-server choice and the remaining [integration decisions](api-expansion-design.md#centralized-agent-authorization) are not resolved by this planning baseline.

### Temporary routing freeze

The [stable routing baseline](architecture.md#failover) is the selected Generation contract. Codec work must not change membership, stable selection, health scopes, affinity, budgets, credentials or public model routing. Do not rank targets by projection loss or add a fallback to hide an unsupported conversion. The selected [model configuration](usage.md#model-configuration) is a narrow exception for explicitly configured existing bindings and their deployment/policy parameters. The separate Agent-key table changes ingress identity, not routing policy or Provider-bound replay authority.

IR/codec contracts take priority over keeping the full client-to-gateway-to-Provider chain connected. When existing admission or topology checks reject a new codec contract, record the exact boundary and leave integration disconnected rather than weakening the IR or changing routing. Work may change semantic requirements, representation checks, target-local projection policy, codecs, delivery representation and diagnostics; trusted target policy data may evolve without activating endpoints. Stop at any interface that needs a routing decision.

### IR and codec migration

Preserve the [selected request policies](architecture.md#selected-request-approximations), [thinking-budget contract](architecture.md#thinking-budget-quantization), [reported-fact policy](architecture.md#adaptation-evidence) and [capture boundary](architecture.md#configurable-exchange-capture). Track semantic expression, codec representation and enabled bindings separately. Apply target-local conversions once on target copies, keep source IR unchanged and reject unselected combinations and loss of protected task, execution or security meaning. Client-compatibility losses follow the [compatibility-before-rejection decision](architecture.md#settled-decisions); client compatibility never defines the semantic vocabulary.

Request acceptance, upstream-reported settings and delivered history remain separate. Repeated reports of a bound envelope fact must agree before normalization or omission; ordered safety assessments remain separate observations. JSON and SSE use the same delivery policy; necessary continuation and EOF-gated terminals remain authoritative. Diagnostics expose body-free typed receipts; optional capture never selects a semantic mapping.

Coverage targets concrete failure modes found through measured uncovered branches, not a percentage: exact JSON and byte/node/depth limits, replay ownership and history edits, sparse usage and scope separation, stream fragmentation, cancellation and EOF-gated termination, and declared projection policies with independent directional expectations. Use bounded property tests only where they explore real state or fragmentation boundaries.

Defer success scenarios whose completion conversion or binding admission is not selected; remove them rather than invent a conversion or expect a gateway error. Keep identity, replay isolation, strict parsing, cancellation, transport bounds and honest-terminal assertions. Rebuild selected integration coverage during reconnection.

### Verification and integration boundary

For every implementation slice:

1. Keep independently written wire-to-IR and IR-to-wire oracles; a round trip is supplementary.
2. Run focused offline checks, then the [full offline baseline](development.md#offline-baseline). Report measured coverage separately from SDK, client and Provider evidence.
3. Separate codec regressions from an incompatible frozen integration contract. A failed baseline remains a failure; removing unselected success scenarios never removes safety assertions or whole suites, and removed coverage is reported.
4. Commit and push only slices that pass the baseline, unless the user authorizes a specific exception.
5. Run the synthetic SDK loopback gate where the codec boundary is connected; otherwise mark it blocked, not passed.

| Gate | Required evidence | Effect |
|---|---|---|
| Advance independent development | Focused checks pass; baseline failures are classified and new regressions fixed | Continue without changing frozen routing |
| Commit and push | Full offline baseline passes, or an explicit user exception | A local completion claim does not grant publication |
| Accept integration | Boundary review, admitted route contracts, full offline baseline and connected SDK gate pass | Only then claim a selected full client loop works; real Provider acceptance stays separate |

## Integration handoff

Native `representation_contract()` carrier availability is separate from default `semantic_contract()` binding admission. Completing a codec activates nothing: [topology compilation](../src/topology/compile.rs) still checks the selected admission contracts, and widening a synthetic endpoint for a test does not establish a public route. The [all-native Go/Haiku contract](architecture.md#owner-local-cache-admission) is an explicitly selected configured exception; generic defaults and unrelated frozen features remain unchanged.

| Boundary | Current treatment | Exit condition when reopened |
|---|---|---|
| Binding admission narrower than codec representation | Keep the narrower public/topology contract | Verify model-effective support, capability publication and preflight/preparation agreement per binding |
| No exact target carrier for protected meaning | Reject, including normalization-default tools on plain native tools and unsupported terminal causes | Only a separately selected carrier or semantic policy resolves it; wider admission cannot |
| Missing evidence for a target-local approximation | Reject uncalibrated media detail/context conversions | Supply target evidence and the approved mapping without changing route order |
| Deferred protocol direction or feature | Keep Google static/client intake and unselected hosted tools out of scope | Reopen through an explicit slice |

Codec carriers whose binding admission stays closed:

- Google structured output and logprobs remain closed. Messages automatic TTL, owner-local markers, strict tools, ordered instructions and tool changes have the narrow [all-native configured admission](architecture.md#owner-local-cache-admission) on Go/Haiku; mixed routes and the generic profile remain closed for those features. Exact beta carriers for per-message effort, block binding and tool changes are separately enabled by trusted binding policy. Missing carriers use the selected named projections where applicable; explicit block-binding `"error"` still rejects without a carrier.
- generateContent [cached-context references](architecture.md#cached-context-references) with exact identity/expiry and explicit clock checks. Other carriers reject; there is no HTTP field, authorization service or cache lifecycle operation.
- [Safety policies and reports](architecture.md#safety-policy-and-reports) on generateContent. Downstream delivery omits assessment-only reports under a named rule and rejects blocking reports and safety-bearing settings echoes. Requests without a policy may still receive reports; those exchanges are not an established full loop. Vertex severity/method carriers are out of scope.
- [Service policies](architecture.md#service-policies-and-reports). The native generation-only decode helper rejects an execution policy rather than dropping it. Messages service reports need full-message or opening usage; cross-contract classes and late reports without a carrier reject. Absent reports are never supplied from request controls.
- Chat custom tools have static JSON and history carriers but no pinned custom-call SSE delta carrier; custom Chat streams reject.

Explicit constrained-carrier rejections: Google JSON-object mode, schema references and unselected keywords; generateContent optional allowed-tool subsets and implicit object roots for function parameters; Interactions logprobs; Messages strict string enum/const, references and regex; explicit cache retention without an exact carrier or the approved `"24h"` target policy. No registered target selects that policy yet. Google probability intake accepts only unambiguous chunk-local text reports and has no token bytes, so the Responses probability carrier cannot deliver it.

P8 policy selection is complete. Responses normalization-default strictness never becomes a plain native tool, so pi-shaped requests omitting `strict` do not reach those bindings; explicit non-strict tools keep their carrier. Native codecs reject uncalibrated media detail/context and filenames without carriers. No cross-scope or numeric conversion activates without model-level evidence.

Terminal causes remain separate from turn progress and tool continuation. Chat and Responses use the [selected terminal approximations](architecture.md#selected-request-approximations); Google `STOP` remains a source fact and Chat delivers `stop`. Messages retains its strict rejection rules, with only the selected [Google delivery profile](architecture.md#google-messages-terminal-projection) applied to eligible copies. Selected answer/history loops have [stage-3 acceptance](#stage-3-work-plan); codec probes and synthetic smokes alone establish neither full-request admission nor real acceptance. The [Messages gateway smoke](../tests/gateway/anthropic.rs) remains synthetic JSON/SSE evidence for owner-local markers and signed tool-result/final-answer history, not a real-client or Provider gate.

Exchange capture covers authenticated client boundaries and Generation upstream I/O, not frozen standalone tasks. Unsafe units are explicitly incomplete or omitted, never raw fallbacks; capture counters do not establish semantic completion or durable retention.

### P7 boundary review

**Status: the initial boundary and sequence were selected on 2026-10-19 for R1a/R1b only.** That first-stage scope choice was separate from the later [overall design approval](#generation-design-review-gate), full-loop acceptance and future admission decisions.

Entry condition: the first-stage IR/codec scope passes its local gates, and incompatible carriers and frozen admission boundaries are explicit. A connected end-to-end gateway is not a prerequisite.

The review must:

1. Separate exact capability, approved lossy acceptance and explicit rejection in the proposed public contract.
2. Check preflight/preparation agreement and identify the model-level evidence needed before activating existing codec support.
3. Verify the stable routing contract alongside each candidate's binding evidence; cache or recovery success does not establish representability.
4. Specify the selected client-loop contracts and the offline, SDK loopback and separately scoped real-acceptance gates.

Exit condition: a selected boundary design and an explicit implementation and reconnection schedule. The review changes no routes and promises no conversion.

#### First reconnection slices

The 2026-10-18 review checked actual catalog targets, pinned client builders, official protocol/model sources and focused offline request probes. Preflight and preparation share encoding; selected first requests agreeing at those boundaries do not establish response delivery or a client loop. The [evidence and counterexamples](refs/2026-10-17-generation-contracts.md#p7-binding-evidence-2026-10-18) remain distinct from live acceptance.

The selected Google basic reconnection is complete within the [stage-3 evidence](#stage-3-work-plan). Further Google expansion follows the [pause](#google-api-pause), not an implicit next reconnection package.

Selected loop order: R1a, R2 and R1b completed under separate user selections and their [bounded evidence](#stage-2-work-plan). This closes the selected R1 loop scope, not a Provider/model matrix; [overall design approval](#generation-design-review-gate) is a separate decision. Google → Chat/Responses conversion belongs to stage 3. Use one selected binding at a time without changing catalog ordering or activating alternatives implicitly. Do not use DeepSeek Responses as the initial faithful baseline: its documented role, reasoning and parallel-tool behavior needs separate directional policies. Coding Plan's output-cap semantics also need their own evidence; the DashScope fix is not a universal dialect change.

Remaining policy and implementation work is separate from the accepted R1 scope:

- **Messages target policy:** caller-selected request-wide automatic caching now follows its own production-closure slice; sampling-policy placement remains separate from owner-local and pi-profile admission. [Trusted missing-cap defaults](architecture.md#missing-output-ceilings) are implemented; do not infer automatic-cache or cached-context admission from this policy.
- **Model-effective value evidence:** the implemented [binding guards](architecture.md#model-effective-values) reject Gemini `minimal` and cap `65537` without changing generic carriers or selected mappings. Additional model declarations need their own evidence; a passing local rejection check is not Google full-loop or live enforcement acceptance.

After selecting a slice, require an independent static and streamed tool turn, a final answer and next-request replay, plus strict EOF, cancellation and no-post-publication-replay checks. Then run the full offline baseline and the connected SDK gate before separately bounded real-client/Provider acceptance. Only afterwards add controlled synthetic failover/affinity/cooldown scenarios under the unchanged routing contract. No completion or safety rule is weakened to make a loop pass.

#### Public capability and reconnection sequence

The implemented [actual-report contract](architecture.md#public-capability-and-adaptation-notice) supersedes internal-only receipts while retaining the conservative possible-adaptation header. Capability declarations still do not establish model-effective support or full-loop availability. An acceptance boolean cannot distinguish exact support from omission, and streamed usage may introduce losses after headers.

1. Establish model-level evidence and preflight/preparation agreement for each selected existing binding; reject uncalibrated values and stop at interfaces needing routing-policy changes.
2. Extend the capability view only with independently verified declarations. Do not activate bindings implicitly or treat an unlisted case as rejection.
3. Reconnect exact-carrier client loops first, then approved adapted paths. Verify tool calls, results, final answers and replay for JSON and SSE. Blocking safety reports and unsupported completion carriers remain rejected.
   Verify the redesigned legitimate phase/replay carriers through selected release-pinned consumers, including required next-request retention. Safely ignorable diagnostics need no client retention or display; their dedicated checks follow the [extension deferral](#standards-reset-and-redesign). Before enabling extended-retention omission, review target storage policy without treating cache presence as retention evidence.
4. Require the full offline baseline and connected SDK gate per slice, then schedule bounded pi/opencode and Provider acceptance.

### Controlled routing acceptance

**The selected controlled scope passed on 2026-10-20; real Provider failover remains unaccepted.** Existing [Router/SDK recovery](development.md#sdk-loopback-gate) and [installed-client recovery](development.md#installed-client-synthetic-recovery) exercise unpublished failure, compatible synthetic credential migration, primary recovery, sticky continuation and all-cooling admission. Native session/signed-history preservation and no replay after publication remain required. Routing policy and binding admission are unchanged.

These checks do not establish real account-compatible replay, Provider cache behavior, concurrent network recovery or arbitrary bindings. Initial production support remains single-candidate/one-attempt. A real-failover claim needs a separately bounded compatible credential/Provider pair and reproducible advanceable failure; never strip signatures to make it pass. No routing expansion, credential lifecycle change or deployment follows from controlled acceptance.

## Pending decisions

The [settled decisions](architecture.md#settled-decisions) remain the baseline; the audit permits evidence-backed reconsideration of lossy mappings. New semantic or execution-policy structures need a selected design. Continue independent work while a choice blocks only its own contracts.

### Ownership and projection decisions

Minimal counterexamples and pinned sources are in [Generation contract boundaries](refs/2026-10-17-generation-contracts.md).

| Question | Recommendation and exit condition |
|---|---|
| Complete citations and source identity | **Stage 5.** Licence/quote/request-source owners and complete Messages citations remain unselected. The local [point-anchor correction](architecture.md#citation-point-anchors) is selected separately and does not resolve the [1C design](api-expansion-design.md#1c-design-messages-citations) |
| Execution controls and reports | **Unscheduled beyond the selected local repairs.** Geography, access programs and broader safety/review details still need bounded owners. Production closure selects protected budget/dispatch rejection and explanatory refusal reports, not their execution. Continuation execution stays excluded |
| Google probability report owners | **Paused; unselected.** Keep the [remaining proposals](api-expansion-design.md#google-output-report-owners-unselected) and [streaming evidence](refs/2026-10-17-generation-contracts.md#streamed-score-scope-2026-10-20). Terminal explanations are now selected separately; nonempty URL-context reports, hosted tools and complete citations remain excluded |
| Google service report mappings | **Selected concrete reports implemented.** The [source check](refs/2026-10-17-generation-contracts.md#google-text-report-source-recheck-2026-10-21) establishes the selected v1 carriers and processing classes. Unproven defaults/transitions still reject; wider service semantics and model fallback remain unselected |

### Scheduling and product decisions

| Question | Current behavior and recommendation | Blocks |
|---|---|---|
| Binding activation and reconnection (P7 follow-up) | The public capability/notice contract is selected. Complete model-level evidence and preflight/preparation checks, then specify bindings and client loops under the [reconnection sequence](#public-capability-and-reconnection-sequence). Do not reinterpret unsupported completion/safety carriers or reopen P8 policies | Binding activation and full-chain acceptance, not codec-only coverage |
| [Models discovery interfaces](#models-discovery-interfaces) | **Tentative, unscheduled.** Retain the user-reviewed enhanced schema as a design draft; further standard Models completion, enhanced discovery and client metadata integration require a separately selected slice | This interface group only; no change to current priorities or stage gates |
| [Native cache-only prewarming](#deferred-cache-prewarming) | Priority and scheduling undecided; select the operation and result contract when reopened | That slice only |
| [Tool discovery and native `web_search`](#deferred-tool-discovery-and-web-search) | Stage-5 backlog, unscheduled; designs remain undecided | Those slices only |
| [Token counting](#deferred-token-counting) | **Deferred on 2026-10-21.** The [upstream-first policy](architecture.md#token-counting-policy) remains selected; select the profile, estimate rule and provenance reporting when reopened, with no gateway tokenizer counting. The route stays absent, so no stub or estimated placeholder is published | Counting and its cross-protocol promises only; stages 1–2 and stage 4 do not wait on it |
| [API expansion](api-expansion-design.md) | Stage 3: cross-protocol stability, with counting deferred. Stage 4: Decision, Embedding, audio/image/video and Files/multipart Uploads at equal priority. Stage 5: server-side compaction, Batch and Realtime, all unscheduled | Each operation's own contract and activation |
| Files/multipart Uploads | Selected for stage 4. Choose storage authority, authorized public IDs, upload lifecycle, retention/deletion and private upstream mappings. Current inline/URL input and rejection of credential-owned references remain until implemented | File resources and their dependent operations |
| Server-side compaction | Stage-5 backlog, unscheduled. Keep the explicit `/responses/compact` operation separate from automatic triggers and existing compaction-item replay | Compaction execution, not existing replay or stages 1–4 |
| Batch | Stage-5 backlog, unscheduled. Select native job execution, durable acceptance/reconciliation and supported task/resource dependencies | Batch API and execution |
| Realtime | Stage-5 backlog, unscheduled. Choose a client scenario and official contract before deciding session ownership, transport, interruption and recovery | Realtime |

## Deferred work

### Deferred Generation semantics

Selected Messages examples/context/automatic-TTL activation and explanatory reports are implemented. Citation point anchors have a local owner, but unresolved native insertion units retain their [production exclusion](#generation-text-production-closure); these contracts are not general stage-5 reopening.

The remaining deferred gaps are explicit exclusions, not claims of complete official API coverage:

- [Native cache-only prewarming](#deferred-cache-prewarming) and [tool discovery and native `web_search`](#deferred-tool-discovery-and-web-search).
- Messages citations ([1C](api-expansion-design.md#1c-design-messages-citations)) remain unscheduled stage-5 work; carrying requests and responses reject. Separately scheduled Claude Code sampling may motivate that slice.
- Messages turn-scoped system messages (`clear_at`) have no IR owner and reject.
- Generated media beyond Chat audio need a selected output, stream and replay design. Other hosted tools remain deferred; observation types do not imply an end-to-end contract.
- Chat `store: true` rejects; there are no stored completions.
- Tokenizer-unbound `logit_bias` rejects.
- The wider budget-scope exception for manual interleaved thinking remains deferred. S1-3 supports only ordinary manual-budget relations; this does not disable adaptive thinking or existing tool loops.
- Gemini finish reasons without content semantics (`MALFORMED_FUNCTION_CALL`, `UNEXPECTED_TOOL_CALL`, `TOO_MANY_TOOL_CALLS`, `LANGUAGE`, `OTHER`) stay attempt failures under the routing freeze.
- Tool-result video has no carrier on any bound target.
- Multi-candidate execution remains deferred; file services wait for their selected stage-4 packages.

### Deferred cache prewarming

Native cache-only prewarming is unscheduled and is not a prerequisite for stages 1–4 or P7. Existing cached-context references retain their own contract. The [operation proposal](api-expansion-design.md#deferred-cache-prewarming) distinguishes native no-output prewarming from client-owned one-token generation; select its priority, operation/result owners and acceptance scope before reopening.

### Deferred tool discovery and web search

These are unscheduled stage-5 tasks. Select ordered discovery/availability and native Provider-search contracts separately; the [design proposals](api-expansion-design.md#deferred-tool-discovery-and-web-search) retain the required ordering, citations and replay boundaries. Neither authorizes a gateway tool registry, search executor or silent text/function-call substitution.

### Deferred token counting

**Deferred on 2026-10-21.** `POST /v1/messages/count_tokens` left the active queue because no gateway, pi or opencode path depends on it, so stage 3 closes without it. The [upstream-first counting policy](architecture.md#token-counting-policy) and the [operation contract](api-expansion-design.md#token-counting) remain selected; only scheduling changed. Nothing is stubbed: a counting client receives the ordinary 404 rather than a fabricated or estimated count, and no capability entry claims the operation.

Reopen by selecting one model/target profile with its upstream reliability criteria, the coarse estimation rule and its input coverage, the client-visible provenance treatment, and whether the Responses `input_tokens` companion is included. Claude Code acceptance covers token counting, so it stays behind the same decision.

When reopened, implement the independent Messages operation over that profile's effective-input projection, following the [counting contract](api-expansion-design.md#token-counting): share the rules that determine Generation's effective input, including instructions, ordered history, tool declarations/calls/results, media and input-affecting settings/defaults. Keep operation-specific validation and encoding separate; counting is not Generation with a zero output cap or a call to its full execution preparation. Prefer upstream values, run no tokenizer, create no generation task and maintain no separate message/default rules. Identify model/target, projection scope and count source without exposing trusted routing or credentials, and reject inputs outside the selected profile, including media without an applicable counting or estimation rule. Counting can still require authorized resource access and preprocessing; verify applicable byte/time bounds and cancellation without moving I/O into pure codecs or treating no generation as no resource cost.

Exit evidence: independent request/result oracles for the count request's effective input and result meaning, agreement with the selected generation projection on count-relevant input, and upstream preference plus any selected coarse fallback with its provenance reporting and input coverage.

### Models discovery interfaces

**Tentative and unscheduled; documentation only.** The user accepted retaining the [reviewed schema draft](api-expansion-design.md#models-discovery-interfaces), not starting interface completion. Existing standard Models and partial capabilities remain unchanged. The proposed enhanced endpoints, shared discovery revision and client metadata adapters are not implemented or activated by this plan. This work has no assigned stage and does not change current priorities or block another slice.

On resumption, select the standard-conformance scope first, then the enhanced schema/profile, evidence-backed aggregation, finite bounds and one versioned Agent integration. Formalize the wire schema only for the authorized slice; do not add planned paths to OpenAPI or reopen token counting, routing, Files or model activation merely to publish metadata.

Future exit evidence must separate standard SDK list/retrieve/error behavior, enhanced snapshot/unknown-value/media-form and multi-candidate counterexamples, and actual client consumption. Verify that metadata changes the selected Agent's context/output budgeting and request controls rather than merely appearing in a response. Preserve auth, no-upstream-I/O, complete bounded lists, snapshot consistency and private-topology isolation; broader client/provider acceptance requires its own bounded scope.

### Messages citations

The [1C proposal](api-expansion-design.md#1c-design-messages-citations) remains unselected and unscheduled. Citation-bearing requests/responses keep their rejection until source identity, quotes, request flags and output anchors are represented. Claude Code acceptance may motivate this slice, but is itself separately scheduled.

## Deferred acceptance

| Work | Scheduling | Exit condition when reopened |
|---|---|---|
| Non-image multimodal real checks | With the relevant stage-4 slice, except the selected direct-document context check in GT-5; earlier carrier regression remains required | One bounded real check per selected carrier, using model-issued history for tool results; no per-model or format matrix |
| Current-client image replay | With a selected carrier/client slice | Use the current installed binary and record its version; protocol/source pins remain independent references, never a reason to downgrade an Agent |
| Real failover beyond the production support list | Deferred beyond the accepted single-candidate profiles; any new failover claim needs its own bounded real evidence | One bounded run for a model with two credentials or Providers and a reproducible advanceable failure; preserve account/replay compatibility and do not manufacture success by stripping signatures |
| Codex and other agents | Separately scheduled; on hold | Define the client's acceptance contract first; no runs or request sampling before scheduling |
| Claude Code | Separately scheduled; on hold, with citation work in stage 5 | Sample real request shapes, then verify beta headers/query, token counting and user metadata; implement [1C](api-expansion-design.md#1c-design-messages-citations) if samples use citations. pi Messages acceptance does not prove Claude Code |

The coverage pass covers text, tools, tool errors, reasoning replay, usage, truncation, error classes and media carriers, plus client-facing fragment shapes and absent/null/empty distinctions. It does not authorize a variant matrix.

### Target clients

pi and opencode are the primary clients and real-client gates. Pinned client behavior is an acceptance oracle alongside the protocol schema: parsing, stream merging, returned history and error classification. See [References](references.md), [pi acceptance](development.md#pi-acceptance) and [opencode acceptance](development.md#opencode-acceptance).

### Only if needed

- SSE keep-alive comments while initial frames are held.
- Prefix-derived executor affinity when no conversation is declared.
- An opt-in first-content timeout.

## Frozen scope

Maintenance only, when a refactor requires it:

- Standalone Images, Speech, Transcription and Embeddings until their selected stage-4 implementation slices; the stage scope is selected, but per-operation designs and activation remain separate.
- New Providers and model bindings unless an approved slice reopens them. Alternative bindings for an existing public model belong to failover work; activating a registered binding is not adding one.
- New subscription credential features.
- Files/multipart Uploads until their selected stage-4 implementation slices.
- Server-side compaction execution, Batch and Realtime while stage 5 remains unscheduled.

### Not planned

- Inference-engine integration, including llama.cpp/SGLang server bindings or embedded model execution, and engine GPU/KV-cache scheduling.
- Raw wire passthrough, post-encoding JSON patches or weakened schemas.
- Invented tool results or changed roles for orphan results.
- Message-content hashes as conversation identity, or LLM classifiers for routing.
- Plugin hosts, middleware hosts or capabilities learned automatically from upstream errors.
- Provider-held conversation state or background Generation execution.
- Load balancing, credential concurrency/RPM lanes or budget reservations.
- Retrying a refusal on another model.
- Routing below the declared conversation: turn detection, branch/subtask keys, per-turn holds or model switches, and per-message effort.

These boundaries follow [Failover](architecture.md#failover) and [Settled decisions](architecture.md#settled-decisions).

### Maintenance rules

Observed vendor stream shapes become named dialect rules only when a bound upstream emits them, such as empty continuation tool names or call IDs, empty finish reasons and tool-call openers. Treat a bound upstream's in-stream error as a classified failure, not normal output. Do not expand standalone media implementations incidentally during stages 1–3; use the selected stage-4 slices.
