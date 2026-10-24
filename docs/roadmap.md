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
  - [Decision API design](#decision-api-design)
  - [Stage 4 scope](#stage-4-scope)
    - [Next stage-4 slice](#next-stage-4-slice)
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

**Current priority (2026-10-24):** continue the accepted breaking redesign with standard delivery/continuation, actual adaptation reports and the integrated text/SSE gate. Release reset, initial asset integration and the selected canonical ownership/mapping package are complete. Historical acceptance below is not the regression oracle for new semantics. [The integrated sequence](#standards-reset-and-redesign) supersedes conflicting earlier freeze, mapping-preservation and test-supplement-only instructions for this scoped work.

| Area | State | Next step |
|---|---|---|
| Routing | Functional phase complete under the [stable routing baseline](architecture.md#failover); feature development closed. Selected [controlled acceptance](#controlled-routing-acceptance) passed through Router, SDK and installed clients | [Production closure](#generation-text-production-closure) requires separate real evidence for routes promising failover; no routing feature expansion |
| Generation IR/codec | Selected canonical ownership and mapping corrections complete within the [contract inventory](refs/2026-10-23-stable-protocol-contracts.md#official-contract-difference-inventory), including independent request/report null boundaries, prefix composition, historical grouping, Google sampling/structured results and Interactions report/signature/EOF intake | Standard delivery/continuation and actual adaptation reports; inventory exclusions/source disagreements remain, with no all-official-semantics claim |
| Client compatibility | Rejection audit, [phase transfer/return](architecture.md#message-phase-transfer), guarded extended-retention omission and retained citation rejection complete. [Downstream schema completion](architecture.md#downstream-schema-completion) implemented on 2026-10-23 with strict pinned-SDK JSON/SSE and stored-resource checks | Preserve SDK schema, source-fact and terminal-honesty regressions; broader official semantics remain explicitly scoped |
| Integration | Selected R1a/R1b/R2 client loops and controlled routing acceptance passed their bounded scopes. R1b lifecycle reports and client history shorthand were explicitly selected on 2026-10-20 | [Production closure](#generation-text-production-closure): bounded capability and sustained-operation acceptance, not a full client/model matrix |
| Cross-protocol conversion and counting | Stage 3 closed its selected projection and loop scope; counting is deferred | Preserve that evidence; the [Google pause](#google-api-pause) has selected text-report and stage-4 Embedding exceptions, not general Generation reopening |
| Generation Text production readiness | Historical production closure completed on 2026-10-22 with explicit exclusions | Re-establish selected readiness after the standards redesign and integrated text/SSE gate, before further stage-4 implementation |
| Code simplification | Completed on 2026-10-22: five preferred slices and selected local cleanups; full offline baseline and all 12 connected SDK scenarios passed | No remaining scheduled simplification; API/semantic contracts and freeze boundaries unchanged. Further cleanup needs its own demonstrated benefit; no new live-acceptance or measured-performance claim |
| Decision APIs | [Dual standard-SDK direction](architecture.md#decision-api-direction) selected on 2026-10-23; initial TypeSafe/OpenAI/Perplexity/Cloudflare protocol research only | [Decision design review](#decision-api-design) before further modality work; no implementation or binding activation |
| Embedding, other modalities and file resources | Sample-centered Embedding, compatible delivery and native Google/OpenAI-model integration accepted on 2026-10-23: full offline baseline, scoped SDK and bounded Provider checks. Grok static codec has no gateway/live acceptance | Complete the [text/SSE prerequisite](#current-text-and-sse-gate) before further implementation; retain Decision design and [static Images Generate](#next-stage-4-slice) review. Later slices inherit [operation-level test acceptance](#stage-4-reuse-and-acceptance); further Embedding modalities and video remain unselected |
| Stage-5 follow-ups | Citations, tool discovery, hosted tools, server-side compaction, Batch and Realtime are recorded but **unscheduled** | No implementation queue or delivery commitment; select each slice before starting it |
| Agent authorization and diagnostics | [Single-deployer, Agent-centered baseline](architecture.md#agent-identity-and-cluster-diagnostics) selected on 2026-10-23; implementation **unscheduled** | [Follow-up gates](#agent-authorization-and-diagnostics-follow-up); not a stage-5 API assignment or a prerequisite for current modality work |
| External protocol tests | Stable sources, reviewed byte-locked assets, thin codec adapters and selected SDK consumers are integrated; current desired-behavior probes run normally and the [contract inventory](refs/2026-10-23-stable-protocol-contracts.md#official-contract-difference-inventory) separates defects, exclusions and source disagreements | Extend delivery, actual-loss and transport-lifecycle integration to close the current text/SSE gate; fixture integration is not new conformance acceptance |
| Real acceptance | Selected stage-2 loops and bounded [production gates](#generation-text-production-closure) have recorded version/profile-specific evidence | Preserve historical limitations and [other deferred acceptance](#deferred-acceptance); new profiles or stronger claims need their own gates |

The [approved Generation baseline](#generation-design-review-gate) leaves inventory exclusions and binding boundaries unchanged. Other real acceptance gates remain open. Google value guards and Interactions snapshots belong to stage 3: no Google client endpoint forms a same-protocol pair. Provider-specific reset dialects, persistence and cooldown tuning need separate evidence and approval; no background probes, balancing or multi-round retries are scheduled.

Phase completeness covers the declared feature scope, not all official Generation semantics; explicit rejection is safer than silent loss but is not completeness. Chat Completions, Responses and Messages are equally important for same-protocol request, response, static delivery, streaming and history replay. Google generateContent and Interactions supply upstream request encoding and SSE intake only; record missing directions rather than implying a complete same-protocol pair.

## Standards reset and redesign

The user accepted the complete design review on 2026-10-23 and required a current-stable dependency/source reset before implementation. [Architecture](architecture.md#protocols) owns the new contracts; [Development](development.md#upstream-test-sources) owns procedures. This is a revision of Generation stages 1–3 and their test gate, not a sixth API stage.

Release reset and the bounded official-contract inventory/initial asset integration are implemented. Sources, reviewed fixtures and local extraction recipes are pinned; executable unmet requirements remain explicitly separate from regression. This closes preparation, not standard compliance. Extend the inventory and external cases at each responsible owner below rather than restarting a parallel audit framework.

| Slice | Work and exit evidence |
|---|---|
| Standard delivery and continuation | Remove public Provider/MorphieCore dialect assumptions and fabricated reasoning manifest owners. Preserve valid native replay and existing managed-history state without lossy temporary-wire composition. Check required fields on success/failure, stream order, EOF, errors and cancellation |
| Actual adaptation reports | One execution-owned bounded report, selected-attempt and successful-boundary evidence, field-specific estimates, JSON/SSE/retrieval agreement and concise standard/namespaced notice carriers. Verify raw SDK events separately from final-object retention; do not publish a held candidate merely to report preparation |
| Integrated acceptance | Close all four [text/SSE gate](#current-text-and-sse-gate) slices against the new contracts, then independently accept selected stable clients and bounded Providers. No legacy expected-output regeneration or blanket weakening to recover green tests |

**Deferred review follow-up (2026-10-24):** the user deferred unified repair planning for the findings below until the current redesign stage is complete and its new tests are integrated. Record them beside the plan without starting repairs or changing the current execution order. Some are inherited gaps, not regressions introduced by this redesign. New tests must preserve the desired contracts and identify unmet requirements explicitly, not make these behaviors the expected oracle.

| Priority | Recorded gap | Responsible boundary |
|---|---|---|
| P2 | Context edits can move a retained call from one named assistant owner to another without changing either participant label or the call identity; encoded attribution changes | [Context transforms](../src/semantic/task/generation/request/context_transform.rs): protect effective participant association as well as retained owner fields |
| P2 | Public Messages intake erases non-nullable top-level `system`, `thinking`, `tool_choice`, `tools`, `output_config` and `stream` nulls that native intake rejects. An empty ordinary assistant message also disappears, allowing adjacent user content to merge | [Client intake](../src/protocol/anthropic/messages/client/request.rs): reconcile source validation before normalization, preserving legitimate nullable-cache and effort-only-system exceptions |
| P2 | Interactions silently omits an empty assistant envelope when nonempty members already cover all items; the partial-group safeguard does not run | [Native grouping](../src/protocol/native.rs) and [Interactions item projection](../src/protocol/google/interactions/items.rs): preserve or explicitly reject unsupported empty containers |
| P2 | Messages still maps absent visibility plus nullable summary preference to explicit `display: omitted`, conflating the canonical presentation axes | [Messages request projection](../src/protocol/anthropic/messages/request.rs): reconcile nullable preference with explicit visibility without assuming equivalence |
| P3 | Modality-only usage reports skip aggregate accounting-relation validation, so flat usage rejects a contradictory state that report/response construction accepts | [Scoped reports](../src/semantic/task/generation/usage/reports.rs): separate relation validity from nonempty-observation checks |
| P3 | Typed Responses requests can encode `reasoning.mode: null` although request wire intake rejects it; report-null support leaks into the request direction | [Request validation](../src/semantic/task/generation/request/mod.rs) and [OpenAI reasoning](../src/protocol/openai/reasoning.rs): keep request/report validation directional |

Reassess these findings together at that later planning checkpoint before selecting repair slices and acceptance scope. Deferral neither certifies the affected contracts nor authorizes routing, API or binding expansion.

Upstream fixtures and real SDK consumers enter each owner/codec slice as it is rewritten, not as a comprehensive cleanup after implementation. Reuse existing valuable scenarios and safety assertions; classify each affected old expectation as retained, replaced by a source-backed contract, or removed with its obsolete behavior. Keep unresolved semantics visible rather than skipping them as passed.

The initial assets do not exhaust the selected gate: version-aware HTTP admission/failure envelopes, complex multi-call Router/SDK cases, actual-loss extensions and deeper transport lifecycle coverage remain with the dependent implementation slices. Historical client/Provider success cannot certify updated releases. Runtime routing redesign, new bindings, native Google ingress, token counting, later modalities and stage-5 APIs are not activated. Independent Decision design may continue without blocking this reset.

## Planned order

| Stage | Goal | Exit evidence for a selected slice |
|---|---|---|
| **1 — Same-protocol lossless IR codecs** | Preserve the declared protocol semantics through client request → IR → same-protocol upstream request, and upstream response/events → IR → same-protocol client delivery/history. Prioritize known fields, presence/default meaning, ordered owners, reasoning/replay, usage and terminal facts over cross-protocol convenience | Independent wire→IR and IR→wire expectations in each applicable direction, JSON/SSE agreement and returned-history preservation. Report unsupported fields/directions and actual losses explicitly; round trips, HTTP success and named loss receipts are not lossless evidence |
| **2 — Basic agent tool interactions** | Ordinary function-tool declarations and schema intent, call IDs/arguments, results/errors, streaming, final answers and next-turn history for pi/OpenCode. The client executes its tools; no gateway tool executor is implied | Selected same-protocol tool-call → result → final-answer → replay loops, preserving source data and required reasoning/cache owners. Keep offline, SDK and bounded real-client/Provider evidence separate |
| **3 — Cross-protocol stability and token counting** | Stabilize selected request, response/event and history conversions through exact carriers or approved named adaptations. The selected projection/loop scope is closed; `count_tokens` is [deferred](#deferred-token-counting) | Independent directional expectations and selected loops; admission/preparation agreement and rejection of protected unrepresentable semantics. Deferred counting retains its separate upstream-first policy and exit gate |
| **4 — Embedding, modalities and file resources** | Adapt Embedding and audio/image/video APIs through task IR, codecs, execution and delivery; implement Files and multipart Uploads as resource services | Verify formats/dimensions, controls, results/events, bounds, cancellation, accounting and applicable SDK/real gates. File resources require authorization, bounded upload/part handling, identity, retention/deletion and restart-consistent mappings |
| **5 — Unscheduled follow-ups** | Citations/source attribution, `tool_search`, hosted tools, server-side compaction, Batch and Realtime | **Unscheduled:** select each scope/design and explicitly schedule its slice before implementation. No start/delivery commitment or prerequisite for stages 1–4 |

The five-stage order is unchanged. The selected [Generation Text production closure](#generation-text-production-closure) is a bounded handoff before stage 4, not a sixth stage, a reopening of stage-3 counting or a claim of complete official Generation coverage.

The latest 2026-10-23 instruction selects the [standards reset and redesign](#standards-reset-and-redesign), integrating the existing text/SSE plan rather than first accepting the old mappings. Complete the integrated gate before further stage-4 implementation. Decision research/design remains independent and precedes later modality work; neither activity activates stage 5.

“Lossless” means semantic fidelity within an explicit protocol revision and feature domain, not byte-identical JSON or retention of unknown fields. Equivalent wire normalization is allowed. Preserve protocol records in their existing bounded owners; do not introduce raw passthrough or an IR extension bag. A same-named API at a different Provider is not proof of equivalent semantics.

Audit existing same-protocol loss paths and distinguish codec loss from model/product policy. Basic tool-field fidelity belongs to stage 1; stage 2 adds the agent interaction loop. All previously approved conversions are reopened under the new hierarchy; current behavior is not normative merely because an old test asserts it. Deferred features remain explicit coverage gaps, not silently discarded content or complete protocol support.

Design review and focused coverage proceed within each stage, not as a requirement to finish every later-stage design first. Preserve security, bounds, cancellation, replay isolation and honest terminals while replacing obsolete mapping assertions. Existing media safety coverage remains; new modality APIs and execution paths belong to stage 4. Other-agent acceptance keeps its [separate scheduling](#deferred-acceptance).

The P7 boundary review becomes eligible after first-stage local completion, without waiting for deferred cache and tool slices, and is required before route-facing acceptance. Its analysis may proceed alongside design closure.

### Execution sequence and work-package rules

The packages below refine the selected scope, not design approval or binding activation. IDs name coherent deliverables, not a required commit count. Stage-4 ordering is recommended; exact operation contracts still need selection. Stage 5 has no execution queue.

Inference frameworks such as llama.cpp and SGLang are design references, not integration targets. Use their input-preparation, generation-constraint and output-interpretation designs to improve the existing gateway boundaries. This adds no backend work package, engine dependency or generic session/plugin framework; [inference-engine integration is not planned](#not-planned).

1. Stage-1 selected codec work is implemented; use its [inventory and boundaries](refs/2026-10-19-same-protocol-inventory.md) for review and per-loop readiness. No all-official-semantics or real-loop completeness is implied.
2. The selected R1a, R2 and R1b loops completed independently under their own carriers and approval/admission gates. A future loop waits for its own readiness, not every stage-1 package; no additional client/model matrix is scheduled.
3. R2 readiness concerns its implemented pi Haiku profile, not wider native manual-budget or sampling support. An observed defect in a carrier actually used by R2 must still be fixed before accepting it.
4. Complete selected stage-2 loops before routing acceptance and stage-3 conversion work. Keep separately scheduled real failover distinct from controlled synthetic recovery checks.
5. Preserve the closed stage-3 projection/loop evidence. Counting remains deferred; its future implementation waits only for its selected effective-input projection.
6. Apply the [standards reset and redesign](#standards-reset-and-redesign), then close the integrated text/SSE gate before further stage-4 implementation. Preserve safety evidence, not superseded mapping expectations; unrelated API activation remains separate.
7. In stage 4, select the next operation's contract just in time and apply [operation-level upstream-test acceptance](#stage-4-reuse-and-acceptance). Design file authority early, but let direct-input Embedding/audio/image slices proceed without the complete file service. File-reference integration waits for authorized resource resolution.

Every implementation package follows the same bounded workflow:

- **Entry:** name the pinned source revision, dialect, feature domain, relevant binding/client profile, existing owners and unresolved decisions. Extend sourced `docs/refs/` notes for protocol findings; do not create run-report documents.
- **Delivery:** implement one coherent request/result/event/history or resource-lifecycle slice. Present a minimal counterexample and options before introducing an unselected semantic structure; a decision blocks only dependent work.
- **Preparation and continuation:** preserve the gateway's existing retained binding for request preparation and response decoding, with fresh mutable decoder state per attempt. Keep candidate projections independent and preserve [history-prefix stability](architecture.md#candidate-local-projection) as turns are appended. These are invariants to maintain during each slice, not a new session/snapshot deliverable; change ownership only for a demonstrated gap.
- **Evidence:** write independent directional expectations and focused failure checks at the responsible layer. Separate equivalent normalization, exact carriage, approved loss, rejection and unassessed cases. Distinguish IR expression, protocol carriers, binding admission, model effectiveness and accepted client loops; a generic tool-support claim cannot establish required/named choice, strict schemas, parallel calls or streamed arguments. Scope new capability acceptance to the selected profile, including history continuation, without dropping existing regression assertions. Prepare the [human review](#generation-design-review-gate) evidence incrementally; no passing test or agent review closes that gate.
- **Exit:** apply the [publication and integration gates](#verification-and-integration-boundary). Report codec, admission, SDK and real-client/Provider status separately, including blocked checks. Commit/push only after the applicable checks; remove completed packages from this page and keep accepted decisions in Architecture.

### Stage 1 work plan

**Status: selected implementation complete; the overall stage-1/2 baseline was human-approved on 2026-10-20.** Chat/DashScope–TokenPlan, Responses/OpenRouter, Grok and public OpenAI SIWC, and Messages/Go have independent request/result/event/history expectations within the [pinned inventory's](refs/2026-10-19-same-protocol-inventory.md) declared domains. Both Aliyun billing plans use the same dialect and shared-model controls without merging their authority. Google generateContent and Interactions still have no client/static pair; their guards, snapshots and conversions remain stage 3.

Accepted contracts live in Architecture: [Chat readable replay/history reading](architecture.md#chat-reasoning-extensions), [Responses item/event observations](architecture.md#responses-item-and-event-observations), [stored history](architecture.md#stateful-history), and [native Messages controls and partial-input boundaries](architecture.md#native-messages-projection). Completed work packages are removed here, not retained as another implementation specification.

The user-selected [Bailian compatible-request correction](architecture.md#shared-aliyun-billing-dialect) covers existing IR carriers and rejects ignored/unrepresentable controls. It does not claim the full DashScope API, turn native RPC into an endpoint or activate new model features. [Independent output ceilings](api-expansion-design.md#independent-output-ceilings-unselected) remain unselected design, not another stage-1 implementation package.

No new binding is activated. JSON/SSE equivalence applies only where each native schema has a carrier: event-only summary interruption/compaction progress are not a static event log; unfinished Messages arguments are native-SSE-only under the selected `max_tokens` contract and still reject as JSON/history. Manual-interleaved budget scope remains deferred, and adaptive-only Haiku still rejects manual thinking. Target-policy losses/default insertions remain named and are not exactness evidence.

The [overall human review](#generation-design-review-gate) and selected [stage-2 loops](#stage-2-work-plan) are complete within their declared scopes. Synthetic SDK/client-carrier and private visible-history probes do not accept real Provider behavior or signed account-compatible replay.

Grok/SIWC verification covers their existing Responses bindings, restricted request preparation, native static/event directions, JSON/SSE delivery and stateless/stored history parity. SIWC's upstream is always streamed; static upstream JSON fixtures test representation, not HTTP support. Provider-only scoped reports remain an [explicit client-delivery loss](architecture.md#subscription-responses). This extension adds neither Grok Chat nor wider SIWC controls, and does not broaden the separately selected [stage-2 loop acceptance](#stage-2-work-plan).

| Pair | Field or carrier | Current treatment | Recommendation |
|---|---|---|---|
| Chat | `prediction` | Legacy intake omission under D3 | Superseded: retain typed meaning before target selection, then exact carriage or reported omission |
| Chat | `n` other than 1, `store: true`, `logit_bias`, request `moderation`, content `prompt_cache_breakpoint`, non-null moderation reports, custom-tool SSE | Rejected | Keep as recorded stage-1 gaps; rejecting cache boundaries/reports prevents silent loss but is not exact support |
| All | Unsupported hosted/server item and block variants; Messages citations, `clear_at`, active geography/speed/fallback requests; Responses active `prompt`, `conversation`, `context_management`, access programs and misalignment details | Rejected within the inventory's stated subsets | Messages context/examples now have bounded carriers with separately closed binding admission under [production closure](#generation-text-production-closure); other exclusions remain deferred |
| Messages / Responses | Messages `diagnostics` and unowned execution reports; deferred Responses hosted event families | Ignored, explicitly classified in the inventory. Protected controls/beta execution facts reject; stable refusal reports now have native carriers | Keep remaining gaps explicit, not lossless |

Stage-1 exit for a pair means every in-domain official field is exact, or is an explicitly listed rejection or target-policy loss. Approved losses that still fire on same-protocol paths remain listed as non-lossless.

#### Stage-1 to stage-2 handoff

A stage-2 loop needs only its own protocol slice, scoped to that loop's carriers, not the whole of stage 1: R1a follows S1-1 and R1b follows S1-2. R2 is pi's `anthropic-messages` API with the built-in Haiku 5.5 compat flags, which the user's separate pi extension declares for the gateway. The flags are listed in the [pi profile note](refs/2026-10-17-generation-contracts.md#pi-messages-request-profile-2026-10-19). Its carriers are implemented and passed a bounded check, so R2 needs no further S1-3 work. Acceptance runs declare the same model in scratch `models.json`; a declaration without `forceAdaptiveThinking` sends budgeted thinking, which the native target rejects, and is not R2.

Every selected loop must leave no unclassified ignored request fields: `ignored_request_field_count` stays zero, or each ignored name is classified in the inventory. Client-specific dialect fields such as pi's `enable_thinking` or `chat_template_kwargs` are outside the official Chat baseline. The selected pi and OpenCode profiles must emit only registered carriers.

### Stage 2 work plan

**Status: selected R1a/R1b/R2 loops and controlled routing acceptance are complete within their declared scopes.** The user separately selected R1b's [bounded lifecycle reports](architecture.md#lifecycle-settings-and-service-reports) and [client history shorthand](architecture.md#client-responses-history-shorthand) on 2026-10-20. The [overall baseline is now approved](#generation-design-review-gate); neither approval nor these scoped acceptances certify every client, model feature or protocol semantic.

These scoped loops implement the [selected reconnection contracts](#first-reconnection-slices), not a new binding or routing policy. Loop acceptance and overall design approval are separate; neither authorizes later activation. A future loop still needs its own boundary review and binding evidence. R1a and R1b are separate acceptance slices; neither needs R2 or stage-3 conversions to succeed.

R1a's explicitly resumed bounded scope passed on 2026-10-19: DashScope Chat, ordinary non-strict functions, low effort, explicit total output cap and readable reasoning history. Independent Router/SDK JSON/SSE oracles cover tool/result/final/next history and failure boundaries. Live SDK JSON/SSE and installed pi 1.3.0 / OpenCode 2.0.6+e5ecb57 each completed their selected three-turn loops. Client runs used scratch state, at most three serial upstream requests, a 2048-token cap, 120-second request deadlines, one gateway attempt and no forwarded retry; successful checks reported zero ignored request fields. A streamed SDK accumulator and a pi assertion were corrected before their successful checks; those earlier harness failures are not Provider failures. This accepts only the tested profiles, not every effort alias, tool schema, client version or model feature.

R1b passed its selected lifecycle and history gates on 2026-10-20: independent native/IR checks, Router/SDK terminal retrieval parity, 12 real pinned-SDK requests across JSON/SSE and stateless/stored loops, and separate three-request installed pi 1.3.0 and OpenCode 2.0.6+e5ecb57 SSE loops. The [dated evidence](refs/2026-10-17-generation-contracts.md#responses-lifecycle-reports-2026-10-19) records the selected OpenRouter/Luna profile, run bounds, earlier failures and client shorthand. SDK runs observed phase, opaque reasoning and summary content; neither final installed-client run establishes opaque replay.

R2 passed on 2026-10-19 after the user deferred R1b, using `claude-haiku-5-5` / `opencode-go-haiku-messages` and the selected [pi Haiku profile](refs/2026-10-17-generation-contracts.md#pi-messages-request-profile-2026-10-19). Independent Router/SDK JSON/SSE expectations and publication/cancellation/no-replay checks cover its native tool/history contract. Installed pi completed a five-request SSE session with tool/effort changes and reported 1h cache reuse; a separate three-request 5m JSON companion confirmed exact native signature restoration. The [dated evidence](refs/2026-10-17-generation-contracts.md#r2-native-client-and-replay-evidence-2026-10-19) retains run bounds and limitations: no cross-account/fallback, measured-expiry, invoice or effective-effort guarantee. Automatic-cache admission remains closed.

No further stage-2 loop is scheduled. Before any later selected loop, repeat per-loop readiness: exact client shape, model-effective carrier evidence, public admission, shared preflight/preparation and classification of every ignored field. Keep exact support, adaptation and rejection distinct.

For each loop, establish independent static and SSE expectations before real acceptance, including tool errors, strict EOF, cancellation and no replay after publication. Use the [existing SDK and client procedures](development.md), not a new acceptance framework. Before every real run fix the target, request count, output cap and timeout; run serially through the project loader with no hidden retries and body-free reporting.

Extended-retention omission is not a prerequisite for these exact-carrier loops. If a later slice selects it, first verify that target's storage policy; cache usage is not retention evidence.

### Stage 3 work plan

The stage covers request projection, selected cross-protocol loops and Interactions intake, and its token counting is [deferred](#deferred-token-counting). Target-value rejection, trusted missing-cap defaults, attribution priority, owner-by-owner Interactions snapshot reconciliation, final-envelope lifecycle authority and the [upstream-first counting policy](architecture.md#token-counting-policy) are selected, not new alternatives to debate. Selected Google basic correction/history loops and snapshots are complete within the evidence below. The later [production closure](#generation-text-production-closure) selects only a narrow text-report exception to the [Google pause](#google-api-pause); it does not revise this historical acceptance.

Implemented projection contracts are [model-effective guards](architecture.md#model-effective-values), [trusted Messages missing-cap defaults](architecture.md#missing-output-ceilings) and [cross-protocol attribution](architecture.md#end-user-attribution). The existing Gemini bindings declare low/medium/high and a 65,536 output maximum; Go/Haiku retains an explicit `32000` fallback, with none in generic targets or pure codecs. Dated evidence records the [model limits](refs/2026-10-17-generation-contracts.md#model-effective-limits-2026-10-20), [default provenance](refs/2026-10-17-generation-contracts.md#messages-missing-ceiling-evidence-2026-10-20) and [attribution carriers](refs/2026-10-17-generation-contracts.md#attribution-carrier-recheck-2026-10-20). Independent directional and preflight/preparation checks, the full offline baseline and pinned synthetic SDK/Python gates passed. These slices activated no binding and added no installed-client or live Provider acceptance.

No stage-3 package is scheduled: token counting was [deferred on 2026-10-21](#deferred-token-counting), so the stage closes on its projection and loop evidence. Unsupported protected semantics continue to reject, and those slices do not imply full official-protocol coverage.

Google intake implements the [selected foreground contracts](architecture.md#google-foreground-intake): strict supplied identities, required update/final statuses, continuation rejection, native tool-part ordering and rejection of uncarried reports. Missing chosen-token probabilities reject rather than defaulting to zero; sparse alternatives retain unknown facts. Status checkpoints neither infer transitions nor finish the stream. Focused semantic/Router, full offline and pinned SDK/Python regressions passed; this maintenance adds no live acceptance or new report structures. Background execution, recovery, remote cancellation, broader logprobs and unrelated media/hosted tools remain outside scope.

The generateContent single-call correction/final-history slice passes Router and pinned SDK JSON/SSE checks over one candidate, plus a separate CLI-loaded live SSE run on Gemini 3.8 Flash: four Chat requests followed by four Responses requests, 2048 output tokens and 45 seconds per request, one attempt, no SDK retries. Both profiles completed error feedback, corrective call, final answer and returned final history.

Interactions supplied-step reconciliation, [sparse lifecycle metadata](architecture.md#google-foreground-intake) and the selected [call-only signature carrier](architecture.md#reasoning-replay) are implemented. Router and pinned SDK JSON/SSE correction loops preserve distinct thought/call signatures, summary/argument fragments, snapshots and final history. SDK evidence also checks indexed trailing manifests and non-repeated constant format metadata. A separately bounded native v1 run passed Chat 4 + Responses 4 SSE requests through error feedback, correction, final answer and subsequent history. Multipart content and service reports currently remain rejected; model fallback remains unselected. The later report slice must resolve its own [source/version boundaries](refs/2026-10-17-generation-contracts.md#interactions-partial-opening-and-service-evidence-2026-10-20).

On 2026-10-21, installed pi 1.3.0 Chat and OpenCode 2.0.32 Responses passed four-request error → correction → final-answer → next-turn-history loops against each Google API: 16 accepted-case requests. The [dated evidence](refs/2026-10-17-generation-contracts.md#google-installed-client-acceptance-2026-10-21) retains run bounds, history checks and earlier timeout/harness failures; the [version-continuity decision](architecture.md#scope-and-activation-decisions) preserves earlier scoped results.

The initial [pi Messages probes](refs/2026-10-17-generation-contracts.md#google-messages-client-boundary-probes-2026-10-21) failed and remain failure evidence. The user subsequently selected [response-effort projection](architecture.md#google-response-effort-projection) and static diagnostics, then the bounded [Messages terminal profile](architecture.md#google-messages-terminal-projection). Independent delivery, Router and pinned SDK JSON/SSE checks, the full offline baseline and SDK/Python regressions passed. Two fresh pi 1.3.0 Messages SSE loops then passed four requests per API; [dated closure evidence](refs/2026-10-17-generation-contracts.md#google-messages-basic-loop-closure-2026-10-21) records those eight requests separately from earlier failures. No new report or usage policy was needed.

These buffered-SSE client runs do not establish incremental-publication timing, real simultaneous multi-call ordering, every client/protocol combination, full native-field coverage, accounting accuracy or cross-account failover. Native Google client/static intake remains closed; further expansion follows the [pause](#google-api-pause).

Further projection work must retain final-value/default assertions at the intended target boundary and receipts matching the serialized request. Reuse encoder rules without retaining every candidate's serialized body; transport must not reinterpret prepared semantics.

#### Google API pause

The user selected basic-loop closure followed by a pause in further Google API improvement on 2026-10-21, accepting incomplete operation outside the selected profiles. Keep the accepted bindings, three-level effort mapping, terminal rules and replay protections; allow necessary correctness/security maintenance without expanding the feature scope.

A later decision on 2026-10-21 selects one [text-report exception](architecture.md#explanatory-terminal-reports): bounded `finishMessage` and the concrete [processing-class reports](architecture.md#service-policies-and-reports) are implemented. The [source recheck](refs/2026-10-17-generation-contracts.md#google-text-report-source-recheck-2026-10-21) establishes the selected v1 carrier and processing meaning; unproven defaults/transitions still reject. The exception adds no Google downstream/static direction or request policy.

On 2026-10-22 the user requested one Google Embedding target, then paused it for review. On 2026-10-23 the user explicitly resumed Google/OpenAI Embedding implementation with named lossy compatibility. This [standalone text exception](#embedding-integration-targets) does not reopen Google Generation, native downstream APIs, other models or multimodal ingestion. The broader Google API pause remains in force.

Probability/token-identity owners, general Generation usage approximations, native Google client/static directions, cache/strict-tool/dynamic-control expansion, new Generation models/modalities, hosted tools and background execution remain unscheduled. A passing basic loop is not full Google conformance. These gaps do not reopen stage 3. `count_tokens` remains independently [deferred](#deferred-token-counting).

### Generation Text production closure

**Selected on 2026-10-21; complete on 2026-10-22 within the declared profiles and exclusions.** The bounded offline, SDK, installed-client and Provider gates are complete; procedures and exact evidence live in [Development](development.md#production-profile-acceptance) and the [dated notes](refs/2026-10-17-generation-contracts.md#sustained-operation-boundaries-2026-10-22). This closes the stage-4 prerequisite, not public hosting, all official fields, a Provider/model matrix or deployment authorization. Stage-3 and earlier version-specific acceptance remain valid only within their recorded scopes.

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

`count_tokens`, complete citations, cache-only prewarming, hosted tools/discovery, server-side compaction, wider budget scopes, additional clients/models and other stage-5 work remain deferred. Responses history stays process-local TTL storage, not persistence. Production readiness is correct selected text/tools/structured output, history, cancellation, failure handling and diagnosable sustained operation—not accounting accuracy, arbitrary schemas, every real refusal/parallel-call combination or permission to deploy. Stage 4 starts with its next-operation design; new semantic policy still needs a focused selection.

### Decision API design

**Current design priority, selected on 2026-10-23.** Plan stable standard TypeSafe and OpenAI SDK access through one Decision IR and upstream codecs/dialects. The [accepted direction](architecture.md#decision-api-direction), [proposed contracts](api-expansion-design.md#decisions) and [dated evidence](refs/2026-10-23-decision-contracts.md) have separate owners. Research starts with TypeSafe, OpenAI, Perplexity and Cloudflare; no generic mainstream-Provider compatibility claim follows.

The user subsequently selected Generation-style capability-union ownership and consideration of named approximations/simple estimates before rejection. The [IR draft](api-expansion-design.md#decision-semantics-and-compatibility) now includes media/Box, service intent and source/delivery separation; its concrete fields and [projection matrix](api-expansion-design.md#named-decision-projections) remain under review. Union coverage is not first-slice activation or approval of each lossy rule.

Before dependent implementation, complete this bounded review:

1. Pin the SDK/schema baselines and resolve consequential source discrepancies. Include companion discovery, error handling, URL joining and request compression, not only inference JSON.
2. Review the full evidenced union's evidence/question/result and service owners, native preservation domains and directional cross-family rules. Show minimal counterexamples for names/order, structured rubrics, role evidence, Boolean choice values, confidence, refusal, media references and Box framing. Distinguish exact carriage, derivation, behavioral approximation, simple estimation and rejection; select concrete rules separately from the design principle.
3. Select a finite synchronous execution profile, bounds and target admission. Decide how to prevent or explicitly reject unsupported context/truncation behavior; do not inherit Generation fallback or silently split questions.
4. Obtain human selection of the remaining contract choices. Record accepted rules in Architecture and schedule a coherent implementation slice only then. Both SDK families are required; a working single-vendor path is not closure of the interoperability goal.

Future acceptance for an authorized implementation must separate:

- **Offline contracts:** independent wire→IR and IR→wire expectations for each selected baseline/dialect and projection; exact question/answer correspondence, probabilities, refusal, strict JSON, bounds, cancellation and EOF/publication checks.
- **Standard SDK loops:** pinned official clients with configured base URL/key/model, without patches or custom response types; actual typed answer completeness, discovery, success/error behavior and selected defaults including compression. Exercise accepted cross-Provider paths for both families. SDKs that skip unknown answers must not make incomplete delivery pass.
- **Provider evidence:** fixed target/count/output bounds/timeout, serial calls through the project loader, no hidden retries and redacted reporting. Record model identity, grouping/usage and context behavior separately from protocol shape. Statistical quality/calibration and threshold portability require their own evidence, not a successful request.

This review does not activate endpoints, promise every modality in either SDK, reopen deferred routing/counting, or authorize comprehensive coverage of unresolved contracts. Images retains its next internal review below; further modalities wait for this design discussion, not an implied full Decision rollout.

### Stage 4 scope

The [API expansion design](api-expansion-design.md) supplies operation proposals, not implemented endpoints or approved type layouts. Keep conversational media in Generation where appropriate and independent operations in their own task contracts. Further implementation requires the completed [Generation Text production closure](#generation-text-production-closure), the [current text/SSE test gate](#current-text-and-sse-gate) and per-operation selection. The order below is recommended; stage-4 work does not reopen completed stage-1/2/3 gates.

**Current priority: complete the standards reset, accepted Generation redesign and integrated text/SSE gate before further stage-4 implementation.** Decision design remains independent preparation for later modalities. Existing Embedding behavior is historical acceptance; do not expand its operations during the Generation reset. The [readiness comparison](#next-stage-4-slice) retains static Generate as the next Images review, not implementation authorization. Audio/Files are not current implementation work; video remains deferred.

The current review exit for unselected operations is a coherent request/result and applicable event contract, source-backed model restrictions, bounded admission/execution/delivery and counterexamples for unresolved choices. The [accepted Embedding contract](architecture.md#independent-embedding-path) and [Image design choices](api-expansion-design.md#image-design-review) are distinct. Record newly selected structures in Architecture before dependent implementation or ordinary comprehensive coverage; earlier approvals do not extend to unresolved operations.

Every selected implementation slice also meets the shared [upstream-test acceptance conditions](#stage-4-reuse-and-acceptance). The table below adds operation-specific obligations; external tests do not replace local IR, resource or lifecycle evidence.

| Package | Dependency and work | Exit condition |
|---|---|---|
| S4-0 Next-operation design | Pin current official request/result/event contracts, choose one target and task/resource owners, and state exclusions. Select file authority early; select later operations just in time rather than requiring a complete redesign upfront | New semantic structures and execution-policy choices have explicit decisions. Existing owners/shared values are reused where appropriate; no universal task bag, repository-wide rewrite or implicit new binding |
| S4-2a Speech | Complete the selected static binary contract, then independently deliver progressive binary/SSE where supported. Preserve text, instructions, voice, speed, format and reported accounting | Static and streamed evidence remain separate; bounded buffering, cancellation and honest completion hold. No fabricated transcript/usage or implicit voice-resource creation |
| S4-2b Transcription | Complete file-based recognition with native multipart upstream preparation, static decoding and selected stream intake | Real reported text/timing/speaker/probability/usage facts survive; unsupported output formats reject instead of inventing timestamps. Large input and cancellation stay bounded; a reusable File object is not required |
| S4-2c Translation | Reuse the necessary audio-input infrastructure from S4-2b, but select and implement native translation as a distinct operation | Current model/language restrictions are rechecked; translation intent and results are preserved. No implicit ASR-plus-Generation substitute |
| S4-3 Images | **Next modality review after Decision design.** Proposed implementation order: Generate/defaults/result-delivery closure; direct-input Edit; selected single-output preview/final streaming. Keep source order, mask association, controls and artifact identity | Review operation/source owners, Grok public delivery/defaults, mask/input bounds and stream association/reports first. Later delivery needs independent generate/edit oracles and task-specific lifecycle checks; previews do not satisfy final counts. Only file-reference forms wait for S4-4c |
| S4-4a Files | After selecting storage authority, public-ID authorization, private mappings and retention/reference policy, implement create/read/list/delete and content access | Access isolation, bounded streaming/spooling, deletion/expiry with active references and restart-consistent mappings are verified. No credential-owned upstream ID is accepted as public authority |
| S4-4b Multipart Uploads | Build on the selected file identity/lifecycle contract: create upload, add bounded parts, complete ordered parts, cancel and clean up incomplete uploads | Part identity/order, total bytes, expiry/cancel cleanup and uncertain remote completion are handled without blind replay or whole-file memory retention |
| S4-4c File-reference integration | Connect authorized resource resolution to each selected task outside pure codecs; retain the task's existing direct-input path | Resource scope can restrict trusted targets but cannot supply origins/credentials. Selected file-backed operations work across restart and reject stale, unauthorized or incompatible references |
| S4-5 Video | Select concrete current input/output or production operations before choosing task/content and optional native-job owners. Implement their codecs, execution and artifact access | Independent request/result and applicable event/job evidence covers bounds, cancellation, artifact identity and honest completion. Async APIs explicitly handle submission uncertainty/status/cancellation; only file-reference paths depend on S4-4c |

Files design can proceed alongside the first operation slices; implementation need not delay direct-input media. Speech, image and file implementations may advance independently once their own contracts are selected, but shared-owner changes must stay coordinated. Native video/media job lifecycle belongs to that operation, not the general Batch API.

Optional Chat-carried audio is a separate slice after its task contract stabilizes: preserve the existing static path, then verify genuine upstream streaming if selected. It neither blocks the main audio path nor establishes Realtime. For every operation, update the OpenAPI/usage surface only when implemented, keep admission and actual Provider evidence separate, and use the [stage-specific verification](api-expansion-design.md#verification).

#### Next stage-4 slice

**Recommendation from the 2026-10-23 source/code review: close the static Generate public contract before widening Images.** The later [Decision design priority](#decision-api-design) precedes this modality review; it does not select an Image dialect, IR structure or binding. Source detail belongs to the [dated recheck](refs/2026-10-22-embedding-image-contracts.md#static-generate-source-recheck-2026-10-23); the [review proposal](api-expansion-design.md#static-generate-review-proposal) owns the alternatives and minimal contract.

| Candidate | Actual readiness | Consequence for the next slice |
|---|---|---|
| Existing text Embedding | Selected numeric delivery, native Google/source reports and scoped OpenAI-model tokens passed offline, SDK and bounded Provider acceptance | Preserve the completed contract; new modalities, contextual output and native Google ingress need separate selection |
| Static Images Generate | Recent official/pinned-SDK notes, existing bounded task/executor and pure Grok codec. Missing creation time, PNG-default mismatch and native reports are concrete delivery blockers | Highest-value next review: choose one public contract and effective-default/result rules, without waiting for Edit or Files |
| Direct-input Edit and image streaming | Substantial source research, but new source/mask ownership, finite input budgets and preview/final semantics remain unselected | Follow static closure under separate decisions; do not make the whole image family a prerequisite for Generate |
| Speech, then file transcription/translation | Existing static paths, but Audio has no equally detailed pinned operation/model review. Progressive delivery, complete alignment and native multipart are not established by those paths | When selected, pin one static Speech profile first; do not start by promising all Audio formats or stream modes |
| Files and Uploads | Safety requirements are written; storage authority, public-ID authorization, durable private mappings and retention remain unselected | Early authority review is useful, but complete Files implementation is not a dependency of direct-input media |
| Video | Dated Grok/MiniMax protocol research exists; public job contract and persistence are unselected, and integration is explicitly deferred | Keep deferred; a well-documented upstream job API is not a ready gateway contract |

The next review deliverable is deliberately small:

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

**Scope revised on 2026-10-22: static image codec now, video integration deferred.** The pure `grok-imagine-image-2.0` projection/response codec exists; no Grok image gateway binding or live generation acceptance is established. Do not add the retiring `grok-imagine-image-quality`. The user superseded the synchronous-video choice: later video integration should use Grok's native asynchronous API as its primary design reference, not a synchronous wrapper or assumed OpenAI video compatibility. See the [accepted decisions](architecture.md#scope-and-activation-decisions) and [dated sources](refs/2026-10-22-grok-imagine.md).

Remaining image gates:

1. **Public contract and broader controls.** The codec maps prompt/count, user and `low`/`medium`/`auto` quality with static Base64 results. Exact pixel sizes, encoding guarantees, background/compression/moderation controls and streaming reject. The prioritized [Image design](api-expansion-design.md#image-results-and-delivery-compatibility) now reviews typed native ratio/tier controls, missing `created`, serving-model/accounting carriers and the OpenAI-default-PNG mismatch. No delivery option is approved yet; a native codec is not complete OpenAI Images compatibility.
2. **OAuth and binding.** Add an explicit model binding and remove the image path's API-key-only assumption at the responsible layers without weakening Provider/profile checks. Use the existing single-account Grok OAuth pool, with no inference-triggered login/refresh or fallback.
3. **Acceptance.** Preserve independent request/response codec expectations and strict JSON, bounds, accounting and terminal-honesty checks. Before any live run fix target, count, output cap and timeout; call serially through the project loader with no hidden retries and redacted reporting. Model enumeration does not establish OAuth image entitlement.

The implemented Grok slice still excludes editing/deferred jobs/preview streams, Files references, caller-directed upload destinations, cross-Provider failover and automatic media sweeps. Broader Generate/Edit/streaming design now belongs to prioritized S4-3; that design does not widen the implemented Grok profile. Other media paths stay maintenance-only.

**Deferred video:** retain `grok-imagine-video-1.5` and `grok-imagine-video-1.5-lite` as targets, with lite as the future default test model. Do not add endpoints, bindings, codecs, polling executors or synchronous facades now. On resumption select native-style submission/status/artifact contracts, public task identity, authorization, persistence/expiry, bounded polling, cancellation limitations and uncertain-submission handling. A later minimal acceptance proposal is one text-only 1-second 480p lite generation; it is not a scheduled call or proof of the full model's account access.

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

Scope the initial case inventory to official Chat, Responses and Messages contracts, selected upstream dialects and existing Google upstream text directions, including accepted owner/mapping corrections. Preserve valid media and safety assertions, but do not activate standalone modalities, file services or Google ingress. This is a finite declared-contract gate, not every upstream test or every official feature.

Establish source provenance first; protocol, multi-turn and SSE slices can then proceed independently at their existing owners:

| Slice | Exit evidence |
|---|---|
| Source links and latest-version tracking | Reproducible latest stable release/source pins, provenance and visible unresolved source deltas; main-branch research is supplementary. No old baseline fallback or network dependency in ordinary offline runs |
| Official SDK and Vercel integration | Reused fixtures exercise the redesigned codecs with independent directional expectations and actual SDK consumption, including new API versions and actual-loss extensions; generic compatibility is not dialect coverage |
| Multi-turn and complex tools | Reuse existing session/history coverage; add missing combinations such as interleaved multi-call streaming through Router and SDK, reverse-order results and complete next-request verification. Fixed tool results suffice; no gateway executor |
| SSE supplementation | Framing, protocol-event and transport-lifecycle checks remain distinct; reused failure cases plus reproducible fragmentation/mutation checks protect bounds, cancellation, EOF, backpressure and publication safety |

Each slice must distinguish exact preservation, approved adaptation, correct rejection, incomplete outcomes, unassessed cases and defects. Passing upstream-library tests alone is not codec/IR evidence. Apply the normal offline and connected SDK gates before promoting source revisions; installed-client and real Provider acceptance stay separate.

**Exit before further stage-4 implementation:** all four slices above run against a recorded verified baseline, selected cases exercise MorphieCore and retained regressions pass the full offline baseline plus applicable SDK loops. Keep case discovery, expected outcomes, unsupported directions and latest-candidate differences visible. Importer failures, silently skipped selected cases and defects in claimed support block closure; excluded upstream features remain explicit exclusions, not new implementation obligations. An on-demand latest-candidate check is sufficient initially; recurring automation is not a separate blocker.

### Stage-4 reuse and acceptance

Use the same pinned sources, update mechanism and thin adapters rather than another multimodal harness. Apply these conditions per selected operation, after its design approval:

1. Identify the applicable official SDK/Vercel cases, source revisions and local entry points for that operation and target. Inspect operation-specific assets; text tests and a whole-repository checkout do not establish image, audio, file or video coverage.
2. Run selected cases through MorphieCore request preparation, codecs and delivery, with independent wire-to-IR/IR-to-wire expectations and actual SDK consumption where applicable. Exact preservation, approved adaptation and intended rejection need explicit assertions. Where no suitable upstream case exists, record the gap and retain or add a focused, source-backed local oracle; missing external coverage is not a passing skip.
3. Keep the current text/SSE baseline green and add the operation's resource, lifecycle and safety checks. SSE reuse applies only to SSE paths; static, binary, multipart and asynchronous-job contracts need their own evidence. External tests supplement, not replace, the offline baseline and applicable SDK/real acceptance gates.

Static Images Generate is the first planned modality application after its separate review: select image-model/API fixtures for controls, effective defaults, result encoding/counts and reported-field presence. Edit/mask/multipart and preview/final streams follow their own selected slices; do not treat static Generate or conversational image inputs as their acceptance. Apply the same per-operation rule to later Embedding extensions, Audio, Files/Uploads and Video without activating or rescheduling them.

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

**Direction selected on 2026-10-20; minimal production work selected on 2026-10-21 and accepted on 2026-10-22.** Follow [Observability](architecture.md#observability). The completed [production closure](#generation-text-production-closure) adds only signals and bounded diagnostics maintenance/checks needed for concrete acceptance. Keep future investigation at those existing owners; broader instrumentation remains deferred, not a full programme or another stage-1/2 approval gate.

Protocol, security and lifecycle defects retain their own gates. Use established libraries where needed. The later [Agent authorization and diagnostics baseline](#agent-authorization-and-diagnostics-follow-up) governs future cluster work; no telemetry backend deployment or content export is activated.

### Agent authorization and diagnostics follow-up

**Planning baseline selected on 2026-10-23; implementation and deployment remain unscheduled.** [Architecture](architecture.md#agent-identity-and-cluster-diagnostics) owns identity, shared-history and usage semantics; [exchange capture](architecture.md#configurable-exchange-capture) owns sensitive diagnostics. This does not change the five-stage API sequence, current loopback ingress or the active modality slice.

When explicitly scheduled, use the following dependency order; each slice needs its own normal offline and applicable integration gates:

1. **Authentication binding.** Select registry persistence and credential lifecycle; prove two registered clients remain distinct through API-key rotation, OAuth refresh and independent revocation. Verify a real SDK against the selected authorization server, trusted audience/permissions and a stable Agent binding. Do not accept successful login alone as identity acceptance.
2. **Request and history attribution.** Carry one authenticated context through request/attempt completion. Verify restart/rename identity stability, creator versus caller attribution, shared-session handoff, and unchanged Provider replay restrictions. Select only the necessary history persistence and correlation carriers.
3. **Usage and diagnostics.** Integrate established telemetry components with the existing responsible owners. Verify per-Agent aggregation across attempts, cumulative usage, estimates, missing reports and cancellation; expose recording loss. Verify sensitive capture, mandatory credential redaction, bounded streaming, collector/storage failure and honest completeness.
4. **Cluster activation.** Select HTTPS/trusted-proxy topology, explicit collector destinations, retention/capacity and revocation windows/in-flight behavior. Run bounded two-Agent approval → inference → refresh → handoff → revoke acceptance. Deployment still requires explicit authorization.

Do not prebuild multi-user IAM, tenant ACLs, automatic sub-Agent delegation, financial billing or strict quota settlement. Authorization-server choice and the remaining [integration decisions](api-expansion-design.md#centralized-agent-authorization) are not resolved by this planning baseline.

### Temporary routing freeze

The [stable routing baseline](architecture.md#failover) is the selected Generation contract. Codec work must not change membership, stable selection, health scopes, affinity, budgets, credentials or public model routing. Do not rank targets by projection loss or add a fallback to hide an unsupported conversion. The selected [two-file configuration](usage.md#model-configuration) is a narrow exception for explicitly configured existing bindings and their deployment/policy parameters.

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

- Google structured output and logprobs, and Messages request-wide automatic TTL controls. Automatic TTL now has a [selected activation slice](#generation-text-production-closure), not implemented public admission. Messages owner-local markers, strict tools, ordered instructions and tool changes have the narrow [all-native configured admission](architecture.md#owner-local-cache-admission) on the Go/Haiku binding; mixed routes and the generic profile remain closed for those features. Exact beta carriers for per-message effort, block binding and tool changes are separately enabled by trusted binding policy. Missing carriers use the selected named projections where applicable; explicit block-binding `"error"` still rejects without a carrier.
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

The [selected actual-report contract](architecture.md#public-capability-and-adaptation-notice) supersedes the implemented internal-only receipts and possible-adaptation header. Capability declarations still do not establish model-effective support or full-loop availability. An acceptance boolean cannot distinguish exact support from omission, and streamed usage may introduce losses after headers.

1. Establish model-level evidence and preflight/preparation agreement for each selected existing binding; reject uncalibrated values and stop at interfaces needing routing-policy changes.
2. Extend the capability view only with independently verified declarations. Do not activate bindings implicitly or treat an unlisted case as rejection.
3. Reconnect exact-carrier client loops first, then approved adapted paths. Verify tool calls, results, final answers and replay for JSON and SSE. Blocking safety reports and unsupported completion carriers remain rejected.
   Verify the redesigned legitimate phase/replay carriers and actual-loss extension through selected release-pinned consumers; raw-event parsing does not prove final-object or next-request retention. Before enabling extended-retention omission, review target storage policy without treating cache presence as retention evidence.
4. Require the full offline baseline and connected SDK gate per slice, then schedule bounded pi/opencode and Provider acceptance.

### Controlled routing acceptance

**Status: selected controlled scope passed on 2026-10-20; real Provider failover remains separately scheduled.** Admission and routing policy are unchanged.

The 2026-10-20 S2 continuation selects controlled synthetic recovery for the already accepted R1a DashScope Chat and R2 native Go/Haiku Messages profiles. That earlier selection alone did not resume R1b, close the overall human design review, activate bindings or change routing policy.

The [Router and SDK recovery checks](development.md#sdk-loopback-gate) passed on 2026-10-20, alongside the full offline baseline and connected synthetic SDK gate. They cover JSON/SSE tool feedback moving from the primary to a secondary managed key, honest primary recovery in another conversation and sticky continuation on the secondary. Each delivery uses four client requests/five upstream attempts for Chat and six/seven for Messages, with zero SDK retries; independent upstream expectations validate failed and successful tool-result attempts. Native session identity and signed/redacted history are checked through migration. Separate selected-profile HTTP checks create both cooldowns from upstream replies, then require a sanitized local `503`, the remaining `Retry-After` window and zero further dispatches. Existing executor and publication gates retain exclusive recovery, generation fences, cancellation, strict EOF and no replay after publication.

The separate [installed-client gate](development.md#installed-client-synthetic-recovery) passed with pi 1.3.0 Chat/Messages and OpenCode 2.0.6+e5ecb57 Chat, using the CLI project loader, a test-owned managed-key pool and synthetic upstream. Each case observed four client requests/five upstream attempts, no extra client transmissions, successful `read`/final/next history, an independent primary recovery probe and sticky continuation on the secondary. Client-reported session IDs matched ingress carriers; native session and exact signed/redacted history survived the synthetic migration. Client completion and final text were checked independently of upstream success. Diagnostics reported zero unclassified ignored request fields. Full offline, SDK and installed-client gates passed separately.

This evidence accepts only the selected controlled routing slice. Responses uses its separate evidence above; the [overall baseline approval](#generation-design-review-gate) does not broaden either acceptance. Serial synthetic migration does not establish real account-compatible replay, concurrent network recovery, other bindings or Provider failure/cache behavior. [Real failover](#deferred-acceptance) needs its own bounded run; missing cache usage remains unknown, not a miss. No deployment, routing feature or credential-lifecycle change is authorized by these results.

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
| [API expansion](api-expansion-design.md) | Stage 3: cross-protocol stability, with counting deferred. Stage 4: Embedding, audio/image/video and Files/multipart Uploads. Stage 5: server-side compaction, Batch and Realtime, all unscheduled | Each operation's own contract and activation |
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
| Pinned-client image replay | Conditional | If the installed binary differs from the pinned source, run the bounded image tool round trip with a binary built from that revision |
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
