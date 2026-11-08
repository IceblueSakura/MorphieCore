# AR5 Review

This user-requested record owns remaining AR5 observations and concise passing-test evidence. Completed repairs and their acceptance narratives are removed; Git preserves their history. The [remaining observation dispositions](roadmap.md#remaining-observation-dispositions) and user-approved [partial-delivery exit](roadmap.md#partial-delivery-exit) own repair/defer scope; [Architecture](architecture.md#conversion-ladder) owns accepted conversion policy.

This record retains the dated scenario coverage. Further qualification follows [real-Agent scenario selection](development.md#real-agent-scenario-selection) and the [remaining dispositions](roadmap.md#remaining-observation-dispositions), without repeating the completed broad sweep.

## Contents

- [Test context](#test-context)
- [Pending observations](#pending-observations)
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
- [R19 matched transport qualification, 2026-11-08](#r19-matched-transport-qualification-2026-11-08)
- [Updating this record](#updating-this-record)

## Test context

Baseline date: **2026-11-06**. Installed clients: unwrapped **pi 1.5.0** and **OpenCode 2.0.32**; SDKs use the project pins. Real tests use isolated state, the project credential loader, serial requests, a **2048-token output ceiling**, **120-second deadlines**, and a fixed request budget per profile without harness retries.

Supplementary date: **2026-11-07**, with the same installed client versions and a current `cargo build --locked --offline --bin morphiecore`. The sweep retains bundled compatibility and `thinkingLevelMap` metadata, uses synthetic workspaces and temporary captures, and reports each case independently. A runner's exit code is not the case verdict; the bounded runners emit per-case JSON outcomes.

Supplementary collection totals, including qualification reruns and failed observers: **61 case executions, 162 client requests, 157 upstream attempts**. Gateway outcomes are 150 complete, 6 interrupted and 6 error; five errors have no upstream attempt, and one follows upstream HTTP 200. A complete delivery can still fail the scenario's control/content assertion. Two additional synthetic-only OpenCode tool-declaration characterizations are excluded from these totals.

The subsequent user-selected Bailian sweep separately covers `qwen3.8-flash`, `glm-5.3` (the requested GLM5.3) and `deepseek-v4.1-flash-1017`, each through native Chat and Responses. A new API-key test account validates through the project CLI. Bailian-only totals are **77 case executions, 103 client requests, 101 upstream attempts**: 98 native-companion attempts, 3 gateway attempts and 2 local admission failures. Native companions use the project account loader and trusted HTTP transport with unchanged official SDK request bodies, one send, a 120-second total deadline, 256 KiB requests and 2 MiB responses. They request native Chat `max_tokens:2048` or Responses `max_output_tokens:2048`; the legacy Chat field retains its native ceiling semantics. Responses are buffered before SDK consumption, so incremental publication is not measured.

Cross-protocol profiles reuse bundled model metadata with an explicitly selected serializer. They do not qualify native Google client APIs. DeepSeek uses `deepseek-beta-chat`: public `deepseek-v4.1-flash-1017`, canonical/upstream `deepseek-flash`; OpenCode's passing built-in profile uses the canonical label.

Collection totals retain their original execution scope when resolved entries are removed. Counts below are scoped to the stated tests. Local admission, client configuration, observer failures, upstream rejections and unexercised prerequisites remain distinct. [Development](development.md#production-profile-acceptance) retains the detailed baseline and verification procedures; [Roadmap](roadmap.md#partial-delivery-exit) owns AR5's exit boundary.

## Pending observations

Entries retain unresolved boundaries and their scoped evidence, not an implementation queue. The roadmap owns the [scoped limitations](roadmap.md#scoped-limitations), [unavailable prerequisites](roadmap.md#next-stage-analysis) and [reopened Provider/model observations](roadmap.md#reopened-provider-and-model-observations). Reopening permits follow-up analysis; each repair or qualification still follows its boundary-specific selection and prerequisites. A passing sample closes only its exercised boundary; remove completed portions without renumbering the remaining identifiers.

| ID | Observation and affected path | Current evidence and unresolved boundary |
|---|---|---|
| R01 | Readable-Chat replay projection: OpenCode→Haiku, Grok and Interactions | The dated samples report `representation.replay_origin` at `event.project`: Haiku 3 attempts / 2 complete; Grok and Interactions 1/0 each. Offline discrimination confirms that a correctly bound final replay value also fails when Readable Chat lacks its carrier. Current diagnostics distinguish carrier, origin/dependency and finality failures; no new real sample identifies the historical failing predicate. A client-retained signed carrier remains unselected. |
| R02 | Stateless generateContent signature carriage | OpenCode Chat/Responses and the Responses SDK smoke reach upstream HTTP 200, then fail projection with `codec.unsupported.replay`; ordinary text can carry `thoughtSignature`. Managed continuation passes separately. No disposition for discarding or substituting these attachments is selected. |
| R04 | DeepSeek whole-empty Responses owner and returned history | The real sample makes **1 request / 1 attempt / 1 complete**; its second submission is blocked by the **test authority's** owner-count guard, not production admission. Direct synthetic OpenCode 2.0.32 loops omit empty-content and empty-text owners, including a phase-bearing owner, while preserving a whitespace owner and call/result links. The bundled Responses parser suppresses empty text before Agent storage. Whole-owner equivalence and a client retention repair remain unresolved; the qualified Messages text-block rule does not cover this owner. |
| R05 | pi Google Responses strict-profile qualification | Both original Responses profiles remain locally rejected after ordered-control projection. Three bounded local checks make **0 upstream attempts**; the final generateContent discriminator reports `admission.strict_tools`. Independent preparation tests preserve explicit strict rejection on both bindings. No strict opt-out is selected. |
| R06 | pi Google generateContent Messages stateless carriage | The original Messages profile now reaches generateContent: **1 request / 1 attempt**, then `codec.unsupported.replay` on signed stateless delivery. The Interactions read/result/answer/recall loop passes separately below; only generateContent's carrier limitation remains pending. |
| R08 | OpenCode error-path resubmission | Historical failed cross-protocol profiles offer an additional client submission; test admission blocks it before forwarding. Direct synthetic HTTP/SSE explicit errors terminate after one POST. Truncated streams produce a second POST: after visible text it includes the partial assistant answer and a new user message; before text it repeats the same body, leaving the client retry layer unresolved. These controls bypass the gateway and do not identify the historical failure's exact wire shape or establish that every SDK retry path was disabled. |
| R09 | Real replay isolation and failover prerequisites | Separate JSON and SSE Luna→Grok probes each stop after one successful Luna source request because no opaque replay is returned. No transfer request is sent. No independent compatible account pair is available for same-model replay/failover; same-key aliases and different-model switching do not qualify that gate. These are unexercised prerequisites, not observed isolation failures. |
| R13 | Dynamic-tool/instruction literal-label recall failure | The 2026-11-08 qualification runs **7 stories / 35 attempts**: two full passes, four recall failures and one final-request upstream HTTP 400. All recall failures retain the exact label in preparation and the filename in the answer. Three have complete native text capture: the upstream already omits the label, including under case-folded search, and native/delivered/pi text agrees exactly. Active declarations remain `[read]`, `[read]`, `[ls]`, `[ls]`, `[]`. One other failed recall has incomplete native capture and supplies only delivery/pi agreement. |
| R17 | pi GLM Chat strict-profile admission | 2026-11-07, pi 1.5.0, `dashscope-glm-chat`: the original read/history profile rejects locally with `400 invalid_request`, 0 upstream attempts and planning reason `admission.strict_tools`. Pure preflight isolates explicit strict tools: false/omitted controls encode, while true rejects at capability admission. These non-strict controls are diagnostic variants, not strict opt-outs for acceptance. |
| R18 | Bailian explicit tool selection and strict arguments | Three 2026-11-08 native discriminators each run an initial sample plus three explicit repeats, all HTTP 200: nonthinking Qwen Chat always selects forced `lookup` but reports `stop`; Qwen Responses strict/auto always emits the out-of-enum value; thinking GLM Chat always chooses prompted `decoy`. Qwen's correctly forced call additionally completes a **3-request gateway SDK feedback/recall loop**, preserving `stop`. Responses strict tools and thinking-mode named forcing retain their unsupported-profile boundaries. Earlier GLM Responses required-selection failures and unexercised strictness remain model/API-specific limitations, not qualified guarantees. |
| R19 | Bailian structured-output content and variable native SDK envelope | The matched GLM qualification makes **12 real sends** across native JSON, native SSE and gateway JSON. Strengthened JSON Schema checks observe both enum violations and conforming content on all three paths. Two complete gateway observations independently preserve the entire prepared body and native/delivered text, one failing and one passing the schema. The passing sample also normalizes native `schema_` for strict SDK consumption; direct native SDK rejection remains. HTTP 400, unsuccessful native streams and incomplete observations are separate. Qwen's prior JSON/SSE qualification remains scoped; DeepSeek and TokenPlan are not covered. |
| R21 | Grok recovery-label assertions and newly issued recovery tools | The 2026-11-08 pi thinking-cancel story passes exact recovery. Three effective tool-argument stories interrupt honestly, create no file before recovery, then execute a newly issued write with tools declared; the two-request test budget blocks its feedback. A fourth story misses the cancellation window. Native no-cancellation controls also miss markers and can issue writes; explicit `tool_choice:none` suppresses calls but still misses the marker. The offline three-request Responses loop separately verifies no cancelled-call execution, a distinct recovery call and its legitimate feedback. Grok content behavior and cancellation-specific causality remain unqualified. |

## Passing tests

These results are retained independently of the pending observations. They qualify only the listed profiles, not AR5 closure or untested protocol combinations.

| Test group | Passing result | Scope |
|---|---|---|
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
| pi cross-model session workflow | Haiku Messages → DeepSeek readable Chat → Haiku Messages: 4/4 | Actual `set_model` operations preserve the client session and visible read/result recall. This is a client serializer/model-switch workflow; account-compatible failover and forced opaque transfer remain R09. |
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
| Native JSON Schema conflict sample | Qwen Chat returns the exact closed shape with an allowed numeric enum and Boolean | One request/attempt. Other model/protocol results remain R19; this is not an arbitrary-schema or internal constrained-decoding qualification. |
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
| Native Bailian isolated controls | 17 single-request invocations, 16 HTTP-200 exchanges and 1 transport-failed invocation | Two-tool named selection, matched strict false/true auto-choice pairs, and Responses schema-envelope qualification. The failed GLM Responses false sample and its one explicit retry both remain recorded. The original helper did not retain the failed invocation's send boundary, so its missing attempt count is unknown rather than evidence of zero sends. R18/R19 retain individual verdicts. |

Pure offline controls additionally exercise **25 synthetic client-decode → endpoint-preparation cases**, without credentials or Provider calls. They separate strict tools, inline declarations, ordered effort, adaptive/block-binding admission and public unknown-field discard. Diagnostic non-strict variants are not promoted into default-profile Agent acceptance.

All raw native responses and gateway captures are temporary and deleted; only body-free verdicts are retained locally. No product source, returned history, production guard, binding activation or deployed service is changed to obtain a pass. Remaining failures retain their roadmap dispositions. Account-compatible replay/failover still needs the R09 prerequisites; Google controls and cross-protocol replay retain their qualified boundaries.

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
| Tool-argument cancellation and recovery | Three separate 2-attempt stories, each interrupted → complete | Two original 1200-word tasks and one separately labelled 100-word task. No incomplete tool executes before recovery. Recovery generates a new write with tools still available, after which pi submits feedback; all three excess feedback requests are blocked before forwarding. Instrumentation confirms no file before recovery and actual new write execution during recovery (R21). |
| Observer qualification | 1 interrupted request; no recovery request | The first instrumented thinking observer reads a JSONL record while it is only partially written and raises `JSONDecodeError`. The test-owned reader is corrected to wait for newline-complete records while retaining strict parsing of completed lines; the subsequent 2-attempt story above remains a content-assertion failure. |
| Fresh-context instruction controls | 5 requests / 5 attempts / 5 complete; all exact-label assertions fail | Typed pi-ai API companion, no cancellation or returned history: recovery-only instruction with “ignore prior task”, two consecutive users, the same two instructions within one user message, bare `RECOVERED`, and bare `CEDAR_OK`. The consecutive and merged variants return the first arithmetic answer; other variants also miss their exact-label oracle. These are diagnostic inputs, not normalization of an Agent's returned history. |

The fresh-context failures leave the exact-label recovery oracle unqualified as a cancellation-specific check. Tools remain declared in the tool stories and no explicit no-tool API guarantee is requested. R21 retains the observed content/tool behavior without assigning a gateway defect or selecting an adaptation. Gateway single-permit release and honest interruption are observed; remote computation/billing termination is not measured. R09's compatible-account/replay prerequisites and the other blocked protocol lanes remain open.

Private captures are deleted with each case; body-free verdicts and complete runner logs are saved locally. No deployment, binding activation or account replacement is performed by this sweep. Remaining repair/defer dispositions belong to the roadmap.

## Binding and independent rechecks, 2026-11-07

The user selected the three missing Bailian catalog bindings and independent repeats of unexplained failures. Tests use current unwrapped **pi 1.5.0**, **OpenCode 2.0.32**, pinned **OpenAI Python 3.37.0**, the project account loader and trusted transport, isolated state, serial sends and no hidden retries. Each live case fixes its request budget before starting, **2048 output tokens**, **120-second deadlines**, **256 KiB requests / 2 MiB responses**, and gateway concurrency **1**. On non-Qwen thinking-enabled Bailian Responses, 2048 is the selected native reply ceiling, with the existing total-to-reply loss receipt; byte/time bounds remain independent.

| Boundary | Observed result |
|---|---|
| Bailian DeepSeek Chat | New `dashscope-deepseek-chat`: **4 requests / 4 attempts / 4 complete**; strict SDK JSON arithmetic, SSE function call/result/final answer and unchanged returned-history recall pass. Explicit non-strict tools and native low effort; no native-beta strict guarantee is inherited. |
| Haiku argument cancellation | Two independent **2/2** single-permit stories record `interrupted → complete`; one passes the full recovery oracle. The other reaches observer parsing/excess-feedback boundaries after gateway completion. |
| Native structured output | Initial four fixed one-send invocations: one Qwen transport failure and three HTTP-200 responses. A subsequent user-capped batch sends **exactly 3 requests total** (Qwen once, GLM twice), all HTTP 200: Qwen satisfies the schema; GLM fails once and passes once. All three reproduce the same SDK envelope and original/canonical-schema/text-removed codec failures. Same 2048-token, 120-second, 256-KiB/2-MiB bounds, serial sends and no retries or replacement calls. The earlier transport failure remains separate; remote receipt was unconfirmed. |

Four preceding gateway schema controls stop at local admission, and test-owned constructor/account-file failures stop before dispatch. These are excluded from native schema evidence. Private response/capture files are deleted per case; body-free outcomes remain separate from scenario verdicts.

The full offline baseline passes: **436 library, 415 semantic, 94 transport and 30 gateway tests**, all-target Clippy under `-D warnings`, formatting, production-only checks, **20 pinned assets** and the **8 upstream Python tests**. All **19 connected SDK scenarios** pass against synthetic authorities.

## Capped remaining-observation recheck, 2026-11-07

The user requested the exact undated DeepSeek label and at most **three scenarios per remaining observation**, allowing for Provider instability. The batch uses the same current clients, SDK, loader, bounds and serial/no-retry policy above. **43 independent scenario executions / 73 upstream attempts** cover the selected paths; R08 shares the three R01 executions and adds no sends. Each scenario fixes its request ceiling before dispatch: one for admission/native controls/stateless smoke, two for cancellation, three for ordinary Agent/replay loops, four for the Responses SDK loop, five for tool switching and nine for repeated compaction. Excess client submissions stop before forwarding.

| Observation | Executed scope and result |
|---|---|
| R01 | Three Readable Chat targets: Haiku **3 attempts / 2 complete**, Grok and Interactions **1/0 each**. All reproduce `representation.replay_origin` at `event.project`. |
| R02 | OpenCode Chat, OpenCode Responses and stateless SDK smoke: **1 attempt each**, all repeat `codec.unsupported.replay`. |
| R04 | OpenCode Responses makes **1 complete attempt**, then the unchanged history guard blocks forwarding after omission of a zero-length assistant owner. The separately qualified Messages block case is removed from this retained boundary. |
| R08 | All three failed R01 OpenCode scenarios offer one additional submission after the authority error latch. Each is blocked before forwarding; their upstream counts remain **3, 1, 1**. |
| R09 | JSON and SSE each complete one Luna source attempt, then stop because opaque replay is absent. No cross-target transfer is sent; compatible-account failover prerequisites remain unavailable. |
| Repeated compaction | Two independent stories pass **9/9 each**, including both exact no-tool recalls and rebuilt read. |
| R13 | Three unchanged tool-switching stories, **5/5 each**: one missing-literal-label assertion failure, then two passes. Ingress retains both facts in all three. The final diagnostic checks prepared/delivered/client fact presence, issued calls and terminal independently. |
| R17 | Two original strict-profile stories reject at local admission with **0 attempts**; no strict opt-out is applied. |
| R18 | Three one-send HTTP-200 controls: Qwen Chat chooses `lookup` but reports `stop`; GLM Chat chooses prompted `decoy` instead of forced `lookup`; Qwen Responses strict-true/auto emits out-of-enum 93. Each reproduces its selected boundary; other model/API/control cells are not rerun. |
| R19 | Three one-send HTTP-200 controls: Qwen content and SDK envelope pass; GLM content passes once/fails once and its SDK envelope rejects both. The former unscoped codec diagnostic is retired; current qualification below separates actual preparation and Provider boundaries. |
| R21 | Thinking and argument cancellation each record **interrupted → complete** with **2 attempts**, but fail recovery-label assertions. The argument story leaves write declared: recovery issues and executes a new write, then excess feedback is blocked. A one-attempt fresh-context bare-label control also fails. Ingress/prepared recovery instructions remain present; cancellation-specific causality stays unestablished. |

The third R13 report's ad hoc native-text selector has unestablished coverage; its negative flags are not a qualified upstream-content oracle. Only the independently checked prepared/delivered/client fact presence is retained from that diagnostic. No production mapping or history guard changes were made. Body-free logs are saved locally under `testdata/reports/ar5-capped-20261107-*.log`; private exchange files are deleted with each scenario.

## Volatile-observation recheck, 2026-11-07

The next user-selected batch covers repeated compaction, R13 and R19, **three independent scenarios each / 45 upstream attempts**, with the same clients, SDK, bounds and serial/no-retry policy. All 45 attempts return HTTP 200. Per-scenario request ceilings remain **9**, **5** and **1**, respectively; scenario assertions are checked independently of successful delivery.

| Observation | Latest three-scenario result |
|---|---|
| Repeated compaction | **9/9, 9/9, 9/9**, all passing. Each story verifies two compactions, exact recalls without issued/executed tools, fact presence in summary/context/ingress/answer and rebuilt read. Together with the preceding two stories, five consecutive capped stories pass completely. |
| R13 | **5/5 each** complete, but all three fail the literal-label recall assertion. Filename presence and no-tool checks pass; ingress retains both facts and the active declarations remain read/read/ls/ls/none. The latest reports separate the failing assertion dimension, not an established codec or history-loss cause. |
| R19 | Qwen: canonical schema, conforming content and strict SDK validation pass. GLM: one conforming and one nonconforming output, both with `schema_` and strict SDK failure. The former unscoped codec diagnostic is retired. |

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

Two earlier post-fix Chat stories each complete two attempts, then the test authority races next-request admission against recording the previous forwarded EOF. Synchronizing that observer, without delaying/replacing response frames or changing product behavior, yields the final three passing stories above. The earlier Messages story independently passes **3/3**. A one-send Google Interactions admission probe completes, then its excess feedback is blocked before forwarding; the separate three-send loop above provides full-loop evidence. OpenCode's whole-empty Responses owner omission remains R04.

The final offline baseline passes **436 library, 418 semantic, 96 transport and 30 gateway tests**, all-target Clippy under `-D warnings`, formatting, production-only compilation and **20 pinned assets**. Separate Python suites pass **8 upstream, 17 OpenAPI, 16 SDK-oracle and 45 client-oracle tests**. All **19 connected SDK scenarios** and **22 isolated installed-Agent scenarios** pass, including native defaults, queued stored/stateless Responses history, recovery, cancellation and summaries.

Private exchange files are deleted with each scenario; body-free evidence is retained locally. No deployment, account replacement or new binding activation occurs. Remaining repair/defer dispositions and the partial-delivery handoff belong to [Roadmap](roadmap.md#remaining-observation-dispositions).

## Follow-up qualification, 2026-11-08

The user selects **R19 → R18 → R13 → R21**, permits project-managed OAuth renewal, and allows an initial failing sample plus at most **three explicit repeats**. Every send remains serial and fixes its target, request count, **2048-token ceiling**, **120-second deadline**, **256-KiB request / 2-MiB response bounds** and gateway concurrency **1**. Gateway/SDK hidden retries remain off. Clients are current unwrapped **pi 1.5.0 / OpenCode 2.0.32**, with pinned **OpenAI Python 3.37.0**.

| Boundary | Result | Qualification |
|---|---|---|
| R19 native diagnostic provenance | **3 requests / 3 attempts**, all HTTP 200; scoped decode accepts original, canonical-schema and text-removed forms, while unscoped decode rejects each original | Classified penalty reports need trusted origin. Production endpoint adapters already supply it; no origin guard is weakened. Native `schema_` still fails the standard SDK. |
| R19 gateway format preparation | Before repair: **2 local rejections / 0 attempts**. After repair: **6 complete attempts**, with strict SDK delivery throughout | Qwen JSON/SSE both satisfy the conflicting schema oracle. GLM's initial JSON sample and three explicit repeats all fail content conformity; the repeated samples independently verify unchanged prepared `text.format`. No content repair or weaker strictness is selected. |
| R18 native controls | **12 requests / 12 HTTP-200 attempts** | Each of three discriminator profiles retains its result through three repeats. Tool identity, strict arguments and terminal spelling are evaluated separately; unsupported native diagnostic profiles do not become gateway acceptance. |
| R18 Qwen Chat gateway loop | **3 requests / 3 complete attempts** | Explicit non-strict, nonthinking profile: forced `lookup`, untouched returned-call feedback and exact same-session value recall. Native `stop` is retained. This is SDK qualification, not default-pi strict acceptance. |
| R13 instrumented Agent stories | **35 attempts: 34 complete, 1 upstream HTTP 400** | Two stories pass, four fail recall and one stops on the HTTP error. Complete captures in three failing recalls establish native omission with unchanged delivery/pi text; the other incomplete capture is not an upstream-content oracle. One failed story passes on its first repeat; another exhausts all three. |
| R21 current pi cancellation | **5 stories / 9 gateway attempts** | Thinking passes `interrupted → complete` with exact recovery. Three tool-argument stories interrupt honestly, create no file before recovery, then execute a new write; third submissions are blocked. The fourth tool story has one already-complete gateway attempt and misses the cancellation window. No claim of remote compute termination is made. |
| R21 native controls without cancellation | **9 HTTP-200 attempts** | Initial bare-label sample plus three repeats all miss the marker, including case-folded comparison. Initial two-user/tool sample plus three repeats all miss it; the first three issue a new write and the fourth issues none. One explicit `tool_choice:none` contrast issues none and still misses the marker. No native diagnostic executes a tool. |

Grok access was expired and one project-CLI OAuth renewal succeeds. Do not attribute the later thinking pass to renewal without a causal comparison. Six earlier native-observer invocations stop at missing OAuth driver registration with **zero inference sends**; the corrected helper supplies the native evidence above. Their unavailable matched-recovery controls are not claimed as acceptance.

The full offline baseline, all-target Clippy under `-D warnings`, formatting, production-only compilation, **20 pinned assets** and the four Python suites (**8 upstream / 16 SDK-oracle / 45 client-oracle / 17 OpenAPI**) pass. Separate connected acceptance passes **19 SDK scenarios** and **22 isolated current-client scenarios** on pi 1.5.0 / OpenCode 2.0.32.

Private wire bodies are deleted with each scenario. Only body-free verdicts are retained; the request/response shapes in synthetic regressions are independently authored, not Provider captures.

## Offline attribution, 2026-11-08

These checks use current unwrapped **pi 1.5.0 / OpenCode 2.0.32**, isolated homes, synthetic files and loopback authorities only. No real Provider, credential renewal or private exchange is involved. Direct OpenCode probes have a 30-second process bound and at most two POSTs per owner case or three per error case; subsequent error-case POSTs receive a local nonretryable HTTP 400. The pi gateway case permits exactly three requests, a 2048-token ceiling, 25-second route / 30-second exchange deadlines, 256-KiB upstream requests and concurrency one.

| Boundary | Evidence | Remaining limit |
|---|---|---|
| R01 replay attribution | A trusted final replay value without a Readable Chat carrier reports `replay_format`; a supported carrier with foreign origin reports `replay_origin`; partial Chat closure reports `replay_phase`. Independent positive Responses replay and source/identity checks remain passing. | No carrier activation, signature omission or reclassification of an unobserved historical event. |
| R04 client projection | Four direct Responses stories each complete **2 POSTs** and one actual read. `content:[]`, `text:""` and phase-bearing empty text omit the owner; `" "` preserves exact text, ID, status and phase. Call/result association survives all four. Installed bundled parser inspection locates suppression before text-start events and Agent history storage. | These synthetic shapes do not recover the exact private sample or prove empty-owner semantic equivalence. The original exact-history guard remains. |
| R08 error shape | Three explicit-error controls (HTTP 400, SSE error before text, SSE error after text) each make one POST. Two truncated-SSE controls each make two; instrumentation repeats preserve those outcomes. After-text truncation appends the partial assistant answer and a new user message to the next request and leaves two assistant records; before-text truncation repeats the identical body and leaves one record. | Persistence is asynchronous: database counts sampled at POST time alone cannot identify invocation boundaries. The after-text history establishes application continuation; the before-text retry layer and original live wire shape remain unresolved. |
| R21 cancellation ownership | The installed-pi Responses gate observes an unfinished argument delta, aborts before execution, observes upstream release and a non-complete gateway receipt, then executes one distinct new read and forwards its result. **3 requests / 3 attempts**, no cancelled-call execution or replay; outcomes are non-complete, complete, complete. | This qualifies identity/lifecycle and the feedback budget, not Grok marker adherence, write-specific behavior or remote compute termination. |

The direct characterization consists of **14 isolated executions / 24 POSTs**, including instrumentation repeats; no request is sent by MorphieCore in those probes. The durable cancellation gate is `pi_responses_tool_argument_cancellation`; it reuses the existing lifecycle authority and protocol fixtures rather than weakening historical assertions.

The full offline baseline, all-target Clippy under `-D warnings`, formatting, production-only compilation and **20 pinned assets** pass. Separate Python suites pass **8 upstream / 16 SDK-oracle / 46 client-oracle / 17 OpenAPI** checks; connected synthetic acceptance passes **19 SDK** and **23 installed-client** scenarios. Local evidence is retained under `testdata/reports/ar5-offline-20261108/`; disposable characterization scripts and test-owned state are removed.

## R19 matched transport qualification, 2026-11-08

At `0b44afb`, this collection fixes **GLM `glm-5.3`**, the existing DashScope Responses endpoint/account, low effort, unchanged synthetic prompt and strict schema, **2048 reply tokens**, **120-second** route/exchange deadlines and **256-KiB requests / 2-MiB responses**. Each of three profiles sends an initial sample and at most three explicit repeats: **12 real requests total**, serially, without hidden retries or replacement sends. The schema requires an integer `record` from `[71,82]` and Boolean `ok`; the prompt conflicts by requesting `93`.

Native JSON and SSE use the same offline-prepared body except `stream`, serialized by pinned **OpenAI Python 3.37.0** and sent once through the project account loader and trusted transport. Gateway preparation is independently compared with that intended body. All SSE is buffered before SDK consumption; this does not qualify incremental delivery or timing.

| Profile | Four real sends | Qualified content and envelope evidence |
|---|---|---|
| Native JSON | Three HTTP 200; one HTTP 400 | The final two completed samples independently produce an enum violation and conforming content. Completed native envelopes use `schema_` and fail strict SDK validation; the final sample explicitly identifies missing canonical `schema`. The first sample passes the earlier, stricter literal-integer oracle. |
| Native SSE | Four HTTP 200; one does not successfully complete | The final two completed samples produce an enum violation and conforming content, with deltas, text-done, content-part-done, item-done and terminal text agreeing. Strict EOF passes. Both still fail native SDK validation on `schema_`. The first sample's negative content verdict is not requalified under the strengthened oracle. |
| Gateway JSON, upstream SSE | Three complete SDK-valid deliveries; one HTTP 502 | Two completed deliveries violate the enum; the final delivery conforms. For the last failing and passing deliveries, complete source observation proves exact request preparation and unchanged native/delivered text. Their native envelopes use canonical `schema` and `schema_`, respectively; both delivered envelopes are canonical. The failed attempt has a non-successful native stream and gateway `event.decode` / `codec.invalid.settings_conflict`; its precise failed-envelope propagation remains unqualified. |

The initial gateway observer fails locally before any gateway/upstream request; it is excluded from the 12 real sends. Observer checks then use the existing locked `jsonschema` validator, accepting integral decimal spellings such as `71.0` while retaining exact enum comparison, duplicate-key and nonfinite-value rejection. Independent negative controls reject a changed content-part snapshot and unfinished trailing SSE. Earlier deleted samples cannot be reclassified from their literal-integer verdict alone.

Private production capture intentionally stops at a JSON-looking fragment that cannot be independently sanitized. It therefore cannot establish complete native text in this profile. The first dispatched gateway sample retains only its SDK/content verdict. Later samples use a test-owned loopback observer with synthetic gateway credentials; the observer delegates the unchanged body to the same real account/endpoint through the project loader, then forwards the bounded response bytes unchanged. It never reads or logs real authentication. Native bytes remain temporary and are deleted per sample. A synthetic full-chain check verifies split JSON text, `schema_` normalization and source/delivery equality while capture still rejects the unsafe fragment; no capture safety rule is weakened.

These controls establish scoped Provider content variability in **both** native delivery modes, and unchanged gateway delivery for a failing and a passing schema sample. They do not justify switching to native JSON as a reliability repair, weakening the schema, repairing generated content or retrying after publication. HTTP 400 and unsuccessful streams remain separate from completed-content violations; the small failure-triggered sample does not estimate a reliability rate. No product code or binding changes are needed for these observed completed paths. Body-free evidence is retained under `testdata/reports/ar5-r19-matched-20261108/`.

## Updating this record

Append evidence to the relevant observation or add an identifier for a distinct phenomenon. Record the date, actual client version, selected model/binding/protocol, bounds, counts and failure boundary; retain only redacted summaries, not credentials or private payloads. Remove repaired and accepted entries and their obsolete failure/verification narratives; retain unresolved portions of mixed entries. Keep unmet prerequisites explicit.

Do not infer a product defect from an observer/configuration failure, silently relax a guard to obtain a pass, or promote a passing sample into broader acceptance. Add focused evidence as repairs proceed and retain explicit unknowns. Accepted design decisions belong in [Architecture](architecture.md), while repair ordering, deferrals and the partial-delivery exit belong in [Roadmap](roadmap.md#remaining-observation-dispositions).
