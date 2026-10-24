"""Pinned SDKs: managed Responses history and stateless Chat/Messages rejection."""
import json
import sys

import httpx2 as httpx
import openai
import anthropic

KEY = "synthetic-google-loop-client-000001"


def check(condition):
    if not condition:
        raise ValueError("Google SDK history assertion failed")


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
    if profile.startswith("chat-"):
        return run_chat_rejection(base, api, profile == "chat-sse")
    check(profile == "responses")
    function = {"name": "read", "strict": False, "parameters": {
        "type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}}
    transmissions = []
    requests = 0
    with httpx.Client(trust_env=False, event_hooks={"request": [transmissions.append]}) as http, \
            openai.OpenAI(base_url=base, api_key=KEY, max_retries=0,
                          timeout=5.0, http_client=http,
                          _strict_response_validation=True) as client:
        for stream in (False, True):
            params = dict(model="gemini-3.8-flash", max_output_tokens=256,
                              reasoning={"effort": "low", "summary": "auto"},
                              tools=[{"type": "function", **function}], store=True,
                              include=["reasoning.encrypted_content"],
                              input=[{"role": "user", "content": "Read."}])
            for turn in range(1, 5):
                requests += 1
                result = responses_completion(client, params, stream)
                if api == "generate_content":
                    check(result["usage"]["total_tokens"] == 15)
                else:
                    check(result["service_tier"] == "default")
                    # Preserve 10 input / 5 nonreasoning output; missing thought
                    # accounting uses six visible UTF-8 bytes in turns 1..3.
                    output = 7 if turn <= 3 else 5
                    check(result["usage"]["total_tokens"] == 10 + output)
                    check(result["usage"]["input_tokens"] == 10)
                    check(result["usage"]["output_tokens"] == output)
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


def run_chat_rejection(base, api, stream):
    transmissions = []
    with httpx.Client(trust_env=False, event_hooks={"request": [transmissions.append]}) as http, \
            openai.OpenAI(base_url=base, api_key=KEY, max_retries=0, timeout=5.0,
                          http_client=http, _strict_response_validation=True) as client:
        params = dict(model="gemini-3.8-flash", max_completion_tokens=256,
                      reasoning_effort="low", tools=[{"type":"function","function":{
                          "name":"read","strict":False,"parameters":{"type":"object",
                          "properties":{"path":{"type":"string"}},"required":["path"]}}}],
                      messages=[{"role":"user","content":"Read."}])
        try:
            value = client.chat.completions.create(**params, stream=stream)
            if stream:
                with value:
                    for event in value:
                        check(all(choice.finish_reason is None for choice in event.choices))
        except openai.APIStatusError as error:
            check(error.status_code == 502)
        except openai.APIConnectionError as error:
            check(stream and isinstance(error.__cause__, httpx.RemoteProtocolError))
        except openai.APIError as error:
            check(stream and type(error) is openai.APIError
                  and error.body["code"] == "upstream_error")
        except httpx.RemoteProtocolError:
            check(stream)
        else:
            raise ValueError("unrepresentable signature must not complete")
    check(len(transmissions) == 1)
    print(json.dumps({"upstream": api, "requests": 1, "http_requests": 1,
                      "unrepresentable_signature_rejected": True}))


if __name__ == "__main__":
    run(sys.argv[1], sys.argv[2], sys.argv[3])
