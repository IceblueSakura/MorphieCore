# Roadmap

This page owns current scope, priorities, dependencies, and exit gates. [Architecture](architecture.md) owns accepted design, [API expansion design](api-expansion-design.md) owns unselected proposals, and [Development](development.md) owns verification procedures. Code and tests define behavior. Completed slices are removed; Git preserves history.

## Contents

- [Current status](#current-status)
- [Account rewrite execution order](#account-rewrite-execution-order)
  - [Ordered work and exit gates](#ordered-work-and-exit-gates)
  - [Pre-Stage-4 real-Agent scenario sweep](#pre-stage-4-real-agent-scenario-sweep)
  - [Concentrated repair plan](#concentrated-repair-plan)
    - [Next-stage analysis](#next-stage-analysis)
    - [Partial-delivery exit](#partial-delivery-exit)
  - [Verification and operator handoff](#verification-and-operator-handoff)
- [Three-protocol fidelity repair](#three-protocol-fidelity-repair)
  - [Scope and boundaries](#scope-and-boundaries)
  - [Reference strategy](#reference-strategy)
  - [Agent-default production findings](#agent-default-production-findings)
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

**Current focus (2026-11-07):** Joint review has grouped the [AR5 evidence](ar5-review.md) by control adaptation, admission ownership, response/history carriage and stream closure. The user reaffirmed semantic matching and recorded low-loss conversion over excessive rejection, and accepted partial delivery. The [concentrated repair plan](#concentrated-repair-plan) prioritizes concrete root causes; uncertain cells move to next-stage analysis without blocking independent delivery. Implementation and affected-path verification remain pending; Stage 4 resumes at the [partial-delivery exit](#partial-delivery-exit).

| Priority | Remaining work | Exit |
|---|---|---|
| Conditional tool follow-ups | [S3-T5–S3-T7](#conditional-stage-3-follow-ups): namespace aliases, custom-text wrappers and examples projection | Separate policy selection and full affected-loop acceptance; not default prerequisites for AR5 |
| Account and Agent acceptance | Deliver the concentrated root-cause repairs with low-loss adaptation | Principal workflows and affected paths verified; remaining boundaries explicitly deferred under the partial-delivery exit |
| API expansion | Select and implement Stage-4 operation contracts after AR5 | Per-operation lifecycle, protocol and SDK acceptance |
| Downstream integration | Implement opt-in per-Agent JSONL session storage | Selected isolation and lifetime bounds |
| Production preparation | Decide diagnostic packaging and deferred observability scope | Explicit deployer selection |

## Account rewrite execution order

The remaining gate for the accepted [account specification](provider-account-redesign-plan.md) is real acceptance on fresh credentials and selected models. The user selected **offline three-protocol preparation before fresh credentials** on 2026-11-03; that default-profile preparation is complete. Setup uses the [Provider-owned, alias-only credential contract](provider-account-redesign-plan.md#credential-kind-and-header-placement). No compatibility mode, private-store migration or deployment is included.

### Ordered work and exit gates

The identifiers below belong only to planning and acceptance documentation, not production modules or test names.

| Order | Work | Completion evidence |
|---|---|---|
| **AR5 — Fresh credentials, model selection and real acceptance** | Fresh-account preparation and the initial broad sweep are complete within their recorded profiles. Apply the [concentrated repair plan](#concentrated-repair-plan), using focused investigation for unresolved boundaries and rechecking changed workflows. | Meet the [partial-delivery exit](#partial-delivery-exit): principal workflows work, losses are recorded, affected paths are verified, and remaining gaps have explicit next-stage dispositions. Production activation remains separately approved. |

Selected upstream lanes (2026-11-06; Bailian added 2026-11-07):

| Provider | Credential kind | Public model | Upstream binding |
|---|---|---|---|
| `openai` | API key | `gpt-6-luna` | `openai-luna-responses` |
| `opencode-go` | API key | `claude-haiku-5-5` | `opencode-go-haiku-messages` |
| `google` | API key | `gemini-3.8-flash` | `google-gemini-3.8-flash-generate-content` |
| `google` | API key | `gemini-3.8-flash` | `google-gemini-3.8-flash-interactions` (isolated acceptance lane) |
| `grok` | OAuth | `grok-4.7` | `grok-responses` |
| `deepseek-beta` | API key | `deepseek-v4.1-flash-1017` | `deepseek-beta-chat` |
| `aliyun-dashscope-cn` | API key | `qwen3.8-flash` | `dashscope-flash-chat`, `dashscope-flash-responses` |
| `aliyun-dashscope-cn` | API key | `glm-5.3` | `dashscope-glm-chat`; native Responses companion only |
| `aliyun-dashscope-cn` | API key | `deepseek-v4.1-flash-1017` | Native Chat/Responses companions only; no production binding |

Fresh local provisioning, Google key replacement and account-reference validation are complete. The selected [Responses replay authority](architecture.md#reasoning-replay) is implemented. The DeepSeek publication keeps canonical/upstream `deepseek-flash`; its label does not pin that upstream alias.

Observations and concise passing results have one evidence home in [AR5 Review](ar5-review.md). This roadmap owns their execution/defer dispositions; [Architecture](architecture.md#conversion-ladder) owns accepted conversion policy. Historical samples retain their original scope as repairs and focused supplementary checks proceed.

The user selected a separate six-cell [Bailian model/protocol sweep](ar5-review.md#bailian-model-and-protocol-coverage-2026-11-07), retaining model-specific controls rather than a Provider-wide support assumption. Native SDK evidence, installed-pi gateway acceptance and catalog gaps stay separate. Missing binding activation is a separate follow-up, not a prerequisite for repairing existing bindings.

### Pre-Stage-4 real-Agent scenario sweep

**User-selected sequence (2026-11-06):** Test common coding-Agent workflows against real upstreams, primarily through pi, before selecting concentrated product repairs. Use OpenCode to cover distinct client serialization, history, compaction and error-handling behavior. Other Agents require a concrete intended workflow and their own admission prerequisites; this sweep does not implicitly activate deferred Claude Code acceptance.

Coverage follows workflows and semantic boundaries, not a Cartesian product of every setting, client and Provider. The inventory below guided the initial collection; remaining gaps now take the [repair or deferral dispositions](#concentrated-repair-plan), rather than requiring another complete sweep before repairs. Keep the selected upstream lanes above and reuse still-applicable passing evidence with its exact version/profile. Exercise shared workflows on successful representative lanes, and repeat where a change affects native reasoning, signatures or client behavior.

| Scenario family | Required real coverage |
|---|---|
| Default delivery and controls | Ordinary streaming text, no-tool answers, actual model selection/public labels, default tool declarations and strictness, system/developer instructions, and same-session follow-up across the three client protocols. Qualify selected cross-protocol lanes separately. |
| Thinking levels and reasoning summaries | Characterize the current client's exposed levels and actual wire first. On representative native reasoning families, exercise every exposed thinking level, including off/default where available, and an in-session change after tool history. Separately test absent/default and exposed `auto`/`concise`/`detailed` reasoning-summary controls on Responses. Use a labelled SDK companion for controls the Agent cannot emit; record admission/upstream failures without lowering the requested level. |
| Real coding and tool repair | In a test-owned workspace, inspect multiple files, actually edit/write code, run a bounded local check, return a local tool failure, repair it and verify the result. Include repeated/multiple calls, argument fidelity, call/result identities, ordering and a final no-tool recall. Observe actual concurrency rather than requiring the model to emit parallel calls. |
| Tool and instruction changes | Change active tools, instructions and effort using supported client operations after an initial tool round; verify subsequent declarations, retained historical controls/signatures and recall. Cover the real tool schemas and strict declarations used by the client rather than only a minimal read schema. |
| Session history and user intervention | Multi-turn recall, persisted client-session restart/resume, legitimate earlier-turn edit/rewind or branch, steering/follow-up during work, and an explicit model switch with existing history. Use current client operations; observe incompatible replay boundaries without stripping returned state. Check separate-session isolation with interleaved conversations. |
| Long sessions and compaction | Retain ordinary manual-summary evidence; add threshold-triggered automatic compaction, split-turn retention, a second compaction, and subsequent real tool/recall turns. Cover summary cancellation/failure and recovery where reachable within a bounded run. Use ordinary Generation summarization on the selected model; native compaction stays deferred. |
| Cancellation, limits and recovery | Retain first-text cancellation; add cancellation during thinking, tool-argument generation and a safe local tool execution where observable, followed by same-session recovery with one gateway permit. Exercise a deliberately small output cap and incomplete tool input without execution. Record naturally encountered upstream errors and bounded error-path resubmissions. |
| Existing input and settings boundaries | Exercise Unicode/multiline content, bounded larger file/tool results, actual cache/session carriers and reported usage, plus the already supported image-input/read path on a selected image-capable profile. SDK companions cover explicit structured-output/tool-choice controls absent from the Agent UI. New modality APIs, counting and hosted tools remain deferred. |
| Replay and account boundaries | Recheck signed/readable/opaque history through actual continuations. Account-compatible replay/failover needs independent compatible accounts and observed replay; keep R09 prerequisites explicit. Different-model switching belongs to session behavior, not same-model failover acceptance. |

Execution order and checks:

1. **Characterize and inventory:** Record current unwrapped versions, exposed operations/settings, exact profiles and reusable evidence. Extend existing isolated drivers only where needed. Check actual client wire against public source or a synthetic authority before spending live requests.
2. **Run the broad sweep:** Start with native/default controls and thinking/summary levels, then coding/tools, history changes, compaction and cancellation. Each run has fixed target, request/attempt budgets including auxiliary calls, output/wire limits and deadlines. Check filesystem/tool outcomes, client terminals, actual upstream controls and continuation history; keep SDK-only evidence separate.
3. **Collect before repairing:** Add redacted findings and concise passes only to [AR5 Review](ar5-review.md). Distinguish observer/configuration errors, local admission, upstream rejection, delivery/replay failure and unmet prerequisites. Correct a broken test observer or test-owned configuration separately; do not patch product semantics, weaken guards or rewrite history to continue the sweep. Stop only the dependent cell and continue independent workflows.
4. **Joint root-cause review:** The 2026-11-07 review produced the plan below. Resolve remaining field/event ambiguities only where they decide a current repair; investigation is local to that slice, not a global entry gate.
5. **Concentrated repairs and exit:** Implement the current slices with focused regressions and required offline/SDK checks. Rerun affected real scenarios and newly reachable dependents. Use the partial-delivery exit; unresolved cells may retain a limited profile and a next-stage analysis entry.

Risks that change execution: unavailable compatible accounts, no returned replay carrier, a client operation not exposed by the current version, or a required thinking/compaction budget exceeding the run's cap. Record these boundaries before selecting a separate bounded profile or an explicit deferral. A test-only reduced context threshold qualifies the trigger/rebuild path, not full model-context capacity. Do not induce throttling, rotate credentials or change host/deployed services for coverage. Controlled fault injection remains separately labelled from a naturally observed real-upstream failure.

### Concentrated repair plan

**Revised with the user on 2026-11-07; no implementation is claimed here.** Prefer the existing conversion ladder, binding dispositions, client profiles, projection owners and stream reducers. No new capability framework, replay store, retry layer or diagnostic platform is planned. Ordinary semantic mappings and low-loss control conversions proceed under the [accepted policy](architecture.md#conversion-ladder), including [output ceilings](architecture.md#output-ceiling-adaptation).

The first three slices form the current delivery focus. The final two are bounded extensions: complete a clear, locally supported mapping, or carry the unresolved portion forward without blocking the first three. Investigation for one slice must not gate the others.

| Order / observations | Root-cause repair and owner | Check that completes the slice |
|---|---|---|
| **1. Generation controls — R11/R15** | Select the actual DeepSeek Chat cap spelling through the existing dialect rule. Apply equivalent, narrower-scope or omitted ceilings through shared lowering with loss evidence. Configure the pi gateway profile to express off through the supported standard control rather than retaining unknown public fields. | Actual client serialization → prepared request → JSON/SSE result agrees on the selected disposition. Check a small cap on an equivalent carrier, scoped/omitted-cap evidence on incompatible targets, and explicit thinking disablement. Do not infer length from usage or output silence. |
| **2. Truncated tool streams — R20; R14 only if the same boundary is established** | Carry the trusted Go/Haiku intake selection into the Messages decoder. Normalize the observed open tool block + explicit `max_tokens` termination into incomplete input, preserving exact argument bytes and identity; reuse existing partial-input handling and require final message/EOF validation. Keep generic baseline decoding independent. | A source-shaped truncated stream produces an honest incomplete result, no partial tool executes, and the same-session follow-up works. Malformed input, wrong terminal and missing EOF keep their failure checks. Classify R14 separately unless its failing event/state matches. |
| **3. Admission ownership — R05/R17/R18** | Reconcile model defaults, binding/API dispositions and codec checks. Fix contradictory capability declarations and redundant policy gates at their owner; retain actual wire/schema grammar checks. Send carriable unknowns upstream and use selected low-loss mappings instead of blanket pre-rejection. | Discovery, planning and prepared bodies agree on the effective contract for the affected bindings. Check root/inline controls where they reach different code. Separate forced identity, cardinality, schema conformity and terminal reports; a known unavailable explicit guarantee remains a scoped limitation until its downgrade is selected. |
| **4. Delivery/history compatibility — R01/R04** | Separate unavailable replay carrier from invalid origin. Recommended R01 policy is whole-owner omission of uncarriable signed reasoning on Readable Chat, with bounded deferred publication until carriage is known; this profile change still needs its concrete selection. Qualify zero-byte, unbound text normalization first at the history oracle, then change target projection only if needed. | JSON, SSE and actual next-request serialization agree; meaningful text, tool links and replay dependencies remain intact. Do not equate whitespace, signed parts or boundary-bearing empty messages with absence. Defer R01's policy-dependent portion if unresolved. |
| **5. Google control projection — R05/R06** | Candidate extension: reuse source-timed configuration resolution and effective active-tool-set projection before Google root conversion. Select the concrete ordered-history loss first; do not replace this with taking the last marker or weakening strict tools. Keep existing managed signature continuation. | Active/pending timing, tool-feedback turns, current declarations and protected history remain correct in a bounded loop. Any unresolved signature/profile requirement stays outside this slice. |

Use focused, body-free diagnostics only where a decision needs them: safe known field paths, event/block kinds, terminal class and recognized upstream error category. Reuse existing error/evidence owners. R03/R19 may receive a narrow discriminating check alongside this work, but their investigation is not an initial gate. If the first boundary remains unclear, retain the observation and continue the independent slices rather than guessing a lossy repair.

#### Next-stage analysis

These entries are deferred investigation or activation scope, not failed prerequisites for the current delivery. At handoff, retain the affected profile, observed boundary, available working path and next discriminating check; update only the affected entry if new evidence changes its disposition.

| Observations | Retained boundary and next action |
|---|---|
| **R02; unresolved R01/R05/R06 portions** | Keep managed Google continuation available. Stateless part/call signatures need a selected client-retainable carrier; do not discard attachments or fabricate reasoning owners. Revisit the profile/ordered-projection decisions independently of other Generation work. |
| **R03/R14/R19** | Unknown Luna rejected field, intermittent recovery decoder failure and the remaining Bailian response rejection need a precise field/event boundary. R19 already has schema-alias handling: add a successful directional control and isolate the remaining shape before changing it. Existing native Luna/Haiku and Qwen workflows retain their recorded scope. |
| **R07** | Qualify the current OpenCode public-label declaration and full redacted client error before changing gateway model routing; the canonical-label workflow is already available. |
| **R08/R12/R13/R21** | Separate application resubmission, fact retention, formatting, new tool execution and terminal behavior in existing observers. Grok needs a fresh direct-upstream/prepared-request comparison; retain passing coding/controls. Use explicit tool withdrawal for a structural no-tool recovery guarantee, separate from prompt-following assertions. No prompt injection or automatic retry is planned. |
| **R09** | Account-compatible replay/failover awaits observed replay and independent compatible accounts. Record the unexercised boundary without creating aliases or reopening frozen routing to satisfy it. |
| **R16; broader R18/R19 guarantees** | Missing Bailian bindings and broader strict/schema/tool guarantees are separate next-stage selections. Qualify each API's model identity, controls, ceiling scope and history; native success does not activate production bindings. Existing ordinary loops remain usable within their documented profiles. |
| **R10** | Authorization prerequisite resolved; preserve historical blocked samples and later passing evidence, with no repair work. |

#### Partial-delivery exit

**User-approved 2026-11-07:** principal workflows are usable, selected low-loss conversions record their losses, affected paths are verified, and remaining difficulties have explicit scope for next-stage analysis.

1. Preserve the representative real tool/history paths across Haiku Messages, DeepSeek readable Chat and Grok Responses, plus the existing Qwen Responses coding path. Use installed pi/OpenCode for their affected client boundaries; reuse still-applicable evidence rather than requiring every client × model × protocol combination.
2. Complete and verify the delivered repair slices. A new ambiguity can defer the dependent subcase, with its retained behavior stated; it does not justify reporting an unverified repair as complete or delaying unrelated slices.
3. Apply required offline and applicable SDK gates before publishing implementation. Recheck changed real workflows, including honest incomplete/cancelled delivery and continuation where affected. No broad re-sweep is required merely to preserve test counts.
4. Classify each remaining observation as resolved, a scoped limitation, next-stage analysis, or an unavailable prerequisite. A recorded deferral is sufficient disposition; closing every R identifier is not an exit condition.
5. Resume Stage 4 after this handoff. Deferred AR5 analysis may be scheduled alongside relevant next-stage work and does not make Stage 5 features or production activation implicit.

### Verification and operator handoff

- Deliver current configuration examples, human-readable validation/status and explicit setup commands; secret values do not appear in source, command logs or reports. OAuth provisioning is an interactive managed login, not token copying.
- Keep [installed-client synthetic recovery](development.md#installed-client-synthetic-recovery) distinct from real Provider/default-Agent acceptance. Use currently installed Agents with isolated state, never downgrade them to source pins or alter their personal credentials.
- Fix target, request count, output cap and timeout before each real run, use the project loader and redacted reporting, and run serially without hidden harness retries. Use the [production-profile acceptance procedure](development.md#production-profile-acceptance).
- Do not stop/restart deployed services, overwrite old credential stores, revoke sessions or activate new bindings as a side effect of code acceptance. Deployment and any destructive private-data operation remain separately approved.
- Resolve Provider/profile-specific restrictions at their contract owner using source inspection and focused reproductions. Apply selected semantic/low-loss conversions rather than blanket admission bypasses or speculative compatibility layers; credential-publication failures retain their independent security boundary.

AR5 closure resumes the existing Stage-4 sequence, not a new API expansion.

## Three-protocol fidelity repair

Remaining stabilization covers the [default-profile findings](#agent-default-production-findings) and [fresh-account exit](#remaining-stabilization-exit) across standard Chat Completions, Responses and Messages. The tool/ordered-effort core is implemented. Authoritative contracts: [conversion ladder](architecture.md#conversion-ladder), [field handling](architecture.md#field-handling), [projection principles](architecture.md#clientir-projection-principles), and [stream normalization](architecture.md#semantic-stream-normalization).

### Scope and boundaries

- **Protocols:** Covers all three same-protocol paths and six cross-protocol directions across request decoding, IR ownership, target request encoding, upstream decoding, delivery, and history replay.
- **Domain:** Generation Text (instructions/roles, text/refusal, function tools/results, structured output, reasoning/replay, presence/settings, usage, errors, and terminal events).
- **Extension properties:** Unknown extension fields are accepted and discarded at legal positions; known fields follow the conversion ladder; unscheduled capabilities keep [retained rejections](#retained-rejections).
- **Precedence:** Plain-text token-cap output may omit item status under named rules; Responses metadata echoes the admitted request only when upstream metadata is omitted.
- **Budget direction:** Apply [output-ceiling adaptation](architecture.md#output-ceiling-adaptation); broader thinking-budget adjustment formulas remain unselected.

### Reference strategy

| Reference family | Target contracts |
|---|---|
| OpenAI and Anthropic official specs & SDKs (Python / TypeScript) | Request/history unions, presence vs null defaults, tool arguments, signatures, stream accumulation, and terminal events |
| WHATWG HTML EventSource & RFC 8259 | SSE framing, multi-line events, UTF-8/JSON boundaries, EOF, and resource bounds |
| LiteLLM & CLIProxyAPI | OpenAI/Anthropic bridge mechanics, output-index mapping, block scheduling, and usage provenance |
| Vercel AI SDK & OpenAI Agents SDK | Typed stream consumers, incremental tool assembly, and history materialization |
| Installed pi and OpenCode | Real-world CLI/TUI serialization, SDK event consumption, and signature replay |

### Agent-default production findings

The [native-default characterization](development.md#installed-client-synthetic-recovery) exercises all three protocols with pi 1.5.0 and OpenCode 2.0.32. All six default tool/history loops pass, including OpenCode Responses under the selected empty-annotations history rule. These synthetic checks do not qualify fresh credentials or actual Provider/model endpoints. Endpoint qualification and Chat usage-emitter work retain their separate scope.

| Lane | Remaining issue | Required resolution | Scope |
|---|---|---|---|
| Pi Responses | Current installed pi completes inline declarations, direct call/result, final answer and recall | Real SIWC endpoint acceptance remains separate from the synthetic CLI gate | AR5 |
| Responses client → OpenAI subscriptions, flat tools | SIWC inline `additional_tools` declarations and their direct call/result history are admitted; namespace requirements stay for root declarations | Real endpoint acceptance for the inline continuation path | Tool contract repair; real endpoint acceptance separate |
| Chat upstream usage | Duplicate nonnull final reports | Qualify at emitter | Outside stabilization slices |

### Conditional Stage-3 follow-ups

These existing-tool compatibility improvements belong to Stage 3, not modality expansion or hosted execution. They remain **conditionally scheduled after the core**, requiring separate policy selection. They do not block AR5 or Stage 4 by default; if a selected acceptance lane requires one, make that package an explicit prerequisite for that lane. Deferred packages remain here rather than being silently activated or treated as complete.

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
- Reuse the [offline baseline](development.md#offline-baseline) and [SDK gate](development.md#sdk-loopback-gate) per implementation slice. Real Provider/model checks remain AR5 with fresh credentials and separately bounded runs; synthetic acceptance does not certify endpoint behavior.

Risks that change the plan:

- Unknown root/inline precedence prevents computing a faithful source active set for colliding definitions; scope that approximation to known cases, while preserving native shapes when possible.
- Missing custom-SSE carriers or lawful returned-role replay prevent complete advanced-family loops. Keep those boundaries explicit; ordinary function work need not wait for a speculative transport.
- Explicit strict, selected-tool, cardinality or serial guarantees with no target carrier remain rejected unless that concrete guarantee downgrade is selected. Do not repair by schema deletion, automatic prompting or a new silent fallback.
- S3-T5–S3-T7 require their own policy selection and affected-loop acceptance; textual tool-history annotations, media relocation and grammar-to-prose remain outside the scheduled packages.

Tool discovery, hosted/programmatic execution, new endpoints and Google expansion remain outside these tool packages. Client-history annotations use their separate [L2 contract](architecture.md#semantic-equivalence-compatibility). Public capability metadata must describe conditional mappings honestly; general protocol or SDK presence is not a per-model promise.

### Conversion-ladder migration

#### Remaining stabilization exit

Offline default-profile preparation, fresh-account setup and the initial real sweep have their recorded evidence. Remaining work is the [concentrated repair plan](#concentrated-repair-plan) and affected current-client loops.

Use the [partial-delivery exit](#partial-delivery-exit) for AR5 closure. Offline regression, SDK consumption and real-account acceptance remain separate; dated evidence is reused only for unchanged relevant contracts.

### Retained rejections

These L5 boundaries and invariant guards constrain the remaining work. Current additional blockers and their scope are tracked in the [findings table](#agent-default-production-findings).

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

Close the current Client/IR stabilization slice at the [partial-delivery exit](#partial-delivery-exit). Discovery must agree with implemented dispositions; unresolved cross-protocol cells may retain explicit next-stage limitations rather than requiring a new rejection policy or complete support before handoff. Deployer-selected public model profiles (e.g. `readable_reasoning`) remain configuration.

## Planned order

Only remaining work is listed. Offline tool/ordered-effort and default-profile preparation is complete; [AR5](#ordered-work-and-exit-gates) is next. Conditional tool compatibility packages require their own policy selection and are not default prerequisites. Stage 4 resumes after AR5; Stage 5 remains unscheduled.

| Stage | Goal | Exit evidence |
|---|---|---|
| **3 — Conditional compatibility follow-ups** | [S3-T5–S3-T7](#conditional-stage-3-follow-ups): namespace aliases, plain custom wrappers and examples projection | Separate policy selection and affected full-loop verification; not default blockers for AR5 or Stage 4 |
| **AR5 — Pre-Stage-4 acceptance gate** | Deliver the [concentrated repair plan](#concentrated-repair-plan) following the collected sweep and joint review | Meet the [partial-delivery exit](#partial-delivery-exit), with explicit next-stage dispositions rather than all-observation closure |
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
| Namespace aliases, custom text and examples | Conditional S3-T5–S3-T7 | Select each content/interface adaptation separately; do not make all three implicit core prerequisites |
| Ordered control null/reset | Named-level effort repair is delivered; optional-null reset/default meanings remain unqualified | Qualify field-specific semantics before broadening admission; retain current null rejections |
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
