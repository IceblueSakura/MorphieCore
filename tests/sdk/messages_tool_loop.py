"""Selected native Messages SDK loop; synthetic Router only, no Provider access."""
import copy
import json
import sys
from urllib.parse import urlsplit

import anthropic
import httpx2 as httpx
from sdk_support import require_sdk

require_sdk("anthropic")
origin, key = sys.argv[1:3]
recovery_stream = sys.argv[3] if len(sys.argv) > 3 else None
assert urlsplit(origin).hostname == "127.0.0.1"
profile = json.load(sys.stdin)
requests = 0
transmissions = []
deliveries = (False, True) if recovery_stream is None else (recovery_stream == "sse",)


def send(client, body, streamed, session):
    headers = {"x-session-affinity": session}
    if streamed:
        with client.beta.messages.stream(**{k: v for k, v in body.items() if k != "stream"}, extra_headers=headers) as stream:
            return stream.get_final_message()
    return client.beta.messages.create(**body, extra_headers=headers)


with httpx.Client(trust_env=False, timeout=10,
                  event_hooks={"request": [transmissions.append]}) as http:
    with anthropic.Anthropic(api_key=key, base_url=origin, max_retries=0, http_client=http,
                             _strict_response_validation=True) as client:
        for streamed in deliveries:
            body = copy.deepcopy(profile)
            body["stream"] = streamed
            for turn in range(1, 6):
                requests += 1
                message = send(client, body, streamed, "synthetic-messages-session")
                result = message.model_dump(exclude_none=True)
                assert result["stop_reason"] == ("tool_use" if turn in (1, 3) else "end_turn")
                assert result["usage"] == {
                    "input_tokens": 11, "output_tokens": 7,
                    "cache_read_input_tokens": 13, "cache_creation_input_tokens": 17,
                    "cache_creation": {"ephemeral_5m_input_tokens": 0, "ephemeral_1h_input_tokens": 17},
                }
                content = result["content"]
                if turn in (1, 3):
                    assert content[0]["type"] == "thinking"
                    assert content[0]["thinking"] == ("plan lookup" if turn == 1 else "plan inspect")
                    assert content[0]["signature"].startswith("mc1.")
                    assert content[-2 if turn == 1 else -1] == {
                        "type": "tool_use", "id": "call-lookup" if turn == 1 else "call-inspect",
                        "name": "lookup" if turn == 1 else "inspect", "input": {"key": "A"} if turn == 1 else {},
                    }
                    if turn == 1:
                        assert content[1]["type"] == "redacted_thinking" and content[1]["data"].startswith("mc1.")
                        assert content[-1] == {"type": "tool_use", "id": "call-second",
                                               "name": "lookup", "input": {"key": "B"}}
                else:
                    assert content == [{"type": "text", "text": "A" if turn == 2 else "B" if turn == 4 else "A B"}]
                if recovery_stream is not None and turn == 2:
                    requests += 1
                    recovered = send(client, copy.deepcopy(profile), streamed, "other-messages-session")
                    assert recovered.stop_reason == "tool_use"
                if turn == 5:
                    continue
                for prior in body["messages"]:
                    for block in prior["content"]:
                        block.pop("cache_control", None)
                body["messages"].append({"role": "assistant", "content": content})
                if turn in (1, 3):
                    next_block = {"type": "tool_result", "tool_use_id": "call-lookup" if turn == 1 else "call-inspect",
                                  "content": "A" if turn == 1 else "B", "is_error": turn == 3}
                else:
                    next_block = {"type": "text", "text": "inspect" if turn == 2 else "repeat"}
                next_block["cache_control"] = {"type": "ephemeral", "ttl": "1h"}
                next_blocks = [next_block]
                if turn == 1:
                    next_block.pop("cache_control")
                    next_blocks.insert(0, {"type": "tool_result", "tool_use_id": "call-second",
                                           "content": "second-result", "is_error": True,
                                           "cache_control": {"type": "ephemeral", "ttl": "1h"}})
                body["messages"].append({"role": "user", "content": next_blocks})
                if turn == 2:
                    body["messages"].append({"role": "system", "content": [
                        {"type": "text", "text": "Inspect mode."},
                        {"type": "tool_removal", "tool": {"type": "tool_reference", "name": "lookup"}},
                        {"type": "tool_addition", "tool": {"type": "tool_definition", "definition": {
                            "name": "inspect", "input_schema": {"type": "object"},
                            "eager_input_streaming": True, "cache_control": None,
                        }}},
                    ], "output_config": {"effort": "low"}})
                else:
                    body["messages"].append({"role": "system", "content": [], "output_config": {"effort": "high" if turn == 1 else "low"}})
assert len(transmissions) == requests
print(json.dumps({"requests": requests, "http_requests": len(transmissions),
                  "tool_changes": True, "ttl": "1h"}))
