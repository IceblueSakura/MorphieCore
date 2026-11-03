# Roadmap

This page owns current scope, priorities, dependencies, and exit gates. [Architecture](architecture.md) owns accepted design, [API expansion design](api-expansion-design.md) owns unselected proposals, and [Development](development.md) owns verification procedures. Code and tests define behavior. Completed slices are removed; Git preserves history.

## Contents

- [Current status](#current-status)
- [Account rewrite execution order](#account-rewrite-execution-order)
  - [Scope and cutover](#scope-and-cutover)
  - [Ordered work and exit gates](#ordered-work-and-exit-gates)
  - [Verification and operator handoff](#verification-and-operator-handoff)
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

**Current focus (2026-11-03):** Slices R0–R5 and account rewrite gates AR0–AR4 are complete. AR5 starts with fresh credential provisioning and actual model selection, then closes real Provider and full three-protocol installed-Agent acceptance. Stage 4 remains paused.

| Area | State | Next step |
|---|---|---|
| Routing | Account rewrite and installed-client synthetic recovery closed through AR4 | AR5 selected-account/model Provider acceptance |
| Generation IR / Codec | R0–R5 complete; shared projections/discovery, qualified intake, history transfer and reviewed null contracts | Maintain these contracts through the installed-Agent exit |
| Client compatibility | Current pi 1.5.0 / OpenCode 2.0.32 pass 17 isolated synthetic scenarios on the rewritten account path | Full three-protocol/default-profile exit follows fresh model selection |
| Integration | R1a/R1b/R2 loops, controlled recovery and production closure passed | Revalidate changed paths against established gates |
| Cross-protocol conversion | Stage 3 closed for selected projections; `count_tokens` deferred | Stage-4 Embedding paths active; Google API paused |
| Decision APIs | Dual standard-SDK direction selected (TypeSafe/OpenAI) | Contract selection and Stage-4 implementation |
| Embedding & Media | Operational (Google native, OpenAI via OpenRouter, DashScope); Grok image codec implemented | Stage-4 contract selection for Images, Audio, Files, and Video |
| Downstream integration | Agent API key authentication and diagnostic attribution implemented | Per-Agent JSONL session storage design |
| Test baseline | Pinned upstream test baseline closed; strict SDK accumulators verified | Maintain offline baseline (`cargo test --locked --offline`) |

## Account rewrite execution order

Plan prepared on **2026-11-02** for the accepted [account specification](provider-account-redesign-plan.md). The sequence follows ownership dependencies: replace the shared account source before routing health, establish health/lifecycle ownership before retrying attempts, then validate consumers before requesting real credentials. No migration phase or compatibility mode is included.

### Scope and cutover

- Replace account configuration, stored-record shapes where needed, provisioning/inspection CLI, Provider/model binding, startup wiring and Generation recovery. Bind existing diagnostic and standalone-task entrances to the same account source without adding their own multi-account retry machinery or activating frozen features.
- Remove pools, historical aliases, legacy readers, per-pool attempt policy and request-internal target advancement. Do not retain old schemas merely to reuse credentials. The user will provide fresh keys; OAuth accounts use managed login where needed.
- Reuse the current semantic IR, protocol codecs, pure lowering, secure-store primitives, OAuth drivers and transport/client libraries unless an account-boundary defect requires a scoped change. Keep official protocol, replay, publication, cancellation and credential-security contracts; “fully breaking” does not discard these invariants.
- Use the existing Nix toolchain, Tokio timers/cancellation, reqwest with implicit retries disabled, serde/schema parsing, clap, `fs-mistrust`/atomic storage helpers and `httpdate`. No new scheduler, persistence service or test framework is planned.
- Work on `main` in coherent verified slices. A cutover may touch all consumers of one account contract in one slice; do not create a compatibility adapter just to preserve smaller commits. Intermediate slices are development checkpoints, not deployment candidates. Remove superseded helpers/tests with their owners, while preserving safety assertions.
- No private credentials are needed for AR0–AR4. Use fresh test-owned storage, synthetic identities and local upstream authorities. Do not inspect, migrate, erase or revoke existing private credentials.

### Ordered work and exit gates

The identifiers below belong only to planning and acceptance documentation, not production modules or test names.

| Order | Work | Completion evidence |
|---|---|---|
| **AR5 — Fresh credentials, model selection and real acceptance** | The user provisions new API keys and any required OAuth sessions through the new loader/CLI. Validate configured accounts and select actual Provider/model bindings. Then run bounded serial Provider checks and the deferred full installed-Agent exit on the rewritten path. | Actual credential kinds and selected models are verified separately from synthetic recovery. Current installed pi/OpenCode complete default reasoning/tool/history loops across the three client protocols and selected cross-protocol lanes. Unavailable account combinations are reported as unverified, not inferred from mocks. Production activation remains separately approved. |

AR1–AR3 are implementation dependencies, not three runtime modes or mandatory separate commits. Internal steps may be split for review only when the resulting slice remains coherent and does not introduce old/new compatibility execution. If the account cutover needs all three to form a coherent runtime, keep them in one implementation slice instead of manufacturing compatibility shims. Delete obsolete code as soon as its owner is replaced. A failure in protocol evidence, trusted-target binding or cancellation blocks dependent work; the retry ceiling and publication rules are not relaxed to make a check pass.

### Account cutover ownership

AR0 closed on **2026-11-02** against clean `9c68a7d`: the full offline baseline passed (950 Rust tests, 39 ignored; 20 pinned assets; nine upstream Python tests), with no existing failures. The [dependency and official-source recheck](refs/2026-11-02-account-recovery-sources.md) requires no pin changes. No private configuration, credentials or real Providers were used.

AR1 closed on **2026-11-02**: the full offline baseline passes (962 Rust tests, 39 ignored; Clippy, formatting, library/binary compilation, 20 pinned assets and nine upstream Python checks). The other Python suites pass (19 SDK-oracle, 33 client-driver, 24 gateway/schema tests), as do all 18 pinned SDK loopback consumers. Production-loader checks cover named account ordering, independent API-key/SIWC contracts, task targets, duplicate security targets, network-free validation and single-target dispatch. Regressions first reproduced removal-tombstone/revision failures and observation-validation gaps, then passed after repair. Installed-client fixture/configuration acceptance remains AR4; no private state, real Provider or deployment was used.

AR2 closed on **2026-11-02**: the full offline baseline passes (980 Rust tests, 39 ignored; Clippy, formatting, library/binary compilation and 20 pinned assets), with the 9/19/33/24 Python gates and all 18 serial SDK loopbacks. Native failed Responses, concurrent absent affinity observers, reverse-order account/target probes and pre-EOF failure cancellation each reproduced their intended defect before repair. Deterministic checks cover scoped constraints, short bound-target waits, deadline/cancellation, probe competition, expiry/eviction ABA, stale results and bounded observations. Actual credential maintenance advances verified material receipts consumed by network-free request availability; replacement releases only old auth for a probe, fences stale 401s and preserves quota/lifecycle vetoes. Managed-key replacement now changes the codec replay issuer; same-account signed history remains intact and account switching strips only foreign opaque owners. Configurable OAuth timing retains short-grant spacing and cancellation quarantine; zero scan/renewal budgets dispatch no unbounded work. Installed-client configuration and recovery remain AR4; no private state, real Provider or deployment was used.

AR3 closed on **2026-11-03**: the full offline baseline passes (988 Rust tests, 39 ignored; Clippy, formatting and production compilation), with 20 pinned assets, the 9/19/33/24 Python gates and all 18 serial SDK consumers. Real HTTP and controlled clocks cover exact same-target wire reuse, two-send bounds, hint waiting, zero budget, cancellation, original deadlines, long generation after content and valid empty terminal readiness without early JSON publication. Failed terminal usage, native failure evidence, malformed canonical fields, cross-Provider code isolation and clean EOF versus typed transport interruption have focused regressions. TLS alerts remain health-neutral and non-retryable; a ready backup suppresses a primary's long route hint. Legacy chain/transition and route send-budget internals are removed. Installed-Agent configuration/recovery remains AR4; no private state, real Provider or deployment was used.

AR3 review fixes on **2026-11-03** pass the full offline baseline again (989 Rust tests, 39 ignored; Clippy, formatting and production compilation), 20 pinned assets, the 9/19/33/24 Python gates and all 18 serial SDK consumers. Regressions first reproduced lost long/reset hints, missing HTTP-200 failure hints, malformed Chat messages and Messages canonical fields, and omitted OpenAI overload eligibility. Real HTTP checks now retain one-send vetoes for long delta/date/overflow hints and verify in-band hint waiting, unchanged wire reuse and health ownership. Installed-Agent recovery remains AR4; no private state, real Provider or deployment was used.

AR4 closed on **2026-11-03**: the final tree passes the full offline baseline (991 Rust tests, 42 ignored; Clippy, formatting and production compilation), 20 pinned assets, the 9/19/37/24 Python gates and all 18 serial SDK consumers. Current installed pi **1.5.0** and OpenCode **2.0.32** pass all 17 isolated synthetic cases. Named-account writers replace old candidate targets and attempt ceilings; same-target recovery checks unchanged bytes and post-header hint waiting. Separate cases prove long-hint one-send failure, the two-send ceiling, next-request failover and cancellation before a delayed retry. Cancellation waits for the correlated gateway terminal receipt before follow-up; publication/cancellation gates retain an eligible secondary. Summary retries remain client-owned. Fresh CLI provisioning/explanation/status/validation, the explicit-account live-runner constructor, diagnostic dispatch and fixed-account task dispatch use production construction with synthetic credentials. The current pi default-body corpus reproduces byte-for-byte; no old-Agent compatibility branch, private state, real Provider or deployment was used.

AR4 timing review on **2026-11-03** reproduced a false early-retry verdict by delaying the authority's post-header sample by 150ms. The repaired gate brackets publication, reports its elapsed-time interval and still rejects definitely early retries; the Rust owner independently requires the full delay from retained failure observation. The original reproduction now passes, as do all 17 current-Agent synthetic cases, 37 client oracles and the full offline baseline (991 Rust tests, 42 ignored; Clippy, formatting, production compilation, 20 pinned assets and nine upstream Python checks). SDK source and production behavior are unchanged; their preceding acceptance remains valid.

Existing fixture owners suffice: credential storage fault injection and synthetic authorities for lifecycle; executor paused-time tests for health/probes; native Responses Router fixtures for failed-task health; transport loopback fixtures for interruption/TLS/deadline facts; production bootstrap, SDK and installed-client harnesses for the connected path. No new framework is needed.

### Verification and operator handoff

- Add focused assertions at the responsible owner while rewriting, using `tests/{credential,process,gateway,transport}` and existing unit helpers. Integrate independent source-derived wire expectations when binding/classification changes; do not use old pool mappings or round trips as the oracle.
- Before publishing each completed implementation slice, run the [full offline baseline](development.md#offline-baseline). Run applicable [Python checks](development.md#python-checks) and [SDK loopback](development.md#sdk-loopback-gate) for changed surfaces; reuse still-valid evidence instead of rerunning unchanged suites after documentation edits. Keep tests enabled and preserve strict JSON, bounds, auth, origin, replay, lifecycle and terminal assertions.
- AR4's [installed-client synthetic recovery](development.md#installed-client-synthetic-recovery) is distinct from AR5's real Provider/default-Agent exit. Use currently installed Agents with isolated state, never downgrade them to source pins or alter their personal credentials.
- Finish AR0–AR4 before asking the user for keys. Deliver current configuration examples, human-readable validation/status and explicit setup commands; secret values do not appear in source, command logs or reports. OAuth provisioning is an interactive managed login, not token copying.
- For AR5, fix target, request count, output cap and timeout before each real run, use the project loader and redacted reporting, and run serially without hidden harness retries. New model selection follows the rewrite; only correctly bound existing models serve as intermediate reference cases.
- Do not stop/restart deployed services, overwrite old credential stores, revoke sessions or activate new bindings as a side effect of code acceptance. Deployment and any destructive private-data operation remain separately approved. A new credential format does not require automatic deletion of the old files.
- Likely blockers are loss of transport failure-stage information, Provider/profile-specific model restrictions and credential-publication races. Resolve them with pinned source inspection and synthetic reproductions within the affected slice; do not add compatibility layers or broaden semantic admission as a workaround.

Completed rows are removed as slices land; meaningful contract changes belong in Architecture and the account specification. AR5 closure resumes the existing Stage-4 sequence, not a new API expansion.

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

Captured pi 1.4.0 shapes run through the permanent [production-construction gate](development.md#sdk-loopback-gate). R1–R4 resolved earlier lanes; unresolved lanes are listed below:

| Lane | Current gate disposition | Ladder resolution | Slice |
|---|---|---|---|
| Responses client → OpenAI subscriptions, flat tools | Flat tools reject (requires namespace definitions and qualified calls) | Retain selected tool contract until endpoint evidence qualifies carrier | Endpoint qualification |
| Chat upstream usage | Duplicate nonnull final reports | Qualify at emitter | Outside slices |

### Conversion-ladder migration

R0–R5 migrated rejection sites onto shared request/delivery projections and compiled discovery. [Architecture](architecture.md#delivery-and-evidence-decisions) owns the completed contracts, including the identity probe and null review.

#### Remaining stabilization exit

| Slice | Work | Exit gate |
|---|---|---|
| **Exit (deferred)** | After account rewrite and new model selection: current-client characterization, then default reasoning/tool/history gateway loops with installed pi and OpenCode across all three client protocols and selected cross-protocol lanes. | Gateway execution and replay pass within documented profiles on the rewritten account path and selected models. |

Dependencies: R0–R5 are complete. Local representation identities remain after the probe; [null-contract evidence](refs/2026-10-27-client-ir-boundaries.md#generation-null-contract-review-2026-11-02) distinguishes defaults from reported absence. The final current-client exit remains independent of offline regression and SDK gates. The [account-redesign sequence](#temporary-routing-freeze) now precedes this exit; interim verification uses focused offline and applicable SDK gates without requiring the deferred full installed-Agent loop. Existing acceptance remains dated evidence, not qualification of the rewritten path or newly selected models.

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

The separately requested [Provider account redesign plan](provider-account-redesign-plan.md) was **finalized on 2026-11-02**; Architecture records its [accepted decisions](architecture.md#routing-and-security-decisions). The user accepted the fully breaking scope, fresh manual provisioning and subsequent implementation. The [execution plan](#account-rewrite-execution-order) owns dependencies and gates: replace the account system, provision fresh credentials and select models, complete the installed-Agent exit, then resume Stage 4. Implementation does not authorize changing deployed credentials or activation.

During the account rewrite, only in-scope work may replace the [stable routing baseline](architecture.md#failover); unrelated codec and protocol work must not alter:
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

#### First reconnection slices

Selected loops (R1a, R2, R1b) and Google basic reconnection are completed under their recorded evidence. Further reconnection follows the [public capability contract](architecture.md#public-capability-and-adaptation-notice) and model-level preflight validation.

### Controlled routing acceptance

Controlled synthetic recovery is verified:
- Router/SDK recovery across simulated network faults and expired credentials.
- Sticky conversation continuation with signed-history preservation.
- All-cooling fail-safe recovery.
Real cross-account failover requires distinct, advanceable production credentials and remains subject to production deployment gates.

## Pending decisions

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
Generation Text model discovery, shared capability derivation and qualified native-interface preference are operational; [Architecture](architecture.md#generation-text-model-discovery) owns the published domains. Discovery for other task families (Embeddings, Media, Decision) remains schema-only until those operations are publicly activated.

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
