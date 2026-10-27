"""Pinned Chat SDK: tool-error feedback and final history via the Router."""
import json
import sys
import openai
import httpx2 as httpx

KEY = "synthetic-tool-history-client-0001"
RESULT = '{"error":"synthetic lookup unavailable","label":"cedar-71"}'


def check(condition):
    if not condition:
        raise ValueError("SDK tool history assertion failed")


def chat_completion(client, params, stream, reasoning=False):
    if not stream:
        value = client.chat.completions.create(**params).model_dump(exclude_unset=True)
        return value
    with client.chat.completions.stream(**params, stream_options={"include_usage": True}) as events:
        for event in events:
            if event.type == "chunk":
                for choice in event.chunk.choices:
                    extra = choice.delta.model_extra or {}
                    check("reasoning_details" not in extra)
                    check(reasoning or "reasoning_content" not in extra)
        result = events.get_final_completion().model_dump(exclude_unset=True)
    message = result["choices"][0]["message"]
    # A nullable response/accumulator field is not nullable request history.
    if message.get("tool_calls") is None:
        message.pop("tool_calls", None)
    # Parsed fields and delta indices belong to the SDK accumulator, not history.
    if "parsed" in message:
        check(message.pop("parsed") is None)
    for index, call in enumerate(message.get("tool_calls") or []):
        if "index" in call:
            check(call.pop("index") == index)
        if "parsed_arguments" in call["function"]:
            check(call["function"].pop("parsed_arguments") is None)
    return result



def run(base, recovery_stream=None):
    reasoning = recovery_stream == "reasoning"
    if reasoning:
        recovery_stream = None
    function = {"name": "lookup", "description": "Synthetic lookup", "strict": False,
                "parameters": {"type": "object", "properties": {"key": {"type": "string"}},
                               "required": ["key"]}}
    requests = 0
    transmissions = []
    deliveries = (False, True) if recovery_stream is None else (recovery_stream == "sse",)
    with httpx.Client(trust_env=False, event_hooks={"request": [transmissions.append]}) as http, \
            openai.OpenAI(base_url=base, api_key=KEY, max_retries=0, timeout=5.0,
                          http_client=http, _strict_response_validation=True) as client:
        for stream in deliveries:
            params = dict(model="qwen3.8-flash", tools=[{"type": "function", "function": function}],
                          messages=[{"role": "user", "content": "reasoning" if reasoning else "lookup"}],
                          reasoning_effort="low", max_completion_tokens=256,
                          extra_headers={"x-session-affinity": "synthetic-chat-session"})
            for turn in range(1, 4):
                requests += 1
                result = chat_completion(client, params, stream, reasoning)
                check(result["usage"]["total_tokens"] == 18)
                choice = result["choices"][0]
                check(choice["finish_reason"] == ("tool_calls" if turn == 1 else "stop"))
                message = choice["message"]
                check("reasoning_details" not in message)
                if reasoning:
                    check(message.get("reasoning_content") == ("Plan 🧪" if turn == 1 else "Check"))
                else:
                    check("reasoning_content" not in message)
                params["messages"].append(message)
                if turn == 1:
                    call, second = message["tool_calls"]
                    check(call == {"id": "call-lookup", "type": "function",
                                   "function": {"name": "lookup", "arguments": '{ "key": "A" }'}})
                    check(second == {"id": "call-second", "type": "function",
                                     "function": {"name": "lookup", "arguments": '{"key":"B"}'}})
                    params["messages"].append({"role": "tool", "tool_call_id": second["id"],
                                               "content": "second-result"})
                    params["messages"].append({"role": "tool", "tool_call_id": call["id"],
                                               "content": RESULT})
                else:
                    check(message["content"] == "cedar-71")
                    params["messages"].append({"role": "user", "content": "next"})
                if recovery_stream is not None and turn == 2:
                    probe = {**params, "messages": [{"role": "user", "content": "lookup"}],
                             "extra_headers": {"x-session-affinity": "other-chat-session"}}
                    requests += 1
                    recovered = chat_completion(client, probe, stream)
                    check(recovered["choices"][0]["finish_reason"] == "tool_calls")
    check(len(transmissions) == requests)
    print(json.dumps({"requests": requests, "http_requests": len(transmissions),
                      "protocols": 1, "deliveries": len(deliveries), "three_turn_history": True}))


if __name__ == "__main__":
    run(sys.argv[1], sys.argv[2] if len(sys.argv) > 2 else None)
