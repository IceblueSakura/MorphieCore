"""Consume actual MorphieCore delivery and send SDK-built history back; no tools execute."""
import json
import sys

import anthropic
import httpx2
from sdk_support import check, client_for, require_sdk

origin = sys.argv[1]
# client_for validates literal loopback authority and clears SDK ambient defaults.
with client_for(origin) as client:
    for streamed in (False, True):
        history = [{"role": "user", "content": "Weather?"}]
        shared = dict(model="chat", max_completion_tokens=32, messages=history)
        if streamed:
            with client.chat.completions.stream(**shared) as stream:
                first = stream.get_final_completion()
        else:
            first = client.chat.completions.create(**shared)
        choice = first.choices[0]
        check(choice.finish_reason == "tool_calls")
        call = choice.message.tool_calls[0]
        check(call.id == "call_c91SqDXlYFuETYv8mUHzz6pp")
        check(call.function.name == "GetWeatherArgs")
        check(json.loads(call.function.arguments) == {"city": "Edinburgh", "country": "UK", "units": "c"})
        check(first.usage.prompt_tokens == 76 and first.usage.completion_tokens == 24)
        history.extend([choice.message.model_dump(exclude_none=True),
                        {"role": "tool", "tool_call_id": call.id, "content": "synthetic weather"}])
        # The second upstream fixture deliberately stops at length, not a repaired answer.
        last = client.chat.completions.create(model="chat", max_completion_tokens=32, messages=history)
        check(last.choices[0].finish_reason == "length")
        check(last.choices[0].message.content == '{"')

require_sdk("anthropic")
with httpx2.Client(trust_env=False, follow_redirects=False, timeout=8) as http:
    with anthropic.Anthropic(api_key="synthetic-local-token", base_url=origin.removesuffix("/v1"),
                             max_retries=0, http_client=http,
                             _strict_response_validation=True) as client:
        for streamed in (False, True):
            history = [{"role": "user", "content": "Weather?"}]
            shared = dict(model="messages", max_tokens=128, messages=history)
            if streamed:
                with client.messages.stream(**shared) as stream:
                    first = stream.get_final_message()
            else:
                first = client.messages.create(**shared)
            check(first.stop_reason == "tool_use")
            check(first.content[0].text == "I'll check the current weather in Paris for you.")
            call = first.content[1]
            check(call.id == "toolu_01NRLabsLyVHZPKxbKvkfSMn" and call.name == "get_weather")
            check(call.input == {"location": "Paris"})
            check(first.usage.input_tokens == 377 and first.usage.output_tokens == 65)
            history.extend([
                {"role": "assistant", "content": [block.model_dump(exclude_none=True) for block in first.content]},
                {"role": "user", "content": [{"type": "tool_result", "tool_use_id": call.id,
                                             "content": "synthetic weather"}]},
            ])
            last = client.messages.create(model="messages", max_tokens=128, messages=history)
            check(last.stop_reason == "end_turn" and last.content[0].text == "Hello there!")

print("upstream assets: JSON, SSE, terminal facts and returned history verified")
