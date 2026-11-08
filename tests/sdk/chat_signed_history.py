"""Pinned Chat SDK: signed reasoning owners survive JSON/SSE tool feedback and recall."""
import base64
import json
import sys

from sdk_support import check, client_for

KEY = "synthetic-gateway-client-token-0001"


def original_token(value):
    check(isinstance(value, str) and value.startswith("mc1."), "missing replay envelope")
    encoded = value.removeprefix("mc1.")
    payload = json.loads(base64.urlsafe_b64decode(encoded + "=" * (-len(encoded) % 4)))
    check(payload["f"] == "openai-responses" and bool(payload["o"]), "wrong replay format/origin")
    return payload["d"]


def completion(client, params, stream):
    if stream:
        with client.chat.completions.stream(**params, stream_options={"include_usage": True}) as events:
            for _ in events:
                pass
            result = events.get_final_completion()
    else:
        result = client.chat.completions.create(**params)
    value = result.model_dump(exclude_unset=True)
    message = value["choices"][0]["message"]
    # Remove only response/SDK accumulator fields, as in the ordinary Chat gate.
    if message.get("tool_calls") is None:
        message.pop("tool_calls", None)
    if "parsed" in message:
        check(message.pop("parsed") is None)
    for index, call in enumerate(message.get("tool_calls", [])):
        if "index" in call:
            check(call.pop("index") == index)
        if "parsed_arguments" in call["function"]:
            check(call["function"].pop("parsed_arguments") is None)
    return value


def run(base):
    requests = 0
    with client_for(base, api_key=KEY) as client:
        for stream in (False, True):
            params = dict(model="cross-model", reasoning_effort="low", max_completion_tokens=256,
                          messages=[{"role": "user", "content": "signed-sse" if stream else "signed-json"}],
                          tools=[{"type": "function", "function": {"name": "lookup", "strict": False,
                              "parameters": {"type": "object", "properties": {"key": {"type": "string"}},
                                             "required": ["key"]}}}])
            for turn in range(1, 4):
                result = completion(client, params, stream)
                requests += 1
                choice = result["choices"][0]
                check(choice["finish_reason"] == ("tool_calls" if turn == 1 else "stop"))
                message = choice["message"]
                if turn == 1:
                    check(message["reasoning_content"] == "Plan 🧪")
                    details = message["reasoning_details"]
                    check([d["type"] for d in details] == [
                        "reasoning.summary", "reasoning.encrypted", "reasoning.encrypted"])
                    check([d["index"] for d in details] == [0, 1, 2])
                    check(details[0]["summary"] == "Plan 🧪")
                    check([original_token(d["data"]) for d in details[1:]] == [
                        "synthetic-readable-token", "synthetic-opaque-token"])
                    check([d["id"] for d in details[1:]] == ["rs-readable", "rs-opaque"])
                    call, = message["tool_calls"]
                    check(call == {"id": "call-lookup", "type": "function", "function": {
                        "name": "lookup", "arguments": '{ "key": "A" }'}})
                else:
                    check(message["content"] == "cedar-71")
                params["messages"].append(message)
                params["messages"].append(
                    {"role": "tool", "tool_call_id": "call-lookup", "content": "lookup-result"}
                    if turn == 1 else {"role": "user", "content": "next"})
    print(json.dumps({"requests": requests, "deliveries": 2, "signed_reasoning_replayed": True,
                      "opaque_reasoning_replayed": True, "three_turn_history": True}))


if __name__ == "__main__":
    run(sys.argv[1])
