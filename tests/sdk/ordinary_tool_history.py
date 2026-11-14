"""Real SDK ordinary-function loops across all three upstream protocols."""
import json
import sys

import anthropic
import httpx2 as httpx
import openai

from chat_tool_history import chat_completion
from responses_history import KEY, RESULT, check


def check_input_items(items, turn, calls, answer, empty=None):
    check(len(items) == {1: 1, 2: 5, 3: 7}[turn] + int(turn > 1 and empty is not None))
    if turn > 1 and empty is not None:
        check(items[1]["type"] == "message" and items[1]["role"] == "assistant"
              and items[1]["content"] == [] and items[1]["id"] == empty["id"])
        items = [items[0], *items[2:]]
    check(items[0]["role"] == "user" and items[0]["content"][0]["text"] == "ordinary-responses")
    if turn > 1:
        for retained, delivered in zip(items[1:3], calls, strict=True):
            check(retained["type"] == "function_call")
            check(all(retained.get(key) == delivered[key]
                      for key in ("id", "call_id", "name", "arguments")))
        for retained, identity, output in [(items[3], "call-second", "second-result"),
                                           (items[4], "call-lookup", RESULT)]:
            check(retained["type"] == "function_call_output"
                  and retained["call_id"] == identity and retained["output"] == output)
    if turn == 3:
        check(items[5]["role"] == "assistant" and items[5]["id"] == answer["id"]
              and items[5]["content"][0]["text"] == "cedar-71")
        check(items[6]["role"] == "user" and items[6]["content"][0]["text"] == "next")


def run(base, model):
    function = {"name": "lookup", "parameters": {"type": "object",
                "properties": {"key": {"type": "string"}}, "required": ["key"]}}
    counts = {}
    transmissions = []
    with httpx.Client(trust_env=False, event_hooks={"request": [transmissions.append]}) as http, \
            openai.OpenAI(base_url=base, api_key=KEY, max_retries=0, timeout=5,
                          http_client=http, _strict_response_validation=True) as client, \
            anthropic.Anthropic(base_url=base.removesuffix("/v1"), api_key=KEY,
                                max_retries=0, timeout=5, http_client=http,
                                _strict_response_validation=True) as messages:
        for protocol in ("chat", "responses", "messages"):
            count = 0
            for stream in (False, True):
                initial = [{"role": "user", "content": f"ordinary-{protocol}"}]
                if protocol == "chat":
                    params = dict(model=model, messages=initial, max_completion_tokens=256,
                                  tools=[{"type": "function", "function": function}])
                elif protocol == "responses":
                    params = dict(model=model, input=initial, max_output_tokens=256,
                                  tools=[{"type": "function", **function}], store=False)
                else:
                    params = dict(model=model, messages=initial, max_tokens=256,
                                  tools=[{"name": "lookup", "input_schema": function["parameters"]}])
                for turn in range(1, 4):
                    count += 1
                    if protocol == "chat":
                        result = chat_completion(client, params, stream)
                        choice = result["choices"][0]
                        check(choice["finish_reason"] == ("tool_calls" if turn == 1 else "stop"))
                        output = [choice["message"]]
                        calls = [{"call_id": call["id"], **call["function"]}
                                 for call in output[0].get("tool_calls", [])]
                    elif protocol == "responses":
                        if stream:
                            with client.responses.stream(**params) as events:
                                result = events.get_final_response().model_dump(exclude_unset=True)
                        else:
                            result = client.responses.create(**params).model_dump(exclude_unset=True)
                        check(result["status"] == "completed")
                        output = result["output"]
                        for item in output:
                            if item["type"] == "function_call":
                                check(item.get("parsed_arguments") is None)
                                item.pop("parsed_arguments", None)
                            elif item["type"] == "message":
                                for part in item["content"]:
                                    check(part.get("parsed") is None)
                                    part.pop("parsed", None)
                        calls = [i for i in output if i["type"] == "function_call"]
                    else:
                        if stream:
                            with messages.messages.stream(**params) as events:
                                result = events.get_final_message().model_dump(exclude_none=True)
                        else:
                            result = messages.messages.create(**params).model_dump(exclude_none=True)
                        check(result["stop_reason"] == ("tool_use" if turn == 1 else "end_turn"))
                        output = [{"role": "assistant", "content": result["content"]}]
                        calls = [{"call_id": i["id"], "name": i["name"], "arguments": json.dumps(i["input"])}
                                 for i in result["content"] if i["type"] == "tool_use"]
                    if protocol == "messages":
                        check([i["type"] for i in result["content"]]
                              == (["tool_use", "tool_use"] if turn == 1 else ["text"]))
                    elif protocol == "responses":
                        expected = (["message"] if model == "qwen3.8-flash" else []) + ["function_call"] * 2
                        check([i["type"] for i in output] == (expected if turn == 1 else ["message"]))
                    else:
                        check(len(result["choices"]) == 1)
                        if turn == 1:
                            check(output[0].get("content") in (None, ""))
                    if turn == 1:
                        check([(c["call_id"], c["name"], json.loads(c["arguments"])) for c in calls] == [
                            ("call-lookup", "lookup", {"key": "A"}), ("call-second", "lookup", {"key": "B"})])
                        if protocol != "messages" and model != "claude-haiku-5-5":
                            check([c["arguments"] for c in calls] == ['{ "key": "A" }', '{"key":"B"}'])
                        if protocol == "responses":
                            empty = [i for i in output if i["type"] == "message" and i["content"] == []]
                            check(len(empty) == int(model == "qwen3.8-flash"))
                            identities = [c["id"] for c in calls]
                            check(len(set(identities)) == 2 and all(identities)
                                  and not set(identities) & {"call-lookup", "call-second"})
                            if model == "gpt-6-luna":
                                check(identities == ["call-item", "second-item"])
                        results = [("call-second", "second-result"), ("call-lookup", RESULT)]
                        if protocol == "chat":
                            new = [{"role": "tool", "tool_call_id": identity, "content": value}
                                   for identity, value in results]
                        elif protocol == "responses":
                            new = [{"type": "function_call_output", "call_id": identity, "output": value}
                                   for identity, value in results]
                        else:
                            new = [{"role": "user", "content": [
                                {"type": "tool_result", "tool_use_id": identity, "content": value}
                                for identity, value in results]}]
                    else:
                        check(not calls)
                        content = output[0]["content"]
                        answer = content if isinstance(content, str) else "".join(p["text"] for p in content)
                        check(answer == "cedar-71")
                        new = [{"role": "user", "content": "next"}]
                    params["input" if protocol == "responses" else "messages"].extend([*output, *new])
            counts[protocol] = count
    check([request.method for request in transmissions].count("POST") == 18)
    check([request.method for request in transmissions].count("GET") == 0)
    check(len(transmissions) == 18)
    print(json.dumps({"requests": counts, "http_requests": len(transmissions),
                      "three_turn_history": True, "stateless_history": True}))


if __name__ == "__main__":
    run(*sys.argv[1:])
