# Roadmap

This page owns direction and current scope. Code and tests define behavior; [Architecture](architecture.md) records design decisions. Remove completed slices here; Git keeps history.

## Current focus

Complete the Generation IR against selected semantics accepted by the official OpenAI, Anthropic and Google APIs. Provider dialects remain codec concerns. Realtime and standalone tasks do not need complete integration for this phase. Each slice must state its goal and exit condition before code changes.

Track semantic expression, codec representation and enabled bindings separately. A concept supported by only one official API may belong in the IR; other targets reject it when they have no carrier. Client compatibility does not define the semantic vocabulary. Protocol sources are pinned in [References](references.md).

Chat Completions, Responses and Messages are equally important downstream contracts. Each slice must assess static delivery, streaming and history replay for all three. A protocol used as the semantic reference does not receive higher downstream priority; missing carriers require explicit rejection, not silent loss or invented roles.

Correct existing parameter and content projections before adding missing support. Start with [projection repair](#projection-repair), using the [researched contracts](refs/2026-10-15-generation-projections.md) and the selected [two-responsibility codec boundary](architecture.md#protocol-baselines-and-provider-dialects). Complete the repair gate before the [support queue](#support-queue-after-projection-repair).

Until the IR core is complete, add only minimal regressions for changed contracts and confirmed defects, preferably by extending existing tests. Keep safety invariants and the offline baseline; defer coverage expansion.

An independent delivery-maintenance slice is designed below: [optional delta coalescing](#optional-delta-coalescing). This is an implementation plan, not a claim of shipped functionality or a change to the Generation semantic sequence. It does not reopen deferred failover or real-client acceptance.

### Projection repair

Implement small slices in this order. These are planned corrections, not shipped behavior. Finish independent boundary repairs while the [semantic decisions](#pending-decisions) remain open; do not choose an approximation or IR contract implicitly.

1. **Isolate equivalent usage aliases.** Move DeepSeek Chat cache-hit/miss interpretation out of the unconditional [shared usage reader](../src/protocol/openai/static_response.rs). Select it through the trusted dialect and retain canonical-value conflict and total checks.

   Exit condition: the declared dialect preserves the counts in static and streamed intake; Standard Chat and Responses ignore the unregistered aliases. Conflicting canonical and alias counts still fail where the alias is admitted. Prove that an unknown alias cannot change standard usage or make it fail.

2. **Isolate content extensions and restore registered aliases.** Separate reasoning extension admission from the [Chat message decoder](../src/protocol/openai/chat/mod.rs) and [stream decoder](../src/protocol/openai/events/chat.rs). Preserve the registered MorphieCore reasoning carriers, including standalone plaintext `reasoning`, without coupling alias intake to output spelling.

   Exit condition: request, JSON, SSE and history paths interpret a carrier only under its declared profile. Nonempty duplicate views must agree; specify absent/null/empty handling consistently before coding. Readable text, summary, item order, signatures and owner-local replay keep their distinctions. Standard unknown fields do not become reasoning or parse errors.

3. **Close nested protocol scopes and known content loss.** Stop [Chat function parsing](../src/protocol/openai/function_tools.rs) from reading Responses-only dispatch/output-schema members after reporting them as ignored. Reject active misplaced orchestration semantics and unsupported legacy assistant `function_call` history rather than dropping a call. Keep genuine unknown-field handling unchanged.

   Exit condition: ignored values never affect IR or upstream output. Existing Responses dispatch and registered extensions still work; unsupported Chat custom tools remain explicit rejections, not a newly opened support slice. Resolve D3 before changing `prediction` handling.

4. **Correct existing cache and accounting carriers.** Match optional `prompt_cache_options` members and protocol-specific fields to the pinned official types. Distinguish empty options from active controls without inventing explicit defaults. Correct Standard Chat image-usage projection so an official reported count is not dropped as a vendor extension. Do not broaden Responses usage positions.

   Exit condition: legal partial options survive decode and supported re-encoding without adding defaults; Responses-only controls cannot leak through Chat. Existing cache-key and retention advisory policy remains explicit. Usage mappings preserve reported zero versus absent and reject inconsistent totals. New cache breakpoint/TTL carriers wait for the support queue.

5. **Correct reasoning controls after D1.** Apply the selected [strict representation and same-model calibration policy](architecture.md#reasoning-level-calibration). Validate thinking variants before normalization. Preserve disabled activation, independent effort and numeric budget intent where the selected target has a carrier; never equate hidden summaries with disabled reasoning. Check model restrictions separately from protocol grammar and do not claim a guidance budget is a hard cap.

   Exit condition: the disabled-plus-effort counterexample has the selected representation or an explicit rejection. Off never becomes low/minimal/adaptive and numeric budgets never automatically become levels. Unsupported intent rejects the candidate. Explicit encode/decode level rules apply once for the same model, with independent directions and target validation; no inverse is inferred from a many-to-one map. Sampling admission does not claim effective support solely because an SDK field exists.

6. **Reproduce terminal-projection suspicion independently.** Feed a synthetic Gemini `MAX_TOKENS` finish through [the decoder](../src/protocol/google/stream.rs) and reducer. Inspect the predicted `Progress(Unreported)` rejection before changing lifecycle behavior.

   Exit condition: either an honest incomplete result reaches JSON/SSE without a fabricated success terminal, or the reproduction disproves the suspected defect. Keep terminal correction separate from field-boundary refactoring.

Repair gate: each confirmed defect has a lowest-owner regression, a corrected projection or an explicitly selected rejection/degradation. Assess static delivery, streaming and history for all three downstream contracts. Extend independent wire oracles rather than round trips or variant matrices. Preserve source-IR isolation, bounded expansion, cancellation, replay isolation and EOF-gated publication.

Run affected targets before the [full offline baseline](development.md#offline-baseline), review and commit/push each passing slice. Run the synthetic SDK loopback gate separately at the boundary milestone. Real Provider checks are needed only for a named unresolved remote contract and follow the fixed request/cap/timeout procedure; docs and compilation do not prove live acceptance.

### Support queue after projection repair

Do not start this queue until the repair gate is complete. Before each IR extension, present a minimal counterexample and structural options. An official carrier alone does not activate a binding.

1. Complete existing-IR carriers first: Google structured output, then logprobs; selected Messages strict tools and Chat custom tools/verbosity; explicit cache breakpoints and TTL. Split these into independent contracts rather than a multi-Provider feature batch.
2. Add missing generation controls: `top_k`, then stopping sequences with honest stop reporting, then penalties/seed. Treat `logit_bias` token identities as model-dependent; do not forward them blindly across tokenizers.
3. Design execution-policy semantics separately: service tiers, native cache resources/prewarming and safety settings. Preserve native distinctions; equal names do not establish equivalent billing, capacity or safety behavior. `max_tokens: 0` prewarming is not ordinary zero-length generation.
4. Continue the existing tool-discovery and `web_search` sequence below, preserving their relative order. Other hosted tools, Messages citations, generated media, multi-candidate execution and file services retain their deferred boundaries.

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

These tool slices follow the projection-repair gate and support queue above. Keep their relative order. This is a plan, not an implementation or acceptance result. Show the minimal counterexample and structural options before changing an IR contract.

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
| Other targets | OpenAI carriers have no request-document citation. Response delivery to OpenAI clients drops it through a named rule; history that carries it is rejected by OpenAI targets, like other unrepresentable annotations |

1C-a exit condition: one hand-written Messages oracle each way for a cited PDF (request flag, response page citation, returned history), one streamed `citations_delta`, and an OpenAI target rejecting the flag. A bounded real call must confirm whether documents inside `tool_result` count toward `document_index` before the ordinal rule is final.

Later and separate (1C-b): `search_result` blocks as a new input part with `CitationSource::SearchResult(ordinal)`, and `content` documents made of text and image blocks. Neither has a carrier outside Messages. `web_search_result_location` belongs to Provider web search, not 1C.

The alternative is to keep 1C rejected until Claude Code acceptance: pi never sends citations, and opencode sends them only from part metadata.

The coverage-improvement pass starts only after IR completeness is established. Focused contract tests, safety invariants and the offline baseline remain required for every implementation slice; deferring the coverage pass does not defer those checks.

IR completeness does not mean every upstream can carry every modality. Protocol-specific carrier limits and the [pending decisions](#pending-decisions) remain explicit. This phase does not open file services, generated-image delivery or new Provider bindings.

## Planned order

1. **Generation IR completeness.** Repair existing projections first, complete the support queue, then continue the selected tool slices. Keep deferred concepts explicit rather than claiming full API coverage or expanding frozen tasks.
2. **Coverage improvement.** Start the coverage pass only after IR completeness. Keep focused regressions and safety invariants throughout implementation.
3. **Failover.** The feature remains in early design. Its priority is undecided, lowest for now; design, implementation improvement and real acceptance follow the second stage.
4. **Other agent acceptance.** Codex, Claude Code and other agents follow failover. Their priority is undecided, lowest for now.

The next Generation slices belong to the first stage. The optional delivery-maintenance slice above may proceed independently without changing these stage dependencies. This plan does not start deferred acceptance runs; each later stage stays deferred until scheduled.

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

| Question | Current behavior | Options |
|---|---|---|
| D1: activation and the scope of effort | Messages `thinking: disabled` and response-wide effort can coexist. OpenAI effort concerns reasoning, and Google has no documented independent off-plus-level combination in the selected contracts. A generic boolean alone does not resolve this scope difference | After the [cross-protocol comparison](refs/2026-10-15-generation-projections.md#cross-protocol-activation-and-effort), choose either explicit activation plus scope-aware effort semantics, or explicit rejection of combinations the current IR cannot represent. Do not add a universal on/off field or silently broaden the existing effort's meaning |
| D3: Chat predicted-output optimization | Official `prediction` is currently unregistered and dropped by name; usage counters do not restore the request | Recommended: an explicit named omission of this optimization until supported. Alternative: reject active prediction until a typed carrier is implemented. Neither option constrains generated text to the prediction |
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
