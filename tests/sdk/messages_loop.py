"""Pinned SDK over a synthetic loopback codec; no real tools or Provider."""
import json
import sys
from urllib.parse import urlsplit

import anthropic
import httpx2 as httpx
from sdk_support import require_sdk

require_sdk("anthropic")
origin = sys.argv[1]
assert urlsplit(origin).hostname == "127.0.0.1"
with httpx.Client(trust_env=False, timeout=10) as http:
    with anthropic.Anthropic(
        api_key="synthetic-local-token", base_url=origin,
        max_retries=0, http_client=http,
    ) as client:
        shared = dict(
            model="fixture-model", max_tokens=2048,
            thinking={"type": "enabled", "budget_tokens": 1024},
            metadata={"user_id": "sdk-user"},
            tools=[{"name": "lookup", "input_schema": {
                "type": "object", "properties": {"n": {"type": "integer"}}
            }, "input_examples": [{"n": 9007199254740993}, {"n": 7}]}],
        )
        history = [{"role": "user", "content": [
            {"type": "text", "text": "go"},
            {"type": "document", "source": {"type": "text", "media_type": "text/plain",
                "data": "synthetic notes"}, "context": "summary-only"},
        ]}]
        first = client.messages.create(**shared, messages=history)
        assert first.stop_reason == "tool_use"
        assert first.content[0].type == "tool_use"
        assert first.content[0].id == "call_sdk"
        assert first.content[0].input == {"n": 7}
        history += [
            {"role": "assistant", "content": [block.model_dump(exclude_none=True) for block in first.content]},
            {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "call_sdk", "content": "found", "is_error": False}]},
        ]
        for partial, messages in [(False, history), (True, [{"role": "user", "content": "go"}])]:
            fragments = []
            text = []
            stop = None
            ended = False
            with client.messages.create(**shared, messages=messages, stream=True) as stream:
                for event in stream:
                    if event.type == "content_block_delta":
                        if event.delta.type == "input_json_delta":
                            fragments.append(event.delta.partial_json)
                        elif event.delta.type == "text_delta":
                            text.append(event.delta.text)
                    elif event.type == "content_block_start" and partial:
                        assert event.content_block.type == "tool_use"
                        assert event.content_block.id == "partial_sdk"
                        assert event.content_block.name == "lookup"
                    elif event.type == "message_delta":
                        stop = event.delta.stop_reason
                    elif event.type == "message_stop":
                        ended = True
            assert ended
            assert stop == ("max_tokens" if partial else "end_turn")
            assert "".join(fragments) == ('{"n":' if partial else "")
            assert "".join(text) == ("" if partial else "done")
        for streamed in [False, True]:
            result = client.messages.create(**shared, messages=[{"role": "user", "content": "go"}],
                stream=streamed)
            if not streamed:
                assert result.stop_reason == "refusal"
                details = result.stop_details
            else:
                details = None
                with result as stream:
                    for event in stream:
                        if event.type == "message_delta":
                            assert event.delta.stop_reason == "refusal"
                            details = event.delta.stop_details
            assert details is not None
            assert details.type == "refusal" and details.category == "cyber"
            assert details.explanation == "synthetic explanation"
print(json.dumps({"requests": 5, "partial": True, "refusal_details": True}))
