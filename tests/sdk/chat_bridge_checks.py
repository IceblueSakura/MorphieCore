"""Strict Responses delivery over sparse Chat reports; source facts stay separate."""
import json
from openai.types.responses import Response
from sdk_support import check


def check_chat_bridge(client, function):
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
                        check(event.response.metadata == params["metadata"],
                              "opening and terminal Responses must echo the admitted association")
                    terminals += event.type == "response.completed"
                result = events.get_final_response()
            check(terminals == 1)
        else:
            result = client.responses.parse(**params)
        raw = result.model_dump(by_alias=True, exclude_unset=True)
        Response.model_validate(raw)
        check(raw["metadata"] == params["metadata"])
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
