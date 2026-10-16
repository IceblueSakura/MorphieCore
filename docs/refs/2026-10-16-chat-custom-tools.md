# Chat custom-tool carriers

Checked on 2026-10-16 against the [pinned OpenAI Python SDK](https://github.com/openai/openai-python/tree/be9d66628ad7377bd36fe5a76ae6d735843f0e76) and the live [Chat create reference](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create). These are protocol facts, not model-binding or live acceptance claims.

| Carrier | Contract and source |
|---|---|
| Definition | `{"type":"custom","custom":{"name":"sql","format":{"type":"grammar","grammar":{"syntax":"regex","definition":"SELECT [0-9]+"}}}}`; [parameter type](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/chat/chat_completion_custom_tool_param.py) |
| Named choice | `{"type":"custom","custom":{"name":"sql"}}`; [choice type](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/chat/chat_completion_named_tool_choice_custom_param.py) |
| Assistant call | `{"id":"c","type":"custom","custom":{"name":"sql","input":"SELECT 1"}}`; [history parameter](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/chat/chat_completion_message_custom_tool_call_param.py) and [response type](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/chat/chat_completion_message_custom_tool_call.py) |
| Result | `role:"tool"`, `tool_call_id` and string or ordered text parts; no function/custom discriminator. [Result type](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/chat/chat_completion_tool_message_param.py) |
| Allowed subset | Nested `allowed_tools` with `mode` and tool reference objects; [choice](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/chat/chat_completion_allowed_tool_choice_param.py) and [subset](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/chat/chat_completion_allowed_tools_param.py) |
| Streaming | The pinned [chunk type](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/chat/chat_completion_chunk.py) defines only function tool deltas. The static custom-call type does not establish an incremental custom carrier |

Custom inputs are strings, not JSON arguments. Chat and Responses format shells differ; grammar syntax and definition retain their value. Unsupported asynchronous, deferred, qualified or programmatic dispatch cannot be hidden in a plain Chat call.

Resolve a returned tool message's kind from its preceding typed call ID, never its name, payload, declaration or an inferred function fallback. Missing or conflicting call ownership remains subject to strict history validation. No gateway tool execution is implied.
