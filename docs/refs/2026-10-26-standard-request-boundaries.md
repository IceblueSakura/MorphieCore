# Standard Generation request boundaries

Checked on **2026-10-26** against MorphieCore `4560c66`. This note records protocol evidence and bounded counterexamples, not a conformance certificate or a second work queue. [Roadmap](../roadmap.md#standard-request-boundary-repair) owns the selected repair scope and exit gate; [Architecture](../architecture.md#standard-request-boundary) owns the boundary. No implementation repair is claimed here.

## Scope and evidence

The review first covered **standard Client request → IR** for Chat Completions, Responses and Messages, including the subsequent shared replay-opening step. It then covered **request IR → standard target request**. Client admission has no Provider dialect. Routing is fixed; Provider egress dialects, response/SSE encoding and broader API coverage are separate review domains.

The source baseline is [sources.lock.json](../../tests/upstream/sources.lock.json): OpenAI Python `e8a0fd28bbdc1699c97ff0f602b1af3dd378382d` (3.37.0), Node `d02d4de77ea7a6f82325c02e6f44deb34ee3992c` (7.43.0), Anthropic Python `9313f13a5a643ceed10fafba6785eab8e04b6e6e` (2.1.0) and TypeScript `5badc765f17d373405f300ee8e69e7f490b1f172` (1.1.0). These are the audited pins, not a new latest-release claim. Mutable official documentation below was read on 2026-10-26.

Unless marked **source trace**, implementation observations below were reproduced with synthetic, in-process offline probes against the debug library. OpenAI public intake used `GenerationCodec(..., Dialect::Standard, None).decode_client_request`; Messages used its public `decode_client_request`. Replay checks separately invoked `execution::replay::open_from_client`. Encoding probes used independent typed IR or source-valid requests, not malformed parser output as an oracle. They exercised Standard target codecs; source requests remained unchanged by the encoding probes. No Provider requests, HTTP beta experiment or comprehensive SDK/server acceptance formed part of this request review. The earlier synthetic probes are evidence to turn into focused permanent regressions, not an existing committed regression gate.

## Source contracts

- OpenAI Chat: [create parameters](https://raw.githubusercontent.com/openai/openai-python/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/chat/completion_create_params.py), [assistant history parameters](https://raw.githubusercontent.com/openai/openai-python/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/chat/chat_completion_assistant_message_param.py).
- OpenAI Responses: [reasoning input](https://raw.githubusercontent.com/openai/openai-python/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/responses/response_reasoning_item_param.py), [function-call input](https://raw.githubusercontent.com/openai/openai-python/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/responses/response_function_tool_call_param.py), [input union](https://raw.githubusercontent.com/openai/openai-python/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/responses/response_input_item_param.py), [complete output-message history](https://raw.githubusercontent.com/openai/openai-python/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/responses/response_output_message_param.py), [easy-input history](https://raw.githubusercontent.com/openai/openai-python/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/responses/easy_input_message_param.py) and its [content union](https://raw.githubusercontent.com/openai/openai-python/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/responses/response_input_message_content_list_param.py).
- Anthropic: [create parameters](https://raw.githubusercontent.com/anthropics/anthropic-sdk-python/9313f13a5a643ceed10fafba6785eab8e04b6e6e/src/anthropic/types/message_create_params.py), [tool-use parameters](https://raw.githubusercontent.com/anthropics/anthropic-sdk-python/9313f13a5a643ceed10fafba6785eab8e04b6e6e/src/anthropic/types/tool_use_block_param.py), [diagnostics parameters](https://raw.githubusercontent.com/anthropics/anthropic-sdk-python/9313f13a5a643ceed10fafba6785eab8e04b6e6e/src/anthropic/types/diagnostics_param.py), [tool-turn grammar](https://platform.claude.com/docs/en/agents-and-tools/tool-use/handle-tool-calls), [thinking/tool restrictions](https://platform.claude.com/docs/en/claude_api_primer) and [manual-thinking turn structure](https://platform.claude.com/docs/en/build-with-claude/extended-thinking).

SDK request types, SDK response types and SDK convenience objects are different evidence. Optional does not imply nullable. Official server semantic rules supplement generated types; server tolerance of an invalid shape would not establish standard conformance.

## Confirmed source-to-IR findings

### Continuation and explicit intent

| Finding | Counterexample and observed behavior | Responsible seam |
|---|---|---|
| Opaque continuation is silently stripped | Responses reasoning with `encrypted_content:"opaque"` survives standard parsing, then loses the token in shared replay opening. An opaque-only reasoning item disappears; a summary-bearing item keeps only its summary. Messages thinking signatures and redacted data suffer the same downgrade/deletion | [Replay opening](../../src/execution/replay.rs), called by [HTTP admission](../../src/gateway/ingress/admission.rs) |
| Recognized damaged replay does not fail closed | Reasoning `encrypted_content:"mc1.broken"` returns success from replay opening and its owner disappears. Bare compaction instead rejects, demonstrating inconsistent failure handling | Same replay-opening seam; the parsed semantic owner is not the authority to dispatch a token |
| Explicit Chat storage intent disappears | `store:false` becomes `ExecutionHints.store = Absent` before the public request envelope is returned | [Chat context](../../src/protocol/openai/chat/envelope.rs) and [envelope assembly](../../src/protocol/openai/codec/request.rs) |

The replay defect is not permission to forward untrusted opaque values. The existing [issuer/dependency contract](../architecture.md#reasoning-replay) still applies: unknown provenance is not foreign provenance, malformed recognized envelopes must reject, and failure to authorize continuation must not become successful deletion. Whether to admit bare native continuation under a new trusted provenance mechanism remains a separate decision. Chat's storage omission proves loss of explicit intent, not actual upstream storage.

### Request shape and field presence

| Finding | Counterexample and observed behavior | Responsible seam |
|---|---|---|
| Chat audio request and response shapes overlap | Assistant `audio:{"id":"a"}` becomes an audio reference; adding an unknown sibling rejects; adding response `data/expires_at/transcript` changes it into generated audio | [Audio parser](../../src/protocol/openai/chat/audio.rs); shape selection depends on object length |
| Response/SDK-only fields alter standard intake | Chat assistant `annotations` creates a resource. Chat `parsed` and Responses `parsed/parsed_arguments` participate in validation instead of remaining unregistered request fields | [Chat items](../../src/protocol/openai/chat/items.rs), [Responses items](../../src/protocol/openai/responses/items.rs) and shared content readers |
| Extension content enters Standard intake | Chat `video_url` and Responses `input_video` are accepted as resources | Same item readers; a known extension carrier is not an official Client content discriminator |
| Malformed known unsupported fields bypass type checks | Chat `functions:""`, `function_call:[]` and `web_search_options:[]` are accepted and discarded | [Chat request admission](../../src/protocol/openai/chat/request.rs); generic empty-value treatment replaces field-specific validation |
| Non-nullable request fields inherit report/default handling | Responses function-call `status:null` becomes Completed, `async:null` becomes false, and `namespace:null`/`id:null` become absent. Messages `stop_sequences:null` becomes empty and tool-use `caller:null` becomes default context | [Responses items](../../src/protocol/openai/responses/items.rs), [Messages client request](../../src/protocol/anthropic/messages/client/request.rs), shared native intake |
| Official diagnostic intent is ignored | Messages `diagnostics:{"previous_message_id":null}` is accepted without its opt-in meaning | Messages public request/context parsing; the official field is not a safely meaningless unknown |

Strict known-field validation must precede equivalent normalization. Rejecting an unsupported known semantic is a valid bounded implementation; silently discarding it is not. These findings do not authorize stored Chat completions, web search, new media input or a diagnostics implementation.

### Messages source grammar

The public Client decoder accepts an assistant tool call followed by either (a) user text before its tool result or (b) a user message missing the required result. The native [wire validator](../../src/protocol/anthropic/messages/wire/request.rs) contains adjacency/result-prefix checks, but the [public Client path](../../src/protocol/anthropic/messages/client/request.rs) does not execute the complete source grammar. Shared IR call/result linkage is not a replacement for protocol turn validation.

The manual-thinking combinations in the target findings below also pass public source intake. They are source and target constraint gaps, not universal restrictions to add to the protocol-independent IR.

## Confirmed IR-to-standard-request findings

| Target | Minimal valid source or typed intent | Observed output / failure | Required contract distinction |
|---|---|---|---|
| Chat storage | Set typed `context.store = Presence::Value(false)` independently of intake | Encoded request omits `store` | Fix the [target context assembly](../../src/protocol/openai/target/request.rs) as well as intake; standard Chat has the carrier |
| Chat participant ownership | Independent completed assistant `name:"alice"`, then an unowned function call and matching result, with no message envelope | Lowering merges the call into the named message and rejects with `InvalidContextTransform`; unnamed contrast succeeds | [Message grouping](../../src/protocol/openai/lowering/message_groups.rs) must not invent participant association. A separate call-only assistant carrier is available; preserve the identity guard |
| Chat explicit replay enforcement | Typed reasoning effort `low` plus `ReplayMismatch::Reject` | Pure codec emits only `reasoning_effort:"low"`, just like the no-policy contrast | [Reasoning encoder](../../src/protocol/openai/reasoning.rs) lacks enforcement. The [Endpoint wrapper](../../src/topology/endpoint.rs) already rejects: this is a pure-codec gap, not proof of a gateway bypass |
| Responses output-message history | Complete assistant `type:"message"`, `id:"msg_1"`, `status:"completed"`, `output_text` with annotations | `status` disappears. Valid assistant string shorthand becomes `output_text` history lacking both `id` and `status` | [Item encoder](../../src/protocol/openai/responses/items.rs) must choose a complete output-message carrier or a valid easy-input carrier; those union branches are not interchangeable |
| Responses instruction lifecycle | Developer message with `status:"incomplete"` or `"in_progress"` and one `input_text` part | Content becomes a string while `type/status` remain | The lifecycle-bearing input-message branch requires the input-content array. Unmarked single-part shorthand does not license this hybrid |
| Messages manual thinking and forced tools | Independent typed user/tool definition, `max_output_tokens:2048`, enabled budgeted thinking `1024`, `ToolChoice::Required` | Emits `thinking.type:"enabled"` with `tool_choice.type:"any"` | Native manual mode supports auto/none, not forced use. Validate the combination without silently weakening tool choice or thinking; adaptive mode is distinct |
| Messages manual tool continuation | Same manual settings; user → structured function call → matching result, with no reasoning history | Emits a manual-thinking continuation without a leading thinking block in the final assistant turn | Native manual turn structure requires that block; adaptive mode relaxes it. Do not fabricate thinking/signatures or reinterpret activation |

The Messages target gaps are in [request projection](../../src/protocol/anthropic/messages/request.rs) and final [native validation](../../src/protocol/anthropic/messages/wire/request.rs). Numeric thinking-budget validity and ordinary call/result adjacency are necessary but insufficient. Provider/model restrictions can further narrow adaptive behavior; this review does not establish effectiveness on a deployed model.

## Compatibility differences and unresolved semantics

These are not additional confirmed corruption claims:

- **Selected compatibility, reproduced:** Responses fills missing assistant `output_text.annotations` with `[]` and a receipt; Messages accepts empty-signature thinking as unsigned readable reasoning. The [history shorthand](../architecture.md#client-responses-history-shorthand) and [native Messages projection](../architecture.md#native-messages-projection) describe existing choices. They are not strict-standard evidence. Reconcile their placement and dependent Agent behavior explicitly before changing them; do not silently erase previous selections or broaden Standard to justify them.
- **HTTP context, source trace:** ingress validates the Messages version but does not interpret/reject `anthropic-beta`. The effective eager-input default may depend on the fine-grained-streaming beta. Verify the precise header/default relationship before changing admission; an unsupported beta needs an explicit policy, not blind forwarding. No HTTP reproduction or beta activation is claimed.
- **Representation downgrade, reproduced:** Messages Client intake discards generated message envelopes and converts structured function arguments to raw JSON. Existing IR can retain both structures. No independent semantic corruption was demonstrated solely by this conversion; choose retention/target-local conversion based on concrete owner and source-immutability requirements, not a speculative redesign.
- **Null/default questions:** do not generalize the confirmed non-nullable-field defects to every null. In particular, `strict:null` and `store:null` need field-specific source evidence before selecting equivalence or rejection.
- **IR scope:** no additional indispensable core IR owner was proven missing in these counterexamples. Prefer existing typed owners and pure target projections; unsupported official features remain explicit gaps rather than automatic API activation.

## Evidence limits and later review inputs

Duplicate JSON content keys were rejected by all three public Client entry points. The request probes do not establish complete bounds, cancellation, authentication or replay-isolation coverage; preserve the existing independent safety tests. Compiler success, local decoder round trips and historical SDK/Agent loops cannot certify these corrected standard contracts.

Earlier response/SSE and Provider-focused investigation remains separate: text-part order divergence between JSON and streaming, phase-dependent tool-argument validation, metadata JSON/SSE disagreement, summary/signature-only reasoning delivery and terminal error classification, DeepSeek output-cap/schema contracts, and whitespace-only reasoning handling. This request-boundary review neither resolves nor broadens those findings. They remain inputs to subsequent scoped review after the current repair gate, not an implicit Provider audit, deployment or full-conformance claim.
