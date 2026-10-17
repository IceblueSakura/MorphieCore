# Roadmap

This page owns direction and current scope. Code and tests define behavior; [Architecture](architecture.md) records design decisions. Remove completed slices here; Git keeps history.

## Current focus

**Routing status: functional implementation phase complete; integration and real-client/Provider acceptance pending.** Use the selected [stable routing baseline](architecture.md#failover) for subsequent integration. Feature development is closed for this phase; only necessary maintenance and evidence-backed acceptance fixes belong to the baseline. Offline and synthetic SDK checks do not constitute real acceptance.

The remaining [routing acceptance](#routing-acceptance--pending) does not reopen feature scope. Provider-specific reset dialects, persistence and cooldown tuning need separate evidence and approval; no background probes, balancing or multi-round retries are scheduled.

Complete the selected Generation IR/codec design, then stop for [human review and explicit approval](#generation-design-review-gate) before comprehensive coverage work. Prior local completion does not certify design closure; resolve confirmed ownership or contract defects and keep unsupported boundaries explicit. Provider dialects remain codec concerns. Realtime and standalone tasks do not need complete integration for this phase. Each behavior slice must state its goal and exit condition before code changes.

Phase completeness covers the selected non-tool support scope, not all official Generation semantics. [Cache prewarming](#deferred-cache-prewarming) and [tool discovery and native `web_search`](#deferred-tool-discovery-and-web-search) are deferred, with priority and scheduling undecided. None blocks phase completion, coverage improvement or the P7 boundary review. Unsupported semantics remain explicit gaps, not silent losses or support claims.

Track semantic expression, codec representation and enabled bindings separately. Preserve accurate IR, prefer exact carriers, and allow explicitly selected advisory losses or behavioral approximations on target-private copies. Reject loss of protected task, execution or security semantics. Client compatibility does not define the semantic vocabulary. Protocol sources are pinned in [References](references.md).

Chat Completions, Responses and Messages are equally important downstream contracts. Assess static delivery, streaming and history replay for all three. Google generateContent and Interactions currently supply upstream request encoding and SSE intake, not Google client endpoints or static intake. This migration does not expand that direction set.

Follow the [IR and codec contract](#ir-and-codec-migration) and preserve its [integration boundary](#integration-handoff). The selected [support queue](#support-queue-after-projection-repair) has no remaining feature slices. The [architecture contract](architecture.md#accurate-ir-and-declared-loss) and [public capability/notice contract](architecture.md#public-capability-and-adaptation-notice) are selected; remaining reconnection choices stay in [Pending decisions](#pending-decisions).

Before human approval, preserve existing tests and independent wire expectations. Update them only as necessary for a selected design correction; do not add test functions, suites, variant matrices or conduct a coverage-improvement pass. Record uncovered contracts rather than claiming coverage from compilation or a round trip. Design approval opens a separately scoped coverage stage, not a test-count target or an automatic expansion of integration.

Defer successful integration scenarios whose completion conversion or binding-admission contract is not selected. Remove those scenarios rather than invent a conversion or replace success with an expected gateway error. Keep identity, replay isolation, strict parsing, cancellation, transport bounds and honest-terminal assertions. Orthogonal fixtures should use exact supported carriers. Rebuild the selected integration coverage after IR/codec contracts stabilize, during the coverage stage.

Next, request human review of the selected Generation IR/codec design using the contract map below and the explicit [integration handoff](#integration-handoff). Exit condition: address review feedback and obtain explicit human approval before coverage work. Keep default admission, HTTP schemas and routing unchanged. P7 analysis may proceed in parallel, but neither an agent audit nor its selected public capability contract substitutes for human approval.

### Generation design review gate

**Status: awaiting human design review; approval pending.** Comprehensive coverage work has not started. The review covers the selected Generation scope, not every official API feature or a green full-client loop.

| Review area | Contract and implementation entry points | Question for closure |
|---|---|---|
| Semantic ownership and bounds | [Semantic IR](architecture.md#semantic-ir), [request](../src/semantic/task/generation/request/mod.rs), [response](../src/semantic/task/generation/response.rs), [context](../src/semantic/context.rs) | Do ordered content, resources, requested controls and reported facts have typed owners without runtime state or dangling references? |
| Source parsing and target projection | [Adaptation](architecture.md#adaptation), [reasoning](../src/protocol/reasoning.rs), [OpenAI projection](../src/protocol/openai/lowering/projection.rs), native protocol modules | Are source validation, exact carriers, selected adaptations and rejection separate, with unchanged source IR and directional mappings? |
| Tools and schemas | [Strictness handoff](#integration-handoff), [tool types](../src/semantic/task/generation/tools/mod.rs), [Messages strict schema](../src/protocol/anthropic/strict_schema.rs) | Are explicit strictness, normalization defaults, call/result identity and unsupported schema constraints preserved? |
| Streaming, terminal and reported evidence | [Delivery](architecture.md#streaming-and-delivery), [OpenAI terminal](../src/protocol/openai/terminal.rs), [Messages delivery](../src/protocol/anthropic/client.rs), [Google safety](../src/protocol/google/safety.rs) | Do JSON/SSE agree on protected facts, repeated reports, continuation and honest termination, including explicit unrepresentable conversions? |
| History and replay | [Replay](architecture.md#reasoning-replay), [context transforms](../src/semantic/task/generation/request/context_transform.rs), [Responses storage](architecture.md#stateful-history) | Are content ownership, deployment isolation and required replay preserved separately from TTL history and affinity? |
| Public and integration boundary | [Integration handoff](#integration-handoff), [capability contract](architecture.md#public-capability-and-adaptation-notice), [topology compilation](../src/topology/compile.rs) | Is codec representation distinguished from default admission, model evidence and accepted client loops? |

Review readiness is not approval. After design closure, request human review; address requested changes and wait for explicit approval before strengthening coverage. If review exposes another design defect, keep coverage blocked until its correction is reviewed. Existing regression, safety and publication gates remain in force throughout.

### Temporary routing freeze

The [stable routing baseline](architecture.md#failover) is the selected Generation contract. Codec work must not change membership, stable selection, health scopes, affinity, budgets, credentials or public model routing. Further routing changes need their own selected scope. Do not rank targets by projection loss or add a fallback to hide an unsupported conversion.

The selected [two-file configuration](usage.md#model-configuration) is a narrow exception for explicitly configured existing bindings and their deployment/policy parameters. It does not reopen affinity/cooling algorithms, retry eligibility, public semantic admission, new bindings or frozen-task contracts.

IR/codec contracts take priority over keeping the full client-to-gateway-to-Provider chain connected. Existing public admission or topology checks may reject the new codec contract; record the exact incompatible boundary and leave integration disconnected rather than weakening IR or changing routing. Changed codec representability may affect an unchanged policy's result; it does not authorize changing that policy.

Work may change semantic requirements, faithful representation checks, target-local projection policy, client/upstream codecs, delivery representation and diagnostics. Necessary call-site/type maintenance is allowed only without route-policy changes. Trusted target policy data may evolve without activating endpoints or changing route membership. If an old public-contract check cannot consume the new distinction without a routing decision, stop at that interface.

Reconnection remains a separately scheduled integration step, not an automatic effect of codec or routing completion. It must verify preflight/preparation agreement, public capability publication and the full client loop against the selected routing contract. Deferred features remain deferred. No compatibility shim, weakened assertion or live deployment is required to conceal the temporary gap.

### IR and codec migration

Preserve the [selected request policies](architecture.md#selected-request-approximations), [thinking-budget contract](architecture.md#thinking-budget-quantization), [reported-fact policy](architecture.md#adaptation-evidence) and [capture boundary](architecture.md#configurable-exchange-capture). Apply target-local conversions once, keep source IR unchanged and reject unselected combinations.

Request acceptance, upstream-reported settings and delivered history remain separate. Repeated reports of a bound envelope fact must agree before normalization or omission; ordered safety assessments remain separate observations. JSON and SSE use the same delivery policy; necessary continuation and EOF-gated terminals remain authoritative. Diagnostics expose body-free typed receipts; optional capture never selects a semantic mapping.

Preserve completed local contracts without reconnecting routing implicitly. Concrete target tables still need model-level evidence; a codec interface or passing synthetic test is not a new public capability promise.

### Verification and integration boundary

For every implementation slice:

1. Before human approval, maintain affected existing semantic, transport and delivery expectations only as necessary for the selected correction; defer coverage expansion. Keep independently written wire-to-IR and IR-to-wire oracles; a round trip is supplementary. Do not add tests or weaken safety assertions to make a route pass.
2. Run affected offline targets, then the [full offline baseline](development.md#offline-baseline). Compilation, local codec correctness and integrated gateway behavior are separate results.
3. Separate codec regressions from an incompatible frozen integration contract. A failed full baseline remains a failure; identify the exact blocker and continue independent work without repairing routing. The transitional test scope above permits removing unselected success scenarios, not safety assertions or entire suites. Report the removed coverage; a passing reduced baseline does not establish the deferred integration.
4. Commit/push slices that pass the baseline under the existing project policy. If a disconnected integration prevents that gate, leave publication blocked and report it; this plan does not create a new permission to publish a failing slice.
5. Run the synthetic SDK loopback gate when the relevant codec boundary is connected. Otherwise mark it blocked, not waived or passed. No real client/Provider check or deployment is required for this planning phase.

Phase completion means the selected policies and carriers pass their local gates and unresolved decisions/integration boundaries are explicit. Restoring and accepting the full client loop belongs to a later routing/integration phase, not a hidden exit condition for IR/codec work.

| Gate | Required evidence | Effect |
|---|---|---|
| Advance independent development | The current local contract passes its focused checks; full-baseline failures are classified and new regressions are fixed | Continue the next independent slice without changing frozen routing |
| Start comprehensive coverage | Selected Generation IR/codec design closure and explicit human review approval | Schedule coverage work; neither passing regression nor agent review grants approval or reconnects routes |
| Commit and push | Full offline baseline passes under the project policy, or the user explicitly authorizes a specific exception | A local completion claim does not grant publication; one exception does not change the standing gate |
| Accept integration | The scheduled boundary review, admitted route contracts, full offline baseline and connected SDK loopback gate pass | Only then claim the selected full client loop works; real Provider acceptance remains separate |

### Integration handoff

Native `representation_contract()` carrier availability is separate from default `semantic_contract()` binding admission. Google structured output/logprobs and Messages strict tools are not enabled by completing their codecs. [Topology compilation](../src/topology/compile.rs) still checks the narrower default contracts; widening a synthetic endpoint for a codec test does not establish a compiled public route.

Remaining constrained-carrier gaps are explicit rejections: Google JSON-object mode, schema references and unselected schema keywords; generateContent optional allowed-tool subsets; Interactions logprobs; and Messages strict string enum/const, references and regex. Google probability intake selects only unambiguous chunk-local text reports; mixed or cumulative reports reject rather than guess ownership. Google reports no token bytes, so the current Responses probability carrier cannot deliver that evidence.

generateContent function-parameter schemas require an explicit object root; omitted schemas retain the no-parameters carrier. Do not infer an object constraint from properties, expand references or rewrite scalar/union inputs to fit this target.

Responses normalization-default strictness cannot become a plain native tool. P8 selects continued rejection of that conversion; pi-shaped requests that omit `strict` therefore do not reach those native bindings. Explicit non-strict tools retain their separate carrier. No new schema-normalization implementation is selected.

P8 policy selection is complete. Native codecs reject uncalibrated media detail/context and filenames without carriers. No concrete cross-scope or numeric conversion is activated by adding the trusted request-policy interface; each binding still needs model-level evidence. Missing evidence means rejection, not an implicit approximation or a reopened policy decision.

Explicit cache retention also rejects targets without an exact carrier. No cross-Provider retention approximation or route-policy change is selected.

Owner-local Messages cache markers and separate automatic TTL controls have codec carriers, but default binding admission remains unchanged. New generation-control carriers likewise do not establish model-effective support or public capability promises.

Existing [cached-context references](architecture.md#cached-context-references) have a generateContent request carrier with exact identity/expiry association and explicit clock checks. Other carriers reject; default binding admission remains closed. There is no new HTTP request field, resource authorization service or cache lifecycle operation. This is separate from cache-only prewarming.

[Safety policies and reports](architecture.md#safety-policy-and-reports) have a classic generateContent probability-policy and streamed-assessment contract. Current downstream carriers reject nonempty reports and safety-bearing settings echoes rather than dropping security facts. Requests without an explicit policy may still receive such reports upstream; those exchanges are not an established full client loop. Default admission, HTTP schemas and routing remain unchanged. Vertex severity/method carriers are not part of this contract.

Service-policy codecs follow the [closed policy/report contract](architecture.md#service-policies-and-reports); default binding admission remains unchanged. The native generation-only decode helper rejects an execution policy rather than dropping it; full request envelopes retain it. Messages service reports require full-message or opening usage. Cross-contract classes and late reports without a target carrier reject instead of being reclassified or omitted.

| Boundary | Current treatment | Exit condition when reopened |
|---|---|---|
| Binding admission narrower than codec representation | Keep the narrower public/topology contract; codec support alone activates nothing | Verify model-effective support, public capability publication and preflight/preparation agreement for each selected binding |
| No exact target carrier for protected meaning | Reject, including normalization-default tools on plain native tools and unsupported terminal completion causes | Preserve rejection unless a separately selected carrier or semantic policy resolves the mismatch; widening admission alone cannot do so |
| Missing evidence for a selected target-local approximation | Reject uncalibrated detail/context conversions | Supply concrete target evidence and the approved mapping without changing route ordering |
| Deferred protocol direction or feature | Keep Google static/client intake and unselected hosted-tool carriers out of scope | Reopen through an explicit slice, not as a side effect of reconnection |

Content and tool cache owners remain local codec contracts. Public binding acceptance of those markers is deferred, not asserted by the [Messages gateway smoke](../tests/gateway/anthropic.rs). Its ordinary native tool loop remains covered without asserting owner-cache admission; no production binding is widened and no runtime cache marker is stripped.

Terminal completion causes are independent of turn progress and tool continuation. Messages can report natural completion or a matched stop sequence; Chat and generateContent `STOP` do not distinguish those causes. Chat and Responses delivery use the [selected terminal approximations](architecture.md#selected-request-approximations); Messages rejects ambiguous stops, unknown completed terminals without pending calls and explicit continuation. Completed pending calls can carry Messages `tool_use` without synthesizing a source disposition. Google `STOP` remains a reported fact even when function calls also require continuation, and Chat delivers it as `stop` beside the calls.

Cross-protocol final-answer delivery to OpenAI clients now has selected local codec contracts; Messages ambiguous-stop and recitation delivery remain [rejections](../src/protocol/anthropic/client.rs). Gateway and SDK smokes still retain exact tool-response carriers, not completed Chat-to-Responses or Messages-to-OpenAI answer loops. Google signed tool-call delivery to OpenAI clients also remains uncovered. Rebuilding those success scenarios belongs to the coverage and reconnection stages.

Messages [mid-conversation system messages](architecture.md#native-messages-projection) have client and upstream codec carriers. The Messages default `semantic_contract()` keeps `ordered_instructions` closed; only `representation_contract()` declares it. Activating it on the OpenCode Go binding needs model-level and proxy evidence.

Google directions remain upstream request encoding and SSE intake, not static intake or Google client endpoints. Native policy propagation requires a concrete binding and an actually reported carrier; absent reports are not supplied from request controls.

Exchange capture covers authenticated client boundaries and Generation upstream I/O. Frozen standalone-task upstreams are not instrumented. Binary, multipart, fragmented argument carriers and other unsafe units are explicitly incomplete or omitted, never raw fallbacks. Capture EOF and drop counters do not establish semantic completion or durable full retention.

Chat custom tools have static JSON and history carriers, not a pinned custom-call SSE delta carrier. Custom Chat streams reject explicitly; Responses custom streaming remains separate. Codec support does not activate custom tools on a binding or expand its public contract.

### Support queue after projection repair

The selected non-tool feature queue is locally complete; retain its contracts under regression and carry the explicit integration handoff into coverage improvement and P7. An official carrier alone does not activate a binding. Before a future IR extension, present a minimal counterexample and structural options.

Messages-client `top_k` decodes into the IR and the selected Messages profile rejects it at projection. Tokenizer-unbound `logit_bias` remains rejected. Cache prewarming, tool discovery, native `web_search`, other hosted tools, Messages citations, generated media, multi-candidate execution and file services retain their deferred boundaries.

#### Model-name service-policy carriers

Service-policy carriers may also be upstream model names rather than a dedicated field. Keep canonical model identity and service intent separate from the exact wire name. A trusted, evidence-backed binding declaration must associate a model alias with the complete policy it represents; never infer a tier from a suffix or accept a client-supplied upstream selector. A different model version, quantization or context contract is not automatically a tier of the same canonical model.

When alias selection is scheduled, extend existing tests with an independent alias-carrier oracle and an incompatible-policy rejection. Preparation must agree with preflight on the effective deployment; selecting a model-name carrier must not leave cache/replay identity bound to a different wire model. The selected alias proves request projection, not the service actually delivered: require an authoritative report before recording a reported tier.

The current [endpoint contract](../src/topology/endpoint.rs) fixes the upstream model at binding time. Static model-name overrides are not request-dependent service-policy selection. Choosing between predeclared deployment variants or resolving a trusted alias during preparation remains an integration design question; do not bypass the routing freeze through a codec-only string rewrite.

### Deferred cache prewarming

Native cache-only prewarming is deferred, with priority and scheduling undecided. It is not a prerequisite for first-stage completion, coverage improvement or the P7 boundary review. Existing cached-context references retain their separate local contract.

The [pinned pi cache warmer](https://github.com/earendil-works/pi/blob/4270522473e677ddba7b093caa0064a3df51d459/packages/coding-agent/src/core/cache-warmer.ts) replays a real request with a one-token output cap and keeps refresh usage outside model history. Scheduling, context validity and refresh economics belong to that client strategy; no gateway-owned background warmer is selected. Such requests remain ordinary generation, not evidence of native cache-only support, and a one-token cap does not identify a warming request.

When reopened, first select the cache-only operation and result contract using pi's strategy and official native carriers as separate references. Exit condition: preserve the selected operation, usage and reported cache facts through exact codecs, distinguish prewarming completion from generated output, and reject targets without carriers. `max_tokens: 0` is not an ordinary zero-length answer; do not silently replace explicit no-output prewarming with one-token generation. Preserve existing cache-option handling without claiming a complete cache-only contract.

### Deferred tool discovery and web search

Both slices are deferred, with priority and scheduling undecided. Select their IR ownership and execution/replay contracts before implementation; the goals below do not select an implementation design or require a relative order. They are not prerequisites for first-stage completion, coverage improvement or the P7 boundary review. Show the minimal counterexample and structural options when either slice reopens.

- **Tool discovery and ordered availability updates.** When scheduled, complete the semantics and codecs for `tool_search`, its call/output items and `additional_tools`, building on existing deferred-loading and namespace types. Tools discovered later must not be retroactively added to earlier history through top-level settings.

   Exit condition: discovery results and availability changes retain order, definitions and references through decode, encode and history replay. Client/server execution remains explicit. This slice models the interaction; it does not add a gateway tool registry, search executor or activate another hosted tool.

- **Native OpenAI `web_search`.** When scheduled, complete the selected end-to-end official built-in tool path through Generation: declaration and supported controls, Provider-executed search observations, reported sources and URL citations, streamed lifecycle and subsequent history. Reuse the existing observation and citation foundations; keep native wire shapes in the OpenAI codec.

   Exit condition: the selected Responses path preserves search operations, reported results, citation associations and honest terminals in JSON and SSE. Targets without a carrier reject explicitly. Select and verify a trusted binding's capability before activation; the gateway does not fetch search URLs or execute searches itself. Other built-in tools remain deferred.

Keep existing deferred-loading, namespace, observation and citation foundations under regression; they do not establish either end-to-end contract. Unsupported discovery and native search semantics remain rejected, not converted to plain text or ordinary function calls.

Use the pinned official API sources in [References](references.md). These slices do not reopen generated-media delivery, file services, new Provider/model bindings, Realtime or Embeddings. Real Provider and client acceptance remain separate, explicitly scoped steps.

### Deferred Generation semantics

These are explicit gaps, not claims of complete official API coverage:

- [Native cache-only prewarming](#deferred-cache-prewarming) has undecided priority and scheduling. It is outside this phase's completeness claim; client-driven replay warming does not establish this contract.
- [Tool discovery and native `web_search`](#deferred-tool-discovery-and-web-search) have undecided priority, scheduling and implementation designs. They are outside this phase's completeness claim.
- Messages citations (1C) are deferred to Claude Code acceptance; requests and responses that carry them are rejected, not dropped. The [design](#1c-design-messages-citations) records the IR change for when it reopens.
- Messages document `context` stays rejected until its model-visible semantics have an IR owner. Its lack of a carrier on other targets is not a reason to exclude it permanently.
- Generated media beyond the existing Chat audio contract need a selected output, stream and replay design. Standalone media tasks remain frozen.
- Other hosted tools remain deferred. Existing observation types do not imply an admitted end-to-end contract.
- Messages turn-scoped system messages (`clear_at`), `tool_addition`/`tool_removal` blocks and per-message `output_config` have no IR owner and reject. Tool changes belong to the deferred tool-discovery slice.
- Chat participant `name` has no IR owner and rejects. Chat `store: true` rejects; the gateway provides no stored completions.
- Gemini finish reasons without content semantics (`MALFORMED_FUNCTION_CALL`, `UNEXPECTED_TOOL_CALL`, `TOO_MANY_TOOL_CALLS`, `LANGUAGE`, `OTHER`) stay attempt failures under the routing freeze.
- Tool-result video has no carrier on any bound target: OpenAI and Interactions results have no video part, and Gemini function responses admit only images, PDF and plain text.

### 1C design: Messages citations

Counterexample: a request holds two PDF documents with `citations: {"enabled": true}`. The response cites `{"type":"page_location","document_index":1,"document_title":"B","cited_text":"…","start_page_number":2,"end_page_number":3}`, and the client sends that text block back with its citation next turn. The IR has page coordinates, but it cannot hold the per-document flag or the cited text. Nor can it name the second request document: `Citation.source` resolves only in its owner's resource table, and a response table does not contain request documents.

Protocol facts (pinned SDK; [Citations](https://platform.claude.com/docs/en/build-with-claude/citations) and [Search results](https://platform.claude.com/docs/en/build-with-claude/search-results), read 2026-10-13):

- `document_index` is 0-based over all document blocks in the request, across messages. `search_result_index` is 0-based over all `search_result` blocks, across messages and tool results.
- Character and block ranges are 0-based; pages are 1-based. All ends are exclusive.
- Citations are enabled on all or none of the documents, and likewise for search results. They are incompatible with structured output; the upstream enforces both.
- Messages cites whole text blocks: the response splits text at citation boundaries. `cited_text` is not billed as input or output.

Proposed structure for 1C-a, documents only:

| Concept | IR change |
|---|---|
| Enable flag | `FileDescription.citations: bool` per use. Messages carries it; other targets reject `true`, because it changes the output contract |
| Cited source | `Citation.source` becomes `CitationSource::Resource(ResourceId)` or `CitationSource::RequestDocument(ordinal)`. The ordinal counts the request's file uses in order; append-only history keeps it stable |
| Cited text | `Citation.quote: Option<Text>`, kept exactly |
| Location | Existing `SourceCoordinates` text, page and block ranges; the claim is `Whole` |
| Other targets | OpenAI carriers have no request-document citation. Required citation evidence must survive delivery and history; reject an incompatible target instead of dropping the citation. A named rule alone does not establish permission to omit evidence |

1C-a exit condition: one hand-written Messages oracle each way for a cited PDF (request flag, response page citation, returned history), one streamed `citations_delta`, and an OpenAI target rejecting the flag. A bounded real call must confirm whether documents inside `tool_result` count toward `document_index` before the ordinal rule is final.

Later and separate (1C-b): `search_result` blocks as a new input part with `CitationSource::SearchResult(ordinal)`, and `content` documents made of text and image blocks. Neither has a carrier outside Messages. `web_search_result_location` belongs to Provider web search, not 1C.

The alternative is to keep 1C rejected until Claude Code acceptance: pi never sends citations, and opencode sends them only from part metadata.

The coverage-improvement pass starts only after selected Generation IR/codec design closure and explicit human approval. Existing focused contract checks, safety invariants and the offline baseline remain required for every implementation slice; deferring coverage expansion does not waive regression checks.

IR completeness does not mean every upstream can carry every modality. Protocol-specific carrier limits and the [pending decisions](#pending-decisions) remain explicit. This phase does not open file services, generated-image delivery or new Provider bindings.

## Planned order

1. **Generation design closure and human review.** Resolve confirmed IR/codec design defects, reconcile the contract map and request human review. Stop for explicit approval; agent review and a green baseline do not open coverage work.
2. **Coverage improvement — blocked pending human approval.** After approval, schedule independent wire expectations, codec assertions and safety boundaries without a per-model/Provider matrix. Preserve the explicit integration handoff and deferred concepts; deferred cache and tool slices do not block this stage.
3. **Integration acceptance — pending.** Reconnect selected exact carriers against the phase-complete routing contract, then already-approved adaptations. Perform the routing acceptance below; Provider and full-client acceptance are not implied by offline recovery checks.
4. **Other agent acceptance.** Codex, Claude Code and other agents follow failover. Their priority is undecided, lowest for now.

The [P7 boundary review](#p7-boundary-review) becomes eligible after first-stage local completion, without waiting for the deferred cache and tool slices, and is required before route-facing acceptance. Its analysis may proceed alongside design closure, but is distinct from the human approval gate. Codec-only coverage may proceed independently of route reconnection only after that approval; deferred acceptance runs remain separate.

### Routing acceptance — pending

Select a small set of admitted bindings and exact client carriers before running acceptance. Preserve unresolved protocol-conversion boundaries rather than weakening admission to make a loop pass.

- Verify pi/opencode conversation identity, tool-result/final-answer loops and continued exact-credential affinity on the selected client protocols.
- Use controlled synthetic failures to verify client-visible failover, affinity migration, all-cooling responses, `Retry-After`, cancellation and post-publication termination. Count client retries separately from gateway attempts so combined retry amplification remains visible.
- Use separately bounded real Provider checks to verify applicable failure scopes and reported cache usage. Missing cache usage is unknown, not a miss; offline recovery tests do not establish cache savings or production recovery behavior.

Exit condition: the selected full client loops and recovery scenarios have explicit results, the offline and connected SDK gates remain green, and unverified bindings/protocols remain identified. Follow [real-client and Provider procedures](development.md); this pending stage authorizes no deployment or new routing features by itself.

### P7 boundary review

Entry condition: the selected first-stage IR/codec scope has passed its local contract gates, and incompatible carriers and frozen admission boundaries are explicit. A green end-to-end gateway is not an entry prerequisite for reviewing the boundary it cannot yet cross.

The review must:

1. Separate exact capability, approved lossy acceptance and explicit rejection in the proposed public contract.
2. Check preflight/preparation agreement and identify which model-level evidence is needed before activating existing codec support.
3. Verify the selected stable routing contract alongside the candidate's binding evidence; cache/recovery success does not establish representability or authorize wider admission.
4. Specify the selected client-loop contracts and the offline, SDK loopback and separately scoped real-acceptance gates.

Exit condition: a selected boundary design and an explicit implementation/reconnection schedule. The review itself neither changes routes nor promises that an unrepresentable protocol conversion will become supported. Full-chain coverage remains blocked where its required boundary is unresolved.

#### Public capability and reconnection sequence

The [selected contract](architecture.md#public-capability-and-adaptation-notice) has an [initial partial gateway surface](usage.md#capability-discovery-and-adaptation-notice), with actual receipts internal. It does not establish model-effective request support or full-loop availability. An acceptance boolean cannot distinguish exact verbosity support from the approved `gemini.omit-verbosity.v1` omission; streamed usage may introduce losses after headers. Keep both limitations explicit.

Schedule implementation and reconnection in this order:

1. Establish model-level evidence and preflight/preparation agreement for each selected existing binding; reject uncalibrated values and stop at interfaces that require routing-policy changes.
2. Extend the partial capability view only with independently verified feature/domain declarations. Do not activate new bindings, widen admission implicitly or treat an unlisted case as rejection.
3. Reconnect selected exact-carrier client loops first, then already-approved adapted paths. Verify tool calls, returned results, final answers and replay for JSON and SSE. Incompatible completion/safety carriers remain blocked; neither wider admission nor a notice resolves them.
4. Require the full offline baseline and connected SDK gate for each implementation slice, then separately schedule bounded pi/opencode and Provider acceptance. Preserve the selected routing contract during carrier reconnection.

## Target clients

pi and opencode are the primary clients and real-client gates. Codex, Claude Code and other agent acceptance belong to the final planned stage, with priority undecided and lowest for now.

Pinned client behavior is an acceptance oracle alongside the protocol schema: parsing, stream merging, returned history and error classification. See [References](references.md), [pi acceptance](development.md#pi-acceptance) and [opencode acceptance](development.md#opencode-acceptance).

## Frozen scope

Maintenance only, when a refactor requires it:

- Standalone Images, Speech, Transcription and Embeddings.
- New Providers and model bindings unless an approved slice reopens them. Alternative bindings for an existing public model belong to failover work; activating a registered binding is not adding one.
- New subscription credential features.
- File services.
- Realtime: implementation is undecided; no protocol, transport or session runtime is selected.

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

## Maintenance rules

Observed vendor stream shapes become named dialect rules only when a bound upstream emits them. Examples include empty continuation tool names or call IDs, empty finish reasons, and tool-call openers.

Treat a bound upstream's in-stream error as a classified failure, not normal output. Do not extend frozen media decoders as part of this work.

## Deferred acceptance

| Work | Scheduling | Exit condition when reopened |
|---|---|---|
| Coverage pass | Second stage, only after selected IR/codec design closure and explicit human approval | Independent wire→IR and IR→wire fixtures, synthetic upstreams and chunked SSE intake |
| Non-image multimodal real checks | Deferred acceptance after IR completeness | One bounded real check for each selected audio, video or document carrier, using model-issued history for tool results; no per-model or format matrix |
| Pinned-client image replay | Deferred, conditional acceptance | If the installed binary differs from the pinned source, run the bounded image tool round trip with a binary built from that revision; do not infer it from installed-client acceptance |
| Real failover | Third stage; priority undecided, lowest for now | One bounded real run for a model with two credentials or Providers and a reproducible advanceable failure |
| Codex and other agents | Final stage; priority undecided, lowest for now | Define the selected client's acceptance contract before reopening |
| Claude Code | Final stage; priority undecided, lowest for now | Sample real request shapes, then verify beta headers/query, token counting and user metadata hints; implement [1C](#1c-design-messages-citations) if the samples use citations |

The coverage pass covers text, tools, tool errors, reasoning replay, usage, truncation, error classes and media carriers (user and tool-result image, audio, video and document). It also checks client-facing fragment shapes and meaningful absent/null/empty distinctions. It does not authorize a variant matrix.

Codex, Claude Code and other agent acceptance are on hold until scheduled: no acceptance runs or request sampling before then.

Real failover needs a controlled failure source, such as an exhausted key in a scratch store. Functional tests and pi Messages acceptance do not prove Claude Code acceptance.

### Only if needed

- SSE keep-alive comments while initial frames are held.
- Prefix-derived executor affinity when no conversation is declared.
- An opt-in first-content timeout.

## Pending decisions

P1/P2/P4/P5/P6, P3's six-budget quantization, P3b's rejection of over-ceiling budgets without adjustment, all P8 policies and D3's prediction omission are selected in [Architecture](architecture.md#settled-decisions). P7 keeps routing frozen until the selected IR/codec/protocol-conversion work is complete. New execution-policy structures require selection as their slices start; they do not reopen settled adaptation policies. Continue independent work and ask only when an unresolved choice blocks the affected contract.

| Question | Unresolved choice and recommendation | Blocks |
|---|---|---|
| P7 follow-up: Binding activation and reconnection | The public capability/notice contract is selected. Complete model-level evidence and preflight/preparation checks, then specify selected bindings and client loops under the [reconnection sequence](#public-capability-and-reconnection-sequence). Do not reinterpret unsupported completion/safety carriers or reopen P8 policies | Later binding activation and full-chain acceptance, not the current codec-only coverage work |
| Deferred native cache-only prewarming | Priority and scheduling are undecided. Select the operation and result contract when reopening the [slice](#deferred-cache-prewarming); client replay warming remains separate | Native cache-only implementation only, not native cache resource references, first-stage completion, coverage improvement or the P7 boundary review |
| Deferred tool discovery and native `web_search` | Priority, scheduling and implementation designs are undecided. Select the affected IR and execution/replay contracts when reopening each [slice](#deferred-tool-discovery-and-web-search) | Implementation of those slices only, not first-stage completion, coverage improvement or the P7 boundary review |

| Question | Current behavior | Options |
|---|---|---|
| File API (deferred) | Inline data or public URLs only. Inline media fit the IR budgets (12 MiB decoded per item, 32 MiB of IR per request), with narrower target limits where required. Client Provider file IDs and credential-owned file URIs are rejected; there is no gateway file service | Gateway store resolved to inline before lowering; upload pinned to one credential; or a gateway store with per-executor upstream upload. The first and last share one IR slot |
| Realtime implementation (undecided) | Deferred; no implementation approach or schedule is selected | Select a client scenario and official API contract before deciding session ownership, transport, interruption and recovery. A separate session runtime is an option, not an adopted implementation plan |

## Future work

- **Server-side compaction (priority and scheduling undecided).** Use Provider-executed compaction, beyond receiving and replaying compaction items. Select the trigger policy, supported bindings and client-visible behavior before implementation. Preserve opaque history and reject incompatible replay targets; this does not imply Provider-held conversation state.
- File services.
- Generated-image output with a selected semantic contract and downstream carrier; input support does not authorize this work.
- [Independent Embedding path](#independent-embedding-path), with scheduling undecided.
- Resumed standalone media work.
- Realtime implementation remains a [pending decision](#pending-decisions).

### Independent Embedding path

Direction selected; implementation scheduling is undecided. Extend the existing Embedding path independently of Generation. This plan does not reopen frozen scope or make Embedding completeness a prerequisite for the Generation IR.

```text
client request → Embedding codec → Embedding IR
  → task admission and trusted route → target codec → HTTP execution
  → Embedding IR response → client codec
```

The [task contract](../src/semantic/task/embedding.rs) owns bounded inputs, requested dimensions, vectors, input/result association and usage. The [adapter](../src/adapter/embeddings.rs), [execution path](../src/execution/embeddings.rs) and [topology](../src/topology/embeddings.rs) keep model bindings, encoding choices and runtime state outside that contract. Do not represent embeddings as Generation messages, tool results or stream events.

Add semantics only for selected official API requirements; Provider dialects stay in codecs. Reuse stable value types, credential loading, trusted transport, limits and cancellation without introducing a universal request or session framework.

Routes must preserve embedding-space compatibility. Equal dimensions do not prove that two models produce compatible vectors; do not apply Generation's cross-model failover policy without an explicit compatibility contract. Document chunking, vector indexes and retrieval remain caller responsibilities.

When scheduled, the exit condition is one selected official Embedding contract working end to end through this independent path, with input/result correspondence, dimensions, bounds and cancellation verified. Keep the contract smoke tests lean; no per-Provider matrix.
