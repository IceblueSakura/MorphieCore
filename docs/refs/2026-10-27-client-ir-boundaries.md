# Client and IR protocol boundary review

Dated protocol-source evidence from 2026-10-27 through 2026-11-13. [Architecture](../architecture/index.md) owns accepted contracts, [References](../references.md) owns regression pins, and the [issue register](../roadmap/unscheduled-issues.md) owns unresolved qualifications. Synthetic observations, installed-client behavior and real service observations below retain their separate scopes; none is a complete support matrix.

## Contents

- [Scope and evidence](#scope-and-evidence)
- [Remaining source disagreements](#remaining-source-disagreements)
- [Protocol expressiveness check](#protocol-expressiveness-check)
  - [Independent item lifecycle versus overall stopping](#independent-item-lifecycle-versus-overall-stopping)
  - [Custom tools: source disagreement versus a real carrier gap](#custom-tools-source-disagreement-versus-a-real-carrier-gap)
  - [Metadata and configuration reports are direction-specific](#metadata-and-configuration-reports-are-direction-specific)
  - [Reasoning and argument representations](#reasoning-and-argument-representations)
- [Diagnostic extension source policies (2026-10-27)](#diagnostic-extension-source-policies-2026-10-27)
- [Follow-up source and feasibility review (2026-10-28)](#follow-up-source-and-feasibility-review-2026-10-28)
- [Binding-history source review (2026-10-28)](#binding-history-source-review-2026-10-28)
  - [Hosted MiniCPM control recheck (2026-11-13)](#hosted-minicpm-control-recheck-2026-11-13)
- [Pre-stage-4 contract qualification (2026-10-29)](#pre-stage-4-contract-qualification-2026-10-29)
- [Semantic-equivalence review (2026-10-28)](#semantic-equivalence-review-2026-10-28)
- [Chat JSON intake evidence (2026-10-30)](#chat-json-intake-evidence-2026-10-30)
- [Generation null-contract review (2026-11-02)](#generation-null-contract-review-2026-11-02)
- [Nine-direction tool assessment (2026-11-04)](#nine-direction-tool-assessment-2026-11-04)
  - [Discriminating counterexamples](#discriminating-counterexamples)
- [Tool-contract qualification (2026-11-04)](#tool-contract-qualification-2026-11-04)
  - [Primary sources](#primary-sources)
- [Ordered effort contract qualification (2026-11-05)](#ordered-effort-contract-qualification-2026-11-05)
  - [Temporal and presence disposition](#temporal-and-presence-disposition)

## Scope and evidence

Covered standard Chat Completions, Responses and Messages: public request/history intake, envelope and execution-context admission, semantic ownership, downstream JSON/SSE rendering, final snapshots and returned history. Pure closed request decoding is not the public Client decoder. Provider dialects, upstream model effectiveness, deployment, credential lifecycle and unrelated APIs were not audited. Representable IR, codec support, public-model admission and an activated service operation are different claims.

The source baseline remains [sources.lock.json](../../tests/upstream/sources.lock.json): OpenAI Python **3.37.0** (`e8a0fd28bbdc1699c97ff0f602b1af3dd378382d`), Node **7.43.0** (`d02d4de77ea7a6f82325c02e6f44deb34ee3992c`), Anthropic Python **2.1.0** (`9313f13a5a643ceed10fafba6785eab8e04b6e6e`) and TypeScript **1.1.0** (`5badc765f17d373405f300ee8e69e7f490b1f172`). Installed SDK source files were inspected alongside these pins. Mutable official reference pages were rechecked on the review date; discrepancies are listed separately rather than silently replacing the baseline.

The [official SDK source guide](2026-10-27-official-sdk-sources.md) indexes primary request/output unions, serializers and accumulators; the sections below cite the specific contracts behind each distinction.

## Remaining source disagreements

- **`user:null`:** pinned Python request types and current OpenAI reference differ; do not tighten solely from one SDK.
- **Chat response `message.refusal`:** Node required-nullable and reference optional-nullable differ. This is distinct from required `choice.logprobs`.
- **Custom Chat SSE:** Node declares/accumulates custom deltas; Python and the streaming reference remain function-only. See the [detailed comparison](#custom-tools-source-disagreement-versus-a-real-carrier-gap).
- **Legacy token ceilings:** `max_tokens` conversion exists, but deprecated versus visible-plus-reasoning ceiling semantics still require domain-specific equivalence evidence.
- **Responses image detail:** generated types require it while prose supplies default `auto`; omission alone is not a confirmed violation.
- **Other source drift:** Python/Node Responses `async` carriers and newer per-breakpoint TTL declarations need explicit source/version review, not silent baseline promotion.

## Protocol expressiveness check

Checked on **2026-10-27** against **`c8d0c96`**, current official references and pinned SDK contracts.

Additional source inspection used OpenAI Python [v3.39.0](https://github.com/openai/openai-python/releases/tag/v3.39.0), revision `f97798bc86e41f4a5d18644113ef94cf58722778`, and Node [v7.45.0](https://github.com/openai/openai-node/releases/tag/v7.45.0), revision `af8336b8c13bd5402868cf2ca0d9c64530c3848f`.

These October 26 release candidates for our baseline were inspected, not installed or promoted to regression pins.

This is analysis, not approval for additional omissions, wrapping or inferred state.

### Independent item lifecycle versus overall stopping

Responses has separate output-item status and response status carriers.

Its [output-message type](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/responses/response_output_message.py) and [response type](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/responses/response.py) do not require every item's status to equal the enclosing response's status.

The [streaming reference](https://developers.openai.com/api/reference/resources/responses/streaming-events) separately defines item-done, response-incomplete and response-failed events.

Schema expressiveness is not proof that a particular live model emits every combination.

Minimal relevant fields:

```json
{
  "status": "incomplete",
  "incomplete_details": {"reason": "max_output_tokens"},
  "output": [{"type": "message", "id": "m", "role": "assistant",
    "status": "completed",
    "content": [{"type": "output_text", "text": "A.", "annotations": []}]}]
}
```

Chat's [finish reason](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create) describes why generation stopped, not an independent status for every component. `content:"A."` plus `finish_reason:"length"` preserves the text and token-cap cause above; it cannot preserve the explicit completed-item observation. Changing only that source item to incomplete yields the same Chat view. Messages has no arbitrary per-block status either: `content_block_stop` closes a block, while `stop_reason` describes the message, including truncation.

**Intrinsic difference:** the explicit per-item lifecycle vector has no standard Chat/Messages carrier. Equality of source item and overall status is not a Chat wire requirement. The selected [delivery disposition](../architecture/adaptation-and-dialects.md#conversion-ladder) owns which per-item observations can be omitted.

The subsequently selected narrow omission preserves source IR and overall token-cap meaning while dropping eligible plain-text per-item observations. It remains lossy and must not establish executable tool completion. Do not rewrite source items to satisfy lowering or map `failed` to `length`. Failure after partial output needs an honest error path, not successful finalization; Chat/Messages lack the same normal result envelope for output plus an overall failed object.

### Custom tools: source disagreement versus a real carrier gap

| Surface | Evidence and classification |
|---|---|
| Chat complete JSON | Current [create reference](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create) and both SDK families define `type:"custom"` with `custom:{name,input}`. This is a real string-input carrier |
| Chat SSE | Both pinned and inspected [Node definitions](https://github.com/openai/openai-node/blob/af8336b8c13bd5402868cf2ca0d9c64530c3848f/src/resources/chat/completions/completions.ts) define custom deltas; its [stream accumulator](https://github.com/openai/openai-node/blob/af8336b8c13bd5402868cf2ca0d9c64530c3848f/src/lib/ChatCompletionStream.ts) concatenates custom input and finalizes custom calls. The inspected [Python chunk type](https://github.com/openai/openai-python/blob/f97798bc86e41f4a5d18644113ef94cf58722778/src/openai/types/chat/chat_completion_chunk.py) and current [streaming reference](https://developers.openai.com/api/reference/resources/chat/subresources/completions/streaming-events) remain function-only. The disagreement survives a stable-source refresh |
| Responses JSON/SSE | `custom_tool_call` and `response.custom_tool_call_input.delta`/`.done` explicitly carry string input |
| Messages | [Final tool-use input is always an object](https://platform.claude.com/docs/en/build-with-claude/streaming#input-json-delta). An arbitrary custom input such as `print(1)` has no direct native carrier; wrapping it in `{"input":"print(1)"}` changes the declared tool interface and requires coordinated execution/schema policy |

Classify Chat custom SSE as **source-disputed**, not intrinsically impossible. Node supplies a feasible codec design, not enough evidence to override the contradictory public reference silently. Python's type disagreement does not prove runtime rejection: its tolerant/generic accumulation is a separate consumer question. An additional ignored property is different from an actionable custom-call discriminator. Buffering into one chunk cannot resolve that semantic-contract disagreement.

### Metadata and configuration reports are direction-specific

| Fact | Chat | Responses | Messages |
|---|---|---|---|
| Request metadata | Arbitrary bounded string map | Same bounded map meaning | `metadata.user_id` is request attribution, not the OpenAI arbitrary map |
| Response metadata | Optional map in full `ChatCompletion`; no standard chunk member | Nullable map in the Response object, including lifecycle SSE | No corresponding standard Message response map |
| Sampling/configuration echo | Request controls do not create `temperature`/`top_p` members in Chat JSON/chunks | Response objects have configuration fields and applicable nullable values | No general response settings echo; the native request control surface is independently defined |

Sources: current [Chat create](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create), [Chat chunks](https://developers.openai.com/api/reference/resources/chat/subresources/completions/streaming-events), [Responses create](https://developers.openai.com/api/reference/resources/responses/methods/create) and [Messages create](https://platform.claude.com/docs/en/api/messages/create), corroborated by their SDK response types.

Responses metadata can map exactly to full Chat JSON when valid, but aggregating chunks cannot create a standard Chat SSE metadata slot. Only the already selected null omission is authorized; `{}` and nonempty maps retain their current rejection. Likewise, arbitrary OpenAI metadata must not be relabeled as Messages `user_id`.

The reverse direction has a different design question.

A gateway-owned Response can echo admitted request metadata/configuration in its separate delivery context, but those values are not observations returned by the Chat upstream.

Establish field meaning, precedence and original-versus-prepared request authority; never overwrite an explicit upstream value or claim unreported model-effective settings.

At the review baseline, [completion](../../src/execution/delivery/completion.rs) selected only tools/tool-choice/parallelism, and [envelope writing](../../src/protocol/openai/responses/envelope.rs) preserved reports before filling those missing echoes.

The selected [schema-completion contract](../architecture/adaptation-and-dialects.md#downstream-schema-completion) adds metadata alone; broader settings echo requires its own field authority.

### Reasoning and argument representations

OpenAI [reasoning summaries and encrypted continuity](https://developers.openai.com/api/docs/guides/reasoning) and Anthropic [thinking/encryption](https://platform.claude.com/docs/en/build-with-claude/thinking) have distinct native owners.

Standard Chat has no corresponding reasoning-content/replay carrier; Provider-specific replay extensions do not establish standard support.

Summary text is not interchangeable with an opaque replay value, and no standard contract makes OpenAI encrypted content usable as a Messages signature.

Keeping characters while changing hidden reasoning into an answer changes visibility, not just formatting.

Function argument fragments may be streamed as strings in all three systems, but their complete-value contracts differ: Chat/Responses preserve generated strings, including invalid JSON, whereas complete Messages input is an object.

A valid object string has a value-preserving mapping, subject to existing lexical-normalization/replay rules; an array, invalid JSON or arbitrary custom string does not.

A truncated object prefix can retain incomplete streaming meaning under the selected Messages max-token path, but cannot become a complete JSON response/history object.

Buffering establishes whether a value is complete; it cannot manufacture validity.

**New source clarification:** the current Anthropic [Thinking page](https://platform.claude.com/docs/en/build-with-claude/thinking) explicitly states that thinking deltas are “followed by a single `signature_delta` event just before the block's `content_block_stop`,” for summarized and omitted display.

The reviewed native reducer's concatenation of multiple signature events and its synthetic split fixture were not justified by that documented shape.

SDK assignment behavior does not authorize duplicate events either.

This established a source-validation/fixture correction, distinct from incompatible cross-protocol replay formats.

## Diagnostic extension source policies (2026-10-27)

- OpenAI's [backwards-compatibility policy](https://developers.openai.com/api/reference/overview#backwards-compatibility) expressly permits additional JSON response/event-data properties and streaming event types.
- Anthropic's [versioning policy](https://platform.claude.com/docs/en/api/versioning) permits additional output values and enum-like variants; its [streaming guide](https://platform.claude.com/docs/en/build-with-claude/streaming#other-events) requires graceful handling of unknown events. Its wording is broader, not identical to OpenAI's.

These policies inform the bounded [adaptation notice](../architecture/adaptation-and-dialects.md#public-capability-and-adaptation-notice). Extra fields do not excuse invalid standard members, reordered content or fabricated replay authority, and continuation never requires diagnostic echo.

## Follow-up source and feasibility review (2026-10-28)

Inspected OpenAI Python 3.39.0 / Node 7.45.0 and Anthropic Python 2.3.0 / TypeScript 1.3.0 independently of regression pins. The review supplied these durable distinctions:

- The [Python accumulator](https://github.com/anthropics/anthropic-sdk-python/blob/202db72c87d7c6eaa72129c67ce67fd343a3a20c/src/anthropic/lib/streaming/_messages.py) addresses content by event index.

  The [TypeScript accumulator and callbacks](https://github.com/anthropics/anthropic-sdk-typescript/blob/cb5fba419808e3844fa6407e59577ab7ba346b79/src/lib/MessageStream.ts) differ: the stored snapshot uses indices, but `signature`, `thinking`, `text`, `inputJson` and `contentBlock` callbacks inspect `content.at(-1)`.

  Starting tool block 1 before block 0's signature/stop can therefore suppress its signature callback and attribute its stop to block 1.

  This is source-derived consumer risk; final snapshots alone miss callback misattribution.
- Both Anthropic accumulators use a separate fragment buffer. That choice does not establish a service merge contract for a nonempty tool opener followed by nonempty JSON fragments.
- The [manual-thinking type](https://github.com/anthropics/anthropic-sdk-python/blob/202db72c87d7c6eaa72129c67ce67fd343a3a20c/src/anthropic/types/thinking_config_enabled_param.py) requires at least 1024 thinking tokens and a budget strictly below `max_tokens`. A syntactically valid one-token answer reserve does not establish usefulness; no general budget-increase formula follows.
- Legacy `role:function` results name a function, not a modern call ID. Two outstanding calls with the same name expose an association ambiguity; no automatic legacy adapter is implied.

[Stream normalization](../architecture/runtime-and-routing.md#semantic-stream-normalization) owns the selected bounded scheduler. [Protocol boundaries](../roadmap/unscheduled-issues.md#protocol-boundaries) owns remaining source and budget qualification.

## Binding-history source review (2026-10-28)

Endpoint-qualified source distinctions, not codec flags or blanket Provider guarantees:

| Source | Durable history distinction |
|---|---|
| [SIWC inference](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference) and [limitations](https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations) | Subscription request restrictions and namespace/inline tool alternatives are separate from general OpenAI API semantics |
| [Bailian Responses](https://help.aliyun.com/zh/model-studio/qwen-api-via-openai-responses) | Reasoning input lists ID, summary and optional status, not full-readable or encrypted content; summary-only history is a separate positive case |
| [Bailian Chat](https://help.aliyun.com/zh/model-studio/qwen-api-via-openai-chat-completions) | Qwen accepted carriage and GLM clear-thinking defaults cannot become one dialect-wide history requirement |
| [DeepSeek thinking](https://api-docs.deepseek.com/guides/thinking_mode/) | Rechecked 2026-10-28: thinking requests carrying tools require reasoning from all previous turns, including non-tool turns. Without tools it is ignored |
| [DeepSeek Responses](https://api-docs.deepseek.com/guides/responses_api/) | Plaintext reasoning merges into adjacent assistant content; Chat's history rejection requirement cannot be inherited |
| [Zhipu thinking](https://docs.bigmodel.cn/cn/guide/capabilities/thinking-mode) | The dated review identified current-tool-cycle scope, distinct from DeepSeek all-prior history. The 2026-10-29 refresh described Coding Plan versus standard preserved-thinking defaults but did not independently re-establish that earlier requirement |
| [Grok reasoning](https://docs.x.ai/developers/model-capabilities/text/reasoning) | Encrypted continuation can be emitted without explicit encrypted include; summary presence is not replay authority |
| [Xiaomi Responses](https://mimo.mi.com/docs/zh-CN/api/chat/responses) | Retention recommendations do not alone establish rejection when history is missing |
| [OpenRouter reasoning](https://openrouter.ai/docs/guides/best-practices/reasoning-tokens#preserving-reasoning) | Readable/structured Chat and encrypted Responses carriers do not establish every routed model's omission requirements |
| [NVIDIA hosted schema](https://docs.api.nvidia.com/nim/reference/nvidia-nemotron-3-super-120b-a12b-infer.md) and [Go endpoints](https://opencode.ai/docs/go/#endpoints) | Output examples, self-hosted NIM behavior and endpoint availability do not establish hosted history input or another billing service's contract |

The minimal history discriminator is a reasoning-emitting tool turn plus its result versus a completed answer plus new user input. Syntax acceptance does not establish whether historical reasoning is consumed, ignored or required. [Binding history](../architecture/history-and-replay.md#binding-reasoning-history-contract) owns assessed and unassessed declarations.

### Hosted MiniCPM control recheck (2026-11-13)

The [pinned hosted guide](https://github.com/OpenBMB/MiniCPM-V/blob/4dbbb6c58b6f8a004d6689028417296719f0831a/docs/api.md#minicpm-v-47) documents `chat_template_kwargs:{"enable_thinking":true}` for both MiniCPM-V 4.7 sizes, thinking off by default, readable `message.reasoning`, and image/video input. The [hosted directory](https://platform.modelbest.cn/api/models) independently lists `deep_thinking:true` for both; `show_reasoning:false` is not a thinking-admission flag.

Bounded native wire characterization establishes these current service shapes:

| Hosted models | Default thinking | Activation carrier | Readable JSON/SSE field |
|---|---|---|---|
| `MiniCPM5-1B`, `MiniCPM5-2B` | Enabled | Nested `enable_thinking` Boolean | `reasoning_content` |
| `MiniCPM-V-4.7-1B`, `MiniCPM-V-4.7-35B-A3B` | Disabled | Same Boolean | `reasoning` |

All four accept explicit on/off and the same-name `reasoning_effort:"medium"` alongside enabled thinking.

Boolean activation is independently characterized; qualitative level calibration remains upstream-owned.

The two output aliases have one semantic owner.

Native tool-call/result continuations preserve the returned readable field, while whether history is consumed, ignored or required remains unassessed.

The [MiniCPM5 deployment guide](https://github.com/OpenBMB/MiniCPM/blob/f4293a1a7bde05d1acce99488235792615fc241b/docs/deployment/sglang.md) and model templates are self-hosted references, not authority for hosted history requirements.

## Pre-stage-4 contract qualification (2026-10-29)

The dated native observations established grouped call-ID association on direct DashScope Qwen Flash/Max, TokenPlan Qwen Flash, native MiMo and OpenRouter GPT Luna for the tested inputs. Independent nonce-result comparisons observed both supplied values consumed. This qualifies those layouts only, not arbitrary interleaving, reverse result order or every billing endpoint.

The [Bailian Responses reference](https://help.aliyun.com/zh/model-studio/qwen-api-via-openai-responses) nevertheless says each result must immediately follow its call. It also permits string or array output, while the gateway's narrower string implementation is a separate restriction. [Tool-result association](../architecture/adaptation-and-dialects.md#tool-result-association) owns the selected grouped association; a literal documentation example is not proof of an enforced adjacency rule.

Phase is absent from the documented input union, whose unlisted-parameter rule says ignored, not rejected. The approximate 80%-of-window allowance and automatic truncation also require a distinct long-context qualification; planning metadata does not enforce an input-token cap.

## Semantic-equivalence review (2026-10-28)

Reviewed existing Generation Text request, response, stream and history named-loss paths and empty/default guards at `5611943`. This inventory classifies concrete entry points; it does not activate unsupported standard fields.

| Candidate family | Classification and disposition |
|---|---|
| `metadata:null`; empty output/history logprobs | Already equivalent under the selected policy. Preserve nonempty/null-probability rejection and original observations |
| Null system fingerprint to Responses/Messages; null terminal explanation to Messages | Equivalent: no report value is removed. Keep target-copy normalization, native Chat presence, non-null losses and terminal cause/status. [Pinned Chat type](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/chat/chat_completion.py) types fingerprint as optional-nullable |
| Explicit request `logprobs:false` to Messages | Equivalent disabled intent: no probability carrier is required. `true`, any `top_logprobs` and null remain distinct. Minimal counterexample: ordinary Chat text request with a token cap and `logprobs:false` previously rejected at Messages encoding |
| Empty `prompt_cache_options:{}` on a target without options | Equivalent only for exact `CacheOptions::default()`. Remove the dedicated normalization rule/receipt, not active-control/null rejection or cache-key/retention losses |
| Parallel permission on a trusted always-enabled target | Equivalent permission: keep candidate-local wire omission and explicit serial-intent rejection, remove the loss rule/receipt. [DeepSeek Responses](https://api-docs.deepseek.com/guides/responses_api/) explicitly documents always-enabled parallel calls; the existing selected Chat dialect uses the same trusted rule. No new live Chat behavior is inferred |
| Phase, grouping/envelopes, item status, history scope, tool execution/error wrapping, cache key/identity hints | Genuine semantic facts or selected approximations. Preserve their named losses and identity/source-isolation assertions |
| Usage totals/components, report scope/stage, zero counts, opening reports, requested versus reported settings | Meaningful accounting/lifecycle distinctions, not empty placeholders. Preserve sparse observations, estimates/default provenance, checked arithmetic and completion guards |
| Tool-argument parsing, linked call-ID projection, required schema/envelope completion | Necessary representation/default/derivation, already non-loss categories. Preserve conversions and behavioral assertions |
| Disabled document citations | Equivalent: the [official API reference](https://platform.claude.com/docs/en/api/go/messages) gives `enabled` default false; the [citations guide](https://platform.claude.com/docs/en/build-with-claude/citations) makes document citations opt-in. Omitted/false `enabled` produces an uncited document without loss; true remains unselected and malformed known values reject |
| Null output effort/format | Insufficient evidence: pinned nullable syntax does not define omission/default or conversation-level effort semantics. Keep current rejection; root `output_config:null` is a separate invalid shape |
| Empty/default-only settings or instruction reports; isolated nullable reasoning controls; explicit medium verbosity | Insufficient field-specific default/report evidence. An empty container may still report a scope or clear state; retain current behavior |
| Omitted Responses strictness; auto summary/encrypted include | Conditional target-specific rules selected under S3/S4, not blanket empty-field normalization. Keep their target-specific dispositions |


## Chat JSON intake evidence (2026-10-30)

The Chat limitation is independently defined by the pinned OpenAI Python **3.37.0** [full response](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/chat/chat_completion.py), [message](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/chat/chat_completion_message.py) and
[chunk](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/chat/chat_completion_chunk.py) schemas, rechecked against the installed locked SDK.

Metadata and URL annotations have full-response carriers but no standard chunk carriers.

Service tier and system fingerprint do have chunk carriers; the corrected synthetic wire retains service tier instead of manufacturing another gap.

Chat custom-tool SSE remains its separately retained carrier restriction.

Responses lifecycle events contain complete [Response objects](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/responses/response_completed_event.py); the pinned Vercel JSON/event pair independently carries settings, metadata, storage and usage.

Messages start events contain a [Message](https://github.com/anthropics/anthropic-sdk-python/blob/9313f13a5a643ceed10fafba6785eab8e04b6e6e/src/anthropic/types/raw_message_start_event.py), with terminal reports carried by deltas.

No analogous JSON-only blocker was found in this bounded corpus.

Current [intake policy](../architecture/index.md#protocols) retains Chat's JSON exception while other qualified native targets use SSE.

## Generation null-contract review (2026-11-02)

Scope: Generation settings and shared context owners remaining after the stream-intent/document-context migration. Image-generation-specific fields, routing, credentials and new APIs are outside this review. The source baseline remains OpenAI Python **3.37.0** and Anthropic Python **2.1.0** from [the independent source lock](../../tests/upstream/sources.lock.json); installed test-owned SDK declarations were inspected. This review changes neither source pins nor Agent versions.

Primary evidence:

- OpenAI [Responses create parameters](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/responses/response_create_params.py), [response model](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/responses/response.py),
  [reasoning parameters](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/shared_params/reasoning.py) and [reported reasoning](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/shared/reasoning.py).
- OpenAI [Chat parameters](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/chat/completion_create_params.py) and [reported text configuration](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/responses/response_text_config.py).
- Anthropic [adaptive thinking](https://github.com/anthropics/anthropic-sdk-python/blob/9313f13a5a643ceed10fafba6785eab8e04b6e6e/src/anthropic/types/thinking_config_adaptive_param.py) and [enabled thinking](https://github.com/anthropics/anthropic-sdk-python/blob/9313f13a5a643ceed10fafba6785eab8e04b6e6e/src/anthropic/types/thinking_config_enabled_param.py) explicitly allow nullable display and document a model-dependent default.

| Owner | Null disposition and protected distinctions |
|---|---|
| Reasoning activation / history read | `Option`; no valid native null activation is produced, and selected Chat `preserve_thinking:null` still rejects before IR. Explicit false remains distinct from absence |
| Summary / visibility | `Option`; nullable summary has no summary opt-in and never means hidden display. Nullable Messages display uses the model default; explicit summarized/omitted guarantees remain values |
| Root instructions | `Option`; null and omission supply no new root instructions. The official previous-response contract does not inherit prior root instructions in either case. Empty text and ordered application instructions remain distinct |
| Text container / format | Nullable report defaults normalize; observed `{}` remains a present container, and explicit text/schema formats remain values. Request `text:null` and `format:null` still reject per the non-nullable create types |
| Chat modalities / audio options | Nullable defaults normalize; no default modality/options are invented. Null options cannot satisfy a requested audio modality |
| Cache options | Nullable report defaults normalize. Request null containers and non-nullable mode/TTL/prewarm members still reject; empty default options do not weaken active cache/storage guarantees |
| Completion time / cache diagnostics / comparison count | Null means no observation. Never invent a timestamp/count or completion. Preserve zero and the tagged `unavailable` / `comparison_response_not_found` observations |
| Report metadata | Retain tri-state authority. A request containing `{"tenant":"A"}` followed by an upstream `metadata:null` must publish null, not echo the request map; an explicit empty map also suppresses fallback |
| Reported reasoning object / effective context and mode | Retain explicit-null observations independently of missing reports. Only missing effective settings may take the selected phase-local resolution; request reasoning/context null defaults, while request mode null rejects |

Reported nullability alone is not a request-side clear operation.

In particular, `reasoning.summary:null` does not request `display:"omitted"`.

Scalar normalization does not erase an observed settings snapshot or let one lifecycle phase fill another.

With a sibling reported setting present, `reasoning:null` and an omitted reasoning object remain different observations; likewise an opening effective `context:null` or `mode:null` cannot silently become a known terminal value.

Existing report-mode acceptance follows the pinned Python response contract without claiming identical nullability in every SDK.


## Nine-direction tool assessment (2026-11-04)

The native and cross-protocol source review distinguished availability, model-visible definitions, permitted calls and actual execution. [Projection proposals](../proposals/index.md#three-protocol-tool-conversion-design) own unresolved conversions; ordinary function-loop evidence does not qualify advanced tool families.

### Discriminating counterexamples

| Input distinction | What an insufficient conversion gets wrong |
|---|---|
| `f(V1) → call/result → f(V2)` | Final-set collapse erases V1 visibility; a generic root-plus-inline mapping assumes undocumented collision precedence |
| Deferred `f`, never activated; or `f` removed at the end | Unioning all historical definitions makes a currently unavailable tool callable |
| Root `f,g`, allowed only `f`, optional versus required | Dropping allowed selection permits `g`; changing required to auto permits no call. Filtering can preserve the restriction but loses definition visibility |
| Named Messages tool permitting repetition versus OpenAI forced singleton | Copying a name loses call-count meaning in at least one direction |
| Namespaced `a.read`, `b.read`, then remove `a.read` | Flat-name collisions or current-inventory-only reverse lookup misidentify historical calls |
| Custom input containing quotes, Unicode and partial JSON wrapper bytes | Forwarding target wrapper deltas as source custom input changes bytes or exposes incomplete executable input |
| Old call/result supplied without its old declaration | Synthesizing a schema or applying today's schema invents historical meaning |
| Completed `status:null` input result versus required output terminal status | Global null normalization weakens the wrong boundary; input-only documented normalization remains valid |

## Tool-contract qualification (2026-11-04)

Rechecked the shared owners at **`a22cf0d`** and the [current stable SDK release sources](2026-10-27-official-sdk-sources.md#tool-contract-release-recheck-2026-11-04). This qualifies the first tool package; it does not select a new content loss, activate an endpoint or establish Provider acceptance. The subsequently accepted dispositions live in [Architecture](../architecture/index.md#ordered-tool-availability).

### Primary sources

The ordinary OpenAI page extractor returned 404 and the Claude extractor returned regional-unavailability pages. Direct, bounded reads of the official `.md` endpoints below succeeded. Their actual text, not those failed fetches or search excerpts, supports these observations:

| Primary source | Consequential contract |
|---|---|
| [OpenAI ordered additions](https://developers.openai.com/api/docs/guides/tools-tool-search.md) | Developer `additional_tools` batches become available after their position; manual round-tripping preserves that position. The inspected guide gives no root/inline same-name winner or repeated-definition precedence |
| [SIWC limitations](https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations.md) | Functions/custom tools may use namespaces **or** `additional_tools`. `tool_search` and top-level programmatic calling remain unsupported. Inline direct-call continuation requires its own endpoint qualification; a blanket namespace requirement contradicts the documented declaration alternative |
| [OpenAI function calling](https://developers.openai.com/api/docs/guides/function-calling.md#tool-choice) | Required means one-or-more; forced function means exactly one. Allowed tools constrain calls without removing other definitions. `parallel_tool_calls:false` permits zero or one; combine it with the minimum-call requirement, rather than confusing the two controls |
| [Messages parallel control](https://platform.claude.com/docs/en/agents-and-tools/tool-use/parallel-tool-use.md) | Auto plus disabled parallel use means at most one; named/any plus that flag means exactly one. Parallel use defaults on. The page also excludes named/any choice on several current models, including Sonnet 5.5: protocol carriage alone cannot qualify a model binding |
| [Messages ordered changes](https://platform.claude.com/docs/en/build-with-claude/mid-conversation-system-messages.md) | Same-type by-value definitions replace from their position. References surface deferred or removed tools. Inline-only definitions are accepted with a first-definition cache miss. Content-bearing system messages have constrained positions and cannot separate a tool call from its result |
| [Messages strict schemas](https://platform.claude.com/docs/en/build-with-claude/structured-outputs.md#json-schema-limitations) | Native strict supports local nonrecursive references and a simple regex subset, but does not guarantee string enum/const casing. The combined request limits remain 20 strict tools, 24 optional parameters and 16 union parameters. The inspected pages do not explain charging repeated historical versions against those limits |
| [DeepSeek strict tools](https://api-docs.deepseek.com/guides/tool_calls.md) | Strict requires `https://api.deepseek.com/beta`, every function marked strict and server schema validation. The documented subset excludes string lengths and array item-count constraints. A standard-endpoint capability flag cannot establish this guarantee |

Current Python and TypeScript OpenAI unions agree on the input/output additions role asymmetry. They provide no lawful rule to relabel a returned assistant/tool addition as developer input. Input result `status:null` has an independent normalization basis; call status and output terminal requirements stay strict. Namespace-qualified choice spelling and Chat custom SSE remain separate unresolved boundaries.

The stable Python [input batch](https://github.com/openai/openai-python/blob/9b3458a2ef563493111a0a4265d4e9e7e14fe840/src/openai/types/responses/response_input_item_param.py) requires `type`, developer `role` and a `ToolParam` list, with optional-nullable `id`.

The [output batch](https://github.com/openai/openai-python/blob/9b3458a2ef563493111a0a4265d4e9e7e14fe840/src/openai/types/responses/response_output_item.py) requires a non-null ID and admits unknown/user/assistant/system/critic/discriminator/developer/tool roles.

Neither type imposes a nonempty list or an item status.

The stable [Node event declarations](https://github.com/openai/openai-node/blob/42563fe08083ed7af6feba046063e3100fa7ef50/src/resources/responses/responses.ts) require an output item at both added and done, but do not assert that the added definitions already equal the final list.

This supports a bounded final snapshot rather than invented argument deltas or opener/final equality.

Empty-list service acceptance remains upstream validation.

The pinned Google [v1beta declarations](https://github.com/googleapis/googleapis/blob/1e9cfa78c60620af3c66aba343d99a75a9f79057/google/ai/generativelanguage/v1beta/content.proto) distinguish typed Schema from JSON Schema and restrict nonblocking behavior to BidiGenerateContent.

The pinned Interactions [ordinary call type](https://github.com/googleapis/python-genai/blob/92f72b3a236a5bc56e8e3327ab9563f77dd119c3/google/genai/_gaos/types/interactions/functioncallstep.py) has no signature member.

Ordinary-call signatures require version-specific authority, not analogy with thought signatures.

The pinned Interactions OpenAPI URL returned 404 during the 2026-11-04 review; rendered v1 docs and the SDK were available, but artifact hash equality was not reverified.

## Ordered effort contract qualification (2026-11-05)

Current stable sources were inspected separately from regression pins:

| Source | Release / immutable revision | Qualified definition |
|---|---|---|
| Anthropic Python | `v2.5.0` / `dd18a659419fe4d83aa9109bdcdcf9a35d5c8ddb` | [System message](https://github.com/anthropics/anthropic-sdk-python/blob/dd18a659419fe4d83aa9109bdcdcf9a35d5c8ddb/src/anthropic/types/beta/beta_system_message_param.py), [message output configuration](https://github.com/anthropics/anthropic-sdk-python/blob/dd18a659419fe4d83aa9109bdcdcf9a35d5c8ddb/src/anthropic/types/beta/beta_system_message_output_config_param.py) |
| Anthropic TypeScript | `sdk-v1.5.0` / `3cde979dcebea3450c508f7afc71315be09bf99a` | [`BetaSystemMessageParam` / `BetaSystemMessageOutputConfig`](https://github.com/anthropics/anthropic-sdk-typescript/blob/3cde979dcebea3450c508f7afc71315be09bf99a/src/resources/beta/messages/messages.ts) |
| OpenAI Python | `v3.46.0` / `ea5d1606f441d80dca1e9e7f99b74581f41408f8` | [Configuration update](https://github.com/openai/openai-python/blob/ea5d1606f441d80dca1e9e7f99b74581f41408f8/src/openai/types/responses/response_configuration_update_item_param_param.py) |
| OpenAI Node | `v7.51.0` / `738695092f2d8e9e092de7bd19f0994e0ba68e25` | [`ResponseConfigurationUpdateItemParam`](https://github.com/openai/openai-node/blob/738695092f2d8e9e092de7bd19f0994e0ba68e25/src/resources/responses/responses.ts) |


### Temporal and presence disposition

The official [Messages effort guide](https://platform.claude.com/docs/en/build-with-claude/effort#change-effort-mid-conversation-beta) distinguishes activation from scope: a marker immediately after a user turn with new input sets that turn's reply; elsewhere it waits for the next user turn with new input. Tool-result-only turns do not activate it. Once active, it persists until another change. Root effort remains the initial setting.

The official [Responses reasoning guide](https://developers.openai.com/api/docs/guides/reasoning#change-reasoning-mid-conversation) selects the next response and subsequent responses until replaced, while leaving root effort unchanged.

It requires original update positions on manual replay; reported response effort still describes the request-level setting.

Its example places the update before the next user message.

Pinned [Codex effort recording](https://github.com/openai/codex/blob/6aaac694803004a63c6f393d153c96d22521a6b9/codex-rs/core/src/session/reasoning_effort.rs#L28-L90) and [sampling loop](https://github.com/openai/codex/blob/6aaac694803004a63c6f393d153c96d22521a6b9/codex-rs/core/src/session/turn.rs#L460-L575) support recording an override inside the sampling loop after accepted input, without requiring a fresh user turn.

The exact tool-continuation placement below is inferred from these contracts and implementation, not observed against the service.

| Ordered history prefix | Messages active / pending | Responses effective effort for next response |
|---|---|---|
| Root low → new user input | low / absent | low |
| Append medium update | medium / absent | medium |
| Append assistant tool call | medium / absent | medium |
| Append tool-result-only input | medium / absent | medium |
| Append high update | **medium / high** | **high**, under the next-response contract |
| Append later new user input | high / absent | high |

This is the minimal temporal counterexample: identical effort levels and positions need distinct activation boundaries. `EffortScope::Reasoning` versus `Response` describes what effort controls, not when it activates. A target scope conversion cannot supply the missing distinction.


The [mid-conversation system guide](https://platform.claude.com/docs/en/build-with-claude/mid-conversation-system-messages) permits effort-only empty system messages anywhere, but content-bearing runs retain placement restrictions. Nullable syntax alone does not define a reset: Messages local configuration/effort null and Responses null update effort remained unresolved. Empty content-bearing configuration and an empty message with no control are distinct.

The [preserved-thinking guide](https://platform.claude.com/docs/en/build-with-claude/preserved-thinking) separates root settings from the checked system/tools/messages prefix. Changing root effort can invalidate cache without invalidating thinking; rewriting earlier messages can invalidate later thinking. Appending history preserves earlier protected prefixes.

**Selected 2026-11-06:** explicit `FollowingResponse` / `NewInputTurn` activation boundaries and reuse of the existing message-envelope owner, rather than treating effort scope as timing or postponing the IR extension. [Architecture](../architecture/adaptation-and-dialects.md#ordered-effort-projection) owns the implemented resolver and projection contract; nullable/reset semantics remain field-specific.
