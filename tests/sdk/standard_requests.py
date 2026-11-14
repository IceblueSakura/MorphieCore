"""Validate actual encoded requests against pinned SDK types, not local snapshots."""
import json
import sys

from pydantic import TypeAdapter
from openai.types.chat.completion_create_params import CompletionCreateParamsNonStreaming
from openai.types.responses.response_create_params import ResponseCreateParamsStreaming
from anthropic.types.message_create_params import MessageCreateParamsStreaming
from sdk_support import require_sdk

require_sdk("openai")
require_sdk("anthropic")
adapters = {
    "chat": TypeAdapter(CompletionCreateParamsNonStreaming),
    "responses": TypeAdapter(ResponseCreateParamsStreaming),
    "messages": TypeAdapter(MessageCreateParamsStreaming),
}
for case in json.load(sys.stdin):
    wire = case["wire"]
    adapter = adapters[case["api"]]
    adapter.validate_python(wire, strict=True)
print("standard request parameter unions verified")
