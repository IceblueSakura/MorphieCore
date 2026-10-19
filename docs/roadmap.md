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
  - [Temporary routing freeze](#temporary-routing-freeze)
  - [IR and codec migration](#ir-and-codec-migration)
  - [Verification and integration boundary](#verification-and-integration-boundary)
- [Integration handoff](#integration-handoff)
  - [P7 boundary review](#p7-boundary-review)
  - [Routing acceptance — pending](#routing-acceptance--pending)
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
| Routing | Functional phase complete under the [stable routing baseline](architecture.md#failover); feature development closed | [Routing acceptance](#routing-acceptance--pending) after the selected stage-2 loops |
| Generation IR/codec | Selected stage-1 codec implementation complete, with independent native oracles, stored/stateless history parity and pinned SDK/client-carrier checks; [inventory](refs/2026-10-19-same-protocol-inventory.md) retains explicit exclusions | [Human design review](#generation-design-review-gate) and P7/per-loop readiness before stage-2 acceptance; no implicit activation |
| Client compatibility | Rejection audit and policy selection complete: [phase transfer/return](architecture.md#message-phase-transfer), guarded extended-retention omission and retained citation rejection | Client-carrier acceptance and target storage evidence during reconnection |
| Integration | R1 closure explicitly resumed on 2026-10-19; R1a's selected bounded loop passed, while R1b SSE is blocked by lifecycle reports | Select the [report design](api-expansion-design.md#lifecycle-settings-and-service-reports-unselected) before further R1b acceptance; no implicit admission or binding activation |
| Cross-protocol conversion and counting | Target-value limits, missing-cap policy and attribution rules are selected; implementation and Interactions snapshot design remain pending | [Stage-3 work plan](#stage-3-work-plan), with counting dependent on its own input projection rather than every conversion |
| Embedding, other modalities and file resources | Stage 4 includes audio/image/video API adaptation and Files/multipart Uploads | Follow the [operation work packages](#stage-4-scope); existing paths remain maintenance-only until their slices |
| Stage-5 follow-ups | Citations, tool discovery, hosted tools, server-side compaction, Batch and Realtime are recorded but **unscheduled** | No implementation queue or delivery commitment; select each slice before starting it |
| Real acceptance | R1a passed selected SDK/Provider JSON/SSE and installed pi/OpenCode three-turn loops on 2026-10-19; R1b JSON passed but SSE rejected. R2 has request-profile evidence only | Finish R1b after design selection, then R2, routing acceptance and [deferred acceptance](#deferred-acceptance) |

The user explicitly resumed R1 closure on 2026-10-19, then selected analysis only for the newly observed R1b report boundary. This does not approve that design or close the overall Generation review. The user separately resumed Messages owner-local cache admission on 2026-10-19; its [all-native configured contract](architecture.md#owner-local-cache-admission) is implemented without activating bindings or changing routes. Other real acceptance gates remain open. Google value guards and Interactions snapshots belong to stage 3: no Google client endpoint forms a same-protocol pair. Provider-specific reset dialects, persistence and cooldown tuning need separate evidence and approval; no background probes, balancing or multi-round retries are scheduled.

Phase completeness covers the declared feature scope, not all official Generation semantics; explicit rejection is safer than silent loss but is not completeness. Chat Completions, Responses and Messages are equally important for same-protocol request, response, static delivery, streaming and history replay. Google generateContent and Interactions supply upstream request encoding and SSE intake only; record missing directions rather than implying a complete same-protocol pair.

## Planned order

| Stage | Goal | Exit evidence for a selected slice |
|---|---|---|
| **1 — Same-protocol lossless IR codecs** | Preserve the declared protocol semantics through client request → IR → same-protocol upstream request, and upstream response/events → IR → same-protocol client delivery/history. Prioritize known fields, presence/default meaning, ordered owners, reasoning/replay, usage and terminal facts over cross-protocol convenience | Independent wire→IR and IR→wire expectations in each applicable direction, JSON/SSE agreement and returned-history preservation. Report unsupported fields/directions and actual losses explicitly; round trips, HTTP success and named loss receipts are not lossless evidence |
| **2 — Basic agent tool interactions** | Ordinary function-tool declarations and schema intent, call IDs/arguments, results/errors, streaming, final answers and next-turn history for pi/OpenCode. The client executes its tools; no gateway tool executor is implied | Selected same-protocol tool-call → result → final-answer → replay loops, preserving source data and required reasoning/cache owners. Keep offline, SDK and bounded real-client/Provider evidence separate |
| **3 — Cross-protocol stability and token counting** | Stabilize selected request, response/event and history conversions through exact carriers or approved named adaptations; implement `count_tokens` with explicit model/tokenizer and effective-input projection scope | Independent directional expectations and selected loops; admission/preparation agreement and rejection of protected unrepresentable semantics. Counting has no inference side effects or silent estimation, and does not claim one target's count is exact for every fallback candidate |
| **4 — Embedding, modalities and file resources** | Adapt Embedding and audio/image/video APIs through task IR, codecs, execution and delivery; implement Files and multipart Uploads as resource services | Verify formats/dimensions, controls, results/events, bounds, cancellation, accounting and applicable SDK/real gates. File resources require authorization, bounded upload/part handling, identity, retention/deletion and restart-consistent mappings |
| **5 — Unscheduled follow-ups** | Citations/source attribution, `tool_search`, hosted tools, server-side compaction, Batch and Realtime | **Unscheduled:** select each scope/design and explicitly schedule its slice before implementation. No start/delivery commitment or prerequisite for stages 1–4 |

“Lossless” means semantic fidelity within an explicit protocol revision and feature domain, not byte-identical JSON or retention of unknown fields. Equivalent wire normalization is allowed. Preserve protocol records in their existing bounded owners; do not introduce raw passthrough or an IR extension bag. A same-named API at a different Provider is not proof of equivalent semantics.

Audit existing same-protocol loss paths and distinguish codec loss from model/product policy. Basic tool-field fidelity belongs to stage 1; stage 2 adds the agent interaction loop. Previously approved adaptations remain active until deliberately changed, but their occurrence cannot pass a lossless check. Deferred citations or other unsupported semantics remain explicit coverage gaps and reject where required; postponing them never permits silent loss or a claim of complete protocol coverage.

Design review and focused coverage proceed within each stage, not as a requirement to finish every later-stage design first. Keep the existing human approval, publication and integration gates. Preserve security, bounds, cancellation, replay isolation, honest terminals and existing regression assertions at every stage; later scheduling defers expansion, not necessary maintenance. Existing media carriers remain covered during stages 1–3; new modality APIs and execution paths belong to stage 4. Other-agent acceptance keeps its [separate scheduling](#deferred-acceptance).

The P7 boundary review becomes eligible after first-stage local completion, without waiting for deferred cache and tool slices, and is required before route-facing acceptance. Its analysis may proceed alongside design closure.

### Execution sequence and work-package rules

The packages below refine the selected scope, not design approval or binding activation. IDs name coherent deliverables, not a required commit count. Stage-4 ordering is recommended; exact operation contracts still need selection. Stage 5 has no execution queue.

1. Stage-1 selected codec work is implemented; use its [inventory and boundaries](refs/2026-10-19-same-protocol-inventory.md) for review and per-loop readiness. No all-official-semantics or real-loop completeness is implied.
2. Hand off **S1-1 → R1a** and **S1-2 → R1b** independently. A loop waits for its own carriers and review/admission gates, not every stage-1 package. Run R1a/R1b before R2.
3. R2 readiness concerns its implemented pi Haiku profile, not wider native manual-budget or sampling support. An observed defect in a carrier actually used by R2 must still be fixed before accepting it.
4. Complete selected stage-2 loops before routing acceptance and stage-3 conversion work. Keep separately scheduled real failover distinct from controlled synthetic recovery checks.
5. In stage 3, implement shared target checks before the paths that need them. Interactions snapshots and counting are separate branches; counting waits only for its selected effective-input projection. S3-3 builds on native identity preservation without treating it as cross-protocol attribution.
6. In stage 4, select the next operation's contract just in time. Design file authority early, but let direct-input Embedding/audio/image slices proceed without the complete file service. File-reference integration waits for authorized resource resolution.

Every implementation package follows the same bounded workflow:

- **Entry:** name the pinned source revision, dialect, feature domain, relevant binding/client profile, existing owners and unresolved decisions. Extend sourced `docs/refs/` notes for protocol findings; do not create run-report documents.
- **Delivery:** implement one coherent request/result/event/history or resource-lifecycle slice. Present a minimal counterexample and options before introducing an unselected semantic structure; a decision blocks only dependent work.
- **Evidence:** write independent directional expectations and focused failure checks at the responsible layer. Separate equivalent normalization, exact carriage, approved loss, rejection and unassessed cases. Prepare the [human review](#generation-design-review-gate) evidence incrementally; no passing test or agent review closes that gate.
- **Exit:** apply the [publication and integration gates](#verification-and-integration-boundary). Report codec, admission, SDK and real-client/Provider status separately, including blocked checks. Commit/push only after the applicable checks; remove completed packages from this page and keep accepted decisions in Architecture.

### Stage 1 work plan

**Status: selected implementation complete on 2026-10-19; overall human review remains pending.** Chat/DashScope–TokenPlan, Responses/OpenRouter, Grok and public OpenAI SIWC, and Messages/Go have independent request/result/event/history expectations within the [pinned inventory's](refs/2026-10-19-same-protocol-inventory.md) declared domains. Both Aliyun billing plans use the same dialect and shared-model controls without merging their authority. Google generateContent and Interactions still have no client/static pair; their guards, snapshots and conversions remain stage 3.

Accepted contracts live in Architecture: [Chat readable replay/history reading](architecture.md#chat-reasoning-extensions), [Responses item/event observations](architecture.md#responses-item-and-event-observations), [stored history](architecture.md#stateful-history), and [native Messages controls and partial-input boundaries](architecture.md#native-messages-projection). Completed work packages are removed here, not retained as another implementation specification.

The user-selected [Bailian compatible-request correction](architecture.md#shared-aliyun-billing-dialect) covers existing IR carriers and rejects ignored/unrepresentable controls. It does not claim the full DashScope API, turn native RPC into an endpoint or activate new model features. [Independent output ceilings](api-expansion-design.md#independent-output-ceilings-unselected) remain unselected design, not another stage-1 implementation package.

No new binding is activated. JSON/SSE equivalence applies only where each native schema has a carrier: event-only summary interruption/compaction progress are not a static event log; unfinished Messages arguments are native-SSE-only under the selected `max_tokens` contract and still reject as JSON/history. Manual-interleaved budget scope remains deferred, and adaptive-only Haiku still rejects manual thinking. Target-policy losses/default insertions remain named and are not exactness evidence.

Remaining work is review and integration acceptance, not more stage-1 carrier implementation: obtain the [overall human review](#generation-design-review-gate), complete P7 and per-loop readiness, then the selected stage-2 loops. Synthetic SDK/client-carrier and private visible-history probes do not accept real Provider behavior or signed account-compatible replay.

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

These packages implement the [selected reconnection contracts](#first-reconnection-slices), not a new admission policy. Protocol-local handoff does not bypass the pending human design review, P7 boundary review or binding evidence. R1a and R1b are separate acceptance slices; neither needs R2 or stage-3 conversions to succeed.

R1a's explicitly resumed bounded scope passed on 2026-10-19: DashScope Chat, ordinary non-strict functions, low effort, explicit total output cap and readable reasoning history. Independent Router/SDK JSON/SSE oracles cover tool/result/final/next history and failure boundaries. Live SDK JSON/SSE and installed pi 1.3.0 / OpenCode 2.0.6+e5ecb57 each completed their selected three-turn loops. Client runs used scratch state, at most three serial upstream requests, a 2048-token cap, 120-second request deadlines, one gateway attempt and no forwarded retry; successful checks reported zero ignored request fields. A streamed SDK accumulator and a pi assertion were corrected before their successful checks; those earlier harness failures are not Provider failures. This accepts only the tested profiles, not every effort alias, tool schema, client version or model feature.

R1b's real JSON stateless/stored three-turn checks passed with phase and usage; opaque reasoning appeared in the stored case, while summary content was not observed. SSE failed `reported_settings_conflict`; client SSE acceptance is blocked, not passed. The user requested [analysis only](api-expansion-design.md#lifecycle-settings-and-service-reports-unselected) for this boundary. Do not change report semantics or resume R1b real runs before selection.

| Package | Dependency and work | Exit condition |
|---|---|---|
| S2-0 Per-loop readiness | Repeat for the next selected loop after its [stage-1 handoff](#stage-1-to-stage-2-handoff). Confirm exact client request shape, model-effective carrier evidence, public admission and shared preflight/preparation behavior; classify every ignored field | The loop has an explicit carrier/profile contract and no unclassified ignored fields. Capability declarations distinguish exact support, adaptation and rejection. Any unsupported boundary is recorded without activating another binding |
| S2-2 R1b | **Blocked on design selection.** After S1-2 and S2-0, finish the selected OpenRouter Responses loop with explicit non-strict tools. Analyze opening/terminal reports before changing their interpretation | Preserve existing JSON evidence; after explicit design approval, verify independent SSE/lifecycle expectations and bounded pi/OpenCode/Provider loops. Keep stateless/stored history, phase-only manifest return where used and replay isolation |
| S2-3 R2 | After R1a/R1b and its own S2-0, accept the existing pi Haiku profile on the all-native configured route. Include a real session that changes tools, owner-local cache usage/TTL evidence and account-compatible signed-thinking replay | A tool/result/final-answer/history loop is accepted, not merely a request-profile probe. Missing cache reports remain unknown; accepted effort fields do not prove effective effort levels. Automatic-cache admission stays separate |
| S2-4 Routing acceptance | After the selected loops, exercise the unchanged routing contract through controlled synthetic failures; see [routing acceptance](#routing-acceptance--pending) | Conversation identity, exact-credential affinity, migration, all-cooling/`Retry-After`, cancellation and post-publication failure are checked. Client retries and gateway attempts are counted separately; real failover remains a separately scheduled run |

For each loop, establish independent static and SSE expectations before real acceptance, including tool errors, strict EOF, cancellation and no replay after publication. Use the [existing SDK and client procedures](development.md), not a new acceptance framework. Before every real run fix the target, request count, output cap and timeout; run serially through the project loader with no hidden retries and body-free reporting.

Extended-retention omission is not a prerequisite for these exact-carrier loops. If a later slice selects it, first verify that target's storage policy; cache usage is not retention evidence.

### Stage 3 work plan

The stage covers request projection, selected cross-protocol loops, Interactions intake and counting. Target-value rejection, trusted missing-cap defaults and attribution priority are already selected policies, not new alternatives to debate. Configuration details, model evidence, snapshot strategy and the concrete counting profile still need work.

| Package | Dependency and work | Exit condition |
|---|---|---|
| S3-1 Model-effective value guards | Implement bounded allowed-effort and maximum-output declarations in trusted binding policy. Apply them through the shared encoder after projection, including ordered updates and inserted defaults | Preflight/preparation agree; documented invalid Gemini effort/output examples reject at the intended target check. Keep protocol-wide carriers, source IR, existing approved mappings and route selection unchanged; no new clamping |
| S3-2 Messages missing-cap default | Select the bounded configuration shape and per-binding values; replace fixed `32000` insertion with the trusted default only when the source has no ceiling. Integrate with S3-1 final checks | Explicit caps survive; the default is distinct from the model maximum; incompatible ordinary budget/default combinations reject without adjustment. Named insertion evidence appears once on the candidate copy |
| S3-3 Cross-protocol attribution | Build on native identity preservation and the field inventory; implement the selected [end-user attribution](architecture.md#end-user-attribution) priority rules, including generateContent safety labels | Independent directional checks cover safety-ID priority, null/empty/absent, same-priority conflicts and target bounds. Source fields remain unchanged; identity loss and legacy cache-bucketing loss are separate. No inferred Interactions carrier |
| S3-4 Google → Chat/Responses loops | After S3-1 and the carriers required by each selected profile, verify generateContent → Chat, then Responses, one direction/binding at a time. Check request projection, tool results, final answers, usage and signed-history return | JSON/SSE loops pass applicable offline, SDK and separately bounded real gates. Preserve safety/terminal rejection and late-report honesty. Google client/static intake and Google → Messages final-answer conversion are not opened implicitly |
| S3-5 Interactions snapshots | Present the minimal streamed-owner/snapshot counterexample and select owner-by-owner reconciliation or snapshot-only materialization, then implement that design | Supplied content, signatures, errors and lifecycle facts cannot contradict earlier owners silently. Independent positive/negative event cases establish the selected mode; current rejection alone is not complete SSE support |
| S3-6 Token counting | Select one model/target counting profile and its public scope, then implement the independent Messages operation below using that profile's effective-input projection | Independent request/result oracles, no inference side effects, no silent estimates, and explicit model/tokenizer meaning. Update implemented OpenAPI and run applicable SDK/real gates; Responses counting is added only if separately selected |

S3-2 uses S3-1's final validation; S3-4 needs S3-3 only if its chosen profile exercises attribution. S3-5 is not a prerequisite for generateContent or counting. S3-6 may proceed once its own projection is stable rather than waiting for every cross-protocol pair. Unsupported protected semantics continue to reject; completing these packages does not imply full official-protocol coverage.

#### Stage 3 counting scope

Complete `POST /v1/messages/count_tokens` during cross-protocol work, as an independent counting operation over the selected model's effective input projection. Preserve count-relevant instructions, tools, media and settings; identify the native model/tokenizer scope and preserve the source's counting meaning. A count does not reserve the next Generation route or promise billing equality. Unsupported counting rejects rather than silently estimating or invoking inference. The [proposed Responses counting companion](api-expansion-design.md#token-counting) remains a surface-selection decision.

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

**Status: human design review pending; comprehensive audit coverage explicitly authorized on 2026-10-17.** The authorization opens coverage before design approval. It does not approve unresolved designs, certify design closure or reconnect integration. Existing loss policies may be reconsidered with evidence; consequential changes remain explicit user decisions.

The separate 2026-10-19 instruction to complete R1 authorized its selected reconnection/acceptance scope, not overall design closure. R1a's evidence is [scoped above](#stage-2-work-plan). The later analysis-only choice leaves the new R1b lifecycle-report proposal unapproved and its dependent acceptance paused.

| Review area | Contract and implementation entry points | Question for closure |
|---|---|---|
| Semantic ownership and bounds | [Semantic IR](architecture.md#semantic-ir), [request](../src/semantic/task/generation/request/mod.rs), [response](../src/semantic/task/generation/response.rs), [context](../src/semantic/context.rs) | Do ordered content, resources, requested controls and reported facts have typed owners without runtime state or dangling references? |
| Source parsing and target projection | [Adaptation](architecture.md#adaptation), [reasoning](../src/protocol/reasoning.rs), [OpenAI projection](../src/protocol/openai/lowering/projection.rs), native protocol modules | Are source validation, exact carriers, selected adaptations and rejection separate, with unchanged source IR and directional mappings? |
| Tools and schemas | [Strictness handoff](#integration-handoff), [tool types](../src/semantic/task/generation/tools/mod.rs), [Messages strict schema](../src/protocol/anthropic/strict_schema.rs) | Are explicit strictness, normalization defaults, call/result identity and unsupported schema constraints preserved? |
| Streaming, terminal and reported evidence | [Delivery](architecture.md#streaming-and-delivery), [OpenAI terminal](../src/protocol/openai/terminal.rs), [Messages delivery](../src/protocol/anthropic/client.rs), [Google safety](../src/protocol/google/safety.rs) | Do JSON/SSE agree on protected facts, repeated reports, continuation and honest termination, including explicit unrepresentable conversions? |
| History and replay | [Replay](architecture.md#reasoning-replay), [context transforms](../src/semantic/task/generation/request/context_transform.rs), [Responses storage](architecture.md#stateful-history) | Are content ownership, deployment isolation and required replay preserved separately from TTL history and affinity? |
| Public and integration boundary | [Integration handoff](#integration-handoff), [capability contract](architecture.md#public-capability-and-adaptation-notice), [topology compilation](../src/topology/compile.rs) | Is codec representation distinguished from default admission, model evidence and accepted client loops? |

The sourced [contract inventory](refs/2026-10-17-generation-contracts.md) holds the remaining semantic counterexamples. Test counts, round trips, agent audits and passing coverage runs establish neither semantic completeness, design approval nor integration.

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

**Status: the initial boundary and sequence were selected on 2026-10-19 for R1a/R1b only.** This closes the first-stage scope choice, not the overall Generation design review, full-loop acceptance or later admission decisions.

Entry condition: the first-stage IR/codec scope passes its local gates, and incompatible carriers and frozen admission boundaries are explicit. A connected end-to-end gateway is not a prerequisite.

The review must:

1. Separate exact capability, approved lossy acceptance and explicit rejection in the proposed public contract.
2. Check preflight/preparation agreement and identify the model-level evidence needed before activating existing codec support.
3. Verify the stable routing contract alongside each candidate's binding evidence; cache or recovery success does not establish representability.
4. Specify the selected client-loop contracts and the offline, SDK loopback and separately scoped real-acceptance gates.

Exit condition: a selected boundary design and an explicit implementation and reconnection schedule. The review changes no routes and promises no conversion.

#### First reconnection slices

The 2026-10-18 review checked actual catalog targets, pinned client builders, official protocol/model sources and focused offline request probes. Preflight and preparation share encoding; selected first requests agreeing at those boundaries do not establish response delivery or a client loop. The [evidence and counterexamples](refs/2026-10-17-generation-contracts.md#p7-binding-evidence-2026-10-18) remain distinct from live acceptance.

| Order | Candidate contract | Remaining gate |
|---|---|---|
| R1b | Responses client → `gpt-6-luna` / `openrouter-responses`; explicit `strict:false`, `low`, no forbidden sampling/parallel controls, full stateless history | JSON loops passed; SSE remains blocked on the [unselected lifecycle-report design](api-expansion-design.md#lifecycle-settings-and-service-reports-unselected). OpenRouter's model listing and API name do not prove every Responses semantic. Keep retained-history and replay isolation gates |
| R2 | Messages client → `claude-haiku-5-5` / `opencode-go-haiku-messages`; pi's `anthropic-messages` profile with the built-in Haiku 5.5 compat: strict and deferred tools with eager input, adaptive thinking with `drop_block`, per-message effort, mid-conversation system messages and tool changes, caller-supplied cap and owner-local cache markers on an all-native configured route | Request carriers reached Go unchanged in the bounded check. Remaining: a real-client session that changes tools, effective TTL/usage and account-compatible signed-thinking replay. Accepted fields do not show that effort levels take effect |
| Later | `gemini-3.8-flash` generateContent → Chat/Responses; explicit non-strict tools and supported controls | Implement the selected trusted-binding value guards with model evidence and finish the selected safety/terminal loop evidence. Interactions lifecycle snapshots and Google → Messages final-answer conversion remain blocked, not assumed equivalent |

Selected loop order: R1a/R1b before R2, within stage 2 after the [stage-1 handoff](#stage-1-to-stage-2-handoff) for their protocol, not ahead of same-protocol fidelity work or as a Provider/model matrix. Google → Chat/Responses conversion belongs to stage 3. Use one selected binding at a time without changing catalog ordering or activating alternatives implicitly. Do not use DeepSeek Responses as the initial faithful baseline: its documented role, reasoning and parallel-tool behavior needs separate directional policies. Coding Plan's output-cap semantics also need their own evidence; the DashScope fix is not a universal dialect change.

Separate policy and implementation work does not block the selected R1 scope:

- **Messages target policy:** caller-selected request-wide automatic caching and sampling-policy placement remain separate from the owner-local and pi-profile admission. The trusted target-default choice for missing output caps is settled; its stage-3 implementation remains separate. Do not infer automatic-cache or cached-context admission from these choices.
- **Model-effective value guards:** trusted-binding ownership and rejection without new clamping are selected. Implement bounded allowed-effort and maximum-output declarations, checked by the shared target encoder after projection, including ordered updates and inserted defaults. Retain protocol-wide carriers and existing selected mappings; keep model restrictions outside semantic IR and route selection. Reconsider field-specific clamping only after an actual compatibility problem and a separate policy decision. Gemini `minimal` and cap `65537` currently encode despite the model's documented restrictions; implementation and per-binding evidence remain pending.

After selecting a slice, require an independent static and streamed tool turn, a final answer and next-request replay, plus strict EOF, cancellation and no-post-publication-replay checks. Then run the full offline baseline and the connected SDK gate before separately bounded real-client/Provider acceptance. Only afterwards add controlled synthetic failover/affinity/cooldown scenarios under the unchanged routing contract. No completion or safety rule is weakened to make a loop pass.

#### Public capability and reconnection sequence

The [selected contract](architecture.md#public-capability-and-adaptation-notice) has an [initial partial surface](usage.md#capability-discovery-and-adaptation-notice); actual receipts stay internal. It does not establish model-effective support or full-loop availability. An acceptance boolean cannot distinguish exact verbosity support from the approved `gemini.omit-verbosity.v1` omission, and streamed usage may introduce losses after headers.

1. Establish model-level evidence and preflight/preparation agreement for each selected existing binding; reject uncalibrated values and stop at interfaces needing routing-policy changes.
2. Extend the capability view only with independently verified declarations. Do not activate bindings implicitly or treat an unlisted case as rejection.
3. Reconnect exact-carrier client loops first, then approved adapted paths. Verify tool calls, results, final answers and replay for JSON and SSE. Blocking safety reports and unsupported completion carriers remain rejected.
   Verify phase-only manifest return through the selected pinned clients; the codec carrier does not prove clients retain it. Before enabling extended-retention omission, review each target's default storage policy without treating cache presence as retention evidence.
4. Require the full offline baseline and connected SDK gate per slice, then schedule bounded pi/opencode and Provider acceptance.

### Routing acceptance — pending

Select a small set of admitted bindings and exact client carriers first. Do not weaken admission to make a loop pass.

- Verify pi/opencode conversation identity, tool-result/final-answer loops and exact-credential affinity on the selected protocols.
- Use controlled synthetic failures for client-visible failover, affinity migration, all-cooling responses, `Retry-After`, cancellation and post-publication termination. Count client retries separately from gateway attempts.
- Use separately bounded real Provider checks for failure scopes and reported cache usage. Missing cache usage is unknown, not a miss.

Exit condition: selected client loops and recovery scenarios have explicit results, offline and connected SDK gates stay green, and unverified bindings remain identified. Follow the [real-client and Provider procedures](development.md); this stage authorizes no deployment or routing features.

## Pending decisions

The [settled decisions](architecture.md#settled-decisions) remain the baseline; the audit permits evidence-backed reconsideration of lossy mappings. New semantic or execution-policy structures need a selected design. Continue independent work while a choice blocks only its own contracts.

### Ownership and projection decisions

Minimal counterexamples and pinned sources are in [Generation contract boundaries](refs/2026-10-17-generation-contracts.md).

| Question | Recommendation and exit condition |
|---|---|
| Responses lifecycle settings/service reports | **R1b blocker; analysis only.** Compare strict rejection, selected field resolutions and bounded phase-tagged observations in the [proposal](api-expansion-design.md#lifecycle-settings-and-service-reports-unselected). Select ownership, exact transitions and JSON/SSE/history projection before semantic changes; do not use last-wins or weaken repeated-fact checks |
| Complete citations and source identity | **Stage 5.** Select licence/quote/request-source owners and output point anchors, including the changed OpenAI file-citation index contract. Then independently cover request, output, streaming and returned history; retain rejection until represented. Messages citations have a separate [1C design](api-expansion-design.md#1c-design-messages-citations) |
| Execution controls and reports | **Unscheduled; not a stage-1 blocker.** Select bounded owners for geography, access programs, safety/review details and Interactions reported service/continuation only when a scheduled client sends them. Messages block binding already has typed ownership and selected carrier/fallback rules; input transformations use validated named omission, not exact report carriage. Keep other protected unsupported carriers rejected; do not add routing or continuation execution implicitly |
| Interactions lifecycle snapshots | **Stage 3.** Compare supplied steps against streamed owners, including content/signatures/errors, or select snapshot-only materialization. Current rejection prevents contradiction loss but is not complete SSE support |

### Scheduling and product decisions

| Question | Current behavior and recommendation | Blocks |
|---|---|---|
| Binding activation and reconnection (P7 follow-up) | The public capability/notice contract is selected. Complete model-level evidence and preflight/preparation checks, then specify bindings and client loops under the [reconnection sequence](#public-capability-and-reconnection-sequence). Do not reinterpret unsupported completion/safety carriers or reopen P8 policies | Binding activation and full-chain acceptance, not codec-only coverage |
| [Native cache-only prewarming](#deferred-cache-prewarming) | Priority and scheduling undecided; select the operation and result contract when reopened | That slice only |
| [Tool discovery and native `web_search`](#deferred-tool-discovery-and-web-search) | Stage-5 backlog, unscheduled; designs remain undecided | Those slices only |
| [Token counting](api-expansion-design.md#token-counting) | Complete `count_tokens` in stage 3 with a target/projection-aware counting contract. Responses `input_tokens` remains a proposed companion surface | Counting and its cross-protocol promises, not stages 1–2 |
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
