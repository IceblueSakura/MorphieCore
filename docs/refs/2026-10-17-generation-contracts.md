# Generation contract boundaries

Sources checked on 2026-10-17. These are protocol findings and design inputs, not Provider acceptance results. Current codec directions are OpenAI Chat/Responses and Messages in both directions, plus generateContent/Interactions upstream request encoding and SSE intake. Google client endpoints and static intake remain separate work.

## Sources

| Source | Revision | License | Evidence |
|---|---|---|---|
| [OpenAI Python](https://github.com/openai/openai-python/tree/bfada5040b0171cd0ae61aefae8c78fcf83c804a) | `bfada5040b0171cd0ae61aefae8c78fcf83c804a` | Apache-2.0 | Nullable media locators, response errors, access-program requests/reports, citations and probabilities |
| [Anthropic Python](https://github.com/anthropics/anthropic-sdk-python/tree/bcd55fe1d3f0f1240765f8cef04295b67337f55a) | `bcd55fe1d3f0f1240765f8cef04295b67337f55a` (1.15.0) | MIT | Logical turns, nullable delta usage, inference geography and beta replay controls |
| [Google Gen AI Python](https://github.com/googleapis/python-genai/tree/364889aafe40bfa8f3b9db847ba272d299f7d632) | `364889aafe40bfa8f3b9db847ba272d299f7d632` (2.31.0) | Apache-2.0 | Part signatures, citation metadata, Interactions lifecycle, step errors and usage |
| [Vercel AI](https://github.com/vercel/ai/tree/fce9210c6405c0ad8452fee3f50717e8ea910b97) | `fce9210c6405c0ad8452fee3f50717e8ea910b97` | Apache-2.0 | Supplementary codec-test ideas: interleaved thinking, signatures, usage and execution controls |
| [pi](https://github.com/earendil-works/pi/tree/8efd314fa3d3232ce197d8d7a6a5660530b9d83e) | `8efd314fa3d3232ce197d8d7a6a5660530b9d83e` | MIT | Actual cross-model history conversion, tool-turn regression and replay identity handling |

Research checkouts live outside the repository under `/home/sakura/codespace/generation-codec-audit/`. No third-party test code or dataset is vendored. SDK shapes and official prose establish the contract; reference-library fixtures are not an oracle when they contradict required fields or arithmetic. These observations supplement, rather than silently replace, the baseline pins in [References](../references.md).

## Exact carriers and source validation

### Function strictness defaults

The [OpenAI function-calling guide](https://developers.openai.com/api/docs/guides/function-calling) distinguishes Chat's non-strict default from Responses schema normalization. A source `Omitted(NonStrict)` tool has an exact Responses carrier: emit `strict: false` without changing its schema or source IR. This is default materialization, not a loss rule. The inverse conversion is not symmetric: `Omitted(NormalizeSchema)` still requires normalization semantics and must not become a plain tool.

Independent expectations live in [tool_calls.rs](../../tests/semantic/tool_calls.rs). The request writer also serves settings echoes; effective strictness must remain meaningful there.

### Responses media and error metadata

The SDK's `response_input_image_param.py` and `response_input_file_param.py` allow nullable `file_id`. Null names no resource. A non-null ID, including one alongside an otherwise supported URL/data carrier, requires credential-owned resource handling and is rejected. Wrongly typed non-null values cannot hide behind an accepted URL. See [chat.rs](../../tests/semantic/chat.rs).

`response_error.py` exposes `misalignment`: classification, explanation, optional public continuation instructions and a review target. Current `ResponseError` has only code/message/param. Non-null details therefore reject, in static and terminal-event intake, rather than silently losing safety/review information. This carrier already exists at the earlier project pin.

Newer `response_create_params.py` and `response.py` expose `access_programs`; the request selects a program and the response reports the effective program. Both are protected execution facts, not arbitrary metadata. Current codecs reject non-null values before filtering, including reports first appearing in later SSE snapshots. A selected semantic owner is still needed.

### Nested extensions

Unknown fields in probability entries and citation objects are ignored; known token, numeric, byte and citation fields remain strict. Repeated annotation snapshots compare normalized known facts without reallocating resource identities. A changed URL/range/title still fails. Independent static and fragmented-stream expectations live in [chat.rs](../../tests/semantic/chat.rs) and [responses_sse.rs](../../tests/transport/responses_sse.rs).

### Messages turns and usage

`message_create_params.py` specifies that consecutive same-role messages combine into one turn. Native validation checks pending calls and the leading result-block prefix over the logical turn, not one JSON message object. It preserves message boundaries and block order; incomplete, duplicate and orphan results still reject.

`message_delta_usage.py` makes input/cache counts and output detail reports optional/nullable. The SDK reducer in `src/anthropic/lib/streaming/_messages.py` updates them only when non-null. Null means no new observation; zero is a real replacement. Output counts are cumulative, not additive. See [anthropic_sse.rs](../../tests/transport/anthropic_sse.rs).

Natural completion requires evidence: a completed response and finished turn alone do not establish `end_turn`. Native response encoding and downstream delivery use the same terminal mapping. See [anthropic.rs](../../tests/semantic/anthropic.rs).

### Signed Google parts

The [function-calling guide](https://ai.google.dev/gemini-api/docs/generate-content/function-calling#managing-conversation-history-manually) forbids merging signed and unsigned parts. In a single frame, `[{text:"A"},{text:"B",thoughtSignature:"…"}]` must not become signed `"AB"`. Explicit within-frame boundaries are preserved; the first part of a later frame may continue a previous fragment, including an empty signature-bearing closing fragment. The independent replay expectation lives in [google.rs](../../tests/semantic/google.rs).

`Part.replay` and `ToolCall.replay` now own those signatures directly. The old tail/call surrogate is removed. A separate signature format prevents rebinding thought payloads as ordinary attachments; fidelity includes the owner's lifecycle and catches deleted obligations.

The [attachment codec](../../src/protocol/attachments.rs) transports a bounded layout/dependency manifest through existing client opaque fields, consuming it before semantic request construction. It restores original text boundaries only when returned bytes, exact arguments and non-text ordering agree. It never restores business content from a hidden copy or upgrades an observed partial owner. A carrier that does not match, including an unknown or ambiguous scope such as a flat Responses assistant prefix that cannot be associated safely, is dropped with its replay; the visible history proceeds.

The [pi probe](../../tests/sdk/pi_owned_replay.mjs) uses this repository's actual SSE delivery, then pinned pi response parsers and request builders, then MorphieCore intake and native history encoding. On Node 24.21.0, 18 serial synthetic fetches exercised three protocols: equal signed text owners plus a signed call; a large-number mutation; and an empty signed text owner. Source messages stayed unchanged. The first two cases passed on all three protocols; the empty case restored on Responses and Messages.

Pinned pi's [`openai-completions.ts`](https://github.com/earendil-works/pi/blob/8efd314fa3d3232ce197d8d7a6a5660530b9d83e/packages/ai/src/api/openai-completions.ts#L1395-L1406) skips assistant messages without content or tool calls, even after assigning `reasoning_details`. An opaque-only turn therefore disappears before the gateway sees its next request; the local pi checkout from 2026-10-15 behaves the same. A stateless gateway cannot infer a wholly absent carrier, and adding visible filler would violate the selected contract.

On 2026-10-18 the user accepted this as a client limitation rather than a reason to reject requests. The [signature guide](https://ai.google.dev/gemini-api/docs/generate-content/thought-signatures) strictly validates function calls in the current turn; omitting other signatures does not block the request, though it may degrade quality. A dropped turn has no calls by construction. The probe now asserts this limitation for the Chat empty case: no manifest returns, the next request is accepted and its history has no model turn.

pi normalizes `9007199254740993` to `9007199254740992`. On 2026-10-18 the user selected degradation over rejection: all three returns drop that turn's text/call signatures rather than replay a mismatched signature, keep the thought signature, and proceed. Because strict validation covers function calls in the current turn, generateContent may then reject that request upstream; earlier turns are not strictly validated. This cannot undo a tool invocation already performed by the client with rounded arguments; the probe executes no tools. Exact numeric spellings and object order remain conservative dependencies, so other client normalizations also drop their carriers.

This validates the history carrier, not full request admission: tools/controls retain their separate frozen contracts. No Provider calls or opencode probe were run. Identical-content edits and wholesale loss of the opaque manifest remain outside the selected stateless detection contract.

### Opaque-only turns: native API versus client extension

The [OpenAI Chat reference](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create) and pinned `chat_completion_assistant_message_param.py` specify nullable assistant `content`, required unless `tool_calls` or legacy `function_call` is supplied. Required presence does not mean nonempty text. Audio and refusal also have separate typed fields. Neither the pinned assistant input nor output schema defines `reasoning_details` or `encrypted_content`; the former is an [OpenRouter-compatible extension](https://openrouter.ai/docs/guides/best-practices/reasoning-tokens), not OpenAI's native Chat reasoning carrier. The pi behavior therefore does not, by itself, establish an OpenAI protocol violation.

In Responses, a reasoning item is distinct from a message and may have `summary: []` with opaque `encrypted_content`. The [reasoning guide](https://developers.openai.com/api/docs/guides/reasoning) makes summaries opt-in and explicitly describes `max_output_tokens` or context exhaustion before any visible output. Stateless history preserves returned reasoning items. This does not guarantee that every such incomplete response contains a usable token: follow the [completed-item replay rule](https://developers.openai.com/api/reference/resources/responses/subresources/input_items), not the possibly partial value in `response.output_item.added`.

The [Google signature guide](https://ai.google.dev/gemini-api/docs/generate-content/thought-signatures) documents an empty signature-bearing text **part** during non-function-call streaming. That does not establish an entirely empty assistant **turn**. A trailing empty fragment after visible text closes the accumulated text owner in this codec and does not trigger pi's whole-message filter. The probe deliberately constructs the stronger case: no visible text or calls in the entire response, plus a signature and `MAX_TOKENS`. It is a synthetic boundary case, not an observed Provider exchange or evidence of its frequency.

Ordinary tool-only responses and nonempty answers do not lose their carrier. The loss requires an entire turn without text/calls, meaningful state projected into the gateway's Chat extension, and a client that discards that extension-only message. The accepted limitation does not change a generic Chat empty-message policy.

## Protected semantics without complete owners

### Messages execution and replay policy

- [Data residency](https://platform.claude.com/docs/en/manage-claude/data-residency): `inference_geo: "us"` constrains inference geography; omission uses workspace policy. The stable field exists at the earlier project pin. It cannot be dropped as an unknown preference.
- `speed` affects execution/pricing; beta `fallbacks` and `fallback_credit_token` affect fallback/billing behavior. Current client intake rejects active values. This does not implement refusal retries or billing-token forwarding.
- [Preserved thinking](https://platform.claude.com/docs/en/build-with-claude/preserved-thinking): beta `thinking.block_binding.prefix_mismatch_behavior` selects rejection or dropping invalid thinking blocks. Explicit `"error"` can opt older accounts into enforcement; it is not universally equivalent to omission. Current intake rejects this control. Returned `input_transformations` are a separate unmodeled report.

### Attribution

Google [`CitationMetadata`](https://ai.google.dev/api/generate-content#CitationMetadata) can report training-data attribution without a hosted search tool. `CitationSource` includes byte offsets, URI and licence. Current IR claim coordinates support units, but citation licence and some source-association facts lack owners. Nonempty citation metadata and grounding metadata reject instead of becoming plain text.

Interactions `TextContent.annotations` appears in initial model-output content, initial thought summaries and thought-summary deltas. All reject nonempty annotations consistently. Merely rejecting the separate annotation delta is insufficient.

Messages document/search citations remain the separate [1C proposal](../roadmap.md#1c-design-messages-citations): request-document ordinals, cited text and enable flags need explicit owners.

The newer OpenAI SDK describes `file_citation.index` as an insertion position in output text, whereas the earlier pin described a file-list index. Existing `reported_ordinal` is explicitly not a text offset. Do not relabel this silently: select the updated baseline, offset units and point-anchor semantics, with a distinct `file_path.index` contract.

### Interactions lifecycle and failures

The SDK files under `google/genai/_gaos/types/interactions/` distinguish:

- `modeloutputstep.py`: a step can carry a `google.rpc.Status` error; numeric code 7 is not HTTP status 7 or 500. Current intake rejects non-null step errors before emitting their content; it does not invent an HTTP classification or publish success.
- `interactionsseeventinteraction.py`: SSE lifecycle envelopes can carry `steps`, `continuation_token` and `service_tier`. These are not limited to static responses. Current intake rejects unsupported snapshots/continuation/tier reports instead of ignoring contradictions or losing state. Exact snapshot checking and bounded reported-service/continuation owners remain needed.
- `stepstop.py`: per-step usage is distinct from operation-level usage. No operation total may be invented by blindly summing overlapping reports.

A stream producing `"A"` followed by a lifecycle snapshot containing `"B"` is a concrete counterexample to ignoring lifecycle `steps`. A continuation-bearing incomplete result cannot be treated as ordinary truncation without retaining the token's purpose and scope.

### Interactions signatures

The [thinking guide](https://ai.google.dev/gemini-api/docs/thinking) distinguishes thought and built-in tool signatures from standard function calls. The pinned SDK's `functioncallstep.py` has no signature field; `thoughtstep.py` does. The codec preserves thought-owned signatures, rejects a signature on a standard function call, and does not mint a surrogate reasoning item for that nonexistent carrier. Built-in tool signatures remain deferred. A signature-only thought remains ordered thought content, not a movable part/call attachment.

## Scoped accounting contract

The IR retains one latest typed `UsageReport` per operation or response-local step, including sparse modality, cache-TTL and tool-request detail owners. Updates replace their own scope; steps never sum into the operation. Final seals observation updates rather than claiming exhaustive billing. Content edits preserve the reports, and the usual operation usage view is derived rather than independently stored.

Let P = prompt input, U = tool-use prompt input, O = generated output excluding thoughts, R = thoughts, C = cached input:

- [generateContent UsageMetadata](https://ai.google.dev/api/generate-content#UsageMetadata) and pinned SDK `types.py` explicitly give `total = P + U + O + R`, with C within P. Store P/U/O/R separately, derive inclusive input P+U and output O+R, and never add C twice.
- Interactions `usage.py` describes `total_tokens` as prompt + responses + other internal tokens. The [function-calling reference example](https://ai.google.dev/api/interactions-api#CreateInteraction-function_calling) reports P=100, U=50, O=25, R=0, T=125, while supplementary reference fixtures use additive U. This does not establish one universal inclusion equation. Keep tool-input and total relationships unreported; do not choose a relation by arithmetic coincidence.
- The [streaming guide](https://ai.google.dev/gemini-api/docs/streaming) distinguishes cumulative `step.stop.usage` from local `step_usage`. Opening/status updates and terminal usage are operation snapshots. A terminal without another usage value seals the latest observed operation snapshot without adding step counts.
- Google modality lists belong separately to P, U, O and C. Interactions has corresponding `*_tokens_by_modality` fields. Missing categories do not establish zero or exhaustive coverage. Its `modalitytokens.py` permits missing/null category and count; retain unknowns. Google's omitted proto3 scalar zero retains explicit default provenance.
- Grounding tool counts are requests, not token consumption; missing counts remain unknown. Duplicate categories and sums exceeding their own known parent reject, but sparse lists need not equal their parent.

For P=100/U=30/O=20/R=7/C=40/T=157, generateContent yields a derived input of 130 and output of 27 while retaining every source bucket. The equivalent Interactions report preserves the numbers without claiming U's relation or rejecting T solely for not matching an assumed formula. O remains reported even when R is absent and an inclusive output cannot be derived.

The named `generation.usage-report-view.v1` delivery projection preserves the source report and records loss of unrepresented detail/scope/provenance on the target copy. OpenAI's scalar usage requires its known inclusion relation; an Interactions exchange may consequently deliver no scalar usage. Static and streamed omission agree, including reports containing only auxiliary detail. No downstream custom usage event is introduced.

Anthropic's [cache documentation](https://platform.claude.com/docs/en/build-with-claude/prompt-caching) gives disjoint input/read/write categories and `cache_creation = five-minute + one-hour writes`; tool-request counts are separately priced observations. Native JSON and SSE now map these into operation reports. `output_tokens_details.thinking_tokens` remains an output subset, including internal delimiters; subtraction does not establish a visible-text token count.

The pinned `MessageDeltaUsage` has server-tool/thinking reports but no `cache_creation` TTL object. Full TTL and server-tool objects require both children. Optional null means no observation; non-null nested objects replace rather than field-merge or sum. Messages JSON and opening SSE preserve native TTL detail; late new/changed TTL detail rejects, while supported tool/thinking reports use delta fields. Missing generic children are not filled with zero. Exact details stay in the Messages delivery copy; OpenAI keeps its derivable scalar/reasoning view and records omitted breakdowns.

The pinned SDK fixture `tests/lib/streaming/fixtures/message_delta_fields_response.txt` changes the cache-write parent from 10 to 12 without another TTL report, while its reducer retains 10+0. That demonstrates SDK replacement behavior, not a valid exception to the documented arithmetic. The codec rejects the contradictory accumulated report instead of changing a source fact or weakening the global relationship. Input-transformation reports remain separate execution work.

## Selected visible-history transfer

The user selected bounded visible-history transfer after reviewing [pi's actual transformer](https://github.com/earendil-works/pi/blob/8efd314fa3d3232ce197d8d7a6a5660530b9d83e/packages/ai/src/api/transform-messages.ts) and its [tool-turn test](https://github.com/earendil-works/pi/blob/8efd314fa3d3232ce197d8d7a6a5660530b9d83e/packages/ai/test/transform-messages-cross-model-thinking.test.ts). pi drops all foreign thinking rather than presenting it as assistant speech; it keeps ordinary content and call/results, and removes model-bound signatures. A local synthetic probe confirmed this behavior and source immutability without Provider calls.

MorphieCore follows that transfer direction, not pi's synthetic orphan results, image placeholders or generic removal of failed assistant output. Compatible replay retains dependency checks. Incompatible reasoning must have both a completed owner and final opaque value before omission; the transfer can happen before a final assistant answer. Required compacted/cached input remains protected. Unattributed readable reasoning is not assumed foreign. Native cross-format omission occurs before grouping so an omitted reasoning-only turn does not leave a fabricated empty message.

[Responses item-ID handling](https://github.com/earendil-works/pi/blob/8efd314fa3d3232ce197d8d7a6a5660530b9d83e/packages/ai/src/api/openai-responses-shared.ts#L313-L322) also matters: wire call-item IDs may be paired with removed reasoning. The target copy forgets those aliases in affected assistant runs without changing semantic call IDs or actual results. Damaged or missing compatible replay cannot be hidden by another owner's foreign omission.

This policy supersedes the earlier recommendation to reject all active-tool-turn transfer by default. Native API replay requirements do not themselves forbid a different model receiving a visible transcript; transfer is an explicit loss of internal state, not equivalent continuation. Foreign part/call values are cleared without deleting their visible owners.

## Loss-policy decisions still needed

| Current rule | Counterexample | Recommendation |
|---|---|---|
| Messages profile always omits sampling | A sampling-capable model uses the same protocol profile | Keep official sampling carriers in codecs; select fixed/ignored sampling only through evidenced target policy. Current registered binding does not prove the protocol-wide assumption |
| Absent output ceiling becomes `32000` | Target supports a larger cap, but a different client's product default limits the request | Make the mandatory inserted cap an explicit trusted target/product default, or require caller input. Do not automatically raise it to the model maximum |
| File citation index retains older ordinal meaning | Updated official reference calls it an output insertion position | Select the protocol revision and a point anchor; do not reuse a list ordinal |

Keep the inspected protections for normalization-default tools, no automatic thinking-budget clipping, explicit role approximations, and incomplete terminals. Source-level numeric labels do not establish cross-model equivalence. An official API feature may justify IR vocabulary even when another target must reject it.

## Contract and evidence map

| Area | Existing owners/checks | Remaining boundary |
|---|---|---|
| Ordered history, roles, call/result links | `Item`, message envelopes, `ClientManaged`; history/tool tests | Tool discovery, turn-scoped configuration and participant names remain rejected/deferred |
| Tools and output constraints | Typed definitions, exact JSON, strictness defaults; schema/tool tests | Hosted tools and native schema subsets are not universal support |
| Reasoning controls and replay | Separate controls, real part/call replay slots and bounded client manifests; native and pi carrier checks | Target-policy placement and unverified client transformations remain separate boundaries |
| Input media | Resource tables and per-use description; media tests | File services, Messages document context and unselected detail conversions remain gaps |
| Generated media | Existing Chat audio owner and lifecycle tests | General generated image/audio/video needs a selected contract |
| Content evidence | Text-owned probabilities/citations; static and SSE tests | Citation licence, request-source associations and newer point anchors remain gaps |
| Safety and completion | Ordered safety reports; separate outcome/progress/cause; terminal tests | Current downstream safety-report rejection still blocks ordinary full-loop delivery |
| Usage | Typed source-scoped reports, Google/Interactions modality/step intake, Messages TTL/tool/thinking reports and exact native carriers | Interactions mode-specific total relationships remain unestablished; downstream carriers do not express all source scope/detail |
| Execution policy | Separate service intent/report and cache ownership | Geography, access programs, replay enforcement and several returned execution reports lack complete owners |

This map does not claim all official semantics are represented or all protocol pairs are usable. Offline codec checks, synthetic SDK loops, live Provider behavior and public binding admission remain separate evidence.
