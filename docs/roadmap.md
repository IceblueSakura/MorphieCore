# Roadmap

This page owns current scope, priorities, dependencies and exit gates. Start with the next work package; it is not an approval record or implementation specification. [Architecture](architecture.md) owns accepted design, [API expansion design](api-expansion-design.md) owns unselected proposals, and [Development](development.md) owns verification procedures. Code and tests define behavior. Remove completed slices; Git keeps history.

## Contents

- [Current status](#current-status)
- [Planned order](#planned-order)
  - [Execution sequence and work-package rules](#execution-sequence-and-work-package-rules)
  - [Stage 1 work plan](#stage-1-work-plan)
  - [Stage 2 work plan](#stage-2-work-plan)
  - [Stage 3 work plan](#stage-3-work-plan)
  - [Stage 4 scope](#stage-4-scope)
  - [Stage 5 backlog](#stage-5-backlog)
- [Phase rules](#phase-rules)
  - [Generation design review gate](#generation-design-review-gate)
  - [Observability follow-up](#observability-follow-up)
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
  - [Messages citations](#messages-citations)
- [Deferred acceptance](#deferred-acceptance)
  - [Target clients](#target-clients)
  - [Only if needed](#only-if-needed)
- [Frozen scope](#frozen-scope)
  - [Not planned](#not-planned)
  - [Maintenance rules](#maintenance-rules)

## Current status

| Area | State | Next step |
|---|---|---|
| Routing | Functional phase complete under the [stable routing baseline](architecture.md#failover); feature development closed. Selected [controlled acceptance](#controlled-routing-acceptance) passed through Router, SDK and installed clients | Real Provider failover remains [separately scheduled](#deferred-acceptance); no routing feature expansion |
| Generation IR/codec | Selected stage-1 codec implementation complete; the [stage-1/2 design baseline is human-approved](#generation-design-review-gate). Independent native oracles, history parity and pinned SDK/client-carrier checks retain the [inventory's explicit exclusions](refs/2026-10-19-same-protocol-inventory.md) | [Stage-3 work plan](#stage-3-work-plan); preserve exclusions before further binding/model claims. No implicit activation |
| Client compatibility | Rejection audit and policy selection complete: [phase transfer/return](architecture.md#message-phase-transfer), guarded extended-retention omission and retained citation rejection | Client-carrier acceptance and target storage evidence during reconnection |
| Integration | Selected R1a/R1b/R2 client loops and controlled routing acceptance passed their bounded scopes. R1b lifecycle reports and client history shorthand were explicitly selected on 2026-10-20 | [Stage-3 work plan](#stage-3-work-plan); no routing or binding expansion follows from loop acceptance or baseline approval |
| Cross-protocol conversion and counting | Model-effective guards, Messages defaults, attribution and Interactions snapshots are implemented. Selected Google Chat/Responses and Messages basic loops passed their bounded gates | [Stage-3 work plan](#stage-3-work-plan): counting remains; further [Google API expansion is paused](#google-api-pause), not complete |
| Embedding, other modalities and file resources | Stage 4 includes audio/image/video API adaptation and Files/multipart Uploads | Follow the [operation work packages](#stage-4-scope); existing paths remain maintenance-only until their slices |
| Stage-5 follow-ups | Citations, tool discovery, hosted tools, server-side compaction, Batch and Realtime are recorded but **unscheduled** | No implementation queue or delivery commitment; select each slice before starting it |
| Real acceptance | Selected [stage-2](#stage-2-work-plan) loops passed their SDK/client gates. Google Chat/Responses passed four installed-client cases; pi Messages later passed two more, eight requests | Preserve the [stage-3 scope and limitations](#stage-3-work-plan). [Deferred acceptance](#deferred-acceptance), including real failover, remains open |

The [approved Generation baseline](#generation-design-review-gate) leaves inventory exclusions and binding boundaries unchanged. Other real acceptance gates remain open. Google value guards and Interactions snapshots belong to stage 3: no Google client endpoint forms a same-protocol pair. Provider-specific reset dialects, persistence and cooldown tuning need separate evidence and approval; no background probes, balancing or multi-round retries are scheduled.

Phase completeness covers the declared feature scope, not all official Generation semantics; explicit rejection is safer than silent loss but is not completeness. Chat Completions, Responses and Messages are equally important for same-protocol request, response, static delivery, streaming and history replay. Google generateContent and Interactions supply upstream request encoding and SSE intake only; record missing directions rather than implying a complete same-protocol pair.

## Planned order

| Stage | Goal | Exit evidence for a selected slice |
|---|---|---|
| **1 — Same-protocol lossless IR codecs** | Preserve the declared protocol semantics through client request → IR → same-protocol upstream request, and upstream response/events → IR → same-protocol client delivery/history. Prioritize known fields, presence/default meaning, ordered owners, reasoning/replay, usage and terminal facts over cross-protocol convenience | Independent wire→IR and IR→wire expectations in each applicable direction, JSON/SSE agreement and returned-history preservation. Report unsupported fields/directions and actual losses explicitly; round trips, HTTP success and named loss receipts are not lossless evidence |
| **2 — Basic agent tool interactions** | Ordinary function-tool declarations and schema intent, call IDs/arguments, results/errors, streaming, final answers and next-turn history for pi/OpenCode. The client executes its tools; no gateway tool executor is implied | Selected same-protocol tool-call → result → final-answer → replay loops, preserving source data and required reasoning/cache owners. Keep offline, SDK and bounded real-client/Provider evidence separate |
| **3 — Cross-protocol stability and token counting** | Stabilize selected request, response/event and history conversions through exact carriers or approved named adaptations; implement upstream-first `count_tokens` with explicit model/target, effective-input projection and estimate scope | Independent directional expectations and selected loops; admission/preparation agreement and rejection of protected unrepresentable semantics. Counting distinguishes upstream reports from permitted coarse estimates, uses no gateway tokenizer or inference, and makes no cross-candidate exactness promise |
| **4 — Embedding, modalities and file resources** | Adapt Embedding and audio/image/video APIs through task IR, codecs, execution and delivery; implement Files and multipart Uploads as resource services | Verify formats/dimensions, controls, results/events, bounds, cancellation, accounting and applicable SDK/real gates. File resources require authorization, bounded upload/part handling, identity, retention/deletion and restart-consistent mappings |
| **5 — Unscheduled follow-ups** | Citations/source attribution, `tool_search`, hosted tools, server-side compaction, Batch and Realtime | **Unscheduled:** select each scope/design and explicitly schedule its slice before implementation. No start/delivery commitment or prerequisite for stages 1–4 |

“Lossless” means semantic fidelity within an explicit protocol revision and feature domain, not byte-identical JSON or retention of unknown fields. Equivalent wire normalization is allowed. Preserve protocol records in their existing bounded owners; do not introduce raw passthrough or an IR extension bag. A same-named API at a different Provider is not proof of equivalent semantics.

Audit existing same-protocol loss paths and distinguish codec loss from model/product policy. Basic tool-field fidelity belongs to stage 1; stage 2 adds the agent interaction loop. Previously approved adaptations remain active until deliberately changed, but their occurrence cannot pass a lossless check. Deferred citations or other unsupported semantics remain explicit coverage gaps and reject where required; postponing them never permits silent loss or a claim of complete protocol coverage.

Design review and focused coverage proceed within each stage, not as a requirement to finish every later-stage design first. Keep the existing human approval, publication and integration gates. Preserve security, bounds, cancellation, replay isolation, honest terminals and existing regression assertions at every stage; later scheduling defers expansion, not necessary maintenance. Existing media carriers remain covered during stages 1–3; new modality APIs and execution paths belong to stage 4. Other-agent acceptance keeps its [separate scheduling](#deferred-acceptance).

The P7 boundary review becomes eligible after first-stage local completion, without waiting for deferred cache and tool slices, and is required before route-facing acceptance. Its analysis may proceed alongside design closure.

### Execution sequence and work-package rules

The packages below refine the selected scope, not design approval or binding activation. IDs name coherent deliverables, not a required commit count. Stage-4 ordering is recommended; exact operation contracts still need selection. Stage 5 has no execution queue.

Inference frameworks such as llama.cpp and SGLang are design references, not integration targets. Use their input-preparation, generation-constraint and output-interpretation designs to improve the existing gateway boundaries. This adds no backend work package, engine dependency or generic session/plugin framework; [inference-engine integration is not planned](#not-planned).

1. Stage-1 selected codec work is implemented; use its [inventory and boundaries](refs/2026-10-19-same-protocol-inventory.md) for review and per-loop readiness. No all-official-semantics or real-loop completeness is implied.
2. The selected R1a, R2 and R1b loops completed independently under their own carriers and approval/admission gates. A future loop waits for its own readiness, not every stage-1 package; no additional client/model matrix is scheduled.
3. R2 readiness concerns its implemented pi Haiku profile, not wider native manual-budget or sampling support. An observed defect in a carrier actually used by R2 must still be fixed before accepting it.
4. Complete selected stage-2 loops before routing acceptance and stage-3 conversion work. Keep separately scheduled real failover distinct from controlled synthetic recovery checks.
5. In stage 3, build on the implemented shared target checks without widening admission. Interactions snapshots and counting are separate branches; counting waits only for its selected effective-input projection. S3-3 builds on native identity preservation without treating it as cross-protocol attribution.
6. In stage 4, select the next operation's contract just in time. Design file authority early, but let direct-input Embedding/audio/image slices proceed without the complete file service. File-reference integration waits for authorized resource resolution.

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

Grok/SIWC verification covers their existing Responses bindings, restricted request preparation, native static/event directions, JSON/SSE delivery and stateless/stored history parity. SIWC's upstream is always streamed; static upstream JSON fixtures test representation, not HTTP support. Provider-only scoped reports remain an [explicit client-delivery loss](architecture.md#subscription-responses). This extension does not add Grok Chat, widen SIWC controls or replace the R1a/R1b-first acceptance order.

| Pair | Field or carrier | Current treatment | Recommendation |
|---|---|---|---|
| Chat | `prediction` | Omitted at intake for every target under D3 | Keep D3 unless a selected Chat binding supports it; then move the omission to target projection |
| Chat | Participant `name`, `n` other than 1, `store: true`, `logit_bias`, request `moderation`, content `prompt_cache_breakpoint`, non-null moderation reports, custom-tool SSE | Rejected | Keep as recorded stage-1 gaps; rejecting cache boundaries/reports prevents silent loss but is not exact support |
| All | Unsupported hosted/server item and block variants; Messages citations, document `context`, `clear_at`, `input_examples`, active geography/speed/fallback requests; Responses active `prompt`, `conversation`, `context_management`, access programs and misalignment details | Rejected within the inventory's stated subsets | Stage 5 or a later owner selection; not stage-1 blockers |
| Messages / Responses | Messages `diagnostics`, beta `output_config.task_budget`, tool `toolset_name` and unowned execution/refusal reports; deferred Responses hosted event families | Ignored, explicitly classified in the inventory | No lossless claim for these fields/events; later scheduling does not make ignored protected meaning exact |

Stage-1 exit for a pair means every in-domain official field is exact, or is an explicitly listed rejection or target-policy loss. Approved losses that still fire on same-protocol paths remain listed as non-lossless.

#### Stage-1 to stage-2 handoff

A stage-2 loop needs only its own protocol slice, scoped to that loop's carriers, not the whole of stage 1: R1a follows S1-1 and R1b follows S1-2. R2 is pi's `anthropic-messages` API with the built-in Haiku 5.5 compat flags, which the user's separate pi extension declares for the gateway. The flags are listed in the [pi profile note](refs/2026-10-17-generation-contracts.md#pi-messages-request-profile-2026-10-19). Its carriers are implemented and passed a bounded check, so R2 needs no further S1-3 work. Acceptance runs declare the same model in scratch `models.json`; a declaration without `forceAdaptiveThinking` sends budgeted thinking, which the native target rejects, and is not R2.

Every selected loop must leave no unclassified ignored request fields: `ignored_request_field_count` stays zero, or each ignored name is classified in the inventory. Client-specific dialect fields such as pi's `enable_thinking` or `chat_template_kwargs` are outside the official Chat baseline. The selected pi and OpenCode profiles must emit only registered carriers.

### Stage 2 work plan

**Status: selected R1a/R1b/R2 loops and controlled routing acceptance are complete within their declared scopes.** The user separately selected R1b's [bounded lifecycle reports](architecture.md#lifecycle-settings-and-service-reports) and [client history shorthand](architecture.md#client-responses-history-shorthand) on 2026-10-20. The [overall baseline is now approved](#generation-design-review-gate); neither approval nor these scoped acceptances certify every client, model feature or protocol semantic.

These scoped loops implement the [selected reconnection contracts](#first-reconnection-slices), not a new binding or routing policy. Loop acceptance and overall design approval are separate; neither authorizes later activation. A future loop still needs its own boundary review and binding evidence. R1a and R1b are separate acceptance slices; neither needs R2 or stage-3 conversions to succeed.

R1a's explicitly resumed bounded scope passed on 2026-10-19: DashScope Chat, ordinary non-strict functions, low effort, explicit total output cap and readable reasoning history. Independent Router/SDK JSON/SSE oracles cover tool/result/final/next history and failure boundaries. Live SDK JSON/SSE and installed pi 1.3.0 / OpenCode 2.0.6+e5ecb57 each completed their selected three-turn loops. Client runs used scratch state, at most three serial upstream requests, a 2048-token cap, 120-second request deadlines, one gateway attempt and no forwarded retry; successful checks reported zero ignored request fields. A streamed SDK accumulator and a pi assertion were corrected before their successful checks; those earlier harness failures are not Provider failures. This accepts only the tested profiles, not every effort alias, tool schema, client version or model feature.

R1b's selected lifecycle implementation passed independent native/IR direction checks, bounded report accounting, immutable delivered contexts, honest failures and Router/SDK terminal retrieval parity. On 2026-10-20, 12 real pinned-SDK requests passed JSON/SSE × stateless/stored three-turn tool/result/final/next-turn loops; GET matched the delivered terminal. Phase, opaque reasoning and summary content were observed. Installed pi 1.3.0 and OpenCode 2.0.6+e5ecb57 each passed a separate three-request SSE read/final/next-turn loop with call/result and phase history checks. pi used a test-owned explicitly non-strict read declaration; OpenCode exercised the selected annotations shorthand once. Neither final installed-client run produced opaque reasoning, so those runs do not establish client opaque replay. All used the CLI loader, only `openrouter-responses` / `gpt-6-luna`, low effort, 2048 output tokens, 120-second deadlines, one attempt and zero unclassified ignored request fields. The [dated evidence](refs/2026-10-17-generation-contracts.md#responses-lifecycle-reports-2026-10-19) retains the earlier rejection and scope limitations.

R2's selected native scope passed on 2026-10-19 after the user deferred R1b. The target remains `claude-haiku-5-5` / `opencode-go-haiku-messages`, with pi's built-in Haiku compatibility contract. Configured admission, production Router and pinned SDK checks cover independent JSON/SSE five-turn expectations, strict/deferred/eager tools, ordered system/effort changes, signed/redacted replay, tool-error feedback and next-turn final history. Native Messages now shares the existing tool-publication hold/cut/cancel/no-replay checks.

Installed pi 1.3.0 completed a five-request SSE session: `read` → final, replace `read` with `ls` and change high to low effort → final, then replay both results without tools. Root tools/system stayed stable; explicit 1h markers, reported 1h writes and subsequent cache reads were observed. A separate three-request native JSON companion used 5m markers and the existing error-on-prefix-mismatch carrier: private request/response comparison confirmed exact native signature restoration, successful final/next history and no reported input transformations. Both gates used the CLI project loader, one binding, a 2048-token output ceiling, 120-second request deadlines, one attempt and zero ignored request fields. The [dated evidence](refs/2026-10-17-generation-contracts.md#r2-native-client-and-replay-evidence-2026-10-19) retains limitations: no cross-account/fallback guarantee, no measured expiry or invoice validation, and no effective-effort calibration. Automatic-cache admission remains closed.

No further stage-2 loop is scheduled. Before any later selected loop, repeat per-loop readiness: exact client shape, model-effective carrier evidence, public admission, shared preflight/preparation and classification of every ignored field. Keep exact support, adaptation and rejection distinct.

For each loop, establish independent static and SSE expectations before real acceptance, including tool errors, strict EOF, cancellation and no replay after publication. Use the [existing SDK and client procedures](development.md), not a new acceptance framework. Before every real run fix the target, request count, output cap and timeout; run serially through the project loader with no hidden retries and body-free reporting.

Extended-retention omission is not a prerequisite for these exact-carrier loops. If a later slice selects it, first verify that target's storage policy; cache usage is not retention evidence.

### Stage 3 work plan

The stage covers request projection, selected cross-protocol loops, Interactions intake and counting. Target-value rejection, trusted missing-cap defaults, attribution priority, owner-by-owner Interactions snapshot reconciliation, final-envelope lifecycle authority and the [upstream-first counting policy](architecture.md#token-counting-policy) are selected, not new alternatives to debate. Selected Google basic correction/history loops and snapshots are complete within the evidence below. Counting remains active; further Google report/API work is [paused](#google-api-pause), with incomplete coverage explicitly accepted.

Model-effective value guards are implemented in the [shared target encoders](architecture.md#model-effective-values), with candidate-local configuration that inherits and may narrow catalog limits. Both existing Gemini 3.8 Flash bindings declare low/medium/high and a 65,536 output maximum from [dated model evidence](refs/2026-10-17-generation-contracts.md#model-effective-limits-2026-10-20). Focused checks cover final mapped values, retained effort updates, inserted Messages defaults and preflight/preparation agreement. The full offline baseline, pinned synthetic SDK loopback and Python checks passed; no new binding, installed-client rerun, live Provider enforcement or Google full-loop acceptance follows.

Messages [missing-cap defaults](architecture.md#missing-output-ceilings) are implemented under the user-approved configuration and constructor contract. The Go/Haiku binding explicitly retains `32000`; generic targets and pure codecs have no fallback. Focused checks cover inheritance/clearing, invalid declarations, explicit precedence, converted budgets, candidate isolation and preflight/prepared values and receipts. The full offline baseline and pinned synthetic SDK/Python gates passed; no installed-client rerun or real Provider acceptance was performed for this slice.

Selected [cross-protocol attribution](architecture.md#end-user-attribution) is implemented over the existing identity owners, including generateContent's fixed safety label and separate identity/legacy-cache loss evidence. Directional checks cover native OpenAI preservation, Messages↔OpenAI, safety priority, null/empty/absent, source/target bounds, Unicode label rejection and preflight/preparation agreement. Current sources expose no simultaneous same-priority aliases; duplicate raw keys reject without a speculative alias framework. The full offline baseline and pinned synthetic SDK/Python regressions passed. New projection evidence is synthetic, not real Provider attribution enforcement, installed-client acceptance or Google full-loop acceptance; earlier model/default protections remain intact.

| Package | Dependency and work | Exit condition |
|---|---|---|
| S3-6 Token counting | Select one model/target profile, upstream reliability criteria, coarse estimation rule and public provenance reporting; implement the independent Messages operation over its effective-input projection | Independent request/result oracles cover upstream preference, explicit estimation for inaccurate reports and input coverage; no inference or gateway tokenizer execution. Update implemented OpenAPI and run applicable SDK/real gates; Responses counting is added only if separately selected |

Counting may proceed once its own projection is stable rather than waiting for every cross-protocol pair or unresolved Google report profile. Unsupported protected semantics continue to reject; completing the selected slices does not imply full official-protocol coverage.

Google maintenance now checks required lifecycle identities/statuses and all known continuation owners, rejects uncarried execution/probability/terminal reports, and preserves tool-part order through native references. Source checks also exposed an optional chosen-probability default that fabricated zero; missing now rejects while sparse alternatives retain unknown facts. The subsequent user choice permits six-status checkpoint observations without inferred transitions or terminal equality; updates do not finish the stream or override final call/progress validation. Focused semantic/Router and full offline checks passed; pinned SDK/Python regressions passed for maintenance and SDK regressions passed again after the lifecycle selection. These do not establish new Google SDK/client/Provider acceptance. No new report structures have been implemented. Scope excludes queued/background execution, recovery, remote cancellation, requested-logprob admission expansion and unrelated media/hosted tools.

The generateContent single-call correction/final-history slice passes Router and pinned SDK JSON/SSE checks over one candidate, plus a separate CLI-loaded live SSE run on Gemini 3.8 Flash: four Chat requests followed by four Responses requests, 2048 output tokens and 45 seconds per request, one attempt, no SDK retries. Both profiles completed error feedback, corrective call, final answer and returned final history.

Interactions supplied-step reconciliation, [sparse lifecycle metadata](architecture.md#google-foreground-intake) and the selected [call-only signature carrier](architecture.md#reasoning-replay) are implemented. Router and pinned SDK JSON/SSE correction loops preserve distinct thought/call signatures, summary/argument fragments, snapshots and final history. SDK evidence also checks indexed trailing manifests and non-repeated constant format metadata. A separately bounded native v1 run passed Chat 4 + Responses 4 SSE requests through error feedback, correction, final answer and subsequent history. Multipart content and service reports remain rejected; model fallback and new report structures remain unselected. See the [source/version boundaries](refs/2026-10-17-generation-contracts.md#interactions-partial-opening-and-service-evidence-2026-10-20).

On 2026-10-21, installed pi 1.3.0 through Chat and OpenCode 2.0.32 through Responses each passed a four-request live loop against each Google API: actual missing-file error → corrective read → final answer → next-turn history. All 16 accepted-case requests used the project CLI loader, one selected candidate/attempt, low effort, 2048 output tokens and 45-second upstream deadlines, with no extra client transmissions and zero ignored request fields. Delivered signature manifests, call/result IDs and arguments survived client history reconstruction. The [dated evidence](refs/2026-10-17-generation-contracts.md#google-installed-client-acceptance-2026-10-21) separates the earlier connect timeout and harness-oracle failure from successful runs. The bounded guard buffered SSE before unchanged delivery to validate tool paths, so this is not incremental-publication timing evidence. Multi-call ordering, every client/protocol combination, all native fields, real accounting accuracy and cross-account failover were not established. Google client/static intake remains closed; Messages was accepted only by the later selected profile below. The [OpenCode version-continuity decision](architecture.md#scope-and-activation-decisions) preserves earlier results without version-driven script changes.

The separately requested [pi Messages probes](refs/2026-10-17-generation-contracts.md#google-messages-client-boundary-probes-2026-10-21) did not pass. Adaptive/low response-effort requests rejected before upstream I/O on both Google bindings. A basic profile without requested thinking/effort controls reached generateContent once and Interactions three times, all upstream 200, but failed Messages response projection: generateContent on its first tool turn, Interactions after two completed signed tool turns when producing the final answer. No final-history continuation was accepted. These failures preserve the existing scope/terminal boundaries and do not invalidate the accepted Chat/Responses loops or authorize a new effort/terminal approximation.

The user subsequently selected only [Google response-effort projection](architecture.md#google-response-effort-projection) and static diagnostics. Both bindings now accept the declared low/medium/high scope approximation, with independent Messages→IR and IR→Google expectations, admission/preflight/preparation agreement, loader inheritance/override and original/projected control receipts. Existing reasoning-scoped Chat/Responses requests and absent controls bypass the policy; invalid scope declarations cannot satisfy topology promises. Messages codec failures now have body-free terminal/usage classifications, tested with real codec rejection branches. The full offline baseline and pinned SDK/Python regressions passed. Terminal and accounting behavior are unchanged; no new installed-client or live Provider acceptance is claimed.

Further projection work must retain existing final-value and default assertions at the intended target boundary, not an earlier admission failure. Projection receipts must describe the checked request that preparation serializes. Reuse the encoder rules without retaining every candidate's serialized body; transport consumes the prepared request without reinterpreting its semantics.

The subsequent [Messages terminal profile](architecture.md#google-messages-terminal-projection) closes the selected Google basic loop. Both APIs pass independent delivery guards, Router and pinned SDK JSON/SSE correction/final-history cases. The full offline baseline and SDK/Python regressions passed. Two fresh pi 1.3.0 Messages SSE runs also passed, four serial requests per API, 2048 output tokens, 45 seconds and one attempt each; tool errors, correction, final answer, next-turn history, manifests and terminal receipts were checked. [Dated acceptance](refs/2026-10-17-generation-contracts.md#google-messages-basic-loop-closure-2026-10-21) keeps this eight-request result separate from earlier failures. No new report or usage policy was needed. Dual-call ordering has independent offline evidence, not new real multi-call acceptance.

#### Google API pause

The user selected basic-loop closure followed by a pause in further Google API improvement on 2026-10-21, accepting incomplete operation outside the selected profiles. Keep the accepted bindings, three-level effort mapping, terminal rules and replay protections; allow necessary correctness/security maintenance without expanding the feature scope.

Further service/terminal/probability report owners, general usage approximations, native Google client/static directions, cache/strict-tool/dynamic-control expansion, new models, modalities, hosted tools and background execution are unscheduled pending renewed selection. Existing unsupported semantics stay rejected or explicitly unassessed; a passing basic loop is not full Google conformance. The identified `serviceTier` version/coverage question also remains unresolved, not silently marked supported. These gaps no longer block the selected stage-3 scope. `count_tokens` remains an independent operation; selecting a new Google counting target is not implied by this closure.

#### Stage 3 counting scope

Complete `POST /v1/messages/count_tokens` during cross-protocol work, as an independent counting operation over one selected model/target profile. Follow the [counting contract](api-expansion-design.md#token-counting): share the rules that determine Generation's effective input, including instructions, ordered history, tool declarations/calls/results, media and input-affecting settings/defaults. Keep operation-specific validation and encoding separate; counting is not Generation with a zero output cap or a call to its full execution preparation.

Exit evidence must independently establish the count request's effective input and result meaning, and agreement with the selected generation projection on count-relevant input. Verify upstream preference and the selected coarse fallback for inaccurate reports, including its approximation/provenance reporting and input coverage. Do not run a tokenizer, create generation tasks, initialize unnecessary output decoders or maintain separate message/default rules. Counting can still require authorized resource access and preprocessing; verify applicable byte/time bounds and cancellation without moving I/O into pure codecs or treating no generation as no resource cost.

Follow the [selected counting policy](architecture.md#token-counting-policy): prefer upstream values; for known inaccurate reports, allow a documented rough estimate such as a character-based heuristic rather than tokenizer-based exact calculation. Identify model/target, projection scope and count source without exposing trusted routing or credentials. Reject inputs outside the selected profile, including media without an applicable counting/estimation rule. The proposed Responses counting companion remains a separate surface-selection decision.

### Stage 4 scope

The [API expansion design](api-expansion-design.md) supplies operation proposals, not implemented endpoints or approved type layouts. Keep conversational media in Generation where appropriate and independent operations in their own task contracts. The order below is recommended, with per-operation selection before implementation; stage-4 work does not block stages 1–3.

| Package | Dependency and work | Exit condition |
|---|---|---|
| S4-0 Next-operation design | Pin current official request/result/event contracts, choose one target and task/resource owners, and state exclusions. Select file authority early; select later operations just in time rather than requiring a complete redesign upfront | New semantic structures and execution-policy choices have explicit decisions. Existing owners/shared values are reused where appropriate; no universal task bag, repository-wide rewrite or implicit new binding |
| S4-1 Embedding | Extend the [independent path](architecture.md#independent-embedding-path) after its S4-0. Preserve text/token-ID alternatives, input/result association, dimensions, encoding and usage | One selected official contract works end to end; tokenizer scope, bounds and cancellation hold. Equal dimensions do not authorize cross-model failover |
| S4-2a Speech | Complete the selected static binary contract, then independently deliver progressive binary/SSE where supported. Preserve text, instructions, voice, speed, format and reported accounting | Static and streamed evidence remain separate; bounded buffering, cancellation and honest completion hold. No fabricated transcript/usage or implicit voice-resource creation |
| S4-2b Transcription | Complete file-based recognition with native multipart upstream preparation, static decoding and selected stream intake | Real reported text/timing/speaker/probability/usage facts survive; unsupported output formats reject instead of inventing timestamps. Large input and cancellation stay bounded; a reusable File object is not required |
| S4-2c Translation | Reuse the necessary audio-input infrastructure from S4-2b, but select and implement native translation as a distinct operation | Current model/language restrictions are rechecked; translation intent and results are preserved. No implicit ASR-plus-Generation substitute |
| S4-3 Images | Deliver Generate completion, direct-input Edit, then selected preview/final streaming as coherent sub-slices. Keep source order, mask association, controls and artifact identity | Independent generate/edit oracles and task-specific lifecycle checks pass. Previews do not satisfy final-image counts; direct inputs need not wait for Files, while file-reference forms wait for S4-4c |
| S4-4a Files | After selecting storage authority, public-ID authorization, private mappings and retention/reference policy, implement create/read/list/delete and content access | Access isolation, bounded streaming/spooling, deletion/expiry with active references and restart-consistent mappings are verified. No credential-owned upstream ID is accepted as public authority |
| S4-4b Multipart Uploads | Build on the selected file identity/lifecycle contract: create upload, add bounded parts, complete ordered parts, cancel and clean up incomplete uploads | Part identity/order, total bytes, expiry/cancel cleanup and uncertain remote completion are handled without blind replay or whole-file memory retention |
| S4-4c File-reference integration | Connect authorized resource resolution to each selected task outside pure codecs; retain the task's existing direct-input path | Resource scope can restrict trusted targets but cannot supply origins/credentials. Selected file-backed operations work across restart and reject stale, unauthorized or incompatible references |
| S4-5 Video | Select concrete current input/output or production operations before choosing task/content and optional native-job owners. Implement their codecs, execution and artifact access | Independent request/result and applicable event/job evidence covers bounds, cancellation, artifact identity and honest completion. Async APIs explicitly handle submission uncertainty/status/cancellation; only file-reference paths depend on S4-4c |

Files design can proceed alongside the first operation slices; implementation need not delay direct-input media. Speech, image and file implementations may advance independently once their own contracts are selected, but shared-owner changes must stay coordinated. Native video/media job lifecycle belongs to that operation, not the general Batch API.

Optional Chat-carried audio is a separate slice after its task contract stabilizes: preserve the existing static path, then verify genuine upstream streaming if selected. It neither blocks the main audio path nor establishes Realtime. For every operation, update the OpenAPI/usage surface only when implemented, keep admission and actual Provider evidence separate, and use the [stage-specific verification](api-expansion-design.md#verification).

### Stage 5 backlog

**Status: unscheduled.** This is a future-work bucket, not a promised implementation queue after stage 4. Each task needs a selected design and explicit scheduling before work begins.

Reopening a task requires its client scenario, minimum semantic/resource dependencies, design choice and bounded acceptance contract. Do not prebuild these systems as stage-3/4 infrastructure. Native cache-only prewarming and other separately deferred features retain their own scheduling decisions; they are not silently added to this queue.

- Citations/source attribution, `tool_search`/ordered discovery and hosted tools retain their separate [design proposals](api-expansion-design.md#unscheduled-designs).
- **Server-side compaction:** include the explicit Provider-executed `/responses/compact` operation. Receiving/replaying compaction items remains existing Generation scope. Automatic compaction triggers, mutation of stored conversations and background execution are not implied by the explicit operation.
- **Batch:** select supported typed tasks, file dependencies, submission reconciliation, durable job identity, cancellation and per-record outcomes.
- **Realtime:** select client scenarios, session/transport ownership, event/media contracts, interruption, recovery and resource bounds. Stage-4 audio/video adaptation does not implement Realtime.

## Phase rules

### Generation design review gate

**Status: approved and closed for the declared stage-1/2 baseline on 2026-10-20.** The user explicitly approved the six-area baseline at `26b63eb`; [Architecture](architecture.md#scope-and-activation-decisions) records that decision. The earlier 2026-10-17 audit-coverage authorization was not approval; this later human decision closes the gate. The selected R1a/R1b/R2 loops, lifecycle reports and history shorthand retain their [bounded acceptance](#stage-2-work-plan).

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

The reported text-container gap has a [source-backed local repair](refs/2026-10-17-generation-contracts.md#responses-text-container-presence-2026-10-20), verified by the full offline baseline, pinned SDK loopback and Python checks. No installed-client or real Provider run was repeated for this repair. Earlier real-client/Provider results remain scoped to their recorded runs; this review does not establish R1b installed-client opaque replay, cross-account compatibility or real failover. Stage-3 snapshot/counting choices and later operation designs stay in their own slices. [Observability follow-up](#observability-follow-up) is not an additional approval gate.

The sourced [contract boundaries](refs/2026-10-17-generation-contracts.md) retain deferred counterexamples. Test counts, round trips, agent audits and passing coverage runs establish neither semantic completeness, design approval nor integration; a scoped design approval is not a defect-free implementation claim.

### Observability follow-up

**Direction selected on 2026-10-20; implementation deferred.** Follow [Observability](architecture.md#observability). After the relevant stage's functional acceptance, use concrete investigation gaps to select the smallest useful log/span/metric additions and any migration of existing diagnostics. Do not turn the audit checklist into a full instrumentation programme or a new acceptance gate.

Until then, keep observability changes to necessary maintenance for existing acceptance. Protocol, security and lifecycle defects remain subject to their own gates; deferring telemetry does not waive those contracts. The observability direction and the separately approved Generation baseline do not approve other unresolved audit proposals. No new dependencies, backend deployment or content export is selected by the observability decision.

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

- Google structured output and logprobs, and Messages request-wide automatic TTL controls. Messages owner-local markers, strict tools, ordered instructions and tool changes have the narrow [all-native configured admission](architecture.md#owner-local-cache-admission) on the Go/Haiku binding; mixed routes and the generic profile remain closed for those features. Exact beta carriers for per-message effort, block binding and tool changes are separately enabled by trusted binding policy. Missing carriers use the selected named projections where applicable; explicit block-binding `"error"` still rejects without a carrier.
- generateContent [cached-context references](architecture.md#cached-context-references) with exact identity/expiry and explicit clock checks. Other carriers reject; there is no HTTP field, authorization service or cache lifecycle operation.
- [Safety policies and reports](architecture.md#safety-policy-and-reports) on generateContent. Downstream delivery omits assessment-only reports under a named rule and rejects blocking reports and safety-bearing settings echoes. Requests without a policy may still receive reports; those exchanges are not an established full loop. Vertex severity/method carriers are out of scope.
- [Service policies](architecture.md#service-policies-and-reports). The native generation-only decode helper rejects an execution policy rather than dropping it. Messages service reports need full-message or opening usage; cross-contract classes and late reports without a carrier reject. Absent reports are never supplied from request controls.
- Chat custom tools have static JSON and history carriers but no pinned custom-call SSE delta carrier; custom Chat streams reject.

Explicit constrained-carrier rejections: Google JSON-object mode, schema references and unselected keywords; generateContent optional allowed-tool subsets and implicit object roots for function parameters; Interactions logprobs; Messages strict string enum/const, references and regex; explicit cache retention without an exact carrier or the approved `"24h"` target policy. No registered target selects that policy yet. Google probability intake accepts only unambiguous chunk-local text reports and has no token bytes, so the Responses probability carrier cannot deliver it.

P8 policy selection is complete. Responses normalization-default strictness never becomes a plain native tool, so pi-shaped requests omitting `strict` do not reach those bindings; explicit non-strict tools keep their carrier. Native codecs reject uncalibrated media detail/context and filenames without carriers. No cross-scope or numeric conversion activates without model-level evidence.

Terminal completion causes are independent of turn progress and tool continuation. Chat and Responses use the [selected terminal approximations](architecture.md#selected-request-approximations); Messages rejects ambiguous stops, recitation, unknown completed terminals without pending calls and explicit continuation. Google `STOP` stays a reported fact beside function calls, and Chat delivers it as `stop`. Gateway and SDK smokes retain exact tool-response carriers, not completed cross-protocol answer loops; signed Google history has pinned pi codec coverage, not full-request admission. The [Messages gateway smoke](../tests/gateway/anthropic.rs) checks owner-local markers and signed tool-result/final-answer history with a synthetic native upstream in JSON/SSE, not real client or Provider acceptance.

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

- **Messages target policy:** caller-selected request-wide automatic caching and sampling-policy placement remain separate from the owner-local and pi-profile admission. [Trusted missing-cap defaults](architecture.md#missing-output-ceilings) are implemented; do not infer automatic-cache or cached-context admission from this policy.
- **Model-effective value evidence:** the implemented [binding guards](architecture.md#model-effective-values) reject Gemini `minimal` and cap `65537` without changing generic carriers or selected mappings. Additional model declarations need their own evidence; a passing local rejection check is not Google full-loop or live enforcement acceptance.

After selecting a slice, require an independent static and streamed tool turn, a final answer and next-request replay, plus strict EOF, cancellation and no-post-publication-replay checks. Then run the full offline baseline and the connected SDK gate before separately bounded real-client/Provider acceptance. Only afterwards add controlled synthetic failover/affinity/cooldown scenarios under the unchanged routing contract. No completion or safety rule is weakened to make a loop pass.

#### Public capability and reconnection sequence

The [selected contract](architecture.md#public-capability-and-adaptation-notice) has an [initial partial surface](usage.md#capability-discovery-and-adaptation-notice); actual receipts stay internal. It does not establish model-effective support or full-loop availability. An acceptance boolean cannot distinguish exact verbosity support from the approved `gemini.omit-verbosity.v1` omission, and streamed usage may introduce losses after headers.

1. Establish model-level evidence and preflight/preparation agreement for each selected existing binding; reject uncalibrated values and stop at interfaces needing routing-policy changes.
2. Extend the capability view only with independently verified declarations. Do not activate bindings implicitly or treat an unlisted case as rejection.
3. Reconnect exact-carrier client loops first, then approved adapted paths. Verify tool calls, results, final answers and replay for JSON and SSE. Blocking safety reports and unsupported completion carriers remain rejected.
   Verify phase-only manifest return through the selected pinned clients; the codec carrier does not prove clients retain it. Before enabling extended-retention omission, review each target's default storage policy without treating cache presence as retention evidence.
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
| Complete citations and source identity | **Stage 5.** Select licence/quote/request-source owners and output point anchors, including the changed OpenAI file-citation index contract. Then independently cover request, output, streaming and returned history; retain rejection until represented. Messages citations have a separate [1C design](api-expansion-design.md#1c-design-messages-citations) |
| Execution controls and reports | **Unscheduled; not a stage-1 blocker.** Select bounded owners for geography, access programs and safety/review details only when a scheduled client sends them. Messages block binding already has typed ownership and selected carrier/fallback rules; input transformations use validated named omission, not exact report carriage. Interactions service expansion is paused; continuation execution stays excluded |
| Google terminal/probability report owners | **Paused after basic-loop closure; design remains unselected.** Keep the [bounded owner proposals](api-expansion-design.md#google-output-report-owners-unselected) and [streaming evidence](refs/2026-10-17-generation-contracts.md#streamed-score-scope-2026-10-20) without an implementation commitment. Rejection is not support; nonempty URL-context reports, hosted tools and complete citations remain excluded |
| Interactions service source baseline | **Paused; renewed selection required.** [Source-version discrepancies](refs/2026-10-17-generation-contracts.md#interactions-partial-opening-and-service-evidence-2026-10-20) still do not authorize service mapping or model fallback. Preserve rejection until a source-version profile and equivalent report semantics are selected |

### Scheduling and product decisions

| Question | Current behavior and recommendation | Blocks |
|---|---|---|
| Binding activation and reconnection (P7 follow-up) | The public capability/notice contract is selected. Complete model-level evidence and preflight/preparation checks, then specify bindings and client loops under the [reconnection sequence](#public-capability-and-reconnection-sequence). Do not reinterpret unsupported completion/safety carriers or reopen P8 policies | Binding activation and full-chain acceptance, not codec-only coverage |
| [Native cache-only prewarming](#deferred-cache-prewarming) | Priority and scheduling undecided; select the operation and result contract when reopened | That slice only |
| [Tool discovery and native `web_search`](#deferred-tool-discovery-and-web-search) | Stage-5 backlog, unscheduled; designs remain undecided | Those slices only |
| [Token counting](api-expansion-design.md#token-counting) | Select the concrete profile, coarse estimate rule and provenance reporting under the accepted upstream-first policy; no gateway tokenizer counting. Complete the Messages operation in stage 3; Responses `input_tokens` remains a proposed companion surface | Counting and its cross-protocol promises, not stages 1–2 |
| [API expansion](api-expansion-design.md) | Stage 3: counting. Stage 4: Embedding, audio/image/video and Files/multipart Uploads. Stage 5: server-side compaction, Batch and Realtime, all unscheduled | Each operation's own contract and activation |
| Files/multipart Uploads | Selected for stage 4. Choose storage authority, authorized public IDs, upload lifecycle, retention/deletion and private upstream mappings. Current inline/URL input and rejection of credential-owned references remain until implemented | File resources and their dependent operations |
| Server-side compaction | Stage-5 backlog, unscheduled. Keep the explicit `/responses/compact` operation separate from automatic triggers and existing compaction-item replay | Compaction execution, not existing replay or stages 1–4 |
| Batch | Stage-5 backlog, unscheduled. Select native job execution, durable acceptance/reconciliation and supported task/resource dependencies | Batch API and execution |
| Realtime | Stage-5 backlog, unscheduled. Choose a client scenario and official contract before deciding session ownership, transport, interruption and recovery | Realtime |

## Deferred work

### Deferred Generation semantics

These are explicit gaps, not claims of complete official API coverage:

- [Native cache-only prewarming](#deferred-cache-prewarming) and [tool discovery and native `web_search`](#deferred-tool-discovery-and-web-search).
- Messages citations ([1C](api-expansion-design.md#1c-design-messages-citations)) remain unscheduled stage-5 work; carrying requests and responses reject. Separately scheduled Claude Code sampling may motivate that slice.
- Messages document `context` rejects until its model-visible semantics have an IR owner.
- Messages turn-scoped system messages (`clear_at`) have no IR owner and reject.
- Generated media beyond Chat audio need a selected output, stream and replay design. Other hosted tools remain deferred; observation types do not imply an end-to-end contract.
- Chat participant `name` has no IR owner and rejects. Chat `store: true` rejects; there are no stored completions.
- Messages codecs carry `top_k`; the selected fixed-sampling Go/Haiku binding rejects it. Tokenizer-unbound `logit_bias` rejects.
- The wider budget-scope exception for manual interleaved thinking remains deferred. S1-3 supports only ordinary manual-budget relations; this does not disable adaptive thinking or existing tool loops.
- Gemini finish reasons without content semantics (`MALFORMED_FUNCTION_CALL`, `UNEXPECTED_TOOL_CALL`, `TOO_MANY_TOOL_CALLS`, `LANGUAGE`, `OTHER`) stay attempt failures under the routing freeze.
- Tool-result video has no carrier on any bound target.
- Multi-candidate execution remains deferred; file services wait for their selected stage-4 packages.

### Deferred cache prewarming

Native cache-only prewarming is unscheduled and is not a prerequisite for stages 1–4 or P7. Existing cached-context references retain their own contract. The [operation proposal](api-expansion-design.md#deferred-cache-prewarming) distinguishes native no-output prewarming from client-owned one-token generation; select its priority, operation/result owners and acceptance scope before reopening.

### Deferred tool discovery and web search

These are unscheduled stage-5 tasks. Select ordered discovery/availability and native Provider-search contracts separately; the [design proposals](api-expansion-design.md#deferred-tool-discovery-and-web-search) retain the required ordering, citations and replay boundaries. Neither authorizes a gateway tool registry, search executor or silent text/function-call substitution.

### Messages citations

The [1C proposal](api-expansion-design.md#1c-design-messages-citations) remains unselected and unscheduled. Citation-bearing requests/responses keep their rejection until source identity, quotes, request flags and output anchors are represented. Claude Code acceptance may motivate this slice, but is itself separately scheduled.

## Deferred acceptance

| Work | Scheduling | Exit condition when reopened |
|---|---|---|
| Non-image multimodal real checks | With the relevant stage-4 slice; earlier selected-carrier regression remains required | One bounded real check per selected audio, video or document carrier, using model-issued history for tool results; no per-model or format matrix |
| Pinned-client image replay | Conditional | If the installed binary differs from the pinned source, run the bounded image tool round trip with a binary built from that revision |
| Real failover | Separately scheduled routing acceptance after selected client loops; not implied by stage 3 | One bounded run for a model with two credentials or Providers and a reproducible advanceable failure, such as an exhausted key in a scratch store |
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
