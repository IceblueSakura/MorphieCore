# Same-protocol Generation inventory

Researched **2026-10-19** (with subsequent updates) against official SDK sources: OpenAI Python 3.37.0 and Anthropic Python 2.1.0. This note provides a field-by-field inventory for same-protocol pass-through and dialect mappings (Chat/DashScope, Responses/OpenRouter, Grok/SIWC, and Messages/OpenCode Go). [Architecture](../architecture.md) owns canonical semantics, and [Roadmap](../roadmap.md) owns execution gates.

## Contents

- [Reading the inventory](#reading-the-inventory)
- [Shared Aliyun dialect](#shared-aliyun-dialect)
- [Chat and DashScope dialect](#chat-and-dashscope-dialect)
  - [Request fields](#request-fields)
  - [Content, tools and history](#content-tools-and-returned-history)
  - [Static results and SSE chunks](#static-results-and-chunks)
- [Responses and OpenRouter dialect](#responses-and-openrouter-dialect)
  - [Request and result fields](#request-and-result-fields)
  - [Items and history](#items-and-history)
  - [SSE event families](#every-pinned-sse-event-family)
- [Grok and public SIWC Responses](#grok-and-public-siwc-responses)
- [Messages and Go/Haiku profile](#messages-and-the-gohaiku-profile)
  - [Request fields](#messages-request-fields)
  - [Blocks, tools and history](#blocks-tools-and-history)
  - [Results, usage and events](#results-usage-and-events)
- [Named rules on native paths](#named-rules-on-native-paths)
- [Gap disposition and minimal counterexamples](#gap-disposition-and-minimal-counterexamples)

## Reading the inventory

- **E (Exact / Supported):** Inspected typed carrier preserved within the declared subset.
- **L (Declared Loss):** Explicitly declared loss; tracked under a named rule receipt.
- **R (Rejected):** Unsupported whole variant or conflicting carrier rejected fail-closed.
- **I (Ignored / Dropped):** Unregistered or safely ignored field.

## Shared Aliyun dialect

DashScope and TokenPlan select shared `Dialect::Bailian`.
- **Reasoning**: Readable reasoning and `preserve_thinking` controls are supported.
- **Caps**: Qwen 3.8 and GLM-5.3 emit total `max_completion_tokens`.
- **Responses**: Basic Responses intake supported; native Responses does not inherit automatic tool normalization or unlisted controls.

## Chat and DashScope dialect

### Request fields

| Official field | Classification | Semantics and boundary |
|---|---|---|
| `model`, `messages` | E | Trusted model selection and ordered message history |
| `temperature`, `top_p`, `seed`, `stop` | E | Admitted values; scalar stop normalizes to array |
| `max_completion_tokens`, `max_tokens` | E | Total cap; simultaneous conflicting spellings reject |
| Bailian `enable_thinking`, `thinking_budget` | E | Supported thinking controls with strict model guards |
| `logprobs`, `top_logprobs` | E | Boolean and 0–20 count; count requires `logprobs: true` |
| `reasoning_effort` | E | Admitted qualitative levels (`low`, `medium`, `high`) |
| `response_format` | E | `text`, `json_object`, `json_schema` with strict schema validation |
| `tools`, `tool_choice`, `parallel_tool_calls` | E | Function/custom definitions; strict schema validation |
| `stream`, `stream_options` | E | `include_usage`, `include_obfuscation` |
| `safety_identifier`, `user` | E / L | Bounded end-user attribution; declared loss if target incapable |
| `prompt_cache_key`, `prompt_cache_options` | L / R | Omitted under declared rules; active unsupported options reject |
| `prediction` | L | Validated then dropped under `chat.omit-prediction.v1` |
| `functions`, `function_call` | R | Legacy function call rejected if active |
| `logit_bias`, `moderation` | R | Explicit rejection at intake |

### Content, tools and returned history

| Carrier | Classification | Semantics and boundary |
|---|---|---|
| Message `role`, `content` | E | Admitted roles (`system`, `developer`, `user`, `assistant`) |
| Function / custom tool definitions | E | Raw definitions retained without schema repair |
| Calls and results | E | Exact call IDs, raw arguments, and string/array tool results |
| User media (`image_url`, `input_audio`) | E | Inline base64 and URLs; credential file IDs reject |
| `prompt_cache_breakpoint` | R | Rejected on Chat intake until target carrier selected |
| Assistant `refusal` | E | Representable refusal content; conflicts reject |
| DashScope `reasoning_content` | E | Registered readable reasoning extension |
| DashScope `preserve_thinking` | E | Boolean historical thinking consumption flag |

### Static results and chunks

| Field / Event | Classification | Semantics and boundary |
|---|---|---|
| Envelope (`id`, `model`, `choices`, `usage`) | E | Exactly one choice with index 0 |
| `finish_reason` | E | `stop`, `tool_calls`, `length`, `content_filter` |
| Usage breakdown | E | Verified counters and token details |
| SSE chunks and `[DONE]` | E | Strict sequence verification; EOF cannot substitute for DONE |

## Responses and OpenRouter dialect

### Request and result fields

| Official request field | Classification | Semantics and boundary |
|---|---|---|
| `model`, `input`, `instructions` | E | Supported item subset and request-wide instructions |
| `temperature`, `top_p`, `max_output_tokens` | E | Admitted settings; null normalizes to absent |
| `reasoning` (`effort`, `summary`, `context`, `mode`) | E | Supported reasoning controls |
| `include` (`reasoning.encrypted_content` etc.) | E | Normalized inclusion requests |
| `text` (`format`, `verbosity`) | E | Null request format rejects strictly |
| `tools`, `tool_choice`, `max_tool_calls` | E | Supported tool declarations and invocation budgets |
| `stream_options.include_obfuscation` | E | Bounded non-semantic padding |
| `store`, `previous_response_id` | E / R | Local gateway history managed; raw upstream storage rejected |

| Official result field | Classification | Semantics and boundary |
|---|---|---|
| `id`, `created_at`, `model`, `output` | E | Supported response envelope |
| `status` | E | `completed`, `incomplete`, `failed`, `cancelled` |
| `usage` | E | Cumulative counters and token details |
| `error` | E | Classified errors; non-null `misalignment` rejects |
| `incomplete_details.reason` | E | Unlabelled cause remains unspecified rather than guessed |

### Items and history

| Item variant | Classification | Semantics and boundary |
|---|---|---|
| `message`, `input_text`, `output_text` | E | Roles, item IDs, content, and phase (`commentary`/`final_answer`) |
| `input_image`, `input_file` | E | URLs, inline data, breakpoints; file IDs reject |
| `function_call`, `function_call_output` | E | Call ID association, raw arguments, output content |
| `custom_tool_call`, `custom_tool_call_output` | E | Custom tools and grammar-constrained calls |
| `reasoning` | E | Summary text, reasoning text, encrypted content, replay slots |
| `configuration_update` | E | Mid-conversation reasoning effort changes |
| Hosted tools (`file_search`, `code_interpreter` etc.) | R | Whole variant rejected fail-closed |

### Every pinned SSE event family

- Supported: `response.*` (lifecycle), `response.output_item.*`, `response.content_part.*`, `response.output_text.*`, `response.function_call_arguments.*`, `response.reasoning_summary_text.*`, `response.reasoning_text.*`.
- Deferred / Rejected: `response.audio.*`, `response.code_interpreter_call.*`, `response.file_search_call.*`, `response.image_generation_call.*`, `response.mcp_call.*`.

## Grok and public SIWC Responses

- **Grok**: Standard Responses input with developer instructions and encrypted reasoning; custom context and strict tools closed by binding.
- **SIWC**: Namespace-grouped tool calls, forced upstream SSE, and mandatory `store: false`.

## Messages and the Go/Haiku profile

### Messages request fields

| Official field | Classification | Semantics and boundary |
|---|---|---|
| `model`, `max_tokens`, `messages`, `system` | E | Ordered messages, output ceiling, root instructions |
| `tools`, `tool_choice` | E | Function declarations; auto/none/any/tool modes |
| `thinking` (`adaptive`, `enabled`) | E | Adaptive or numeric budget ($\ge 1024$) |
| `output_config.effort` | E | Independent response-wide effort (`low`, `medium`, `high`) |
| `cache_control` | E | Owner-local ephemeral cache markers (`5m`, `1h`) |
| `metadata.user_id` | E | End-user abuse attribution (up to 512 chars) |
| `temperature`, `top_p`, `top_k` | R | Removed in API version `2026-10-21`; 400 rejection |

### Blocks, tools and history

| Block carrier | Classification | Semantics and boundary |
|---|---|---|
| `text`, `image` | E | Base64 and URL image data; direct assistant documents reject |
| `tool_use`, `tool_result` | E | Call ID, tool name, raw JSON input, tool results, error flag |
| `thinking` block | E | Preserves reasoning text and native signatures |
| Mid-conversation `system` messages | E | Supported native placement; turn boundaries preserved |
| Tool changes (`tool_addition`, `tool_removal`) | E | By-value tool additions and removals |

### Results, usage and events

| Field / Event | Classification | Semantics and boundary |
|---|---|---|
| `message` envelope | E | `id`, `role: "assistant"`, `model`, `stop_reason`, `usage` |
| `stop_reason` | E | `end_turn`, `tool_use`, `max_tokens`, `stop_sequence` |
| Usage breakdown | E | `input_tokens`, `output_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens` |
| SSE events | E | `message_start`, `content_block_start`, `content_block_delta`, `content_block_stop`, `message_delta`, `message_stop` |

## Named rules on native paths

Where loss is unavoidable across target dialects, MorphieCore emits declared loss receipts:
- `cache.omit-key.v1`: Dropping cache key on targets lacking native key carriers.
- `chat.omit-prediction.v1`: Dropping speculative prediction content while preserving user text.
- `generation.omit-identity-hints.v1`: Dropping attribution hints when target capabilities reject them.
- `messages.omit-input-transformations.v1`: Dropping upstream request-referencing path indices.

## Gap disposition and minimal counterexamples

1. **Simultaneous conflicting fields**: Simultaneous `max_completion_tokens` and `max_tokens` reject fail-closed.
2. **Missing terminal cause**: Interactions `completed` envelopes without finish cause map conservatively to prevent fabricated success.
3. **Out-of-order blocks**: Client SSE delivery rejects backward block sequences, poisoning the stream cleanly on protocol violations.
