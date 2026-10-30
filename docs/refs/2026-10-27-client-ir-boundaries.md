# Client and IR protocol boundary review

Reviewed on **2026-10-27** at **`1b4f828`**, after the five-slice request repair. This is a source-backed review and capability inventory, not implementation approval, a new work queue or a conformance certificate. [Architecture](../architecture.md#field-handling) owns the clarified rules; [Roadmap](../roadmap.md#standard-request-boundary-repair) owns subsequent scope. The earlier [request review](2026-10-26-standard-request-boundaries.md) retains its original date and repair disposition.

## Contents

- [Scope and evidence](#scope-and-evidence)
- [Resolved boundary counterexamples](#resolved-boundary-counterexamples)
- [Remaining source disagreements](#remaining-source-disagreements)
- [Supported baseline and missing standard capabilities](#supported-baseline-and-missing-standard-capabilities)
  - [Chat Completions](#chat-completions)
  - [Responses](#responses)
  - [Messages](#messages)
- [Open-source bridge cross-check](#open-source-bridge-cross-check)
- [Source-qualified repair disposition](#source-qualified-repair-disposition)
- [Protocol expressiveness check](#protocol-expressiveness-check)
  - [Independent item lifecycle versus overall stopping](#independent-item-lifecycle-versus-overall-stopping)
  - [Custom tools: source disagreement versus a real carrier gap](#custom-tools-source-disagreement-versus-a-real-carrier-gap)
  - [Metadata and configuration reports are direction-specific](#metadata-and-configuration-reports-are-direction-specific)
  - [Reasoning and argument representations](#reasoning-and-argument-representations)
  - [Selected follow-up disposition](#selected-follow-up-disposition)
- [Diagnostics and approval boundaries](#diagnostics-and-approval-boundaries)
- [Follow-up source and feasibility review (2026-10-28)](#follow-up-source-and-feasibility-review-2026-10-28)
- [Agent-default production probe (2026-10-28)](#agent-default-production-probe-2026-10-28)
- [Conversion-ladder inventory (2026-10-30)](#conversion-ladder-inventory-2026-10-30)
- [Production corpus and intake experiment (2026-10-30)](#production-corpus-and-intake-experiment-2026-10-30)

## Scope and evidence

Covered standard Chat Completions, Responses and Messages: public request/history intake, envelope and execution-context admission, semantic ownership, downstream JSON/SSE rendering, final snapshots and returned history. Pure closed request decoding is not the public Client decoder. Provider dialects, upstream model effectiveness, deployment, credential lifecycle and unrelated APIs were not audited. Representable IR, codec support, public-model admission and an activated service operation are different claims.

The source baseline remains [sources.lock.json](../../tests/upstream/sources.lock.json): OpenAI Python **3.37.0** (`e8a0fd28bbdc1699c97ff0f602b1af3dd378382d`), Node **7.43.0** (`d02d4de77ea7a6f82325c02e6f44deb34ee3992c`), Anthropic Python **2.1.0** (`9313f13a5a643ceed10fafba6785eab8e04b6e6e`) and TypeScript **1.1.0** (`5badc765f17d373405f300ee8e69e7f490b1f172`). Installed SDK source files were inspected alongside these pins. Mutable official reference pages were rechecked on the review date; discrepancies are listed separately rather than silently replacing the baseline.

Primary source entry points:

- OpenAI Python [Chat parameters](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/chat/completion_create_params.py), [Chat history](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/chat/chat_completion_assistant_message_param.py), [Responses parameters](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/responses/response_create_params.py), [input union](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/responses/response_input_item_param.py) and [output union](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/responses/response_output_item.py).
- OpenAI Node [Chat definitions](https://github.com/openai/openai-node/blob/d02d4de77ea7a6f82325c02e6f44deb34ee3992c/src/resources/chat/completions/completions.ts) and [Responses definitions](https://github.com/openai/openai-node/blob/d02d4de77ea7a6f82325c02e6f44deb34ee3992c/src/resources/responses/responses.ts). Required nullable members must not be confused with optional members or Python model defaults; SDK-derived `output_text` is not a required wire field.
- Anthropic [create parameters](https://github.com/anthropics/anthropic-sdk-python/blob/9313f13a5a643ceed10fafba6785eab8e04b6e6e/src/anthropic/types/message_create_params.py), [message union](https://github.com/anthropics/anthropic-sdk-python/blob/9313f13a5a643ceed10fafba6785eab8e04b6e6e/src/anthropic/types/message_param.py), [content union](https://github.com/anthropics/anthropic-sdk-python/blob/9313f13a5a643ceed10fafba6785eab8e04b6e6e/src/anthropic/types/message_content_param.py), [tool union](https://github.com/anthropics/anthropic-sdk-python/blob/9313f13a5a643ceed10fafba6785eab8e04b6e6e/src/anthropic/types/tool_union_param.py) and separate [beta types](https://github.com/anthropics/anthropic-sdk-python/tree/9313f13a5a643ceed10fafba6785eab8e04b6e6e/src/anthropic/types/beta).
- Current official [Chat create](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create), [Chat streaming](https://developers.openai.com/api/reference/resources/chat/subresources/completions/streaming-events) and [Responses create](https://developers.openai.com/api/reference/resources/responses/methods/create) references.

Evidence combined source traces, public codec/adapter probes with synthetic input or independently constructed IR, and `Attempt`/`ResponseDelivery` contrasts. No live Provider calls accompanied the review. SDK-derived nullability contrasts were checked against the service reference; `user:null` remained a source disagreement rather than a confirmed defect. Temporary probes and later permanent regressions are distinct evidence.

The inventory compares all create-request root fields and the relevant nested content/tool/history/output/event families. Unsupported discriminated families are grouped: rejecting a family also leaves its nested options unsupported. This is not an assertion that every value/combination has been executed or every future field discovered.

## Resolved boundary counterexamples

The original review used synthetic public codec/delivery probes at `1b4f828`; the following are source distinctions, not an open defect list. The subsequent repair is owned by [Architecture](../architecture.md#conversion-ladder).

| Minimal counterexample | Durable distinction |
|---|---|
| Parts receive `p1:A, p2:B, p1:C`; final parts are `AC, B` | Arrival-order forwarding yields `ABC`, while part-order materialization yields `ACB`. Grouping/phase loss does not authorize text reordering |
| Adding only a message phase makes adjacent `arguments:"not-json"` reject | Phase removal must not introduce object-JSON validation into Chat's generated string-argument contract |
| Unsigned reasoning becomes `thinking` with an empty signature | Readable text cannot manufacture native replay authority; completed output must also form valid native history |
| Required nullable fields disappear from Chat/Responses snapshots or tool echoes | Use explicit null where the response contract requires it; never guess sampling settings or failure causes |
| Input-only `ConfigurationUpdate` reaches Responses output | Input/history/output unions are distinct, even when they share semantic owners |
| `ProgramOutput` has no in-progress status | Buffer until actual closure and emit a lawful added/done pair; do not invent completion or infer that no added-event carrier exists |
| Foreign error code/incomplete reason crosses unchanged into a standard response | A semantic string is not necessarily a valid target enum; enforce the target domain |
| Chat assistant shell lacks both content and calls; Responses mixes input/output parts | Validate a whole request/history branch, not independent permissive fields |
| Chunk `index` or SDK convenience fields alter complete-message intake | Unknown nonsemantic extras must not acquire request meaning |
| Nullable report logic accepts non-nullable request values | Check request nullability independently, including complete history logprob members |
| Messages tool-choice extra rejects; unsupported attribution headers disappear | Ignore unknown object extras, but reject recognized unsupported HTTP semantics before body polling |
| Stable Messages accepts beta-only controls without a Client selector | Stable Client admission and trusted native beta capability are separate boundaries |

[Request rules](../architecture.md#standard-request-boundary), [stream normalization](../architecture.md#semantic-stream-normalization) and [schema completion](../architecture.md#downstream-schema-completion) own current behavior. Source IR remains immutable; no repaired boundary activates hosted tools or another API.

## Remaining source disagreements

- **`user:null`:** pinned Python request types and current OpenAI reference differ; do not tighten solely from one SDK.
- **Chat response `message.refusal`:** Node required-nullable and reference optional-nullable differ. This is distinct from required `choice.logprobs`.
- **Custom Chat SSE:** Node declares/accumulates custom deltas; Python and the streaming reference remain function-only. See the [detailed comparison](#custom-tools-source-disagreement-versus-a-real-carrier-gap).
- **Legacy token ceilings:** `max_tokens` conversion exists, but deprecated versus visible-plus-reasoning ceiling semantics still require domain-specific equivalence evidence.
- **Responses image detail:** generated types require it while prose supplies default `auto`; omission alone is not a confirmed violation.
- **Other source drift:** Python/Node Responses `async` carriers and newer per-breakpoint TTL declarations need explicit source/version review, not silent baseline promotion.

The earlier hypotheses about `error.code:null` did not survive valid-IR construction and are not retained as defects. Private extras now follow the selected ignore-without-execution policy; stable Messages beta-only fields and unsupported attribution/workspace headers reject. These are resolved policy boundaries, not remaining source disagreements.

## Supported baseline and missing standard capabilities

The tables retain the 2026-10-27 capability-family review, with corrected dispositions for the resolved boundaries above. They are not a continuously maintained support matrix or a demand to implement every family. Explicit rejection is compliant with the accepted bounded implementation rule. A supported carrier may still be unavailable on a particular public model. Already approved projections remain scoped decisions, not newly approved losses.

### Chat Completions

The normal text/tool loop already represents model/messages, all ordinary roles, participant names, function/custom definitions and calls/results, parallel/tool-choice controls, sampling/penalties/seed, reasoning effort, response format/verbosity, prediction text, scalar output limits, metadata/identity, service tier, cache options, stream options, usage/probabilities, and selected image/audio/file carriers.

| Missing or restricted standard surface | Current disposition | Later IR-review relevance |
|---|---|---|
| Legacy `functions`, `function_call`, `role:function`, response/SSE `function_call` | Nonempty definitions/selection/history reject; empty definitions are inert. No legacy delivery style | Most function meaning already exists. Mapping must establish name/call association, single-call semantics, defaults/conflicts and return style; do not add an opaque legacy bag |
| `n` greater than one | Reject; one choice only | Response multiplicity needs a deliberate task/output contract, not duplicate stream emission |
| `logit_bias` | Reject | Missing token-ID bias control; token vocabulary/model scope must be explicit |
| `web_search_options` and nested location/context size | Reject | Search configuration/report families; not just a root string option |
| `moderation` policy and response/chunk reports | Reject active standard semantics | Safety-policy and reported assessment ownership, independent of gateway error formatting |
| `store:true` and stored-completion operations | Reject/unavailable | Execution/resource lifecycle, not model content IR |
| `prompt_cache_breakpoint` on content and prediction | Unsupported locations reject; no implicit prediction exception | Existing cache owners cover some related meanings; exact use/TTL support is incomplete |
| Custom `audio.voice:{id:...}` | Reject; string voices and six format labels represented | Explicit custom voice reference and authority, not coercion into a built-in label |
| File IDs and some inline-file forms | File IDs reject; inline reader requires a data URL rather than arbitrary bare-base64 text | Resource identity/provenance and media-type interpretation; not authorization for Provider-owned file-ID forwarding |
| Custom-tool streaming and streaming citations | Reject; custom streaming has the source disagreement above | Often a missing codec/event carrier rather than missing custom-call/citation IR |
| Mixed text/refusal or multiple output parts without an approved projection | Some layouts reject; plain-text grouping preserves source order; richer grouping remains restricted | Existing part owners are present; prioritize lawful output representation before changing semantic owners |

`max_tokens` is already recognized. Standard Chat does not acquire Provider reasoning/video controls merely because another protocol supports them. Output-format/model restrictions and catalog admission must not be reported as absent parser fields.

### Responses

The ordinary path already represents easy/input/output message history, instructions, lifecycle/phase, function/custom tools and namespaces, raw arguments/results, reasoning summary/content/replay and controls, compaction/program/configuration history, text formats, sampling, token/tool-call caps, metadata/identity/service/cache hints, stream options and selected annotations/media. `store` and `previous_response_id` use local managed history; create/retrieve/delete/input-items exist separately from stateless codec parsing. Cache diagnostics have a typed report path and are not wholly absent.

| Missing or restricted standard surface | Current disposition | Later IR-review relevance |
|---|---|---|
| `access_programs.cyber` | Unsupported request/report semantics reject; request nullability is checked separately | Entitlement/execution intent and reported effective selection, not a Provider-only spelling |
| `background:true`, cancel/resume/streamed retrieval | Unavailable; false/null do not implement background execution | Execution lifecycle; separate from generation-content IR |
| `conversation`, prompt/template ID/version/variables | Active values reject | Managed resource/input-resolution semantics; existing local previous-response history is not Conversations support |
| Nonempty `context_management` / automatic compaction controls | Reject | Context transformation intent; opaque compaction history alone does not implement it |
| `moderation` and `error.misalignment` detail | Non-null semantics reject | Safety policy/reports, review target and continuation authority require explicit owners |
| Additional `include` selections | File/web-search results/sources, code-interpreter outputs, input-image URL and computer screenshot expansion reject; encrypted reasoning and output-text logprobs represented | Requested report/content selection, not just optional diagnostics |
| Hosted tool definitions | File search; computer and computer-preview; web search/preview; MCP; code interpreter; programmatic-tool definition; image generation; local shell/shell; tool search; apply-patch reject | Each discriminated family and its nested filters, approvals, environments/containers/network policy and operation settings is unsupported. Definition support, history and execution remain separate |
| Hosted history/output variants | Corresponding search/computer/interpreter/image/shell/patch/MCP items; tool-search call/output; additional-tools; compaction-trigger; item-reference reject | Typed operation/result/status/resource meanings, not vendor JSON bags |
| Hosted tool-choice variants | Choices other than supported function/custom/allowed subsets reject | May reuse selected tool identity when the tool family is represented |
| File-ID image/document references | Reject; URL/data-URL carriers represented | Trusted resource provenance/lifecycle |
| File citation and file-path annotation-added SSE | File-citation rejects pending index semantics; static file-path and selected URL/container-file annotations represented, file-path stream event rejects | Separate missing coordinate meaning from missing event encoding |
| Hosted output events/audio deltas | Unsupported; ordinary text/refusal/function/custom/reasoning/program/compaction events have carriers subject to the findings above | Reuse existing owners where possible; standard input video is not a missing field in the selected input union |
| Compact/count/Files/Conversations operations | Separate unimplemented endpoints | Do not inflate a Client generation field gap into implicit API activation |

### Messages

The ordinary path already represents root/system instructions, text/images/selected documents, tool definitions and input examples, tool-use/results, error flags, direct caller mode, output-format/effort settings, manual/adaptive/disabled thinking, signed/redacted history, cache controls, stops, service tier and selected usage/refusal details. Public tool-turn grammar and active manual-thinking checks were repaired previously; this review does not reopen those fixes. Stable `system` messages exist in the pinned union; the beta-only per-message controls are distinct.

| Missing or restricted standard surface | Current disposition | Later IR-review relevance |
|---|---|---|
| `thinking.type:between_tools` | Reject | Distinguish disabled hidden reasoning with visible tool-progress thinking from enabled reasoning; not automatically equivalent to `disabled` |
| `max_tokens:0` prewarming | Reject; positive cap required | Cache/execution operation, not zero-length normal generation |
| `diagnostics.previous_message_id` | Non-null request rejects; official diagnostics output has no supported owner | Official diagnostics intent is distinct from optional MorphieCore notices |
| `container`, `inference_geo`, `speed` and related reports | Active requests reject; Client report coverage absent | Execution context/placement/service facts, not plain text settings |
| `anthropic-user-profile-id`, `anthropic-workspace-id` | Unsupported headers reject before body polling | Trusted attribution/tenancy policy outside task content; default rejection until selected |
| Programmatic `allowed_callers` and non-direct caller histories | Reject; direct/omitted subset supported | Existing common call-context owners may help, but this is not program execution support |
| Hosted/versioned tools | Bash; memory; text editor; code execution; browser/computer toolsets; web search/fetch; BM25/regex tool search reject | Family rejection includes version-specific domains, use/token caps, location, URL policies, browser/computer options and result controls |
| Hosted content/history | Search results, server-tool use/results, container upload; nested result search/tool-reference/browser-state variants reject | Content/result/resource meanings and terminal progress, not passthrough |
| Document sources | File-ID and nested `content` document sources reject; base64/URL PDF and plain text supported | Resource ownership and structured document composition |
| Citation configuration/locations/events | Enabled document citations, nonempty text citations and citation events reject; disabled document configuration now follows the [equivalence review](#semantic-equivalence-review-2026-10-28) | Full citation location/index ownership and Client JSON/SSE representation remain unselected |
| `pause_turn` and server-tool continuation | No selected Client output carrier/loop | Existing interaction-progress distinctions need assessment before new IR variants |
| Usage geography/speed and beta accounting | Geography/speed lack Client coverage; beta iteration/fallback-credit accounting unavailable | Sparse accounting/report ownership, not invented aggregate counts |

**Beta-only, not stable-field omissions:** context management/compaction, MCP configuration, fallbacks/credit tokens, task budgets, toolset dispatch, advisor/MCP/compaction/fallback blocks and display `updates` remain unsupported or narrowly excepted. Block binding, per-message effort and inline tool changes remain trusted-native capabilities, not stable Client admission. Removed sampling fields are recognized and rejected; that is not missing support for the selected stable API.

## Open-source bridge cross-check

The initial **2026-10-27** review inspected LiteLLM `cfe0100e1fca613e7b1af27dd1fe4807ca4d1d1a` and CLIProxyAPI `1f917e77c4562623aa49e9804d999524151a2f96`. The [release-qualified client/proxy guide](2026-10-27-client-and-proxy-sources.md) now owns navigation, licenses and mechanism cautions. It does not change those earlier observations or select a point-to-point gateway architecture.

The additional [OpenAI Agents Python handler](https://github.com/openai/openai-agents-python/blob/b040da59037b1a34fee3a0dd4da95a571460eb98/src/agents/models/chatcmpl_stream_handler.py) shows stable announced indices and buffered call fragments. It is an Agent event adapter, not a complete Responses HTTP/SSE gateway; placeholder IDs are not standard-wire precedents.

## Source-qualified repair disposition

The **2026-10-27** corrections use existing owners: preserve source text order and raw arguments; enforce required nullable/output unions; separate stable intake from native beta carriers; require genuine Messages signatures; and reject unsupported HTTP controls. [Architecture](../architecture.md#field-handling) owns these decisions; [Development](../development.md#verification-gates) owns acceptance.

Additional source distinctions retained from the repair:

- The [OpenAI function-calling guide](https://platform.openai.com/docs/guides/function-calling) binds optional `id`, `type` and `function.name` independently while appending arguments. Hold unresolved calls in bounded codec-local state; contradictory/missing identity must not become a fabricated call ID.
- The [Messages streaming guide](https://platform.claude.com/docs/en/build-with-claude/streaming) and [pinned accumulator](https://github.com/anthropics/anthropic-sdk-python/blob/9313f13a5a643ceed10fafba6785eab8e04b6e6e/src/anthropic/lib/streaming/_messages.py) distinguish complete opener input, zero-byte deltas and nonempty partial JSON. Whitespace/truncation cannot become `{}`. A nonempty opener followed by nonempty fragments remains unsupported without an established merge/replacement contract.
- Strict JSON/framing validation precedes unknown-event filtering. Valid unknown Messages events do not supply lifecycle evidence.
- Arbitrary cross-protocol block interleaving, annotated grouping and broader settings echo remain separate choices. Null-metadata omission and the narrow lifecycle projection below do not generalize to those cases.

## Protocol expressiveness check

Checked on **2026-10-27** against **`c8d0c96`**, current official references and pinned SDK contracts. Additional source inspection used OpenAI Python [v3.39.0](https://github.com/openai/openai-python/releases/tag/v3.39.0), revision `f97798bc86e41f4a5d18644113ef94cf58722778`, and Node [v7.45.0](https://github.com/openai/openai-node/releases/tag/v7.45.0), revision `af8336b8c13bd5402868cf2ca0d9c64530c3848f`. These October 26 release candidates for our baseline were inspected, not installed or promoted to regression pins. This is analysis, not approval for additional omissions, wrapping or inferred state.

### Independent item lifecycle versus overall stopping

Responses has separate output-item status and response status carriers. Its [output-message type](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/responses/response_output_message.py) and [response type](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/responses/response.py) do not require every item's status to equal the enclosing response's status. The [streaming reference](https://developers.openai.com/api/reference/resources/responses/streaming-events) separately defines item-done, response-incomplete and response-failed events. Schema expressiveness is not proof that a particular live model emits every combination.

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

**Intrinsic difference:** the explicit per-item lifecycle vector has no standard Chat/Messages carrier. **Implementation policy:** [Chat intake](../../src/protocol/openai/chat/response.rs) infers incomplete item states from `length`/`content_filter`, and [lowering](../../src/protocol/openai/lowering/response.rs) rejects a source that cannot round-trip through that inference. Equality of source item and overall status is not a Chat wire requirement.

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

The reverse direction has a different design question. A gateway-owned Response can echo admitted request metadata/configuration in its separate delivery context, but those values are not observations returned by the Chat upstream. Establish field meaning, precedence and original-versus-prepared request authority; never overwrite an explicit upstream value or claim unreported model-effective settings. At the review baseline, [completion](../../src/execution/delivery/completion.rs) selected only tools/tool-choice/parallelism, and [envelope writing](../../src/protocol/openai/responses/envelope.rs) preserved reports before filling those missing echoes. The subsequent [selection below](#selected-follow-up-disposition) adds metadata alone; broader settings echo remains unselected.

### Reasoning and argument representations

OpenAI [reasoning summaries and encrypted continuity](https://developers.openai.com/api/docs/guides/reasoning) and Anthropic [thinking/encryption](https://platform.claude.com/docs/en/build-with-claude/thinking) have distinct native owners. Standard Chat has no corresponding reasoning-content/replay carrier; an explicitly selected private profile is not standard support. Summary text is not interchangeable with an opaque replay value, and no standard contract makes OpenAI encrypted content usable as a Messages signature. Keeping characters while changing hidden reasoning into an answer changes visibility, not just formatting.

Function argument fragments may be streamed as strings in all three systems, but their complete-value contracts differ: Chat/Responses preserve generated strings, including invalid JSON, whereas complete Messages input is an object. A valid object string has a value-preserving mapping, subject to existing lexical-normalization/replay rules; an array, invalid JSON or arbitrary custom string does not. A truncated object prefix can retain incomplete streaming meaning under the selected Messages max-token path, but cannot become a complete JSON response/history object. Buffering establishes whether a value is complete; it cannot manufacture validity.

**New source clarification:** the current Anthropic [Thinking page](https://platform.claude.com/docs/en/build-with-claude/thinking) explicitly states that thinking deltas are “followed by a single `signature_delta` event just before the block's `content_block_stop`,” for summarized and omitted display. The reviewed native reducer's concatenation of multiple signature events and its synthetic split fixture were not justified by that documented shape. SDK assignment behavior does not authorize duplicate events either. This established a source-validation/fixture correction, distinct from incompatible cross-protocol replay formats.

### Selected follow-up disposition

Synthetic probes at `c8d0c96` confirmed independent item/response status, Chat metadata JSON/SSE asymmetry and the complete-object versus string-argument boundary. These were not live Provider observations.

On **2026-10-27**, the user selected narrow plain-text/token-cap item-status omission and original-request Responses metadata echo with whole-field report precedence and strict-complete rejection. [Architecture](../architecture.md#conversion-ladder) owns the lifecycle policy; [schema completion](../architecture.md#downstream-schema-completion) owns metadata provenance. Messages accepts one opaque native signature delta; repeated deltas reject rather than concatenate.

The selection preserves source item statuses and reported metadata, does not make incomplete tools executable, and does not infer Provider-effective settings. Custom Chat SSE, Messages custom-input wrapping and broader settings echo remain unselected. [Development](../development.md#verification-gates) records verification separately.

## Diagnostics and approval boundaries

Official evidence does **not** justify rejecting every additional response property:

- OpenAI's [backwards-compatibility policy](https://developers.openai.com/api/reference/overview#backwards-compatibility) expressly permits “Adding new properties to JSON response objects or event data” and new streaming event types.
- Anthropic's [versioning policy](https://platform.claude.com/docs/en/api/versioning) permits additional output values and enum-like variants; its [streaming guide](https://platform.claude.com/docs/en/build-with-claude/streaming#other-events) requires graceful handling of unknown event types. Its field wording is broader, not identical to OpenAI's.

The current bounded [notice attachment](../../src/protocol/diagnostics.rs) uses extra object properties in existing envelopes, does not overwrite standard fields, and removes the optional notice if the complete payload exceeds its budget. JSON/SSE continuation does not require a notice echo. No new concrete violation was established merely from the `morphiecore` field's existence. SDK retention and diagnostic aggregation remain deferred, not certified. These notices cannot cure unsigned thinking, reordered text, an invalid output union or absent required standard members.

Previously approved usage completion, phase/report omission, reasoning concatenation, refusal-as-text and terminal coarsening remain named policies to assess against the clarified output contract. They are not new loss approvals. New legacy-field conversions must distinguish exactness from changed meaning and obtain user confirmation for loss before implementation.

## Follow-up source and feasibility review (2026-10-28)

Rechecked implementation **`1e3fdca`**, including `c8d0c96` and `14bddfe`, against public release checkouts: OpenAI Python **3.39.0** (`f97798bc86e41f4a5d18644113ef94cf58722778`), Node **7.45.0** (`af8336b8c13bd5402868cf2ca0d9c64530c3848f`), Anthropic Python **2.3.0** (`202db72c87d7c6eaa72129c67ce67fd343a3a20c`) and TypeScript **1.3.0** (`cb5fba419808e3844fa6407e59577ab7ba346b79`). Service references were rechecked on this date. No test pins, dependencies, bindings or runtime behavior changed. [Development](../development.md#verification-gates) records executed probes; [Roadmap](../roadmap.md#remaining-issue-dispositions) owns scheduling and reopening conditions.

### Messages scheduling evidence

The [streaming guide](https://platform.claude.com/docs/en/build-with-claude/streaming) describes indexed start/delta/stop blocks and shows sequential examples; this review did not establish an explicit universal prohibition of overlapping block lifetimes. The local native reducer accepts one open block at a time. That admission contract must not be broadened merely to make current encoder output pass.

| Synthetic source prefix | Observed local behavior |
|---|---|
| Start text owner and part; emit `A`; start a tool owner | The shared semantic reducer and downstream encoder accept it. Native Messages intake rejects the emitted second `content_block_start` because the text block is still open |
| Start text owner; start tool owner; announce the earlier owner's first text part | The semantic reducer accepts it; the downstream encoder rejects the delayed part after publishing the later tool opener |
| Tool opener `input:{"a":1}` followed by complete `partial_json:"{\"b\":2}"` | Native intake rejects at the fragment, independently of truncated/invalid JSON |

Responsible code: [downstream encoder](../../src/protocol/anthropic/messages/client/sse.rs), [native reducer](../../src/protocol/anthropic/messages/sse/wire.rs), and [semantic owner reduction](../../src/semantic/task/generation/stream/reduce/items.rs). Existing ordering tests cover prefixes but do not prove exclusive block lifetime or intermediate consumer behavior.

The [Python accumulator](https://github.com/anthropics/anthropic-sdk-python/blob/202db72c87d7c6eaa72129c67ce67fd343a3a20c/src/anthropic/lib/streaming/_messages.py) addresses content by event index. The [TypeScript accumulator and callbacks](https://github.com/anthropics/anthropic-sdk-typescript/blob/cb5fba419808e3844fa6407e59577ab7ba346b79/src/lib/MessageStream.ts) differ: the stored snapshot uses indices, but `signature`, `thinking`, `text`, `inputJson` and `contentBlock` callbacks inspect `content.at(-1)`. Starting tool block 1 before block 0's signature/stop can therefore suppress its signature callback and attribute its stop to block 1. This is source-derived consumer risk, not an executed SDK assertion; final snapshot checks alone miss it.

For the next implementation, the existing [OpenAI event projection](../../src/protocol/openai/lowering/native_response/events.rs) provides a local pattern for owner barriers, ordered parts and blocked-fragment accounting. [CLIProxyAPI's bridge](https://github.com/router-for-me/CLIProxyAPI/blob/1f917e77c4562623aa49e9804d999524151a2f96/internal/translator/openai/claude/openai_claude_response.go) also serializes tool delivery, but its complete-argument buffering, JSON repair and fallback identities are unsuitable precedents.

The review recommended codec-local owner/part serialization with incremental head delivery and bounded successor storage. The implemented follow-up is owned by [semantic stream normalization](../architecture.md#semantic-stream-normalization); [Development](../development.md#sdk-loopback-gate) records the actual Python/TypeScript callback and history checks. Delayed successors remain necessary when earlier content or signatures have not finished.

The scheduling review exposed two pre-existing boundaries. The subsequent 2026-10-28 lifecycle repair closes remaining wire text blocks on an observed `ItemFinished(Incomplete)`, matching existing static and terminal-only delivery without completing source parts. Required Responses `metadata:null` also caused setup rejection through actual Attempt/ResponseDelivery. The user separately selected Messages JSON/SSE null-only omission on 2026-10-28; [Architecture](../architecture.md#conversion-ladder) owns that rule, not an implicit extension of Chat's omission. [Development](../development.md#sdk-loopback-gate) owns lifecycle, metadata and SDK/history evidence. Maps and other unsupported execution hints remain in [Roadmap](../roadmap.md#remaining-issue-dispositions).

### Budget feasibility evidence

- The [Anthropic manual-thinking type](https://github.com/anthropics/anthropic-sdk-python/blob/202db72c87d7c6eaa72129c67ce67fd343a3a20c/src/anthropic/types/thinking_config_enabled_param.py) requires at least `1024` tokens and a budget strictly below `max_tokens`. [Client intake](../../src/protocol/anthropic/messages/client/request.rs) enforces this before adaptation. Relaxing a later target encoder cannot rescue invalid source requests.
- [Generic settings validation](../../src/semantic/task/generation/request/settings.rs) enforces numeric budget no greater than output ceiling. [Effort conversion](../../src/protocol/reasoning.rs) validates both source and derived settings. The existing synthetic `xhigh` preset of `16384` with an explicit `4096` ceiling rejects after derivation; that is a policy edge, not evidence of a live model failure.
- [Messages defaults](../../src/protocol/anthropic/messages/target.rs) apply on the candidate copy after selected reasoning conversion. Existing native-control tests distinguish `2048` thinking with a default ceiling of `2048` (reject) from `2049` (syntactically valid). One answer token does not establish a useful reserve.
- [Go/Haiku](../../src/topology/catalog/messages.rs) already selects `32000` and disables manual thinking. [Startup](../../src/gateway/startup/models.rs) currently exposes configured effort-to-budget conversion only for generateContent. No generally applicable fix follows from raising the Haiku default or deleting the semantic guard.
- The follow-up catalog/example review found no qualified active conflict. [Startup regression](../../src/gateway/startup/bootstrap/tests.rs) preserves native medium effort and explicit output `6000` even with a configured preset of `7000`. The [advanced example](../../config/models.advanced.example.json) selects budget-to-effort for Haiku, not a numeric thinking allocation under its smaller output default. Private deployment activation was not inspected; [Roadmap](../roadmap.md#remaining-issue-dispositions) records deferral and target-specific reopening requirements.
- [Model limits](../../src/protocol/model_limits.rs) distinguish output maxima from defaults; context/input capacities are planning metadata, not input-token enforcement. Unknown maxima cannot justify unlimited expansion. [Independent-ceiling alternatives](../api-expansion-design.md#independent-output-ceilings-unselected) retain the reply-versus-total counterexample and the smaller target-qualified candidates.

### Remaining source distinctions

The inspected [Node Chat types](https://github.com/openai/openai-node/blob/af8336b8c13bd5402868cf2ca0d9c64530c3848f/src/resources/chat/completions/completions.ts) and [accumulator](https://github.com/openai/openai-node/blob/af8336b8c13bd5402868cf2ca0d9c64530c3848f/src/lib/ChatCompletionStream.ts) support custom deltas; [Python chunks](https://github.com/openai/openai-python/blob/f97798bc86e41f4a5d18644113ef94cf58722778/src/openai/types/chat/chat_completion_chunk.py) and the [streaming reference](https://developers.openai.com/api/reference/resources/chat/subresources/completions/streaming-events) remain function-only. Node's custom-chunk test uses a mock producer, not a real server capture. Keep upstream ingestion, downstream publication and history compatibility as separate claims.

The [Chat create reference](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create) permits nullable request `user` and optional-nullable output refusal, while inspected SDK declarations differ. Do not tighten intake from annotations alone. Emitting complete-response `refusal:null` is a possible compatibility normalization, not a newly confirmed defect or a reason to densify SSE deltas. The [Responses image parameter](https://github.com/openai/openai-python/blob/f97798bc86e41f4a5d18644113ef94cf58722778/src/openai/types/responses/response_input_image_param.py) still marks detail required while documenting default `auto`; omission remains distinct from null. Its newer breakpoint TTL field does not silently promote regression pins, and sampled function/custom `async` fields already existed at the pin.

Both Anthropic accumulators replace tool input from a separate fragment buffer; that implementation choice does not select a service-defined merge/replacement contract for a nonempty opener. The existing permanent negative test uses `"{"`; its next relevant extension should use complete `"{\"b\":2}"` to isolate this boundary from truncation.

Legacy `role:function` results identify a name rather than a modern call ID; two outstanding same-name calls expose ambiguous association. Broader settings echo also needs field-specific authority: requested `service_tier:"auto"` is not the actual served tier described by the [Responses type](https://github.com/openai/openai-node/blob/af8336b8c13bd5402868cf2ca0d9c64530c3848f/src/resources/responses/responses.ts). Neither case justifies an automatic adapter or blanket request echo.

## Agent-default production probe (2026-10-28)

Rechecked implementation **`b5094d6`** with disposable, uncommitted probes; no Provider calls. [Roadmap](../roadmap.md#agent-default-production-findings) owns the resulting dispositions and order; selections remain pending.

### Method

- **Production construction:** a crate-internal probe loaded the gateway through the startup fixture with real catalog bindings, a test-owned credential store and per-candidate `base_url` overrides to a loopback fake upstream. Each case used a fresh gateway with body-free diagnostics; failure codes come from the request record. Bindings: `dashscope-glm-chat`, `deepseek-chat`, `openrouter-responses` (`gpt-6-luna`), `dashscope-flash-responses` (`qwen3.8-flash`) and `opencode-go-haiku-messages`. Google bindings were not probed ([pause](../roadmap.md#google-api-pause)).
- **Request shapes:** installed pi 1.4.0 defaults per API, read from its bundled `@earendil-works/pi-ai` dist (`openai-responses`, `openai-completions`, `anthropic-messages`): Responses `developer` items without `type`, tools without `strict`, `reasoning:{effort,summary:"auto"}` plus `include:["reasoning.encrypted_content"]`, `prompt_cache_key`; Chat `reasoning_effort`, `stream_options`; Messages `cache_control` on system/last user block/last tool, `eager_input_streaming`, manual `thinking`. pi's replay transforms (reasoning items verbatim, rebuilt assistant messages, `function_call` without `status`) built second turns from the gateway's own delivered output; a verbatim-output replay was checked alongside.
- **Upstream shapes:** official-shape streams per protocol, including required `logprobs:[]` on Responses text events ([`response_text_done_event.py`](https://github.com/openai/openai-python/blob/f97798bc86e41f4a5d18644113ef94cf58722778/src/openai/types/responses/response_text_done_event.py)).
- **Cross-product codec probe:** every `testdata/repair-and-recall` upstream corpus through every client encoder (JSON/SSE), and every client request corpus through every target encoder. Its adapters used test contracts; only findings reproduced through production construction or exact code traces are recorded below.

Probe artifacts excluded from findings: Messages-target admission without catalog `explicit_cache`; a changed loopback port that made sealed reasoning foreign under the [deployment issuer](../../src/topology/endpoint.rs) (stripped by `transfer-visible-history`, consistent with design); Chat clients routed to a Responses-only binding with Chat-shaped upstream bytes.

### Findings

| Id | Lane | Observation | Code |
|---|---|---|---|
| A | Responses → Responses | A delivered reasoning item with `status:"incomplete"` (after `max_output_tokens`) is rejected as next-turn history (`representation.terminal`, before dispatch). pi keeps length-stopped turns and replays reasoning verbatim | [checks.rs](../../src/protocol/openai/lowering/checks.rs) request lifecycle guard |
| Q1 | Responses → Bailian Responses | pi defaults reject before dispatch: `summary:"auto"` (`codec.unsupported.reasoning`), encrypted `include` and tools without `strict` (`codec.unsupported.unavailable`). `strict:false` and effort-only pass | [Bailian dialect](../../src/protocol/openai/dialect/bailian.rs) |
| Q2 | Responses → Bailian Responses | The gateway delivers sealed reasoning from this binding, then rejects any reasoning item in the next request (`codec.unsupported.unknown_field`) | Bailian history guard in the same file |
| E | Responses upstream → Messages client | Every text answer fails after dispatch: `TextMetadata{logprobs:[]}` hits `!is_absent()` while the neighbouring delta guard uses `!is_empty()`. Removing `logprobs:[]` leaves only the terminal gap F | [client SSE](../../src/protocol/anthropic/messages/client/sse.rs), [requirements](../../src/semantic/task/generation/requirements.rs) for the request-side twin |
| F | Chat/Responses upstream → Messages client | Ordinary completion (`finish_reason:"stop"`, Responses `completed`) rejects `codec.unsupported.terminal` after dispatch; tool-use turns succeed | [terminal encode](../../src/protocol/anthropic/messages/terminal.rs) |
| G | Responses client → Chat/Messages/Bailian | Omitted `strict` rejects (`representation.strict_default`, `admission.tool_schema_normalization`, `unavailable`) | Chat lowering, Messages request, Bailian dialect |
| H | Responses client → Chat | `summary` and encrypted `include` reject (`representation.reasoning`) | [checks.rs](../../src/protocol/openai/lowering/checks.rs) presentation/encrypted-output guards |
| R | Standard Chat client ← reasoning-emitting upstream | Upstream reasoning fails at event projection after dispatch; SSE ends without a Chat terminal (pi treats it as retryable), JSON returns 502. The `readable_reasoning` profile passes the same lanes | Chat delivery projection |
| M1 | Messages client → non-Messages routes | pi's default `cache_control` rejects at `admission.explicit_cache` | [requirements](../../src/semantic/task/generation/requirements.rs) |
| M2 | Messages client → Haiku | pi's bundled `@anthropic-ai/sdk` sends `anthropic-version: 2023-06-01`; pi adds `interleaved-thinking-2025-05-14` for manual thinking. Both reject before body collection ([usage](../usage.md#messages)); manual thinking is disabled on this binding. Adaptive thinking with signed replay passes | [admission](../../src/gateway/ingress/admission.rs) |
| B/C/D | Messages upstream → OpenAI clients | `service_tier:"standard"` and refusal `category:"cyber"` hit catch-alls; `max_tokens`-truncated tool input succeeds in SSE but fails JSON/retained (`Invalid`) | [service](../../src/protocol/openai/service.rs), [terminal](../../src/protocol/openai/terminal.rs), [native response](../../src/protocol/openai/lowering/native_response.rs) |
| U | Chat upstream; Interactions → Messages | Two nonnull usage reports, on the finish chunk and the trailing chunk, reduce as a duplicate final; this sequence still needs Provider qualification. An Interactions details-only final usage reports `Invalid` in JSON but `Unsupported` in SSE | [Chat decode](../../src/protocol/openai/events/decode/chat.rs), [reduce](../../src/semantic/task/generation/stream/reduce.rs), [Messages accounting](../../src/protocol/anthropic/messages/accounting.rs) |

Also observed: `glm-5.3` accepts only `low`/`high` effort; effort scope blocks Chat/Responses reasoning effort on Messages targets without a configured conversion; Responses `function_call_output.status` rejects on Messages; explicit `detail:"auto"` rejects on Messages ([usage](../usage.md#image-input)). These are documented or binding facts that discovery must publish rather than defects.

Passing controls: plain text and non-reasoning tool loops on every same-protocol lane; Responses and Messages same-issuer reasoning/signature replay over two turns (`gpt-6-luna`, Haiku adaptive); cross-protocol tool-call turns into Messages clients; Responses clients on Chat and Messages upstreams with explicit `strict:false` and no reasoning presentation.

**Permanent-gate source recheck (2026-10-28):** installed pi-ai 1.4.0 was exercised with intercepted fetch, synthetic credentials and no compat overrides to capture the nine initial HTTP bodies in [agent-defaults.json](../../testdata/repair-and-recall/agent-defaults.json). Medium is the clean Agent thinking default; nonreasoning text/tools are separately labelled controls. The actual serializer adds `required:[]` to Messages tool schemas. JSON delivery and stored Responses remain gateway controls because pi streams with `store:false`. The production gate also records Bailian's rejection of returned message phase and reverse-order tool-result history; those are distinct from encrypted-reasoning continuity.

The [pinned OpenAI chunk definition](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/chat/chat_completion_chunk.py) specifies null usage except on the final empty-choice chunk. The initial gate uses that standard sequence. Finding U's double-nonnull sequence is not established as standard by that source and must not justify a generic duplicate-final relaxation without the bounded Provider observation.

### Binding-history source review (2026-10-28)

Reviewed the catalog and public endpoint contracts at `5611943`. These are source distinctions for the production-construction gate and S11, not live Provider observations or new binding activation. Existing replay booleans describe codec configuration, not proof of an endpoint's history contract.

| Contract | Source distinction | Reviewed disposition |
|---|---|---|
| SIWC subscription | The [inference contract](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference) and selected local dialect forbid output caps and flat tools; upstream delivery always streams | Keep captured pi defaults rejected with zero dispatch. A separately labelled uncapped Responses text control verifies OAuth construction, stateless upstream execution and two-turn history; no successful default tool/reasoning claim |
| Bailian Responses | The [input union](https://help.aliyun.com/zh/model-studio/qwen-api-via-openai-responses) lists reasoning `id`, `summary` and optional lifecycle status, not opaque encrypted/full-readable content | Summary-only history has a distinct positive control. Q2's generic encrypted corpus remains a different counterexample; do not declare all reasoning unsupported |
| Bailian Chat | The [Chat reference](https://help.aliyun.com/zh/model-studio/qwen-api-via-openai-chat-completions) documents Qwen preserved thinking, while GLM's default `clear_thinking:true` ignores earlier thinking | Do not derive one history fact from the shared dialect. No new clear-thinking control is activated |
| DeepSeek Chat | [Thinking mode](https://api-docs.deepseek.com/guides/thinking_mode/) requires returned reasoning during tool continuations, while ordinary non-tool history differs | Use an explicitly readable Chat control and inspect actual second-upstream reasoning. Standard Chat remains an S11/S6 planning/delivery repair |
| Native Messages | Existing Go/Haiku binding disallows manual thinking; adaptive thinking has signed and redacted carriers | Keep manual/default rejection; separately label adaptive controls and verify both opened values on turn two |
| Grok Responses | The [reasoning guide](https://docs.x.ai/developers/model-capabilities/text/reasoning) describes encrypted reasoning and continuation even without explicit encrypted include | Use empty-summary opaque reasoning on a nonreasoning request; inspect the opened original token, independently of the common summary-plus-encrypted protocol control |
| Xiaomi native | [Responses](https://mimo.mi.com/docs/zh-CN/api/chat/responses) and [integration guidance](https://mimo.mi.com/docs/zh-CN/quick-start/faq/api-integration) describe retained thinking; recommendation alone is not a tool-continuation rejection requirement | Qualify readable versus opaque carriers separately; `replay_responses:false` is not proof that visible history is unsupported |
| Native Zhipu | [Thinking mode](https://docs.bigmodel.cn/cn/guide/capabilities/thinking-mode) requires current tool-turn reasoning, but default clear-thinking ignores earlier completed turns. The [Responses schema](https://docs.bigmodel.cn/openapi/openapi-responses.json) has readable `content` as an object | S11 must preserve tool-turn requirement versus earlier-turn scope. Do not assume standard Responses arrays or an opaque carrier |
| OpenRouter | [Preserving reasoning](https://openrouter.ai/docs/guides/best-practices/reasoning-tokens#preserving-reasoning) documents readable/structured Chat history and encrypted Responses items; context scope can vary by model | Accepted carriers do not establish native-model omission requirements for every routed binding |
| NVIDIA hosted | The [model-specific Chat schema](https://docs.api.nvidia.com/nim/reference/nvidia-nemotron-3-super-120b-a12b-infer.md) documents reasoning output but closes message input over role/content; its tool examples also exceed that input schema | Keep schema/deployment inconsistency explicit. Hosted Responses history is not established by general self-hosted NIM documentation |
| ModelBest / non-Messages Go | The [hosted MiniCPM guide](https://github.com/OpenBMB/MiniCPM-V/blob/main/docs/api.md) documents output thinking; [Go endpoint docs](https://opencode.ai/docs/go/#endpoints) establish native endpoint bindings, not full history semantics | History acceptance/ignoring remains unestablished. Do not copy native Xiaomi facts through Go or treat absent documentation as unsupported Provider behavior |

The minimal S11 distinction is a reasoning-emitting assistant tool turn followed by its actual result, versus a completed assistant answer followed by another user turn. A successful syntax check or one generic stream cannot establish whether the binding consumes, ignores or requires the historical value. Unestablished cases retain current behavior pending endpoint-specific qualification.

**Endpoint qualification recheck (2026-10-28, `a5634c8`):** the current [DeepSeek thinking guide](https://api-docs.deepseek.com/guides/thinking_mode/) requires reasoning from **all previous turns**, including non-tool turns, whenever a thinking request carries `tools`; incorrect return produces HTTP 400. Without tools, returned reasoning is ignored. This is broader than Zhipu's documented current-tool-cycle requirement. Do not encode `required_with_tools` as current-cycle-only, or treat it as an unconditional requirement on tool-free requests.

The Qwen Chat reference both recommends complete preservation and states that missing historical reasoning does not error; it establishes accepted carriage, not rejection-based planning. The [DeepSeek Responses guide](https://api-docs.deepseek.com/guides/responses_api/) describes plaintext reasoning merged into adjacent assistant content, with no summary/encrypted carrier; do not copy Chat's rejection requirement to Responses. TokenPlan applicability and hosted GLM/Kimi tool-turn requirements need separate qualification rather than inheritance from the shared Bailian dialect.

The four selected facts cannot yet classify every endpoint honestly. NVIDIA's hosted reference does not establish `/v1/responses` availability or historical reasoning intake; general self-hosted NIM documentation is insufficient. ModelBest's hosted guide documents reasoning output, not historical input. Go documents endpoint/model availability, not native Xiaomi's or Meta's history requirements. Explicit unassessed declarations and phased migration were accepted on 2026-10-29: preserve existing behavior and publish no invented fact while established bindings migrate to the [common owner](../architecture.md#binding-reasoning-history-contract). Unmigrated bindings remain unassessed even when some carrier evidence exists; neither replay switches nor shared dialects fill the gap.

**Q2 source qualification (2026-10-29):** one direct `aliyun-dashscope-cn` / `qwen3.8-flash` Responses request used effort `low`, `max_output_tokens:512`, JSON, `store:false`, a 60-second total deadline and no retry. The project loader supplied the first pool member; only structural counts/flags were reported, and no response payload was retained as a fixture. HTTP 200 reported completion with one reasoning owner: an ID field, one `summary_text` part, no status field, no content array and no non-null encrypted carrier. This matches the current [Bailian Responses reference](https://help.aliyun.com/zh/model-studio/qwen-api-via-openai-responses), whose reasoning input admits `id`, `summary` and optional status. It establishes this observed shape, not live second-turn acceptance or a guarantee that every response has the same shape.

The generic encrypted Q2 probe remains a synthetic uncarried-history counterexample, not evidence of native Qwen encryption. The first S5 consumer therefore preserves summary-only history exactly and projects only uncarried owners on assessed Bailian Responses bindings. Existing production controls now omit optional status at the synthetic source and verify returned summary IDs; codec regressions separately cover same-issuer finalized-incomplete omission, partial rejection, mixed retained summaries/tool aliases, unattributed full-readable history and explicit read-intent rejection. Unassessed TokenPlan and other request consumers retain their old behavior.

**Q1 presentation/continuity qualification (2026-10-29):** one direct `aliyun-dashscope-cn` / `qwen3.8-flash` JSON Responses request supplied effort `low`, summary `auto` and `include:["reasoning.encrypted_content"]`, without tools. Bounds were 512 output tokens, a 60-second total deadline, one request and no retry, using the project loader. HTTP 200 reported completion with one summary owner: nonempty string ID, absent status, one summary part, no content array and no nonempty encrypted string. Only structural flags were reported; no payload became a fixture.

The [OpenAI reasoning guide](https://developers.openai.com/api/docs/guides/reasoning) defines auto as the most detailed available summarizer and keeps opaque continuation distinct from summary text. The current [Bailian reference](https://www.alibabacloud.com/help/en/model-studio/qwen-api-via-openai-responses) lists effort, not a summary selector or encrypted carrier, and states that unlisted OpenAI parameters are ignored. The observation therefore supports the selected S4 projection, not a claim that Bailian honors those controls. Auto resolves to the fixed native summary; explicit granularity and unavailable opaque continuity use separate named omissions. The existing no-tool summary control now carries the original controls through both turns while its receiver requires projected effort-only requests. Omitted tool strictness and live continuation remain separate, unverified contracts.

**Standard Chat delivery boundary (2026-10-29):** the selected omission applies to unsigned IR reasoning, including summary and readable-text parts arriving after visible content. Static/retained copies filter those owners before grouping; incremental delivery suppresses their lifecycle only after source validation, and the direct event encoder still reduces them before suppressing rendering. Opaque opening/closing values and part replay remain protected, with matching replay-format rejection on static and grouped event paths. Source observations, usage and visible call IDs remain independent of the delivered copy.

Absence of a native signature is not proof of an unsigned IR owner. The existing [Google thought decoder](../../src/protocol/google/generate_content/sse.rs) finalizes a `GoogleGenerateContentPart` replay value even when its [thought-provenance payload](../../src/protocol/google/generate_content/replay.rs) has no signature. That current carrier remains protected; this slice does not reinterpret or remove it. The Responses control supplies truly unsigned late summary/text independently. The pre-existing TokenPlan Qwen admission guard is retained while those bindings are unassessed; direct assessed DashScope Qwen can use the Standard omission.

### Pre-stage-4 contract qualification (2026-10-29)

Reviewed `450f5bc`, current official references and the current unwrapped installed pi. This section separates source contracts, client observations, native observations and unresolved mappings. No production code, source pins, private configuration or host Agent state was changed. Disposable probes were not imported as protocol fixtures.

#### Conditional strictness

The [Responses create reference](https://developers.openai.com/api/reference/resources/responses/methods/create) defines omitted strictness as attempted strict validation when compatible, with non-strict fallback otherwise. The [function-calling guide](https://developers.openai.com/api/docs/guides/function-calling#strict-mode) additionally says Responses normalizes a schema into strict mode when possible; Chat omission remains non-strict. Compatibility therefore cannot mean only an already-closed strict schema. Explicit `strict:true` instead requires a conforming schema and rejects invalid declarations.

The service's complete normalization algorithm is not documented. The [Structured Outputs subset](https://developers.openai.com/api/docs/guides/structured-outputs#supported-schemas) also has model-specific exclusions. An open object with an optional property is the minimal distinction between service-normalizability and already-closed strictness. The reviewed baseline's `strict_schema::admits(schema, false)` checked normalization feasibility; the closed form added closure/required checks. Its permissive keywords and lack of model context made neither result a complete source-default or cross-target equivalence oracle. The later strictness migration below deletes that open/classifier path.

The pinned Python [`_pydantic.py`](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/lib/_pydantic.py) normalization helper is used by explicit-strict SDK helpers; it does not specify the server's omitted-default algorithm. Retain native conditional omission where established. Resolve cross-target branches only with compatibility and carrier evidence; unknown compatibility is not evidence for exact non-strict fallback. Explicit true never takes the omitted-default loss rule. Source pins remain independent regression references, not automatically promoted by this review.

The OpenAI wording was checked through official search results/guides; complete direct-reference extraction was unavailable. Public Bailian and DeepSeek pages were directly fetched. The uncertainty about the normalization algorithm remains explicit.

**Messages carrier qualification (2026-10-30):** the [native strict-tool guide](https://platform.claude.com/docs/en/agents-and-tools/tool-use/strict-tool-use) defines `strict:true` as grammar-constrained validation. The [structured-output limitations](https://platform.claude.com/docs/en/build-with-claude/structured-outputs#json-schema-limitations) apply jointly to strict tools and JSON output: at most 20 strict tools, 24 optional parameters and 16 union parameters per request, plus internal compilation-size limits. The same source explicitly excludes case guarantees for string enum/const values, including normally completed responses. Native schema acceptance alone is therefore not an equivalent source-strict carrier. Official indexed page text was checked; direct extraction was unavailable. The pinned SDKs already expose optional non-null boolean `strict`; no source pins changed.

The selected bounded implementation uses an unchanged, source-closed schema only when the native exact subset and binding opt-in both allow strict carriage. Otherwise assessed source compatibility takes the existing conditional-default loss; assessed incompatibility resolves exactly to false. Root and inline-added definitions use the same source resolver, and final native validation charges all encoded strict definitions plus JSON output. The 21-tool counterexample retains request rejection rather than introducing a first-N or whole-request downgrade policy. References, omitted parameters, larger enum properties and model-specific source constraints remain unassessed; these guards do not classify them as incompatible. No Provider or installed-client observation was added by this qualification.

#### Conditional strictness migration (2026-10-30)

The conversion-ladder selection supersedes the qualification baseline's compatibility gate and whole-request conditional-overflow rejection. The shared resolver now tests only an already-closed schema plus the target's equivalent strict carrier. Unknown/open/incompatible defaults take `openai.omit-conditional-tool-strictness.v1`; no supplied schema is normalized or rewritten. Closed local-reference schemas retain strict carriage on an OpenAI target; the Messages reference limitation instead resolves them non-strict. Native Responses still preserves omitted strictness and parameters.

The pinned OpenAI Python [function definition](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/shared/function_definition.py) explicitly defines omitted parameters as an empty parameter list. Required carriers materialize `{"type":"object","properties":{},"additionalProperties":false}` on private copies without a loss receipt. This also permits equivalent strict carriage for parameterless tools when the binding supplies it; otherwise the conditional-default loss still applies.

The official [OpenAI function-calling guide](https://developers.openai.com/api/docs/guides/function-calling#strict-mode) and [Anthropic structured-output limits](https://platform.claude.com/docs/en/build-with-claude/structured-outputs#json-schema-limitations) were fetched again on 2026-10-30; source/SDK pins remain unchanged. Anthropic still publishes the joint 20-strict-tool, 24-optional-parameter and 16-union-parameter limits and the string enum/const casing exclusion. The selected all-conditional fallback is gateway adaptation policy, not a native promise to downgrade.

Messages now resolves once after tool-change carrier projection, so grammar accounting sees the definitions actually sent. Projection and final wire validation share the same subset/budget checker. On overflow the resolver starts from source intent and relaxes every conditional definition, including inline additions; explicit tools and JSON output remain unchanged. The 19-explicit-plus-2-conditional case keeps all 19 guarantees and relaxes both defaults. Twenty-one explicit tools and a 17-union output schema still reach the native schema rejection. A carrier-collapse control removes one of 21 definitions before budgeting and retains strictness on the 20 carried definitions.

The expanded existing [schema tests](../../tests/semantic/schema.rs) first failed at the intended `StrictDefault` and `Unsupported(Schema)` guards, then passed with independent wire assertions and source-immutability checks. The existing [native transport test](../../tests/transport/native.rs) checks unknown and omitted parameter schemas through public admission, preflight and actual request preparation. The obsolete `StrictDefault` representation error and compatibility classifier are deleted. Request disposition ownership, broader feature-gate removal and the remaining R1 fields remain separate unfinished work.

#### Actual installed pi Responses behavior

An isolated pi 1.4.0 RPC process used `openai-responses`, one read-tool declaration, no body compat overrides and its observed medium thinking default. Test-owned HOME/XDG/model/settings state, an unwrapped launcher and resource-disable flags excluded personal state. A direct loopback scripted authority supplied the successful streams; this bypassed the gateway and every real Provider.

Five cases used three HTTP requests each: tool response, tool feedback/final answer, then an independent user continuation. The test model declared a 2048-token cap; each prompt had a 30-second bound, each request was capped at 256 KiB and each scripted response below 32 KiB. Retries, warming and automatic compaction were off for bounded observation. Only structural facts were reported.

| Supplied stream | Actual client observation |
|---|---|
| Summary-only reasoning plus one call | Reasoning summary is retained through both subsequent requests; call ID/result association survives; function-call status is omitted |
| Summary plus synthetic opaque reasoning | The opaque value is also returned unchanged; this says nothing about gateway sealing or Provider acceptance |
| Two calls in one assistant turn | Both scratch reads succeed; requests contain both calls before both results, in call order, not interleaved adjacent call/result pairs |
| A completed message with `phase:final_answer` | Phase is retained on the next user request; ordinary text reconstruction does not erase it |
| Incomplete call, unfinished JSON and `max_output_tokens` | Assistant stop is `length`; pi creates an error result without executing the read and continues |
| Incomplete call with valid arguments and the same token-limit outcome | The same no-execution error occurs; this distinguishes truncation protection from a mere JSON/schema validation failure |

Initial requests carry summary auto, encrypted include, `store:false` and omitted tool strictness. These observations validate actual client reconstruction, not gateway default-profile acceptance or a native source's complete event sequence. The two incomplete cases differ only to identify the execution guard; they are not new wire conformance fixtures.

The installed `pi-agent-core` loop explicitly fails all calls on a length-stopped message instead of executing salvaged arguments. Its error-result events must not be counted as successful tool execution. The installed Responses serializer preserves phase in text signatures and serializes retained reasoning items. Public installed-file SHA-256 provenance:

- `pi-ai/dist/api/openai-responses-shared.js`: `00796997da00cd4cb77ae8c1c4bb92a3d4a4a4dd5bea18524de7173e4f877e4c`.
- `pi-agent-core/dist/agent-loop.js`: `22ee81f9977e211044c1ad41a763876c793ca7fc1c5b39b9fa7f76f3cb281129`.

**Resolved-model correction:** the initial nine-body corpus constructs a bare synthetic model; that is API-form characterization, not a copy of every built-in model's metadata. Current public `opencode-go.json` gives `claude-haiku-5-5` `forceAdaptiveThinking:true` and `supportsTemperature:false`. The serializer consults that metadata, not the ID alone. A second isolated current-pi RPC observation overrode only the built-in provider's loopback origin, synthetic key and documented headers, with no custom model or compat declaration. Its observed medium default sent adaptive thinking/medium effort, executed one scratch read and returned the signed history on two subsequent requests; three requests completed with toolUse/stop, then stop. Retry/warming/compaction remained bounded/off and prompts had 30-second deadlines. This direct-authority loop bypassed the gateway and real Providers.

The earlier bare-model manual rejection remains a valid limitation/control, but is not evidence that actual Go/Haiku default reasoning is impossible. Conversely, an arbitrarily added `forceAdaptiveThinking` flag is not automatically a default-profile acceptance result. Preserve and inspect the model actually resolved by the client.

#### Native observations and documentary conflicts

Each native request selected the first direct `aliyun-dashscope-cn` pool member through the project loader, used `qwen3.8-flash` with low effort, a 60-second total deadline, no retry and a 1 MiB response bound. Payloads, IDs and nonce values stayed in memory; only statuses/shape/comparison flags were reported.

| Observation | Result and qualification |
|---|---|
| Chat SSE with `include_usage` and total `max_completion_tokens:512` | HTTP 200; one finish frame, one nonnull usage report on an empty-choice tail, `[DONE]`; reported completion count within the total cap. The double-report U sequence was not reproduced |
| Responses, calls A/B followed by results A/B, no phase, explicit non-strict declarations, `max_output_tokens:512` | HTTP 200/completed. This first request establishes acceptance, not that both values were consumed |
| The same grouped association with two distinct synthetic nonce results | A separate one-request observation returned exactly both values in the requested order. This establishes consumption for this direct endpoint/model and input, not reverse-order, arbitrary interleaving or TokenPlan behavior |

User-requested diversity checks then sent the same grouped layout and independent nonce comparison through four additional catalog bindings:

| Provider / upstream model | HTTP / terminal | Both values consumed |
|---|---|---|
| Direct DashScope / `qwen3.8-max` | 200 / completed | Yes |
| TokenPlan / `qwen3.8-flash` | 200 / completed | Yes |
| Native Xiaomi / `mimo-v2.6-pro-ultraspeed` | 200 / completed | Yes |
| OpenRouter / `openai/gpt-6-luna` | 200 / completed | Yes |

Each was a separate serial request with `max_output_tokens:512`, low effort, a 60-second total deadline, no retry, loader-selected first pool member and a 1 MiB response bound. The observations cover five provider/model pairs including direct Qwen Flash; no credentials, IDs, answer text or nonce values became fixtures. They establish grouped association beyond Qwen, not every reverse-order or interleaved layout.

An earlier Chat observation used legacy `max_tokens:512`, which bounds the reply rather than total reasoning-plus-reply on this model. It also observed tail-only usage, but is not total-ceiling evidence; the corrected independent request above is the total-ceiling qualification. There were eight native requests in total, serially, one per run; no hidden retry supplied a passing result.

The current [Bailian Responses reference](https://help.aliyun.com/zh/model-studio/qwen-api-via-openai-responses) still says a result must immediately follow its corresponding call or error. The grouped observations conflict with that literal adjacency claim; the gateway's former adjacency rejection was not verified native runtime behavior. The user selected grouped call-ID association without reordering on 2026-10-29 and requested the diversity checks above. [Architecture](../architecture.md#tool-result-association) owns the selected contract; the shared Bailian consumer migrated on 2026-10-30. Directional and protected-history regressions plus a phase-free production-construction control now check grouped order and actual managed/stateless continuation. This adds synthetic gateway evidence, not new native observations or default-Agent acceptance. The same source documents string **or array** outputs, while the implementation and the older [subset note](2026-10-19-bailian-compatible-requests.md) are string-only.

Phase is absent from the documented input union, whose compatibility rule says unlisted OpenAI parameters are ignored. This establishes no meaningful phase carrier, not a Provider phase-rejection guarantee. The gateway's known-semantic rejection remains distinct from upstream ignored-field handling; no new phase omission is selected by this observation.

The source also states an approximately 80%-of-window input allowance with automatic truncation beyond it. Current catalog context limits are planning metadata, not input-token admission (`ModelLimits::check_request` checks output and effort). Compare this native default with the public source default and reachable request bounds before claiming long-context fidelity. No large-input run, private deployment inspection, derived 80% token ceiling, tokenizer or budget formula was selected here.

#### Partial output, usage and history scope

The [Responses event reference](https://developers.openai.com/api/reference/resources/responses/streaming-events) separates argument deltas, finalized argument text, item lifecycle and incomplete response termination. Its item schema carries arguments as a string and status independently. The [Anthropic stop-reason guide](https://platform.claude.com/docs/en/build-with-claude/handling-stop-reasons) and [fine-grained tool guide](https://platform.claude.com/docs/en/agents-and-tools/tool-use/fine-grained-tool-streaming) explicitly describe token-limit truncation inside tool input and the inability to execute incomplete JSON.

The user selected D's byte-preserving generated argument text with incomplete call/outcome on 2026-10-29; it is not JSON repair or a complete invocation. [Architecture](../architecture.md#generated-tool-arguments-and-execution-input) owns that contract. Native Messages complete JSON/history still requires an object; its intentional restriction must not be generalized into an all-protocol equality rule. The installed-pi observations establish one real consumer's no-execution behavior for length termination; protected replay, stored/static delivery and other consumers still need their own checks.

The current [Chat create reference](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create) places whole-request usage on an extra empty-choice chunk before `[DONE]`, with null usage on other chunks. The reviewed official options do not establish continuous checkpoints. The native observation agrees for this one binding. Preserve duplicate-final rejection; any future dialect checkpoint classification needs evidence from the actual emitting endpoint. Canonical usage-event cleanup is a separate checkpoint/final ownership decision, including published versus unpublished accounting.

A direct [DeepSeek source recheck](https://api-docs.deepseek.com/guides/thinking_mode/) confirms all-prior-turn reasoning with tools and ignored reasoning without tools. The refreshed [Zhipu guide](https://docs.bigmodel.cn/cn/guide/capabilities/thinking-mode) distinguishes preserved-thinking defaults on Coding Plan and standard endpoints. Its current page alone did not re-establish the earlier current-tool-cycle requirement; retain the dated qualification rather than broadening it. Never inherit those defaults through a shared dialect or another billing endpoint. S11 planning checks available client carriage, not proof that a stateless caller supplied every required historical value; request projection preserves available required data and upstream validation retains its authority.

### Semantic-equivalence review (2026-10-28)

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
| Omitted Responses strictness; auto summary/encrypted include | Conditional target-specific rules selected under S3/S4, not blanket empty-field normalization. Keep their dedicated implementation scheduling |

The corrected regressions retain real wire omission, source immutability, native presence, active-control negatives and failed-encoder state. Obsolete receipt expectations are removed from those existing checks; no separate diagnostic-extension test suite is introduced. Foreign finalized-history transfer is independently checked against exact same-issuer continuation and partial-token rejection, not classified as semantic equivalence.

Disabled-citation qualification uses the official false default, corroborated by the pinned [Python config](https://github.com/anthropics/anthropic-sdk-python/blob/9313f13a5a643ceed10fafba6785eab8e04b6e6e/src/anthropic/types/citations_config_param.py) with optional non-null boolean `enabled`. The existing media regression checks equivalent client IR, native uncited encoding with the filename retained, enabled-citation rejection and a malformed known flag; it introduces no new test harness or full-citation capability. Empty/default-only report containers remain meaningful observed report presence until field-specific clearing/precedence evidence says otherwise; request defaults alone do not settle report equivalence.

**Field-specific follow-up (2026-10-30):** the [Chat create reference](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create) defines `logit_bias` as an optional map or null with default null; the pinned [Python request type](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/chat/completion_create_params.py) also permits `None`. The presence-only rejection incorrectly rejected that default. Null now admits the same task as omission and encodes without bias or a loss receipt. Non-null values, including the separately unqualified empty map, retain rejection. A regression reproduced `Unsupported(UnknownField)` before the change and independently checks task equivalence, wire omission and retained negative controls. Broader empty/default report dispositions above are unchanged.

### Defect-history classes

About 115 defect items fixed from 2026-10-17 to 10-28 were classified from commit documentation diffs; counts depend on how bundled commits are split. Same-protocol intake strictness dominated through 10-23 (16 of ~55); from 10-24 consumer catch-alls rejecting another producer's state rose from 2 to 9, required-field completion from 1 to 5 and JSON/SSE/retained divergence from 2 to 4. Presence/null/default confusion stayed high in both windows (10, then 14). Reading audits found most items. Through 10-23, installed clients found 3 and live Providers 8, the latter all lifecycle or presence assumptions; from 10-24 offline probes and SDK consumers found most cross-protocol items.

The structural causes are test-side:

- [Session corpus tests](../../tests/semantic/history/sessions.rs) are the only delivered-output feedback loop. They run diagonal pairs only and decode with `Dialect::MorphieCore`, wider than [production client construction](../../src/gateway/startup/bind.rs).
- Delivery unit fixtures hand-build IR without required standard members such as `logprobs:[]`.
- [Installed-client scenarios](../../tests/clients/drivers.py) override pi compat and Messages headers, and never drive pi over Responses. Agent default shapes and cross-protocol lanes are unexercised.
- Most rejections that depend on the response shape surface only after dispatch, so a single reachable upstream state fails a whole lane.

## Conversion-ladder inventory (2026-10-30)

Code-traced at **`61d478d`** by three read-only sweeps (client intake, planning/target encoding, delivery/history), with key sites spot-checked. No probes, tests or Provider calls were run; tiers are classifications for the [migration](../roadmap.md#conversion-ladder-migration), not verified behavior. [Architecture](../architecture.md#conversion-ladder) owns the ladder; opencode observations come from a 2026-10-11 source clone, not the installed 2.0.32 binary.

### Structural causes

- **Source-level binary gate.** [`admit`](../../src/execution/attempt.rs) calls [`GenerationFeatureSet::check`](../../src/semantic/task/generation/contract.rs) on the source request before any candidate projection; any `required && !supported` rejects. Candidate-local omissions such as owner-local cache breakpoints cannot be reached; the same breakpoint is also rejected in OpenAI lowering and Messages item encoding.
- **Unknown treated as rejection.** [Strictness resolution](../../src/protocol/openai/lowering/strictness.rs) rejects unknown compatibility before consulting the strict carrier, although targets without one (all Bailian writers) resolve both branches to non-strict. [Planning](../../src/execution/plan.rs) rejects unassessed history facts.
- **Dispersed delivery owners.** OpenAI lowering/projection/events/native response, Messages client projection/response/SSE, the downstream adapter, Messages terminal policy in delivery and capability publication decide the same facts independently; D, annotations, usage details and item-lifecycle checks diverge across JSON, SSE and retained views.
- **Dead or unreachable paths.** `ReportedFactPolicy::StrictComplete` is set only in tests; `mca1.` markers have no remaining producer; several delivery guards follow earlier stripping and cannot fire.

### Reachable sites by slice

| Slice | Site | Current behavior | Ladder level |
|---|---|---|---|
| R0 | [`agent-defaults.json`](../../testdata/repair-and-recall/agent-defaults.json); [conformance wire fixtures](../../src/gateway/startup/bootstrap/conformance_tests/wire.rs) | Bare-model manual thinking for Haiku; null upstream echoes hide Messages delivery failures | Corpus and fixture correction |
| R1 | [`contract.rs`](../../src/semantic/task/generation/contract.rs) explicit cache, advisory controls | Public rejection before projection (M1, sampling, metadata, service hints) | L4 |
| R1 | [Bailian dialect](../../src/protocol/openai/dialect/bailian.rs) `GLM_CHAT`/`KIMI_CHAT` | No Minimal/Medium/XHigh encodings; `GLM_RESPONSES` already maps Medium → High | L3 nearest |
| R1 | [Messages target](../../src/protocol/anthropic/messages/target.rs); [Messages request](../../src/protocol/anthropic/messages/request.rs) | Manual budget on adaptive-only binding and effort-scope mismatch reject | L3 |
| R1 | [`checks.rs`](../../src/protocol/openai/lowering/checks.rs) presentation/include; Bailian `phase` | `representation.reasoning` (H); turn-2 unavailable | L4 |
| R1 | [`subscriptions.rs`](../../src/topology/catalog/subscriptions.rs), SIWC dialect | Output ceiling and sampling reject on subscription lanes | L4; flat-tool shape needs evidence |
| R1 | [`model_limits.rs`](../../src/protocol/model_limits.rs) | Output above maximum and unsupported effort reject | L3 clamp / nearest |
| R2 | [`readable_reasoning.rs`](../../src/protocol/openai/lowering/readable_reasoning.rs) | Opaque reasoning → Standard Chat rejects after dispatch on every Responses-upstream reasoning lane | L3 omission |
| R2 | [Messages client projection](../../src/protocol/anthropic/messages/client/projection.rs) and SSE | Unsigned reasoning, OpenAI service classes and echoed hints reject after dispatch | L3 / L4 |
| R2 | [Messages terminal](../../src/protocol/anthropic/messages/terminal.rs) | Chat `stop` with calls and stop-sequence ambiguity reject at the terminal | L2 `tool_use` / L3 `end_turn` |
| R2 | [OpenAI terminal](../../src/protocol/openai/terminal.rs); [Chat decode](../../src/protocol/openai/events/decode/chat.rs) | Unspecified or vendor finish reasons reject | Per-dialect map; Responses `{}`; otherwise L5 |
| R3 | [Admission](../../src/gateway/ingress/admission.rs) | Version other than `2026-10-21` and nonempty beta lists return 400 before body collection; both installed Agents' Messages defaults hit it | Revised version policy |
| R3 | [Messages client request](../../src/protocol/anthropic/messages/client/request.rs) | Any `thinking.block_binding`, per-message `output_config` and tool additions/removals reject at public intake, contrary to the previous usage text | L2/L3/L5 by value; tool changes L1 where admitted |
| R4 | [Planning](../../src/execution/plan.rs) | Unassessed TokenPlan Chat rejects even text; `required_with_tools` rejects Standard Chat tools | Delegation; thinking-off fallback |
| R4 | [Replay opening](../../src/execution/replay.rs); Chat reasoning details intake | Unsealed or foreign reasoning is invalid | L3 visible-history transfer |

Invariant sites confirmed outside the ladder: replay origin/format checks, body and total-size bounds, streaming capability, call-ID and contiguous-owner validation, incomplete-input execution guards, explicit `read_history`, published-usage retraction, trust headers and client file IDs.

## Production corpus and intake experiment (2026-10-30)

R0 extends the [production-construction gate](../../src/gateway/startup/bootstrap/conformance_tests.rs) without changing production conversion or transport selection. The [capture script](../../tests/clients/pi_default_shapes.mjs) still reproduces the original nine bare API-form bodies, and adds three bodies from installed pi-ai **1.4.0**'s `getBuiltinModel("opencode-go", "claude-haiku-5-5")`. The complete public model record, source-file digests and raw version/beta headers live beside those bodies in the [corpus](../../testdata/repair-and-recall/agent-defaults.json). Only the built-in origin is replaced; fetch is intercepted, synthetic authentication/session values and bounded options are supplied, and no personal state or network is used.

The resolved record enables adaptive thinking, disables temperature and retains its 128,000-token maximum. Medium reasoning serializes as `thinking:{type:"adaptive",display:"summarized"}` plus `output_config:{effort:"medium"}`. With the documented header opt-ins, both native gateway turns now pass in JSON and SSE, including actual signed/redacted-history replay. The bare manual-budget body still rejects before dispatch. The raw resolved `2023-06-01` header still rejects before dispatch; R3 owns that blocker. This corrects the earlier bare-model characterization, not the all-protocol default-Agent closure claim.

Independent fixture envelopes now carry OpenAI `service_tier:"default"` and Responses `metadata:{"synthetic":"echo"}`, `background:false`, and `store:false`. These are official non-null observations, not client request overrides. They expose existing delivery guards: Responses → Chat SSE fails on metadata even where Chat JSON succeeds; cache-free Messages controls reach service/hint rejection after dispatch. Their exact current codes remain asserted, with acceptance as the target and R2 as owner.

### Intake experiment and source limits

Every JSON-client observation in the gate runs twice: current upstream selection, then a per-fixture `cfg(test)` streaming override. The clone passed to upstream preparation changes transport and requests Chat usage; the downstream request, JSON renderer, production loader, planning, credential binding, transport and decoder remain real. One candidate/attempt, at most two upstream calls per observation, 1 MiB collected output and five-second waits bound the experiment. It does not qualify changed streaming-only planning or any live Provider.

The comparison retains the entire JSON-client envelope, including settings/reports and absence/null distinctions, plus source usage, reasoning reports, task outcome, EOF, delivery completion and failures. It normalizes request-owned IDs/timestamps, issuer-bound seals, the existing empty/null Chat shell and optional `morphiecore` diagnostics. The current JSON/SSE delivery comparison remains separate and explicitly asserts the newly exposed Chat SSE metadata divergence.

| Binding membership | Observed result | Consequence |
|---|---|---|
| All 40 reviewed OpenAI API-key endpoints, four subscriptions and native Go/Haiku Messages (45 endpoints; exact catalog coverage enforced by the gate) | Current versus streaming upstream observations agree for the existing default/control corpus. Output distinguishes accepted turns, pre-dispatch blockers, continuation pre-dispatch blockers and post-dispatch blockers per binding/client/profile | Reachable corpus agreement supports R2 investigation; rejected lanes are not successful decoder qualification |
| 24 admitted Chat → Chat text bindings, enumerated by the gate's reviewed Chat membership | An additional independent control preserves concrete JSON metadata and URL annotations on current intake and loses exactly those two facts on standard chunk intake; all other full-envelope/accounting/terminal facts agree | Named Chat JSON exception; unconditional decoder deletion is blocked |
| `tokenplan-max-chat`, `aliyun-tokenplan-cn-chat` | The same Chat-client fact control is blocked before dispatch by unassessed history; current and forced intake have the same rejection | Keep the Chat JSON exception; requalify after R4 rather than count this as field parity |
| `google-gemini-3.8-flash-generate-content`, `google-gemini-3.8-flash-interactions` | Catalog contracts already force streaming and their adapters do not accept upstream JSON; checked separately in the coverage guard | No JSON decoder to compare or delete; existing Google SDK/session checks retain their own evidence |

The Chat limitation is independently defined by the pinned OpenAI Python **3.37.0** [full response](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/chat/chat_completion.py), [message](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/chat/chat_completion_message.py) and [chunk](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/chat/chat_completion_chunk.py) schemas, rechecked against the installed locked SDK. Metadata and URL annotations have full-response carriers but no standard chunk carriers. Service tier and system fingerprint do have chunk carriers; the corrected synthetic wire retains service tier instead of manufacturing another gap. Chat custom-tool SSE remains its separately retained carrier restriction.

Responses lifecycle events contain complete [Response objects](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/responses/response_completed_event.py); the pinned Vercel JSON/event pair independently carries settings, metadata, storage and usage. Messages start events contain a [Message](https://github.com/anthropics/anthropic-sdk-python/blob/9313f13a5a643ceed10fafba6785eab8e04b6e6e/src/anthropic/types/raw_message_start_event.py), with terminal reports carried by deltas. No analogous JSON-only blocker was found in this bounded corpus. R2 must retain the named Chat exception and the current decoders until its selected delivery/intake migration lands.
