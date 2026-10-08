"""SDK-default consumption of faithful Chat reports; never fabricate missing facts."""
import json
import openai
from openai.types.responses import Response
from pydantic import ValidationError
from sdk_support import check


def check_chat_bridge(base_url, function):
    with openai.OpenAI(
        api_key="synthetic-gateway-client-token-0001", base_url=base_url,
        organization="", project="", max_retries=0, timeout=5.0,
        http_client=openai.DefaultHttpxClient(trust_env=False, follow_redirects=False),
    ) as client:
        for stream in (False, True):
            history = [{"role": "user", "content": "lookup"}]
            for turn in (1, 2):
                params = dict(model="cross-model", input=history,
                              metadata={"case": "cross-tools"},
                              tools=[{"type": "function", **function}],
                              tool_choice="auto" if turn == 1 else "none")
                if stream:
                    with client.responses.stream(**params) as events:
                        terminals = 0
                        for event in events:
                            terminals += event.type == "response.completed"
                        result = events.get_final_response()
                    check(terminals == 1)
                else:
                    result = client.responses.parse(**params)
                    # The independently selected Faithful contract intentionally does
                    # not satisfy SDK StrictComplete when Chat did not report facts.
                    raw = result.model_dump(exclude_unset=True)
                    check(all(key not in raw for key in ("tools", "tool_choice", "parallel_tool_calls")))
                    check("input_tokens_details" not in raw["usage"])
                    try:
                        Response.model_validate(raw)
                    except ValidationError as error:
                        check({e["loc"] for e in error.errors()} == {
                            ("tools",), ("tool_choice",), ("parallel_tool_calls",),
                            ("usage", "input_tokens_details"), ("usage", "output_tokens_details"),
                        })
                    else:
                        check(False, "unreported facts cannot become strict-complete")
                check(result.model == "cross-model" and result.status == "completed")
                dumped = [item.model_dump(exclude_none=True) for item in result.output]
                check(not any("_openbridge" in item for item in dumped))
                if turn == 1:
                    calls = [item for item in dumped if item["type"] == "function_call"]
                    check(len(calls) == 1 and calls[0]["call_id"] == "call-local"
                          and calls[0]["arguments"] == '{"n":1}'
                          and json.loads(calls[0]["arguments"]) == {"n": 1})
                    history.extend(dumped)
                    history.append({"type": "function_call_output",
                                    "call_id": "call-local", "output": '{"n":1}'})
                else:
                    check(result.output_text == "old 🧪")
    return 4
