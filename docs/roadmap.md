# Roadmap

This page owns current scope, priorities, dependencies, and exit gates. [Architecture](architecture.md) owns accepted design, [API expansion design](api-expansion-design.md) owns unselected proposals, and [Development](development.md) owns verification procedures. Code and tests define behavior. Completed slices are removed; Git preserves history.

## Contents

- [Current status](#current-status)
- [Standard request boundary repair](#standard-request-boundary-repair)
- [Three-protocol fidelity repair](#three-protocol-fidelity-repair)
  - [Client/IR audit follow-up](#clientir-audit-follow-up)
  - [Agent-default production findings](#agent-default-production-findings)
  - [Repair order and exit gates](#repair-order-and-exit-gates)
  - [Current Client/IR blocking semantics](#current-clientir-blocking-semantics)
  - [Remaining issue dispositions](#remaining-issue-dispositions)
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

**Current focus (2026-10-28):** The [three-protocol fidelity repair](#three-protocol-fidelity-repair) is reopened as Client/IR stabilization. The [agent-default production findings](#agent-default-production-findings) show installed pi 1.4.0 default requests failing on several same-protocol lanes and most cross-protocol lanes, while existing gates pass only curated client configurations. Build the production-construction conformance gate first, then follow the [repair order](#repair-order-and-exit-gates). The next stage starts after the [closure boundary](#closure-boundary). Ordered Messages delivery, incomplete text-owner closure and null-only metadata omission remain completed baselines. Budget adjustment stays deferred. Stabilization does not activate new capabilities or bindings, change equal priority among Stage-4 families, or schedule Stage 5.

**Completed baseline:** Generation Text correctness/lifecycle reviews, domain evidence, telemetry and standard request boundary repairs preceded the 2026-10-27 parsing/stream corrections. Single Messages signature validation, narrow Chat text/token-cap status projection and original-request Responses metadata echo are implemented. Their offline, SDK and installed-client evidence is separate from live acceptance; see [verification gates](development.md#verification-gates). Stage 4 retains equal priority across its API families.

| Area | State | Next step |
|---|---|---|
| Routing | Stable routing baseline frozen; controlled synthetic recovery verified across Router, SDK, and installed clients | Real-failover production evidence required for multi-credential routes; no routing feature additions |
| Generation IR / Codec | Fidelity baseline retained; Client/IR stabilization reopened with confirmed same-protocol and cross-protocol gaps | Production-construction conformance gate, then the ordered repairs; no broad IR redesign |
| Client compatibility | Standard request intake enforced; installed-client gates use curated compat and never drive pi over Responses | Agent-default corpus and default-profile installed-agent acceptance |
| Integration | R1a/R1b/R2 loops, controlled recovery and bounded production-profile closure passed | Revalidate changed paths; retain real-failover exclusions without reopening a blanket model/client matrix |
| Cross-protocol conversion | Stage 3 closed for selected projections; `count_tokens` deferred | Preserve verified evidence; Google API remains paused except text-report and Stage-4 Embedding paths |
| Decision APIs | Dual standard-SDK direction selected; initial TypeSafe/OpenAI protocol research complete | Contract selection and Stage-4 implementation at equal priority with other modalities |
| Embedding & Media | Text Embedding operational (Google native, OpenAI via OpenRouter, DashScope); Grok static image codec implemented | Stage-4 contract selection for Images Generate/Edit, Audio, Files/Uploads, and Video |
| Downstream integration | Agent API key authentication and minimal diagnostic attribution implemented | Design per-Agent JSONL session storage; OAuth/managed registration remain deferred |
| Test baseline | Pinned upstream test baseline closed; strict SDK accumulators and transport safety verified | Maintain offline baseline; evaluate candidate updates separately |

## Standard request boundary repair

The five-slice request correction was completed on 2026-10-26. [Architecture](architecture.md#standard-request-boundary) defines the standard-priority contracts, and [dated findings](refs/2026-10-26-standard-request-boundaries.md#repair-disposition) detail specific resolutions and exclusions. Any further protocol-boundary refinements proceed through the [three-protocol fidelity repair](#three-protocol-fidelity-repair).

## Three-protocol fidelity repair

Selected on 2026-10-27, this workstream repairs semantic fidelity across standard Chat Completions, Responses, and Messages. The 2026-10-28 audit at `5b04a03` reopens the scoped closure for the concrete follow-up below. Authoritative contracts: [semantic fidelity](architecture.md#accurate-ir-and-declared-loss), [field handling](architecture.md#field-handling), and [stream normalization](architecture.md#semantic-stream-normalization). Supporting protocol research lives in the [Client/IR notes](refs/2026-10-27-client-ir-boundaries.md); [Development](development.md#sdk-loopback-gate) separates offline/consumer verification from live acceptance.

### Scope and boundaries

- **Protocols:** Covers all three same-protocol paths and assesses six cross-protocol directions across request decoding, IR ownership, target request encoding, upstream decoding, and history replay.
- **Domain:** Generation Text (instructions/roles, text/refusal, function tools/results, structured output, reasoning/replay, presence/settings, usage, errors, and terminal events).
- **Extension properties:** Unknown protocol extension fields are accepted and discarded at legal object positions, never forwarded or given semantic execution meaning. Known unsupported controls reject.
- **Precedence:** Plain-text token-cap output may omit item status under named rules; Responses metadata echoes the admitted request only when upstream metadata is omitted.
- **Budget direction:** Bounded relaxation may preserve functionality when gateway-derived budgets conflict. The [accepted boundary](architecture.md#bounded-token-budget-adaptation) does not select a formula, relax source grammar, or raise model limits.

### Reference strategy

| Reference family | Target contracts |
|---|---|
| OpenAI and Anthropic official specs & SDKs (Python / TypeScript) | Request/history unions, presence vs null defaults, tool arguments, signatures, stream accumulation, and terminal events |
| WHATWG HTML EventSource & RFC 8259 | SSE framing, multi-line events, UTF-8/JSON boundaries, EOF, and resource bounds |
| LiteLLM & CLIProxyAPI | OpenAI/Anthropic bridge mechanics, output-index mapping, block scheduling, and usage provenance |
| Vercel AI SDK & OpenAI Agents SDK | Typed stream consumers, incremental tool assembly, and history materialization |
| Installed pi and OpenCode | Real-world CLI/TUI serialization, SDK event consumption, and signature replay |

### Client/IR audit follow-up

The audit covers Generation IR, its stream state and associated codec/delivery envelopes, not every standalone modality IR. Read/write tracing and bounded offline probes identified two delivery gaps; neither requires a duplicate semantic fact:

- **Chat JSON grouping defect:** one text part plus a custom call succeeds, and two text parts plus a function call succeed, but two text parts plus a custom call reject. The [grouping path](../src/protocol/openai/lowering/native_response.rs) recognizes only function calls in both pending-call progress and call preservation. Extend the already selected plain-text grouping without opening custom-tool SSE.
- **Responses-to-Messages terminal restriction:** ordinary completed Responses text with `metadata:null` still rejects through actual Attempt/ResponseDelivery with `Unsupported(Terminal)`; explicit `max_output_tokens` incomplete text succeeds. [Messages terminal encoding](../src/protocol/anthropic/messages/terminal.rs) needs a supported completion cause/progress combination. Null-only metadata omission does not supply it. A bounded terminal inference needs separate selection and must leave the source's unreported cause unchanged.

The structural findings support local cleanup, not deletion of large parts of the semantic union:

| Candidate | Finding | Disposition |
|---|---|---|
| `StreamEvent::Usage` / `UsageReport` | Two event representations feed the same report owner. [Reduction](../src/semantic/task/generation/stream/reduce.rs) rejects checkpoint → flat final `Usage`, but accepts checkpoint → final operation `UsageReport` | Prefer a canonical report event after resolving the transition contract; preserve protocol-specific rejection and resource accounting |
| Stream result-header `ToolResult.status` / `StreamItem.status` | The [snapshot](../src/semantic/task/generation/stream/snapshot.rs) replaces the embedded initial status with the outer lifecycle | Low-priority stream-header cleanup only if it removes complexity; preserve opening validation and materialized/history result status |
| `GenerationDelivery.settings` | An entire `GenerationSettings` is retained, but [construction](../src/execution/delivery/completion.rs) and [Responses completion](../src/protocol/openai/responses/envelope.rs) consume only tools, tool choice and parallel permission. A manually supplied valid `top_k` causes an irrelevant rendering rejection; production construction avoids it | Narrow the delivery container; this is envelope overbreadth, not duplicate core IR payload |
| Unit/null `ExecutionHints` placeholders | `previous_response_id`, `conversation`, `prompt`, `moderation` and `context_management` currently retain bounded presence/inactive values rather than full active operations | Ownership review only, not dead-field deletion. Any relocation must preserve absent/null/empty distinctions and known-active-field rejection |

Retain independently meaningful owners: root versus ordered instructions; requested, reported and delivery-completed values; response outcome, item lifecycle and interaction progress; value versus part completion; readable reasoning, opaque replay and dependency provenance; sparse usage totals and components. Public Responses storage facts also remain distinct from upstream execution intent. Unsupported client carriage alone is not evidence that an official-union field is redundant.

### Agent-default production findings

The [2026-10-28 probe](refs/2026-10-27-client-ir-boundaries.md#agent-default-production-probe-2026-10-28) drove installed pi 1.4.0 default request shapes and official-shape upstream streams through production startup construction over catalog bindings. Ids refer to its findings table.

| Lane class | Confirmed gaps | Disposition |
|---|---|---|
| Same-protocol Responses | A: delivered incomplete reasoning rejects as history. Q1: Bailian Responses rejects pi's default reasoning summary, encrypted include and omitted strictness. Q2: it rejects its own delivered reasoning in the next turn | Order 1 under [S3/S4](#clientir-stabilization-decisions) for Q1, S5 and S11 for Q2 and A |
| Same-protocol Chat | Standard profile fails after dispatch when the upstream emits reasoning (R); `readable_reasoning` passes. Standard trailing usage after finish-chunk usage reduces as a duplicate final (U) | R under S6 and S11; U in order 1 |
| Same-protocol Messages | pi's bundled SDK version and beta header reject before body collection (M2); manual thinking is unavailable on the Haiku binding; adaptive signed history passes | Retained under S8; binding facts belong in discovery (S10) |
| Into Messages clients | E (`logprobs:[]`) and F (unreported completion cause) fail every ordinary text answer from Chat/Responses upstreams after dispatch | Orders 2 and 4 |
| Messages upstream into OpenAI clients | B/C enum catch-alls; D succeeds in SSE but fails JSON/retained | Orders 3 and 4 |
| Responses/Chat clients into other protocols | G (omitted strictness), H (reasoning summary/encrypted include), effort scope without configured conversion, explicit image detail | Order 5 under S3/S4 |
| Messages clients into other protocols | M1: pi's default owner-local `cache_control` rejects on every non-Messages route | Order 5 under S7 |

Root cause: consumers default-reject IR states from producers they are never tested with, and current gates exercise neither agent default shapes nor production client construction off the diagonal. Response-side impossibilities also surface only after dispatch, so one reachable upstream state fails a whole lane.

### Repair order and exit gates

Use existing startup fixtures, session corpora, delivery and diagnostics helpers; no new framework or broad compatibility layer. Mapping correctness keeps independent wire→IR and IR→wire expectations; the conformance gate classifies reachability and dispositions only. Each slice implements only its accepted [decision](#clientir-stabilization-decisions); deferred capabilities stay unselected.

| Order | Work | Exit gate |
|---|---|---|
| 0 | Production-construction conformance gate: startup-built gateways over catalog bindings with loopback `base_url`; an agent-default request corpus (pi 1.4.0 shapes per API, taken from the installed source); official-shape upstream streams with every required member; two-turn replay in each client's own replay style. Extend the session corpus loop to production client construction and off-diagonal pairs | Each lane outcome equals acceptance, a named rule or a documented rejection by diagnostic failure code; unexpected outcomes fail. JSON, SSE and retained views agree. Delivered output re-decodes as history for the same route. The disposition list is reviewed with each change |
| 1 | Same-protocol defects: binding reasoning-history contract (S11), A and Q2 (S5), Q1 (S3/S4), R planning rejection (S6), U | Permanent per-lane regressions from the probe counterexamples; same-issuer replay controls stay exact |
| 2 | Presence/empty agreement: E in delivery and admission, plus one audit of `is_absent`/`is_empty` guards against producer behavior | `logprobs:[]` passes Messages delivery and history admission without a loss entry; nonempty logprobs still reject |
| 3 | Delivery agreement and grouping: Chat JSON custom-call grouping, D, details-only usage error class (U) | Grouping: one-part/custom and multipart/function controls pass; multipart/custom preserves combined text and unchanged call identity/input; custom SSE and rich/replay negatives unchanged. One input yields one disposition across JSON, SSE and retained views |
| 4 | Messages client terminals and enums: F for Chat and Responses sources, B, C | Accepted S1/S9 rules with independent JSON/SSE/history expectations through actual delivery; source causes stay unreported and EOF honesty holds |
| 5 | Agent-default cross-protocol rules: G and H (S3/S4), Standard Chat omission (S6), M1 (S7); publish image-detail and effort-scope facts | Each accepted rule is named, reported and covered by the gate; cases outside the accepted rules remain documented rejections |
| 6 | Discovery extension: probes use the order-0 corpus; publish preferred interface and per-interface adaptation rules from one compiled owner | Discovery facts match order-0 dispositions for every activated interface; see [Models discovery interfaces](#models-discovery-interfaces) |
| 7 | Structural cleanup: narrow `GenerationDelivery.settings` to its three consumed values; unify usage events under a checkpoint/final contract; qualify `between_tools`, nullable output config and explicit disabled citations | Preserve report precedence, required-field completion, bounds and rejection; usage covers checkpoint → final, duplicate final, scoped reports and sparse counts; exact IR or a named projection precedes each capability change |
| 8 | Installed-agent default acceptance: pi 1.4.0 over Responses, Chat and Messages without compat overrides beyond documented header opt-ins, offline scripted upstreams; OpenCode afterwards | Tool, reasoning and history loops pass on same-protocol lanes and selected cross-protocol lanes; actual agent versions recorded |

Orders 0–2 precede the rest; 3 and 4 may run alongside once order 0 exists. Stream-header status and placeholder relocation remain optional reviews. Each implementation slice needs the [offline baseline](development.md#offline-baseline), affected [SDK consumers](development.md#sdk-loopback-gate) and applicable installed-agent checks. Real Provider gates apply only when live behavior is claimed; Q1, Q2 and U each need one bounded real call to confirm the upstream shape before their rules are finalized.

### Current Client/IR blocking semantics

These are Client intake/history or downstream delivery restrictions, not a catalog of Provider capability failures. Unknown extension fields are discarded; malformed known values remain grammar errors. The principal active boundaries are:

| Boundary | Remaining blocking semantics |
|---|---|
| Chat → IR | Multi-result `n > 1`, tokenizer-unbound `logit_bias`, `web_search_options`, nonempty legacy `functions`, `function_call` and `role:function`; custom tools are not blanket-rejected |
| Responses → IR | Unselected hosted tools/items and `include` families, background execution, active conversation/prompt/moderation and context-management operations. Public local `store`/`previous_response_id` are supported through [managed ingress](../src/gateway/ingress/admission.rs), not blanket blockers |
| Messages → IR | `between_tools` lacks an exact admitted mode; nullable output effort/format, task budgets, per-message configuration, block binding/tool changes and citation controls reject. Direct ordinary tools remain distinct from hosted/programmatic callers. Execution extensions and zero-token prewarming are unselected; removed sampling controls still reject |
| Resources and protected history → IR | File-ID carriers need trusted resource resolution; supported URL/data media remain available. Raw external reasoning signatures/tokens may pass syntax but fail [replay opening](../src/execution/replay.rs); gateway-sealed history is the supported continuation path |
| IR → Chat | Standard reasoning has no carrier without an explicitly selected profile. Custom calls and some annotations reject in SSE; metadata maps lack a chunk carrier. Independent lifecycle and rich grouping have limits; the JSON custom-grouping defect is order 3 above |
| IR → Responses | Incompatible replay formats/bindings, unrepresentable reported settings and some annotation SSE carriers reject. JSON cancellation does not imply an available cancellation SSE event |
| IR → Messages | Unsigned/multi-summary or incompatible redacted reasoning; custom string-input calls, non-object tool input, unrepresentable call context, annotations/logprobs/generated media, metadata maps and other unsupported execution hints. Normal terminal meaning, continuation, service-class and already-published usage constraints remain; partial structured tool input has only the selected max-token SSE path |

The [pinned Anthropic SDK](../tests/upstream/sources.lock.json) types already include `between_tools`, optional-nullable output configuration and zero-token cache prewarming; these rejections are capability/semantic-selection gaps, not all invalid source syntax. Offline contrasts also confirmed that an otherwise admitted text document rejects explicit `citations:{"enabled":false}`. Review null/default equivalence before normalization. Full citation delivery and prewarming are not activated by reviewing these narrow cases.

Do not re-list repaired owner ordering, incomplete text closure or null-only metadata omission as blockers. Nonempty Messages tool openers followed by nonempty fragments concern native upstream SSE intake, not Client request intake. Missing scalar usage alone is not a general runtime blocker where selected delivery completion applies.

### Remaining issue dispositions

The [2026-10-28 source and feasibility recheck](refs/2026-10-27-client-ir-boundaries.md#follow-up-source-and-feasibility-review-2026-10-28) supports these retained dispositions. The two active delivery gaps and structural follow-up are owned by the [repair order](#repair-order-and-exit-gates), not deferred by this table.

| Issue | Current disposition | Reopen or complete when |
|---|---|---|
| Chat custom-tool SSE | Retain standard rejection; investigate upstream intake and downstream delivery separately | Official event contract resolves the disagreement, or a separately selected dialect/client contract supplies independent wire/consumer evidence |
| Nonempty Messages tool opener plus nonempty fragments | Retain rejection, including a complete-fragment negative regression; complete opener plus zero-byte deltas remains supported | Authoritative replacement/merge semantics exist |
| Responses execution metadata to Messages | Null has a selected omission; empty/nonempty maps and other unsupported execution hints still reject | An exact standard carrier or separately selected field-specific loss exists |
| Bounded token-budget adjustment | Deferred: catalog/example review identified no qualified active conflict; preserve native effort and existing validation | An authorized target plus failing request, limit provenance, useful reserve, finite maximum and infeasible behavior qualify a target-local rule; private deployment activation was not inspected |
| `user:null`, Chat refusal requiredness, Responses image-detail default | Preserve current intake; optional egress canonicalization is not an established defect | Service contract clarifies the discrepancy or a selected strict consumer exposes a concrete failure |
| Legacy function carriers | Defer; preserve explicit rejection | A real client needs a constrained association/default/conflict/history contract |
| Broader settings echo and richer output grouping | Case-specific design only | A concrete failure has an exact mapping or a separately selected named loss; preserve report authority and content-coordinate ownership |
| Messages custom-input wrapping | Defer as an end-to-end tool-interface adaptation | Declaration, arguments, return/history mapping and client execution expectations are selected together |
| Token scope and SDK drift | Investigate only the selected target/field; no automatic source-pin promotion | A bounded budget rule or reviewed version change has independent expectations and a reachable target path |

### Closure boundary

Reclose Client/IR stabilization when order 0 is permanent and passes with no unexpected disposition for the pi default corpus on every same-protocol lane and each selected cross-protocol lane; orders 1–5 have verified changes or reasoned retained rejections; discovery publishes the same facts; and order 8 passes. Selected paths need permanent directional regressions and applicable consumer gates; old passing suites do not cover the newly reproduced contrasts. Remaining capability gaps stay explicit with reopening conditions. Do not claim full standard-field support or new live Provider behavior. Resource APIs, session persistence and real multi-account failover keep their own packages; budget deferral does not reopen Google expansion or require a universal multi-budget model.

## Standards reset and redesign

Accepted on 2026-10-23, this redesign reset protocol dependencies and upstream references to current stable baselines. [Architecture](architecture.md#protocols) owns the resulting contracts; [Development](development.md#upstream-test-sources) owns procedures.

- **Baseline:** Pinned upstream release sources, reviewed fixtures, and local extraction recipes. Standard delivery uses public protocol dialects, native-only replay, and typed Responses history.
- **Scoped closure (2026-10-24):** Offline baseline, 16 SDK scenarios, and 12 current-host synthetic Agent scenarios pass. Real `pi` 1.4.0 and `opencode` 2.0.32 verified across 3 consecutive tool/reasoning attempts.
- **Diagnostic-extension deferral:** Safe-to-ignore diagnostic extensions (such as routing and loss receipts) are permitted without dedicated extension tests. Organization, schema finalization, and testing are deferred to deployer production packaging.
- **Instruction mapping (2026-10-25):** Canonical IR maps standard and deprecated roles to native target carriers, verified across offline, SDK, and current-host gates.

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
4. **Evidence:** Provide independent directional tests and focused failure assertions at the responsible layer. Separate exact carriage, approved loss, and rejection.
5. **Exit:** Run the offline baseline and relevant SDK gates before commit and push. Remove completed roadmap packages and record decisions in [Architecture](architecture.md).

### Stage 1 work plan

**Status: Closed.** Replaced by the standards-reset baseline. Chat, Responses, and Messages maintain independent directional expectations within the [contract inventory](refs/2026-10-23-stable-protocol-contracts.md). Google generation has no downstream client pair.

### Stage 2 work plan

**Status: Closed.** Verified client interaction loops:
- Chat tool/reasoning history ([R1a gate](development.md#sdk-loopback-gate)).
- Responses native history and lifecycle reports ([R1b gate](development.md#sdk-loopback-gate)).
- Native Messages signed history and cache controls ([R2 gate](development.md#sdk-loopback-gate)).

### Stage 3 work plan

**Status: Closed for Generation Text projections; token counting deferred.** Implemented contracts: [model-effective guards](architecture.md#model-effective-values), [trusted missing-cap defaults](architecture.md#missing-output-ceilings), [attribution](architecture.md#end-user-attribution), [foreground Google intake](architecture.md#google-foreground-intake), and [Messages terminal projection](architecture.md#google-messages-terminal-projection).

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
| **S4-0 Per-operation design** | Pin official contracts, select targets, and define resource lifecycles | Explicit contract decisions without speculative shared abstractions |
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
- **Tools & schemas:** Tool identity, schema constraints, and strictness flags are preserved without silent relaxation.
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
1. Distinguish exact support, declared lossy conversion, and explicit rejection.
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

The user accepted principles P1–P5 and selections S1–S11 on 2026-10-28. [Architecture](architecture.md#clientir-projection-principles) owns their content; this table maps scheduling ids to owners and orders.

| Id | Accepted rule | Owner | Order |
|---|---|---|---|
| S1 | Messages terminal derivation | [Google Messages terminal projection](architecture.md#google-messages-terminal-projection) | 4 |
| S2 | Empty logprobs omission | [Projection principles](architecture.md#clientir-projection-principles) | 2 |
| S3 | Responses strictness resolution | [Projection principles](architecture.md#clientir-projection-principles) | 1, 5 |
| S4 | Reasoning presentation and continuity | [Projection principles](architecture.md#clientir-projection-principles) | 1, 5 |
| S5 | Generalized visible-history transfer | [Reasoning replay](architecture.md#reasoning-replay) | 1 |
| S6 | Standard Chat reasoning omission | [Chat reasoning extensions](architecture.md#chat-reasoning-extensions) | 1, 5 |
| S7 | Owner-local cache omission | [Owner-local cache admission](architecture.md#owner-local-cache-admission) | 5 |
| S8 | Messages baseline retained | [Projection principles](architecture.md#clientir-projection-principles) | — |
| S9 | Carrier-less report omission | [Projection principles](architecture.md#clientir-projection-principles) | 4 |
| S10 | Agent-facing discovery | [Generation Text model discovery](architecture.md#generation-text-model-discovery) | 6 |
| S11 | Binding reasoning-history contract | [Binding reasoning-history contract](architecture.md#binding-reasoning-history-contract) | 1 |

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
Generation Text model discovery is operational. Its agent-facing extension (preferred interface, adaptation rules, agent-default probe forms) is order 6 of the [repair order](#repair-order-and-exit-gates), with S10 pending. Discovery for other task families (Embeddings, Media, Decision) remains schema-only until those operations are publicly activated.

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
