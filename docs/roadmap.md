# Roadmap

This page owns direction and current scope. Code and tests define behavior; [Architecture](architecture.md) records design decisions. Remove completed slices here; Git keeps history.

## Current focus

Complete the Generation IR against selected semantics accepted by the official OpenAI, Anthropic and Google APIs. Provider dialects remain codec concerns. Realtime and standalone tasks do not need complete integration for this phase. Each slice must state its goal and exit condition before code changes.

Track semantic expression, codec representation and enabled bindings separately. Preserve accurate IR, prefer exact carriers, and allow explicitly selected advisory losses or behavioral approximations on target-private copies. Reject loss of protected task, execution or security semantics. Client compatibility does not define the semantic vocabulary. Protocol sources are pinned in [References](references.md).

Chat Completions, Responses and Messages are equally important downstream contracts. Assess static delivery, streaming and history replay for all three. Google generateContent and Interactions currently supply upstream request encoding and SSE intake, not Google client endpoints or static intake. This migration does not expand that direction set.

Follow the [IR and codec contract](#ir-and-codec-migration) and preserve its [integration boundary](#integration-handoff). Continue with the remaining [support queue](#support-queue-after-projection-repair). The [architecture contract](architecture.md#accurate-ir-and-declared-loss) is selected; deferred public-surface and reconnection decisions stay in [Pending decisions](#pending-decisions).

For this migration, update existing tests and independent wire expectations only; do not add test functions, suites or variant matrices. Preserve safety assertions. Record uncovered contracts rather than claiming coverage from compilation or a round trip.

Optional delta coalescing remains a separate planned slice. Do not combine it with loss-policy, routing or codec repair work.

### IR contract repairs

Goal: preserve replay dependencies across history edits and client sealing, and align typed tool validation and strictness requirements with codec contracts.

Exit condition: extend existing lowest-owner tests with independent rejection and replay expectations, then pass the offline baseline for each repair. Keep routing frozen and do not activate new controls or bindings.

### Temporary routing freeze

During this IR/codec migration, do not change route construction, candidate membership/order/selection policy, cache affinity, cooling, failover eligibility, attempt budgets, credentials or public model routing. Do not rank targets by projection loss or add a fallback to hide an unsupported conversion.

IR/codec contracts take priority over keeping the full client-to-gateway-to-Provider chain connected. Existing public admission or topology checks may reject the new codec contract; record the exact incompatible boundary and leave integration disconnected rather than weakening IR or changing routing. Changed codec representability may affect an unchanged policy's result; it does not authorize changing that policy.

Work may change semantic requirements, faithful representation checks, target-local projection policy, client/upstream codecs, delivery representation and diagnostics. Necessary call-site/type maintenance is allowed only without route-policy changes. Trusted target policy data may evolve without activating endpoints or changing route membership. If an old public-contract check cannot consume the new distinction without a routing decision, stop at that interface.

P7 keeps routing frozen until the selected Generation IR, codec and protocol-conversion work is complete. Only then reopen routing/integration design; deferred features remain deferred rather than becoming new freeze prerequisites. Reconnection is a later explicitly scheduled integration step, not an automatic effect of finishing a codec slice. It must verify preflight/preparation agreement, public capability publication and the full client loop against the then-selected routing design. No compatibility shim, weakened test or live deployment is required to conceal the temporary gap.

### IR and codec migration

Preserve the [selected request policies](architecture.md#selected-request-approximations), [thinking-budget contract](architecture.md#thinking-budget-quantization), [reported-fact policy](architecture.md#adaptation-evidence) and [capture boundary](architecture.md#configurable-exchange-capture). Apply target-local conversions once, keep source IR unchanged and reject unselected combinations.

Request acceptance, upstream-reported settings and delivered history remain separate. Repeated native reports must agree before normalization or omission. JSON and SSE use the same delivery policy; necessary continuation and EOF-gated terminals remain authoritative. Diagnostics expose body-free typed receipts; optional capture never selects a semantic mapping.

Continue the support queue without reconnecting routing implicitly. Concrete target tables still need model-level evidence; a codec interface or passing synthetic test is not a new public capability promise.

### Verification and integration boundary

For every implementation slice:

1. Update the affected existing semantic, transport and delivery expectations. Keep independently written wire-to-IR and IR-to-wire oracles; a round trip is supplementary. Do not add tests or weaken safety assertions to make a route pass.
2. Run affected offline targets, then the [full offline baseline](development.md#offline-baseline). Compilation, local codec correctness and integrated gateway behavior are separate results.
3. Separate codec regressions from an incompatible frozen integration contract. A failed full baseline remains a failure; identify the exact blocker and continue independent work without repairing routing. Temporary disconnection does not authorize deleting assertions, excluding suites or claiming the baseline passed.
4. Commit/push slices that pass the baseline under the existing project policy. If a disconnected integration prevents that gate, leave publication blocked and report it; this plan does not create a new permission to publish a failing slice.
5. Run the synthetic SDK loopback gate when the relevant codec boundary is connected. Otherwise mark it blocked, not waived or passed. No real client/Provider check or deployment is required for this planning phase.

Phase completion means the selected policies and carriers pass their local gates and unresolved decisions/integration boundaries are explicit. Restoring and accepting the full client loop belongs to a later routing/integration phase, not a hidden exit condition for IR/codec work.

### Integration handoff

Native `representation_contract()` carrier availability is separate from default `semantic_contract()` binding admission. Google structured output/logprobs and Messages strict tools are not enabled by completing their codecs. [Topology compilation](../src/topology/compile.rs) still checks the narrower default contracts; widening a synthetic endpoint for a codec test does not establish a compiled public route.

Remaining constrained-carrier gaps are explicit rejections: Google JSON-object mode, schema references and unselected schema keywords; generateContent optional allowed-tool subsets; Interactions logprobs; and Messages strict string enum/const, references and regex. Google probability intake selects only unambiguous chunk-local text reports; mixed or cumulative reports reject rather than guess ownership. Google reports no token bytes, so the current Responses probability carrier cannot deliver that evidence.

Responses normalization-default strictness cannot become a plain native tool. P8 selects continued rejection of that conversion; pi-shaped requests that omit `strict` therefore do not reach those native bindings. Explicit non-strict tools retain their separate carrier. No new schema-normalization implementation is selected.

P8 policy selection is complete. Native codecs reject uncalibrated media detail/context and filenames without carriers. No concrete cross-scope or numeric conversion is activated by adding the trusted request-policy interface; each binding still needs model-level evidence. Missing evidence means rejection, not an implicit approximation or a reopened policy decision.

Explicit cache retention also rejects targets without an exact carrier. No cross-Provider retention approximation or route-policy change is selected.

Google directions remain upstream request encoding and SSE intake, not static intake or Google client endpoints. Native policy propagation requires a concrete binding and an actually reported carrier; absent reports are not supplied from request controls.

Exchange capture covers authenticated client boundaries and Generation upstream I/O. Frozen standalone-task upstreams are not instrumented. Binary, multipart, fragmented argument carriers and other unsafe units are explicitly incomplete or omitted, never raw fallbacks. Capture EOF and drop counters do not establish semantic completion or durable full retention.

Chat custom tools have static JSON and history carriers, not a pinned custom-call SSE delta carrier. Custom Chat streams reject explicitly; Responses custom streaming remains separate. Codec support does not activate custom tools on a binding or expand its public contract.

### Support queue after projection repair

The remaining queue follows its local repair and contract gates, not successful route reconnection. Before each IR extension, present a minimal counterexample and structural options. An official carrier alone does not activate a binding.

1. Complete explicit cache breakpoints/TTL carriers. Keep each contract separate; unsupported semantics do not become advisory omissions.
2. Add missing generation controls: `top_k`, then stopping sequences with honest stop reporting, then penalties/seed. Treat `logit_bias` token identities as model-dependent; do not forward them blindly across tokenizers.
3. Design execution-policy semantics separately: service tiers, native cache resources/prewarming and safety settings. Preserve native distinctions; equal names do not establish equivalent billing, capacity or safety behavior. `max_tokens: 0` prewarming is not ordinary zero-length generation.

After these non-tool support slices, continue tool discovery and then `web_search` below. Other hosted tools, Messages citations, generated media, multi-candidate execution and file services retain their deferred boundaries.

### Optional delta coalescing

Goal: reduce downstream event/envelope and handoff overhead with an optional lossless text-delta stage. Follow the selected [ownership, merge and scheduling contract](architecture.md#optional-delta-coalescing-planned). Final-only delivery from forced-stream upstreams remains on existing materialization, without a duplicate accumulator.

Implement these slices in order when this delivery work starts:

1. **Separate event preparation from encoding.** Refine `execution/delivery.rs` and adapter projection so each event is projected once and deferred encoding cannot observe future metadata, fidelity or identity. Keep current output behavior with no coalescing.

   Exit condition: existing static/stream/replay contracts pass; extend the lowest-owner test to cover a pending event followed by a context change. No new response reducer or generic stage framework.

2. **Build a minimal end-to-end text coalescer.** Add a small synchronous delivery-local state machine, then drive it from `gateway/generation/intake.rs` with a monotonic timer in the existing task. Use explicit test-owned policy values before public configuration; disabled behavior remains unchanged.

   Exit condition: one synthetic Messages-upstream to Responses-downstream conversation delivers fewer text events with identical materialized content, releases its first content batch without intentional delay, and flushes pending text during upstream silence. Test one mergeable run and one barrier at the lowest owner; avoid per-Provider copies.

3. **Close lifecycle and budget boundaries.** Extend existing contracts for metadata/fidelity barriers, encoded-size expansion, tail drain, slow-client backpressure, cancellation and failure. Preserve original intake limits, attempt isolation and EOF-gated terminals.

   Exit condition: no data crosses attempts, no post-publication replay becomes possible, and malformed/truncated input never ends successfully. A declared forced-SSE upstream still produces one correct final JSON response for `stream:false`, with no coalescing buffer or second accumulator. Check each downstream carrier's affected contract without creating a variant matrix.

4. **Expose bounded opt-in configuration and measure.** Add the disabled-by-default startup policy and finite time/byte validation, then document the implemented fields in Usage. Preserve the existing HTTP request schema. Compare enabled/disabled behavior on fixed synthetic input, reporting events, encoded bytes, handoffs, first-content/update latency and peak pending bytes without bodies.

   Exit condition: omission preserves the existing path; invalid bounds fail startup; the fixture demonstrates reduced downstream overhead with bounded buffering and semantic equivalence. Run the SDK loopback gate as a separate synthetic compatibility check before recommending opt-in use. Real Provider/client acceptance and deployment remain separate work; do not infer universal latency or packet-count improvements.

For each slice, run affected tests before the [full offline baseline](development.md#offline-baseline), review the diff, then commit and push the passing slice. Extend existing tests rather than adding near-copies. Select finite implementation bounds using the synthetic measurements; broad performance tuning, non-text delta merging, live failover and keep-alives are outside this slice.

### Next Generation slices

These tool slices follow the projection-repair gate and the non-tool support queue above. Keep their relative order. This is a plan, not an implementation or acceptance result. Show the minimal counterexample and structural options before changing an IR contract.

1. **Tool discovery and ordered availability updates.** Complete the semantics and codecs for `tool_search`, its call/output items and `additional_tools`, building on existing deferred-loading and namespace types. Tools discovered later must not be retroactively added to earlier history through top-level settings.

   Exit condition: discovery results and availability changes retain order, definitions and references through decode, encode and history replay. Client/server execution remains explicit. This slice models the interaction; it does not add a gateway tool registry, search executor or activate another hosted tool.

2. **First built-in tool: OpenAI `web_search`.** Complete the first end-to-end official built-in tool path through Generation: declaration and supported controls, Provider-executed search observations, reported sources and URL citations, streamed lifecycle and subsequent history. Reuse the existing observation and citation foundations; keep native wire shapes in the OpenAI codec.

   Exit condition: the selected Responses path preserves search operations, reported results, citation associations and honest terminals in JSON and SSE. Targets without a carrier reject explicitly. Select and verify a trusted binding's capability before activation; the gateway does not fetch search URLs or execute searches itself. Other built-in tools remain deferred.

Use the pinned official API sources in [References](references.md). These slices do not reopen generated-media delivery, file services, new Provider/model bindings, Realtime or Embeddings. Real Provider and client acceptance remain separate, explicitly scoped steps.

### Deferred Generation semantics

These are explicit gaps, not claims of complete official API coverage:

- Messages citations (1C) are deferred to Claude Code acceptance; requests and responses that carry them are rejected, not dropped. The [design](#1c-design-messages-citations) records the IR change for when it reopens.
- Messages document `context` stays rejected until its model-visible semantics have an IR owner. Its lack of a carrier on other targets is not a reason to exclude it permanently.
- Generated media beyond the existing Chat audio contract need a selected output, stream and replay design. Standalone media tasks remain frozen.
- Hosted tools other than the selected OpenAI `web_search` path remain deferred. Existing observation types do not imply an admitted end-to-end contract.
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

The coverage-improvement pass starts only after IR completeness is established. Focused contract tests, safety invariants and the offline baseline remain required for every implementation slice; deferring the coverage pass does not defer those checks.

IR completeness does not mean every upstream can carry every modality. Protocol-specific carrier limits and the [pending decisions](#pending-decisions) remain explicit. This phase does not open file services, generated-image delivery or new Provider bindings.

## Planned order

1. **Generation IR completeness.** Complete the remaining non-tool support queue under the temporary routing freeze, then continue tool discovery and `web_search` in that order. Preserve the explicit integration handoff and deferred concepts; a disconnected client loop does not invalidate correct IR/codec work.
2. **Coverage improvement.** Start the coverage pass only after IR completeness. Keep focused regressions and safety invariants throughout implementation.
3. **Failover.** The feature remains in early design. Its priority is undecided, lowest for now; design, implementation improvement and real acceptance follow the second stage.
4. **Other agent acceptance.** Codex, Claude Code and other agents follow failover. Their priority is undecided, lowest for now.

The next Generation slices belong to the first stage. Optional delivery maintenance remains separate. Route reconnection needs an explicitly scheduled boundary review; it does not authorize redesigning failover during this migration. This plan does not start deferred acceptance runs.

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
| Coverage pass | Second stage, only after IR completeness | Independent wire→IR and IR→wire fixtures, synthetic upstreams and chunked SSE intake |
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

P1/P2/P4/P5/P6, P3's six-budget quantization, P3b's rejection of over-ceiling budgets without adjustment, all P8 policies and D3's prediction omission are selected in [Architecture](architecture.md#settled-decisions). P7 keeps routing frozen until the selected IR/codec/protocol-conversion work is complete. Only the following deferred policies remain open. Continue independent slices; ask only when an unresolved choice blocks the affected contract. Unimplemented support or missing target evidence is not a reason to reconsider an already selected policy.

| Question | Unresolved choice and recommendation | Blocks |
|---|---|---|
| P7 follow-up: Public surface and reconnection | Revisit only after the selected IR/codec/protocol-conversion scope is complete. Decide the public native-versus-lossy capability representation and whether/how to warn clients; then schedule reconnection. No false native support claim or per-request policy override is selected | Later route-facing publication and full-chain acceptance, not the current migration |

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
