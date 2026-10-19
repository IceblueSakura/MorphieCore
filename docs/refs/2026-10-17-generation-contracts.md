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

### P7 binding evidence, 2026-10-18

These findings concern registered bindings, not inspected private deployments. No Provider calls were made. Public source/model documentation was rechecked on 2026-10-18; the [reconnection sequence](../roadmap.md#first-reconnection-slices) owns scheduling and unresolved selections.

- **DashScope Qwen3.8:** the [Chat reference](https://help.aliyun.com/zh/model-studio/qwen-api-via-openai-chat-completions) distinguishes `max_completion_tokens` (thinking plus answer) from Qwen's answer-only `max_tokens`. Qwen3.8 Flash/Max support the former. The registered DashScope Chat bindings now retain that total-cap carrier rather than applying the broad Bailian legacy spelling. GLM-5.3 has different documented `max_tokens` accounting; Coding Plan and other deployments are not inferred from this fix. The reference allows a small output-count deviation, so request projection does not certify token-perfect enforcement.
- **DeepSeek Responses:** the [official support table](https://api-docs.deepseek.com/guides/responses_api/) treats developer input as user, accepts summary without generating it, does not support encrypted reasoning/include, and ignores `parallel_tool_calls` while always allowing parallel calls. Plaintext reasoning input is merged into an adjacent assistant message. These are semantic differences, not transport compatibility. Its `/responses` path and a successful request cannot establish faithful OpenAI behavior; no new omission policy was selected here.
- **OpenRouter Luna:** the [model endpoints](https://openrouter.ai/api/v1/models/openai/gpt-6-luna/endpoints), [Responses overview](https://openrouter.ai/docs/api/reference/responses/overview) and [GPT-6 migration guide](https://openrouter.ai/docs/cookbook/evaluate-and-optimize/model-migrations/gpt-6) establish a model/API candidate and ordinary call/result history carriers. They do not prove a fixed routed backend's complete strict-default, encrypted-replay, phase or usage behavior. Prefer an explicit non-strict tool contract and verify observed history rather than asserting universal support.
- **Client strictness:** pi `8efd314`'s `openai-responses.ts` defaults `supportsStrictMode` to false and passes it to `convertResponsesTools`; despite that helper's own true default, the normal path omits `strict`. Configuring support true emits `strict:false` for ordinary tools, not `strict:true`. OpenCode `e5a557e`'s `packages/ai/src/protocols/open-responses.ts` explicitly emits false. Omitted Responses strictness still means schema normalization and cannot become a plain native tool.
- **Messages clients and Go:** pi defaults to short caching, while OpenCode's cache policy adds tool/system/tail markers. Owner-local `cache_control` therefore reaches the frozen public `ExplicitCache` rejection even when the codec has a carrier. [Haiku 5.5](https://platform.claude.com/docs/en/models/haiku-5-5/overview) supports caching/adaptive thinking but restricts thinking blocks to their issuing or linked accounts; omit sampling. The [Go](https://opencode.ai/docs/go/) and [v2 Go](https://opencode.ai/v2/docs/console/go) interfaces require deployment-version confirmation before changing the pinned `/zen/go/v1/messages` binding. A local credential's stable identity does not prove the proxy's internal account stability. Neither published cache pricing nor the stated 30-day request retention establishes a ZDR or compatible cache-retention policy.
- **Gemini values and delivery:** [Gemini 3.8 Flash](https://ai.google.dev/gemini-api/docs/models/gemini-3.8-flash) supports low/medium/high, rejects minimal, and lists 65,536 maximum output tokens. Current generic target encoding accepts minimal and 65,537; this is a model-effective validation gap, not a reason to delete the protocol's MINIMAL carrier or silently clamp. generateContent STOP remains natural-or-stop-sequence, while Interactions completed has no equivalent precise cause. Neither proves a Messages `end_turn`.

Focused synthetic request probes confirmed that the proposed Qwen Chat and Luna Responses shapes pass public admission and agree between preflight/preparation. A native Haiku request without owner cache markers passes those boundaries, while adding a marker reaches `ExplicitCache`; Gemini omitted-strict tools reach `ToolSchemaNormalization`. The unsupported Gemini numeric/level counterexamples also currently pass encoding. These probes exercised no upstream response, SDK or client loop and are not retained as success tests for unresolved contracts.

### Messages owner-local cache admission, 2026-10-19

The user resumed this bounded admission slice after the initial P7 scope choice. The [Claude caching guide](https://platform.claude.com/docs/en/build-with-claude/prompt-caching) and [Haiku 5.5 model reference](https://platform.claude.com/docs/en/models/haiku-5-5/overview) establish owner-local 5m/default and 1h cache carriers. Explicit breakpoints are limited to four in tools → system → messages prefix order, with longer TTLs before shorter ones. Four explicit markers plus automatic caching, or an automatic TTL conflicting with the tail marker, return an error. Empty text and direct thinking markers are not valid cache owners.

At pi `8efd314`, `packages/ai/src/api/anthropic-messages.ts` defaults to short caching and places markers on the last tool, system text and final eligible user block, including a tool-result wrapper. Long caching emits 1h. At OpenCode `e5a557e`, `packages/ai/src/cache-policy.ts` normally assigns up to four inline markers; this client-side automatic placement is not the Messages request-wide automatic-cache field. Its native compiler preserves tool-result wrapper markers.

The pinned [Go handler](https://github.com/anomalyco/opencode/blob/e5a557efeee02c94486d391afc48f24a49ebc9a5/packages/console/app/src/routes/zen/util/handler.ts) calls `requestBody.stream(providerModel ?? model, false)`. The [body forwarder](https://github.com/anomalyco/opencode/blob/e5a557efeee02c94486d391afc48f24a49ebc9a5/packages/console/app/src/routes/zen/util/requestBody.ts) replaces only the root model value and forwards remaining bytes; the [inference bridge](https://github.com/anomalyco/opencode/blob/e5a557efeee02c94486d391afc48f24a49ebc9a5/packages/console/app/src/lib/inference-proxy.ts) maps the legacy Messages path to the inference service. That hop does not strip or rebuild marker/TTL fields. [Current Go documentation](https://opencode.ai/v2/docs/console/go) lists Haiku cache pricing and a newer public endpoint; it does not certify deployed legacy-path compatibility or explicitly promise 1h handling at the destination.

This evidence supports exact request forwarding on the selected binding, not live cache effectiveness. The [admission contract](../architecture.md#owner-local-cache-admission) opens only all-native configured candidates and suppresses unrequested automatic caching beside explicit markers. Mixed routes and caller-selected automatic policy remain closed. Actual writes/reads, billed TTL, proxy version and account-compatible signed-thinking replay remain separate acceptance gates; cache admission does not imply storage retention or ZDR.

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

`Part.replay` and `ToolCall.replay` now own those signatures directly. The old tail/call surrogate is removed; a signature-only surrogate returned from earlier history is dropped under `gemini.omit-legacy-signature.v1` rather than re-anchored or rejected. A separate signature format prevents rebinding thought payloads as ordinary attachments; fidelity includes the owner's lifecycle and catches deleted obligations.

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

### Compatibility policy review, 2026-10-18

Primary pages rechecked on 2026-10-18:

- [OpenAI reasoning](https://developers.openai.com/api/docs/guides/reasoning#phase-parameter) requires preserving reported assistant phase during manual replay: dropping it can make a preamble look like a final answer. [Agents SDK](https://openai.github.io/openai-agents-js/guides/models/) retains it in Responses history but warns/drops it for Chat unless strict feature validation is enabled. The selected gateway policy permits target-local request omission, not permanent default loss during client delivery.
- pi [`4954d76`](https://github.com/earendil-works/pi/tree/4954d760504c11df9b2cbdc040346107b6a8153d) stores phase in `packages/ai/src/api/openai-responses-shared.ts` text signatures; its `transform-messages.ts` creates stripped target copies for different models. OpenCode [`e5a557e`](https://github.com/anomalyco/opencode/tree/e5a557efeee02c94486d391afc48f24a49ebc9a5) retains phase in `packages/ai/src/protocols/open-responses.ts` provider metadata and projects it through `packages/core/src/session/runner/to-llm-message.ts`. This source inspection is not a full-client acceptance run.
- [Google CitationSource](https://ai.google.dev/api/generate-content#CitationSource) uses byte offsets and says licence information is required for code citations. [Gemini API terms](https://ai.google.dev/gemini-api/terms) explain that applicable law may require providing returned attribution to users. These are content-evidence obligations, not merely incidental settings. Nonempty sources remain rejected; a synthetic late-citation stream is not evidence of real arrival timing or frequency.
- [OpenAI prompt caching](https://developers.openai.com/api/docs/guides/prompt-caching) distinguishes `in_memory` from maximum extended retention and model/organization-dependent defaults. [Data controls](https://platform.openai.com/docs/guides/your-data) classify cached KV tensors in GPU-local storage as application state. Dropping `in_memory` can select a longer default; `store: false` does not replace this control. `"24h"` is not a guaranteed 24-hour cache hit, and newer minimum-TTL options are not equivalent maximum-retention carriers.

The selected implementation keeps retention support separate from the default-off `omit_day_retention` target permission. No registered binding opts in. Phase-only manifests retain source labels and boundaries for Chat/Messages, but cannot make a client UI phase-aware or recover a wholly discarded carrier. Native Responses remains exact. Unsupported completion, citation, structured-output and replay contracts are not relaxed by either policy.

The extended pinned pi probe (`8efd314`, Node 24.21.0) passed 24 serial synthetic fetches, adding phase-only Responses output with two unlabelled prefixes to the earlier nine cases. Chat/Messages returned the manifest and restored the original four messages; Responses returned native labels without a phase-only manifest. These fixtures use the already-representable output-token-limit terminal, not an invented cross-protocol natural completion. This is client-codec evidence only; no OpenCode or real Provider loop was run.

### Messages execution and replay policy

- [Data residency](https://platform.claude.com/docs/en/manage-claude/data-residency): `inference_geo: "us"` constrains inference geography; omission uses workspace policy. The stable field exists at the earlier project pin. It cannot be dropped as an unknown preference.
- `speed` affects execution/pricing; beta `fallbacks` and `fallback_credit_token` affect fallback/billing behavior. Current client intake rejects active values. This does not implement refusal retries or billing-token forwarding.
- [Preserved thinking](https://platform.claude.com/docs/en/build-with-claude/preserved-thinking): beta `thinking.block_binding.prefix_mismatch_behavior` selects rejection or dropping invalid thinking blocks. Explicit `"error"` can opt older accounts into enforcement; it is not universally equivalent to omission. Since 2026-10-18 Messages intake admits only `drop_block`, omitted with a receipt; default and `"error"` enforcement still reject. Returned `input_transformations` are a separate unmodeled report.

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
| Safety and completion | Ordered safety reports; separate outcome/progress/cause; terminal tests | Assessment-only reports are omitted with a receipt since 2026-10-18; blocking-report rejection still blocks those full loops |
| Usage | Typed source-scoped reports, Google/Interactions modality/step intake, Messages TTL/tool/thinking reports and exact native carriers | Interactions mode-specific total relationships remain unestablished; downstream carriers do not express all source scope/detail |
| Execution policy | Separate service intent/report and cache ownership | Geography, access programs, replay enforcement and several returned execution reports lack complete owners |

This map does not claim all official semantics are represented or all protocol pairs are usable. Offline codec checks, synthetic SDK loops, live Provider behavior and public binding admission remain separate evidence.
