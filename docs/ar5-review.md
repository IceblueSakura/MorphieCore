# AR5 Review

This user-requested record owns remaining AR5 observations and concise passing-test evidence. Completed repairs and their acceptance narratives are removed; Git preserves their history. The [remaining observation dispositions](roadmap.md#remaining-observation-dispositions) and user-approved [partial-delivery exit](roadmap.md#partial-delivery-exit) own repair/defer scope; [Architecture](architecture.md#conversion-ladder) owns accepted conversion policy.

This record retains the dated scenario coverage. Further qualification follows [real-Agent scenario selection](development.md#real-agent-scenario-selection) and the [remaining dispositions](roadmap.md#remaining-observation-dispositions), without repeating the completed broad sweep.

## Contents

- [Test context](#test-context)
- [Pending observations](#pending-observations)
  - [Tool invocation and argument guarantees](#tool-invocation-and-argument-guarantees)
  - [History and replay carriers](#history-and-replay-carriers)
  - [Instruction following and content recall](#instruction-following-and-content-recall)
- [Cache-affinity qualification evidence](#cache-affinity-qualification-evidence)
- [Passing tests](#passing-tests)
  - [Supplementary coverage, 2026-11-07](#supplementary-coverage-2026-11-07)
  - [Bailian model and protocol coverage, 2026-11-07](#bailian-model-and-protocol-coverage-2026-11-07)
- [Targeted follow-up, 2026-11-07](#targeted-follow-up-2026-11-07)
  - [Passing workflows](#passing-workflows)
  - [Diagnostic and failed samples](#diagnostic-and-failed-samples)
- [Grok supplementary coverage, 2026-11-07](#grok-supplementary-coverage-2026-11-07)
- [Binding and independent rechecks, 2026-11-07](#binding-and-independent-rechecks-2026-11-07)
- [Capped remaining-observation recheck, 2026-11-07](#capped-remaining-observation-recheck-2026-11-07)
- [Volatile-observation recheck, 2026-11-07](#volatile-observation-recheck-2026-11-07)
- [Repair verification, 2026-11-07](#repair-verification-2026-11-07)
- [Follow-up qualification, 2026-11-08](#follow-up-qualification-2026-11-08)
- [Offline attribution, 2026-11-08](#offline-attribution-2026-11-08)
- [R21 write-feedback qualification, 2026-11-08](#r21-write-feedback-qualification-2026-11-08)
- [Readable Chat replay qualification, 2026-11-08](#readable-chat-replay-qualification-2026-11-08)
- [Tool policy and Grok request qualification, 2026-11-08](#tool-policy-and-grok-request-qualification-2026-11-08)
- [Selected root repairs and qualification, 2026-11-09](#selected-root-repairs-and-qualification-2026-11-09)
- [Bailian function-guarantee follow-up, 2026-11-09](#bailian-function-guarantee-follow-up-2026-11-09)
- [Shared Bailian strict relaxation, 2026-11-09](#shared-bailian-strict-relaxation-2026-11-09)
- [Updating this record](#updating-this-record)

## Test context

Baseline date: **2026-11-06**. Installed clients: unwrapped **pi 1.5.0** and **OpenCode 2.0.32**; SDKs use the project pins. Real tests use isolated state, the project credential loader, serial requests, a **2048-token output ceiling**, **120-second deadlines**, and a fixed request budget per profile without harness retries.

Supplementary date: **2026-11-07**, with the same installed client versions and a current `cargo build --locked --offline --bin morphiecore`. The sweep retains bundled compatibility and `thinkingLevelMap` metadata, uses synthetic workspaces and temporary captures, and reports each case independently. A runner's exit code is not the case verdict; the bounded runners emit per-case JSON outcomes.

Supplementary collection totals, including qualification reruns and failed observers: **61 case executions, 162 client requests, 157 upstream attempts**. Gateway outcomes are 150 complete, 6 interrupted and 6 error; five errors have no upstream attempt, and one follows upstream HTTP 200. A complete delivery can still fail the scenario's control/content assertion. Two additional synthetic-only OpenCode tool-declaration characterizations are excluded from these totals.

The subsequent user-selected Bailian sweep separately covers `qwen3.8-flash`, `glm-5.3` (the requested GLM5.3) and `deepseek-v4.1-flash-1017`, each through native Chat and Responses. A new API-key test account validates through the project CLI. Bailian-only totals are **77 case executions, 103 client requests, 101 upstream attempts**: 98 native-companion attempts, 3 gateway attempts and 2 local admission failures. Native companions use the project account loader and trusted HTTP transport with unchanged official SDK request bodies, one send, a 120-second total deadline, 256 KiB requests and 2 MiB responses. They request native Chat `max_tokens:2048` or Responses `max_output_tokens:2048`; the legacy Chat field retains its native ceiling semantics. Responses are buffered before SDK consumption, so incremental publication is not measured.

Cross-protocol profiles reuse bundled model metadata with an explicitly selected serializer. They do not qualify native Google client APIs. DeepSeek uses `deepseek-beta-chat`: public `deepseek-v4.1-flash-1017`, canonical/upstream `deepseek-flash`; OpenCode's passing built-in profile uses the canonical label.

Collection totals retain their original execution scope when resolved entries are removed. Counts below are scoped to the stated tests. Local admission, client configuration, observer failures, upstream rejections and unexercised prerequisites remain distinct. [Development](development.md#production-profile-acceptance) retains the detailed baseline and verification procedures; [Roadmap](roadmap.md#partial-delivery-exit) owns AR5's exit boundary.

## Pending observations

The **seven retained IDs** form one consolidated problem set. The inventory below separates global boundaries, protocol carriers and residual Provider/model evidence; it does not assert that all symptoms share one cause. The [roadmap analysis order](roadmap.md#ar5-closure-order) owns global → protocol → dialect priority, and the [dispositions](roadmap.md#remaining-observation-dispositions) own work, re-entry and deferrals. Google and strict guarantees are included without the earlier temporary postponement. Detailed observations remain grouped by function for lookup, not as separate queues. Dated evidence below retains its original collection scope. R09 evidence is retained [separately](#cache-affinity-qualification-evidence), following its move to post-AR5, pre-Stage-4 cache-affinity qualification. R17 and R18's strict-admission subcase are closed by the selected [shared-service relaxation](#shared-bailian-strict-relaxation-2026-11-09).

| Shared boundary / evidence | IDs | Global question | Protocol boundary | Residual Provider/model evidence |
|---|---|---|---|---|
| [Tool invocation and argument guarantees](#tool-invocation-and-argument-guarantees) | R05, R18 | Distinct invocation identity, cardinality, seriality and schema-enforcement guarantees at admission/lowering boundaries | Google supports the selected strict schema subset; unsupported constraints and seriality remain independent gaps | Bailian enabled-thinking forcing limitations; original full Google Agent profiles are not qualified by the supported SDK subset |
| [History and replay carriers](#history-and-replay-carriers) | R01, R02, R06 | Ordinary attachments and protected reasoning have distinct owners under shared identity, signature and replay-trust invariants | R02/R06 share one generateContent ordinary-attachment gap across Chat/Responses/Messages clients; managed continuation is distinct | Existing Readable Chat carrier repair passes; real Haiku protected output remains unobserved |
| [Instruction following and content recall](#instruction-following-and-content-recall) | R13, R21 | Distinct instruction/history preservation, cancellation/feedback ownership and upstream content-attribution boundaries | Existing native/prepared/delivered/client comparisons retain exact content assertions; no request rewrite is supported | Complete captures show scoped upstream omissions; R21 evidence joins shared analysis while its Provider-specific retest/repair remains deferred |

A passing sample closes only its exercised boundary. Completed portions stay in passing evidence, not the pending inventory; identifiers are not renumbered.

### Tool invocation and argument guarantees

| ID | Observation and affected path | Current evidence and unresolved boundary |
|---|---|---|
| R18 | Bailian forced invocation | Thinking GLM Chat chooses the prompted decoy through an initial sample plus three repeats. Single-tool, low-effort Qwen/GLM Responses both ignore `allowed_tools/required`; string `required` independently returns zero calls in **4/4 per model**. These exact default/enabled-thinking profiles retain local rejection independently of the selected strict relaxation. Completed thinking-off forcing, including multi-declaration inventory filtering, serial carriage and Qwen Chat call/`stop` qualification are retained in passing evidence. |
| R05 | Full Google Agent strict-profile qualification | The selected closed-object function subset now has strict native mode mappings on both APIs. Full original Agent profiles remain unqualified: length/pattern/reference/composition constraints and serial requests are outside that subset, and a supported SDK loop does not qualify default-pi declarations. This remaining profile scope is independent of the ordinary-attachment gap in R02/R06. |

### History and replay carriers

R02 and R06 are distinct affected client paths through the same generateContent attachment boundary. Ordinary signed text/calls remain distinct from reasoning; the Readable Chat reasoning extension and managed continuation do not qualify stateless ordinary-attachment carriage.

| ID | Observation and affected path | Current evidence and unresolved boundary |
|---|---|---|
| R02 | Stateless generateContent signature carriage | OpenCode Chat/Responses and the Responses SDK smoke reach upstream HTTP 200, then fail projection with `codec.unsupported.replay`; ordinary text can carry `thoughtSignature`. Managed continuation passes separately. No disposition for discarding or substituting these attachments is selected. |
| R06 | pi Google generateContent Messages stateless carriage | The original Messages profile now reaches generateContent: **1 request / 1 attempt**, then `codec.unsupported.replay` on signed stateless delivery. The Interactions read/result/answer/recall loop passes separately below; only generateContent's carrier limitation remains pending. |
| R01 | Real Haiku protected Readable-Chat replay qualification | The existing profile's carrier repair is selected and verified below, including current-client signed/redacted Messages loops and real Grok/Interactions replay. Haiku's bounded real read/feedback/recall loop completes **3/3**, but returns no opaque replay; that real protected-output cell remains unobserved. Unsigned output does not exercise that cell. |

### Instruction following and content recall

These observations retain content assertions independently from the qualified tool/lifecycle mechanics. Complete native captures support scoped upstream attribution; incomplete captures do not establish equivalence or a local divergence.

| ID | Observation and affected path | Current evidence and unresolved boundary |
|---|---|---|
| R13 | pi Messages dynamic-tool/instruction literal-label recall failure | The 2026-11-08 qualification runs **7 stories / 35 attempts** on pi 1.5.0 with `opencode-go-haiku-messages` (Messages, `opencode-go/claude-haiku-5-5`): two full passes, four recall failures and one final-request upstream HTTP 400. All recall failures retain the exact label in preparation and the filename in the answer. Three have complete native text capture: the upstream already omits the label, including under case-folded search, and native/delivered/pi text agrees exactly. Active declarations remain `[read]`, `[read]`, `[ls]`, `[ls]`, `[]`. One other failed recall has incomplete native capture and supplies only delivery/pi agreement. |
| R21 | Grok recovery-label assertions and newly issued recovery tools — **Provider-specific retest/repair deferred** | Two pi write-cancellation stories complete **3/3 each**, without cancelled execution and with exact new-write/feedback identities, but omit `RECOVERED` and issue write despite the prompt-only no-tool instruction. One complete native final already omits the marker and equals delivery/pi. Native tool-call snapshots remain incomplete. Four new no-cancellation, no-tool controls vary only instruction role, instruction type and user text carrier; all return the prior arithmetic answer, not the marker. Exact offline request checks preserve developer text, user order and tool policy. These samples participate in shared instruction/attribution analysis; they support no role/carrier repair. The [explicit Provider-specific deferral](roadmap.md#instruction-following-and-content-recall) remains in force. |

## Cache-affinity qualification evidence

R09 is retained here as evidence for the [separate post-AR5, pre-Stage-4 qualification stage](roadmap.md#cache-affinity-qualification), not a pending AR5 observation.

| ID | Observation and affected path | Current evidence and unresolved boundary |
|---|---|---|
| R09 | Real same-model, independent-account failover prerequisite | Real stateless JSON/SSE replay isolation is qualified in the passing table. Same-model account failover remains unexercised: the project loader reports one enabled declared account per Provider, with no qualified independent compatible pair. Same-key aliases and different-model switching do not qualify that remaining gate. |

## Passing tests

These results are retained independently of the pending observations. They qualify only the listed profiles, not AR5 closure or untested protocol combinations.

The **2026-11-08 R09** follow-up uses the production CLI gateway/project loader and pinned OpenAI Python **3.37.0**, with isolated state and unchanged stateless `/v1/responses` history. JSON and SSE each fix **four serial requests / four attempts maximum**, **2048 output tokens**, **120-second route/exchange deadlines**, **256-KiB requests / 2-MiB responses**, concurrency **1**, no compaction or hidden retries. Read executes only against a synthetic scratch file. Complete native `response.output_item.done` values independently verify delivered envelopes and restored requests. The **49 focused offline checks** pass: reasoning replay **28**, ingress **1**, dispatch fencing **2**, routing/affinity **16**, configured-account isolation **1**, and key-rebinding **1**. Existing full-baseline evidence remains applicable without product/test changes. Body-free logs are retained at `testdata/reports/ar5-r09-replay-{json,sse,offline}-20261108.log`; private captures, scratch state and the disposable observer are removed.

| Test group | Passing result | Scope |
|---|---|---|
| Real replay isolation, 2026-11-08 | JSON and SSE each complete **4 requests / 4 attempts**, all complete, clean gateway shutdown | Grok protected call → same-Grok read feedback → Luna visible-history transfer → return to Grok with unchanged mixed history. Both models return protected reasoning. Same-target native values restore exactly; foreign reasoning is omitted under `responses.transfer-visible-history.v1` in both directions. Visible assistant text, semantic call/result links and original Grok replay survive transfer. This qualifies these SDK/stateless profiles, not independent-account failover or an installed-Agent model switch. |
| Bailian serial Responses tools, 2026-11-08 | Qwen and GLM each pass **3 requests / 3 attempts** | Pinned SDK SSE call/feedback and JSON recall; explicit serial control and auto/none choices survive preparation. Strictness is explicitly false. This does not qualify required selection. |
| Qwen Chat forced-call continuation, 2026-11-08 | **3-request gateway SDK feedback/recall loop** | Nonthinking, explicit non-strict profile preserves the correctly forced `lookup` and native `stop`; tool identity and terminal spelling are checked separately. |
| Bailian structured-output samples, 2026-11-08 | Qwen gateway JSON/SSE and one matched GLM gateway JSON sample pass SDK and schema checks | The GLM sample preserves exact preparation and native/delivered text while normalizing native `schema_`. These are scoped passing samples, not reliable enforcement of arbitrary schemas. |
| Fresh accounts and basic delivery | Five account references validate; Haiku, Grok OAuth, DeepSeek beta and Luna each complete a one-request smoke | Google basic delivery is included in the managed SDK checks below |
| Native Agent tool/history loops | pi and OpenCode: Haiku Messages, DeepSeek beta readable Chat, Grok OAuth Responses; OpenCode: Luna Responses | Each loop completes 3 requests / 3 attempts, real read/result/answer and same-session recall. [Detailed profiles](development.md#production-profile-acceptance) preserve their native controls and model-label qualifications |
| OpenCode Go native DeepSeek identity and loop | Authenticated models GET returns `deepseek-v4.1-flash`; OpenCode 2.0.32 completes **3/3** actual read/result/answer/recall with `opencode-go/deepseek-v4.1-flash` | [Current Go endpoints](https://opencode.ai/docs/console/go/#endpoints): `/inference/go/openai/v1/chat/completions`, with the documented client identity and stable `x-opencode-session`. Project-loader native passthrough preserves request bytes and returned history; this bypasses MorphieCore codecs and does not activate a gateway binding. |
| Additional Agent protocol loops | OpenCode Responses→Haiku, Responses→Interactions; pi Messages→Grok | Each completes 3 requests / 3 attempts with tool history and same-session recall |
| generateContent managed Responses SDK | Smoke 1/1; JSON loop 3/3; SSE loop 3/3 | Explicit `store:true` / `previous_response_id`; signed call/text attachments are retained. Stateless carriage remains R02 |
| Interactions managed Responses SDK | 7 requests / 7 attempts / 7 complete deliveries | JSON smoke and JSON/SSE tool loops; four unchanged thought signatures and two native call/result associations verified. Upstream remains stateless. Signed-call replay is unqualified: no signed call was observed |
| Ordinary Generation summaries and rebuilt tools | pi: 6/6 complete; OpenCode: 5/5 complete, on real Haiku | Same-session summary and another real read/result cycle; pi retains the recent turn, OpenCode produces one completed checkpoint. No native-compaction operation or retry/nudge case is claimed |
| Real first-text cancellation and recovery | Each Agent: 3 requests / 3 attempts; `complete → interrupted → complete`, gateway concurrency 1 | pi `--no-tools`; OpenCode `tool_choice:none`. Cancel after visible text, honest aborted/interrupted outcome and same-session single-permit recovery. Remote computation/billing termination is not asserted |
| Independent synthetic regression | 21 installed-client scenarios, 19 SDK scenarios, 8 session-history cases and the Interactions terminal service-tier regression pass | Separate from real Provider and actual-account acceptance |

Interactions capture qualification: the original runner exits nonzero only on an overbroad complete-capture assertion. Post-run safe-boundary verification establishes the listed signatures and associations; optional unsafe argument fragments are excluded, and complete capture coverage is not claimed.

Single normal-request timing observations were pi RPC **1580 / 3685 ms** and OpenCode core SSE **1811 / 4453 ms** for first text / terminal. These are individual client-clock observations, not latency thresholds or comparable transport benchmarks. Expected broken-pipe/reset notes belong to the cancelled forwarding connection.

### Supplementary coverage, 2026-11-07

These real upstream observations use the CLI gateway and installed Agents, or the explicitly labelled pi-ai API companion. Synthetic characterization remains separate. Each case has one selected account per Provider, no recovery retries, bounded auxiliary calls and the test-context limits above. Counts describe the stated sample, including separate observer-qualification reruns where noted.

| Test group | Passing result | Scope |
|---|---|---|
| pi thinking selections | Haiku 7/7, DeepSeek 4/4 and Luna 6/6 single-request delivery profiles | Haiku: off/minimal/low/medium/high/xhigh/max; minimal and low both emit native low. DeepSeek: low/high/max retain native effort. Luna: off/low/medium/high/xhigh/max; a separate six-request wire-qualification sweep confirms native `configuration_update` efforts for enabled levels, with root effort fixed at medium. Grok's later four-level coverage is [recorded separately](#grok-supplementary-coverage-2026-11-07). These short arithmetic checks qualify observed selection/control carriage and delivery, not comparative reasoning quality. |
| Reasoning-summary API controls | Luna default, auto, concise and detailed each complete 2 requests / 2 attempts | Actual installed pi-ai 1.5.0 typed Responses API, medium effort, generated answer and untouched API-returned history follow-up. Both ingress and prepared summary settings agree. Pi's ordinary UI/RPC has no separate summary-verbosity selector; default emits auto. |
| pi actual coding and effort changes | Haiku: 6/6; DeepSeek: 8/8 in qualified reruns | Real failing Python check, multiple reads, actual edit, successful check and Unicode recall; low → high → low effort. The verifier stays unchanged and independent code assertions pass. Five Haiku and six DeepSeek call/result pairs retain their identities. Initial 6/7-request coding samples are separate evidence. |
| OpenCode actual coding and repair | Haiku: 6 requests / 6 attempts in the corrected shell profile | Actual shell failure, read, edit, successful shell verification and same-session no-tool recall. Four tool calls (`shell`, `read`, `edit`, `shell`) complete. The verifier stays unchanged and independent code assertions pass. Earlier profiles below remain configuration/observer qualifications. |
| pi active tools and instruction changes | Haiku: 5/5 in the instrumented sample | Supported test-owned extension operations change system instructions, effort high → low and active declarations read → ls → none. Actual read/list and no-tool label/filename recall pass. R13 retains the first combined-assertion failure. |
| pi persisted history and earlier-turn edits | Haiku and DeepSeek each complete 8 requests / 8 attempts | Read/result history survives process restart with exact projected role/content, then no-tool recall. Fork before an earlier user note, submit an edited replacement, and independently resume original and fork; both retain their own labels. Gateway process stays running. |
| pi cross-model session workflow | Haiku Messages → DeepSeek readable Chat → Haiku Messages: 4/4 | Actual `set_model` operations preserve the client session and visible read/result recall. This is a client serializer/model-switch workflow. Real SDK/stateless opaque transfer is qualified separately above; independent-account failover remains R09. |
| pi repeated and automatic compaction | Repeated manual story: 9/9; automatic story: 4/4 | Two ordinary manual compactions, split-turn work, exact label recalls and a rebuilt actual read pass in the qualified sample. Five consecutive capped repeated-compaction stories also pass **9/9 each**. The automatic story observes one threshold compaction and subsequent recall using a test-owned 5000-token trigger, not full-context capacity. |
| pi steering and follow-up | Haiku: 3/3 | Queue steering and follow-up during visible generation; observe the subsequent distinct responses and user-history delivery through `agent_settled`. |
| pi thinking and local-tool cancellation | Each scenario passes an initial sample and a separate `concurrency:1` sample, 2 attempts each | Thinking: interrupted → complete; abort after a real thinking delta and observe the gateway interruption receipt before recovery in the single-permit sample. Local bash: 2 complete upstream responses; abort after a Python execution marker, require honest tool cancellation, process release, no late file and same-session recovery. Source generation for the local tool had already completed. No remote-computation termination is claimed. |
| pi cancellation during ordinary summarization | Initial and `concurrency:1` samples each complete the 4-request story: complete → complete → interrupted → complete | Cancel manual compaction after its own upstream `message_start`; the compact command fails honestly, projected context stays exactly unchanged and label recall recovers. The single-permit sample observes the interruption receipt before recovery. |
| pi cancellation during tool arguments | Initial 2-request sample: interrupted → complete | Abort after actual argument deltas and before tool execution; no file is created and same-session recovery passes. |
| pi image and larger tool-result history | Haiku image: 3/3; larger text file: 3/3 | Real read returns a synthetic PNG; the model identifies the center color and recalls it without another tool. A 420-line synthetic text file retains early/late Unicode label facts through read and follow-up. |
| pi native Messages small output limit | Haiku: 1/1 at `max_tokens:1` | Client-facing `max_tokens`/length termination is observed without tools. |

Collection qualifications:

- OpenCode's initial coding configuration allowed `bash`, but the installed client's command tool is `shell`. A synthetic direct-authority comparison confirms that only the `shell` permission exposes that tool. The initial 6-request assertion failure and a 5-request edit/independent-check pass are kept separate from the corrected 6-request shell profile.
- Product code, returned history and production guards were not changed to obtain passes. Captures are temporary, body-free verdict logs are saved locally, and private exchanges are not public fixtures.
- Remaining replay, Google-control, cross-protocol and account prerequisites stay open. Full-context capacity, deterministic transport/rate-limit faults and remote computation/billing termination are outside these real samples.

### Bailian model and protocol coverage, 2026-11-07

Provider is **`aliyun-dashscope-cn`**, separate from Token Plan and native DeepSeek. The six cells below use the real Provider through the project loader/transport and OpenAI Python **3.37.0**. JSON and SSE are separate serial runs. Each smoke uses one request. Each ordinary non-strict tool story uses three requests: obtain a model-issued call, submit its test-owned result and consume the final answer, then recall using returned history. The SDK adapter retains returned reasoning and call/result identities; it removes only SDK-only parsed attributes when constructing Responses history. No Agent tool execution is claimed for this companion.

| Model | Native upstream API | JSON / SSE smoke | JSON / SSE tool-history loops |
|---|---|---|---|
| `qwen3.8-flash` | Chat | 1/1 + 1/1 | 3/3 + 3/3 |
| `qwen3.8-flash` | Responses | 1/1 + 1/1 | 3/3 + 3/3 |
| `glm-5.3` | Chat | 1/1 + 1/1 | 3/3 + 3/3 |
| `glm-5.3` | Responses | 1/1 + 1/1 | 3/3 + 3/3 |
| `deepseek-v4.1-flash-1017` | Chat | 1/1 + 1/1 | 3/3 + 3/3 |
| `deepseek-v4.1-flash-1017` | Responses | 1/1 + 1/1 | 3/3 + 3/3 |

Additional passing evidence and qualified restrictions:

| Profile | Observed result | Scope |
|---|---|---|
| Native thinking controls | Qwen off/low/medium/xhigh and DeepSeek off/low/high/max complete in both protocols; GLM low/high complete in both | 20 successful single-request profiles. The four Qwen/DeepSeek `enable_thinking:false` samples return no reasoning carrier. GLM off is explicitly rejected in each protocol (HTTP 400), qualifying that restriction. Short arithmetic does not measure effort quality or prove how the Provider computes internally. |
| Native Responses summary settings | All three models accept auto/concise/detailed and complete the arithmetic answer | 9/9 single-request profiles. Qwen/DeepSeek return reasoning owners; GLM's selected low-effort samples do not. Requested summary granularity is not independently qualified. |
| Native specific-tool samples | DeepSeek passes both combined strict/named profiles, plus non-strict Responses allowed-tools and nonthinking non-strict Chat specific selection | Four one-request samples. Numeric enum conformity is observed only in the two strict/named samples; R18 retains the other models' failing profiles. |
| Native JSON Schema conflict sample | Qwen Chat returns the exact closed shape with an allowed numeric enum and Boolean | One request/attempt. This is not an arbitrary-schema or internal constrained-decoding qualification. |
| pi through the production gateway | Configured Responses → Qwen Responses completes 3 requests / 3 attempts | pi 1.5.0 with bundled Luna Responses serializer metadata and the selected Qwen ID: actual read/result/answer and same-session recall. Client root medium plus ordered low-effort updates prepare as native low; no strict opt-out or request/history rewrite is used. GLM Chat admission remains R17; later binding acceptance is recorded below. |

The native companion establishes upstream wire/SDK behavior independently of gateway codecs. Temporary response files stay private, reject credential echoes and are deleted with each case. Its disposable loader helper does not add deployment bindings. This collection predates the separately selected catalog activation below; broader structured-output/tool guarantees remain unqualified.

## Targeted follow-up, 2026-11-07

The user authorized focused supplementary tests and explicit retries of unexpected errors. The current build passes `cargo build --locked --offline --bin morphiecore --bin morphiecore-auth`; installed clients remain pi 1.5.0 / OpenCode 2.0.32, with pi used for these new Agent cases. Tests reuse existing isolated drivers and bundled serializer metadata. Normal requests retain the 2048-token/120-second bounds. Native helpers are rebuilt from inspected disposable sources, use the project account loader/trusted transport, send once per invocation, and bound requests/responses to 256 KiB / 2 MiB. Native buffering does not measure incremental network publication.

New evidence is scoped below, separately from the preceding collection totals:

### Passing workflows

| Group | Requests and outcome | Qualification |
|---|---|---|
| Interleaved pi sessions, Haiku Messages | 6 client requests / 6 attempts / 6 complete | A-read → B-read → A-recall → B-recall, same gateway/model/account, distinct sessions and labels. No foreign label appears in either session's ingress history or answer; gateway concurrency is 1. |
| Same-turn multiple tools, Haiku Messages | 3/3 complete | One assistant turn issues two real reads; distinct call/result IDs retain the correct left/right file values, followed by no-tool recall. Actual parallel execution is not required. |
| Reported usage and cache, Haiku Messages | 2/2 complete | Upstream input/output/cache-read/cache-write counters match pi's counters independently. The follow-up actually reports 745 cache-read tokens, also shown by pi; no cache-hit rate or billing claim is made. |
| Tool-argument cancellation, Haiku Messages | 2 attempts: interrupted → complete | Fresh single-permit sample waits for the gateway interruption receipt, executes no partial tool, creates no file, then recovers in the same session. |
| Real pi coding, Qwen Responses | 7/7 complete | Actual failing Python check, two reads, edit, successful check, independent code assertions, unchanged verifier and Unicode no-tool recall. Five call/result pairs retain identity. UI effort low → high → low prepares as native low → xhigh → low. Uses the existing production binding and bundled Luna Responses serializer; no strict opt-out. |

### Diagnostic and failed samples

| Group | Requests and outcome | Qualification |
|---|---|---|
| Remaining original-profile admission samples | GLM Chat: 1 request / 0 attempts, local rejection | The explicit strict-tools boundary remains R17. Google ordered-control failures have their updated qualification below. |
| Native Bailian isolated controls | 17 single-request invocations, 16 HTTP-200 exchanges and 1 transport-failed invocation | Two-tool named selection, matched strict false/true auto-choice pairs, and Responses schema-envelope qualification. The failed GLM Responses false sample and its one explicit retry both remain recorded. The original helper did not retain the failed invocation's send boundary, so its missing attempt count is unknown rather than evidence of zero sends. R18 retains the tool-control verdicts. |

Pure offline controls additionally exercise **25 synthetic client-decode → endpoint-preparation cases**, without credentials or Provider calls. They separate strict tools, inline declarations, ordered effort, adaptive/block-binding admission and public unknown-field discard. Diagnostic non-strict variants are not promoted into default-profile Agent acceptance.

All raw native responses and gateway captures are temporary and deleted; only body-free verdicts are retained locally. No product source, returned history, production guard, binding activation or deployed service is changed to obtain a pass. Remaining failures retain their roadmap dispositions. Real independent-account failover still needs the R09 prerequisites; Google controls and cross-protocol replay retain their qualified boundaries.

## Grok supplementary coverage, 2026-11-07

The current gateway is rebuilt with `cargo build --locked --offline --bin morphiecore`. Tests use installed unwrapped pi **1.5.0**, public model **`grok-4.7`**, existing **`grok-responses`** OAuth binding, retained bundled xAI metadata, isolated state, synthetic workspaces, serial sends, 2048 output tokens and 120-second route/exchange deadlines. Request/response observations are bounded to 256 KiB / 2 MiB. Explicit retries and changed diagnostic profiles are counted separately; no product or returned-history rewrite is used.

This collection contains **20 case executions**, including retries, controls and one failed observer: **38 observed client submissions**, of which **35 reach the gateway and make 35 upstream attempts**; the test authority blocks three tool-feedback submissions before forwarding. Gateway outcomes are **29 complete and 6 interrupted**, with no gateway error outcome. Nine cases pass their full assertions; ten fail recovery/content expectations or the resulting fixed-budget boundary, and one stops at the identified observer check. Complete delivery and scenario verdicts remain separate.

Passing workflows:

| Group | Result | Scope |
|---|---|---|
| Actual pi thinking selections | 4 requests / 4 attempts / 4 complete | Low, medium, high and xhigh remain selected in UI/RPC, reach ingress/prepared native reasoning fields unchanged, and complete arithmetic. The client exposes these four levels; reasoning quality is outside this check. |
| Actual pi coding and effort changes | 7/7 complete | Real failing Python check, two reads, edit, successful check, unchanged verifier, independent code assertions and Unicode no-tool recall. Five call/result pairs retain identity; low → high → low survives native preparation and the same session. |
| Reasoning-summary API controls | 8/8 complete | Installed pi-ai typed Responses API companion, medium effort, default/auto/concise/detailed with two requests each: arithmetic answer and untouched returned-history recall. Default emits auto; ingress and prepared summary values agree. This qualifies control carriage and continuation, not summary granularity or an additional ordinary pi UI selector. |

Diagnostic and failed samples:

| Group | Result | Boundary |
|---|---|---|
| Thinking cancellation and recovery | Two separate 2-attempt stories, each interrupted → complete | Abort follows an actual thinking delta; the interruption receipt precedes recovery and high → low effort changes. Both recovery answers fail the exact `RECOVERED` assertion. The instrumented answer contains no marker, and its delivered text reaches pi unchanged; the recovery instruction is present in the prepared upstream request. |
| Observer qualification | 1 interrupted request; no recovery request | The first instrumented thinking observer reads a JSONL record while it is only partially written and raises `JSONDecodeError`. The test-owned reader is corrected to wait for newline-complete records while retaining strict parsing of completed lines; the subsequent 2-attempt story above remains a content-assertion failure. |
| Fresh-context instruction controls | 5 requests / 5 attempts / 5 complete; all exact-label assertions fail | Typed pi-ai API companion, no cancellation or returned history: recovery-only instruction with “ignore prior task”, two consecutive users, the same two instructions within one user message, bare `RECOVERED`, and bare `CEDAR_OK`. The consecutive and merged variants return the first arithmetic answer; other variants also miss their exact-label oracle. These are diagnostic inputs, not normalization of an Agent's returned history. |

The fresh-context failures leave the exact-label recovery oracle unqualified as a cancellation-specific check. Tools remain declared in the tool stories and no explicit no-tool API guarantee is requested. R21 retains the observed content/tool behavior without assigning a gateway defect or selecting an adaptation. Gateway single-permit release and honest interruption are observed; remote computation/billing termination is not measured. R09's independent-compatible-account prerequisite and the other blocked protocol lanes remain open.

Private captures are deleted with each case; body-free verdicts and complete runner logs are saved locally. No deployment, binding activation or account replacement is performed by this sweep. Remaining repair/defer dispositions belong to the roadmap.

## Binding and independent rechecks, 2026-11-07

The user selected the three missing Bailian catalog bindings and independent repeats of unexplained failures. Tests use current unwrapped **pi 1.5.0**, **OpenCode 2.0.32**, pinned **OpenAI Python 3.37.0**, the project account loader and trusted transport, isolated state, serial sends and no hidden retries. Each live case fixes its request budget before starting, **2048 output tokens**, **120-second deadlines**, **256 KiB requests / 2 MiB responses**, and gateway concurrency **1**. On non-Qwen thinking-enabled Bailian Responses, 2048 is the selected native reply ceiling, with the existing total-to-reply loss receipt; byte/time bounds remain independent.

| Boundary | Observed result |
|---|---|
| Bailian DeepSeek Chat | New `dashscope-deepseek-chat`: **4 requests / 4 attempts / 4 complete**; strict SDK JSON arithmetic, SSE function call/result/final answer and unchanged returned-history recall pass. Explicit non-strict tools and native low effort; no native-beta strict guarantee is inherited. |
| Haiku argument cancellation | Two independent **2/2** single-permit stories record `interrupted → complete`; one passes the full recovery oracle. The other reaches observer parsing/excess-feedback boundaries after gateway completion. |

The full offline baseline passes: **436 library, 415 semantic, 94 transport and 30 gateway tests**, all-target Clippy under `-D warnings`, formatting, production-only checks, **20 pinned assets** and the **8 upstream Python tests**. All **19 connected SDK scenarios** pass against synthetic authorities.

## Capped remaining-observation recheck, 2026-11-07

The user requested the exact undated DeepSeek label and at most **three scenarios per remaining observation**, allowing for Provider instability. The batch uses the same current clients, SDK, loader, bounds and serial/no-retry policy above. **43 independent scenario executions / 73 upstream attempts** retain the original collection scope. Each scenario fixes its request ceiling before dispatch: one for admission/native controls/stateless smoke, two for cancellation, three for ordinary Agent/replay loops, four for the Responses SDK loop, five for tool switching and nine for repeated compaction. Excess client submissions stop before forwarding.

| Observation | Executed scope and result |
|---|---|
| R02 | OpenCode Chat, OpenCode Responses and stateless SDK smoke: **1 attempt each**, all repeat `codec.unsupported.replay`. |
| Repeated compaction | Two independent stories pass **9/9 each**, including both exact no-tool recalls and rebuilt read. |
| R13 | Three unchanged tool-switching stories, **5/5 each**: one missing-literal-label assertion failure, then two passes. Ingress retains both facts in all three. The final diagnostic checks prepared/delivered/client fact presence, issued calls and terminal independently. |
| R17 | Two original strict-profile stories reject at local admission with **0 attempts**; no strict opt-out is applied. |
| R18 | Three one-send HTTP-200 controls: Qwen Chat chooses `lookup` but reports `stop`; GLM Chat chooses prompted `decoy` instead of forced `lookup`; Qwen Responses strict-true/auto emits out-of-enum 93. Each reproduces its selected boundary; other model/API/control cells are not rerun. |
| R21 | Thinking cancellation records **interrupted → complete** with **2 attempts**, but fails the recovery-label assertion. A one-attempt fresh-context bare-label control also fails. Ingress/prepared recovery instructions remain present; cancellation-specific causality stays unestablished. |

The third R13 report's ad hoc native-text selector has unestablished coverage; its negative flags are not a qualified upstream-content oracle. Only the independently checked prepared/delivered/client fact presence is retained from that diagnostic. No production mapping or history guard changes were made. Body-free logs are saved locally under `testdata/reports/ar5-capped-20261107-*.log`; private exchange files are deleted with each scenario.

## Volatile-observation recheck, 2026-11-07

This dated batch contains **three groups of three independent scenarios / 45 upstream attempts**, with the same clients, SDK, bounds and serial/no-retry policy. All 45 attempts return HTTP 200. Per-scenario request ceilings are **9**, **5** and **1**; the retained repeated-compaction and R13 observations follow below.

| Observation | Latest three-scenario result |
|---|---|
| Repeated compaction | **9/9, 9/9, 9/9**, all passing. Each story verifies two compactions, exact recalls without issued/executed tools, fact presence in summary/context/ingress/answer and rebuilt read. Together with the preceding two stories, five consecutive capped stories pass completely. |
| R13 | **5/5 each** complete, but all three fail the literal-label recall assertion. Filename presence and no-tool checks pass; ingress retains both facts and the active declarations remain read/read/ls/ls/none. The latest reports separate the failing assertion dimension, not an established codec or history-loss cause. |

Separate correct-Provider qualification uses three bounded scenarios: an initial native smoke and OpenCode loop each stop after one HTTP-400 attempt using the catalog-root/minimal-header setup; the final documented-root/client-session profile passes the native **3-request** OpenCode loop. The failed controls do not isolate the path and header contributions. The authenticated models result and current documented profile establish the Go identity above; gateway codec/binding and target/header maintenance remain outside this qualification. No production configuration or mapping was changed.

Body-free evidence is saved locally under `testdata/reports/ar5-volatile-20261107-*.log`; private request/response files are deleted per scenario.

## Repair verification, 2026-11-07

The selected repairs use the current unwrapped **pi 1.5.0**, **OpenCode 2.0.32**, pinned **OpenAI Python 3.37.0**, production credential loader and isolated test-owned state. Every live scenario fixes its target, request/attempt ceiling, **2048-token output bound**, **120-second route/exchange deadlines**, **256-KiB requests / 2-MiB responses** and concurrency **1**. Thinking-enabled non-Qwen Bailian Responses retains the selected reply-only ceiling. Sends are serial with no hidden retries or replacement calls.

This repair collection contains **24 independent scenario executions / 55 upstream attempts**, including pre-fix discriminators, observer failures and local rejections. The following results retain their narrower boundaries:

| Path | Result | Qualification |
|---|---|---|
| Luna cross-protocol continuation | Final Chat/Messages/Chat stories each pass **3/3**, **9 complete attempts** in total | Actual pi read/result/answer and same-session recall, original client serialization and call/result identities. No request/history rewrite or signature deletion. |
| Bailian DeepSeek Responses | Three independent SDK stories each pass **4/4**, **12 complete attempts** | JSON arithmetic smoke, SSE call/result/final answer and untouched returned-history recall on exact upstream `deepseek-v4.1-flash-1017`. |
| GLM Responses | Two SDK stories pass **4/4**; one separate first-request timeout | **9 attempts: 8 complete, 1 timeout**. Successful stories consume the queued opening, tool feedback and history without the former SDK accumulation failure. The timeout has no replacement send and is not assigned a protocol cause. |
| DeepSeek Chat → pi Messages history | Two independent stories pass **3/3**, **6 complete attempts** | The comparison ignores only an exact unsigned/unbound `{"type":"text","text":""}` block. Original wire and history remain untouched; owner counts, thinking and tool links remain checked. |
| pi Messages → Google Interactions | Actual read/result/answer/recall passes **3/3** | Original ordered effort and declarations reach the selected binding without a strict opt-out. This one complete story does not qualify every signed-call shape. |
| Google original Responses / generateContent Messages | **3 local rejections / 0 attempts** and **1 signed-stateless rejection / 1 attempt**, respectively | Remaining boundaries are R05/R06; explicit guarantees and signature guards stay intact. |

The collection also includes **six attempts across three pre-fix discriminators**. Structural observations use only known field paths, item/part domains, ID families and equality booleans; native aliases and private response contents do not become fixtures.

Two earlier post-fix Chat stories each complete two attempts, then the test authority races next-request admission against recording the previous forwarded EOF. Synchronizing that observer, without delaying/replacing response frames or changing product behavior, yields the final three passing stories above. The earlier Messages story independently passes **3/3**. A one-send Google Interactions admission probe completes, then its excess feedback is blocked before forwarding; the separate three-send loop above provides full-loop evidence.

The final offline baseline passes **436 library, 418 semantic, 96 transport and 30 gateway tests**, all-target Clippy under `-D warnings`, formatting, production-only compilation and **20 pinned assets**. Separate Python suites pass **8 upstream, 17 OpenAPI, 16 SDK-oracle and 45 client-oracle tests**. All **19 connected SDK scenarios** and **22 isolated installed-Agent scenarios** pass, including native defaults, queued stored/stateless Responses history, recovery, cancellation and summaries.

Private exchange files are deleted with each scenario; body-free evidence is retained locally. No deployment, account replacement or new binding activation occurs. Remaining repair/defer dispositions and the partial-delivery handoff belong to [Roadmap](roadmap.md#remaining-observation-dispositions).

## Follow-up qualification, 2026-11-08

This user-selected collection permits project-managed OAuth renewal, and allows an initial failing sample plus at most **three explicit repeats**. Every send remains serial and fixes its target, request count, **2048-token ceiling**, **120-second deadline**, **256-KiB request / 2-MiB response bounds** and gateway concurrency **1**. Gateway/SDK hidden retries remain off. Clients are current unwrapped **pi 1.5.0 / OpenCode 2.0.32**, with pinned **OpenAI Python 3.37.0**.

| Boundary | Result | Qualification |
|---|---|---|
| R18 native controls | **12 requests / 12 HTTP-200 attempts** | Each of three discriminator profiles retains its result through three repeats. Tool identity, strict arguments and terminal spelling are evaluated separately; unsupported native diagnostic profiles do not become gateway acceptance. |
| R18 Qwen Chat gateway loop | **3 requests / 3 complete attempts** | Explicit non-strict, nonthinking profile: forced `lookup`, untouched returned-call feedback and exact same-session value recall. Native `stop` is retained. This is SDK qualification, not default-pi strict acceptance. |
| R13 instrumented Agent stories | **35 attempts: 34 complete, 1 upstream HTTP 400** | Two stories pass, four fail recall and one stops on the HTTP error. Complete captures in three failing recalls establish native omission with unchanged delivery/pi text; the other incomplete capture is not an upstream-content oracle. One failed story passes on its first repeat; another exhausts all three. |
| R21 current pi cancellation | **5 stories / 9 gateway attempts** | Thinking passes `interrupted → complete` with exact recovery. Three tool-argument stories interrupt honestly, create no file before recovery, then execute write despite the prompt-only no-tool instruction. The fourth tool story has one already-complete gateway attempt and misses the cancellation window. No claim of remote compute termination is made. |
| R21 native controls without cancellation | **9 HTTP-200 attempts** | Initial bare-label sample plus three repeats all miss the marker, including case-folded comparison. Initial two-user/tool sample plus three repeats all miss it; the first three issue a new write and the fourth issues none. One explicit `tool_choice:none` contrast issues none and still misses the marker. No native diagnostic executes a tool. |

Grok access was expired and one project-CLI OAuth renewal succeeds. Do not attribute the later thinking pass to renewal without a causal comparison. Six earlier native-observer invocations stop at missing OAuth driver registration with **zero inference sends**; the corrected helper supplies the native evidence above. Their unavailable matched-recovery controls are not claimed as acceptance.

The full offline baseline, all-target Clippy under `-D warnings`, formatting, production-only compilation, **20 pinned assets** and the four Python suites (**8 upstream / 16 SDK-oracle / 45 client-oracle / 17 OpenAPI**) pass. Separate connected acceptance passes **19 SDK scenarios** and **22 isolated current-client scenarios** on pi 1.5.0 / OpenCode 2.0.32.

Private wire bodies are deleted with each scenario. Only body-free verdicts are retained; the request/response shapes in synthetic regressions are independently authored, not Provider captures.

## Offline attribution, 2026-11-08

These checks use current unwrapped **pi 1.5.0 / OpenCode 2.0.32**, isolated homes, synthetic files and loopback authorities only. No real Provider, credential renewal or private exchange is involved. The pi gateway case permits exactly three requests, a 2048-token ceiling, 25-second route / 30-second exchange deadlines, 256-KiB upstream requests and concurrency one.

| Boundary | Evidence | Remaining limit |
|---|---|---|
| Replay discrimination | A trusted final value on a target without a structured carrier reports `replay_format`; a supported carrier with foreign origin reports `replay_origin`; partial Chat closure reports `replay_phase`. Independent Responses replay and source/identity checks remain passing. | These guards do not retroactively identify an old sample's failing predicate. Current Readable Chat qualification follows below. |
| R21 cancellation ownership | The installed-pi Responses gate holds write arguments after the target path, aborts before execution, observes upstream release and a non-complete receipt, and verifies no file before recovery. One distinct new write creates the exact UTF-8 bytes and its feedback completes request three. Cancelled identities are absent from returned history; call/result identities match. **3 requests / 3 attempts**, no cancelled execution or replay; outcomes are non-complete, complete, complete. | This qualifies write lifecycle, side effects and feedback, not Grok instruction adherence or remote compute termination. |

The durable cancellation gate is `pi_responses_tool_argument_cancellation`; its write story replaces the earlier read story using the existing authority and protocol fixtures. Negative oracles reject cancelled identities, unrelated feedback, skipped final answers, duplicate execution and CRLF/LF byte changes. The shared pi driver defaults to read; this gate explicitly enables write. No production cancellation or client behavior is changed.

After the write-gate change, the full offline baseline, Clippy, formatting, production-only checks and **20 pinned assets** pass again. Python suites pass **8 upstream / 16 SDK-oracle / 47 client-oracle / 17 OpenAPI** checks; all **23 current installed-client synthetic scenarios** pass on pi 1.5.0 / OpenCode 2.0.32. The earlier SDK loopback evidence is reused because SDK fixtures and production code are unchanged.

## R21 write-feedback qualification, 2026-11-08

Current unwrapped **pi 1.5.0**, built-in `xai/grok-4.7` Responses metadata and the **grok-responses** binding use isolated state, write-only tools, low/auto reasoning, no compaction or hidden retries. Each story fixes **three client requests / three upstream attempts maximum**, **2048 tokens**, **120-second route/exchange deadlines**, **256-KiB requests / 2-MiB responses**, concurrency **1** and a **420-second outer bound**. The original recovery instruction remains `Do not use tools. Reply only RECOVERED.` No request/history rewrite or inferred tool withdrawal is used.

An initial local rejection has **one client request / zero upstream attempts** because access is expired. One authorized project-CLI renewal succeeds. Two subsequent independent stories each make **3 requests / 3 attempts**, recording `interrupted → complete → complete`. Both observe cancellation before execution, no file before recovery, different cancelled/recovered call IDs, one successful new write with exact argument bytes, matching execution/feedback IDs, absent cancelled identities in returned history and final `stop`. The old feedback-budget boundary is qualified.

Both stories retain failed exact-marker and prompt-only no-tool verdicts. Complete delivery text equals pi text in both. The second story aligns capture by exchange: request three's complete native final text already lacks the marker and equals delivery; all prepared user text and the returned call/result feedback agree with ingress. Only that final native output is complete enough for comparison; partial native tool-call capture is not a call-equivalence oracle. The first story's aggregate native comparison lacks full-output coverage and is not a mismatch finding. These samples support the retained upstream content limitation without claiming cancellation-specific causality or remote compute termination.

Body-free evidence is retained at `testdata/reports/ar5-r21-{write-baseline,live-slot1,renewal,live-slot2,live-slot3}-20261108.log`. Temporary captures, client state and the disposable observer are removed; no model prose or credentials are published.

## Readable Chat replay qualification, 2026-11-08

The selected [existing-profile extension](architecture.md#chat-reasoning-extensions) reuses typed details and independent `mc1.` envelopes, not the whole MorphieCore dialect. The multi-owner directional regression fails on the old public profile with `representation.replay_format` and passes after repair. A late signed-text regression verifies immediate readable text, closure-time metadata without repeated text, original replay restoration and JSON/SSE agreement. The earlier unsigned SDK no-details assertions remain unchanged and pass; foreign-origin, malformed-envelope, dependency and partial-replay guards remain passing.

Current unwrapped **pi 1.5.0 / OpenCode 2.0.32** each pass a **three-request** CLI-gateway loop with independent native Messages signature/redacted-data expectations, actual read execution, feedback and recall. In-memory observation verifies the delivered native-format envelopes equal returned client details. The pinned OpenAI SDK separately passes **six requests** across JSON/SSE, retaining two protected reasoning owners through tool feedback and recall. Direct current-client characterization also establishes the [adjacent text-signature counterexample](refs/2026-11-08-chat-reasoning-carriers.md).

Real checks use **OpenCode 2.0.32**, its generic Chat client, `readable_reasoning`, one read tool and project-loader credentials. Every story fixes **3 requests / 3 attempts maximum**, **2048 output tokens**, **120-second route/exchange deadlines**, **256-KiB requests / 2-MiB responses**, concurrency **1**, a **420-second outer bound** and no hidden retries. Messages default output is cleared in the test-owned selection so its catalog default cannot exceed the fixed cap; no client body/history is rewritten.

| Target | Completed story | Qualified boundary |
|---|---|---|
| `claude-haiku-5-5` / `opencode-go-haiku-messages` | **3 requests / 3 complete attempts**, no authority errors | Actual read, feedback and recall succeed; no opaque replay is returned. Real protected output remains unobserved, separately from the passing synthetic native-signature gate. |
| `grok-4.7` / `grok-responses` | **3 requests / 3 complete attempts**; one protected owner per response | Delivered envelope bytes equal returned client details; prepared native history restores the original opaque values on requests two and three. Tool feedback/recall and exact call identities pass. |
| `gemini-3.8-flash` / `google-gemini-3.8-flash-interactions` | **3 requests / 3 complete attempts**; one thought replay owner per response | Client details remain unchanged; prepared native thought anchors and optional signatures agree with the enclosed native payloads. This is thought replay, not ordinary generateContent attachment or signed-call qualification. |

Two startup trials and one loader-only diagnostic send no inference. An earlier **one-attempt Haiku** observer stops on raw JSON argument formatting; an earlier **three-complete-attempt Interactions** observer compares its internal thought wrapper to a native signature string. Those observer verdicts are retained separately. Correct wire oracles compare JSON object arguments semantically and restore the Interactions native payload; signature/envelope bytes and call IDs stay exact.

The final full offline baseline, all-target Clippy under `-D warnings`, formatting, production-only checks and **20 pinned assets** pass. Python suites pass **8 upstream / 16 SDK-oracle / 47 client-oracle / 17 OpenAPI** checks. Separate connected acceptance passes all **20 pinned SDK** and **25 current installed-client synthetic** scenarios. Body-free evidence is retained in `testdata/reports/ar5-r01-*20261108.log`; temporary observers, captures and test-owned state are removed.

## Tool policy and Grok request qualification, 2026-11-08

The selected root-cause follow-up uses the project loader/trusted transport, pinned OpenAI Python **3.37.0**, isolated state, serial sends and no hidden retries. Each send fixes **2048 output tokens**, a **120-second deadline**, **256-KiB requests / 2-MiB responses**; gateway runs use concurrency **1**. GLM Responses retains the existing reply-only ceiling scope. Native responses are buffered, not incremental-publication measurements.

| Boundary | Requests / attempts and result | Qualified scope |
|---|---|---|
| Qwen/GLM Responses tool-policy discrimination | **12/12**, all HTTP 200 | Each model returns one call under string required/serial false when prompted for two, then zero calls under required-subset selection with a conflicting no-tool prompt. String required independently returns zero calls in an initial sample and three explicit repeats per model. Native controls execute no tool. |
| Qwen/GLM gateway serial-auto loops | **3/3 each**, all complete | SDK SSE function call, unchanged returned-call feedback and exact final value; JSON same-session recall. Prepared `parallel_tool_calls:false` and auto/none choices match the client requests. No strict opt-out is used: these profiles explicitly request non-strict tools. |
| Grok native request-shape discrimination | **4/4**, all HTTP 200 and completed | No tools or cancellation. Starting with the current developer/user/user shape, change only developer→system, then the instruction type discriminator, then user arrays→strings. Every sample answers the prior arithmetic task and omits the recovery marker. This supplies no evidence for a role/carrier rewrite or cancellation-specific cause. |

Independent directional regressions fail on the old Bailian serial/forcing behavior and pass after repair. They preserve explicit strict rejection, source immutability, the Kimi serial boundary and model-qualified direct-lowering checks; the native GLM negative first proves its unforced baseline is expressible without a reply-only source cap. A separate Grok request audit checks exact UTF-8 developer text, consecutive user order, retained declarations and absent versus explicit-none tool policy. It is coverage of the existing request contract, not a Grok content repair.

The full offline baseline passes **436 library, 424 semantic, 96 transport and 30 gateway tests**, all-target Clippy under `-D warnings`, formatting, production-only compilation, **20 pinned assets** and Python suites (**8 upstream / 16 SDK-oracle / 47 client-oracle / 17 OpenAPI**). Separate synthetic acceptance passes all **20 SDK scenarios** and the **three affected current-client Responses history/cancellation gates**, on pi **1.5.0** and OpenCode **2.0.32**.

Four earlier gateway startup trials send no inference: the disposable Messages-oriented setup included a Messages-only default-output field on Responses bindings. Correcting only that test-owned configuration yields the two loops above. Body-free evidence is retained in `testdata/reports/ar5-tool-{serial-native,required-native,serial-gateway}-20261108.log` and `ar5-grok-shape-native-20261108.log`. Temporary request/response files are deleted; no private output becomes a fixture.

## Selected root repairs and qualification, 2026-11-09

Source evidence uses the user-selected local Google SDKs, Python **2.33.0** and JavaScript **2.21.0**; [source distinctions](refs/2026-10-27-official-sdk-sources.md#google-function-argument-carriers-2026-11-09) retain their separate scope. Pinned OpenAI Python **3.37.0** consumes the gateway. Installed-client synthetic acceptance uses current unwrapped **pi 1.5.0 / OpenCode 2.0.32**.

The preceding **2026-11-08** preparation sends **24 serial native requests** (one per case, 2048 tokens / 120 seconds / 2 MiB, no retries). Qwen thinking-off required-subset forcing passes **3/3** against a no-tool prompt while auto returns zero calls. Google ANY forces calls while VALIDATED permits text; ANY also emits two calls from one declaration. Basic schema constraints pass, but both validation spellings ignore the tested length constraint. A root `$ref` object succeeds natively and fails the former local check. Interactions' initial terminal oracle incorrectly excludes `requires_action`; its corrected repeat accepts that legitimate call-bearing terminal without weakening the length-failure verdict. Body-free logs remain in `testdata/reports/ar5-root-native-*-20261108.job`.

Every real story fixes one account and one binding, disables SDK/gateway retries, uses serial dispatch and concurrency **1**, **2048 output tokens**, **120-second route/exchange deadlines**, **256-KiB requests / 2-MiB responses** and a **420-second outer bound** per three-request story. The production loader alone reads credential/account inputs. All workspaces, captures and private account subsets are disposable.

| Boundary | Passing result | Qualified scope |
|---|---|---|
| Qwen Responses forcing with thinking off | JSON **3/3**, SSE **3/3**, all complete | `dashscope-flash-responses`, explicit non-strict tools, named forcing lowered to required subset with `parallel_tool_calls:false`; conflict prompt, exact arguments, untouched returned-call feedback and exact recall. Prepared thinking stays off. No default-pi strict acceptance is claimed |
| Google strict managed continuation | generateContent and Interactions each pass JSON **3/3** and SSE **3/3** | Required invocation, unchanged closed-object/string/numeric-enum schema, call/result identities and exact recall through `store:true` / `previous_response_id`; upstream remains stateless |
| Google automatic validated subset | Each API passes an additional JSON **3/3** | Separate native validation spellings; nested array/object, optional property, boolean type and fractional enum `1.25` remain in the original schema and pass adversarial-argument validation and continuation |
| Capture continuation | Focused security regression passes | Fails on the old stream-wide rejection; later complete call/terminal units remain sanitized after 64 rejected fragments, one gap marker preserves quota, framing failure is irreversible and the source remains `incomplete` |
| Referenced parameter schemas | Independent non-strict wire regression passes | Fails on the old literal-root-type check; the original `$ref` object document reaches native `parametersJsonSchema` without rewriting |

The **2026-11-09** collection contains **18 independent scenario executions / 36 inference attempts**: **30 HTTP-200 attempts and 6 HTTP-400 attempts**. The four native controls isolate plain nested schema, integer enum `9007199254740993`, fractional enum and boolean enum. The large-integer control rejects upstream; the boolean-enum control returns an out-of-enum Boolean despite HTTP 200. These results narrow local strict admission; boolean enums, unsafe integer enums and precision-changing numeric literals now reject before dispatch. Earlier combined nested controls account for the other five HTTP-400 attempts; they do not identify the individual cause.

One earlier nested managed story completes **3/3** and passes content assertions, then its disposable observer writes literal newline escapes into its capture ledger and fails JSON parsing. Fixing only that observer yields the two final passing automatic-validation stories above. Initial strict SDK fixtures also exposed an obsolete `parsed_arguments is None` assertion: strict SDK accumulation legitimately returns parsed arguments, now checked against the unchanged raw JSON. Shared non-strict gateway fixtures retain their original independent expectations.

The full offline baseline passes **436 library, 426 semantic, 97 transport and 30 gateway tests**, all-target Clippy under `-D warnings`, formatting, production-only compilation and **20 pinned assets**. Separate Python suites pass **8 upstream / 16 SDK-oracle / 47 client-oracle / 17 OpenAPI** checks. Connected synthetic acceptance passes **20 SDK scenarios** and **25 current installed-client scenarios**. The initial installed-client command stops before launch because explicit safe binary paths are missing; supplying those paths yields the passing run without changing product code or assertions.

Body-free evidence is retained locally at `testdata/reports/ar5-selected-*-20261109.job`; private captures and disposable helper sources are removed. Managed continuation is the selected ordinary-signature path, not qualification of stateless Chat/Messages or reconstructed Responses history. R01/R09 prerequisites and R13/R21 content limitations remain independent.

## Bailian function-guarantee follow-up, 2026-11-09

This section retains the native counterexamples and pre-selection dispositions. Current function-strict admission follows the subsequently selected [shared-service policy](architecture.md#shared-aliyun-billing-dialect).

[Current service sources](refs/2026-10-19-bailian-compatible-requests.md#function-guarantee-recheck-2026-11-09) distinguish function-schema guarantees from output-text strictness, request inventory from allowed names, and GLM **5.3** from hybrid 5.2. No new model, binding or endpoint is activated.

All **six native requests** are serial, loader-bound to one declared `aliyun-dashscope-cn` account, **2048 output tokens / 120-second total deadlines / 256-KiB requests / 2-MiB responses**, with one send each and no retries. They use the same closed object, required typed fields, integer/string enums and conflicting prompt. The disposable oracle checks positive/negative examples before dispatch; all six exchanges return HTTP 200 and complete.

| Native contrast | Result | Disposition |
|---|---|---|
| GLM 5.3 Chat, auto, strict false / true | False returns no call; true returns one correctly named call with valid object/types/closure/required fields but **both enums violated** | The true sample directly exercises and fails the argument guarantee; retain R17 strict rejection |
| Qwen 3.8 Flash Responses, thinking off, auto, strict false / true | False returns one enum-conforming call; true returns no call | Auto permits no call; this pair cannot qualify argument enforcement |
| Same Qwen profile, qualified required-subset choice, strict false / true | Both return exactly one correctly named call and **both enums violated** | R18 strict failure also persists without thinking; do not broaden strict admission |

R18's separate local over-rejection is repaired through `generation.filter-allowed-inventory.v1`: a singleton forced subset can select one original function from several declarations. The independent root/inline regression fails at `Admission(ToolChoice)` on the old implementation and passes after the repair, preserving selected schema, raw historical arguments, call/result identities, seriality and source immutability. Direct lowering agrees with endpoint encoding; thinking-enabled forcing and unrestricted multi-function `required` still reject at `ToolChoice`. The focused Bailian suite and directional cardinality check pass.

Actual production CLI + pinned OpenAI Python **3.37.0** accepts **JSON 3/3 and SSE 3/3** on `dashscope-flash-responses`, thinking disabled and tools explicitly non-strict. Both stories declare lookup and decoy, force lookup against a no-tool prompt, preserve the exact selected schema and `parallel_tool_calls:false`, then complete original-call feedback and exact recall. Captured preparation has one definition only on the forced request; later `none` requests retain both declarations and unchanged call/result history. Budgets are three serial requests/attempts each, concurrency one, the native output/wire/deadline bounds above, standard body-timeout defaults and a 420-second outer bound per story.

The full offline baseline passes after the repair, including all-target Clippy `-D warnings`, formatting, production compilation, pinned assets and the four Python suites. Separate connected synthetic acceptance passes **20 SDK scenarios / 25 installed-client scenarios**, using safe unwrapped **pi 1.5.0 / OpenCode 2.0.32** and isolated state.

Disposable setup failures occur before inference: one exporter type error, a body-timeout value outside documented bounds and the Messages-only `default_output_tokens` candidate field. Correcting the temporary setup yields the passing stories without changing product configuration validation. The initial regression fixture also needs an ownership correction before reaching its intended admission failure. Helpers, scratch stores and private captures are removed; body-free evidence is retained in `testdata/reports/ar5-bailian-functions-*-20261109.job`.

## Shared Bailian strict relaxation, 2026-11-09

The user selected one [default-relaxation policy](architecture.md#shared-aliyun-billing-dialect) for DashScope and TokenPlan, with positive exceptions only for independently verified model/API pairs. The current set is empty. Supplied schemas and original source strictness remain intact; the accepted loss is best-effort arguments, not newly enforced schemas.

Offline directional checks cover both plans, Chat/Responses, root and retained inline definitions, explicit true/false and conditional omission, unknown models under permissive direct contracts, and unchanged standard-API behavior. Endpoint encoding and direct lowering agree. Discovery reports `approved_adaptation` with `bailian.relax-function-strictness.v1`, rather than `faithful`. The original inline-wire expectation was corrected to its standard batch spelling `strict:false`; the source guarantee is still relaxed.

| Real consumer | Passing result | Independent observations |
|---|---|---|
| Current unwrapped pi **1.5.0**, `dashscope-glm-chat` | **3 requests / 3 complete attempts** | A curated read/feedback/recall profile explicitly enables client strict serialization: every client declaration carries `strict:true`, prepared declarations omit it and preserve the exact parameter schema. Actual scratch read succeeds; returned call IDs/arguments, result association and exact recall survive. Low effort emits no readable reasoning in this sample |
| OpenAI Python **3.37.0**, `dashscope-flash-responses` | JSON **3/3**, SSE **3/3** | Every SDK request uses `strict:true`; native strict is omitted while schema, thinking-off, forced invocation and original-call feedback/recall remain intact |

Each story fixes one loader-selected account, concurrency **1**, no retries, **2048 output tokens / 120-second route/exchange deadlines / 256-KiB requests / 2-MiB responses**, and a **420-second outer bound**. An initial pi observer required nonempty reasoning and aborted its second ingress after **one complete upstream attempt**. The target-appropriate repeat permits absent reasoning but compares any observed reasoning exactly, then passes the full story. Total real inference attempts are **10**: that initial attempt plus the three passing three-request stories. No new strict-enforcement exception or separate TokenPlan live qualification is inferred.

The full offline baseline passes, including production construction, all-target Clippy `-D warnings`, formatting, production compilation, pinned assets and the four Python suites. Separate synthetic acceptance passes **20 SDK scenarios / 25 current-client scenarios**. Production-lane and discovery fixtures now expect the selected relaxation instead of obsolete `strict:false`/faithful assumptions; their schema and history comparisons remain.

Temporary helpers, scratch credentials and private captures are removed; body-free evidence is retained locally as `testdata/reports/bailian-strict-relaxation-*-20261109.job`. The earlier native enum failures remain the reason to use best-effort behavior rather than add a verified exception.

## Updating this record

Append evidence to the relevant observation or add an identifier for a distinct phenomenon. Record the date, actual client version, selected model/binding/protocol, bounds, counts and failure boundary; retain only redacted summaries, not credentials or private payloads. Remove repaired and accepted entries and their obsolete failure/verification narratives; retain unresolved portions of mixed entries. Keep unmet prerequisites explicit.

Do not infer a product defect from an observer/configuration failure, silently relax a guard to obtain a pass, or promote a passing sample into broader acceptance. Add focused evidence as repairs proceed and retain explicit unknowns. Accepted design decisions belong in [Architecture](architecture.md), while repair ordering, deferrals and the partial-delivery exit belong in [Roadmap](roadmap.md#remaining-observation-dispositions).
