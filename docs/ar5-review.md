# AR5 Review

This record owns observations and concise verification evidence. [Roadmap](roadmap.md#remaining-observation-dispositions) owns dispositions and re-entry; [Architecture](architecture.md#conversion-ladder) owns accepted semantics. Evidence is organized by contract, not by successive debugging runs. Git retains superseded repair narratives.

## Contents

- [Test context](#test-context)
- [Pending observations](#pending-observations)
  - [Tool invocation and argument guarantees](#tool-invocation-and-argument-guarantees)
  - [History and replay carriers](#history-and-replay-carriers)
  - [Instruction following and content recall](#instruction-following-and-content-recall)
- [Passing tests](#passing-tests)
  - [Current-client qualification, 2026-11-09](#current-client-qualification-2026-11-09)
  - [Tool policy and Grok request qualification, 2026-11-08](#tool-policy-and-grok-request-qualification-2026-11-08)
  - [Selected root repairs and qualification, 2026-11-09](#selected-root-repairs-and-qualification-2026-11-09)
  - [Bailian function-guarantee follow-up, 2026-11-09](#bailian-function-guarantee-follow-up-2026-11-09)
  - [Shared Bailian strict relaxation, 2026-11-09](#shared-bailian-strict-relaxation-2026-11-09)
  - [Readable Chat replay qualification, 2026-11-08](#readable-chat-replay-qualification-2026-11-08)
  - [Agent session and execution workflows](#agent-session-and-execution-workflows)
  - [R21 write-feedback qualification, 2026-11-08](#r21-write-feedback-qualification-2026-11-08)
- [Cache-affinity qualification evidence](#cache-affinity-qualification-evidence)
- [Updating this record](#updating-this-record)

## Test context

**Current-client recheck: 2026-11-09, pi 1.5.0 and OpenCode 2.2.5.** Adaptation and acceptance target only the locally installed current Agents. Older version labels below identify dated evidence, not supported compatibility branches. Protocol/SDK/source pins remain independent baselines; OpenAI Python is **3.37.0**.

Real checks use the production account loader and trusted targets, isolated workspaces, serial sends, no hidden retries, **2048 output tokens**, **120-second route/exchange deadlines**, **256-KiB requests / 2-MiB responses** and concurrency **1**. Each scenario fixes its request/attempt budget before dispatch; a three-request story has a 420-second outer bound. Thinking-enabled non-Qwen Bailian Responses uses the selected reply-only ceiling, with independent byte/time bounds. Private captures are temporary; logs contain counts, known controls and equality verdicts only.

Historical Agent evidence dates from **2026-11-06–09**, with pi 1.5.0 / OpenCode 2.0.32 unless stated otherwise. Cross-protocol profiles explicitly select an existing serializer and do not qualify native Google client APIs. DeepSeek native/beta uses canonical `deepseek-flash`; the dated public label and Bailian upstream name remain profile-specific.

Local rejection, client configuration, observer failure, upstream rejection, complete delivery and scenario success are separate outcomes. Successful repeats do not erase failures. SDK, installed-client synthetic and real-Provider acceptance remain separate. Procedures live in [Development](development.md#real-agent-scenario-selection).

## Pending observations

**Five IDs, four boundaries:** R05 tool guarantees; R02/R06 ordinary signatures; R01 real protected output; R13 content recall. Their dispositions are recorded once in the [roadmap](roadmap.md#remaining-observation-dispositions). R18/R21 are closed; R04/R08 and Bailian structured-output variability were removed by user selection. R09 belongs to separate cache-affinity qualification.

### Tool invocation and argument guarantees

| ID | Observation | Evidence boundary |
|---|---|---|
| R05 | Full Google Agent strict profiles remain unqualified | Both APIs support the selected closed-object subset, now including required nullable `anyOf:[supported schema,{type:null}]`. Native root `$ref` and inline controls also pass, but strict references/general composition remain outside local admission. Length enforcement and seriality have native counterexamples. Conservative strict/serial checks also run under no-call selection; that separate admission edge has not been repaired or qualified. Supported SDK schemas do not qualify every current Agent declaration/control combination. |

### History and replay carriers

| ID | Observation | Evidence boundary |
|---|---|---|
| R02 / R06 | Stateless generateContent ordinary text/call signatures lack a Client carrier | Earlier OpenCode Chat/Responses, SDK stateless and pi Messages samples reach upstream HTTP 200, then reject with `codec.unsupported.replay`. Managed Responses continuation passes separately. Current OpenCode 2.2.5 Responses still uses `store:false` and full history without `previous_response_id`; upgrading the CLI does not integrate managed continuation. |
| R01 | Real Haiku protected Readable-Chat output remains unobserved | OpenCode 2.2.5 completes the original read/feedback/recall story **3 requests / 3 complete attempts**, with zero protected owners. Prepared Messages has effort low and no `thinking`. A separate **1/1** explicit adaptive/medium Messages discriminator also completes without thinking/signature/redacted blocks. Missing activation is therefore not sufficient to explain sample availability; neither sample exercises real protected replay. Synthetic multi-owner carrier acceptance remains passing. |

Ordinary signatures remain on their text/call owners. They are not protected reasoning, and no signature deletion or synthetic reasoning owner is selected.

### Instruction following and content recall

| ID | Observation | Evidence boundary |
|---|---|---|
| R13 | pi Messages read → ls → none story can omit the literal label | The 2026-11-08 collection has **7 stories / 35 attempts**: two passes, four recall failures and one final-request HTTP 400. Three failing recalls have complete native output already missing the label and exact native/delivered/pi agreement; another has incomplete native capture. The **2026-11-09** unchanged **5/5** story again omits the label, retains the filename and issues no recall tool. Its full ordered ingress/prepared comparison closes the request-fidelity gap below. |

The new R13 audit verifies cumulative role sequences, unchanged historical prefixes, assistant replay, call/result associations, current root instructions and declarations `[read],[read],[ls],[ls],[]`. Root effort stays low; ordered message-local effort vectors are `[high]`, `[high,high]`, then append low at each request. Complete prepared requests equal ingress for system/messages/tools/effort/cap; only the selected `thinking.block_binding` omission differs. Native comparison joins by exchange/attempt, folds text starts and deltas, and requires complete unredacted capture through `source_eof`. The native final answer already lacks `cedar-71`, retains `listed-marker.txt`, and equals delivered text. No project-owned request/delivery divergence was found; the exact-label assertion remains failed, not waived.

## Passing tests

Results qualify the stated contracts and profiles, not every protocol pair or model behavior.

### Current-client qualification, 2026-11-09

The first OpenCode 2.2.5 run passes five existing scenarios and stops two default loops at an obsolete test-only whole-inventory assertion. Direct isolated characterization shows `read` plus the new `skill(id:string)` declaration. The revised oracle requires the actual read contract and compares each request's complete observed tool inventory/schema with preparation, without snapshotting auxiliary names. All three native protocol loops pass. No forwarded payload or returned history is rewritten.

Current OpenCode still omits empty Responses annotations in replay; the selected Client-history shorthand remains necessary. The acceptance oracle also accepts the official empty array, rather than requiring omission. Signed/opaque Readable Chat history, credential recovery, summaries and incremental cancellation/recovery pass. Pi's current adjacent-detail accumulation still requires independent protected-owner envelopes. The obsolete Google `call`/`tail` surrogate-signature compatibility path and an unused test-only Messages comparator are removed; current payload validation and replay guards remain.

The full offline baseline passes: Rust tests, all-target Clippy with `-D warnings`, formatting, production compilation, 20 pinned assets and Python suites (8 upstream / 16 SDK-oracle / 45 client-oracle / 17 OpenAPI). All **25 installed-client scenarios** pass on pi 1.5.0 / OpenCode 2.2.5, including recovery, honest cancellation, signed/opaque history and default native tool loops. Literal-default mutations, fixture-assembly echoes and redundant helper tests are pruned; actual driver bounds and terminal checks remain. The preceding **20 SDK loopbacks** and real-Provider evidence are reused because those paths are unchanged. Body-free evidence is under `testdata/reports/ar5-convergence-*-20261109.job` and `current-agent-{inventory-clients,pruning-offline}-20261109.job`.

### Tool policy and Grok request qualification, 2026-11-08

| Profile | Passing evidence | Boundary retained |
|---|---|---|
| Qwen/GLM Responses, non-strict | Each **3/3** SDK SSE call/feedback plus JSON recall; serial control and auto/none preserved | Serial permission is independent of required selection |
| Qwen Chat, thinking off, non-strict | **3/3** forced lookup/feedback/recall; native `stop` preserved | Invocation identity and terminal spelling checked separately |
| Qwen/GLM native Responses forcing | **6 single requests**: serial auto succeeds; required and required allowed-subset receive HTTP 400 with parallel unspecified/false | Default/enabled-thinking forcing remains unsupported |
| Grok request shape | **4 native HTTP-200 controls**: function declarations on/off × omitted/explicit-none policy | Successful delivery does not repair prompt-only content behavior |

Body-free evidence: `ar5-tool-{serial-native,required-native,serial-gateway}-20261108.log` and `ar5-grok-shape-native-20261108.log` under `testdata/reports/`.

### Selected root repairs and qualification, 2026-11-09

Google source evidence uses local Python **2.33.0** and JavaScript **2.21.0**, independently of regression-source pins; see the [dated source distinctions](refs/2026-10-27-official-sdk-sources.md#google-function-argument-carriers-2026-11-09).

| Contract | Passing evidence | Limit |
|---|---|---|
| Google strict managed continuation | Each API: JSON **3/3**, SSE **3/3**, plus automatic-validated nested-schema JSON **3/3** | Original closed schema, identities and exact recall retained; upstream stateless |
| Required nullable arguments, new recheck | Each API: native **1/1** with an actual null, then repaired CLI + pinned SDK managed loop **3/3** | Original schema survives preparation; model call, original-call feedback and exact recall pass. No serial guarantee or full Agent profile is inferred |
| Root reference versus inline, native discriminator | Each API: **1/1** per shape, conflicting enum prompt yields valid arguments | No normalization; subsequent bounded admission and managed continuation are checked below |
| Bounded strict reference admission | Independent root/nested/nullable-reference and inline wire expectations on both APIs; conditional mixed-inventory, cycles, sibling/scope and cached-depth controls | Complete schemas and source IR remain unchanged; no reference expansion, remote resolution or broader composition |
| Native no-call and managed reference continuation | Each API: native none **1/1**, then strict root+nested-reference call → feedback with none/serial intent → none continuation **3/3** using the pinned OpenAI SDK | Actual argument enums, zero calls under none and exact marker recall pass. Declaration schemas retain otherwise-unqualified length constraints under none; enabled-call rejection remains |
| Non-strict reference parameters | Independent directional regression preserves the complete original document | No literal root-type injection or reference rewriting |
| Capture continuation | Security regression preserves later complete sanitized units after rejected fragments | Framing failure stays irreversible; incomplete source remains incomplete |
| Qwen Responses forcing, thinking off | JSON **3/3**, SSE **3/3** | Named subset, seriality, exact schema and feedback preserved; explicitly non-strict |

Native counterexamples remain: Google length constraints can be ignored; one declaration can produce two calls; Boolean enums can be violated; unsafe integer enums reject. Nullable, no-call and strict-reference regressions fail before their repairs, then pass on both directional writers. Unsupported referenced constraints, reference cycles/scopes and general compositions still reject when calls are enabled.

The new Google native collection makes **9 attempts**, all HTTP 200. Its first three Interactions samples were misread by a disposable oracle using `outputs` instead of the current `steps` field; they establish HTTP delivery only. Three explicit corrected repeats provide the Interactions argument verdicts above. The separate repaired managed loops make **6 complete attempts**. Evidence is saved as `ar5-convergence-google-*-20261109.job`; earlier selected-root evidence remains `ar5-selected-*-20261109.job` under `testdata/reports/`.

**No-call/reference repair recheck:** full offline baseline, **20 SDK loopbacks** and **25 current-Agent scenarios** pass. The Google SDK loops retain eight transmissions per API, using root-reference JSON, inline SSE and explicit-none final continuation. Real qualification adds **2 successful native-none controls** and **6 successful managed requests**, serially bounded to one attempt, 2048 output tokens, 120 seconds, 256 KiB request and 2 MiB response. The disposable managed fixture first omitted model metadata (zero dispatch), then made one direct-connect attempt returning 504 because embedded test construction disables implicit proxies; explicit trusted proxy selection fixes that fixture, without a product change or automatic retry. The initial SDK invocation also used bare Python without pinned modules; the explicit locked interpreter passes. Body-free logs, including failed invocations, are saved as `google-no-call-references-*-20261109.job`.

### Bailian function-guarantee follow-up, 2026-11-09

Native counterexamples motivating the selected policy remain recorded:

| Profile | Result |
|---|---|
| GLM 5.3 Chat auto, strict false / true | False returns no call; true returns one correctly named closed/typed call violating both enums |
| Qwen Responses thinking off, auto, strict false / true | False returns a conforming call; true returns no call, permitted by auto |
| Same Qwen profile, required subset, both strict values | Each returns the correct identity but violates both enums |

All **6 one-send controls** return HTTP 200. A subsequent local singleton forced-inventory repair preserves original schemas/history/seriality; production CLI + SDK JSON **3/3** and SSE **3/3** preserve exact arguments and continuation. Default/enabled-thinking forcing remains unsupported. These native failures do not qualify a strict-enforcement exception. Current policy belongs to [Architecture](architecture.md#shared-aliyun-billing-dialect).

### Shared Bailian strict relaxation, 2026-11-09

| Consumer/profile | Passing evidence |
|---|---|
| pi 1.5.0, `dashscope-glm-chat` | **3/3** read/feedback/recall; client strict true becomes best-effort native omission, with the complete schema and call/result links unchanged |
| OpenAI Python 3.37.0, `dashscope-flash-responses` | JSON **3/3**, SSE **3/3**; strict intent takes the named relaxation while thinking-off forcing/schema/history survive |

A preceding pi observer requires nonexistent readable reasoning and stops after one complete attempt; the corrected target-appropriate observer compares any reasoning that exists, then passes. Total **10 real attempts** remain separate outcomes. Offline checks cover both plans, root/inline declarations, conditional/explicit strictness and discovery's `approved_adaptation`. No verified strict exception or separate TokenPlan live qualification is inferred. Evidence: `bailian-strict-relaxation-*-20261109.job`.

### Readable Chat replay qualification, 2026-11-08

| Boundary | Evidence |
|---|---|
| Synthetic current-Agent native Messages → Readable Chat | pi and OpenCode each **3/3**, with independent signed/redacted owners, exact envelopes, actual read/result and recall; rechecked on current clients above |
| Pinned SDK JSON/SSE | **6 requests**, two protected owners retained through feedback and recall |
| Real OpenCode 2.0.32 → Grok Responses | **3/3**, one protected owner per response; returned envelopes restore original native values |
| Real OpenCode 2.0.32 → Google Interactions | **3/3**, thought anchors/signatures restored; no signed-call qualification |
| Real current OpenCode → Haiku | Original Chat **3/3**, no protected owners; explicit adaptive discriminator **1/1** also returns none, as retained in R01 |

Late signature metadata does not repeat visible text or require whole-response buffering. Foreign-origin, malformed-envelope, dependency, incomplete-owner and replay-isolation checks remain authoritative. Dated carrier research is indexed in [References](references.md#research-notes).

### Agent session and execution workflows

These are dated **2026-11-06–08** real samples, not a historical-version support matrix. Current affected-client acceptance is separate above.

| Domain | Passing evidence | Scope |
|---|---|---|
| Native Agent tool/history | pi/OpenCode Haiku Messages, DeepSeek beta Chat and Grok Responses; OpenCode Luna Responses: **3/3 each** | Actual read/result/final answer and same-session recall |
| Cross-protocol Agent loops | OpenCode Responses→Haiku/Interactions and pi Messages→Grok; final pi Luna Chat/Messages/Chat stories **3/3 each** | Explicit serializers, untouched history and call links |
| Google managed SDK | generateContent smoke **1/1**, JSON/SSE **3/3 each**; Interactions smoke + loops **7/7** | Ordinary signatures and thought replay retained; no stateless/signed-call extrapolation |
| Bailian native SDK | Qwen/GLM/DeepSeek × Chat/Responses: JSON/SSE smoke **1/1** and non-strict tool loops **3/3** per cell | SDK argument/history behavior, not Agent execution or arbitrary strictness |
| Bailian gateway paths | DeepSeek Chat **4/4**; DeepSeek Responses three **4/4** stories; GLM Responses two **4/4** stories and one separate timeout | Current registered bindings; timeout has no replacement send or assigned protocol cause |
| Coding/repair | pi Haiku **6/6**, DeepSeek **8/8**, Grok **7/7**, Qwen Responses **7/7**; OpenCode Haiku **6/6** | Actual failure/read/edit/verification, unchanged verifier, independent final code checks and Unicode recall |
| Dynamic controls | pi Haiku/DeepSeek/Luna/Grok thinking selections; Luna/Grok typed summary API controls | UI/control carriage and continuation, not reasoning quality or summary granularity |
| History/session isolation | Haiku/DeepSeek restart/fork/edit **8/8 each**; Haiku interleaved sessions **6/6**, multiple reads **3/3**, model switch **4/4** | Original/fork identities, independent labels and actual call/result links |
| Summaries | pi ordinary **6/6**, OpenCode **5/5**; five consecutive pi repeated-compaction stories **9/9 each**; automatic **4/4** | Ordinary Generation summaries, exact recall and rebuilt read; not a server compaction API |
| Cancellation/recovery | First text **3/3 each Agent**; pi thinking/local bash, tool arguments and compaction single-permit stories | Honest interruption, released resource, no partial execution/file and subsequent recovery; no remote billing/compute claim |
| Inputs/accounting | Haiku image **3/3**, 420-line tool result **3/3**, usage/cache **2/2**, native output limit **1/1** | Actual PNG read/recall, Unicode facts, native/client counters and observed length terminal |
| Go native DeepSeek companion | Authenticated models GET and OpenCode native loop **3/3** | Exact Go identity/root/session headers; bypasses gateway codecs and activates no binding |

Earlier failed OpenCode coding profiles allowed `bash` rather than current `shell`; corrected permission evidence is distinct. Earlier repeated-compaction failures remain superseded by scoped passing repeats, not a claim of deterministic model recall. Structured-output variability remains removed from follow-up; passing schema samples do not erase native failures.

### R21 write-feedback qualification, 2026-11-08

Two real pi/Grok stories each make **3 requests / 3 attempts**, `interrupted → complete → complete`. They establish cancellation before execution, no file before recovery, distinct cancelled/new IDs, one exact new write and its feedback, and no cancelled identity in returned history. A durable synthetic write-cancellation gate independently verifies these mechanics.

Both real stories fail the exact recovery marker and prompt-only no-tool expectations. The second has a complete final native comparison: upstream already lacks the marker and equals delivery/pi; prepared user text and feedback match ingress. Partial native tool-call captures do not qualify complete tool snapshots. Separate native no-tool/explicit-none counterexamples also omit the marker. These support the scoped [Grok limitation](architecture.md#subscription-responses), not cancellation-specific causality or a strictness repair. R21's user-selected closure remains in force. Evidence: `ar5-r21-{write-baseline,live-slot1,renewal,live-slot2,live-slot3}-20261108.log`.

## Cache-affinity qualification evidence

R09 belongs to the [post-AR5, pre-Stage-4 gate](roadmap.md#cache-affinity-qualification). Independent compatible same-model accounts have not been qualified; previous loader evidence reports one enabled declared account per Provider. No account provisioning or new account inventory was performed in this review.

Real stateless Responses SDK JSON and SSE each pass **4/4**: Grok protected call → same-target feedback → Luna visible-history transfer → Grok again. Original native values restore on the same target; foreign reasoning takes the selected omission while visible text and call/result links survive. This establishes replay isolation for those profiles, not independent-account failover. Body-free evidence: `ar5-r09-replay-{json,sse,offline}-20261108.log`.

## Updating this record

Update the owning domain rather than appending another chronological run report. Retain actual date/version/profile, bounds, scenario verdict, first divergent boundary and unknown prerequisites. Keep contradictory native outcomes and incomplete-capture limits concise. Private payloads never become fixtures. Accepted design belongs in Architecture; work and deferrals belong in Roadmap. Structural cleanup grants no deployment, API activation, signature loss or weaker guarantee.
