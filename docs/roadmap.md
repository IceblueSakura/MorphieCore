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

**Current focus (2026-10-30):** Client/IR stabilization now follows the [conversion ladder](architecture.md#conversion-ladder) accepted on 2026-10-30: same-name, same-semantic, partial lossy, omit, and reject only when omission would misrepresent the response. The [conversion-ladder migration](#conversion-ladder-migration) replaces the former repair orders 1, 2b, 3, 5, 6 and 7 and is driven by the agent-default production gate. Under the former fail-closed rules most default reasoning lanes still reject; see [agent-default production findings](#agent-default-production-findings). Stage 4 resumes after the [closure boundary](#closure-boundary). Budget formulas stay deferred; no new binding activation, Google expansion or Stage-5 scheduling follows.

**Completed baseline:** Generation Text correctness/lifecycle reviews, domain evidence, telemetry and standard request boundary repairs preceded the 2026-10-27 parsing/stream corrections. Implemented stabilization slices include the order-0 production-construction gate, phased binding-history ownership and discovery, Standard Chat readable-reasoning omission, Bailian Responses presentation/continuity and summary-only history, conditional strictness for OpenAI and Messages targets, grouped call-ID association and the first semantic-equivalence applications. Their offline, SDK and installed-client evidence is separate from live acceptance; see [verification gates](development.md#verification-gates). Stage 4 retains equal priority across its API families.

| Area | State | Next step |
|---|---|---|
| Routing | Stable routing baseline frozen; controlled synthetic recovery verified across Router, SDK, and installed clients | Real-failover production evidence required for multi-credential routes; no routing feature additions |
| Generation IR / Codec | Conversion ladder accepted; rejection sites inventoried; dispositions still spread across admission, lowering, dialect and client encoders | Migration R0–R5: one request-side and one delivery-side disposition owner |
| Client compatibility | Gateway installed-client gates remain curated; current-pi Responses replay and Go/Haiku adaptive defaults characterized directly | Default reasoning/tool/history loops on all three client protocols after R0–R5 |
| Integration | R1a/R1b/R2 loops, controlled recovery and bounded production-profile closure passed | Revalidate changed paths; retain real-failover exclusions without reopening a blanket model/client matrix |
| Cross-protocol conversion | Stage 3 closed for selected projections; `count_tokens` deferred | Preserve verified evidence; Google API remains paused except text-report and Stage-4 Embedding paths |
| Decision APIs | Dual standard-SDK direction selected; initial TypeSafe/OpenAI protocol research complete | Contract selection and Stage-4 implementation at equal priority with other modalities |
| Embedding & Media | Text Embedding operational (Google native, OpenAI via OpenRouter, DashScope); Grok static image codec implemented | Stage-4 contract selection for Images Generate/Edit, Audio, Files/Uploads, and Video |
| Downstream integration | Agent API key authentication and minimal diagnostic attribution implemented | Design per-Agent JSONL session storage; OAuth/managed registration remain deferred |
| Test baseline | Pinned upstream test baseline closed; strict SDK accumulators and transport safety verified | Maintain offline baseline; evaluate candidate updates separately |

## Standard request boundary repair

The five-slice request correction was completed on 2026-10-26. [Architecture](architecture.md#standard-request-boundary) defines the standard-priority contracts, and [dated findings](refs/2026-10-26-standard-request-boundaries.md#repair-disposition) detail specific resolutions and exclusions. Further protocol-boundary work proceeds through the [conversion-ladder migration](#conversion-ladder-migration).

## Three-protocol fidelity repair

Selected on 2026-10-27, this workstream repairs semantic fidelity across standard Chat Completions, Responses, and Messages. Since 2026-10-30 it executes as the [conversion-ladder migration](#conversion-ladder-migration). Authoritative contracts: [conversion ladder](architecture.md#conversion-ladder), [field handling](architecture.md#field-handling), [projection principles](architecture.md#clientir-projection-principles) and [stream normalization](architecture.md#semantic-stream-normalization). Supporting research and the [rejection-site inventory](refs/2026-10-27-client-ir-boundaries.md#conversion-ladder-inventory-2026-10-30) live in the Client/IR notes; [Development](development.md#sdk-loopback-gate) separates offline/consumer verification from live acceptance.

### Scope and boundaries

- **Protocols:** Covers all three same-protocol paths and the six cross-protocol directions across request decoding, IR ownership, target request encoding, upstream decoding, delivery and history replay.
- **Domain:** Generation Text (instructions/roles, text/refusal, function tools/results, structured output, reasoning/replay, presence/settings, usage, errors, and terminal events).
- **Extension properties:** Unknown protocol extension fields are accepted and discarded at legal object positions, never forwarded or given semantic execution meaning. Known fields follow the conversion ladder; unscheduled capabilities keep their [retained rejections](#retained-rejections).
- **Precedence:** Plain-text token-cap output may omit item status under named rules; Responses metadata echoes the admitted request only when upstream metadata is omitted.
- **Budget direction:** Bounded relaxation may preserve functionality when gateway-derived budgets conflict. The [accepted boundary](architecture.md#bounded-token-budget-adaptation) selects only the model-maximum clamp; no formula, source-grammar relaxation or raised model limit is selected.

### Reference strategy

| Reference family | Target contracts |
|---|---|
| OpenAI and Anthropic official specs & SDKs (Python / TypeScript) | Request/history unions, presence vs null defaults, tool arguments, signatures, stream accumulation, and terminal events |
| WHATWG HTML EventSource & RFC 8259 | SSE framing, multi-line events, UTF-8/JSON boundaries, EOF, and resource bounds |
| LiteLLM & CLIProxyAPI | OpenAI/Anthropic bridge mechanics, output-index mapping, block scheduling, and usage provenance |
| Vercel AI SDK & OpenAI Agents SDK | Typed stream consumers, incremental tool assembly, and history materialization |
| Installed pi and OpenCode | Real-world CLI/TUI serialization, SDK event consumption, and signature replay |

### Agent-default production findings

The permanent [production-construction gate](development.md#sdk-loopback-gate) drives captured pi 1.4.0 default shapes through production startup over catalog bindings. Finding ids refer to the [2026-10-28 probe](refs/2026-10-27-client-ir-boundaries.md#agent-default-production-probe-2026-10-28). Dispositions below are read from the gate's current expectation tables, not fresh runs.

| Lane | Current gate disposition | Ladder resolution | Slice |
|---|---|---|---|
| Messages client → every non-Messages route | `admission.explicit_cache` before dispatch for text, tools and reasoning (M1) | Owner-local cache omission at public admission, lowering and Messages items (L4); then Messages delivery omissions, otherwise failures move after dispatch | R1, R2 |
| Messages client → Go/Haiku reasoning | Corpus uses bare-model manual thinking, unlike pi's resolved adaptive default; raw version/beta headers reject (M2) | Corrected corpus; manual budget → adaptive plus effort (L3); revised version policy | R0, R1, R3 |
| Chat/Responses client → GLM/Kimi Bailian Chat reasoning | `codec.unsupported.reasoning`: Medium is missing from the Chat effort table | Nearest supported level (L3) | R1 |
| Chat client → DeepSeek/Zhipu Chat tools and reasoning | `admission.client_representation` from `required_with_tools` | Deployer `readable_reasoning`; otherwise thinking-off fallback (L3) | R4 |
| Chat client → TokenPlan Chat, all profiles | `admission.client_representation` from unassessed history | Delegation | R4 |
| Chat client ← Responses upstream reasoning (OpenRouter, Bailian Responses, subscriptions) | `representation.replay_format` after dispatch | Omit uncarried opaque reasoning (L3) | R2 |
| Responses client → Bailian Responses, turn 2 | First turn dispatches; continuation rejects at phase/carrier guards | Phase omission (L4), uncarried reasoning transfer (L3) | R1, R4 |
| Responses client → Chat, Messages and Bailian | G/H: unknown strictness, `summary`/encrypted `include` presentation | Non-strict (L3); presentation omission (L4) | R1 |
| Responses client → OpenAI subscriptions | `admission.max_output_tokens`; flat tools need endpoint evidence | Ceiling omission (L4); tool shape after evidence | R1 |
| Messages upstream → OpenAI clients | D: truncated tool input succeeds in SSE, fails JSON/retained | Selected prefix-string projection, one view owner | R2 |
| Chat upstream usage | U: duplicate nonnull final reports | Qualify at the actual emitter; no generic relaxation | Outside slices |

### Conversion-ladder migration

Each slice moves existing rejection sites onto the ladder through existing owners where possible; the only new structure is the two disposition owners in [Architecture](architecture.md#conversion-ladder). Mapping correctness keeps independent wire→IR and IR→wire expectations; the production gate classifies reachability only. Every slice runs the [offline baseline](development.md#offline-baseline), affected [SDK consumers](development.md#sdk-loopback-gate) and applicable installed-agent checks, and updates the gate's expected dispositions toward acceptance. Every remaining non-accepted lane must cite an L5 reason or an invariant.

| Slice | Work | Exit gate |
|---|---|---|
| **R0 Gate and corpus** | Add pi's resolved built-in model metadata (Go/Haiku adaptive, no temperature) beside the bare-model corpus; use official non-null upstream echoes (`service_tier`, `metadata`, `background`, `store`) so delivery-side failures are visible; turn expectation tables into a burn-down toward acceptance | Gate reports current dispositions on the corrected corpus with no behavior change |
| **R1 Request-side owner** | Target disposition tables and one per-candidate projection in planning; source-level admission keeps grammar and L5 only. Migrate cache omission at all three owners, strictness (unknown compatibility, missing parameters, conditional overflow), effort (nearest level, scope conversion, budget → adaptive, effort omission where no control exists), model-maximum clamp and ceiling omission, Chat presentation/include and Bailian phase omission, and advisory controls (sampling, verbosity, seed, penalties, `top_k`, metadata and service hints, image `detail`, empty tool descriptions, documented null defaults, default parallel permission) | Messages-client requests, GLM/Kimi reasoning and Responses-client tool lanes dispatch on every reviewed route; explicit strict, media and output-constraint L5 negatives still reach their intended failure |
| **R2 Delivery-side owner** | `ClientAdapter` table, one snapshot projection for JSON/retained and one per-event SSE projection; encoders only render. Migrate uncarried reasoning omission on Standard Chat and Messages, Messages report/hint omission, `stop` with calls → `tool_use`, stop-sequence `end_turn`, empty-string arguments → `{}`, per-dialect finish-reason maps and Responses `incomplete_details:{}`, D, and JSON/SSE divergences for annotations and usage details | JSON, SSE and retained views agree for every fact; no report or metadata fact fails after dispatch |
| **R3 Messages intake** | `2023-06-01` over current grammar, removed sampling omission, ignored beta header, `block_binding` levels, tool additions/removals where the route admits tool changes; align [usage](usage.md#messages) with code | Installed pi and OpenCode default Messages requests pass intake without header opt-outs |
| **R4 Unknown facts and history** | Delegation for unassessed binding facts; `required_with_tools` fallback; visible-history transfer for every request consumer, including unsealed or foreign reasoning, foreign citations and obsolete markers | TokenPlan Chat, DeepSeek/Zhipu Chat tools and two-turn Bailian Responses lanes accept; replay trust checks unchanged |
| **R5 Cleanup and discovery** | Remove the test-only strict-complete reporting policy, obsolete `mca1.` marker handling and unreachable delivery guards; derive `/capabilities` and discovery, including the preferred interface (S10), from the disposition tables; narrow delivery settings to tools/choice/parallel permission and settle checkpoint/final usage ownership; report unsupported `conversation`/`prompt` as unsupported rather than invalid | Discovery publishes the same facts the gate observes |
| **Exit** | Current-client characterization, then default reasoning/tool/history gateway loops with installed pi on all three client protocols and selected cross-protocol lanes; OpenCode afterwards | Actual execution and replay pass. Preserve resolved built-in model metadata; only test origin/key, operational bounds and documented header opt-ins may differ |

Dependencies: R0 comes first. R1 and R2 are otherwise independent, but the Messages delivery items in R2 must land no later than R1's cache omission, so Messages clients do not move from pre-dispatch rejection to failures after billing. R3 is independent. R4 needs R1's planning owner. R5 follows R1 and R2. U stays an endpoint qualification and does not justify a generic duplicate-final relaxation.

Stream-header status cleanup and `ExecutionHints` placeholder relocation remain optional ownership reviews: preserve absent/null/empty distinctions and opening validation. Bailian long-context defaults remain a qualification question: catalog context windows are planning metadata, not input-token admission, and no truncation ceiling or budget formula is selected.

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
| Responses execution metadata to Messages | Null has a selected omission; maps and other echoed hints take L4 omission in R2 | R2 lands |
| Bounded token-budget adjustment | Model-maximum clamp selected; broader formulas deferred; native effort and existing validation preserved | An authorized target plus failing request, limit provenance, useful reserve, finite maximum and infeasible behavior qualify a target-local rule |
| `user:null`, Chat refusal requiredness, Responses image-detail default | Preserve current intake; image `detail:"auto"` takes L2 in R1 | Service contract clarifies the discrepancy or a selected strict consumer exposes a concrete failure |
| Legacy function carriers | Retained rejection | A real client needs a constrained association/default/conflict/history contract |
| Broader settings echo and richer output grouping | Case-specific ladder application | A concrete failure has an exact mapping or a bounded approximation; preserve report authority and content-coordinate ownership |
| Messages custom-input wrapping | Defer as an end-to-end tool-interface adaptation | Declaration, arguments, return/history mapping and client execution expectations are selected together |
| Token scope and SDK drift | Investigate only the selected target/field; no automatic source-pin promotion | A bounded budget rule or reviewed version change has independent expectations and a reachable target path |

### Closure boundary

Reclose Client/IR stabilization when the corrected R0 gate is permanent; every same-protocol lane and each selected cross-protocol lane accepts or cites a [retained rejection](#retained-rejections) or invariant; R1–R5 each have verified completion; discovery publishes the same facts; and the exit loops pass. A lane cannot close by keeping an old expected rejection that the ladder resolves.

On 2026-10-29 the user retained the requirement for default reasoning/tool/history gateway success on **all three client protocols**. Retained-rejection controls and explicitly nondefault profiles remain separate evidence, not closure substitutes. Actual resolved model metadata, not version or model ID alone, defines the client default. Deployer-selected public model profiles such as `readable_reasoning` are configuration, not client overrides. Resource APIs, persistent sessions, real multi-account failover and budget formulas retain their separate boundaries. The former field-by-field equivalence review is not a separate Stage-4 gate; L2 applications are found during R1 and R2.

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

The user accepted principles P1–P5 and selections S1–S11 on 2026-10-28, generalized S2 to semantic-equivalence compatibility, and on 2026-10-30 accepted the conversion ladder with its selected applications. [Architecture](architecture.md#conversion-ladder) owns their content; this table maps scheduling ids to owners and migration slices.

| Id | Accepted rule | Owner | Slice |
|---|---|---|---|
| L | Conversion ladder and its 2026-10-30 applications | [Conversion ladder](architecture.md#conversion-ladder) | R0–R5 |
| S2 | Semantic-equivalence compatibility (ladder L2) | [Semantic-equivalence compatibility](architecture.md#semantic-equivalence-compatibility) | R1, R2 |
| S3 | Responses strictness resolution, revised for unknown compatibility | [Projection principles](architecture.md#clientir-projection-principles) | R1 |
| S4 | Reasoning presentation and continuity | [Projection principles](architecture.md#clientir-projection-principles) | R1 |
| S5 | Generalized visible-history transfer | [Reasoning replay](architecture.md#reasoning-replay) | R4 |
| S6 | Standard Chat reasoning omission, extended to opaque reasoning | [Chat reasoning extensions](architecture.md#chat-reasoning-extensions) | R2 |
| S7 | Owner-local cache omission | [Owner-local cache admission](architecture.md#owner-local-cache-admission) | R1 |
| S8 | Messages baseline, superseded by the revised version policy | [Protocols](architecture.md#protocols) | R3 |
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
