"""Pinned Chat SDK: readable reasoning, tool-error feedback and final history via the Router."""
import json
import sys
import openai
import httpx2 as httpx

KEY = "synthetic-tool-history-client-0001"
RESULT = '{"error":"synthetic lookup unavailable","label":"cedar-71"}'


def check(condition):
    if not condition:
        raise ValueError("SDK tool history assertion failed")


def chat_completion(client, params, stream):
    if not stream:
        value = client.chat.completions.create(**params).model_dump(exclude_unset=True)
        return value
    message = {"role": "assistant", "content": "", "reasoning_content": ""}
    calls = {}
    details = {}
    finish = None
    usage = None
    with client.chat.completions.create(**params, stream=True,
                                       stream_options={"include_usage": True}) as events:
        for event in events:
            if event.usage is not None:
                usage = event.usage.model_dump(exclude_unset=True)
            for choice in event.choices:
                delta = choice.delta.model_dump(exclude_unset=True)
                # These are actual SDK-returned dialect fields, not invented history.
                for key in ("content", "reasoning_content"):
                    if delta.get(key):
                        message[key] += delta[key]
                for part in delta.get("reasoning_details") or []:
                    owner = ("index", part["index"]) if "index" in part else ("id", part["id"])
                    detail = details.setdefault(owner, {})
                    for key, value in part.items():
                        if key in ("text", "summary"):
                            detail[key] = detail.get(key, "") + value
                        else:
                            check(key not in detail or detail[key] == value)
                            detail[key] = value
                for part in delta.get("tool_calls") or []:
                    call = calls.setdefault(part["index"], {
                        "type": "function", "function": {"name": "", "arguments": ""}})
                    if part.get("id"):
                        call["id"] = part["id"]
                    for key in ("name", "arguments"):
                        call["function"][key] += part.get("function", {}).get(key) or ""
                if choice.finish_reason:
                    check(finish is None)
                    finish = choice.finish_reason
    check(finish is not None)
    if calls:
        message["tool_calls"] = [calls[k] for k in sorted(calls)]
    if details:
        message["reasoning_details"] = list(details.values())
    return {"choices": [{"message": message, "finish_reason": finish}], "usage": usage}



def run(base, recovery_stream=None):
    function = {"name": "lookup", "description": "Synthetic lookup", "strict": False,
                "parameters": {"type": "object", "properties": {"key": {"type": "string"}},
                               "required": ["key"]}}
    requests = 0
    transmissions = []
    deliveries = (False, True) if recovery_stream is None else (recovery_stream == "sse",)
    with httpx.Client(trust_env=False, event_hooks={"request": [transmissions.append]}) as http, \
            openai.OpenAI(base_url=base, api_key=KEY, max_retries=0, timeout=5.0,
                          http_client=http) as client:
        for stream in deliveries:
            params = dict(model="qwen3.8-flash", tools=[{"type": "function", "function": function}],
                          messages=[{"role": "user", "content": "lookup"}],
                          reasoning_effort="low", max_completion_tokens=256,
                          extra_headers={"x-session-affinity": "synthetic-chat-session"})
            for turn in range(1, 4):
                requests += 1
                result = chat_completion(client, params, stream)
                check(result["usage"]["total_tokens"] == 18)
                choice = result["choices"][0]
                check(choice["finish_reason"] == ("tool_calls" if turn == 1 else "stop"))
                message = choice["message"]
                check(message["reasoning_content"] == ("Plan 🧪" if turn == 1 else "Check"))
                params["messages"].append(message)
                if turn == 1:
                    call, = message["tool_calls"]
                    check(call == {"id": "call-lookup", "type": "function",
                                   "function": {"name": "lookup", "arguments": '{ "key": "A" }'}})
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
