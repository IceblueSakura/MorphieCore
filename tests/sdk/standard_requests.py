"""Validate actual encoded requests against pinned SDK types, not local snapshots."""
import copy
import json
import sys

from pydantic import TypeAdapter, ValidationError
from openai.types.chat.completion_create_params import CompletionCreateParamsNonStreaming
from openai.types.responses.response_create_params import ResponseCreateParamsNonStreaming
from openai.types.responses.response_input_item_param import Message
from anthropic.types.message_create_params import MessageCreateParamsNonStreaming
from sdk_support import require_sdk

require_sdk("openai")
require_sdk("anthropic")
adapters = {
    "chat": TypeAdapter(CompletionCreateParamsNonStreaming),
    "responses": TypeAdapter(ResponseCreateParamsNonStreaming),
    "messages": TypeAdapter(MessageCreateParamsNonStreaming),
}
for case in json.load(sys.stdin):
    wire = case["wire"]
    adapter = adapters[case["api"]]
    adapter.validate_python(wire, strict=True)
    if case["api"] != "responses":
        continue
    item = wire["input"][0]
    if item["role"] == "assistant":
        for field in ("id", "status"):
            broken = copy.deepcopy(wire)
            del broken["input"][0][field]
            try:
                adapter.validate_python(broken, strict=True)
            except ValidationError:
                pass
            else:
                raise AssertionError(f"complete output history requires {field}")
    else:
        assert item["status"] == "incomplete"
        broken = copy.deepcopy(wire)
        broken["input"][0]["content"] = "P"
        branch = TypeAdapter(Message)
        branch.validate_python(item, strict=True)
        try:
            branch.validate_python(broken["input"][0], strict=True)
        except ValidationError:
            pass
        else:
            raise AssertionError("lifecycle-bearing input requires a content array")
print("standard request parameter unions verified")
