# Architecture

MorphieCore translates client and Provider protocols through one typed semantic IR. It is a gateway, not an agent loop: planning, agent memory and tool execution belong to the caller.

This page owns accepted principles, domain contracts and decision records. [Roadmap](roadmap.md) owns scope, priority and acceptance status; [API expansion design](api-expansion-design.md) owns unselected alternatives. Code and tests define behavior. Configuration and commands belong to [Usage](usage.md) and [Development](development.md).

## Contents

- [System boundaries](#system-boundaries)
  - [Protocols](#protocols)
  - [Layers and data flow](#layers-and-data-flow)
- [Semantic contracts](#semantic-contracts)
  - [Semantic IR](#semantic-ir)
  - [Independent task boundaries](#independent-task-boundaries)
- [Adaptation](#adaptation)
  - [Source and target boundaries](#source-and-target-boundaries)
  - [Request and reported meaning](#request-and-reported-meaning)
  - [Reasoning controls](#reasoning-controls)
  - [Media](#media)
  - [Adaptation evidence](#adaptation-evidence)
- [History and replay](#history-and-replay)
  - [Reasoning replay](#reasoning-replay)
  - [Chat reasoning extensions](#chat-reasoning-extensions)
  - [Compacted history](#compacted-history)
  - [Stateful history](#stateful-history)
- [Native Messages projection](#native-messages-projection)
  - [Request rules](#request-rules)
  - [Missing output ceilings](#missing-output-ceilings)
  - [Owner-local cache admission](#owner-local-cache-admission)
  - [Response rules](#response-rules)
- [Runtime and trust](#runtime-and-trust)
  - [Failover](#failover)
  - [Conversation and cache affinity](#conversation-and-cache-affinity)
  - [Streaming and delivery](#streaming-and-delivery)
  - [Credentials and security](#credentials-and-security)
  - [Observability](#observability)
  - [Configurable exchange capture](#configurable-exchange-capture)
- [Settled decisions](#settled-decisions)
  - [Scope and activation decisions](#scope-and-activation-decisions)
  - [Semantic ownership decisions](#semantic-ownership-decisions)
  - [Projection and reasoning decisions](#projection-and-reasoning-decisions)
  - [History and cache decisions](#history-and-cache-decisions)
  - [Routing and security decisions](#routing-and-security-decisions)
  - [Delivery and evidence decisions](#delivery-and-evidence-decisions)

## System boundaries

### Protocols

| Direction | Protocols |
|---|---|
| Downstream | OpenAI Chat Completions, OpenAI Responses, Anthropic Messages |
| Upstream | OpenAI Responses/Chat and compatible dialects, Anthropic Messages, Google generateContent, Google Interactions |

Generation supports JSON and SSE. Google upstreams use streaming intake even for downstream JSON delivery. Interactions uses stateless, full-history requests.

All three Generation client protocols use the public model's configured ordered candidates. A Messages client does not imply a native Anthropic upstream. Standalone Embeddings, Images, Speech and Transcription have separate contracts and remain maintenance-only until their [stage-4 slices](roadmap.md#stage-4-scope).

Provider definitions, public models and bindings are maintained in [provider/catalog.rs](../src/provider/catalog.rs), [topology/catalog.rs](../src/topology/catalog.rs) and [gateway bootstrap](../src/gateway/startup/bootstrap.rs). Registration does not prove activation or live account access.

### Layers and data flow

```text
downstream HTTP: authenticate and collect within bounds
  → client codec: wire → IR
  → validate requirements and resolve the trusted route
  → prepare each candidate from the unchanged source IR
  → target projection and upstream codec: IR → wire
  → transport: trusted origin, auth and deadline
  → upstream codec: JSON/SSE → IR response or events
  → client projection and codec: IR → JSON/SSE
  → acknowledged body delivery
```

| Module | Responsibility | Boundary |
|---|---|---|
| `semantic` | Task types, validation, requirements and stream reduction | No protocol, Provider or execution dependency |
| `protocol` | Codecs, dialects, capabilities and named projections | No network, credentials or registry |
| `lowering` | Cross-protocol capability dispatch and representation errors | Protocols own their representation checks |
| `adapter` | Requests with protocol provenance, replay isolation and codec dispatch | Trusted bindings select rules; no I/O or publication state |
| `topology` | Compile public model → route → endpoints | Endpoints fix origin, path, model and credential scope |
| `execution` | Candidate planning, preparation, intake and delivery state | No agent planning or tool execution |
| `transport` | HTTP I/O and SSE framing | Prepared trusted requests only |
| `credential` | Private store, API keys, OAuth and ordered pools | No credentials in the IR |
| `gateway` | Ingress, startup, budgets, routing and cancellation | CLI listener is loopback only |

Directories follow the Rust module tree. Keep architectural root boundaries; group internals by domain and expose deliberate entry points. Do not add compatibility-only aliases or a generic plugin framework.

Generation validation keeps ordered item/dependency traversal and aggregate charging together; `semantic/task/generation/validate/controls.rs` owns tool/output controls. The stream reducer retains the consuming entry point and global guards; its private `reduce/items.rs` helpers own item identity, result replacement and closure at the original transition positions. They do not expose resumable mutable state after rejection. Execution delivery, body acknowledgement and credential lifecycle remain separate ownership boundaries, not a common dispatcher.

Catalog binding families own both their static declarations and endpoint/public-model construction. `topology/catalog.rs` assembles those families and owns configured ordering; it does not hide individual binding construction among assembly passes.

Protocol implementation is organized by the wire responsibility being reviewed:

| Review boundary | Owner |
|---|---|
| OpenAI complete task requests, settings and history validation | `openai/{chat,responses}/request.rs`; the protocol `mod.rs` files expose the public task entry points |
| OpenAI message/item mapping reused by input, static output and events | `openai/{chat,responses}/items.rs`; these are not whole-request codecs |
| OpenAI shared shape admission and content construction | `openai/shape.rs` owns readers and field policy; `content/{intake,call}.rs` owns item construction and function-call mapping |
| OpenAI event state, poisoning, reduction and profile dispatch | `openai/events/{decode,encode}/mod.rs`; private Chat and Responses grammar/rendering modules operate on that same state, without a second reducer |
| OpenAI representability and candidate-local projection | `openai/lowering/{request,response}.rs`, with shared constraints in `checks.rs`, final encoding guards in `carriers.rs` and incremental checks in `events.rs` |
| Native Messages wire DTOs versus semantic mappings | `anthropic/messages/wire/` versus `anthropic/messages/{request,response,settings}.rs` and `anthropic/messages/items/`; native target binding is not client admission |
| Messages client admission versus delivery | `anthropic/messages/client/{request,response}.rs`; `client/mod.rs` exposes the deliberate client API, `client/projection.rs` owns delivery projection and `client/sse.rs` owns outgoing stream state |
| Messages shared accounting and terminal meaning | `anthropic/messages/{accounting,terminal}.rs`; native mappings and client JSON/SSE use these owners, not helpers belonging to another front end |
| Google request settings versus history content | `google/{generate_content,interactions}/request.rs` owns whole requests; each API's `items.rs` owns its history and media projection |
| OpenAI wire prefix fingerprints | `openai/cache_affinity.rs`; the disposable routing index and affinity decisions remain in `gateway` |
| Shared fidelity state, bounds and owner pruning | `fidelity.rs` owns the aggregate; private `fidelity/{replay,response_records}.rs` implementations group replay binding and output-only report associations |
| Attachment manifests, wire carriers and reconstruction | `attachments.rs` owns the manifest and output view; `attachments/carriers.rs` owns wire recognition/construction and `restore.rs` owns trial-based reconstruction and fallback |

Paths in this table are relative to `src/protocol/`. Public codec entry points remain at their protocol façades; private item and grammar modules are not additional APIs. Fidelity retains one aggregate and attachment reconstruction retains one trial/commit boundary; grouping their implementations does not create separate stores or independent restoration passes.

Request transforms stay with their request owner. Stream grammar, state, reduction and materialization share a stream boundary. Chat and Responses own their envelopes and SSE codecs; only genuinely shared content and state logic is shared.

Protocol differences stay in protocol modules. Native Anthropic, Google and Interactions adapters do not borrow OpenAI switches.

Execution is typed dataflow coordinated by scoped state machines. Semantic reduction, protocol representation, attempt intake and acknowledged delivery each own their state. A stage may transform its output but cannot rewrite another stage's authority or expand the trusted candidate set. Optional delivery stages do not require a generic pipeline registry or detached workers.

## Semantic contracts

### Semantic IR

The IR is neither a protocol DTO nor a union of vendor fields.

Generation semantics come from the official OpenAI, Anthropic and Google APIs. One official API is enough to justify a concept. Preserve its meaning in the IR even when a target needs a declared lossy projection; reject losses of protected semantics or cases without an approved adaptation. Provider dialects and adaptation policies belong to protocol codecs, not semantic types. Reasoning activation, numeric budgets and qualitative effort are not interchangeable. Realtime and standalone tasks do not determine Generation completeness.

- **Ordered items:** instructions, messages, tool calls/results, reasoning, compaction and Provider tool observations share one ordered history. A message is one container, not the whole conversation.
- **Text-only instructions:** request-wide instructions and ordered system/developer instruction parts contain plain text only. Reject media-bearing instructions at source decoding, including reported instruction echoes before any approved omission; do not drop media or move it to another role. Typed instruction parts retain identity and cache ownership without resource references. Request-wide instructions precede every ordered item. An instruction item after conversation content keeps its position; targets without an ordered carrier reject it rather than hoist it.
- **Role-scoped content:** input media belong to user turns and tool results; generated assistant media need their own output owner, not an input resource use. Citations and probabilities belong to assistant text, refusals to assistant messages. No official request carrier attaches them to user input, so validation rejects these states instead of leaving them to target projection.
- **Explicit links:** calls and results link by call ID, never position or name. Request program callers name a preceding program call; linked tool calls/results preserve the same caller. Response-only snapshots may remain self-describing. Item IDs, part IDs, wire IDs and stream indexes remain distinct.
- **Exact JSON:** arguments and schemas retain numeric precision and key order. Parsing rejects duplicate keys and bounds depth, nodes and bytes. A schema is any JSON object; keyword vocabularies and strict subsets are target constraints.
- **Meaningful presence:** distinguish absent, null, empty and default where semantics differ, not mechanically for every field.
- **Separate reasoning:** readable text and summaries are distinct from opaque continuation values. Effort, budget and display are controls.
- **Shared resources:** request/response resources have one declaration table and scoped references. A URL match does not merge identities or authorize I/O.
- **Owner-local replay:** typed replay belongs to its semantic owner. Resource identity, replay permission and runtime cache scope are different concepts.
- **Honest usage:** missing counts are not zero; cumulative snapshots are not summed; derived counts are not reported counts.
- **Honest terminals:** response closure, artifact completeness and turn progress are separate. Failure, truncation and cancellation never become success.
- **No runtime state:** targets, credentials, retries and publication state stay outside the IR.

Clients may resend full history or use [gateway-managed Responses history](#stateful-history), which materializes the same full IR and uses the same pipeline. Cache affinity is not history storage.

When a concept cannot be expressed, show a minimal counterexample and structural options before changing the IR. Do not hide it in private adapter fields.

#### Responses item and event observations

An output `created_by` is a bounded, item-owned Responses report for function/custom results and compaction, not a caller, credential or execution authority. Its record binds the item and referenced resources, including model-visible filenames. Unrelated resources do not invalidate it; owner edits do. Repeated non-null actors must agree, including independently encoded events after publication. Missing later observations cannot erase a published actor. Request history has no actor field, so native input encoding never forwards it.

Summary interruption and compaction sampling progress are event facts. `PartInterrupted` closes a finished summary value without changing item lifecycle, response outcome or replay finality. Responses emits `reasoning_summary_part.done.status: "incomplete"`; other target protocols without a selected carrier reject. Repeatable `CompactionProgress` requires an open compaction owner and supplies no content or completion evidence. Static/history schemas have neither observation: do not invent fields or a retained event log, and do not claim that JSON retrieval can reconstruct their event history.

`ResponseError.original_type` retains a bounded original error classification on failed response snapshots. It is separate from the standalone SSE `type: "error"` discriminator; that event has no original-type carrier and rejects an independently populated classification rather than overwriting it.

#### Google foreground intake

On 2026-10-20 the user selected lifecycle IDs as optional source observations: `id` on created/completed resources and `interaction_id` on status updates may be omitted. Missing IDs neither invent nor erase a source fact. Supplied IDs remain strict: reject null, wrong types, empty strings and over-limit values. Bind the first real ID and reject later changes before accepting usage. The execution-owned HTTP attempt, not the availability of a source ID, owns the stream.

Creation may also omit status; a supplied opening status must be `in_progress`, never a default for absence. Started acknowledges native stream creation, not a reported execution state. Model is still required; no model fallback was selected. Update/final statuses remain required; updates accept the six public v1 values as observations. Other known opening states and unselected SDK status additions are unsupported, not malformed public enum values. Non-null continuation tokens reject at each actual owner, including status updates. Optional event IDs do not activate resumption. Service reports and function-call signatures remain rejected independently of sparse lifecycle admission; the latter has an [unselected carrier proposal](api-expansion-design.md#interactions-call-signature-replay-unselected).

Status observations, generated outcome/progress and delivery publication are separate authorities. On 2026-10-20 the user selected final-envelope authority: validate update fields and checkpoint usage without inferring a transition graph or equality with an earlier status. Do not retain a replayable status-event archive or emit semantic Progress/Terminal from updates, including `failed`/`cancelled` observations. The final envelope alone determines outcome/progress under the existing step/call constraints; DONE and strict EOF are still required. Neither a schema union nor `interaction.completed` alone proves success. This does not add background execution, remote cancellation or retry policy.

Supplied lifecycle `steps` reconcile against a bounded source-ordinal registry, including empty output steps that have no semantic member. Omitted/null steps make no snapshot assertion; an explicit list asserts the source-step count and order. Compare supplied content, call identity/object arguments and thought signatures against closed owners before terminal publication, without adding content, signatures or closure events. Optional content/signature omission does not erase earlier facts. Multipart content is unsupported at both step start and snapshot because deltas carry no content-part index; do not concatenate unknown boundaries. Non-null step errors remain unsupported. Source ordinals are not compacted semantic indexes, and reconciliation uses an ordered scan rather than repeated owner searches.

Both Google request encoders reject explicit tool execution reports: `is_error` cannot distinguish nonexecution, failure and successful execution. Ordinary results without an execution report preserve their error flag independently. Capabilities and direct encoder checks agree; no execution fact is inferred from the flag or result text.

generateContent rejects supplied terminal explanations, aggregate probability reports and model-specific token IDs until their owners/carriers are selected. Nonempty URL-context observations also reject; absent/null or empty URL metadata supplies no retrieval fact. Chosen-token probabilities require an actual number, including explicit zero; missing is not defaulted to zero. Sparse alternative-token facts retain their existing optional IR fields, and unknown token bytes stay unknown. See the [source boundaries](refs/2026-10-17-generation-contracts.md#google-foreground-contracts-2026-10-20).

#### Scoped accounting

`UsageReports` is the sole stored accounting source. It retains the latest bounded snapshot per operation or response-local execution step; steps never synthesize an operation total. Checkpoints replace their scope without summation. A final observation seals that scope but does not certify exhaustive billing. Content edits retain the reports.

`UsageReport` separates reported counters, accounting premises and sparse modality/cache/tool breakdowns. Missing counts remain unknown; source-defined default zeros are marked separately. Duplicate categories, overflowing known sums and breakdowns exceeding their own known parent reject. Sparse lists do not imply exhaustive coverage, and tool-request counts are not tokens.

Split input/tool-input premises also constrain two known counts of the same named modality, even when parent totals are absent: additional counts must add without overflow, and included counts cannot exceed their input counterpart. Unknown modality labels do not establish identity. Failed snapshot replacement leaves every scope and its byte charge unchanged; successful replacement clears omitted facts rather than merging old fields.

The existing `usage()` accessor derives an operation view from that source rather than storing independently mutable totals. generateContent has an explicit prompt + tool-input + output + thought relationship. Interactions retains these source buckets but leaves tool-input and total inclusion unreported because the official examples and generic descriptions do not establish one universal equation. Missing thoughts do not erase a reported non-reasoning output count; they leave the inclusive output view unknown.

Google modality reports and Interactions opening/status/step/terminal snapshots use this contract. `step.stop.usage` is cumulative operation usage; `step_usage` is local to the step. Unknown category/count fields in the optional Interactions breakdown shape remain unknown, not zero. The source contract and conflicting examples are recorded in [protocol notes](refs/2026-10-17-generation-contracts.md#scoped-accounting-contract).

OpenAI delivery requires its established input/output/total relationship. Native reports without that premise may have no deliverable scalar usage, even while the source retains counts. The named `generation.usage-report-view.v1` projection records loss of source scope, breakdowns or provenance on a delivery copy; static and event paths share the optional-view behavior. No custom downstream SSE event is introduced. Strict-complete direct OpenAI projection rejects report loss. Messages still requires representable input/output counts.

Messages preserves cache-write TTL breakdowns, cumulative search/fetch request counts and thinking-token subsets. Input excludes disjoint cache reads/writes; thinking stays within output and is not subtracted into a purported visible-text count. Optional null deltas leave prior observations intact; non-null nested reports replace whole objects. Both children of the native TTL/tool-count object are required: sparse generic reports cannot invent the missing count. A reported TTL sum must equal its known parent, including after a parent update.

Native Messages JSON stores a final operation report; SSE emits opening/delta checkpoints and a final report at message stop, while successful terminal publication remains behind strict EOF. Client Messages delays its opening until the first usable accounting observation or content owner, without buffering content. TTL detail has only an opening carrier: later changed or newly supplied TTL detail rejects rather than entering a fabricated delta field. Tool counts and thinking counts use their real delta carriers. Exact native details survive JSON and SSE; other scoped details use the existing named delivery loss. OpenAI derivation preserves thinking-token counts while omitting unrepresented TTL/request breakdowns with receipts.

An absent delta field cannot erase an optional observation already published in opening usage; such delivery transitions reject. A final report with no usable scalar view cannot resurrect an older checkpoint. Static client projection records checkpoint-to-final observation sealing on its private copy, matching its JSON carrier; exact native static encoding requires a final report. Accounting checkpoints remain lifecycle-only for the existing bounded pre-content hold, not a reason to publish and disable failover.

#### Cached context references

`GenerationRequest` may name one existing cached input base, separately from settings and ordered items. The reference carries bounded opaque identity and an optional reported expiry with subsecond precision. It is required task context, not an advisory optimization. Item edits preserve it; history composition may share an identical base but rejects incompatible bases or retroactive introduction after selected input.

Boundary fidelity records associate the complete reference with a logical issuer scope. This is compatibility evidence, not resource authorization, credential selection or a trusted origin. A changed identity or expiry needs a new association. Known expiry requires an explicit caller-supplied clock and rejects at the expiration instant; missing expiry remains unknown, not unlimited retention. Pure codecs read no clock and perform no I/O.

The generateContent carrier encodes the bound reference without expanding its content. Cached instructions/tools cannot be overlaid implicitly. Other carriers reject without support; default binding admission and public HTTP schemas remain unchanged. Cache creation, fetching, renewal, resource authorization and cache-only prewarming are separate work. See [Google's resource contract](https://ai.google.dev/api/caching).

#### Safety policy and reports

`GenerationSettings` holds requested per-category probability filtering. Target default, disabled filtering and no category blocking are distinct choices. A caller identity hint is not a safety policy. Threshold names do not establish equivalent calibration or enforcement across targets or models.

`GenerationResponse` holds ordered `SafetyReport` observations independently of content and terminal details. Report events distinguish input from output assessment; they can represent blocked input without an output item. Successive assessments remain separate rather than overwriting or requiring agreement with an earlier assessment. Their arrival does not identify an exact content span. Reports never come from requested thresholds, and probability is not severity.

Report counts, ratings, messages and aggregate response storage use the existing semantic bounds. Content edits and native client-view reconstruction preserve reports. Report debug output is redacted. Reports cannot follow the terminal; an assessment alone never computes the outcome. Codecs translate explicit native blocking signals separately under the existing terminal contract.

The classic generateContent codec encodes probability policies and reads prompt feedback and candidate assessments. Omitted proto3 rating scalars use the [source-defined defaults](https://github.com/googleapis/googleapis/blob/8cc2927c2ea64e6876568683050deabf2b52ce56/google/ai/generativelanguage/v1beta/safety.proto), not guesses from the request. An empty native rating list is not an inferred safe verdict. Existing native envelope identity requirements remain unchanged; reports do not synthesize missing identities or models. This does not add Vertex severity/method carriers, Google static intake or Google client endpoints.

Other request carriers reject these policies. Downstream carriers have no assessment slot. An assessment-only report, with no block reason or message and every rating explicitly unblocked, is omitted under `generation.omit-safety-assessment.v1` in JSON and SSE. Blocking reports and safety-bearing settings echoes still reject rather than being treated as incidental settings; rejection prevents a later successful terminal. Default admission and public HTTP schemas stay unchanged; model-effective enforcement and blocking-report delivery remain integration work.

### Independent task boundaries

Standalone task types retain their own request, result and lifecycle contracts. [Operation proposals](api-expansion-design.md#proposed-ownership) describe pending expansion; they do not extend Generation execution.

#### Token counting policy

Prefer upstream-reported counts for the selected model/target and effective input projection, preserving any documented estimate semantics. If those reports are known to be inaccurate, a documented coarse heuristic such as character-based estimation is allowed. MorphieCore does not perform tokenizer-based exact counting or reconstruct hidden model templates. This does not constrain how an upstream implements its own counting.

Keep upstream-versus-estimated provenance and approximation meaning explicit. A rough heuristic disagreeing with an upstream count is not itself evidence that the upstream is inaccurate. Estimates are not inference usage or billing facts; never use them to replace reported Generation usage or fill absent counters. One target's count neither reserves the next Generation route nor guarantees equality across fallback candidates.

Counting must not trigger generation. Concrete reliability criteria, estimation rules/input coverage and client-visible provenance remain part of selecting the [counting profile](api-expansion-design.md#token-counting). A text heuristic cannot silently treat non-text media as zero. This policy activates no endpoint or binding; implementation and acceptance remain [stage-3 work](roadmap.md#stage-3-counting-scope).

#### Independent Embedding path

Embedding has an independent task and execution path. Its [stage-4 expansion](roadmap.md#stage-4-scope) needs per-operation design and acceptance; this boundary does not activate new support.

```text
client request → Embedding codec → Embedding IR
  → task admission and trusted route → target codec → HTTP execution
  → Embedding IR response → client codec
```

The [task contract](../src/semantic/task/embedding.rs) owns bounded inputs, requested dimensions, vectors, input/result association and usage. The [adapter](../src/adapter/embeddings.rs), [execution path](../src/execution/embeddings.rs) and [topology](../src/topology/embeddings.rs) keep bindings, encoding choices and runtime state outside it. Do not represent embeddings as Generation messages, tool results or stream events.

Routes must preserve embedding-space compatibility; equal dimensions do not prove compatible vectors, so Generation's cross-model failover does not apply without an explicit contract. Chunking, vector indexes and retrieval remain caller responsibilities. The roadmap owns implementation and acceptance gates.

## Adaptation

### Source and target boundaries

#### Protocol baselines and Provider dialects

Use two codec responsibilities in each direction:

```text
decode: wire → dialect normalization → protocol baseline + declared extensions → IR + fidelity
encode: IR + eligible fidelity → target baseline + declared extensions → dialect representation → wire
```

This is the selected boundary, not a claim that all existing codecs implement it. The baseline is one official API and its selected source revision, not another cross-protocol IR. Keep Chat Completions, Responses, Messages, generateContent and Interactions distinct. Model restrictions and Provider aliases do not redefine the baseline.

A dialect may normalize an equivalent alias, with conflict checks against the canonical field. A declared semantic extension enters typed IR through an explicit decoder; it cannot hide required content in a restoration-only sidecar. Non-semantic reports and representation records stay bounded, source- and owner-bound outside the semantic task. Unknown fields never become restorable extensions.

Trusted adapters select dialects. Apply the same extension admission to requests, static reports, streamed events and returned history, while retaining their direction-specific unknown-field policies. Registered MorphieCore downstream extensions remain explicit extensions, not additions to an official schema.

Encode from the current typed meaning. A source extension cannot override canonical output, restore a deleted owner or cross an incompatible target. Dialect representation precedes final serialization; there are no post-encoding JSON patches. Projection need not be invertible: named approximations remain losses, not equivalent mappings.

Introduce this seam incrementally around concrete mappings. Do not add a universal protocol DTO, arbitrary extension bag, plugin registry, second stream reducer or whole-stream buffering. Existing framing limits, conflict validation, EOF-gated terminals and replay authority remain unchanged.

#### Protocol tree ownership

Organize protocol code as vendor → wire API → responsibility: `openai/{chat,responses}`, `anthropic/messages`, and `google/{generate_content,interactions}`. The vendor name is where the wire API is defined, not which Provider serves the request, so a compatible endpoint reuses the standard codec plus its dialect instead of copying one. Inside an API, the same module names carry the same role: `wire` holds DTOs, `request`/`response`/`items`/`settings` map to and from the IR, `sse` owns framing and stream lifecycle, `target` owns capabilities and admission, `accounting` owns usage, `replay` owns opaque carriers, and `dialect` owns named deviations. An operation that a protocol does not have keeps no placeholder module.

A shared rule belongs to the nearest common owner: one API, then one vendor, then `protocol`. Dialect code differs by API, so `openai/chat` and `openai/responses` may each name the same Provider; only a rule that must change together is shared. Model or endpoint capability, adaptation policy, Provider identity, origin and credentials stay outside the dialect.

OpenAI's shared dialect selection lives in `openai/dialect/trusted.rs`; Chat/Responses Bailian request rules live in `openai/dialect/bailian.rs`, while the Responses-only SIWC check lives in `openai/responses/dialect/siwc.rs`. Adapters invoke those checks at their required validation stages; moving the implementation does not move its authority or execution order.

Where a direction is missing, the tree says so instead of imitating a fuller sibling: a whole re-encode is not a static response, and a truncated dependency is not a mirror of the source that defined it. Shared records still live at protocol level; the [report views](../src/protocol/reports.rs) are one example, because a protocol that cannot express a lifecycle change must not import that rule from a sibling API.

##### Shared Aliyun billing dialect

DashScope and Aliyun TokenPlan select one `Bailian` wire dialect for their existing Chat/Responses bindings. Billing plans do not own separate parser flags. Readable reasoning and the [history-reading switch](#chat-reasoning-extensions), Chat creation-time normalization/empty continuation call IDs, and response-only `schema_` report aliases apply equally; conflicting identities and aliases still reject. Current Qwen3.8, GLM5.3/Prime and Kimi-K3 Chat bindings use the documented `max_completion_tokens` total ceiling on both plans, not an inferred legacy-cap rewrite.

The [compatible-request subset](refs/2026-10-19-bailian-compatible-requests.md) uses existing IR owners only. Explicit activation, positive Chat thinking allocations and top-k have native carriers without filling defaults. Qualitative effort aliases are model/protocol-bound and recorded, never token presets. Native reply-only caps cannot become total caps unless thinking is explicitly off; GLM5.3's documented legacy total cap is a separate intake exception. Unsupported model values, ignored controls and unlisted Responses guarantees reject before upstream publication. Function output order is not repaired. Native function tables do not establish strict generation or default schema normalization; do not confuse these with Qwen's separate structured-output support.

Client obfuscation preferences remain local delivery intent: Bailian Chat receives only the documented usage option and Responses receives no `stream_options`. Independent answer/thought ceilings remain [unselected design](api-expansion-design.md#independent-output-ceilings-unselected); this slice changes no IR, public admission, bindings or stored-history ownership.

Model catalogs, trusted origins, paths, credentials, billing and replay issuers remain separate. Shared syntax does not authorize exchanging keys, pooling subscriptions or replaying account-bound opaque values across the two origins. TokenPlan is not Coding Plan; see the [dated source note](refs/2026-10-19-same-protocol-inventory.md#shared-aliyun-dialect).

##### Subscription Responses

Grok and OpenAI SIWC use public Responses endpoints, not Chat or a CLI subscription proxy. Both keep upstream history stateless with `store:false` and use body `prompt_cache_key`, not product session headers. SIWC always requests upstream SSE even for a JSON client, groups function/custom definitions and call history in namespaces, and rejects its unsupported controls under the [final typed check](../src/protocol/openai/responses/dialect/siwc.rs).

OpenAI keeps only standard API-key codecs and public SIWC. The legacy ChatGPT/Codex product-backend dialect, combined product-accounting path and unused backend-header carrier are removed, without compatibility aliases. Codex as a downstream client remains a separate concern. This boundary does not activate a direct Platform API-key model binding; the built-in OpenAI binding remains SIWC.

Only SIWC's selected compatibility profile permits absent SSE Content-Type and an empty terminal output reconstructed from already closed event owners. Grok does not inherit those exceptions or `[DONE]` tolerance. Missing owners, conflicting terminal snapshots, incomplete framing and data after termination remain errors. These compatibility observations are not universal guarantees of the public API.

Ordinary usage counts retain their typed meaning. xAI context counters/cost ticks and SIWC program/attribution/zero-tool reports stay bounded in source-bound fidelity, never summed into billed usage or treated as invocation authority. Known numeric reports are strict; unknown nested members are filtered before retention. These reports can return only to the same dialect/issuer and unchanged response dependency. MorphieCore client delivery has no carrier for them: that is an explicit non-lossless report exclusion, not exact same-protocol delivery. Identity omission and any opted-in retention degradation retain their existing named rules.

#### Field handling

| Class | Handling |
|---|---|
| Must preserve | Preserve instructions, order, tools, arguments, call/result links, required continuations, stop reasons and core usage; reject an unrepresentable target |
| Degradable | Apply an approved advisory omission or behavioral approximation to a target-private copy and record it; never silently patch the source |
| Unknown | Ignore unregistered protocol fields; keep known fields strict and never forward unknown input |

Upstream responses are strict on known types, values and relations. Unknown fields are ignored. Unknown Responses event types are skipped and counted; registered content and terminal events remain strict.

Downstream generation requests ignore unregistered fields; diagnostics record their count, not untrusted names or values. Unknown content or role discriminators are rejected because ignoring them would lose content. Protected known semantics that a target cannot express return an error. Trusted configuration and frozen-task requests stay closed.

Parsing direction is explicit: client input, upstream reports and encoder checks have different policies. Nested parsing inherits that direction; no ambient or thread-local policy is used.

#### Client Responses history shorthand

The user selected an input-only compatibility rule on 2026-10-20. A client Responses assistant-history `output_text` part may omit `annotations`, like the already supported assistant string shorthand. Intake materializes the empty annotation list and records `responses.default-history-annotations.v1` once as a body-free `request.decode` insertion receipt. This is not an ignored field or a report of upstream annotation completeness.

Explicit null, malformed arrays and supplied citations retain strict validation; existing citations are never erased. Closed request decoding, upstream JSON and all SSE snapshots continue to require the official array. The rule does not apply to other roles or content kinds, reconstruct discarded citations or change routing. Encoding the resulting IR emits the canonical array. The [client/source evidence](refs/2026-10-17-generation-contracts.md#responses-lifecycle-reports-2026-10-19) distinguishes this selected shorthand from the pinned OpenAI parameter schema.

#### Candidate-local projection

Each candidate starts from the unchanged source IR. Static and streaming paths use the same named rules.

Semantic support, wire representation, execution properties and client promises are separate capability layers. Public capability is not the union of all candidates.

For a fixed target, earlier history projects byte-identically as later turns are added; cache markers may move. Rules must not reinterpret earlier content based on the current turn or request position. This protects cache prefixes and preserved-thinking replay.

Native container boundaries may be omitted by a named client projection. Chat coalesces unannotated text into its scalar wire value without changing bytes; metadata-bearing merges reject. Ordered content, exact arguments and call/result identity remain intact. Required replay attachments use the manifest below to retain their original owner layout rather than treating this wire view as native ownership.

#### Model-effective values

Trusted target caps own optional allowed-effort and maximum-output declarations in [ModelLimits](../src/protocol/model_limits.rs), separate from protocol carriers and semantic feature admission. Omission adds no restriction or capability. Declaration validation rejects empty/duplicate effort sets and zero maxima; configured candidates inherit catalog declarations and may narrow, never widen them. Effort scopes and existing conversion tables remain unchanged.

The shared preflight/preparation encoder checks final typed controls after selected mappings and carrier projections, including retained ordered efforts and inserted defaults. Messages checks the projected native request so its default ceiling and per-message efforts cannot bypass the guard. Reject violations without clipping, remapping to another allowed effort or changing source IR. A model maximum never supplies a missing ceiling; [Messages default insertion](#missing-output-ceilings) is a separate policy. These declarations neither rank routes nor publish broader capabilities or prove upstream enforcement.

### Request and reported meaning

#### Accurate IR and declared loss

The selected policy favors useful conversion over exact wire correspondence. Preserve source intent and observed facts in the IR; prefer an exact target carrier, then an explicitly approved omission or approximation on a target-private copy. Missing fidelity alone need not reject a request. This is the migration contract, not a claim that current codecs implement every rule; see the [implementation plan](roadmap.md#ir-and-codec-migration).

Classify losses by their effect on task constraints, output quality, execution and security, not by whether a field is optional in an SDK. A verbosity preference may be omitted when no target carrier exists. A required output schema, tool selection, call/result link, content item or honest terminal cannot be discarded merely to complete a request. Billing, storage and disclosure controls also need an explicit policy even when they do not change answer quality.

Cache grouping keys are advisory. Prefer an exact cache-retention carrier. A trusted target may explicitly select `cache.omit-day-retention.v1` for `"24h"` only after its default storage policy is reviewed; the policy defaults off, including on all registered bindings. It relinquishes extended reuse, not a promise of shorter storage, no storage or ZDR. Explicit `in_memory`, null and unapproved targets still reject without a carrier. Do not substitute another protocol's TTL or erase active cache options. Automatic caching does not establish a compatible storage policy; null request cache-options containers remain invalid.

Explicit prefix-cache breakpoints belong to the content or tool-definition owner, not a separate request-wide reference plan. Keep TTL intent typed and independent of resource identity, cache affinity and replay. Owner edits carry or remove their own marker; no fidelity record restores a deleted marker. Request-wide automatic caching is a separate control.

| Treatment | Contract |
|---|---|
| Exact representation | Preserve meaning; check source conflicts before alias normalization |
| Advisory loss | Omit selected verbosity preferences or predicted-output optimization through named rules; other fields need individual classification |
| Behavioral approximation | Use selected effort/budget, summary-granularity and role mappings on target copies. Preserve visibility and output ceilings; preserve supported sampling and omit fixed/ignored sampling only under a target policy. Record changes and validate target restrictions |
| Protected meaning | Preserve task content, constraints, execution identity, security boundaries and result truth; reject when no faithful carrier exists |

Decode according to the source protocol before target projection. Known types, variants and relations remain strict even when a later target may omit the field. A known but unsupported critical field is not an unknown field. A target profile's restriction must not prevent the client decoder from expressing a valid source concept already supported by the IR.

Keep source validation, acceptance with approved adaptation, and faithful target support distinct. Project before checking the final target representation; do not advertise an omitted feature as native support. Capability assessment belongs to IR/codec contracts, not to a new route-ranking or fallback policy. The [temporary routing freeze](roadmap.md#temporary-routing-freeze) permits integration to remain disconnected while these contracts change.

Request controls, upstream-reported settings and downstream delivery have different authority. Sent controls do not prove effective settings. A response-only echo can be omitted under a selected delivery rule without discarding generated content; validate source facts and repeated snapshots before that omission. JSON, SSE and retained delivered history must agree on the applied loss.

#### Reported text configuration

Reported text configuration preserves absent, null and object containers through the existing `TextOptions.presence: Presence<()>`; an empty object stays distinct from absent/null. Format and verbosity children require an object container. Native Responses JSON/SSE and stored terminal snapshots retain these observations without inserting format or verbosity defaults; repeated settings comparisons include container presence before any delivery omission.

Null is a reported state, not a request to disable text or an inferred default configuration. Request-side null text containers remain invalid, including direct IR construction. This is a local repair of the reported-fact contract, not a mechanical presence expansion across all settings. The [dated source evidence](refs/2026-10-17-generation-contracts.md#responses-text-container-presence-2026-10-20) distinguishes SDK-observable state from unproven Provider behavior.

#### Message phase transfer

Responses preserves assistant `commentary` and `final_answer` exactly, independently of lifecycle and terminal meaning. Missing/null labels stay unlabelled. These labels affect history interpretation, not merely display; never infer them from text, tool calls or completion.

Requests to Chat, Messages, generateContent and Interactions may omit phase on a target-private copy under `[chat,messages,gemini,interactions].omit-message-phase.v1`. This is lossy visible-history transfer, not equivalent continuation. Source content, order, call/result links and phase remain unchanged.

Chat/Messages delivery does not permanently discard phase by default. `generation.return-message-phase.v1` carries it and its original message boundaries through the existing bounded attachment manifest, including phase-only responses without signatures. Responses uses its native field instead. The manifest is built before wire grouping and published only with the EOF-validated terminal. Standard Chat without structured reasoning rejects this carrier. Existing content-order, annotation, accounting and terminal restrictions still apply; this policy does not establish every Responses-to-Messages answer loop. A carrier preserves replay information, not phase-aware client UI or structured-output parsing.

Returned edited or missing manifests follow the existing attachment contract: keep visible history, do not guess labels, and record a recognized mismatching carrier's omission. See the [source evidence](refs/2026-10-17-generation-contracts.md#compatibility-policy-review-2026-10-18).

#### Service policies and reports

Service selection uses closed request-policy variants, separate from the reported service class. They belong to request execution intent and response metadata, not generation settings or token accounting. Keep native spellings in codecs; do not infer a report from the requested policy, configured target or model alias.

[Messages service tiers](https://platform.claude.com/docs/en/api/service-tiers) distinguishes standard-capacity-only intent from preferring available committed capacity. [OpenAI Responses](https://developers.openai.com/api/reference/resources/responses/methods/create) resolves `auto` through project settings. These policies, and their reported capacity/processing classes, are not interchangeable merely because native names overlap. Reject conversions without a selected exact carrier.

Messages carries a service report in full-message usage, including stream opening usage. The [pinned delta usage](https://github.com/anthropics/anthropic-sdk-python/blob/b4b7916deaf4e5570cd627b1ae7ee4394a4de39f/src/anthropic/types/message_delta_usage.py) has no such carrier. Reject tier-bearing deltas and downstream reports introduced or changed after opening; never manufacture an opening value or emit a successful terminal after rejection. Other repeated native reports must agree, except the explicitly selected [Responses lifecycle relation](#lifecycle-settings-and-service-reports); absent reports stay absent.

Model names may encode a service policy only through an evidence-backed trusted binding declaration. Keep canonical model identity, semantic policy and wire spelling distinct. Effective deployment identity must agree across preparation, cache and replay. Request-dependent alias selection remains deferred with routing integration.

#### Lifecycle settings and service reports

The user selected bounded phase-tagged reports and resumed the Responses acceptance slice on 2026-10-20. That selection alone did not close the overall Generation review or select another routing policy.

Response context has one typed report owner for settings and service class. Native Responses uses at most one opening and one terminal report; protocols without this lifecycle distinction retain their single report. Do not keep another mutable current-settings value, raw snapshots or an event archive. All retained reports share a bounded budget. Instructions, content, execution, usage and terminal facts retain their existing owners.

Opening/progress reports agree within their phase. Validate native types and differences before normalization. Cross-phase equality is the default; the selected OpenRouter Responses contract additionally admits only `summary: auto → detailed`, absent context → `all_turns`, absent mode → `standard`, and service `auto → default`. These independently checked transitions may coexist; other fields must agree. Null is a reported value, not absence or a wildcard. Terminal reports never inherit missing fields from opening reports or request controls. Same-phase drift, concrete-to-different-concrete drift and unselected transitions reject. The [dated evidence](refs/2026-10-17-generation-contracts.md#responses-lifecycle-reports-2026-10-19) distinguishes observations from the selected interpretation.

Responses SSE emits the actual opening and terminal reports in their respective carriers. A terminal-only source does not supply an opening report. JSON and stored retrieval select the terminal snapshot, with named evidence when an opening observation is omitted; strict-complete delivery rejects that loss. Stored typed metadata and retrieval bytes describe the same delivered view, not an unseen opening archive. Terminal absence is not permission to synthesize a successful snapshot. Source validation precedes every projection, and this slice does not approve changing service reports across Chat/Messages carriers.

Metadata updates must preserve already emitted observations, including an absent opening report once a queued/started carrier is emitted. Rejection poisons the encoder; EOF, cancellation, publication and replay restrictions are unchanged. Phase reports do not establish terminal success or permit replay after publication. A gateway-classified failure may emit `response.failed` without an upstream terminal report; it carries no invented settings or service class.

#### End-user attribution

Request projection implements shared opaque end-user attribution through OpenAI Chat/Responses `safety_identifier`, Messages `metadata.user_id` and generateContent `labels.safety_identifier` on admitted targets. These carriers overlap in abuse attribution; they are not authentication, credential, workspace, billing-project or conversation selectors. The [source comparison](refs/2026-10-17-generation-contracts.md#cross-provider-user-attribution-2026-10-19) records their distinct bounds and purposes. Google Interactions has no selected equivalent carrier.

Unified meaning does not collapse independently supplied controls. Retain general user attribution, an explicitly supplied safety identifier and cache grouping separately, reusing the existing typed identity/cache owners rather than an arbitrary metadata bag. OpenAI's deprecated `user` combines attribution and legacy cache bucketing; preserving only one use is not full semantic equivalence.

Apply these rules on the target-private copy:

1. Validate all known source fields before selection, including lower-priority fields. Same-protocol faithful paths preserve each native field and meaningful presence independently; do not automatically modernize a lone OpenAI `user` into new safety/cache fields.
2. When projecting into another protocol's single attribution carrier, prefer an explicit safety identifier; otherwise use the general end-user identifier, including the attribution meaning of legacy `user`. Never replace a supplied safety identifier with a derived fallback. An explicit null blocks gateway fallback; preserve null where representable, otherwise record its omission without filling another ID. This does not assert that null disables Provider attribution. A permitted empty value is not absence.
3. Different `user` and `safety_identifier` values are valid independent facts, not conflicting aliases. If the target cannot retain both, carry the higher-priority value and record omission of the other under `generation.omit-identity-hints.v1`, without identifier values in evidence. Competing explicit aliases for the same semantic field at the same priority must agree in value and presence or reject; object order never resolves a conflict. Duplicate JSON keys still reject.
4. Validate the selected value against the target's length and character constraints. An invalid value rejects; never truncate, lowercase, hash or fall back to a different identity to make it fit. Targets without an admitted carrier retain named omission. All-absent input stays absent; do not derive identifiers from credentials, prompts or conversation IDs.
5. Keep `prompt_cache_key` independent. Neither a user ID nor a safety ID automatically becomes a cache key, and a cache key never becomes attribution. Preserve an explicit cache key through its own carrier or named cache projection. Cross-protocol transfer that loses legacy `user` cache bucketing records `cache.omit-legacy-user-bucketing.v1` separately from identity loss; this decision selects no automatic `user`-to-cache-key conversion.

For example, source `user: "legacy_a"`, `safety_identifier: "usr_b"` and `prompt_cache_key: "template_c"` retain all three on an exact OpenAI path. A single-carrier Messages or generateContent projection selects `"usr_b"` for attribution, never `"template_c"`; source values remain unchanged and unrepresented identity/cache facts receive their respective loss evidence.

Native Messages `metadata.user_id` uses the bounded general-user owner, preserving absent/null/empty and up to 512 characters. OpenAI native limits are 256 characters for `user` and 128 for safety identity; shared storage permits 512/128 independently of source/target checks. Messages attribution projects to OpenAI safety identity, never legacy `user` or cache bucketing; a value over the safety bound rejects even when Messages can carry it. Chat↔Responses preserves both native OpenAI fields independently. Empty/absent metadata without a user ID makes no attribution claim.

GenerateContent emits only the fixed `labels.safety_identifier` entry. Values permit 0–63 Unicode codepoints in `[\p{Ll}\p{Lo}\p{N}_-]`, including uncased international letters, without normalization. Null has no label representation and receives named identity omission without fallback. Interactions and targets without an identity carrier omit unrepresented fields explicitly. A known OpenAI legacy `user` that cannot be retained also loses its cache-bucketing meaning, including on carrier-less OpenAI targets, and receives the independent cache-loss receipt.

[Attribution projection](../src/protocol/attribution.rs) uses the existing typed identity owners and fixed, value-free loss evidence. Current wire contracts register one spelling per semantic slot: no simultaneous same-priority aliases are exposed. Duplicate raw keys reject; arbitrary metadata entries are not identity aliases. Future aliases must uphold the conflict rule above, not add object-order precedence. The existing `source=None` typed-entry convention preserves native target slots for OpenAI/Messages; it is not a newly admitted wire source. No binding activation or generic metadata/label forwarding is implied.

#### Selected request approximations

D3 selects named omission of Chat `prediction` until a faithful carrier is implemented. Validate its known source shape before omitting the optimization, and record the omission rather than treating it as an unknown field. Predicted content is not a constraint on generated text. Neither output text nor reported accepted/rejected prediction counts may be synthesized from it; loss receipts do not contain the predicted body.

P8 permits approximation of summary/display granularity, such as concise or detailed summary intent to a target's single summary mode. Preserve the requested show/hide behavior independently: hiding must not become disclosure, and requested display must not be silently disabled. An absent preference remains absent. These control approximations do not enable reasoning, fabricate summaries, discard generated content or remove required replay. Reject a target that cannot preserve the visibility contract.

P8 also permits named, target-specific approximation among developer, system and user roles when an exact carrier is unavailable. Preserve the original role and authority in source IR; project only the target copy and record the source/target roles. Preserve content and the relative order of existing history. This does not permit inventing assistant/tool speakers, repairing orphan results or accepting an invalid source role.

Request-wide `instructions` may use a leading role-bearing message when the target has no dedicated instructions field. For Chat, a trusted target-local selector chooses `developer` or `system`: prefer developer for targets that support it, and declare system for compatibility fallback. Validate the selected carrier before encoding; an unsupported carrier still rejects. This is an explicit representation choice, not model-name guessing, client-controlled policy or an upstream-error-triggered retry.

Chat already supports system/developer/user messages, so this selector applies only to the leading message generated from request-wide instructions. It neither moves existing ordered instruction items to the front nor rewrites their explicit roles; those follow exact carriers or a separate selected role rule. Keep instruction text unchanged and emit it once. Trusted `chat_system_instructions` selects system; the default remains developer. Keep the selector in protocol/binding policy, outside semantic IR and the public request schema.

P8 keeps image detail and file detail separate. Prefer exact carriers. Omit an explicit detail only when the selected target's default is proven equivalent, or apply a named, directional approximation backed by evidence for that target. An absent carrier alone does not justify omission, and explicit `auto` is not automatically equivalent to absence. Undeclared or unsupported conversions reject. These rules do not permit dropping media, transcoding it or moving it to another role.

P8 protects the model-visible filename. Preserve it through an exact carrier, including OpenAI `filename` and Messages document `title`; reject a target without one when a filename is present. Do not omit the name or inject it into surrounding text. An empty name remains distinct from absence.

P8 permits omission of an explicit reasoning-context control only when the selected target's default is proven equivalent. Otherwise preserve its exact carrier or reject, including when equivalence is unknown. Keep absent controls absent. This policy does not permit deleting messages, necessary reasoning replay or compacted history to simulate a context setting, and does not apply to a Messages document's `context` field.

P8 rejects `DefaultStrictAsPlain`. Keep omitted normalization-default strictness distinct from omitted non-strict defaults and explicit `strict: false`. Responses normalization-default tools must not become plain native tools; reject when their source semantics cannot be preserved. This decision does not select a new schema-normalization implementation or permit removal of explicit schema constraints.

The reverse direction has an exact carrier: an omitted non-strict source default becomes explicit `strict: false` on Responses. Materialize the effective default in the target wire without changing the source IR or schema. This is not a lossy rule and does not permit normalization-default-to-plain conversion.

Schema normalization and explicit strict tools have separate semantic requirements and capability checks. Supporting one does not imply the other. Native contracts reject normalization-default tools even when their codec carries explicit strict tools.

Terminal delivery has four selected approximations. Chat's `stop` officially covers a natural stop and a matched stop sequence, including beside forced tool calls, so natural and sequence-matched causes coarsen to it under `chat.coarsen-completion-cause.v1`; an ambiguous stop is exact. Responses `completed` has no cause carrier, so `responses.omit-completion-cause.v1` omits any cause. A recitation stop becomes OpenAI `content_filter` under `openai.recitation-as-content-filter.v1` and stays incomplete. OpenAI has no context-window reason: Chat reports `length` under `chat.context-window-as-length.v1`, and Responses stays incomplete with a null reason under `responses.omit-context-window-reason.v1`. Messages still rejects ambiguous stops and recitation. These rules never turn a non-success outcome into success.

Messages delivery uses the same terminal contract for JSON and SSE:

| Outcome and reported progress | Required evidence | Messages terminal |
|---|---|---|
| Completed, turn finished | Natural cause, no calls | `end_turn` |
| Completed, turn finished | Matched sequence, no calls | `stop_sequence` with that sequence |
| Completed, awaiting tool results or progress unreported | Completed pending calls, no completion cause | `tool_use` |
| Incomplete, progress unreported | Max-output, context-window or content-filter reason | `max_tokens`, `model_context_window_exceeded` or `refusal` |

Other combinations reject. Response completion alone does not prove natural completion or end-of-turn. Completed pending calls establish required tool results independently of a reported disposition; `tool_use` carries those dependencies without inventing a source progress report. Messages output has no ordinary result items that could already resolve those calls. Unknown completed terminals without pending calls and explicit continuation reject; no unknown cause becomes natural completion and no new terminal approximation is selected.

Messages has no refusal block, and `stop_reason: "refusal"` reports classifier intervention rather than a model-authored refusal message. Messages delivery therefore keeps refusal content as text under `messages.refusal-as-text.v1`. A source without an output ceiling gets the required Messages `max_tokens` under `messages.default-max-tokens.v1`.

These are selected policies, not claims that all bindings support every conversion. Concrete mappings and default-equivalence checks use trusted target policy with source evidence, not client-selected fallback logic or upstream-error discovery. Native targets reject uncalibrated detail/context controls and filenames without carriers; a named rule alone does not establish approval.

### Reasoning controls

#### Reasoning activation and effort scope

The IR separates activation from a scope-qualified effort control. [Reasoning types](../src/semantic/task/generation/content/reasoning.rs), [settings](../src/semantic/task/generation/request/settings.rs) and their codecs own the contract; target-specific integration limits are in [Roadmap](roadmap.md#integration-handoff).

Put activation on `ReasoningRequest` and one optional `EffortControl { scope, level }` on `GenerationSettings`. The initial scopes are `Reasoning` and `Response`. One scoped control is sufficient for the selected contracts; do not add two parallel effort fields without a counterexample requiring both at once. Keep protocol-relevant absence and null separate from active intent.

Activation distinguishes disabled from enabled reasoning; enabled permits reasoning and does not promise a nonzero token count. Execution/allocation mode, numeric budget, summary, display and replay stay separate. `EffortLevel` contains only positive levels: native off spellings normalize to activation, not a minimum effort. An omitted activation does not invent enabled behavior.

Disabled activation plus response-wide high effort is valid IR. Disabled activation plus a positive reasoning-only effort, adaptive/budgeted allocation or numeric thinking budget is contradictory. Standard/pro execution mode is not an activation switch; validate its combinations against the target contract rather than banning it solely because reasoning is disabled. Validate official wire variants separately before constructing this contract.

OpenAI non-off effort and Google levels have reasoning scope. Anthropic `output_config.effort` has response scope and must not change reasoning-object presence by itself. Preserve source scope in the IR. P1 selects target-private cross-scope approximation through trusted directional policy with loss evidence; equal level labels are not an equivalence. Concrete target mappings must be declared, including identities, and undeclared entries reject rather than silently inherit a same-label conversion. A binding without a selected policy retains rejection.

P2 gives explicit disabled activation priority when the target can represent off but cannot represent independent response-wide effort. Preserve off and omit only that effort through a named rule. A target with no off carrier still rejects; the approximation never enables reasoning.

Requirements and target contracts must distinguish response-wide effort from reasoning support. Reported settings use the same scoped types without acquiring request intent that was not reported. No Provider identifiers or mapping tables enter semantic types, and no compatibility-only effort field survives the migration.

#### Reasoning level calibration

The existing level interface performs explicit, hand-authored calibration for the same model under trusted binding/dialect selection. It is not a universal adapter for all reasoning controls. P1/P2 select scope approximation and disabled-priority handling; P3 selects preset-based numeric-budget conversion inspired by [pi's pinned implementation](refs/2026-10-15-generation-projections.md#pi-thinking-budget-presets). Request-policy machinery exists for OpenAI and native projections, and OpenAI intake has directional reported-level calibration. Those interfaces do not establish model-effective support or activate a binding.

Different Providers may expose different labels for one model. A selected rule may map `low` to `medium`, for example. Qualitative level-to-level calibration still requires explicit pairs; equal labels or ordinal proximity do not establish equivalence. P3 separately selects nearest-budget quantization as a named numeric approximation, defined below. Preserve original numeric intent and validate the target's mode, minimum and output ceiling.

P4 preserves supported sampling controls. A named target policy may omit controls known to be fixed or ignored by that model. Unknown effectiveness does not justify guessing or dropping them; keep the rejection until a valid carrier or explicit target policy exists. Protocol grammar and model-effective support remain separate.

Encode and decode mappings are directional. A many-to-one encode mapping has no automatic inverse; normalization of an upstream report needs its own explicit rule. Preserve request intent and reported effective settings separately, never synthesizing a reported setting from the request. Apply each mapping once at its boundary, record its named provenance, and validate the result against the target contract.

##### Thinking-budget quantization

The selected shared preset uses six distinct budgets across the positive OpenAI/IR level vocabulary. It follows pi's preset-based approach but deliberately differs from [pi's default values](refs/2026-10-15-generation-projections.md#pi-thinking-budget-presets). These are MorphieCore approximation settings, not official OpenAI token equivalents or universal target limits:

| Level | Thinking tokens |
|---|---|
| none / disabled | Activation only; not a positive budget entry |
| minimal | 1024 |
| low | 2048 |
| medium | 4096 |
| high | 8192 |
| xhigh | 16384 |
| max | 24576 |

A trusted target policy selects this shared preset or an explicitly declared target-specific table. Duplicate token values are valid. For a positive numeric budget `b`, choose the entry with the smallest absolute token distance `abs(b - tokens(level))`; among every entry with that minimum distance, choose the highest level in `minimal < low < medium < high < xhigh < max`. Table iteration order must not affect the result. Use overflow-safe integer distance, not floating-point midpoints.

For a target-specific table `{low: 500, medium: 500, high: 1000, xhigh: 1000}`, budget `750` selects `xhigh`. An exact `500` selects `medium`, and exact `1000` selects `xhigh`; duplicate budgets can intentionally make a level round trip non-identical.

The shared preset's upward tie boundaries are `1536 -> low`, `3072 -> medium`, `6144 -> high`, `12288 -> xhigh` and `20480 -> max`. Exact preset values retain their level, including `8192 -> high`, `16384 -> xhigh` and `24576 -> max`. Arbitrary numeric budgets remain lossy: the representative tokens do not replace the original source value.

Absent controls stay absent. Off is decoded into activation before matching; protocol-specific zero/automatic sentinels never enter positive-budget quantization. If a target can preserve the requested native numeric budget, prefer that carrier instead of quantizing unnecessarily. Qualitative requests with an exact target carrier likewise need no budget round trip.

Retain the original budget and record the selected table, matched level and representative tokens as adaptation evidence, not reported model effort. Quantization does not bypass scope or target-level admission, invent a missing table entry, activate a binding or change route policy.

P3b rejects a request when its applicable explicit output-token ceiling is smaller than the thinking budget. This applies to both client-supplied numeric budgets and budgets generated by a selected level-to-budget conversion. Preserve the budget and ceiling; do not lower the budget, select a cheaper preset, raise the ceiling or disable thinking to make the request fit.

Passing this comparison is necessary, not sufficient. Equality is accepted only if the target contract permits it; stricter budget/output relations, protocol minima and required answer room still apply. Reject a target-invalid combination without automatic adjustment. No extra gateway-defined answer reservation or budget-reduction policy is selected. Ordinary preset lookup and nearest-budget quantization remain unchanged.

Rules cannot expand accepted content types, bypass model restrictions, select a route or modify the source IR. No public request override is selected. Qualitative level calibration remains hand-authored Rust policy. Trusted startup configuration may supply a public model's six numeric thinking budgets; each candidate explicitly selects budget-to-effort or effort-to-budget conversion. Omission selects no conversion. The existing algorithms, target restrictions and output-ceiling checks remain authoritative; there is no generic rule engine. P6's diagnostic switches configure capture, not semantic mapping.

##### Initial mapping plan

Select a policy for one canonical model and trusted endpoint/protocol binding, not for every model of a Provider. It declares a stable rule identity, effort scope, accepted native levels and independent encode/decode tables. Use finite typed pairs or match arms; off, absence, null, budgets, display and execution mode are not table entries.

The following is a hypothetical reasoning-scope encode policy, not an activated binding or an assertion of model equivalence:

| Canonical level | Endpoint A accepts medium/high | Endpoint B accepts medium/high/xhigh |
|---|---|---|
| low | medium, explicit calibration | medium, explicit calibration |
| medium | medium, identity | medium, identity |
| high | high, identity | high, identity |
| xhigh | reject until an explicit mapping is chosen | xhigh, identity |
| minimal/max | reject until explicitly supported or mapped | reject until explicitly supported or mapped |

For this example, decode explicitly declares identity on each native level: a reported medium remains medium, even when the request was low. A separately authored decode rule may normalize it differently; never infer that rule from encode. Every handled entry, including an identity, must be declared. Missing entries reject, with no supported-level fallback. Do not chain table entries or apply the encode table to an already adapted candidate.

The code interface is [ReasoningEffortMapping](../src/protocol/reasoning.rs): hand-authored encode/decode pairs, effort scope and declared native levels selected by `ApiKeyBinding.effort_mappings` for an exact OpenAI protocol member. OpenAI endpoint construction carries that mapping into request encoding and static/SSE metadata intake; adapter selection validates its canonical-model identity. Bindings without a selected mapping use their protocol's representation rules, not a generated mapping. Native request-policy projection is separate from this qualitative encode/decode mapping.

Activation is outside the positive-level table, so a table cannot turn off into on. Absence and null never create a level. The interface does not add Provider-only level labels to the standard IR.

Selected policies calibrate settings and ordered configuration items independently. Responses intake checks native configuration snapshots across added, done and terminal boundaries before normalization. Request projection maps a private ordered copy; a configuration budget without an item carrier rejects. Always-reasoning bindings also reject disabled configuration history. A settings policy must not silently bypass historical native-level admission.

Validate duplicate keys, output levels, effort scope and binding identity at the trusted target boundary. Existing topology compilation also checks these declarations; its policy is frozen for this migration. Preflight and preparation project afresh from unchanged input through the same pure codec path when connected. Calibration precedes the final supported-level check, but cannot bypass source validation or final target restrictions.

Validate native types, alias conflicts and source-snapshot consistency before a many-to-one normalization can hide a disagreement. Responses intake compares repeated reported settings before effort calibration, separately from item-snapshot validation. Normalize only fields actually reported. Static and streamed metadata use the same rule, and downstream delivery does not reapply the source Provider's encode map.

##### Implementation owners

| Owner | Responsibility |
|---|---|
| [Semantic reasoning types](../src/semantic/task/generation/content/reasoning.rs), [settings](../src/semantic/task/generation/request/settings.rs), [requirements](../src/semantic/task/generation/requirements.rs) and [contract](../src/semantic/task/generation/contract.rs) | Activation and scoped effort only; no model-specific level policy |
| [Protocol reasoning helper](../src/protocol/reasoning.rs) | Explicit qualitative mapping pairs plus separate P3 preset lookup and nearest-budget quantization with highest-level ties; no inferred qualitative inverse |
| [Catalog binding declarations](../src/topology/catalog/bindings.rs), [catalog construction](../src/topology/catalog.rs), native catalog constructors and [endpoint compilation](../src/topology/endpoint.rs) | Select policy by canonical model and binding, validate it and pass it through representation caps to the adapter; do not reuse a Provider-wide switch for model-specific levels |
| [OpenAI adaptation](../src/protocol/openai/dialect/adaptation.rs), [request projection](../src/protocol/openai/adapter/request.rs), [Messages projection](../src/protocol/anthropic/messages/request.rs), [generateContent projection](../src/protocol/google/generate_content/request.rs) and [Interactions projection](../src/protocol/google/interactions/request.rs) | Preserve scope and apply a selected level policy once to candidate-private typed representation before final validation and serialization; no Provider-wide level calibration switch |
| [Upstream adapter](../src/adapter/upstream.rs), [Chat response intake](../src/protocol/openai/chat/response.rs) and [Responses response intake](../src/protocol/openai/responses/response.rs), [event intake](../src/protocol/openai/events/decode/mod.rs) and native decoders | OpenAI carries its selected decode mapping into actual reported-setting parsing, including SSE. Any future native report calibration needs independent policy propagation; do not put it in `ReplayTarget` or discover it from a response model string |
| [Fidelity](../src/protocol/fidelity.rs) and existing named request rules | Record bounded normalization provenance and actual encode adaptation, without restoring unsupported controls or synthesizing effective settings |

Native `Endpoint::adapter()` currently carries profile/replay labels rather than all representation caps. Native request conversions do not supply an implicit inverse for reported settings. If a concrete native binding needs reported-level calibration, select an independent decode policy and propagate it explicitly into intake before claiming support; request projection alone does not establish it. The ordinary downstream client decoder uses its declared client contract, not a guessed Provider policy chosen from business JSON.

Keep [candidate preflight](../src/execution/plan.rs) and transport free of mapping tables. Preflight already invokes endpoint encoding, so preparation must reuse that same codec path rather than add a second execution-layer mapper.

### Media

Image, audio, video and file resources are inert: inline data or a URL, never fetched, decoded or uploaded by the gateway. Each kind is explicit, because a URL source carries no media type. An inline file keeps its declared MIME, and targets that dispatch by MIME may carry it.

- Chat video uses the registered extension `{"type":"video_url","video_url":{"url":…}}`, shared by OpenRouter, DashScope, Zhipu, vLLM and ModelBest. Responses video uses `{"type":"input_video","video_url":…}` from Open Responses and OpenRouter, in user messages only. Only targets whose dialect declares the carrier receive video. Responses clients may send `input_video` in tool results, but no bound target carries tool-result video: OpenAI and Interactions results have no video part, and Gemini function responses admit only PNG, JPEG, WebP, PDF and plain text.
- Native codecs reject explicit image/file detail without a declared carrier or selected target rule, and filenames without exact carriers. No target-default equivalence is inferred from a missing slot. Inbound options with no IR slot that change model-visible content, such as a Messages document `context`, remain rejected.
- A file name is the model-visible document name: OpenAI `filename` and Messages document `title` map to each other. A present name needs an exact carrier; do not drop it or inject it into text.
- Instructions are text-only, including Responses system/developer messages and Messages mid-conversation system messages. Reject image, audio, video and file parts before target selection, even when a shared SDK content union admits them; user and tool-result media retain their separate contracts.
- Tool results may carry media of any kind. Admission needs tool-result media and the kind's input modality. An inline file whose MIME type is image, audio or video also needs that modality, in addition to file input. Responses carries tool media; standard Chat does not, so Chat targets reject it.
- generateContent preserves tool-result parts in an ordered array under `functionResponse.response.output` or `error`: text stays a string element and each media occurrence has one `$ref` to its own `parts[].inlineData.displayName`. Result-local positions determine stable labels, including repeated uses of one resource. Labels are not filenames. Scalar text remains a string; no text concatenation or media deduplication occurs. Existing inline/MIME/options/size restrictions remain authoritative.
- Provider file IDs and file URIs are bound to the gateway credential's workspace or project. Client-supplied references are rejected.
- User and tool-result media uses admit only inline data or URLs. Generic opaque/namespaced identities remain available for citation sources, not model-input media.
- Google protocol boundaries reject media URLs on the Files API host, even when a client labels them as ordinary image or file URLs. Public URLs stay inert.
- Image and file formats, encoded sizes and counts are target constraints, including tool-result media. Pixel dimensions and actual image validity remain upstream checks; the gateway does not decode or transcode images.
- Some standard options are protocol records, not IR fields. A Responses `prompt_cache_breakpoint` on an image or file reaches Responses targets; Chat targets reject it. Native Chat content-part `prompt_cache_breakpoint` also rejects at intake until its carrier is selected, before permissive client filtering can discard it. A Messages image with `transformations.oversized_image: "error"` keeps it on Messages targets and is rejected elsewhere, because other targets may downsize; `downsize` is the default and needs no record.
- A failover route may include candidates narrower than the public contract. Requests they cannot represent skip them, which reduces failover rather than admission.

Carrier tables and sources: [Multimodal carriers](refs/multimodal.md).

### Adaptation evidence

Every selected loss produces bounded typed evidence with a stable rule, direction, semantic field category and action. Safe control summaries show enum values and presence; numeric request summaries allow only thinking budgets, representative preset tokens and output-token ceilings. Keep request intent, sent controls, upstream reports and delivered facts distinguishable. Evidence cannot restore omitted values, change semantic meaning or select an upstream.

Keep the bounded typed projection evidence needed for adaptation correctness; operational reporting follows [Observability](#observability), not a parallel diagnostic model or generic rule engine. Record losses independently once per affected control or aggregate repeated event occurrences within existing budgets. A prior refusal receipt cannot suppress a later usage-view receipt. These summary records do not contain bodies, prompts, schemas, arguments or opaque values; full business content belongs only to the separately controlled capture below. Arbitrary input strings and unknown field names are not automatically safe summary labels.

Evidence belongs to a successfully checked projection boundary. A rejected batch adds no new receipts; earlier successful prefixes retain theirs when later intake or encoding fails. Late-reported facts follow the same rule. Native grouping and accounting transformations record evidence before their source ownership is removed. Encoding evidence alone proves neither network transmission nor effective Provider behavior.

Required output evidence is established by explicit request controls, the source contract or continuation requirements. Preserve it or reject an incompatible target. Selected incidental settings echoes and auxiliary reports may be omitted after source validation, with evidence. Absence of a request flag alone does not prove that citations or other content are incidental; deferred citation support remains deferred.

Chat omits incidental settings echoes because its response has no settings carrier. Responses omits unsupported independent reasoning-control echoes while retaining representable settings. Strict-complete delivery rejects either loss. These rules never drop generated reasoning, probabilities, citations or continuation values. Messages rejects unsupported probabilities and citations; its retained usage uses the same explicit cache/total view as delivery.

#### Public capability and adaptation notice

P7 selects an authenticated read-only capability snapshot and a conservative HTTP response-header notice of possible adaptation. The [initial partial surface](usage.md#capability-discovery-and-adaptation-notice) publishes gateway-boundary contracts for activated Generation clients, not model-effective support or full-loop availability. Unlisted cases are unassessed. Actual per-exchange loss receipts remain in internal diagnostics; no public receipt store, custom SSE events or response-body extensions are selected.

Publish by public model and distinguish request acceptance, downstream delivery and history replay, including JSON/SSE limits. Describe faithful support, approved adaptation with stable rule IDs, and rejection. Reuse typed contracts and policies; do not serialize admission booleans or union candidate capabilities. Reveal no credentials, deployment origins or internal candidate identities. Keep `/v1/models` as model discovery rather than overloading its standard fields.

The notice means possible adaptation, not applied loss or proof of Provider-effective settings. Its absence does not certify an exact full loop, and clients may not display it. Late stream reports make response headers unsuitable as complete actual-loss receipts. Do not buffer a complete stream to collect receipts, rely on trailers, add per-request loss-policy overrides or change routing. Neither a notice nor a receipt authorizes dropping completion causes, safety reports or other protected facts; only a selected named rule does.

The [P7 boundary review](roadmap.md#p7-boundary-review) owns further capability expansion and reconnection sequencing. Model-level binding evidence and selected client loops must be specified before activating additional support; publishing a gateway rejection or delivery policy does not activate it.

## History and replay

### Reasoning replay

Opaque upstream values reach clients in a tagged envelope:

```text
mc1. + base64url JSON: {o: issuer fingerprint, f: native format, d: payload}
```

Runtime fingerprints bind Provider, upstream model and the normalized trusted deployment origin/path. They do not include public aliases, credentials or ingress keys, and reveal no URL or credential locator. Changing a deployment does not authorize replay of another deployment's opaque values. Clients must treat the whole value as opaque.

Once an item's replay value is final, item closure may repeat it unchanged but cannot remove, downgrade or rewrite it. Final replay does not imply a completed artifact. Partial-to-final replacement preserves format and intake origin.

1. Restore the native payload only for the matching deployment/model and carrier.
2. Transfer visible history to an incompatible target by omitting complete foreign reasoning, including its readable thinking and opaque value. Do not present another model's thinking as an assistant answer.
3. Preserve visible messages, media and real call/result associations. A tool turn need not have a final assistant answer, but incomplete owners or partial replay cannot be repaired by omission. Compaction and cached context remain required input.
4. Never send bare, malformed or unsupported-version envelopes upstream.
5. Reject damaged compatible replay or an unavailable required-context carrier. A different native replay format is an explicit transfer case, not permission to weaken compatible replay checks. A returned attachment manifest that no longer matches its visible history is the selected exception below: it is dropped, not repaired.

The selected pi-inspired transfer uses `[chat,responses,messages,gemini,interactions].transfer-visible-history.v1`. It changes only the target copy and records the loss; it is not equivalent reasoning continuation. Dependencies bound to the selected target are checked before foreign removal, so transfer cannot hide a missing or edited compatible replay owner. Responses call-item wire IDs associated with removed reasoning are forgotten; semantic call IDs and results remain unchanged. A mixed assistant run with retained replay and potentially shared call-item aliases rejects rather than guessing which dependency may be removed. No tool results, validator-bypass values or replacement user messages are synthesized.

Reasoning without provenance is not automatically foreign. `reasoning_as_text` remains disabled by default and controls the existing explicit conversion of unattributed readable reasoning when no exact native carrier is available. It does not enable or disable foreign-history transfer, nor change reasoning display controls. Valid native carriers remain available. A changed item kind gets a derived identity in the candidate copy, not the issuer's identity.

Text conversion preserves the reasoning item's lifecycle. A partial or in-progress owner never becomes completed merely because its readable text has a carrier. Responses can retain that status; targets without a partial-history carrier, including native Messages, reject it.

The envelope is not encryption, authentication or local proof. It cannot choose a route, origin or credential. The issuing upstream validates its payload.

Some native thinking is account-bound. Exact-credential affinity helps preserve it, but deployment/model compatibility alone cannot guarantee replay after credential fallback.

Carriers include Responses `encrypted_content`, Chat encrypted reasoning details, Anthropic thinking signatures and redacted thinking `data`, Gemini `thoughtSignature` and Interactions signatures.

Redacted thinking is an opaque-only reasoning item with its own replay format, so the native block type survives the envelope. Messages clients receive it as `redacted_thinking` whose `data` is the envelope; a streamed reasoning block opens on its first delta or at its end, once a redacted value can be recognized.

generateContent signatures on ordinary text and function calls belong to `Part.replay` and `ToolCall.replay`, with a distinct `GoogleGenerateContentSignature` format. Thought state remains on its reasoning owner; Provider-tool replay stays separate. Fidelity binds part identity, ordinal, text, lifecycle and value, or the call's identity, name, exact arguments, context, lifecycle and value. Sealing checks recorded obligations even when an owner or token was removed.

The bounded `mca1.` attachment manifest contains original item/part boundaries, content dependencies, lifecycle, message phase and individual `mc1.` values, not replacement text or arguments. It travels through a trailing Responses encrypted reasoning item, a trailing structured Chat encrypted detail, or a trailing empty Messages thinking-signature block. Phase alone requires it only for Chat/Messages; Responses retains native phase without an extra record. Each client encoder emits that carrier from typed input before its own validation and bounds. These are transport records, not semantic reasoning items. Client intake consumes them before ordinary reasoning decoding.

Restoration requires unchanged text bytes and exact parsed call arguments, preserved non-text order and positions, and a matching manifest. Text coalescing can therefore recover two distinct signed `"OK"` parts without guessing which one owns a signature. Changed/deleted visible content, conflicting phase or partial status, malformed or misplaced carriers, ambiguous scopes and metadata/cache boundaries that cannot survive a split make that carrier unusable. Under `generation.omit-unmatched-replay-manifest.v1`, intake keeps the returned visible history unchanged, drops only that carrier's part/call replay and phase-restoration information, and records the omission. Other carriers and ordinary reasoning replay are unaffected. The current generateContent turn may still fail upstream when a function call loses its required signature; the gateway does not substitute a value. Responses scopes end at their carrier and start after a preceding carrier or non-assistant separator; unrelated unsigned assistant prefixes can make that scope ambiguous.

This is original-segmentation recovery, not authenticated edit identity: identical-content edits and complete removal of the manifest cannot be detected statelessly. Neither the manifest nor its dependency hashes authenticate the client. A stream builds the manifest from its existing EOF-validated materialization and publishes it with the withheld terminal; it does not add a second stream buffer. Static, SSE and retained history share the same owner contract. Standard Chat without structured reasoning support rejects this carrier in both delivery modes.

- An attachment is never a tail-anchored or call-ID-anchored surrogate reasoning item. Moving or dropping its visible owner cannot silently move or erase required state. Signature-only surrogates returned from earlier gateway history are dropped with `gemini.omit-legacy-signature.v1`; other malformed thought payloads still reject.
- An explicitly observed empty signed text part keeps its text owner. A signature on an empty streaming fragment may also close accumulated text. Both differ from a missing carrier.
- Distinct parts within one generateContent frame retain their boundaries; unsigned preceding text never becomes part of a later signed owner.
- A signed thought closes its owner. Later thought text starts another item, so adjacent signed parts never overwrite one another.
- The selected Interactions carrier is a thought signature. Standard function calls have no official signature field; such fields and call-signature payloads reject. Built-in tool signatures remain a separate deferred carrier.
- Preserve every compatible native signature, even when an upstream sample accepts an omitted one. An empty thought remains ordered reasoning, not a movable attachment.
- Foreign part/call replay is removed only from completed owners with final values; visible text and real calls/results remain. Affected call wire aliases are cleared without changing semantic call identity.
- generateContent and Interactions are separate formats. Foreign state is omitted under the selected transfer policy, not translated.

Pinned pi parser/request-builder checks preserve equal-text owners and signed calls on Chat, Responses and Messages; JavaScript-rounded arguments drop that turn's carrier. Empty signed text returns through Responses and Messages. Pinned pi Chat drops an assistant turn containing only opaque details, so that turn's signature is lost. This is an accepted client limitation: generateContent does not strictly validate non-function-call signatures, and turns with calls keep their carrier. No placeholder text or hidden-state reconstruction replaces the turn. This is client-codec evidence, not opencode compatibility, binding admission or Provider acceptance; see the [protocol notes](refs/2026-10-17-generation-contracts.md#signed-google-parts).

### Chat reasoning extensions

Chat uses OpenRouter shapes for semantics its standard fields cannot carry:

| Carrier | Meaning |
|---|---|
| `reasoning`, `reasoning_content` | Readable reasoning view |
| `reasoning_details` | Ordered reasoning items for replay |
| `reasoning.text` | `text`, optional `signature` |
| `reasoning.summary` | `summary` |
| `reasoning.encrypted` | Opaque `data` |

Details may also carry `id`, `format` and `index`. Signatures and encrypted data use the replay envelope above. Clients return details unchanged.

One assistant message carries the reasoning items ahead of it. A replay value closes its item; adjacent items without values merge. The readable view concatenates readable details.

Streamed readable deltas carry both readable views and no empty opener. Client input does not rely on returned indexes or format labels because target clients merge fragments.

For registered plaintext views, absence, null and an empty string make no text assertion. Nonempty duplicate views must agree with each other and with the readable structured view. Malformed known values still fail. An empty details list has no parts; explicit structured parts, including empty text, retain their content, order and owners. This normalization does not change visibility controls or relax post-terminal lifecycle checks.

Register new extension shapes here one at a time. The old `_openbridge` attachment is rejected and never emitted.

DashScope's selected Bailian Chat carrier is readable `reasoning_content` plus the independent `preserve_thinking` control, not OpenRouter encrypted details. The public gateway Chat codec registers this source control so it is not lost before target preparation. `ReasoningRequest.read_history` preserves absent/true/false without inventing effort or activation; null and non-booleans reject. It controls whether supplied historical reasoning is read, not current-turn context, replay permission or retention. Do not insert a model default or map false to `CurrentTurn`. The evidenced native dialect carries it; unsupported targets reject. Readable reasoning cannot resume after text or call output begins, even when an earlier reasoning owner exists.

### Compacted history

A compaction item owns opaque context at one position in the ordered history. It is not a readable summary, reasoning or a message with an invented role. Responses preserves its identity and `encrypted_content` in input, static output and streamed item snapshots. Only a completed opaque value can enter replay history; an unfinished stream cannot become a static compaction item.

Compaction uses the [replay envelope](#reasoning-replay) with a distinct format tag. The gateway restores the native payload only for the matching deployment/model and Responses carrier. Bare, malformed, foreign or unbound values are rejected, never stripped: unlike optional reasoning, removing compacted context would lose history. Chat Completions, Messages and Google targets reject it in the absence of a carrier.

Compaction admission is separate from reasoning support. Codec support does not activate existing model bindings. This contract receives and replays compacted history; it does not schedule compaction, expose `/responses/compact` or store conversations. The [official guide](https://developers.openai.com/api/docs/guides/compaction) and [pinned SDK](references.md#pinned-sources) define the selected wire contract.

### Stateful history

Downstream Responses offers stateless full-history requests and gateway-managed `previous_response_id` continuation. The gateway retains canonical request input and the delivered response projection, with replay envelopes intact. The Responses codec materializes their ordered items with new input before ordinary IR validation and candidate preparation. Storage facts stay in protocol metadata, outside Generation IR and upstream execution hints.

Storage defaults to true, matching OpenAI's Responses default. Omitted or null `store` retains the new response; explicit `store: false` opts out. A request may read a stored predecessor without storing its successor. Top-level instructions and other settings come from the new request, not its predecessor; instruction items in history remain history.

The synchronous API includes response retrieval and deletion and cursor-paginated input items. Only fully delivered terminal responses become visible. Unknown, deleted and expired references have the same not-found result. Stored branches own complete snapshots: deleting or expiring an ancestor does not invalidate an already stored descendant. Retrieval is JSON only; there is no event log or stream resumption.

History lives in bounded process memory with a TTL. Expiration or restart invalidates stored references; there is no disk durability or restart recovery. Other storage backends require a later decision, not a prebuilt backend framework.

Capacity accounts for typed payloads, retrieval bytes and owner records. A final snapshot reserves capacity before its terminal frame is queued. The body publishes it synchronously at the final acknowledged handoff, so an immediately following request can resolve it. Dropped or failed delivery releases the reservation. TTL starts at publication, reads do not extend it, and expired entries are reclaimed on store access. Unexpired entries are not evicted.

Upstream calls remain stateless, using full history and `store: false` where supported. Neither Provider-only nor hybrid Provider-held continuation is planned. Downstream storage intent never enables upstream storage. Cache affinity remains separate from history ownership.

The scope is conversation history management, not execution persistence. No `background: true`, detached generation jobs, scheduler or agent loop is planned. JSON and SSE requests keep the existing cancellation and honest-delivery rules.

## Native Messages projection

Native requests preserve thinking, tool-use/result order and call IDs. Messages owns its profile, cache controls, limits and session headers.

Consecutive same-role native messages form one logical turn for tool-result adjacency and prefix validation, without changing their wire grouping. Optional null usage-delta counters leave earlier reports intact; numeric zero replaces them. Native response encoding and client delivery share the completion-evidence checks.

### Request rules

These describe current native behavior, not completion of the [loss-policy migration](roadmap.md#ir-and-codec-migration). A richer reasoning conversion requires a model-bound trusted request policy; absent policies retain rejection. Default-strict-to-plain conversion remains rejected by decision. Selected P8 behavior follows the [request policy](#selected-request-approximations).

- Disabled, adaptive and ordinary manual-budget thinking have distinct native carriers. Manual budgets require at least 1024 tokens and strictly less than the explicit output ceiling, independently at source intake and final native encoding. No clipping, cap increase or manual-interleaved exception is selected. Trusted target policy disables manual thinking on adaptive-only Go/Haiku; carrier availability does not widen that binding.
- Response-wide effort maps to `output_config.effort` independently of thinking. Reasoning-only effort is rejected, not broadened to response-wide effort.
- Summary intent maps to summarized or omitted display only with a representable activation/allocation. Display alone does not enable thinking. Uncalibrated reasoning context rejects. Codecs retain native temperature/top-p/top-k; an explicit fixed-sampling target policy omits temperature/top-p with a receipt and rejects top-k. Go/Haiku selects that policy, not the generic protocol profile.
- Client `thinking.display` is preserved independently of effort and budget, using the same display parser as native intake.
- A leading developer instruction maps to the system prompt by name.
- Later instructions, `ToolChange` items and response-scope `ConfigurationUpdate` items become [mid-conversation system messages](https://platform.claude.com/docs/en/build-with-claude/mid-conversation-system-messages) at their envelope boundary, also under the developer rule. A run of them forms one system section and shares a message; an effort sets that message's `output_config` and closes it. Content placement follows the native rules: after a user turn, before an assistant turn or at the end; an effort-only message may appear anywhere. An item inside a merged turn rejects, and turn-scoped `clear_at` rejects. The profile default keeps `ordered_instructions` and `tool_changes` closed; a reviewed binding opens them.
- Per-message effort, `thinking.block_binding` and tool-change blocks are beta carriers that the trusted binding enables (`Carriers`). Without one, `messages.flatten-message-effort.v1` sets top-level effort to the level in effect for the requested reply, `messages.omit-block-binding.v1` drops tolerant `drop_block`, and `messages.collapse-tool-changes.v1` declares the final available tool set, withholding never-surfaced deferred tools. Explicit `"error"` binding has no fallback and rejects. Non-Messages targets omit `drop_block` and eager tool input (`generation.omit-eager-tool-input.v1`) the same way. `anthropic-beta` values are derived from the encoded body; client headers never select them.
- Returned `input_transformations` are validated, ignoring unknown entry types as documented, then omitted under `messages.omit-input-transformations.v1`: their paths index the upstream request.
- Call IDs outside the native grammar map to stable digests; collisions fail rather than merge calls.
- Owner-local cache markers preserve their placement and 5m/1h TTLs. Do not add automatic caching to a request with explicit markers: four explicit markers plus automatic caching, or an automatic TTL differing from the tail marker, would conflict. Generic codecs preserve absent automatic policy. Go/Haiku explicitly selects the existing marker-free 5m default under `messages.default-automatic-cache.v1`, on a candidate copy after source admission. Caller-selected automatic caching remains a separate, default-closed admission feature. Cache hits still depend on upstream thresholds.
- A JSON-schema output format maps to `output_config.format`, which always constrains decoding. Its name is dropped by a named rule; a JSON-object mode or a schema description has no carrier and is rejected.
- A source without an output cap uses the [trusted target default](#missing-output-ceilings), if declared, under `messages.default-max-tokens.v1`. Preserve an explicit client ceiling; pure codecs invent no fallback.
- Client `top_k` decodes into the IR control and has a native carrier; fixed-sampling bindings reject it without changing the source.
- OpenAI stateless controls are not forwarded as native fields. Requesting an encrypted reasoning carrier selects delivery, not native thinking.

Exact rules live in [request.rs](../src/protocol/anthropic/messages/request.rs) and [carriers.rs](../src/protocol/anthropic/messages/carriers.rs).

### Missing output ceilings

The user selected these configuration and migration details on 2026-10-20. Messages `TargetCaps.default_output_tokens` owns an optional positive `u64` insertion policy, separate from `ModelLimits.maximum_output_tokens`. It is not a client field, model capability or routing policy. [Usage](usage.md#model-configuration) owns configuration syntax.

Candidate omission inherits the binding default; a positive value replaces it; explicit `null` clears it. Overrides may raise or lower a default because it is not a restriction on explicit client intent. Reject malformed values and any presence of this field on a non-Messages candidate. Existing model-limit presence semantics remain unchanged.

Resolve model-limit inheritance/narrowing before validating the default. Zero or a default above the effective declared maximum rejects at startup/endpoint compilation and in direct target encoding, even for a request that supplies its own valid ceiling. No default is valid configuration; an uncapped request then rejects locally. A missing maximum supplies neither a default nor evidence of unlimited upstream output. For example, narrowing a maximum to `8192` while inheriting `32000` requires overriding or clearing the default, not taking `min`.

The existing Go/Haiku Messages binding explicitly declares `32000` to preserve its prior missing-cap behavior. This is gateway policy retained from the [pinned client fallback](refs/2026-10-17-generation-contracts.md#messages-missing-ceiling-evidence-2026-10-20), not an upstream default, model maximum, measured optimum or live enforcement claim. Generic `TargetCaps::new` has no default; future bindings select their own values. Other protocols, including Chat for the same model, do not inherit this Messages policy.

Projection order:

1. Validate original source semantics. Native Messages still requires positive `max_tokens` at intake; a target default cannot repair missing/null/zero native input or an invalid explicit source budget/cap relation.
2. Apply existing selected reasoning conversions and carrier projections on a candidate copy. Only then fill an absent effective cap from the trusted default. Preserve every explicit cap, including one larger than the default; never use the model maximum or a thinking preset as a fallback.
3. Validate effective settings and project the typed native request. Pure Profile/IR-to-wire encoding requires a resolved ceiling and rejects absence; it contains no fallback constant or compatibility shim.
4. Apply final model-value and native grammar checks. Retained ordinary manual budgets require `1024 <= B < C`; the candidate-settings check may already reject `B > C`, while native validation rejects equality. Do not clip budgets, raise caps or infer tokens from effort.
5. Successful insertion records `messages.default-max-tokens.v1` exactly once per encoder invocation. Explicit caps record no insertion. Preflight and preparation use the same deterministic encoder; source IR, stored client history and successor candidates remain unchanged. Transport does not resolve defaults.

The ordering preserves budget-to-effort conversion that removes an otherwise conflicting numeric allocation, and checks budgets created by effort-to-budget conversion against the inserted cap. It selects no new mapping or manual-thinking target; Go/Haiku remains adaptive-only. A synthetic target with `B=2048` and `D=1024` rejects without changing a successor target's ability to encode `D=4096`. A retained budget equal to the inserted cap rejects; a larger cap may pass without changing the budget.

Counting does not inherit Generation's required output cap. Its separately selected effective-input projection shares only count-relevant rules, not full generation preparation.

### Owner-local cache admission

The selected `opencode-go-haiku-messages` binding admits existing owner-local cache markers; the generic profile constructor remains closed. The canonical model ceiling includes that capability, but the shared catalog public contract and OpenRouter Chat binding do not inherit it. At configured startup, publish explicit-cache admission only when every selected candidate is an opted-in native Messages target. Mixed routes remain cache-closed instead of admitting requests by skipping an incompatible candidate; membership, order, fallback, affinity and replay identity do not change.

The Messages capability view distinguishes valid owner-local 5m/1h breakpoints from caller-selected request-wide automatic policy. Four-marker bounds, TTL ordering and owner validity still apply; no marker is relocated, dropped or downgraded. The [model and forwarding evidence](refs/2026-10-17-generation-contracts.md#messages-owner-local-cache-admission-2026-10-19) is not proof of a deployed proxy's effective caching, retention or account-compatible signed replay.

### Response rules

Native observations stay unchanged. OpenAI delivery projects a copy:

- Chat requires leading thinking, then text, then calls; it rejects unrepresentable interleaving. Responses retains independent item order.
- Structured tool arguments become exact JSON strings; call/result links remain intact.
- Max-token limits, context-window exhaustion and refusal are incomplete with distinct reasons. Unsupported server-tool continuation is rejected.
- Cache-inclusive usage is computed only from complete reported operands; missing usage is not invented.
- Delivery generates required IDs/timestamps only on its own copy, stable within the response. They are not upstream facts; a stored Responses snapshot uses its gateway ID as a local history key.
- Text, thinking and tool arguments stream incrementally. Full signatures are enveloped after block validation.
- For an eager call ending at `max_tokens`, unfinished object-JSON prefixes retain exact fragments/call identity as `StructuredPartial` and an incomplete item. Native SSE closes the block without inventing a completed/executable value, then publishes only the real incomplete terminal after strict EOF. Duplicate keys, malformed prefixes and byte/depth/node overflow still reject before closure. Successful `tool_use` with invalid input, JSON delivery and request-history replay remain rejected; no argument repair, substitute object, fabricated tool result or automatic retry is selected.

Messages delivery rejects citations until that carrier is implemented. Static and streamed output use the same refusal to discard annotations.

Messages final usage requires reported input/output counts and a known accounting relation. `output_tokens_details.thinking_tokens` maps to the reported reasoning subset in both directions and is never derived. Cache-exclusive input is preserved. Inclusive input needs a reported cache-read count. An unreported cache-write category stays absent rather than zero, and the remaining input keeps any such writes. With both counts reported, at least one must be zero: the IR does not promise that nonzero cache-read and cache-write subsets are disjoint. Missing or ambiguous counts fail projection instead of becoming zero. SSE opening counters are provisional; they never replace the required final report.

## Runtime and trust

### Failover

#### Legal candidates

Startup compiles a public model to a route with legal endpoints and all authorized credential-pool members, at most 64 expanded candidates per entry. Business input cannot create candidates or name an executor. Request representability is checked before selection; it does not widen public admission.

Compiled order is stable. Runtime policy promotes an available conversation preference and defers unavailable targets, then rechecks health atomically before dispatch. Cooling is a dispatch gate, not a last resort. This is failover, not load balancing.

The exact executor/cache identity includes ingress principal, credential provenance, public model, Provider, deployment origin/path and upstream model. API-key rotation changes it; OAuth token refresh preserves the verified principal. A different credential is a different cache domain. Health uses separate private digests so public aliases do not isolate a shared failure.

#### Advancement

Advancement requires enabled policy, an unpublished body, a legal remaining candidate and remaining attempt/time budget. Scan the stable suffix under every earlier failure restriction; an ineligible adjacent key cannot block a later Provider, and another Provider's failure cannot erase an earlier exclusion.

The pool's runtime ceiling applies per endpoint: `max_attempts` with credential fallback enabled, otherwise one. The route ceiling spans all endpoints. Beginning credential resolution consumes an attempt; representability, health and policy skips do not. Neither budget resets or multiplies, and no candidate is revisited. Transport has no hidden retry or redirect. A retained tail member can be the first attempt even when the pool or route ceiling is one.

| Failure | Advancement |
|---|---|
| Transport/connect failure, upstream timeout, server failure | Next key or endpoint where its credential/route policy permits |
| Locally unavailable credential | Next credential of that endpoint where enabled; no new cross-Provider permission |
| Exhausted quota | Next key or another Provider, where enabled |
| Rate limit | Another Provider; do not rotate keys for unknown quota scope |
| Authentication/permission failure, context overflow, request projection failure | Do not advance |
| Safety refusal | Deliver it; never retry on another model |

A non-API 2xx reply, such as HTML or an empty body, is not success. Invalid or truncated upstream intake can advance while unpublished. Downstream projection failures do not acquire retry permission.

Authentication/permission HTTP statuses remain authoritative: error wording cannot reinterpret them as quota, and reading an optional error body cannot turn them into a timeout. For other failed heads, a body-read timeout retains the known status classification.

#### Health and recovery

| Failure | Health domain |
|---|---|
| Authentication, locally unavailable credential | Credential provenance + Provider + trusted origin |
| Permission, quota, upstream server/transport/intake failure | Exact credential + Provider + deployment origin/path + upstream model |
| Unknown-scope rate limit | Configured pool membership + Provider + deployment origin/path + upstream model |

Pool identity is a digest of its ordered-independent member provenance set, not its display name. An unknown rate limit gates that deployment's pool across requests, without inventing a global Provider/account quota scope. Health keys come only from bound candidates; they and the bounded conversation table remain in process memory.

Before `not_before`, no dispatch occurs. At expiry, one foreground request atomically acquires recovery ownership across all affected scopes. The lease lasts through EOF and final handoff; cancellation releases it. No background probes, waiting queues or repeated candidate cycles run. If every otherwise-legal remaining target is cooling or probing, return sanitized HTTP 503 `upstream_unavailable` with the earliest candidate's remaining delay (one second when only a probe is pending). Policy/budget exhaustion instead preserves the last attempted failure.

Standard `Retry-After` accepts seconds or an HTTP date within one day. A valid delay, including zero, is used without a one-hour truncation. Without it, consecutive transient upstream failures back off from one second to one minute; credential, quota and rate failures from one minute to thirty minutes. Failures during an active cooldown do not multiply the backoff; thirty minutes after the previous cooldown resets the sequence. Active deadlines never shorten. No Provider-specific reset dialect is guessed.

Each attempt captures health generations. Successful cleanup and affinity publication require all captured generations still to match; a late success cannot erase a newer failure. Already observed upstream failures survive interrupted error-frame delivery. Slow downstreams, cancellation alone, local projection failures and semantic refusals do not create upstream health failures.

Diagnostics record bounded skip/stop reasons, scope classes, probe ownership and delay sources, never health keys, credential fingerprints, conversation IDs or business content.

#### Routing unit

The declared conversation is the routing unit. Turns, messages, branches and inferred subtasks are not routing keys.

After honest completion, later requests for the same public model and conversation prefer that executor. Successful failover updates affinity without automatically returning to the old target when it recovers. The table holds at most 4,096 pairs for one hour after completion; reads do not extend it. There are no weights, rotations, concurrency lanes or load-spreading rules.

### Conversation and cache affinity

The explicit `X-MorphieCore-Conversation-Id` header takes precedence over agent session headers. See [Usage](usage.md#conversation-and-cache-affinity) for accepted headers and constraints.

The gateway derives Provider-specific cache/session carriers; it does not read Provider session headers as conversation authority. Standard `prompt_cache_key` is a separate cache-grouping intent.

Without a declared conversation, a short-lived prefix-digest index offers best-effort cache-group reuse. It stores no bodies, selects no credential and restores no history. Opaque-reasoning histories are excluded.

Implicit cache scope is isolated by ingress key, credential source and public model. It is independent of the replay fingerprint. Losing the index loses cache hits, not correctness.

### Streaming and delivery

When another candidate could answer, lifecycle-only frames are held before publication. Content, a 64 KiB bound or a real 20-second timer releases held frames, including during upstream silence. Empty buffers produce no invented keepalive or terminal. This lets a pre-publication failure advance without exposing the attempt.

Publishing any body frame freezes advancement. Body handoff acknowledges commit. Completion requires validated upstream EOF and the final handoff.

After publication:

- Never splice another candidate into the body.
- A classified upstream failure may produce a protocol failure frame.
- Other failures abort delivery.
- Never invent a successful terminal or replay the request.

A Responses stream with context overflow or exhausted quota can return HTTP 200 with `response.failed` even before normal output, because Codex reads those classes from stream events.

Upstream streaming and client delivery are independent. JSON clients receive a bounded aggregate; SSE clients receive incremental output. A native stop event is not transport EOF.

Bodies, frames, event counts and deadlines are bounded. Client disconnect, timeout or shutdown cancels upstream work. Held-stream keep-alives remain [deferred](roadmap.md#only-if-needed).

#### Optional delta coalescing

Delta coalescing is implemented and disabled by default. Prepared batches retain shared immutable metadata and fidelity, with projection and replay sealing performed once. The disabled path encodes each batch immediately.

The goal is fewer downstream events, repeated envelopes and body handoffs without changing semantic content. It does not reduce upstream traffic or token usage, and fewer handoffs do not imply proportionally fewer network packets.

```text
upstream decoding and original-event validation/reduction
  → attempt-local intake with terminal withheld until validated EOF
    → streaming: client projection and prepared event context
      → optional bounded delta coalescer → client encoder → body delivery
    → final-only: existing materialized response → client projection → JSON delivery
```

Final-only delivery already supports a declared forced-stream upstream. It uses the existing authoritative reduction and final response, not an infinite coalescing window, an event log or a second full-response accumulator. Unexpected upstream SSE remains subject to the endpoint's declared delivery contract.

##### Ownership and equivalence

| Owner | Responsibility |
|---|---|
| Upstream decoder and semantic reducer | Validate every original event and retain existing byte, event and semantic limits |
| Client projection | Prepare each event once, in order, with stable identity, metadata, fidelity and replay context |
| Delivery-local coalescer | Keep one bounded adjacent run for one attempt; no network, clock source, credential or complete response state |
| Gateway intake | Drive reads, monotonic-time flushes and body handoff in the existing task; retain deadlines, backpressure and cancellation |
| Client encoder and body | Generate downstream sequence numbers, validate target state and preserve publication/acknowledgement gates |

Successful streams must be equivalent after normalizing delta segmentation: ordered content, identities, probabilities, annotations, replay, usage and terminal outcome remain unchanged. Event count, sequence numbers, padding and timing need not match byte-for-byte. Failure paths preserve published prefixes and honest failure; cancellation is not a promise to deliver buffered data.

Preparation and encoding need a separate seam. Do not buffer raw events and later call the existing combined operation against newer attempt metadata or fidelity. Retain the context needed by the pending run, or flush it under its original context before an update. Budget retained context as well as text; avoid copying a full sidecar per delta. Never rerun stateful native projection during flush. Existing target-encoder validation remains in place.

##### Initial merge contract

The first implementation merges only adjacent, nonempty `Delta` fragments of `PartKind::Text`, with the same projected item/part identity, unchanged encoding context and empty logprobs. Concatenate exact strings; never sample or keep only the latest fragment. Other deltas, including reasoning, tool arguments, audio and opaque signatures, pass through unchanged.

Every noneligible event is a flush barrier. This includes resources, annotations, probability snapshots, final text metadata, usage, progress, part/item closure and terminal events. Empty deltas also remain barriers until their observable equivalence is established. Keep only one pending run, not independently buffered queues per part.

The coalescer has empty, pending and closed states. An eligible event starts or extends a pending run; a barrier or size/time threshold emits that run before subsequent events. Cancellation or abandonment discards its state. No state crosses attempts, clients or requests.

##### Scheduling and failure boundaries

1. Preserve the first content-bearing batch's existing publication path without intentional coalescing delay. Do not release held lifecycle frames while hiding that batch in a new buffer. Existing pre-content fallback holding remains a separate policy.
2. Start a fixed flush deadline when a run becomes pending; later fragments do not reset it. An actual timer must flush during upstream silence, and processing multiple events from one network chunk must not starve it. Existing elapsed-time checks on event arrivals are not such a timer.
3. Flush before exceeding the pending-byte limit. Account conservatively for JSON escaping and envelope overhead so merging cannot make individually valid events exceed target frame limits. Original upstream limits are unchanged.
4. Await the existing bounded body handoff. Do not read ahead in a detached task to bypass a slow client. The flush deadline bounds intentional buffering, not downstream/network blocking.
5. Publication forbids advancement; acknowledgement advances the delivery lifecycle. Do not flush merely to manufacture commitment or create a new fallback permission. A failed unpublished attempt loses its buffer before legal advancement; cancellation discards it. On a published classified failure, handle already validated pending content in order before the failure frame when delivery remains possible; malformed intake still cannot create a success terminal.
6. Normal completion drains pending content before terminal encoding, but the terminal itself remains gated by validated upstream EOF. Final-only delivery, stored response snapshots and replay retain their existing authority.

The policy belongs to trusted startup configuration and defaults to disabled. Both finite time and accounted-byte bounds are required; invalid bounds fail startup. Retained context counts against the pending budget, and oversized contexts/fragments pass through rather than being truncated. Reported instruction-message trees bypass buffering. Conservative escaped-text and envelope allowances keep merged frames within the target event ceiling; the encoder still enforces aggregate output budgets.

Configuration is documented in [Usage](usage.md#delta-coalescing). It is not an IR field or a downstream request extension. The [offline baseline](development.md#offline-baseline), [SDK loopback gate](development.md#sdk-loopback-gate) and separately scoped real acceptance remain distinct; synthetic overhead reductions do not establish universal latency or packet-count improvements.

### Credentials and security

Credentials come only from the application's private store. A manager owns API keys, OAuth identities and ordered pools. Inference borrows access for a fixed identity; it never logs in or refreshes.

Ingress accepts one configured token via Bearer or `x-api-key` and authenticates before body collection. Business data cannot select an origin, credential or auth/proxy header. Inbound headers are not passed through.

Transport uses trusted prepared requests, no redirects and no implicit HTTP retries. Environment proxies require explicit startup opt-in. Credentials, locators, private config and upstream bodies never reach clients.

### Observability

Prefer `tracing` spans/events and established OpenTelemetry log, trace and metrics integrations where needed. Reuse ecosystem SDKs, subscribers and exporters with minimal application wiring; do not build a home-grown diagnostics library, event bus or parallel record framework.

Organize observations around real request, client-turn and session boundaries. A client turn may span several tool-loop requests; upstream attempts belong to one request. Correlate turns/sessions only from established caller context, not prompts, timing or response ancestry. A conversation/cache key alone does not prove either boundary. Unknown associations stay unknown; correlation needs neither long-lived session spans nor a new agent/session store. Request instrumentation follows body completion, failure or cancellation, not merely the handler's return.

Attach logs and adaptation summaries to the relevant request/attempt context instead of maintaining disconnected record trees. Use privacy-safe correlation fields, not high-cardinality metric labels. Semantic reports and projection evidence remain authoritative; do not duplicate their owners or add IR fields solely for telemetry.

Keep operational telemetry body-free and preserve business behavior when export fails. [Private exchange capture](#configurable-exchange-capture) remains a separate disclosure contract; adopting OpenTelemetry does not authorize business-content export.

The current bespoke JSONL diagnostics are an interim implementation, not a framework to expand. Instrumentation and migration follow the [acceptance-driven follow-up](roadmap.md#observability-follow-up): add only signals that answer demonstrated investigation needs, not an exhaustive field/event catalogue.

### Configurable exchange capture

P6 selects configurable full business-message capture for this internal gateway, not just field summaries. Keep independent header/body controls for four boundaries: inbound client request, outbound upstream request, inbound upstream response and outbound client response. Body capture includes JSON, streamed application content, tool arguments/results and non-authentication opaque payloads when enabled. Capture is not a source of replay or semantic restoration.

Capture is opt-in and off by default. Use private local storage, restrictive file permissions and finite queue, per-exchange and aggregate retention limits. Record redaction, truncation, dropped records and incomplete streams explicitly; do not label a partial record as complete. Keep the current best-effort availability contract: capture failure does not change the business response. Enabling content capture does not authorize remote export or printing captured business content into tool output.

Credentials are excluded even in full mode. Redact Authorization/Proxy-Authorization, authentication cookies, API keys, token-bearing query values, known authentication body fields and credential material before enqueueing or writing. Configuration cannot disable this step. If a capture unit cannot be sanitized safely, omit it and record the reason. Complete business capture is not byte-for-byte credential-bearing wire capture.

Capture belongs at gateway/transport I/O boundaries; codecs remain pure and only return bounded typed adaptation evidence. Stream incrementally within existing lifecycle bounds, sanitize at safe parsing boundaries and do not buffer the whole response or wait for storage before publication. Associate capture with an exchange-local identity and existing attempt ordinal without changing routing, retry or cancellation.

`gateway/capture.rs` owns queue/retention accounting and observed-body lifetime. Its private `capture/redaction.rs` owns the pure credential-removal policy; sanitization still precedes enqueueing. Body-free diagnostics and opt-in business capture remain separate disclosure contracts.

The startup-owned [capture configuration](usage.md#private-exchange-capture) selects four independent header/body boundaries. Authenticated client traffic and Generation upstream I/O use a private exclusive-create writer. Static upstream JSON borrows the existing intake buffer; SSE uses bounded event framing. Unsafe serialized-argument fragments and URI headers are omitted explicitly, not logged as raw strings. Frozen standalone-task upstream capture remains outside these hooks.

Capture is normalized application evidence, not a wire archive. Source EOF is distinct from semantic completion, body handoff and retained completeness. Queue, exchange and aggregate limits are finite; observed drops and writer failures remain queryable even when storage cannot retain a status record. No capture failure changes request acceptance or permits replay.

## Settled decisions

These records preserve accepted choices and their consequences, grouped by domain. Detailed contracts above remain authoritative; implementation status, remaining work and acceptance belong to the [roadmap](roadmap.md#current-status). A selected decision is not evidence of a working client loop.

### Scope and activation decisions

| Decision | Consequence |
|---|---|
| Five-stage development order | 1. Same-protocol lossless upstream/downstream IR codecs; 2. Basic agent tool interactions; 3. Cross-protocol stability and `count_tokens`; 4. Embedding/audio/image/video APIs and Files/multipart Uploads; 5. Unscheduled citations, tool discovery, hosted tools, server-side compaction, Batch and Realtime. Lossless claims are semantic and scoped; approved loss is not losslessness. Scope and exit conditions live in the [roadmap](roadmap.md#planned-order) |
| Stage-1 baseline and scope (2026-10-19) | Lossless claims and tests use the latest releases checked that day: OpenAI Python 3.35.1, Anthropic Python 1.15.0 and pi 1.3.0, the installed client. The SDK loopback gate locks the same SDK releases. The user added Grok and public OpenAI SIWC Responses verification to DashScope Chat, OpenRouter Responses and OpenCode Go Messages. This does not reschedule real loops or activate bindings |
| Generation design baseline approval (2026-10-20) | User explicitly approved the six-area stage-1/2 baseline at `26b63eb`, closing the [overall design review gate](roadmap.md#generation-design-review-gate) for that scope. Inventory exclusions, real-acceptance limits, the routing freeze and deferred observability remain unchanged. No new binding, broader Provider acceptance or unselected stage-3 design is approved |
| Shared Aliyun billing dialect (2026-10-19) | User selected one [Bailian dialect](#shared-aliyun-billing-dialect) for DashScope and TokenPlan. No billing-plan parser fork; shared-model controls have the same wire meaning. Catalogs, origin/key authority, charging and replay isolation remain distinct |
| Bailian compatible-request boundary (2026-10-19) | User selected existing-IR-only adaptation. Preserve documented native controls exactly; reject unowned/ignored guarantees and independent ceilings that cannot be represented. No cap summation, clipping, JSON repair, budget/effort preset or IR expansion |
| pi Haiku request profile (2026-10-19) | R2 is pi's `anthropic-messages` API with the built-in Haiku 5.5 compat flags. The user's separate pi extension declares that model for the gateway; MorphieCore adds no client-specific code, and OpenCode Go is only the current upstream. Its strict, deferred and eager-input tools, block binding, per-message effort, mid-conversation system messages and tool changes get typed owners and exact native carriers. Missing carriers use the selected named projections in the [request rules](#request-rules); protected controls without an approved fallback, including explicit block-binding `"error"`, still reject. Trusted binding policy enables carriers, and the encoder derives beta headers from the final wire. On 2026-10-19 bounded Go checks accepted all three beta carriers, so the Go/Haiku binding enables them. Strict tools, ordered instructions and tool changes are published under the all-native rule for owner-local cache admission |
| Stage-3 counting (2026-10-20) | User selected upstream-reported counts first, with explicit coarse estimates such as character-based heuristics allowed for inaccurate upstream reports; no gateway tokenizer-based exact counting. The [counting policy](#token-counting-policy) preserves effective-input scope, provenance and separation from inference usage. Responses input-token counting remains a proposed companion surface |
| Stage-4 operation scope | Extend independent Embedding/media paths and implement Files/multipart Uploads with selected storage, authorization and lifecycle contracts. Video is in scope, but exact operations and owners remain to be designed. Do not force standalone operations into Generation or imply Batch, Realtime or new binding activation |
| Stage-5 scheduling | All stage-5 tasks are unscheduled future work, not a delivery commitment after stage 4. Explicit compaction is separate from automatic triggers; Batch and Realtime require their own task/session/job designs. Existing compaction-item replay and stage-4 media job requirements do not wait for those future APIs |
| Tool discovery and native web search (deferred) | Unscheduled stage-5 backlog; implementation designs and per-slice scheduling remain unresolved. Do not block stages 1–4, coverage improvement or the P7 boundary review on them. Preserve existing foundations and reject unsupported semantics; deferral does not claim complete official API coverage. Goals and reopening boundaries stay in the [roadmap](roadmap.md#deferred-tool-discovery-and-web-search) |
| Cache prewarming (deferred) | Client replay warming already has an owner in pi; no gateway-owned background warmer is selected. Defer native cache-only support with priority and scheduling undecided, without blocking first-stage completion, coverage improvement or the P7 boundary review. Native cache resource references remain in scope. Ordinary one-token generation is not native no-output prewarming; preserve existing cache handling without claiming that contract. See the [roadmap](roadmap.md#deferred-cache-prewarming) |
| P7 first reconnection scope (2026-10-19) | Select Qwen3.8 Flash through DashScope Chat and GPT-6 Luna through OpenRouter Responses, one existing binding at a time under the roadmap's bounded contracts and gates. Messages full-loop acceptance and Google model-value guards remain separate. No new binding, route policy or protected-semantic loss is selected |
| R1 closure authorization (2026-10-19) | The user resumed R1a/R1b closure independently of the then-pending overall Generation review, then limited the newly observed report boundary to analysis. The separate 2026-10-20 lifecycle decision selects that report design; acceptance status belongs to the roadmap |
| Responses lifecycle report selection (2026-10-20) | User approved [bounded phase-tagged reports](#lifecycle-settings-and-service-reports), explicit terminal-only JSON/history projection and resumption of R1b implementation/acceptance. Only the evidenced closed transition relation is admitted; that selection did not close the overall review or approve cross-protocol tier drift or routing expansion |
| R2 sequencing (2026-10-19) | The user deferred R1b and resumed R2's existing native pi Haiku profile independently. Its readiness and acceptance did not require the then-unselected Responses lifecycle-report design. No R1b implementation, overall design approval, automatic-cache admission or routing expansion followed from that scheduling change |
| Messages owner-local cache admission (2026-10-19) | The user separately resumed this slice. Enable the existing Go/Haiku binding's owner-local markers and publish admission only for all-native, opted-in configured candidates. Do not inject automatic caching beside explicit markers; preserve the marker-free default. Automatic-policy admission and unrelated frozen features remain closed; strict tools follow the pi Haiku profile row |
| Routing phase boundary | Generation routing functional development is phase-complete and closed. The roadmap owns [controlled acceptance and separate real failover](roadmap.md#controlled-routing-acceptance). Necessary maintenance and evidence-backed acceptance fixes preserve the selected contract; public semantic admission, bindings, credentials, standalone execution and unresolved client reconnection remain unchanged |
| Protocol tree and dialect ownership (2026-10-20) | Protocol code groups by vendor, wire API and responsibility; the vendor name is where the wire API is defined, not which Provider serves it. Dialect is a named deviation inside that API, model capability and adaptation policy are separate, and trusted bindings select them. A shared rule moves to its nearest common owner; protocols keep their real asymmetries instead of gaining placeholder modules |
| OpenAI access paths (2026-10-20) | Retain standard API-key codecs and [public SIWC](#subscription-responses), not the legacy Codex product backend. Removing its dialect and unused carriers does not remove downstream Codex compatibility or activate new model bindings |
| Native carrier availability is not activation | Native `representation_contract()` describes codec carriers; default `semantic_contract()` keeps generic binding admission unchanged. The selected [all-native Go/Haiku contract](#owner-local-cache-admission) is a narrow configured exception, not activation of other carriers. Public/topology checks remain narrower elsewhere until the scheduled integration review |
| Transitional test scope | While IR/codec contracts are incomplete, defer success scenarios that assume unselected completion conversions or binding admission. Keep established local contracts and safety assertions, measure pruning coverage, and rebuild selected integration coverage after the contracts stabilize. A green reduced suite does not establish the deferred client loops |

### Semantic ownership decisions

| Decision | Consequence |
|---|---|
| One semantic core | Gateway and future agent library share the IR; callers own agent behavior. Beyond context edits and response continuation, agent-only machinery joins the IR when the library needs it |
| Official Generation vocabulary | OpenAI, Anthropic and Google define semantic concepts; dialect carriers and approximation policies stay in protocol modules. Semantic expression, codec support and activated bindings are separate claims |
| Two codec responsibilities | Normalize Provider dialects into an official API baseline plus declared extensions before semantic decoding; encode in reverse before serialization. Semantic extensions enter IR; eligible non-semantic fidelity stays scoped outside it |
| Stage-1 report/event ownership (2026-10-19) | User-selected [Responses observations](#responses-item-and-event-observations): output-only bounded actor records, event-only summary interruption/compaction progress, and original error classification. No actor authority, static progress field or event-log service |
| Reported text-container presence (2026-10-20) | Preserve absent/null/object using the existing bounded presence type, without default insertion or a parallel report owner. Request null remains rejected; [reported text configuration](#reported-text-configuration) keeps this repair separate from broad presence redesign |
| Eager partial-input boundary (2026-10-19) | User selected native SSE-only unfinished argument carriage under `max_tokens`, reusing partial arguments and incomplete lifecycle; strict JSON/history and successful-terminal rejection remain. Complete clients may still repair/drop raw fragments, so wire fidelity does not certify client history fidelity |
| Accurate IR with declared lossy projection | Prefer exact carriers, permit approved advisory omissions and behavioral approximations, and reject loss of protected meaning. Preserve the source and record target-local changes; implementation follows the roadmap |
| Equal downstream contracts | Chat Completions, Responses and Messages receive equal consideration for static delivery, streaming and history replay; a missing carrier needs an approved projection or explicit rejection |
| Text-only instruction scope | Request-wide and ordered system/developer instructions admit only plain text. Instruction parts are `Part<Text>`, not resource-capable input parts. Reject media in requests and reported echoes at source decoding; no role movement, silent loss or instruction resource table is selected. Every official instruction carrier, including Messages mid-conversation system messages, is text-only; a shared SDK content union is not evidence of role-specific semantics |
| Ordered instructions | An instruction after conversation content is a separate `ordered_instructions` feature. Chat and Responses carry it natively; Messages represents it as a mid-conversation system message; Google targets reject it. Codec representation does not widen default Messages admission; the reviewed Go/Haiku binding opens it |
| Role-scoped content | IR validation rejects input media in assistant messages and citations or probabilities on user text. Generated assistant media wait for a selected output owner |
| Unowned message fields | Chat participant `name` and non-null Messages `clear_at` are known official fields without IR owners and reject instead of being ignored. Non-null per-message `output_config.effort` on a system message becomes an ordered `ConfigurationUpdate` and rejects on other roles. Chat `store: true` rejects because the gateway provides no completion storage |
| Protected controls and reports without owners | Messages geography/speed/fallback, Responses access programs and misalignment details, Google attribution and unsupported Interactions lifecycle/step-error carriers reject explicitly. Messages block-binding controls now have typed owners and selected carriers/fallbacks; input transformations use validated named omission under the [request rules](#request-rules). Rejection of the remaining gaps is a temporary representation boundary, not complete IR support; see [contract gaps](refs/2026-10-17-generation-contracts.md#protected-semantics-without-complete-owners) |
| Exact non-strict default materialization | Chat/Messages non-strict defaults can become Responses `strict: false` without schema rewriting. Responses normalization defaults cannot become plain tools |
| Client text-history annotations (2026-10-20) | User selected [input-only Responses shorthand](#client-responses-history-shorthand): omitted annotations become an empty list with named insertion evidence. Null, malformed or supplied citations and all upstream snapshots remain strict; no client-lost citations are reconstructed |
| Program caller dependencies | Request calls name preceding program calls and linked results preserve caller identity. Self-describing response snapshots do not relax complete request-history associations |
| Gemini non-content stops | `MALFORMED_FUNCTION_CALL`, `UNEXPECTED_TOOL_CALL`, `TOO_MANY_TOOL_CALLS`, `LANGUAGE`, `OTHER` and unknown finish reasons remain attempt failures. They never become success; pre-publication failover stays available and the frozen routing classification is unchanged. Recitation has its own incomplete reason |
| Chat custom-tool identity | Use the official nested JSON carriers and preserve raw input/grammar. Resolve a result's function/custom kind only from its preceding call ID. The pinned Chat SSE schema has no custom delta carrier: reject rather than invent one or cast custom calls to functions; see [protocol sources](refs/2026-10-16-chat-custom-tools.md) |
| Separate task contracts | Generation, Embeddings, Images, Speech and Transcription are not one universal message type |
| Pure protocol boundaries | Encode from final typed input; no network in codecs or post-encoding JSON patches |
| Functional ownership | Keep root layers; organize internals by domain, without compatibility aliases or plugin frameworks |
| Explicit downstream identity | A closed client protocol and its adapter own decoding and delivery carriers. All Generation clients share the configured candidate route; Messages needs no dummy OpenAI adapter |
| Official intake fields | Decode registered source semantics accurately. Unsupported preferences need selected omission rules; critical content and execution intent cannot masquerade as unknown fields. Optionality alone does not authorize dropping billing, storage or quality controls |
| Reported tool results | Responses carries function/custom results in static output and item snapshots. Missing call references stay unreported; request history still requires matching preceding calls. Chat and Messages reject ordinary tool results as assistant output, not their supported history forms |
| Envelopes as grouping | Ordered message envelopes are the only IR grouping. Chat's call ownership is a view derived in the OpenAI codec, which also enforces Chat-only rules such as no calls on a refusal |
| Schemas as data | The IR keeps any bounded JSON-object schema an official API accepts. Each target checks its selected subset without rewriting constraints. generateContent function parameters require an explicit object root when a schema is present; omitted schemas stay omitted. Google protocols keep distinct structured-output shells; native strict-tool checks do not restrict client parsing. An unnamed output schema gets the fixed name `response` only on OpenAI targets, which require one |
| Strict tool constraints stay protected | Messages strict tools reject unsupported constraints and documented case-sensitive string enum/const fidelity gaps. Native request-wide schema complexity includes output-only schemas and combined strict-tool/output schemas. No SDK constraint-removal helper or implicit default-strict conversion is used |
| Client-owned output caps | Preserve explicit client ceilings. A protocol requiring an otherwise absent ceiling uses the selected named default policy; [Messages request rules](#request-rules) distinguish the current fixed insertion from the accepted target-configured replacement. No model-maximum fallback or automatic budget adjustment is selected |

### Projection and reasoning decisions

| Decision | Consequence |
|---|---|
| Independent activation and scoped effort | Activation belongs to reasoning; a single scoped effort control belongs to generation settings. Response-wide effort is not reasoning-only effort, and off is not a positive level |
| Compatibility before rejection | Keep every representable fact, because retained state makes conversion more stable and extensible. When a supported client cannot carry or return some state, prefer a documented loss over rejecting its requests. Do not invent content, filler or validator-bypass values. Each existing rejection changes only through a selected rule |
| Selected compatibility losses (2026-10-18) | Superseded for block binding on 2026-10-19 by the typed owner in the pi Haiku profile row. Targets without an identity carrier omit `user` and `safety_identifier` under `generation.omit-identity-hints.v1`. Assessment-only safety reports and OpenAI context-window stops follow the named rules above |
| Unified user attribution (2026-10-19, revised) | Permit projection to OpenAI `safety_identifier`, Messages `metadata.user_id` and generateContent `labels.safety_identifier` under [the attribution contract](#end-user-attribution). Explicit safety identity takes precedence during cross-protocol projection; independently supplied source fields remain distinct and same-priority conflicts reject. Preserve same-protocol native fields and keep cache grouping separate. This supersedes the earlier prohibition on user-to-safety projection, not the trust boundary or binding gates; implementation remains pending |
| Phase transfer and return (2026-10-18) | Named non-Responses request omission preserves source history. Chat/Messages delivery requires a phase-capable manifest; phase-only manifests reuse the existing bounded carrier and dependency checks. Responses remains exact. No inferred labels, changed terminals or new binding activation |
| Gemini citation attribution (2026-10-18) | Nonempty `citationMetadata.citationSources` remains rejected: URI/range/licence may be required output evidence even without a search tool. Complete citation owners and downstream carriers remain deferred; neither a loss receipt nor hidden capture substitutes for attribution |
| Explicit same-model level calibration | P1 permits declared directional scope approximation on target copies; level mappings stay model-local. No inferred nearest level, inverse or off-to-on conversion |
| Disabled activation priority | P2 preserves a supported off carrier and names omission of independent unrepresentable response effort; a target without off support still rejects |
| Preset-based thinking allocation | P3 uses six distinct shared budgets from 1024 to 24576; target-specific tables may contain duplicates. Numeric conversion minimizes absolute token distance and chooses the highest level on every tie. Preserve original intent and hard caps; this is explicit lossy quantization, not an inferred qualitative inverse |
| Thinking budget and output ceiling | P3b rejects explicit or conversion-generated budgets above the applicable client output ceiling. Preserve both values; stricter target relations still apply, without automatic reduction or cap increases |
| Messages manual-budget scope (2026-10-19) | Select ordinary manual budgets only for the pending carrier work: at least 1024 tokens and strictly less than the applicable output ceiling, enforced at Messages source/target boundaries. Defer the wider manual-interleaved budget-scope exception and reject requests requiring it; do not add a new budget-scope abstraction for this slice. Shared IR equality does not establish Messages validity. Preserve P3b and no-clipping rules; exact carriage needs a supporting target, and adaptive-only bindings remain unchanged |
| Messages missing output ceiling (2026-10-19; details approved 2026-10-20) | Use the [candidate-local insertion policy](#missing-output-ceilings): omission inherits, positive overrides, `null` clears. Go/Haiku declares `32000`; generic targets and pure codecs have no fallback. Reject contradictory default/maximum declarations before requests and retain final budget/model checks after selected conversions. Preserve explicit caps; no clipping or cap increase |
| Model-effective value limits (2026-10-19; configuration 2026-10-20) | Keep model-specific effort levels and output maxima in [trusted target caps](#model-effective-values), outside protocol-neutral IR and routing policy. Candidate configuration inherits catalog limits and may only narrow them. Check final controls, retained updates and inserted defaults through the shared encoder; preserve selected mappings, reject violations and add no clamping. This activates no binding and does not certify upstream enforcement |
| Bailian output ceiling | Current Qwen3.8 Flash/Max, GLM5.3/Prime and Kimi-K3 Chat bindings use the documented total `max_completion_tokens` on both billing plans. Qwen/Kimi legacy `max_tokens` is reply-only; GLM5.3's legacy cap already includes thinking. Responses total-cap admission is scoped separately. Do not infer these contracts for Coding Plan or unbound models; wire preservation does not prove upstream enforcement |
| Target-effective sampling | P4 preserves supported controls and permits named omission only for declared fixed/ignored behavior; unknown effectiveness is not permission to discard a control |
| Summary granularity and visibility | P8 permits approximate summary granularity while preserving show/hide intent. Do not enable reasoning, fabricate summaries or expose hidden content |
| Role and instruction approximation | P8 permits named developer/system/user mappings on target copies. A trusted Chat instruction-role selector uses developer for supported targets or system for compatibility fallback, before encoding and without error-triggered retry; existing ordered history stays ordered |
| Target-local media detail | P8 prefers exact image/file detail carriers. Omission needs proven target-default equivalence; other approximations need evidence-backed named directional rules. No blanket media-option omission or inferred auto/absence equivalence |
| Model-visible filenames | P8 requires an exact filename carrier, including filename/title mapping, or rejection when a name is present. No omission or automatic text injection; empty and absent remain distinct |
| Reasoning-context defaults | P8 permits omission only for proven equivalent target defaults; otherwise preserve exactly or reject. Do not simulate context controls by deleting messages, required replay or compacted history |
| No default-strict-to-plain conversion | P8 rejects `DefaultStrictAsPlain`. Omitted Responses normalization defaults are not non-strict tools; preserve their source semantics or reject, without removing explicit constraints |
| Predicted-output optimization | D3 selects named omission of Chat prediction after source validation until supported. Record the loss without changing generated text or fabricating usage |
| Registered reasoning-view presence | Absent/null/empty plaintext views make no assertion; nonempty duplicates must agree. Explicit structured parts and replay ownership remain intact |
| Unsupported known request semantics | Chat `moderation`, content-part `prompt_cache_breakpoint` and Messages tool `input_examples` reject before unknown-field filtering. Non-null Chat moderation reports reject in JSON and every SSE chunk, including usage tails; null is inactive. Neither security policy/reports, explicit cache boundaries nor model-visible tool examples may be silently discarded |
| Layered capabilities | Source validity, acceptance through approved adaptation and faithful target support are distinct. Do not advertise lossy acceptance as native support or infer public promises from a union of candidates |
| Declared adaptation | Named candidate-local losses; known fields strict, unknown protocol fields ignored |
| Candidate-local tool errors | Messages admission preserves tool-result error flags regardless of the route. Native targets retain them; OpenAI targets apply a named loss on their private copy |
| Google tool-result truth and order (2026-10-20) | Reject explicit execution reports without an exact carrier; retain independent error flags. Use the native ordered `$ref`/`displayName` projection for generateContent parts, with occurrence-local labels and unchanged media restrictions. This is codec support, not real Provider or client-loop acceptance |
| Interactions status observations (2026-10-20) | Select [final-envelope authority](#google-foreground-intake), not an inferred transition graph or last-update equality policy. Validate identities/continuations before accepting usage. Updates never authorize tools, retry or successful termination; foreground opening, final step/call checks and EOF gates remain strict |
| Interactions sparse lifecycle metadata (2026-10-20) | Allow lifecycle IDs and opening status to be unreported without fabricating them. Validate provided IDs, bind the first real identity, retain it through later omission and reject contradictions. HTTP attempts own stream identity; Started acknowledges native creation, not an inferred `in_progress`. Update/final statuses, call/result identity, final-envelope authority and EOF gates are unchanged. No model fallback, call-signature carrier or service-report admission follows implicitly |
| Interactions partial snapshots (2026-10-20) | Compare supplied lifecycle fields against original streamed owners, retaining empty source ordinals and rejecting new/contradictory content, call facts, signatures and errors. Never republish snapshot content or select snapshot-only buffering. Unproven multipart boundaries remain unsupported; real opening/service-report admission is independent |
| Configurable reasoning parameters | Numeric thinking tables belong to public models; candidate-local conversion direction is explicit. Reasoning-as-text defaults to false and requires opt-in. Configuration cannot alter dialect implementations, infer qualitative mappings or widen semantic admission |
| Target carriers and protected content | Prefer official carriers or declared extensions. Selected role approximations change only the target representation; they do not invent content, assistant/tool speakers or call/result links. Do not drop task content or media merely to make a target accept the request |
| Target-local image restrictions | Formats, encoded sizes, counts and unavailable per-use options are checked or projected at the protocol boundary, not encoded as Provider-specific IR fields |
| Generation controls | Positive top-k, ordered exact stop sequences, independent presence/frequency coefficients and best-effort seeds have separate carriers. Absent, permitted null and empty stop lists mean no control; target limits remain codec-local. Messages codecs preserve sampling while explicit fixed-sampling target policy omits temperature/top-p and rejects top-k. Ordinary manual budgets have a native carrier; adaptive-only bindings keep rejection. Unbound tokenizer-specific logit bias remains rejected |
| Service-policy intent and reports | [Closed policies and separate reported classes](#service-policies-and-reports), not speculative field combinations. Native names and model aliases never establish cross-service equivalence or effective reports; dynamic alias selection remains deferred with routing integration |

### History and cache decisions

| Decision | Consequence |
|---|---|
| Cache storage intent is protected | Prefer exact retention. Only `"24h"` may be omitted under an explicit, default-off trusted target policy after reviewing its default storage; `in_memory` stays protected. No registered target opts in. Do not infer equivalent TTLs or repair malformed/active request cache options |
| Append-only history projection | Earlier content does not change when a later turn is added |
| Scoped opaque replay and visible-history transfer | Preserve compatible replay. Omit complete incompatible reasoning on a target-private copy with a named transfer receipt; preserve visible content and actual call/results, reject partial or damaged compatible state, and never discard required compacted/cached context |
| Selected attachment return contract | Actual part/call replay stays in the IR. Each client encoder emits the bounded opaque manifest in its own carrier before validation. Restore original owner segmentation only when the visible projection and manifest agree; otherwise drop that carrier's part/call replay with a receipt and keep the visible history. A client that discards a whole opaque-only turn loses that turn's replay; the gateway accepts the resulting history. No immutable edit identity or unverified client compatibility claim |
| Replay dependencies | Validate recorded reasoning, part and call obligations before sealing. Bind owner lifecycle as well as content; missing owners/values reject. Partial replay cannot become final through omission or defaulting |
| Prefix-cache ownership | Typed breakpoints and TTL belong to their content or tool-definition owner, including separate inner and outer tool-result boundaries. Automatic caching is a separate setting; absence preserves target policy. Unsupported targets reject rather than substitute cache keys or retention |
| Compacted context is required history | A distinct ordered compaction item preserves opaque content and identity. Incompatible targets reject it rather than apply the foreign-reasoning strip rule |
| Gateway-only history | [In-memory TTL history](#stateful-history) defaults to enabled for OpenAI Responses compatibility; `store: false` opts out. Continuation, retrieval and deletion remain synchronous, without Provider-held state or background execution |
| Stateless Google Interactions | `store: false` and full history; no `previous_interaction_id` or revision header. Signatures keep their native formats |
| Existing cached context | A typed optional reference belongs to `GenerationRequest` as its input base, not settings, an ordered item or advisory cache hints. Preserve resource identity and reported expiry; unreported expiry is not unlimited retention. Boundary records hold provenance and an explicit target clock checks known expiry. Never fetch, expand, renew or silently omit the reference. Default binding admission and public request schemas stay unchanged |

### Routing and security decisions

| Decision | Consequence |
|---|---|
| Stable entry paths | Provider entry paths are concrete unescaped ASCII. Reject encoded dot segments, separators, whitespace/control bytes and URL-escaped syntax before topology compilation; request-path normalization must not change the path used for cache/replay identity |
| Stable routing revision | Bind complete bounded pools; scan legal successors under accumulated restrictions; gate dispatch on generation-fenced cooling with exclusive foreground recovery. Preserve stable order, exact-credential affinity, one exchange budget and no replay after publication |
| Shared model identities across protocols | Compatible API-key, OAuth and native Messages bindings share one public model and ordered route. Endpoint protocols and credential domains stay distinct; narrower candidates reject requests they cannot represent |
| SIWC cross-Provider fallback | A mixed route may contain one SIWC endpoint and advance to another Provider before publication within the route budget. SIWC pools remain single-account with no credential fallback; standalone SIWC routes remain single-attempt with fallback disabled |
| Case-sensitive model labels | Public IDs may preserve upstream ASCII capitalization, but resolve only registered routes. Provider prefixes remain rejected; Provider, endpoint and route IDs keep their lowercase grammar |
| Codec dispatch and execution | Adapters own upstream decoder and downstream encoder dispatch. Execution owns attempt identity, budgets, EOF validation and publication; candidate-transition rules are pure and independent of I/O |
| Dataflow and scoped state | Typed stages have separate semantic, protocol, attempt and delivery authorities; optional stages do not introduce a generic pipeline framework |
| Typed startup activation | One `TaskEntries` input selects Generation and each standalone task explicitly. Empty lists activate nothing; embedding does not discover catalogs, credentials or environment proxies |
| Two-file startup configuration | `gateway.json` owns service settings; `models.json` explicitly selects catalog models and ordered candidates. Candidate-local trusted deployment declarations may override URLs, wire model names and credential pools without changing Provider identity or dialects. All Generation client protocols use the configured route; frozen tasks keep their single catalog binding |
| Failover, not balancing | Legal candidates, bounded advancement and exact-credential affinity |
| Conversation routing | No inferred turn, branch or message routing |
| Separate credential lifecycle | Private file store and ordered pools; no inference-triggered login or refresh |
| Independent CLI paths | Default configuration lives in the home-relative application directory, with credentials in its `credentials` child. CLI credential/proxy overrides beat file settings; omitted credential paths retain their independent home-relative default. File-relative paths resolve beside the declaring file; no legacy discovery or automatic migration |
| Agent-sized bounds | Bodies, IR budgets and deadlines fit whole-history agent sessions and long turns: tens of MiB and one hour per exchange. Upstreams enforce their own maxima |
| Inert, credential-free media | Inline data or URLs only; no gateway fetch or file service, and no client-supplied Provider file references |

### Delivery and evidence decisions

| Decision | Consequence |
|---|---|
| Ecosystem-first observability (2026-10-20) | Prefer `tracing` and OpenTelemetry with minimal custom glue, correlated to real request/turn/session boundaries without inferred agent state. Signal selection and migration follow stage acceptance, not an exhaustive audit checklist; [Observability](#observability) owns the design |
| Required and incidental output evidence | P5 preserves required output facts and allows selected incidental-report loss after validation; request intent never fabricates upstream reports |
| Configurable internal exchange capture | P6 permits opt-in business headers/bodies at all four I/O boundaries with separate controls, finite retention and best-effort writes. Credentials are always redacted; no switch permits credential-bearing logs |
| Public capability and adaptation notice | P7 selects an authenticated capability snapshot distinguishing faithful support, approved adaptation and rejection, plus a conservative possible-adaptation HTTP notice. Actual receipts remain internal; standard response bodies and SSE stay unchanged. Implementation and reconnection remain separately scheduled |
| Bounded text-delta coalescing | Startup-only, disabled-by-default optimization after client projection and before encoding. One pending run retains its prepared context and fixed deadline; invalid bounds fail startup. Final-only clients retain existing materialization; original intake validation and publication safety remain authoritative |
| Scoped source accounting | Retain raw buckets and latest operation/step snapshots independently; derive delivery views without summing snapshots or inferring unknown inclusion relations. Source-default zeros and optional counts remain distinguishable |
| Honest delivery | Publication forbids advancement; success requires validated EOF and handoff |
| Agent-shaped failures | Sanitized error classes match pinned client behavior; refusals are not retries |
| Delivery-local identity | Generated client IDs/timestamps are not upstream facts and remain stable once selected, even if an upstream ID is first reported later. Keep the source fact on its own metadata owner. Responses allocates a gateway-owned ID; only a stored, fully delivered snapshot makes it a history key |
| Terminal stop evidence | Completion cause is separate from outcome and turn progress, including when Google reports STOP alongside function calls. Preserve natural, sequence-matched and ambiguous stop reports without deriving them from sent controls or output text. Chat coarsens and Responses omits causes under the [selected terminal approximations](#selected-request-approximations); Messages requires the stated cause/progress or pending-call evidence, without inventing source reports. Context-window and recitation reasons stay distinct in the IR; OpenAI clients receive them under named rules. Other targets without a carrier reject |
| Safety policy and reported assessment | Request probability-threshold settings belong to `GenerationSettings`; actual input/output assessments belong independently to `GenerationResponse` and report events. Preserve report order and source facts without deriving them from controls, confusing probability with severity or inferring an outcome. Default, disabled filtering and no category blocking remain distinct. Assessment-only reports are omitted under a named rule; blocking reports and unsupported policy carriers reject |

Pinned clients and protocol sources are in [References](references.md). Observed client behavior is an oracle, not permission to weaken security or fabricate semantics.
