# AR5 Review

This user-requested record owns remaining AR5 observations and concise passing-test evidence. Completed repairs and their acceptance narratives are removed; Git preserves their history. The [concentrated repair plan](roadmap.md#concentrated-repair-plan) and user-approved [partial-delivery exit](roadmap.md#partial-delivery-exit) own repair/defer scope; [Architecture](architecture.md#conversion-ladder) owns accepted conversion policy.

The [pre-Stage-4 pi-first real-Agent sweep](roadmap.md#pre-stage-4-real-agent-scenario-sweep) retains the collection inventory. Further testing is focused on affected repairs or a concrete unresolved boundary; the remaining inventory can receive explicit next-stage dispositions rather than blocking independent delivery.

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
- [Updating this record](#updating-this-record)

## Test context

Baseline date: **2026-11-06**. Installed clients: unwrapped **pi 1.5.0** and **OpenCode 2.0.32**; SDKs use the project pins. Real tests use isolated state, the project credential loader, serial requests, a **2048-token output ceiling**, **120-second deadlines**, and a fixed request budget per profile without harness retries.

Supplementary date: **2026-11-07**, with the same installed client versions and a current `cargo build --locked --offline --bin morphiecore`. The sweep retains bundled compatibility and `thinkingLevelMap` metadata, uses synthetic workspaces and temporary captures, and reports each case independently. A runner's exit code is not the case verdict; the bounded runners emit per-case JSON outcomes.

Supplementary collection totals, including qualification reruns and failed observers: **61 case executions, 162 client requests, 157 upstream attempts**. Gateway outcomes are 150 complete, 6 interrupted and 6 error; five errors have no upstream attempt, and one follows upstream HTTP 200. A complete delivery can still fail the scenario's control/content assertion. Two additional synthetic-only OpenCode tool-declaration characterizations are excluded from these totals.

The subsequent user-selected Bailian sweep separately covers `qwen3.8-flash`, `glm-5.3` (the requested GLM5.3) and `deepseek-v4.1-flash-1017`, each through native Chat and Responses. A new API-key test account validates through the project CLI. Bailian-only totals are **77 case executions, 103 client requests, 101 upstream attempts**: 98 native-companion attempts, 3 gateway attempts and 2 local admission failures. Native companions use the project account loader and trusted HTTP transport with unchanged official SDK request bodies, one send, a 120-second total deadline, 256 KiB requests and 2 MiB responses. They request native Chat `max_tokens:2048` or Responses `max_output_tokens:2048`; the legacy Chat field retains its native ceiling semantics. Responses are buffered before SDK consumption, so incremental publication is not measured.

Cross-protocol profiles reuse bundled model metadata with an explicitly selected serializer. They do not qualify native Google client APIs. DeepSeek uses `deepseek-beta-chat`: public `deepseek-v4.1-flash-1017`, canonical/upstream `deepseek-flash`; OpenCode's passing built-in profile uses the canonical label.

Collection totals retain their original execution scope when resolved entries are removed. Counts below are scoped to the stated tests. Local admission, client configuration, observer failures, upstream rejections and unexercised prerequisites remain distinct. [Development](development.md#production-profile-acceptance) retains the detailed baseline and verification procedures; [Roadmap](roadmap.md#ordered-work-and-exit-gates) owns AR5's exit boundary.

## Pending observations

Entries retain unresolved boundaries and their scoped evidence, not an implementation queue. The [roadmap](roadmap.md#concentrated-repair-plan) owns repair/defer dispositions. A passing sample closes only its exercised boundary; remove completed portions without renumbering the remaining identifiers.

| ID | Observation and affected path | Current evidence and unresolved boundary |
|---|---|---|
| R01 | Readable-Chat replay projection: OpenCode→Haiku, Grok and Interactions | `representation.replay_origin` at `event.project` after upstream HTTP 200. Haiku: 3 attempts / 2 complete; Grok and Interactions: 1 attempt / 0 complete each. The ownership/carriage cause remains unconfirmed; origin validation remains intact. |
| R02 | Stateless generateContent signature carriage | OpenCode Chat/Responses and the Responses SDK smoke reach upstream HTTP 200, then fail projection with `codec.unsupported.replay`; ordinary text can carry `thoughtSignature`. Managed continuation passes separately. No disposition for discarding or substituting these attachments is selected. |
| R03 | Luna cross-protocol continuation | Chat feedback gets `400 recovery.request` after 1 complete delivery / 2 attempts. pi Messages recall gets the same rejection after 2 complete deliveries / 3 attempts. Two new independent samples per protocol reproduce those exact request counts and rejection boundaries. The exact rejected upstream field remains unidentified. |
| R04 | DeepSeek empty carriers and returned history | After one complete delivery, pi omits an unsigned `{"type":"text","text":""}` block; visible text, tool-call values and thinking blocks compare equal. OpenCode omits a zero-length assistant message; its count guard stops later checks. Neither next request is forwarded. Generated ownership versus legitimate normalization remains unresolved; nonempty content loss is not established and history guards were not relaxed. |
| R05 | pi Google Responses profile qualification | generateContent and Interactions reject the original Responses profiles locally with 0 upstream attempts, first reporting `admission.message_effort`. Pure synthetic controls independently reject root strict tools on both bindings and inline non-strict tools with `admission.tool_changes`. Ordered-control projection and endpoint/model qualification remain open; no strict opt-out is selected for acceptance. |
| R06 | pi Google Messages admission | generateContent and Interactions reject locally with 0 upstream attempts. Targeted reruns identify `admission.message_effort` on both. Pure synthetic controls encode adaptive thinking plus `block_binding.prefix_mismatch_behavior:drop_block` without the ordered marker; adding message-local effort reproduces that rejection. Two initial follow-up runners failed startup because they applied a Messages-target output default to Google; corrected test-owned configuration reaches the stated admission boundary. |
| R07 | OpenCode DeepSeek public-label configuration | Current OpenCode 2.0.32 synthetic controls reject both `deepseek-v4.1-flash` and `deepseek-v4.1-flash-1017` with `provider.no-route` before HTTP, with or without explicit compatible-provider packages. Built-in `deepseek-flash` passes both synthetic controls and a fresh real 3-request tool/history loop. Retain this as a custom client-declaration limitation; the model spelling alone does not resolve it. |
| R08 | OpenCode error-path resubmission | Some failed cross-protocol profiles autonomously submit another request despite disabled SDK retries. Test admission latches failure and blocks excess submissions before forwarding; none exceeds its three-attempt ceiling. This is a client error-handling observation, not an attributed gateway defect. |
| R09 | Real replay isolation and failover prerequisites | Separate JSON and SSE Luna→Grok probes each stop after one successful Luna source request because no opaque replay is returned. No transfer request is sent. No independent compatible account pair is available for same-model replay/failover; same-key aliases and different-model switching do not qualify that gate. These are unexercised prerequisites, not observed isolation failures. |
| R12 | Repeated-summary recall assertion varies between samples | 2026-11-07, pi 1.5.0, Haiku Messages: the first two-compact story stops after 7 complete requests on a combined exact-label/no-tool recall assertion. That observer did not distinguish extra prose, tool use and absent facts. A separate instrumented 9-request story passes both compactions, exact no-tool recalls and rebuilt read. The passing sample retains an explicitly instructed label; the first failure remains unclassified, with no established content-loss boundary. |
| R13 | Dynamic-tool/instruction recall assertion varies between samples | 2026-11-07, pi 1.5.0, Haiku Messages: a high→low effort, read→ls→no-tools story completes 5 requests but fails its combined label/filename/no-tool recall assertion. A separate instrumented 5-request story passes; both facts remain in ingress history and the final answer, with no recall tool execution and exact declarations `[read]`, `[read]`, `[ls]`, `[ls]`, `[]`. The first assertion's failing dimension remains unclassified. |
| R17 | pi GLM Chat strict-profile admission | 2026-11-07, pi 1.5.0, `dashscope-glm-chat`: the original read/history profile rejects locally with `400 invalid_request`, 0 upstream attempts and planning reason `admission.strict_tools`. Pure preflight isolates explicit strict tools: false/omitted controls encode, while true rejects at capability admission. These non-strict controls are diagnostic variants, not strict opt-outs for acceptance. |
| R18 | Bailian explicit tool selection and strict arguments | Original 11 HTTP-200 probes: combined strict/named profiles fail the call/terminal oracle on Qwen/GLM in both APIs and pass on DeepSeek, including its enum oracle; non-strict Responses allowed-tools/required fails Qwen/GLM and passes DeepSeek; nonthinking non-strict Chat specific selection fails Qwen and passes DeepSeek. Targeted two-tool probes separate identity from terminal: nonthinking Qwen selects forced `lookup` over prompted `decoy` in both APIs, but Chat reports `stop` with that call; Responses completes. GLM Chat selects `decoy` instead of forced `lookup`, and its Responses sample emits no call. DeepSeek selects `lookup` with the expected terminal in both. Matched Qwen `auto` probes vary only strictness: Chat changes out-of-enum 93 to an allowed value under true; Responses returns 93 under both false and true. GLM's true samples emit no call, leaving strictness unexercised; its false Responses sample has a transport failure followed by an explicit one-request retry that returns a call with 93. Samples retain their activation settings and do not establish general constrained decoding. |
| R19 | Bailian structured-output result and SDK boundary | Original conflict probes: Qwen Chat satisfies the closed numeric-enum oracle; GLM Chat fails it; DeepSeek Chat/Responses reject HTTP 400. Native Responses independent rechecks now contain seven fixed one-send invocations: one Qwen transport failure and six HTTP-200 responses. Both Qwen outputs satisfy the closed numeric-enum/Boolean oracle; GLM passes twice and fails twice. All six repeat OpenAI Python 3.37.0 strict-response failure with object-valued `schema_` and absent canonical `schema`. The current Bailian codec returns `codec.invalid.invalid_value` on each original response and on diagnostic copies with the schema key corrected or reported `text` removed. The schema alias is not the only remaining decode boundary. Four gateway schema controls reject locally with no attempts; they do not re-exercise native output. |
| R21 | Grok recovery-label assertions and newly issued recovery tools | 2026-11-07, pi 1.5.0, Grok Responses, 2048-token/120-second bounds and `concurrency:1`: two thinking-cancel stories and three tool-argument-cancel stories each record `interrupted → complete` but fail the recovery-content assertion. Instrumented requests retain the recovery user instruction at ingress and upstream preparation; delivered text reaches pi unchanged. In the short tool sample, no file exists after cancellation; the recovery response issues a new write, pi executes it, and a third submission is blocked before forwarding. Write remains declared; no explicit `tool_choice:none` is sent. Five fresh-context API controls without cancellation also fail their exact-label oracles, including bare-label prompts. Cancellation-specific causality and the upstream/codec/model boundary remain unselected; this is not attributed to residual execution of the cancelled tool. One separate thinking observer fails before recovery on a partial JSONL line; its corrected retry is counted independently. |
| R22 | GLM Responses queued opening and SDK accumulation | New `dashscope-glm-responses`, low effort: three independent samples each pass a JSON smoke, then the tool stream triggers OpenAI Python 3.37.0's initial `response.created` requirement. Source capture begins `response.created → response.in_progress`; gateway delivery begins `response.queued → response.created → response.in_progress`. The existing Bailian queued-creation normalization and public queued encoding expose this consumer boundary; SDK closure interrupts the stream before tool/history acceptance. |
| R23 | Bailian DeepSeek Responses source item snapshot | New `dashscope-deepseek-responses`, exact upstream `deepseek-v4.1-flash-1017`, low effort: two independent one-request JSON smoke samples reach upstream HTTP 200, then fail `event.decode` with `codec.invalid.item_snapshot` before downstream publication. The rejected snapshot field is not isolated. Native companion results and the working Bailian Chat loop retain their separate scopes. |

## Passing tests

These results are retained independently of the pending observations. They qualify only the listed profiles, not AR5 closure or untested protocol combinations.

| Test group | Passing result | Scope |
|---|---|---|
| Fresh accounts and basic delivery | Five account references validate; Haiku, Grok OAuth, DeepSeek beta and Luna each complete a one-request smoke | Google basic delivery is included in the managed SDK checks below |
| Native Agent tool/history loops | pi and OpenCode: Haiku Messages, DeepSeek beta readable Chat, Grok OAuth Responses; OpenCode: Luna Responses | Each loop completes 3 requests / 3 attempts, real read/result/answer and same-session recall. [Detailed profiles](development.md#production-profile-acceptance) preserve their native controls and model-label qualifications |
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
| pi repeated and automatic compaction | Repeated manual story: 9/9; automatic story: 4/4 | Two ordinary manual compactions, split-turn work, exact label recalls and a rebuilt actual read pass in the qualified sample; R12 retains the first assertion failure. The automatic story observes one threshold compaction and subsequent recall using a test-owned 5000-token trigger, not full-context capacity. |
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
| Remaining original-profile admission samples | 5 requests / 0 attempts, all local rejection | Both Google bindings under Responses and Messages, plus GLM Chat, expose the planning reasons in R05/R06/R17. Two preceding Google Messages runners fail test-owned startup before sending; correcting only that configuration yields the listed two Messages observations. |
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
| GLM Responses | Three independent samples, **6 requests / 6 attempts**: each JSON smoke completes, then the SDK closes its tool stream at R22. Full tool/history acceptance remains open. |
| Bailian DeepSeek Responses | Two independent samples, **2 requests / 2 attempts**: both stop at R23 after upstream HTTP 200. |
| OpenCode model labels | Synthetic controls reject both user-suggested custom spellings before HTTP and pass canonical `deepseek-flash`, with and without explicit compatible packages. The corrected native-cap real canonical profile completes **3/3** read/result/answer/recall. An earlier test-owned cap override causes one local admission rejection with no upstream attempt. |
| Luna cross-protocol continuation | Two samples each: Chat **2/2**, Messages **3/3**; all four repeat R03 at the original feedback/recall boundary, with **6 complete deliveries / 10 attempts** in total. |
| Haiku argument cancellation | Two independent **2/2** single-permit stories record `interrupted → complete`; one passes the full recovery oracle. The other reaches observer parsing/excess-feedback boundaries after gateway completion. |
| Native structured output | Initial four fixed one-send invocations: one Qwen transport failure and three HTTP-200 responses. A subsequent user-capped batch sends **exactly 3 requests total** (Qwen once, GLM twice), all HTTP 200: Qwen satisfies the schema; GLM fails once and passes once. All three reproduce the same SDK envelope and original/canonical-schema/text-removed codec failures. Same 2048-token, 120-second, 256-KiB/2-MiB bounds, serial sends and no retries or replacement calls. The earlier transport failure remains separate; remote receipt was unconfirmed. |

Four preceding gateway schema controls stop at local admission, and test-owned constructor/account-file failures stop before dispatch. These are excluded from native schema evidence. Private response/capture files are deleted per case; body-free outcomes remain separate from scenario verdicts.

The full offline baseline passes: **436 library, 415 semantic, 94 transport and 30 gateway tests**, all-target Clippy under `-D warnings`, formatting, production-only checks, **20 pinned assets** and the **8 upstream Python tests**. All **19 connected SDK scenarios** pass against synthetic authorities.

## Updating this record

Append evidence to the relevant observation or add an identifier for a distinct phenomenon. Record the date, actual client version, selected model/binding/protocol, bounds, counts and failure boundary; retain only redacted summaries, not credentials or private payloads. Remove repaired and accepted entries and their obsolete failure/verification narratives; retain unresolved portions of mixed entries. Keep unmet prerequisites explicit.

Do not infer a product defect from an observer/configuration failure, silently relax a guard to obtain a pass, or promote a passing sample into broader acceptance. Add focused evidence as repairs proceed and retain explicit unknowns. Accepted design decisions belong in [Architecture](architecture.md), while repair ordering, deferrals and the partial-delivery exit belong in [Roadmap](roadmap.md#concentrated-repair-plan).
