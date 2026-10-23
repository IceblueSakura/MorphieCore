"""Strict Responses delivery over sparse Chat reports; source facts stay separate."""
import json
import openai
from openai.types.responses import Response
from sdk_support import check


def check_chat_bridge(base_url, function):
    with openai.OpenAI(
        api_key="synthetic-gateway-client-token-0001", base_url=base_url,
        organization="", project="", max_retries=0, timeout=5.0,
        _strict_response_validation=True,
        http_client=openai.DefaultHttpxClient(trust_env=False, follow_redirects=False),
    ) as client:
        for stream in (False, True):
            params = dict(model="cross-model", input=[{"role": "user", "content": "lookup"}],
                          metadata={"case": "cross-tools"},
                          tools=[{"type": "function", **function}],
                          tool_choice="auto")
            if stream:
                with client.responses.stream(**params) as events:
                    terminals = 0
                    for event in events:
                        if hasattr(event, "response"):
                            Response.model_validate(event.response.model_dump(by_alias=True, exclude_unset=True))
                        terminals += event.type == "response.completed"
                    result = events.get_final_response()
                check(terminals == 1)
            else:
                result = client.responses.parse(**params)
            raw = result.model_dump(by_alias=True, exclude_unset=True)
            Response.model_validate(raw)
            check(raw["tools"] == params["tools"])
            check(raw["tool_choice"] == "auto" and raw["parallel_tool_calls"] is True)
            check(raw["usage"] == {
                "input_tokens": 3, "output_tokens": 2, "total_tokens": 5,
                "input_tokens_details": {"cached_tokens": 0, "cache_write_tokens": 0},
                "output_tokens_details": {"reasoning_tokens": 0},
            })
            check(result.model == "cross-model" and result.status == "completed")
            dumped = [item.model_dump(exclude_none=True) for item in result.output]
            check(not any("_openbridge" in item for item in dumped))
            calls = [item for item in dumped if item["type"] == "function_call"]
            check(len(calls) == 1 and calls[0]["call_id"] == "call-local"
                  and calls[0]["arguments"] == '{"n":1}'
                  and json.loads(calls[0]["arguments"]) == {"n": 1})
    return 2
