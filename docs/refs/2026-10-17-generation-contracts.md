# Generation contract boundaries

Sources checked on 2026-10-17. These are protocol findings and design inputs, not Provider acceptance results. Current codec directions are OpenAI Chat/Responses and Messages in both directions, plus generateContent/Interactions upstream request encoding and SSE intake. Google client endpoints and static intake remain separate work.

## Sources

| Source | Revision | License | Evidence |
|---|---|---|---|
| [OpenAI Python](https://github.com/openai/openai-python/tree/bfada5040b0171cd0ae61aefae8c78fcf83c804a) | `bfada5040b0171cd0ae61aefae8c78fcf83c804a` | Apache-2.0 | Nullable media locators, response errors, access-program requests/reports, citations and probabilities |
| [Anthropic Python](https://github.com/anthropics/anthropic-sdk-python/tree/bcd55fe1d3f0f1240765f8cef04295b67337f55a) | `bcd55fe1d3f0f1240765f8cef04295b67337f55a` (1.15.0) | MIT | Logical turns, nullable delta usage, inference geography and beta replay controls |
| [Google Gen AI Python](https://github.com/googleapis/python-genai/tree/364889aafe40bfa8f3b9db847ba272d299f7d632) | `364889aafe40bfa8f3b9db847ba272d299f7d632` (2.31.0) | Apache-2.0 | Part signatures, citation metadata, Interactions lifecycle, step errors and usage |
| [Vercel AI](https://github.com/vercel/ai/tree/fce9210c6405c0ad8452fee3f50717e8ea910b97) | `fce9210c6405c0ad8452fee3f50717e8ea910b97` | Apache-2.0 | Supplementary codec-test ideas: interleaved thinking, signatures, usage and execution controls |

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

The current tail-digest representation is still narrower than arbitrary part-owned replay. Multiple signed ordinary-text parts, or a signed text part followed by another part, need a more general owner rather than positional attachment to the last text.

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

## Remaining accounting gap

Google reports `promptTokensDetails`, `candidatesTokensDetails`, cached/tool-use modality details and tool-use totals. Interactions reports corresponding `*_tokens_by_modality` lists and optional step usage. Current intake retains selected operation totals but drops these breakdowns; this is an unresolved fidelity gap, not evidence that the reports were absent. Some text/image/audio counts have existing IR slots, while video, cache-by-modality, tool-use ownership and scoped reports do not.

Recommendation: select a bounded typed usage breakdown with explicit measurement scope and inclusion relationships, then map observed facts independently. Keep absent counts unknown, distinguish source-reported zero/default from derived counts, and never mix step snapshots with cumulative operation snapshots. Delivery may omit specifically approved auxiliary details with receipts, but source intake should not erase facts first. Anthropic TTL cache breakdowns, server-tool usage and input transformations need the same ownership review.

## Loss-policy decisions still needed

| Current rule | Counterexample | Recommendation |
|---|---|---|
| Drop all foreign opaque reasoning, deleting empty owners | An opaque-only continuation belongs to an unfinished tool turn; `reasoning_as_text: false` still allows its removal | Separate opaque purpose, actual content/call owner and required replay from readable reasoning. Keep target isolation; reject incompatible required continuation. Optional-history omission needs its own justified rule |
| Messages profile always omits sampling | A sampling-capable model uses the same protocol profile | Keep official sampling carriers in codecs; select fixed/ignored sampling only through evidenced target policy. Current registered binding does not prove the protocol-wide assumption |
| Absent output ceiling becomes `32000` | Target supports a larger cap, but a different client's product default limits the request | Make the mandatory inserted cap an explicit trusted target/product default, or require caller input. Do not automatically raise it to the model maximum |
| File citation index retains older ordinal meaning | Updated official reference calls it an output insertion position | Select the protocol revision and a point anchor; do not reuse a list ordinal |

Keep the inspected protections for normalization-default tools, no automatic thinking-budget clipping, explicit role approximations, and incomplete terminals. Source-level numeric labels do not establish cross-model equivalence. An official API feature may justify IR vocabulary even when another target must reject it.

## Contract and evidence map

| Area | Existing owners/checks | Remaining boundary |
|---|---|---|
| Ordered history, roles, call/result links | `Item`, message envelopes, `ClientManaged`; history/tool tests | Tool discovery, turn-scoped configuration and participant names remain rejected/deferred |
| Tools and output constraints | Typed definitions, exact JSON, strictness defaults; schema/tool tests | Hosted tools and native schema subsets are not universal support |
| Reasoning controls | Separate activation, scope, effort, budget and display; reasoning tests | Opaque replay purpose/part ownership and target-policy placement need selection |
| Input media | Resource tables and per-use description; media tests | File services, Messages document context and unselected detail conversions remain gaps |
| Generated media | Existing Chat audio owner and lifecycle tests | General generated image/audio/video needs a selected contract |
| Content evidence | Text-owned probabilities/citations; static and SSE tests | Citation licence, request-source associations and newer point anchors remain gaps |
| Safety and completion | Ordered safety reports; separate outcome/progress/cause; terminal tests | Current downstream safety-report rejection still blocks ordinary full-loop delivery |
| Usage | Typed totals, relations and selected subsets; usage/SSE tests | Complete modality, cache/tool and step-scoped reporting is not implemented |
| Execution policy | Separate service intent/report and cache ownership | Geography, access programs, replay enforcement and several returned execution reports lack complete owners |

This map does not claim all official semantics are represented or all protocol pairs are usable. Offline codec checks, synthetic SDK loops, live Provider behavior and public binding admission remain separate evidence.
