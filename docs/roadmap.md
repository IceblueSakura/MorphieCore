# Roadmap

This page owns current scope, priorities, dependencies, and exit gates. [Architecture](architecture.md) owns accepted design, [API expansion design](api-expansion-design.md) owns unselected proposals, and [Development](development.md) owns verification procedures. Code and tests define behavior. Completed slices are removed; Git preserves history.

## Contents

- [Current status](#current-status)
- [Account rewrite execution order](#account-rewrite-execution-order)
  - [Ordered work and exit gates](#ordered-work-and-exit-gates)
  - [Verification and operator handoff](#verification-and-operator-handoff)
- [Three-protocol fidelity repair](#three-protocol-fidelity-repair)
  - [Scope and boundaries](#scope-and-boundaries)
  - [Reference strategy](#reference-strategy)
  - [Agent-default production findings](#agent-default-production-findings)
  - [Tool protocol root repair](#tool-protocol-root-repair)
    - [Stage-3 core packages](#stage-3-core-packages)
    - [Conditional Stage-3 follow-ups](#conditional-stage-3-follow-ups)
    - [Tool verification and scope gates](#tool-verification-and-scope-gates)
  - [Conversion-ladder migration](#conversion-ladder-migration)
    - [Remaining stabilization exit](#remaining-stabilization-exit)
  - [Retained rejections](#retained-rejections)
  - [Remaining issue dispositions](#remaining-issue-dispositions)
  - [Closure boundary](#closure-boundary)
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

**Current focus (2026-11-04):** Execute the scheduled Stage-3 tool-repair core, S3-T0–S3-T4, across all nine Chat/Responses/Messages directions. Concrete new loss policies are selected at S3-T0 before activation. Pi Chat strict read and pi Responses `additional_tools` are acceptance cases. OpenCode Responses missing annotations remains postponed; no normalization or acceptance waiver is selected. The core and outstanding default-profile gate precede AR5; Stage 4 resumes only after real Provider and installed-Agent acceptance.

| Priority | Remaining work | Exit |
|---|---|---|
| Stage-3 tool core | Execute [S3-T0–S3-T4](#stage-3-core-packages): contract selection, shared semantics, native mappings, bounded projections and nine-direction acceptance | Ordinary function loops complete in all nine directions, with no hidden guarantee loss |
| Conditional tool follow-ups | [S3-T5–S3-T7](#conditional-stage-3-follow-ups): namespace aliases, custom-text wrappers and examples projection | Separate policy selection and full affected-loop acceptance; not default prerequisites for AR5 |
| Agent defaults | Resolve pi Chat strict read; revisit postponed OpenCode Responses annotations | Actual default reasoning/tool/history loops pass without compatibility opt-outs or input rewriting |
| Account acceptance | Provision fresh credentials, select actual Provider/model bindings, and complete AR5 | Bounded real Provider checks and installed-Agent exit on the rewritten account path |
| API expansion | Select and implement Stage-4 operation contracts after AR5 | Per-operation lifecycle, protocol and SDK acceptance |
| Downstream integration | Implement opt-in per-Agent JSONL session storage | Selected isolation and lifetime bounds |
| Production preparation | Decide diagnostic packaging and deferred observability scope | Explicit deployer selection |

## Account rewrite execution order

The remaining gate for the accepted [account specification](provider-account-redesign-plan.md) is real acceptance on fresh credentials and selected models. The user selected **offline three-protocol preparation before fresh credentials** on 2026-11-03; resolve the [default-profile blockers](#agent-default-production-findings) before AR5 provisioning. Setup uses the [Provider-owned, alias-only credential contract](provider-account-redesign-plan.md#credential-kind-and-header-placement). No compatibility mode, private-store migration or deployment is included.

### Ordered work and exit gates

The identifiers below belong only to planning and acceptance documentation, not production modules or test names.

| Order | Work | Completion evidence |
|---|---|---|
| **AR5 — Fresh credentials, model selection and real acceptance** | Entry: S3-T0–S3-T4 and the outstanding default-profile gate are complete. The user provisions new API keys and any required OAuth sessions through the new loader/CLI. Validate configured accounts and select actual Provider/model bindings. Then run bounded serial Provider checks and the deferred full installed-Agent exit on the rewritten path. | Actual credential kinds and selected models are verified separately from synthetic recovery. Current installed pi/OpenCode complete default reasoning/tool/history loops across the three client protocols and selected cross-protocol lanes. Unavailable account combinations are reported as unverified, not inferred from mocks. Production activation remains separately approved. |

### Verification and operator handoff

- Deliver current configuration examples, human-readable validation/status and explicit setup commands; secret values do not appear in source, command logs or reports. OAuth provisioning is an interactive managed login, not token copying.
- Keep [installed-client synthetic recovery](development.md#installed-client-synthetic-recovery) distinct from real Provider/default-Agent acceptance. Use currently installed Agents with isolated state, never downgrade them to source pins or alter their personal credentials.
- Fix target, request count, output cap and timeout before each real run, use the project loader and redacted reporting, and run serially without hidden harness retries. Use the [production-profile acceptance procedure](development.md#production-profile-acceptance).
- Do not stop/restart deployed services, overwrite old credential stores, revoke sessions or activate new bindings as a side effect of code acceptance. Deployment and any destructive private-data operation remain separately approved.
- Resolve Provider/profile-specific model restrictions or credential-publication failures with pinned source inspection and synthetic reproductions; do not add compatibility layers or broaden semantic admission as a workaround.

AR5 closure resumes the existing Stage-4 sequence, not a new API expansion.

## Three-protocol fidelity repair

This is the targeted **Stage-3 follow-up**, not a reopening of completed Stage-3 scope. It repairs semantic fidelity across standard Chat Completions, Responses, and Messages via the [tool core](#stage-3-core-packages) and [remaining stabilization exit](#remaining-stabilization-exit). Authoritative contracts: [conversion ladder](architecture.md#conversion-ladder), [field handling](architecture.md#field-handling), [projection principles](architecture.md#clientir-projection-principles), and [stream normalization](architecture.md#semantic-stream-normalization).

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

The [native-default characterization](development.md#installed-client-synthetic-recovery) exercises all three protocols with pi 1.5.0 and OpenCode 2.0.32. On 2026-11-04 the user expanded ordered-tool analysis to all nine Chat/Responses/Messages directions and accepted bounded approximation as a design direction. Missing-annotations repair remains postponed and unaccepted; neither postponement nor characterization waives the fresh-credential/default-Agent gates. Endpoint qualification and Chat usage-emitter work retain their separate scope.

| Lane | Remaining issue | Required resolution | Scope |
|---|---|---|---|
| Pi Chat | Actual strict read guarantee is blocked | Resolve the explicit guarantee without client downgrade or input rewriting | Before fresh credentials |
| Pi Responses | `additional_tools` input is blocked | Ordered source preservation plus target-native or selected approximate projection through the [tool root repair](#tool-protocol-root-repair) | Before fresh credentials |
| OpenCode Responses | Recall omits required text annotations | Postponed for user consideration; retain strict intake, no inferred empty annotations | Deferred decision; acceptance remains open |
| Responses client → OpenAI subscriptions, flat tools | Current binding requires namespaces and qualified calls even for inline definitions | Reconcile with official SIWC `additional_tools` support, including subsequent direct call/result history | Tool contract repair; real endpoint acceptance separate |
| Chat upstream usage | Duplicate nonnull final reports | Qualify at emitter | Outside stabilization slices |

### Tool protocol root repair

The [accepted direction](architecture.md#ordered-tool-availability) covers three native and six cross-protocol paths across Chat, Responses and Messages, each including request, client-native return, tool-result submission and recall. The [design](api-expansion-design.md#three-protocol-tool-conversion-design) owns the nine-cell matrix, candidate losses and recommendations; [dated evidence](refs/2026-10-27-client-ir-boundaries.md#nine-direction-tool-assessment-2026-11-04) owns findings. Earlier Google assessment remains reference material; Google expansion is not part of this execution plan.

**Stage placement selected; implementation pending.** The core belongs before AR5, because it repairs the existing Generation tool contract rather than adding Stage-4 APIs or Stage-5 tool execution. Stage placement does not activate unselected loss policies. Use existing semantic types, validation, disposition tables, request/delivery projection, fidelity, strict JSON and SDK/Agent harnesses. No new dependency, persistent registry, executor, general schema simplifier or nine pair-specific adapters. First make one native Responses addition/function loop work end-to-end, then grow shared rules across the remaining cells.

**S3-T0 in progress (2026-11-04):** [Current stable source qualification](refs/2026-10-27-client-ir-boundaries.md#tool-contract-qualification-2026-11-04) confirms the native batch, source-cardinality and strictness distinctions. The [concrete first-package dispositions](api-expansion-design.md#first-package-dispositions) await tool-set content-loss selection; collision-dependent transformations and non-developer returned-addition replay stay isolated. The actual pi Chat strict-read lane additionally needs an explicit DeepSeek strict-endpoint/binding qualification decision. No new loss or endpoint is activated by this preparation.

#### Stage-3 core packages

S3-T0 → S3-T1 → S3-T2 → S3-T3 → S3-T4 is the core dependency order. Verification accompanies each slice; S3-T4 consolidates the connected exit rather than postponing tests. Identifiers are planning labels only, never module or test names.

| Package / dependency | Coherent slice | Exit check |
|---|---|---|
| **S3-T0 — Contracts and loss selection** / first | Qualify current official contracts and stable source/SDK pins; resolve or isolate collision precedence, forced-call cardinality, strictness strength, output roles and custom-stream gaps. Select the concrete first-package loss rows | Independent source expectations and a disposition for each material field family; unknown native behavior delegates, while transformations needing an unknown winner do not guess. No package/source version conflation |
| **S3-T1 — Shared tool semantics** / after S3-T0 | Generalize ordered batches and shared definition/availability checks. Separate current selection from historical identity. Fix inline-only choice, serial emission, feature/dispatch traversal and bounds; add only necessary semantic distinctions | Existing defect regressions fail before repair and pass after it; independent batch/ID, root/inline/namespace, required/named/allowed and malformed/bounds cases. A single Responses native request→call→result→recall slice works |
| **S3-T2 — Native and ordinary cross mappings** / after S3-T1 | Complete native function paths and ordinary cross mappings, source-default strictness, call cardinality, nullable input status and legal Messages positions. Reconcile SIWC inline declarations and subsequent direct calls | Three native function loops preserve their source guarantees; cross-protocol fixtures check both wire directions; no returned role promotion or fabricated historical schema. Pi strict-read admission is fixed at its actual protocol/binding owner, not by removing `strict:true` |
| **S3-T3 — Bounded tool-set projection** / after S3-T2 and policy selection | Generalize active-set/allowed-set projection from existing Messages collapse into one owner. Preserve more native history where current selection can still be enforced; retain existing error/default losses | V1/call/V2, removal/re-addition, deferred reference, empty set, conflicting forced choice and allowed-subset contrasts. Source history is immutable; cache-prefix loss is explicit; no withheld/removed tool becomes callable |
| **S3-T4 — Nine-direction offline and client exit** / after S3-T3 | Complete nine-cell SDK/gateway acceptance and current installed-Agent checks; update discovery from the same dispositions and remove obsolete blanket rejection tests/helpers | Ordinary function call→result→answer→recall succeeds for all nine cells. Pi default tool blockers pass synthetic checks; postponed OpenCode annotations stays separately reported. Full offline baseline and relevant SDK gates pass; real Provider evidence belongs to AR5 |

#### Conditional Stage-3 follow-ups

These existing-tool compatibility improvements belong to Stage 3, not modality expansion or hosted execution. They remain **conditionally scheduled after the core**, requiring separate policy selection. They do not block AR5 or Stage 4 by default; if a selected acceptance lane requires one, make that package an explicit prerequisite for that lane. Deferred packages remain here rather than being silently activated or treated as complete.

| Package / entry | Scope | Exit check |
|---|---|---|
| **S3-T5 — Namespace aliases** / after S3-T4 and alias-policy selection | Reversible leaf names through request, selection, return and history; reuse candidate/delivery ownership | Duplicate leaf names, reorder/append/remove stability, original client identity, candidate isolation and JSON/SSE/retained agreement |
| **S3-T6 — Plain custom-text wrapping** / after S3-T4 and wrapper-policy selection; S3-T5 only for namespaced wrappers | Grammar-free custom text through function wrappers and inverse delivery; no grammar-to-prose fallback | Exact completed text and original call kind, bounded buffering, honest partial/cancelled states and lawful client delivery; no invented custom SSE |
| **S3-T7 — Tool examples projection** / after S3-T4 and content-policy selection | Bounded labelled examples in descriptions when no native examples carrier exists | Example order/content and absent-versus-empty meaning remain deliberate; schema, selection and strictness unchanged; token/cache/salience loss recorded |

#### Tool verification and scope gates

Verification is layered rather than every field multiplied by nine:

- Extend existing semantic and SDK tool-history fixtures with independent wire→IR and IR→wire expectations. Nine ordinary loop cells cover route composition; family-specific tests cover distinct losses and failures, not redundant permutations.
- Check JSON/SSE and retained-view agreement, partial arguments, raw-string authority, distinct item/call IDs, error results, cancellation, bounds, EOF and replay isolation. New alias/wrapper mapping must finish before publication and cannot depend on clients returning diagnostics.
- Reuse `tests/sdk/openai_tool_history.rs`, `tests/sdk/messages_tool_loop.rs`, existing semantic tool/schema/stream cases and installed-client drivers. Keep official baseline assets and real SDK consumers; do not create a parallel fixture framework.
- Current installed pi/OpenCode versions are recorded at execution time, without downgrade. Pi default Responses and the strict Chat tool case are positive goals. OpenCode Responses' postponed annotations case remains a separately reported known failure, not repaired by fixtures or waived by nine SDK loops.
- Reuse the [offline baseline](development.md#offline-baseline) and [SDK gate](development.md#sdk-loopback-gate) per implementation slice. Real Provider/model checks remain AR5 with fresh credentials and separately bounded runs; synthetic acceptance does not certify endpoint behavior.

Risks that change the plan:

- Unknown root/inline precedence prevents computing a faithful source active set for colliding definitions; scope that approximation to known cases, while preserving native shapes when possible.
- Missing custom-SSE carriers or lawful returned-role replay prevent complete advanced-family loops. Keep those boundaries explicit; ordinary function work need not wait for a speculative transport.
- Explicit strict, selected-tool, cardinality or serial guarantees with no target carrier remain rejected unless that concrete guarantee downgrade is selected. Do not repair by schema deletion, automatic prompting or a new silent fallback.
- S3-T5–S3-T7 require their own policy selection and affected-loop acceptance; historical annotations, media relocation and grammar-to-prose remain outside the scheduled packages.

Tool discovery, hosted/programmatic execution, new endpoints, Google expansion and OpenCode annotation normalization remain outside this plan. Public capability metadata must describe conditional mappings honestly; general protocol or SDK presence is not a per-model promise.

### Conversion-ladder migration

#### Remaining stabilization exit

After the default-profile repairs, fresh credentials and model selection, run current-client characterization and default reasoning/tool/history gateway loops with installed pi and OpenCode across all three client protocols and selected cross-protocol lanes.

Exit: gateway execution and replay pass within documented profiles on the rewritten account path and selected models. This exit is part of [AR5](#ordered-work-and-exit-gates), independent of offline regression and SDK gates. Interim verification uses focused offline and applicable SDK gates; dated acceptance does not qualify new account/model paths.

### Retained rejections

These L5 boundaries and invariant guards constrain the remaining work. Current additional blockers and their scope are tracked in the [findings table](#agent-default-production-findings).

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
| Messages custom-input wrapping | Conditional Stage-3 follow-up S3-T6; not a core exit prerequisite | Declaration, arguments, return/history mapping and client execution expectations are selected together |
| Token scope and SDK drift | Investigate only the selected target/field; no automatic source-pin promotion | A bounded budget rule or reviewed version change has independent expectations and a reachable target path |

### Closure boundary

Close Client/IR stabilization when every same-protocol lane and each selected cross-protocol lane accepts or cites a [retained rejection](#retained-rejections) or invariant; discovery publishes the same facts; and the exit loops pass.

Default reasoning/tool/history gateway success on all three client protocols is required for closure. Deployer-selected public model profiles (e.g. `readable_reasoning`) are configuration, not client overrides.

## Planned order

Only remaining work is listed. The Stage-3 tool core precedes the outstanding default-profile gate and [AR5](#ordered-work-and-exit-gates); independent resolution of the postponed annotations decision can proceed alongside the core, but its gate is not waived. Stage 4 resumes after AR5; Stage 5 remains unscheduled.

| Stage | Goal | Exit evidence |
|---|---|---|
| **3 — Tool-semantic core follow-up** | [S3-T0–S3-T4](#stage-3-core-packages): contracts, shared semantics, native/cross mappings, selected bounded projections and nine-direction acceptance | Nine ordinary function SDK/gateway loops, focused loss/invariant tests and current-Agent synthetic checks; no claim of live Provider acceptance |
| **3 — Conditional compatibility follow-ups** | [S3-T5–S3-T7](#conditional-stage-3-follow-ups): namespace aliases, plain custom wrappers and examples projection | Separate policy selection and affected full-loop verification; not default blockers for AR5 or Stage 4 |
| **AR5 — Pre-Stage-4 acceptance gate** | After the core and default-profile gate: fresh credentials, actual model/binding selection and bounded real Provider/current-Agent checks | Real tool/history loops on the rewritten account path; unresolved or unavailable lanes reported honestly |
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

The [accepted account contract](provider-account-redesign-plan.md) and [routing decisions](architecture.md#routing-and-security-decisions) constrain the remaining acceptance work. Provision fresh credentials and select models, complete the installed-Agent exit, then resume Stage 4. Implementation does not authorize changing deployed credentials or activation.

Until AR5 closes, unrelated codec and protocol work must not alter the [stable routing baseline](architecture.md#failover):
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
| Tool-set history loss | S3-T0 selection; S3-T3 implementation | Select effective-set/allowed-set losses without weakening explicit call/schema guarantees |
| Namespace aliases, custom text and examples | Conditional S3-T5–S3-T7 | Select each content/interface adaptation separately; do not make all three implicit core prerequisites |
| OpenCode missing annotations | Postponed user decision; default-profile gate remains open | Keep strict intake until its separate history contract is selected; tool work does not normalize it |
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
| API expansion | Stage 4 paused until AR5, then equal priority; Stage 5 unscheduled | Specific modalities |
| Files & multipart uploads | Stage 4 priority; requires storage authority selection | File resource endpoints |

## Deferred work

### Google API pause

Further general Google API expansion is paused. Preserve the existing [terminal-report exceptions](architecture.md#service-policies-and-reports) and [native text Embedding path](architecture.md#independent-embedding-path). General multimodal input, native client intake, custom tools, unified error parsing and broader probability reports remain excluded from this work.

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
