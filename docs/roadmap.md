# Roadmap

This page owns direction and current scope. Code and tests define behavior; [Architecture](architecture.md) records design decisions. Remove completed slices here; Git keeps history.

## Current focus

Complete the Generation IR against selected semantics accepted by the official OpenAI, Anthropic and Google APIs. Provider dialects remain codec concerns. Realtime and standalone tasks do not need complete integration for this phase. Each slice must state its goal and exit condition before code changes.

Track semantic expression, codec representation and enabled bindings separately. Preserve accurate IR, prefer exact carriers, and allow explicitly selected advisory losses or behavioral approximations on target-private copies. Reject loss of protected task, execution or security semantics. Client compatibility does not define the semantic vocabulary. Protocol sources are pinned in [References](references.md).

Chat Completions, Responses and Messages are equally important downstream contracts. Assess static delivery, streaming and history replay for all three. Google generateContent and Interactions currently supply upstream request encoding and SSE intake, not Google client endpoints or static intake. This migration does not expand that direction set.

Follow the [IR and codec migration](#ir-and-codec-migration) below. It combines existing [projection repairs](#projection-repair), a small loss-tolerant codec path and selected missing carriers already expressible in the IR. Broader additions remain in the [support queue](#support-queue-after-projection-repair). The [architecture contract](architecture.md#accurate-ir-and-declared-loss) is selected; concrete unresolved policies stay in [Pending decisions](#pending-decisions).

For this migration, update existing tests and independent wire expectations only; do not add test functions, suites or variant matrices. Preserve safety assertions. Record uncovered contracts rather than claiming coverage from compilation or a round trip.

Optional delta coalescing remains a separate planned slice. Do not combine it with loss-policy, routing or codec repair work.

### Temporary routing freeze

During this IR/codec migration, do not change route construction, candidate membership/order/selection policy, cache affinity, cooling, failover eligibility, attempt budgets, credentials or public model routing. Do not rank targets by projection loss or add a fallback to hide an unsupported conversion.

IR/codec contracts take priority over keeping the full client-to-gateway-to-Provider chain connected. Existing public admission or topology checks may reject the new codec contract; record the exact incompatible boundary and leave integration disconnected rather than weakening IR or changing routing. Changed codec representability may affect an unchanged policy's result; it does not authorize changing that policy.

Work may change semantic requirements, faithful representation checks, target-local projection policy, client/upstream codecs, delivery representation and diagnostics. Necessary call-site/type maintenance is allowed only without route-policy changes. Trusted target policy data may evolve without activating endpoints or changing route membership. If an old public-contract check cannot consume the new distinction without a routing decision, stop at that interface.

Reconnection is a later explicitly scheduled integration step. It must verify preflight/preparation agreement, public capability publication and the full client loop against the then-selected routing design. No compatibility shim, weakened test or live deployment is required to conceal the temporary gap.

### IR and codec migration

This is an implementation plan. Accurate IR, declared loss, verbosity omission and the routing freeze are selected. P1/P2/P4/P5 follow the [recorded architecture decisions](architecture.md#settled-decisions); P3 selects pi-inspired presets with nearest-token/highest-level-tie quantization, and P6 permits configurable business-message capture with mandatory credential redaction. P7/P8/D3 remain pending, together with P3's insufficient-output-room handling.

Use the sequence below. S1 repairs can be split by independent contract. S2 establishes one working codec-level path before broader abstractions. S3/S4 do not wait for P3b's cap-conflict handling or P8's legacy-rule review; no slice depends on reconnecting the routing layer.

| Slice | Dependency | Deliverable |
|---|---|---|
| S1: Source and output integrity | None | Known-field, role, content and terminal defects no longer masquerade as tolerable loss |
| S2: First declared-loss path | Relevant S1 guards | Responses verbosity reaches a Messages projection with explicit omission and unchanged source IR |
| S3: Advisory controls and text carriers | S2 | Per-field text handling across current protocols; exact carriers used before omission |
| S4: Existing-IR constrained carriers | Relevant S1 guards; S2 contract split | Selected schema/tool/output carriers without weakening required semantics |
| S5: Quality-sensitive approximation | S2; declared target data; P3b/P8 where applicable | Selected scope/allocation/sampling policies, not a universal fallback |
| S6: Reports, SSE and history | S2; each selected S3–S5 rule | Source facts stay distinct from delivery losses and retained history |
| S7: Diagnostic and contract closure | S2–S6, excluding explicitly deferred decisions | Bounded end-to-end evidence at codec/delivery boundaries and an explicit integration handoff |

#### S1: Source and output integrity

Goal: preserve the source contract before introducing more lossy projection. Use the detailed [repair work](#projection-repair) below, not a new repository-wide cleanup.

1. Separate official baseline fields from selected dialect aliases and extensions. An ignored field must never also populate the IR. Validate conflicts before normalization.
2. Fix known content/role gaps in [Chat tools](../src/protocol/openai/function_tools.rs), [Chat messages](../src/protocol/openai/chat/mod.rs), [Messages client intake](../src/protocol/anthropic/client.rs) and [native intake](../src/protocol/anthropic/semantic_intake.rs). Preserve or explicitly reject caller/dispatch intent and legacy calls; do not repair invalid roles by regrouping them.
3. Separate source parsing from target-profile restrictions, including Messages strict tools. Decode concepts the IR already supports without claiming that every target can encode them.
4. Reconcile static/SSE content rejection in [Messages delivery](../src/protocol/anthropic/client_events.rs). Reproduce the Google incomplete-progress conflict before fixing it; do not relax the reducer's terminal contract or invent an incomplete cause.

Exit: existing lowest-owner cases retain strict JSON, discriminators, role/call identity, content and terminal truth. Unknown protocol extensions remain inert. Any unreproduced or uncovered path is reported separately from confirmed repairs.

#### S2: First declared-loss path

Goal: implement the smallest useful conversion without changing the meaning stored in the IR: Responses `text.verbosity: low` to a Messages request.

1. Keep `GenerationSettings.text.verbosity` on the source. In the [Messages adapter](../src/protocol/anthropic/adapter.rs), project a private request copy, omit only that unsupported preference and return bounded typed loss evidence.
2. Split source validity, acceptance through approved adaptation and faithful representation in [requirements](../src/semantic/task/generation/requirements.rs), [contracts](../src/semantic/task/generation/contract.rs) and [lowering](../src/lowering/generation.rs). Validate the source first, then the projected target. Do not set native support flags to true merely because an omission is allowed.
3. Start with the existing per-protocol rules and projection records. Unify only the evidence needed by this slice; no generic transformation registry, raw JSON bag or client-selectable policy.
4. Preserve the input and independent target projections. A second target with a verbosity carrier must still see `low`. Do not add prompt text or reinterpret verbosity as effort.

Exit: existing independent wire expectations show accurate source decoding, omission from Messages output and the named input/output control receipt. Schema, tools, activation and output caps remain unchanged. A direct codec/adapter path is sufficient; if existing public admission blocks the gateway path, record that gap instead of editing route policy.

#### S3: Advisory controls and text carriers

Goal: classify fields individually rather than reject a whole options container.

1. Replace whole-`TextOptions` rejection in [generateContent](../src/protocol/google/request.rs) and [Interactions](../src/protocol/interactions/request.rs) with field-specific handling. Allow the selected verbosity omission without erasing `format` or a required schema.
2. Implement the standard Chat verbosity carrier in [Chat encoding](../src/protocol/openai/chat/mod.rs) and [OpenAI lowering](../src/protocol/openai/lowering/mod.rs). Keep model eligibility separate from protocol grammar.
3. Normalize empty/plain-text/inactive values only when the source and target semantics establish equivalence. Absence, null and defaults are not globally interchangeable.
4. Inventory existing advisory cache losses and schema-label changes; make their evidence complete. Keep explicit cache modes, storage, prewarm, service tiers and other execution controls outside the advisory bucket. Handle prediction only after D3.

Exit: supported fields survive exact encoding, selected unsupported preferences produce named loss, and unsupported constrained output still rejects. Standard carriers are not treated as dialect-only extensions. Malformed known fields remain invalid even if a target would omit them.

#### S4: Existing-IR constrained carriers

Goal: fix missing representations before considering degradation of required output.

1. Add selected Google structured-output carriers and verify their schema restrictions without rewriting user schemas. Treat generateContent and Interactions as separate protocols.
2. Complete eligible Messages strict-tool representation after separating client parsing from profile admission. Audit `DefaultStrictAsPlain` under P8 rather than treating the existing rule as proof of acceptable quality.
3. Complete selected existing-IR logprob and allowed-tool carriers where the official target supports them. A declaration in an SDK does not activate every model or binding.
4. Keep parameter schemas, required tool selection, explicit strictness, parallel prohibition, arguments, call/result links and necessary media protected. Missing target implementations remain explicit gaps until their carrier is completed.

Exit: existing independent request and response expectations preserve the relevant constraint. Unsupported targets reject that constraint rather than remove it; official protocol support and activated model support remain separate claims.

#### S5: Quality-sensitive approximation

Goal: implement the selected approximations without losing the source's meaning. Concrete target tables must still be declared and validated; do not treat an approved policy direction as automatic support on every model.

1. Keep activation, scoped effort, budget, mode, display and verbosity separate. Implement selected cross-scope conversion on a private target copy, not by relabeling source IR or reusing a same-scope table as an implicit conversion.
2. Implement P1 as trusted directional cross-scope approximation with named loss. Keep qualitative model-local mappings explicit, including identities; undeclared entries reject. Do not infer qualitative nearest levels, inverses or policies from upstream errors. P3's numeric quantization is a separate selected rule.
3. Implement P2 by preserving a supported off carrier and omitting only independent unrepresentable response effort. Under P4, retain supported sampling and name omission only for declared fixed/ignored controls; unknown model behavior remains a gap.
4. Implement P3's [complete preset and matching rule](architecture.md#thinking-budget-quantization). Preserve original numeric intent, accept duplicate token values, minimize absolute distance and choose the highest level on all ties. Keep off/sentinels outside matching and exact native carriers ahead of approximation. Update existing expectations with the approved `750 -> xhigh` example and shared-preset boundaries. P3b blocks only an adjustment when a preset cannot fit the output cap, not the lookup/quantization work.
5. Review existing summary/context/display, media-option and instruction-authority losses under P8. Hiding reasoning must not become disclosure; a summary display preference must not invent activation.

Exit: selected rules run once, report source/target controls and preserve unchanged input. Unselected combinations retain explicit rejection. Native request/intake wiring and [configuration-history policy](#reasoning-repair-slices) follow the selected directions rather than inferring effective settings.

#### S6: Reports, SSE and history

Goal: make response-side tolerance independent of request-control admission without weakening source or delivery truth.

1. Validate upstream reports and raw repeated snapshots before normalization or omission. Fix [reported-settings observation](../src/protocol/openai/events/decode.rs) so many-to-one mappings cannot hide conflicts.
2. Apply P5 independently from request-control admission: preserve output evidence required by request controls, source contract or continuation, and allow named loss of selected incidental reports after validation. Missing request flags alone do not establish that an annotation is optional. Never reconstruct actual effort, usage or tool execution from request settings.
3. Apply the same selected projection to static encoding and incremental events. Reject unsupported content before publishing it when known; later discoveries terminate honestly, without replaying the request after publication.
4. Keep [delivery snapshots](../src/execution/delivery.rs), [client encoding](../src/adapter/downstream.rs) and [stored history](../src/gateway/generation/history.rs) aligned. Source input remains canonical; retained response history reflects the delivered projection. Fidelity cannot resurrect a dropped field or owner.
5. Distinguish control loss from ordered configuration history. Keep non-identity history mappings rejected until source-snapshot validation and the selected policy are complete.

Exit: JSON and materialized SSE agree for each affected contract; generated content, call links, required replay, usage relations and EOF-gated terminals remain honest. No second reducer or whole-stream buffer is introduced to make projection easier.

#### S7: Diagnostic and contract closure

Goal: explain each selected loss without making routing or a logging backend part of codec semantics.

1. Extend [diagnostics](../src/gateway/diagnostics.rs) using the typed evidence introduced in S2. Distinguish source intent, sent controls, upstream reports and delivered facts. Include fixed rule, field category, direction, action and bounded control summaries without embedding bodies in those receipts.
2. Propagate request, response and event losses; do not log only failures. Aggregate repeated SSE evidence and keep attempt attribution without changing candidate selection. Treat an unavailable gateway path as an integration gap, not proof that evidence is durably recorded.
3. Implement P6 [exchange capture](architecture.md#configurable-exchange-capture) outside pure codecs, with independent header/body controls for client request, upstream request, upstream response and client response. Business content may be complete when enabled, but authentication material must be redacted before enqueue/write, including in headers, queries and known credential-bearing body fields.
4. Keep capture opt-in, private, bounded and best-effort. Stream incrementally; record redacted, incomplete, truncated or dropped capture instead of claiming full retention. Extend existing diagnostic assertions for disabled capture, enabled synthetic content and non-optional credential redaction. Logs are not fixtures, replay inputs or tool output.
5. Audit capability descriptions against actual faithful carriers and approved acceptance. P7 governs publication/reconnection; do not change existing routing to satisfy the new contract.

Exit: connected codec/delivery boundaries expose bounded loss evidence; selected I/O capture retains configured business content with correct attribution and credential redaction. Disabled/overflow/error paths do not change request semantics. Remaining route-facing gaps are explicitly listed; no completion claim requires a live client-to-Provider loop.

### Verification and integration boundary

For every implementation slice:

1. Update the affected existing semantic, transport and delivery expectations. Keep independently written wire-to-IR and IR-to-wire oracles; a round trip is supplementary. Do not add tests or weaken safety assertions to make a route pass.
2. Run affected offline targets, then the [full offline baseline](development.md#offline-baseline). Compilation, local codec correctness and integrated gateway behavior are separate results.
3. Separate codec regressions from an incompatible frozen integration contract. A failed full baseline remains a failure; identify the exact blocker and continue independent work without repairing routing. Temporary disconnection does not authorize deleting assertions, excluding suites or claiming the baseline passed.
4. Commit/push slices that pass the baseline under the existing project policy. If a disconnected integration prevents that gate, leave publication blocked and report it; this plan does not create a new permission to publish a failing slice.
5. Run the synthetic SDK loopback gate when the relevant codec boundary is connected. Otherwise mark it blocked, not waived or passed. No real client/Provider check or deployment is required for this planning phase.

Phase completion means the selected policies and carriers pass their local gates and unresolved decisions/integration boundaries are explicit. Restoring and accepting the full client loop belongs to a later routing/integration phase, not a hidden exit condition for IR/codec work.

### Projection repair

These are the detailed repair obligations consumed by S1–S6, not a second competing schedule. Finish independent boundary repairs while [semantic decisions](#pending-decisions) remain open; do not choose an approximation or IR contract implicitly.

1. **Isolate equivalent usage aliases.** Move DeepSeek Chat cache-hit/miss interpretation out of the unconditional [shared usage reader](../src/protocol/openai/static_response.rs). Select it through the trusted dialect and retain canonical-value conflict and total checks.

   Exit condition: the declared dialect preserves the counts in static and streamed intake; Standard Chat and Responses ignore the unregistered aliases. Conflicting canonical and alias counts still fail where the alias is admitted. Prove that an unknown alias cannot change standard usage or make it fail.

2. **Isolate content extensions and restore registered aliases.** Separate reasoning extension admission from the [Chat message decoder](../src/protocol/openai/chat/mod.rs) and [stream decoder](../src/protocol/openai/events/chat.rs). Preserve the registered MorphieCore reasoning carriers, including standalone plaintext `reasoning`, without coupling alias intake to output spelling.

   Exit condition: request, JSON, SSE and history paths interpret a carrier only under its declared profile. Nonempty duplicate views must agree; specify absent/null/empty handling consistently before coding. Readable text, summary, item order, signatures and owner-local replay keep their distinctions. Standard unknown fields do not become reasoning or parse errors.

3. **Close nested protocol scopes and known content loss.** Stop [Chat function parsing](../src/protocol/openai/function_tools.rs) from reading Responses-only dispatch/output-schema members after reporting them as ignored. Reject active misplaced orchestration semantics and unsupported legacy assistant `function_call` history rather than dropping a call. Keep genuine unknown-field handling unchanged.

   Exit condition: ignored values never affect IR or upstream output. Existing Responses dispatch and registered extensions still work; unsupported Chat custom tools remain explicit rejections, not a newly opened support slice. Resolve D3 before changing `prediction` handling.

4. **Correct existing cache and accounting carriers.** Match optional `prompt_cache_options` members and protocol-specific fields to the pinned official types. Distinguish empty options from active controls without inventing explicit defaults. Correct Standard Chat image-usage projection so an official reported count is not dropped as a vendor extension. Do not broaden Responses usage positions.

   Exit condition: legal partial options survive decode and supported re-encoding without adding defaults; Responses-only controls cannot leak through Chat. Existing cache-key and retention advisory policy remains explicit. Usage mappings preserve reported zero versus absent and reject inconsistent totals. New cache breakpoint/TTL carriers wait for the support queue.

5. **Complete target-specific reasoning admission.** Check model restrictions separately from the [activation/scoped-effort contract](architecture.md#reasoning-activation-and-effort-scope) and [explicit calibration](architecture.md#initial-mapping-plan). Assess remaining display/context losses separately from level calibration. Do not claim effective sampling or exact reasoning budgets solely from SDK field presence.

   Exit condition: each selected target has sourced model-level admission for the controls it promises. Off, numeric allocation and effort scopes have faithful carriers, selected loss policies or explicit pending rejections; display/context losses do not enter a level table. Extend native policy wiring only for a concretely selected mapping, without changing route membership.

6. **Reproduce terminal-projection suspicion independently.** Feed a synthetic Gemini `MAX_TOKENS` finish through [the decoder](../src/protocol/google/stream.rs) and reducer. Inspect the predicted `Progress(Unreported)` rejection before changing lifecycle behavior.

   Exit condition: either an honest incomplete result reaches JSON/SSE without a fabricated success terminal, or the reproduction disproves the suspected defect. Keep terminal correction separate from field-boundary refactoring.

Repair gate: each confirmed defect has a corrected projection or an explicitly selected rejection/degradation, with existing lowest-owner expectations updated where available and coverage gaps reported. Assess static delivery, streaming and history for all three downstream contracts. Preserve source-IR isolation, bounded expansion, cancellation, replay isolation and EOF-gated publication.

Use the [verification and integration boundary](#verification-and-integration-boundary) above. Real Provider checks, if later needed for a named unresolved remote contract, remain separate fixed-target/count/cap/timeout steps; docs and compilation do not prove live acceptance.

### Reasoning repair slices

S5/S6 own the remaining [effort-policy boundaries](../src/protocol/reasoning.rs). Keep the scoped IR and hypothetical A/B table separate from concrete target-policy decisions. D3 does not block these repairs; do not add an external mapping configuration surface. P6 capture switches do not select semantic mappings.

1. Validate repeated reported reasoning settings before many-to-one normalization. Item-terminal equality is not evidence that created/in-progress/terminal settings agree.
2. Extend non-identity calibration to ordered configuration items only with native snapshot validation before normalization. Until then, reject non-identity or undeclared entries rather than bypass a selected policy.
3. Propagate selected policy into native request/intake paths only for a concrete binding that needs it.

Exit condition: static/SSE metadata and supported history receive the selected scoped, directional policy without inferring an inverse or fabricating absent reports. Invalid native levels fail trusted validation. Preserve pre-normalization source conflicts, unchanged-source candidate projection and existing lifecycle safety. Route reconnection is not required to validate these codec boundaries.

### Support queue after projection repair

S3/S4 promote selected existing-IR carriers into the current migration. The remaining queue follows its local repair and contract gates, not successful route reconnection. Before each IR extension, present a minimal counterexample and structural options. An official carrier alone does not activate a binding.

1. Complete the remaining existing-IR carriers after S3/S4: Chat custom tools and explicit cache breakpoints/TTL. Keep each contract separate; unimplemented output or tool semantics do not become advisory omissions.
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

1. **Generation IR completeness.** Execute S1–S7 under the temporary routing freeze, complete the remaining support queue, then continue the selected tool slices. Keep deferred concepts explicit; a disconnected client loop does not invalidate correct IR/codec work.
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

P1/P2/P4/P5, P3's complete shared preset and nearest-token/highest-level-tie rule, and P6's configurable capture are selected in [Architecture](architecture.md#settled-decisions). Only the following details and deferred policies remain open. Continue independent slices; ask only when an unresolved choice blocks the affected contract. Unimplemented support is not a reason to reconsider an already selected policy.

| Question | Unresolved choice and recommendation | Blocks |
|---|---|---|
| P3b: Insufficient output room | If a selected numeric thinking preset conflicts with the client's explicit output ceiling or the target's required answer room, choose rejection or a named downward budget adjustment within the ceiling. Never raise that ceiling or convert a reduced budget into disabled thinking implicitly. Until selected, retain target-validation failure; nearest-level matching and duplicate resolution are already decided | Only conflicting level-to-budget projection, not ordinary P3 lookup or quantization |
| P7: Public capability publication and reconnection | Decide how the future public surface distinguishes native fidelity from accepted-with-loss controls and whether clients receive warnings. Recommended: no false native support claim, no per-request policy override in this slice. Current route/admission incompatibilities stay blocked until a separate integration decision | Route-facing publication and full-chain acceptance, not local IR/codec work |
| P8: Existing loss rules with quality impact | Review default-strict-as-plain, reasoning-context omission, display suppression, developer-to-system and media-detail loss individually. Recommended: preserve explicit constraints and visibility intent, use faithful carriers first, and retain a legacy approximation only after its consequences are selected. Existing code is not evidence of equivalence | Affected S4–S6 rules only |

| Question | Current behavior | Options |
|---|---|---|
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
