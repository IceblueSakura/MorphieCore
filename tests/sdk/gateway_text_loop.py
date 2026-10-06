"""Exercise the production gateway Router against a synthetic HTTP Provider only."""
import ipaddress
import json
import sys
from sdk_support import check
from models_checks import check_models
from speech_checks import check_speech
from tokenplan_audio_checks import check_native_audio
from urllib.parse import urlsplit

import openai
import httpx2


def run(base_url: str) -> None:
    """Verify real SDK tools, reasoning replay and JSON/SSE consumption without retries."""
    parsed = urlsplit(base_url)
    if (openai.__version__ != "3.24.0" or parsed.scheme != "http"
            or not parsed.hostname or not parsed.port
            or not ipaddress.ip_address(parsed.hostname).is_loopback
            or parsed.path != "/v1" or parsed.query or parsed.fragment
            or parsed.username or parsed.password):
        raise ValueError("pinned SDK and literal loopback /v1 required")
    schema = {"type": "object", "properties": {"n": {"type": "integer"}},
              "required": ["n"], "additionalProperties": False}
    function = {"name": "lookup", "parameters": schema, "strict": True}
    requests = 0
    with openai.OpenAI(
        api_key="synthetic-gateway-client-token-0001", base_url=base_url,
        organization="", project="", max_retries=0, timeout=5.0,
        _strict_response_validation=True,
        http_client=openai.DefaultHttpxClient(trust_env=False, follow_redirects=False),
    ) as client:
        model_requests = check_models(client)
        speech_requests = check_speech(client)
        native_audio_requests = check_native_audio(client)
        invalid = [
            {"reasoning": {"summary": False}},
            {"extra_body": {"session_id": "synthetic-session"}},
            {"extra_body": {"session_id": None}},
            {"tools": [{"type": "function", **function, "async": None}]},
            {"tools": [{"type": "function", **function, "defer_loading": None}]},
            {"tools": [{"type": "function", "name": "lookup", "strict": True,
                        "parameters": {"type": "object", "properties": {"n": {"type": "integer"}}}}]},
            {"tools": [{"type": "function", **function}],
             "tool_choice": {"type": "function", "name": "undeclared"}},
            {"input": [{"type": "function_call_output", "call_id": "missing", "output": "x"}]},
        ]
        # Admission precedes delivery selection; only the SDK's streaming error path
        # needs a second representative request, not the entire invalid-input matrix.
        for stream, extra in [(False, extra) for extra in invalid] + [(True, invalid[0])]:
            requests += 1
            try:
                client.responses.create(**{
                    "model": "public-model", "input": "lookup", "stream": stream, **extra})
            except openai.APIStatusError as error:
                check(error.status_code == 400)
            else:
                check(False, "invalid request must fail before upstream I/O")
        for stream in (False, True):
            history = [{"role": "user", "content": "lookup"}]
            for turn in (1, 2):
                params = dict(model="public-model", messages=history,
                              tools=[{"type": "function", "function": function}],
                              tool_choice="auto" if turn == 1 else "none")
                if stream:
                    with client.chat.completions.stream(
                        **params, stream_options={"include_usage": True, "include_obfuscation": False}
                    ) as events:
                        for _ in events:
                            pass
                        result = events.get_final_completion()
                else:
                    result = client.chat.completions.parse(**params)
                requests += 1
                check(result.model == "public-model" and result.usage.total_tokens == 5)
                message = result.choices[0].message
                if turn == 1:
                    call = message.tool_calls[0]
                    check(call.id == "call-local" and json.loads(call.function.arguments) == {"n": 1})
                    history.append(message.model_dump(exclude_none=True))
                    history.append({"role": "tool", "tool_call_id": call.id, "content": '{"n":1}'})
                else:
                    check(result.choices[0].finish_reason == "stop" and message.content == "old 🧪")
        for stream in (False, True):
            history = [{"role": "user", "content": [
                {"type": "input_text", "text": "lookup"},
                {"type": "input_file", "file_data": "data:application/pdf;base64,AQID",
                 "filename": "synthetic.pdf", "detail": "low"},
                {"type": "input_image", "image_url": "data:image/png;base64,AQID"},
                {"type": "input_file", "file_url": "https://example.invalid/resource?sig=synthetic",
                 "filename": "remote.pdf"},
            ]}]
            tools = [{"type": "function", **function},
                     {"type": "custom", "name": "sql", "format": {
                         "type": "grammar", "syntax": "regex", "definition": "SELECT [0-9]+"}}]
            for turn in (1, 2):
                params = dict(model="public-model", input=history, instructions="Answer precisely",
                              store=False, tools=tools, tool_choice="auto" if turn == 1 else "none",
                              reasoning={"summary": "auto" if turn == 1 else None})
                if stream:
                    completed = 0
                    with client.responses.stream(**params) as events:
                        for event in events:
                            completed += event.type == "response.completed"
                        result = events.get_final_response()
                    check(completed == 1)
                else:
                    result = client.responses.parse(**params)
                requests += 1
                check(result.model == "public-model" and result.status == "completed")
                check(result.usage.total_tokens == 8 and result.max_output_tokens == 32)
                if turn == 1:
                    dumped = [item.model_dump(exclude_none=True) for item in result.output]
                    check({item["call_id"] for item in dumped if item["type"] in (
                        "function_call", "custom_tool_call")} == {"c_lookup", "c_sql"})
                    check(any(item.get("encrypted_content") == "synthetic-final-token" for item in dumped))
                    history.extend(dumped)
                    history.extend([
                        {"type": "function_call_output", "call_id": "c_lookup", "output": '{"n":1}'},
                        {"type": "custom_tool_call_output", "call_id": "c_sql", "output": "1"},
                    ])
                else:
                    check(result.output_text == '{"ok":false}')
        # Validate empty owner items with a complete independent Responses envelope;
        # do not invent missing Chat-reported settings to satisfy strict SDK validation.
        for stream in (False, True):
            params = dict(model="public-model", input="lookup", store=False,
                          metadata={"case": "empty-owner"},
                          tools=[{"type": "function", **function}])
            if stream:
                completed = 0
                with client.responses.stream(**params) as events:
                    for event in events:
                        completed += event.type == "response.completed"
                    result = events.get_final_response()
                check(completed == 1)
            else:
                result = client.responses.parse(**params)
            requests += 1
            check(result.model == "public-model" and result.status == "completed")
            check(len(result.output) == 2)
            owner, call = result.output
            check(owner.type == "message" and owner.status == "completed" and owner.content == [])
            check(call.type == "function_call" and call.call_id == "call-local")
            check(json.loads(call.arguments) == {"n": 1})
        # Upstream-only accounting has no standard downstream carrier. Static
        # delivery fails; a published stream must abort without a fake terminal.
        for stream in (False, True):
            requests += 1
            completed = 0
            received_events = 0
            params = dict(model="public-model", input="lookup",
                          metadata={"case": "nonstandard-usage"})
            try:
                if stream:
                    with client.responses.stream(**params) as events:
                        for event in events:
                            received_events += 1
                            completed += event.type == "response.completed"
                        events.get_final_response()
                else:
                    client.responses.create(**params)
            except (openai.APIError, httpx2.TransportError, RuntimeError) as error:
                if not stream:
                    check(isinstance(error, openai.APIStatusError) and error.status_code == 502)
                else:
                    check(received_events > 0)
                check(completed == 0)
            else:
                check(False, "unrepresentable accounting must not be dropped for success")
        for index, count in enumerate((1, 2, 2)):
            params = dict(model="public-image", prompt="synthetic image", n=count,
                stream=False, size="1536x1024", quality="high", background="transparent",
                output_format="webp", output_compression=80, moderation="low", user="synthetic-sdk-user")
            requests += 1
            if index == 2:
                try:
                    client.images.generate(**params)
                except openai.APIStatusError as error:
                    check(error.status_code == 502 and "data" not in error.response.json())
                else:
                    raise ValueError("bad second artifact must fail the entire response")
                continue
            image = client.images.generate(**params)
            check(image.created == 7 and image.data is not None and len(image.data) == count)
            check([item.b64_json for item in image.data] == ["AQID", "BAUG"][:count])
            check(all(item.url is None for item in image.data))
            check(image.size == "1536x1024" and image.quality == "high"
                  and image.background == "transparent" and image.output_format == "webp")
            check(image.usage is None)
    print(json.dumps({"requests": requests + model_requests + speech_requests + native_audio_requests,
                     "model_requests": model_requests, "native_audio_requests": native_audio_requests,
                     "protocols": 2, "deliveries": 2, "image_requests": 3, "speech_requests": speech_requests}))


if __name__ == "__main__":
    run(sys.argv[1])
