"""Pinned SDK over a synthetic loopback codec; no real tools or Provider."""
import json
import sys
from urllib.parse import urlsplit

import anthropic
from anthropic.types import ImageBlockParam, ToolParam
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
        tool: ToolParam = {
            "type": "custom", "allowed_callers": ["direct"], "name": "lookup",
            "input_schema": {"type": "object", "properties": {"n": {"type": "integer"}}},
            "input_examples": [{"n": 9007199254740993}, {"n": 7}],
            "eager_input_streaming": None, "cache_control": None,
        }
        shared = dict(
            model="fixture-model", max_tokens=2048,
            system="SDK root instructions",
            thinking={"type": "enabled", "budget_tokens": 1024},
            metadata={"user_id": "sdk-user"},
            tools=[tool],
        )
        image: ImageBlockParam = {
            "type": "image", "source": {"type": "url", "url": "https://example.invalid/a.png"},
            "transformations": {"oversized_image": "error"},
            "cache_control": None,
        }
        history = [{"role": "user", "content": [
            {"type": "text", "text": "go", "cache_control": None},
            {"type": "document", "source": {"type": "text", "media_type": "text/plain",
                "data": "synthetic notes"}, "context": "summary-only", "cache_control": None},
            image,
        ]}, {"role": "system", "content": [{"type": "text", "text": "Keep history."}]}]
        first = client.messages.create(**shared, messages=history)
        assert first.stop_reason == "tool_use"
        assert first.content[0].type == "thinking"
        assert first.content[0].signature.startswith("mc1.")
        assert first.content[1].type == "tool_use"
        assert first.content[1].id == "call_sdk"
        assert first.content[1].input == {"n": 7}
        history += [
            {"role": "assistant", "content": [block.model_dump(exclude_none=True) for block in first.content]},
            {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "call_sdk",
                "content": [{"type": "text", "text": "found", "cache_control": None}, image],
                "is_error": False, "cache_control": None}]},
        ]
        shared["system"] = [{"type": "text", "text": "SDK root instructions"}]
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
        for streamed in [False, True]:
            shared_cross = dict(
                model="cross-model", max_tokens=32,
                tools=[{"name": "lookup", "description": "Synthetic lookup", "input_schema": {
                    "type": "object", "properties": {"n": {"type": "integer"}}}}],
            )
            messages = [{"role": "user", "content": "go"}]
            try:
                if streamed:
                    with client.messages.stream(**shared_cross, messages=messages) as stream:
                        stream.get_final_message()
                else:
                    client.messages.create(**shared_cross, messages=messages)
            except anthropic.InternalServerError as error:
                assert error.status_code == 500
            else:
                raise AssertionError("unsigned reasoning has no standard delivery carrier")
            messages += [
                {"role": "assistant", "content": [
                    {"type": "thinking", "thinking": "Plan", "signature": ""}]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "call_cross", "content": "found"}]},
            ]
            try:
                client.messages.create(**shared_cross, messages=messages)
            except anthropic.BadRequestError as error:
                assert error.status_code == 400
            else:
                raise AssertionError("unsigned thinking cannot become standard history")
        active = None
        callbacks = {"text": [], "thinking": [], "signature": [], "input_json": [], "stops": []}
        previous_raw = None
        with client.messages.stream(model="schedule", max_tokens=64,
                messages=[{"role": "user", "content": "go"}]) as stream:
            for event in stream:
                if event.type == "content_block_start":
                    assert active is None
                    active = event.index
                elif event.type == "content_block_delta":
                    assert active == event.index
                elif event.type == "content_block_stop":
                    assert active == event.index
                    if active == 0:
                        assert previous_raw == ("content_block_delta", "signature_delta")
                    active = None
                    callbacks["stops"].append(event.content_block.model_dump(exclude_none=True))
                elif event.type == "text":
                    callbacks["text"].append(event.text)
                elif event.type == "thinking":
                    callbacks["thinking"].append(event.thinking)
                elif event.type == "signature":
                    callbacks["signature"].append(event.signature)
                elif event.type == "input_json":
                    callbacks["input_json"].append(event.partial_json)
                if event.type in ("content_block_start", "content_block_delta", "content_block_stop"):
                    previous_raw = (event.type, event.delta.type if event.type == "content_block_delta" else None)
            final = stream.get_final_message()
        assert active is None and final.stop_reason == "tool_use"
        assert "".join(callbacks["text"]) == "ACB"
        assert "".join(callbacks["thinking"]) == "Plan"
        assert callbacks["signature"] == [final.content[0].signature]
        assert final.content[0].signature.startswith("mc1.")
        assert "".join(callbacks["input_json"]) == ' \n{"n":1}{"n":2}'
        content = [block.model_dump(exclude_none=True) for block in final.content]
        assert callbacks["stops"] == content
        assert content[1:] == [
            {"type": "text", "text": "AC"}, {"type": "text", "text": "B"},
            {"type": "tool_use", "id": "first", "name": "lookup", "input": {"n": 1}},
            {"type": "tool_use", "id": "second", "name": "lookup", "input": {"n": 2}},
        ]
        replayed = client.messages.create(model="schedule", max_tokens=64, messages=[
            {"role": "user", "content": "go"}, {"role": "assistant", "content": content},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "second", "content": "two"},
                {"type": "tool_result", "tool_use_id": "first", "content": "one"},
            ]},
        ])
        assert replayed.content[0].text == "replayed"
        for model in ("schedule-incomplete", "schedule-metadata"):
            stops, text = [], []
            with client.messages.stream(model=model, max_tokens=64,
                    messages=[{"role": "user", "content": "go"}]) as stream:
                for event in stream:
                    if event.type == "content_block_stop":
                        stops.append(event.content_block.model_dump(exclude_none=True))
                    elif event.type == "text":
                        text.append(event.text)
                partial = stream.get_final_message()
            assert partial.stop_reason == "max_tokens"
            content = [block.model_dump(exclude_none=True) for block in partial.content]
            assert stops == content == [
                {"type": "text", "text": "AC"}, {"type": "text", "text": "B"}, {"type": "text", "text": "D"},
            ]
            assert "".join(text) == "ACBD"
            replayed = client.messages.create(model=model, max_tokens=64, messages=[
                {"role": "user", "content": "go"}, {"role": "assistant", "content": content},
                {"role": "user", "content": "continue"},
            ])
            assert replayed.content[0].text == "replayed"
print(json.dumps({"requests": 15, "partial": True, "refusal_details": True, "unsigned_replay_rejected": True, "scheduled_history": True, "incomplete_history": True, "null_metadata_history": True}))
