"""Pinned SDKs: stateless Chat/Messages rejection for unrepresentable Google signatures."""
import json
import sys

import httpx2 as httpx
import openai
import anthropic

KEY = "synthetic-google-loop-client-000001"


def check(condition):
    if not condition:
        raise ValueError("Google SDK history assertion failed")


def run(base, profile, api):
    check(api == "interactions")
    if profile.startswith("messages-"):
        return run_messages_rejection(base, api, profile == "messages-sse")
    if profile.startswith("chat-"):
        return run_chat_rejection(base, api, profile == "chat-sse")
    raise ValueError(f"unsupported profile: {profile}")



def run_messages_rejection(base, api, stream):
    transmissions = []
    with httpx.Client(trust_env=False, event_hooks={"request": [transmissions.append]}) as http, \
            anthropic.Anthropic(base_url=base.removesuffix("/v1"), api_key=KEY,
                            max_retries=0, timeout=5.0, http_client=http,
                            _strict_response_validation=True) as client:
        params = dict(model="gemini-4-argon", max_tokens=256,
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
        params = dict(model="gemini-4-argon", max_completion_tokens=256,
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
