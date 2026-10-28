# Standard Generation request boundaries

Checked **2026-10-26** against MorphieCore `4560c66`, pinned OpenAI Python 3.37.0 / Node 7.43.0 and Anthropic Python 2.1.0 / TypeScript 1.1.0. Exact revisions remain in [sources.lock.json](../../tests/upstream/sources.lock.json). The source-backed counterexamples below motivated the same-day standard-priority repair; they are not current defect reports or a second work queue.

## Scope and evidence

Covered standard public Client request/history → IR, shared replay opening, and IR → standard target request. Synthetic probes used public decoders and independent typed IR; Provider dialects, response/SSE delivery and live effectiveness were outside this review. Source requests remained unchanged. No Provider calls or broad SDK/server conformance claim accompanied the investigation.

## Source contracts

- OpenAI Chat: [create parameters](https://raw.githubusercontent.com/openai/openai-python/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/chat/completion_create_params.py), [assistant history parameters](https://raw.githubusercontent.com/openai/openai-python/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/chat/chat_completion_assistant_message_param.py).
- OpenAI Responses: [reasoning input](https://raw.githubusercontent.com/openai/openai-python/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/responses/response_reasoning_item_param.py), [function-call input](https://raw.githubusercontent.com/openai/openai-python/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/responses/response_function_tool_call_param.py), [input union](https://raw.githubusercontent.com/openai/openai-python/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/responses/response_input_item_param.py), [complete output-message history](https://raw.githubusercontent.com/openai/openai-python/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/responses/response_output_message_param.py), [easy-input history](https://raw.githubusercontent.com/openai/openai-python/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/responses/easy_input_message_param.py) and its [content union](https://raw.githubusercontent.com/openai/openai-python/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/responses/response_input_message_content_list_param.py).
- Anthropic: [create parameters](https://raw.githubusercontent.com/anthropics/anthropic-sdk-python/9313f13a5a643ceed10fafba6785eab8e04b6e6e/src/anthropic/types/message_create_params.py), [tool-use parameters](https://raw.githubusercontent.com/anthropics/anthropic-sdk-python/9313f13a5a643ceed10fafba6785eab8e04b6e6e/src/anthropic/types/tool_use_block_param.py), [diagnostics parameters](https://raw.githubusercontent.com/anthropics/anthropic-sdk-python/9313f13a5a643ceed10fafba6785eab8e04b6e6e/src/anthropic/types/diagnostics_param.py), [tool-turn grammar](https://platform.claude.com/docs/en/agents-and-tools/tool-use/handle-tool-calls), [thinking/tool restrictions](https://platform.claude.com/docs/en/claude_api_primer) and [manual-thinking turn structure](https://platform.claude.com/docs/en/build-with-claude/extended-thinking).

SDK request types, SDK response types and SDK convenience objects are different evidence. Optional does not imply nullable. Official server semantic rules supplement generated types; server tolerance of an invalid shape would not establish standard conformance.

## Durable counterexamples

| Boundary | Minimal contrast | Contract to preserve |
|---|---|---|
| Continuation | Opaque-only reasoning versus summary plus opaque continuation; damaged recognized `mc1.broken` | An unauthorized or damaged replay carrier must not become successful owner deletion. Unknown provenance is not proven foreign provenance |
| Explicit intent | Chat `store:false` versus omission | Preserve presence in both public envelope and target encoding; this does not implement stored Chat completions |
| Request versus response shape | Chat assistant `audio:{id:"a"}` plus response-only siblings | Request history remains the ID-reference branch; object length must not select a response variant |
| Convenience fields | `parsed`, `parsed_arguments`, response-only annotations | SDK-derived objects must not change public request meaning; unknown extras are discarded, not retained |
| Known unsupported fields | `functions:""`, `function_call:[]`, `web_search_options:[]` | Validate the known field before deciding whether its value is inert; generic emptiness is not schema validation |
| Known nullability | Responses `status:null`, `async:null`, `namespace:null`; Messages `stop_sequences:null`, `caller:null` | Request optionality and response/default handling are separate |
| Discriminated history | Complete assistant output message versus easy-input shorthand; lifecycle-bearing input message | Select a whole union branch. Complete output history needs its required members; lifecycle-bearing input keeps its content array |
| Participant ownership | Named assistant message followed by an independently owned call | Do not merge a call into the named participant merely to reduce message count |
| Native turn grammar | Assistant tool call followed by user text before its result, or a missing result | Shared call/result linkage does not replace Messages result-prefix and adjacency constraints |
| Manual thinking | Enabled budgeted thinking plus forced tool choice; active tool continuation without leading thinking | Budget arithmetic is insufficient. Apply the native manual-mode/turn constraints without fabricating thinking or weakening tool intent |
| Explicit enforcement | Typed replay-mismatch rejection policy passed directly to a standard codec | An outer endpoint guard does not excuse a pure encoder silently omitting an unsupported guarantee |
| Official diagnostics | Messages `diagnostics.previous_message_id` | Recognized opt-in intent is not a meaningless unknown extension |

Standard video discriminators, hosted features and new diagnostics APIs were not activated by these repairs. A model's capability does not establish a Client protocol carrier.

## Repair disposition

All five slices were selected and completed on **2026-10-26**. [Architecture](../architecture.md#standard-request-boundary) owns the resulting rules: preserve explicit intent and source owners; enforce known types/unions/native grammar; reject unauthorized replay without deletion; keep SDK convenience fields out of standard semantics. The former missing-annotations insertion and empty-signature thinking intake were removed.

Independent [request regressions](../../tests/semantic/standard_contracts/request_boundaries.rs), [replay cases](../../tests/semantic/reasoning_replay/replay_carrier.rs) and [SDK parameter-union checks](../../tests/sdk/standard_requests.py) protect the corrected directions. They are not exhaustive protocol conformance.

Nonempty incoming Messages beta lists reject before body polling; an empty list selects no beta semantics. The actual pi 1.4.0 stable-profile check used a test-owned `anthropic-beta:""` string override, not launcher/wire rewriting. No host configuration changed. Nullable Chat `strict`/`store` remain field-specific contracts; the repair does not generalize all nulls into rejection or assert new effective defaults.

Response/SSE findings were reviewed separately on [2026-10-27](2026-10-27-client-ir-boundaries.md); unsigned Messages output is no longer a pending request-repair exception. [Development](../development.md#verification-gates) owns gate evidence, and [Roadmap](../roadmap.md#three-protocol-fidelity-repair) owns further scope. Existing bounds, auth, cancellation, terminal-honesty and replay-isolation tests remain independent safeguards.
