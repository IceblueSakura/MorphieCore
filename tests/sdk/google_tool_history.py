"""Pinned SDKs: managed Responses history and stateless Messages rejection."""
import json
import sys

import httpx2 as httpx
import openai
import anthropic

KEY = "synthetic-google-loop-client-000001"


def check(condition):
    if not condition:
        raise ValueError("Google SDK history assertion failed")


def chat_completion(client, params, stream):
    if stream:
        with client.chat.completions.stream(**params, stream_options={"include_usage": True}) as events:
            for _ in events:
                pass
            value = events.get_final_completion()
    else:
        value = client.chat.completions.create(**params)
    result = value.model_dump(exclude_unset=True)
    for choice in result["choices"]:
        message = choice["message"]
        if "parsed" in message:
            check(message.pop("parsed") is None)
        for call in message.get("tool_calls") or []:
            if "parsed_arguments" in call["function"]:
                check(call["function"].pop("parsed_arguments") is None)
    return result


def responses_completion(client, params, stream):
    if stream:
        with client.responses.stream(**params) as events:
            completed = 0
            for event in events:
                check(event.type not in ("error", "response.failed", "response.incomplete"))
                completed += event.type == "response.completed"
                if event.type == "response.created":
                    check(event.response.service_tier is None)
            value = events.get_final_response()
        check(completed == 1)
    else:
        value = client.responses.create(**params)
    result = value.model_dump(exclude_unset=True)
    check(result["status"] == "completed")
    for item in result["output"]:
        if item["type"] == "function_call" and "parsed_arguments" in item:
            check(item.pop("parsed_arguments") is None)
        if item["type"] == "message":
            for part in item["content"]:
                if "parsed" in part:
                    check(part.pop("parsed") is None)
    return result


def run(base, profile, api):
    check(api in ("generate_content", "interactions"))
    if profile.startswith("messages-"):
        return run_messages_rejection(base, api, profile == "messages-sse")
    function = {"name": "read", "strict": False, "parameters": {
        "type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}}
    transmissions = []
    requests = 0
    with httpx.Client(trust_env=False, event_hooks={"request": [transmissions.append]}) as http, \
            openai.OpenAI(base_url=base, api_key=KEY, max_retries=0,
                          timeout=5.0, http_client=http,
                          _strict_response_validation=profile == "responses") as client:
        for stream in (False, True):
            if profile == "chat":
                params = dict(model="gemini-3.8-flash", max_completion_tokens=256,
                              reasoning_effort="low", tools=[{"type": "function", "function": function}],
                              messages=[{"role": "user", "content": "Read."}])
            else:
                params = dict(model="gemini-3.8-flash", max_output_tokens=256,
                              reasoning={"effort": "low", "summary": "auto"},
                              tools=[{"type": "function", **function}], store=True,
                              include=["reasoning.encrypted_content"],
                              input=[{"role": "user", "content": "Read."}])
            for turn in range(1, 5):
                requests += 1
                result = (chat_completion if profile == "chat" else responses_completion)(
                    client, params, stream)
                if api == "generate_content":
                    check(result["usage"]["total_tokens"] == 15)
                else:
                    if profile == "responses":
                        check(result["service_tier"] == "default")
                    # Preserve 10 input / 5 nonreasoning output; missing thought
                    # accounting uses six visible UTF-8 bytes in turns 1..3.
                    output = 7 if turn <= 3 else 5
                    check(result["usage"]["total_tokens"] == 10 + output)
                    check(result["usage"]["prompt_tokens" if profile == "chat" else "input_tokens"] == 10)
                    check(result["usage"]["completion_tokens" if profile == "chat" else "output_tokens"] == output)
                if profile == "chat":
                    choice = result["choices"][0]
                    expected_reason = "tool_calls" if api == "interactions" and turn <= 2 else "stop"
                    check(choice["finish_reason"] == expected_reason)
                    message = choice["message"]
                    params["messages"].append(message)
                    if turn <= 2:
                        call, = message["tool_calls"]
                        check(call["id"] == f"c{turn}" and call["function"]["name"] == "read")
                        check(json.loads(call["function"]["arguments"]) == {
                            "path": "missing" if turn == 1 else "found"})
                        params["messages"].append({"role": "tool", "tool_call_id": call["id"],
                            "content": "missing file" if turn == 1 else "cedar-71"})
                    else:
                        check(message["content"] == ("cedar-71" if turn == 3 else "continued"))
                        params["messages"].append({"role": "user", "content": "Continue."})
                else:
                    output = result["output"]
                    check("mca1." not in json.dumps(result))
                    check(len([item for item in output if item["type"] == "reasoning"]) ==
                          (1 if api == "interactions" and turn <= 3 else 0))
                    params["previous_response_id"] = result["id"]
                    params["input"] = []
                    if turn <= 2:
                        call, = [item for item in output if item["type"] == "function_call"]
                        check(call["call_id"] == f"c{turn}" and call["name"] == "read")
                        check(json.loads(call["arguments"]) == {"path": "missing" if turn == 1 else "found"})
                        params["input"].append({"type": "function_call_output", "call_id": call["call_id"],
                            "output": "missing file" if turn == 1 else "cedar-71"})
                    else:
                        check(any(item["type"] == "message" and item["content"][0]["text"] ==
                                  ("cedar-71" if turn == 3 else "continued") for item in output))
                        params["input"].append({"role": "user", "content": "Continue."})
    check(requests == 8 and len(transmissions) == requests)
    print(json.dumps({"upstream": api, "requests": requests, "http_requests": len(transmissions),
                      "deliveries": 2, "tool_correction_and_final_history": True}))



def run_messages_rejection(base, api, stream):
    transmissions = []
    with httpx.Client(trust_env=False, event_hooks={"request": [transmissions.append]}) as http, \
            anthropic.Anthropic(base_url=base.removesuffix("/v1"), api_key=KEY,
                                max_retries=0, timeout=5.0, http_client=http,
                                _strict_response_validation=True) as client:
        params = dict(model="gemini-3.8-flash", max_tokens=256,
                      thinking={"type": "adaptive"}, output_config={"effort": "low"},
                      tools=[{"name": "read", "input_schema": {"type": "object",
                          "properties": {"path": {"type": "string"}}, "required": ["path"]}}],
                      messages=[{"role": "user", "content": "Read."}])
        try:
            if stream:
                with client.messages.stream(**params) as events:
                    for event in events:
                        check(event.type != "message_stop")
            else:
                client.messages.create(**params)
        except anthropic.APIStatusError as error:
            check(error.status_code in ((200, 502) if stream else (502,)))
        except httpx.RemoteProtocolError:
            check(stream)
        else:
            raise ValueError("unrepresentable signature must not complete")
    check(len(transmissions) == 1)
    print(json.dumps({"upstream": api, "requests": 1, "http_requests": 1,
                      "unrepresentable_signature_rejected": True}))


if __name__ == "__main__":
    run(sys.argv[1], sys.argv[2], sys.argv[3])
