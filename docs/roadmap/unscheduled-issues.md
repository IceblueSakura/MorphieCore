# Unscheduled Issue and Limitation Register

This page is the single register for unresolved Generation/client boundaries, qualification gaps, and conditional tool follow-ups. The [roadmap](index.md#planned-order) selects scheduling and future entry gates; recorded issues do not reopen the delivered baseline.

See [Roadmap Main](index.md) for current sprint priorities and execution gates; [Backlog and frozen scope](backlog-and-frozen.md) for paused work; and [Architecture](../architecture/index.md) for accepted system contracts.

## Contents

- [Unscheduled issues](#unscheduled-issues)
  - [Tool guarantees](#tool-guarantees)
  - [History and replay](#history-and-replay)
  - [Content and client qualification](#content-and-client-qualification)
  - [Protocol boundaries](#protocol-boundaries)
  - [Routing qualification](#routing-qualification)
- [Deferred protocol follow-ups](#deferred-protocol-follow-ups)
  - [Conditional Stage-3 follow-ups](#conditional-stage-3-follow-ups)
    - [Tool verification and scope gates](#tool-verification-and-scope-gates)
  - [Retained rejections](#retained-rejections)

## Unscheduled issues

This is the single register for unresolved Generation/client boundaries and qualification gaps.

Only issues explicitly selected in the [roadmap](index.md#planned-order) gate future work; all others remain unscheduled and none reopens the delivered baseline.

Reopen only after a concrete unmet contract, target/profile, prerequisite and bounded acceptance plan are selected.

Review IDs remain traceability labels, not a second phase queue.

[Development](../development.md#overall-acceptance-status) summarizes overall acceptance; [Architecture](../architecture/index.md) owns accepted contracts and service limitations.

Generation uses standard downstream APIs only. Uncarriable ordinary signatures follow the [field-level rejection contract](../architecture/history-and-replay.md#stateless-api-boundaries). R04/R08 and Bailian structured-output variability remain excluded. R09 real-account failover remains unscheduled.

### Tool guarantees

| Issue | Current boundary | Re-entry / decision |
|---|---|---|
| Google strict function guarantees (R05) | The [qualified subset](../architecture/adaptation-and-dialects.md#google-strict-function-arguments) includes nullable fields, bounded local references and no-call equivalence. Enabled-call length/pattern/general-composition and serial guarantees still reject; full current-Agent profiles remain unqualified. | A concrete current-Agent need and native carrier evidence first. Gateway argument validation or a call-publication barrier needs explicit selection of failure semantics and streaming latency before implementation. No argument repair, automatic retry, schema deletion or strictness disabling. Broader reference semantics need their own bounded design. |
| Effective output ceilings and recovery | Non-Qwen Bailian reply-ceiling enforcement and Haiku same-process cap-switch recovery remain unqualified; preparation receipts and GLM JSON smoke do not close them. | Select the exact client/model/control transition and a bounded real enforcement/recovery story; preserve independent byte/time bounds and honest length terminals. |

### History and replay

| Issue | Current boundary | Re-entry / decision |
|---|---|---|
| Google ordinary-signature boundary (R02/R06) | Standard stateless delivery rejects ordinary text/call signatures without a lawful carrier. The selected Gemini 3.8 Flash generateContent text stream decodes successfully but has ordinary-owned replay, so Responses rejection or published-stream abort is expected. Experimental managed Responses remains disabled; [fields and reasons](../architecture/history-and-replay.md#stateless-api-boundaries) own the limitation. | Reopen only with an official standard carrier and concrete client need; no private replay profile or client adaptation is scheduled |
| Real Haiku protected output (R01) | Current OpenCode's original Chat loop and an explicit adaptive/medium discriminator produce no protected owners; existing carrier repair remains verified. | Resume when the same profile actually yields protected output. Missing activation alone does not justify another repair or automatic effort-to-activation mapping. |
| Interactions signed-call authority | Existing scoped decoding recognizes a call signature, but pinned ordinary-call types and observed issuance do not establish a current official carrier. | Select current primary schema and an actual sample before expanding qualification; if observed, standard stateless delivery retains its explicit attachment rejection |

### Content and client qualification

| Issue | Current boundary | Re-entry / decision |
|---|---|---|
| Standard-only real Agent qualification | Current installed-client loops with synthetic upstreams cover standard history, summaries and recovery. Representative real workflows still need qualification after retiring the private Chat and Responses profiles. | Select exact existing account/model/client routes and bounded tool/history runs; reuse unaffected real evidence. No extension restoration or full cross-product sweep |
| Messages literal-label recall (R13) | Scoped upstream omission: the unchanged five-request audit preserves ordered instructions/history/tools/effort, and complete native text equals delivered/client text on the failed recall. | No further recall sweep or prompt/history rewrite. Re-enter only on concrete new project-owned divergence; preserve the exact-label failure and distinguish incomplete captures from verified attribution. |
| **OpenRouter Luna Responses SSE intake — pre-Stage-4 gate** | Target: `openrouter-responses`, `gpt-6-luna`. Upstream HTTP 200, `response.completed` and cache reuse are observed, but the Responses event decoder fails before IR/downstream accounting can be checked. The diagnostic probe collapsed `push`/`finish` errors to a boolean, so the first failing event and concrete `CodecError` remain unknown. | Before Stage 4 resumes: record the failing `push`/`finish` phase, event type/sequence when applicable, and redacted `CodecError`; identify the responsible contract, add an independent regression, and verify corrected native SSE → IR → standard downstream delivery, cached usage and honest termination. Use bounded serial service checks; do not weaken known-field or terminal validation. Broader router endpoint/full-loop qualification remains separate. |
| Small MiniCPM tool/answer emission | ModelBest MiniCPM-V-4.7-1B has returned tool-shaped ordinary text without structured calls; minicpm5-1b has returned a reasoning-only final turn. These remain upstream emission observations; the gateway preserves the returned type. | Re-enter only on exact request/output evidence of a project-owned divergence. Never execute tool-shaped text or promote reasoning into answers. |
| Additional registered-service failures | Nemotron minimal requests and Go MiMo-Pro streams have shown request failures or interruptions whose source boundary remains unqualified. | Use bounded serial target-specific diagnostics with original upstream status and terminal evidence. Preserve interruption and avoid speculative retries, timeout increases or argument repair. |
| Remaining service capacity evidence | Some subscription/native/router candidates still have unknown total/input/output capacities; mixed routes retain unknown common values and client planning fallbacks. [Sources and scope](../refs/2026-10-26-model-discovery.md#capacity-sources) identify the unresolved declarations. | Exact service-qualified integer evidence or an explicit trusted deployment declaration; do not expand ambiguous abbreviations or inherit another service's limits |
| Go Step Chat intake | `opencode-go-step-chat` returns gateway 502 for stateless text and tool requests. OpenRouter Chat for the same public model completes both, and the deployment prefers that working candidate. The Go upstream status and codec boundary remain unlocalized. | Inspect the exact Go target's head/intake failure; preserve the working router qualification without transferring it to Go or declaring the canonical model unsupported |
| OpenRouter Muse Ember intake | `openrouter-ember-chat` and `openrouter-ember-responses` return gateway 502 for stateless text and tool requests. No selected Ember path has complete service acceptance. | Localize the upstream status, intake or projection failure with the original native response; do not infer the Luna SSE cause or weaken reasoning/terminal checks |
| MiniMax M3.1 Flash intermittent tool stream | A Go tool request returned HTTP 200 but ended without a completed downstream response. A deliberate repeat completed with the expected function and arguments; text also completes. The first interruption remains unexplained. | Re-enter on a captured native failure and exact stream boundary. Keep partial output honest; do not erase the failed observation or add hidden retries |
| Google generateContent qualification | generateContent remains unqualified for Argon/Hydrogen/Carbon stateless delivery after text interruptions and pre-publication tool failures. The selected Gemini deployment uses qualified Interactions-only bindings. | Re-enter only if generateContent activation is selected. Localize native intake versus the accepted ordinary-signature carrier boundary; no private replay extension or signature deletion supplies missing expressiveness |
| TokenPlan account availability | GLM Responses and Qwen4 Max Preview stateless checks return rate-limit failures; Qwen4 also reports `insufficient_quota` inside SSE without generated content. Neither path has complete functional acceptance. | Operator restores usable account quota/entitlement, then repeats ordinary stateless checks. Do not classify the failure as a model/codec defect or change credentials without approval |
| DeepSeek strict-schema source discrepancy | The beta Chat/Responses endpoints accept a strict schema containing `minLength`, although the strict guide excludes it. Ordinary/non-strict requests and tool/history continuation are qualified for the unified beta default; accepting a schema does not establish enforcement. | Keep current strict-schema validation and explicit strictness. Broaden enforcement claims only with an independently discriminating argument/schema case and current authoritative semantics |

### Protocol boundaries

The [source and feasibility recheck](../refs/2026-10-27-client-ir-boundaries.md#follow-up-source-and-feasibility-review-2026-10-28) supports these retained dispositions. Current strict parsing, resource, replay and terminal guarantees remain in force.

| Issue | Current disposition | Reopen or complete when |
|---|---|---|
| Chat custom-tool SSE | Retain standard rejection; investigate upstream intake and downstream delivery separately | Official event contract resolves the disagreement, or a separately selected dialect/client contract supplies independent wire/consumer evidence |
| SIWC Responses tool continuation | Flat root declarations use equivalent inline carriage and stable cache sessions are qualified. Real endpoint tool/reasoning continuation remains a separate current-Agent qualification. | Select the bounded current-Agent loop with unchanged returned history; no alternate billing route or credential change is implied |
| Chat upstream final usage | Duplicate nonnull final reports remain unqualified at the emitter | A concrete upstream profile and independently observed report sequence |
| Nonempty Messages tool opener plus nonempty fragments | Retain rejection, including a complete-fragment negative regression; complete opener plus zero-byte deltas remains supported | Authoritative replacement/merge semantics exist |
| Bounded token-budget adjustment | Model-maximum clamp selected; broader formulas deferred; native effort and existing validation preserved | An authorized target plus failing request, limit provenance, useful reserve, finite maximum and infeasible behavior qualify a target-local rule |
| `user:null`, Chat refusal requiredness | Preserve current intake | Service contract clarifies the discrepancy or a selected strict consumer exposes a concrete failure |
| Legacy function carriers | Retained rejection | A real client needs a constrained association/default/conflict/history contract |
| Broader settings echo and richer output grouping | Case-specific ladder application | A concrete failure has an exact mapping or a bounded approximation; preserve report authority and content-coordinate ownership |
| Messages custom-input wrapping | Conditional Stage-3 follow-up S3-T6; not a core exit prerequisite | Declaration, arguments, return/history mapping and client execution expectations are selected together |
| Token scope and SDK drift | Investigate only the selected target/field; no automatic source-pin promotion | A bounded budget rule or reviewed version change has independent expectations and a reachable target path |

### Routing qualification

The routing freeze and credential-lifecycle authorization remain unchanged. Additional cache-affinity test items await user supplementation; do not invent or launch that broader suite.

| Issue | Current boundary | Re-entry / decision |
|---|---|---|
| OAuth primary-path qualification | Synthetic authorities cover verification, rotation, uncertain-state quarantine, queued publication and graceful shutdown. OpenAI/SIWC and Grok login/verify → bounded request → refresh → request, including use of replacement material, remain unqualified against real authorities. | Authorize lifecycle operations for named accounts, define request/output/time bounds, then verify through the project loader and redacted receipts. Include interrupted-renewal restart state; never inspect or reuse uncertain token material |
| **R09 — Real account-compatible failover** | [Real stateless JSON/SSE replay isolation passes](../development.md#overall-acceptance-status). Loader evidence reports one enabled declared account per Provider; no independent compatible pair is qualified. Unscheduled; does not gate Stage 4. | With independent compatible accounts on one authorized same-model route, verify A success/affinity, a bounded A failure without switching inside that request, B selection on a new request with A-private replay omitted, then B success/stickiness. Define fault injection and budgets first; provisioning new credentials requires separate approval. Same-key aliases, different-model switching and synthetic authorities do not qualify this real-account gate |
| Optional affinity restart continuity | Unselected; current bounded success-updated affinity is in-memory. It does not make Responses history durable | Select opt-in, bounded private hashed keys/target fingerprints, corruption handling, stale-identity rejection, privacy and shutdown behavior before implementation |

## Deferred protocol follow-ups

Conditional tool packages and retained rejection boundaries are collected here; all unscheduled qualification issues live in the [shared register](#unscheduled-issues). Apply the accepted [conversion ladder](../architecture/adaptation-and-dialects.md#conversion-ladder), [field handling](../architecture/adaptation-and-dialects.md#field-handling), [projection principles](../architecture/adaptation-and-dialects.md#clientir-projection-principles) and [stream normalization](../architecture/runtime-and-routing.md#semantic-stream-normalization).

### Conditional Stage-3 follow-ups

These existing-tool compatibility improvements belong to Stage 3, not modality expansion or hosted execution. They remain **conditionally scheduled after the core**, requiring separate policy selection. They do not block Stage 4 or optional Agent qualification by default; if a selected acceptance lane requires one, make that package an explicit prerequisite for that lane. Deferred packages remain here rather than being silently activated or treated as complete.

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
- Reuse the [offline baseline](../development.md#offline-baseline) and [SDK gate](../development.md#sdk-loopback-gate) per implementation slice. Real Provider/model qualification is separately selected and bounded; synthetic acceptance does not certify endpoint behavior.

Risks that change the plan:

- Unknown root/inline precedence prevents computing a faithful source active set for colliding definitions; scope that approximation to known cases, while preserving native shapes when possible.
- Missing custom-SSE carriers or lawful returned-role replay prevent complete advanced-family loops. Keep those boundaries explicit; ordinary function work need not wait for a speculative transport.
- Explicit strict, selected-tool, cardinality or serial guarantees with no target carrier remain rejected unless that concrete guarantee downgrade is selected. Do not repair by schema deletion, automatic prompting or a new silent fallback.
- S3-T5–S3-T7 require their own policy selection and affected-loop acceptance; textual tool-history annotations, media relocation and grammar-to-prose remain outside the scheduled packages.

Tool discovery, hosted/programmatic execution, new endpoints and Google expansion remain outside these tool packages. Client-history annotations use their separate [L2 contract](../architecture/adaptation-and-dialects.md#semantic-equivalence-compatibility). Public capability metadata must describe conditional mappings honestly; general protocol or SDK presence is not a per-model promise.

### Retained rejections

These L5 boundaries and invariant guards constrain future work. Endpoint-specific entry conditions live in the [issue register](#protocol-boundaries).

| Boundary | Retained |
|---|---|
| Chat → IR | `n > 1`, `web_search_options`, nonempty `logit_bias`, `store:true`, `moderation`, legacy `functions`/`function_call`/`role:function` |
| Responses → IR | Hosted tools/items, unselected includes, background execution, active conversation/prompt/moderation/context-management operations and item references remain rejected. `store:true`, nonnull `previous_response_id` and stored-resource operations are [experimental and disabled](../architecture/history-and-replay.md#stateful-history) |
| Messages → IR | `between_tools`, message-local null/reset and unselected format/task-budget controls, `clear_at`, root task budgets pending selection, `container`, `mcp_servers`, context management and compaction, server tools and non-direct callers, block-binding `"error"`, `inference_geo`, enabled citations, null root output effort/format pending field evidence |
| Request → target | Media, tools or explicit content/schema constraints without a carrier; explicit strict guarantees without a carrier or selected relaxation; `stop` without a carrier; compaction without a carrier; explicit history-read intent that cannot be carried. Output ceilings and [Bailian function strictness](../architecture/adaptation-and-dialects.md#shared-aliyun-billing-dialect) follow their selected loss dispositions |
| IR → client | Chat custom-call SSE; Responses SSE cancellation; unknown Chat/Messages stop causes without an evidence-backed map; non-object Messages tool input other than an empty string; call ID or name lengths beyond the client limit; blocking safety reports; retraction of published accounting |
| Invariants | Credential/origin/trust selection, replay origin and format trust, client file IDs, resource bounds, malformed known values, terminal honesty, incomplete tool input never executable |

The [pinned Anthropic SDK](../../tests/upstream/sources.lock.json) types already include `between_tools`, optional-nullable output configuration and zero-token cache prewarming; their rejections are capability-selection gaps, not invalid syntax. Nonempty Messages tool openers followed by nonempty fragments concern native upstream SSE intake, not Client request intake.

