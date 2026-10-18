# Roadmap

This page owns direction and current scope. Code and tests define behavior; [Architecture](architecture.md) records design decisions. Remove completed slices here; Git keeps history.

## Current status

| Area | State | Next step |
|---|---|---|
| Routing | Functional phase complete under the [stable routing baseline](architecture.md#failover); feature development closed | [Routing acceptance](#routing-acceptance--pending) after the [P7 boundary review](#p7-boundary-review) |
| Generation IR/codec | Selected design implemented locally, including [visible-history transfer](architecture.md#reasoning-replay), part/call replay with client manifests and [scoped accounting](architecture.md#scoped-accounting). Audit coverage authorized on 2026-10-17 | [Human design review](#generation-design-review-gate) |
| Client compatibility | Rejection audit and policy selection complete: [phase transfer/return](architecture.md#message-phase-transfer), guarded extended-retention omission and retained citation rejection | Client-carrier acceptance and target storage evidence during reconnection |
| Integration | Disconnected where codec contracts outgrew frozen admission; the [P7 candidate review](#proposed-first-reconnection-slices) identifies bounded starting paths and blockers | Select the reconnection scope; no implicit admission or binding activation |
| Real acceptance | Current codec work has offline and synthetic evidence only; client and Provider loops are not re-accepted | Routing acceptance, then [deferred acceptance](#deferred-acceptance) |

No reconnection implementation slice is authorized yet. The P7 boundary review has a proposed sequence below; scope/admission selection and the overall design review remain open. Provider-specific reset dialects, persistence and cooldown tuning need separate evidence and approval; no background probes, balancing or multi-round retries are scheduled.

The current priority is stability and meaningful coverage of implemented foundations. Citation/source-use/point owners, additional execution controls/reports, Interactions snapshots and Messages target-policy changes wait until the user resumes that feature work. This changes implementation order, not the known semantic gaps.

Phase completeness covers the selected non-tool scope, not all official Generation semantics; explicit rejection is safer than silent loss but is not completeness. Chat Completions, Responses and Messages are equally important downstream contracts for static delivery, streaming and history replay. Google generateContent and Interactions supply upstream request encoding and SSE intake only.

## Phase rules

### Generation design review gate

**Status: human design review pending; comprehensive audit coverage explicitly authorized on 2026-10-17.** The authorization opens coverage before design approval. It does not approve unresolved designs, certify design closure or reconnect integration. Existing loss policies may be reconsidered with evidence; consequential changes remain explicit user decisions.

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

## Pending decisions

The [settled decisions](architecture.md#settled-decisions) remain the baseline; the audit permits evidence-backed reconsideration of lossy mappings. New semantic or execution-policy structures need a selected design. Continue independent work while a choice blocks only its own contracts.

### Ownership and projection decisions

Minimal counterexamples and pinned sources are in [Generation contract boundaries](refs/2026-10-17-generation-contracts.md).

| Question | Recommendation and exit condition |
|---|---|
| Complete citations and source identity | Select licence/quote/request-source owners and output point anchors, including the changed OpenAI file-citation index contract. Then independently cover request, output, streaming and returned history; retain rejection until represented. Messages citations have a separate [1C design](#1c-design-messages-citations) |
| Execution controls and reports | Select bounded owners for geography, access programs, replay enforcement, safety/review details and Interactions reported service/continuation. Keep protected unsupported carriers rejected; do not add routing or continuation execution implicitly |
| Interactions lifecycle snapshots | Compare supplied steps against streamed owners, including content/signatures/errors, or select snapshot-only materialization. Current rejection prevents contradiction loss but is not complete SSE support |
| Messages model/product defaults | Move sampling omission and required output-cap insertion to explicit trusted target policy. Select a bounded default or require caller input; do not raise the cap automatically to a model maximum |

### Scheduling and product decisions

| Question | Current behavior and recommendation | Blocks |
|---|---|---|
| Binding activation and reconnection (P7 follow-up) | The public capability/notice contract is selected. Complete model-level evidence and preflight/preparation checks, then specify bindings and client loops under the [reconnection sequence](#public-capability-and-reconnection-sequence). Do not reinterpret unsupported completion/safety carriers or reopen P8 policies | Binding activation and full-chain acceptance, not codec-only coverage |
| [Native cache-only prewarming](#deferred-cache-prewarming) | Priority and scheduling undecided; select the operation and result contract when reopened | That slice only |
| [Tool discovery and native `web_search`](#deferred-tool-discovery-and-web-search) | Priority, scheduling and designs undecided; select the IR and execution/replay contracts when reopened | Those slices only |
| [API expansion](api-expansion-design.md) | Scope confirmed, design proposed, nothing scheduled. Record decisions in Architecture and schedule explicit slices here before implementation | Its own slices; not a Generation exit condition |
| File API | Inline data or public URLs only, within the IR budgets (12 MiB decoded per item, 32 MiB per request). Client Provider file IDs and credential-owned URIs reject. Options: a gateway store resolved inline before lowering; upload pinned to one credential; or per-executor upstream upload. The first and last share one IR slot | File services |
| Realtime | No approach or schedule selected. Choose a client scenario and official contract before deciding session ownership, transport, interruption and recovery | Realtime |

## Integration handoff

Native `representation_contract()` carrier availability is separate from default `semantic_contract()` binding admission. Completing a codec activates nothing: [topology compilation](../src/topology/compile.rs) still checks the narrower default contracts, and widening a synthetic endpoint for a test does not establish a public route.

| Boundary | Current treatment | Exit condition when reopened |
|---|---|---|
| Binding admission narrower than codec representation | Keep the narrower public/topology contract | Verify model-effective support, capability publication and preflight/preparation agreement per binding |
| No exact target carrier for protected meaning | Reject, including normalization-default tools on plain native tools and unsupported terminal causes | Only a separately selected carrier or semantic policy resolves it; wider admission cannot |
| Missing evidence for a target-local approximation | Reject uncalibrated media detail/context conversions | Supply target evidence and the approved mapping without changing route order |
| Deferred protocol direction or feature | Keep Google static/client intake and unselected hosted tools out of scope | Reopen through an explicit slice |

Codec carriers whose binding admission stays closed:

- Google structured output and logprobs, Messages strict tools, Messages owner-local cache markers and automatic TTL controls, and Messages [mid-conversation system messages](architecture.md#native-messages-projection) (`ordered_instructions`, declared only by `representation_contract()`; the OpenCode Go binding needs model and proxy evidence).
- generateContent [cached-context references](architecture.md#cached-context-references) with exact identity/expiry and explicit clock checks. Other carriers reject; there is no HTTP field, authorization service or cache lifecycle operation.
- [Safety policies and reports](architecture.md#safety-policy-and-reports) on generateContent. Downstream delivery omits assessment-only reports under a named rule and rejects blocking reports and safety-bearing settings echoes. Requests without a policy may still receive reports; those exchanges are not an established full loop. Vertex severity/method carriers are out of scope.
- [Service policies](architecture.md#service-policies-and-reports). The native generation-only decode helper rejects an execution policy rather than dropping it. Messages service reports need full-message or opening usage; cross-contract classes and late reports without a carrier reject. Absent reports are never supplied from request controls.
- Chat custom tools have static JSON and history carriers but no pinned custom-call SSE delta carrier; custom Chat streams reject.

Explicit constrained-carrier rejections: Google JSON-object mode, schema references and unselected keywords; generateContent optional allowed-tool subsets and implicit object roots for function parameters; Interactions logprobs; Messages strict string enum/const, references and regex; explicit cache retention without an exact carrier or the approved `"24h"` target policy. No registered target selects that policy yet. Google probability intake accepts only unambiguous chunk-local text reports and has no token bytes, so the Responses probability carrier cannot deliver it.

P8 policy selection is complete. Responses normalization-default strictness never becomes a plain native tool, so pi-shaped requests omitting `strict` do not reach those bindings; explicit non-strict tools keep their carrier. Native codecs reject uncalibrated media detail/context and filenames without carriers. No cross-scope or numeric conversion activates without model-level evidence.

Terminal completion causes are independent of turn progress and tool continuation. Chat and Responses use the [selected terminal approximations](architecture.md#selected-request-approximations); Messages rejects ambiguous stops, recitation, unknown completed terminals without pending calls and explicit continuation. Google `STOP` stays a reported fact beside function calls, and Chat delivers it as `stop`. Gateway and SDK smokes retain exact tool-response carriers, not completed cross-protocol answer loops; signed Google history has pinned pi codec coverage, not full-request admission. Content and tool cache owners are not asserted by the [Messages gateway smoke](../tests/gateway/anthropic.rs).

Exchange capture covers authenticated client boundaries and Generation upstream I/O, not frozen standalone tasks. Unsafe units are explicitly incomplete or omitted, never raw fallbacks; capture counters do not establish semantic completion or durable retention.

### Model-name service-policy carriers

Service-policy carriers may be upstream model names rather than a dedicated field. A trusted, evidence-backed binding declaration must associate an alias with the complete policy it represents; never infer a tier from a suffix or accept a client-supplied upstream selector. A different model version, quantization or context contract is not automatically a tier of the same model. The current [endpoint contract](../src/topology/endpoint.rs) fixes the upstream model at binding time; choosing predeclared variants during preparation is an integration design question, not a codec-only string rewrite.

When scheduled, add an independent alias-carrier oracle and an incompatible-policy rejection. Preparation must agree with preflight on the effective deployment, and cache/replay identity must follow the selected wire model. A selected alias proves request projection, not delivered service; record a reported tier only from an authoritative report.

## Planned order

1. **Generation design closure and human review.** Resolve confirmed defects, select the pending decisions and reconcile the contract map.
2. **Coverage improvement**, authorized alongside review, without a per-model or per-Provider matrix.
3. **Integration acceptance.** Reconnect selected exact carriers, then approved adaptations, and perform the routing acceptance.
4. **Other agent acceptance.** Codex, Claude Code and other agents; priority undecided, lowest for now.

The P7 boundary review becomes eligible after first-stage local completion, without waiting for deferred cache and tool slices, and is required before route-facing acceptance. Its analysis may proceed alongside design closure.

### P7 boundary review

Entry condition: the first-stage IR/codec scope passes its local gates, and incompatible carriers and frozen admission boundaries are explicit. A connected end-to-end gateway is not a prerequisite.

The review must:

1. Separate exact capability, approved lossy acceptance and explicit rejection in the proposed public contract.
2. Check preflight/preparation agreement and identify the model-level evidence needed before activating existing codec support.
3. Verify the stable routing contract alongside each candidate's binding evidence; cache or recovery success does not establish representability.
4. Specify the selected client-loop contracts and the offline, SDK loopback and separately scoped real-acceptance gates.

Exit condition: a selected boundary design and an explicit implementation and reconnection schedule. The review changes no routes and promises no conversion.

#### Proposed first reconnection slices

The 2026-10-18 review checked actual catalog targets, pinned client builders, official protocol/model sources and focused offline request probes. Preflight and preparation share encoding; selected first requests agreeing at those boundaries do not establish response delivery or a client loop. The [evidence and counterexamples](refs/2026-10-17-generation-contracts.md#p7-binding-evidence-2026-10-18) remain distinct from live acceptance.

| Order | Candidate contract | Remaining gate |
|---|---|---|
| R1a | Chat client → `qwen3.8-flash` / `dashscope-flash-chat`; text, ordinary non-strict functions, `low` effort, explicit total output cap, complete readable reasoning history | Verify the tool-call/result/final-answer/history loop in JSON and SSE, followed by bounded pi/OpenCode and Provider acceptance. The cap carrier is corrected; other effort aliases and model-effective controls are not newly certified |
| R1b | Responses client → `gpt-6-luna` / `openrouter-responses`; explicit `strict:false`, `low`, no forbidden sampling/parallel controls, full stateless history | Confirm actual summary/opaque reasoning, phase, usage and tool-call history carriers. OpenRouter's model listing and API name do not prove every Responses semantic. Keep retained-history and replay isolation gates |
| R2 | Messages client → `claude-haiku-5-5` / `opencode-go-haiku-messages`; native non-strict tools, caller-supplied cap, supported thinking controls | Select owner-local cache admission for normal pi/OpenCode requests, then establish the Go deployment version and account-compatible signed-thinking replay. Disabling client caching only proves a narrower control case |
| Later | `gemini-3.8-flash` generateContent → Chat/Responses; explicit non-strict tools and supported controls | Select model-effective value guards and finish the selected safety/terminal loop evidence. Interactions lifecycle snapshots and Google → Messages final-answer conversion remain blocked, not assumed equivalent |

First-stage recommendation: R1a/R1b before R2, not a Provider/model matrix. Use one selected binding at a time without changing catalog ordering or activating alternatives implicitly. Do not use DeepSeek Responses as the initial faithful baseline: its documented role, reasoning and parallel-tool behavior needs separate directional policies. Coding Plan's output-cap semantics also need their own evidence; the DashScope fix is not a universal dialect change.

Two decisions remain explicit:

- **Messages cache admission:** permit the existing owner-local markers on the selected native binding only after model/proxy evidence, or keep R2 disconnected. Recommendation: an independent R2 slice, preserving marker ownership and TTL; do not drop markers or open strict tools, cached-context references or ordered instructions with it. Request-wide cache policy and sampling/default-cap placement remain separate.
- **Model-effective value guards:** select a bounded trusted binding policy for allowed effort values and the maximum output ceiling, checked by the shared target encoder after projection. Recommendation: reject unsupported values rather than clamp or approximate; retain protocol-wide carriers and keep this outside semantic IR and route selection. Gemini `minimal` and cap `65537` currently encode despite the model's documented restrictions, so they cannot establish effective support.

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

## Deferred work

### Deferred Generation semantics

These are explicit gaps, not claims of complete official API coverage:

- [Native cache-only prewarming](#deferred-cache-prewarming) and [tool discovery and native `web_search`](#deferred-tool-discovery-and-web-search).
- Messages citations ([1C](#1c-design-messages-citations)) are deferred to Claude Code acceptance; carrying requests and responses reject.
- Messages document `context` rejects until its model-visible semantics have an IR owner.
- Messages turn-scoped system messages (`clear_at`), `tool_addition`/`tool_removal` blocks and per-message `output_config` have no IR owner and reject.
- Generated media beyond Chat audio need a selected output, stream and replay design. Other hosted tools remain deferred; observation types do not imply an end-to-end contract.
- Chat participant `name` has no IR owner and rejects. Chat `store: true` rejects; there are no stored completions.
- Messages-client `top_k` decodes but the selected Messages profile rejects it. Tokenizer-unbound `logit_bias` rejects.
- Gemini finish reasons without content semantics (`MALFORMED_FUNCTION_CALL`, `UNEXPECTED_TOOL_CALL`, `TOO_MANY_TOOL_CALLS`, `LANGUAGE`, `OTHER`) stay attempt failures under the routing freeze.
- Tool-result video has no carrier on any bound target.
- Multi-candidate execution and file services.

### Deferred cache prewarming

Native cache-only prewarming is not a prerequisite for first-stage completion, coverage or P7. Existing cached-context references keep their separate contract.

The [pinned pi cache warmer](https://github.com/earendil-works/pi/blob/4270522473e677ddba7b093caa0064a3df51d459/packages/coding-agent/src/core/cache-warmer.ts) replays a real request with a one-token cap and keeps refresh usage outside history. That is ordinary generation owned by the client strategy, not native cache-only support; no gateway-owned warmer is selected.

When reopened, select the cache-only operation and result contract from pi's strategy and official native carriers. Exit condition: exact codecs preserve the operation, usage and reported cache facts, distinguish prewarming completion from output and reject targets without carriers. `max_tokens: 0` is not a zero-length answer; never replace explicit no-output prewarming with one-token generation.

### Deferred tool discovery and web search

Select IR ownership and execution/replay contracts before implementation and show the minimal counterexample when either slice reopens. Neither blocks first-stage completion, coverage or P7.

- **Tool discovery and ordered availability updates.** Complete `tool_search`, its call/output items and `additional_tools`, building on deferred-loading and namespace types. Later tools must not be retroactively added to earlier history through top-level settings. Exit condition: discovery results and availability changes keep order, definitions and references through decode, encode and replay. No gateway tool registry or search executor.
- **Native OpenAI `web_search`.** Complete the selected built-in path: declaration and controls, Provider-executed observations, reported sources and URL citations, streamed lifecycle and history, reusing observation and citation foundations. Exit condition: the Responses path preserves operations, results, citation associations and honest terminals in JSON and SSE; targets without a carrier reject. The gateway never fetches URLs or searches itself.

Unsupported discovery and search semantics stay rejected, not converted to text or ordinary function calls. These slices do not reopen generated media, file services, new bindings, Realtime or Embeddings.

### 1C design: Messages citations

Counterexample: a request holds two PDF documents with `citations: {"enabled": true}`. The response cites `{"type":"page_location","document_index":1,"document_title":"B","cited_text":"…","start_page_number":2,"end_page_number":3}`, and the client returns that text block with its citation. The IR has page coordinates but cannot hold the per-document flag, the cited text or a reference to the second request document: `Citation.source` resolves only in its owner's resource table.

Protocol facts (pinned SDK; [Citations](https://platform.claude.com/docs/en/build-with-claude/citations) and [Search results](https://platform.claude.com/docs/en/build-with-claude/search-results), read 2026-10-13):

- `document_index` is 0-based over all document blocks in the request, across messages. `search_result_index` is 0-based over all `search_result` blocks, across messages and tool results.
- Character and block ranges are 0-based; pages are 1-based. All ends are exclusive.
- Citations are enabled on all or none of the documents, and likewise for search results. They are incompatible with structured output.
- Messages cites whole text blocks, splitting text at citation boundaries. `cited_text` is not billed.

Proposed 1C-a, documents only:

| Concept | IR change |
|---|---|
| Enable flag | `FileDescription.citations: bool` per use. Messages carries it; other targets reject `true` because it changes the output contract |
| Cited source | `Citation.source` becomes `CitationSource::Resource(ResourceId)` or `CitationSource::RequestDocument(ordinal)`, counting the request's file uses in order |
| Cited text | `Citation.quote: Option<Text>`, kept exactly |
| Location | Existing `SourceCoordinates` text, page and block ranges; the claim is `Whole` |
| Other targets | OpenAI has no request-document citation. Reject an incompatible target rather than drop required evidence |

Exit condition: one hand-written Messages oracle each way for a cited PDF, one streamed `citations_delta`, and an OpenAI target rejecting the flag. A bounded real call must confirm whether documents inside `tool_result` count toward `document_index`.

Later 1C-b: `search_result` input parts and `content` documents of text and image blocks; neither has a carrier outside Messages. `web_search_result_location` belongs to Provider web search. The alternative is to keep 1C rejected: pi never sends citations, and opencode sends them only from part metadata.

## Target clients

pi and opencode are the primary clients and real-client gates. Pinned client behavior is an acceptance oracle alongside the protocol schema: parsing, stream merging, returned history and error classification. See [References](references.md), [pi acceptance](development.md#pi-acceptance) and [opencode acceptance](development.md#opencode-acceptance).

## Deferred acceptance

| Work | Scheduling | Exit condition when reopened |
|---|---|---|
| Non-image multimodal real checks | After IR completeness | One bounded real check per selected audio, video or document carrier, using model-issued history for tool results; no per-model or format matrix |
| Pinned-client image replay | Conditional | If the installed binary differs from the pinned source, run the bounded image tool round trip with a binary built from that revision |
| Real failover | Third stage; priority lowest for now | One bounded run for a model with two credentials or Providers and a reproducible advanceable failure, such as an exhausted key in a scratch store |
| Codex and other agents | Final stage; on hold | Define the client's acceptance contract first; no runs or request sampling before scheduling |
| Claude Code | Final stage; on hold | Sample real request shapes, then verify beta headers/query, token counting and user metadata; implement [1C](#1c-design-messages-citations) if samples use citations. pi Messages acceptance does not prove Claude Code |

The coverage pass covers text, tools, tool errors, reasoning replay, usage, truncation, error classes and media carriers, plus client-facing fragment shapes and absent/null/empty distinctions. It does not authorize a variant matrix.

### Only if needed

- SSE keep-alive comments while initial frames are held.
- Prefix-derived executor affinity when no conversation is declared.
- An opt-in first-content timeout.

## Frozen scope

Maintenance only, when a refactor requires it:

- Standalone Images, Speech, Transcription and Embeddings.
- New Providers and model bindings unless an approved slice reopens them. Alternative bindings for an existing public model belong to failover work; activating a registered binding is not adding one.
- New subscription credential features.
- File services.
- Realtime.

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

Observed vendor stream shapes become named dialect rules only when a bound upstream emits them, such as empty continuation tool names or call IDs, empty finish reasons and tool-call openers. Treat a bound upstream's in-stream error as a classified failure, not normal output. Do not extend frozen media decoders as part of this work.

## Future work

- **Server-side compaction** (priority undecided). Provider-executed compaction beyond receiving and replaying compaction items. Select the trigger policy, bindings and client-visible behavior first; preserve opaque history and reject incompatible replay targets. This does not imply Provider-held state.
- File services, generated-image output with a selected semantic contract, and resumed standalone media work.
- The [API expansion](api-expansion-design.md) surfaces, once scheduled.
- The [independent Embedding path](#independent-embedding-path).

### Independent Embedding path

Direction selected; scheduling undecided. Extend the existing Embedding path independently of Generation, without reopening frozen scope or making it a Generation prerequisite.

```text
client request → Embedding codec → Embedding IR
  → task admission and trusted route → target codec → HTTP execution
  → Embedding IR response → client codec
```

The [task contract](../src/semantic/task/embedding.rs) owns bounded inputs, requested dimensions, vectors, input/result association and usage. The [adapter](../src/adapter/embeddings.rs), [execution path](../src/execution/embeddings.rs) and [topology](../src/topology/embeddings.rs) keep bindings, encoding choices and runtime state outside it. Do not represent embeddings as Generation messages, tool results or stream events.

Routes must preserve embedding-space compatibility; equal dimensions do not prove compatible vectors, so Generation's cross-model failover does not apply without an explicit contract. Chunking, vector indexes and retrieval remain caller responsibilities. Exit condition: one selected official Embedding contract works end to end, with input/result correspondence, dimensions, bounds and cancellation verified, under lean smoke tests.
