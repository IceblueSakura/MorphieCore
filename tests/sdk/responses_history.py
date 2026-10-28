"""Pinned Responses SDK: phases, opaque replay, retrieval and stored/stateless history."""
import json
import sys
import openai
from openai.types.responses import Response
from openai.types.responses.response_item import ResponseItem
from pydantic import TypeAdapter

KEY = "synthetic-tool-history-client-0001"
RESULT = '{"error":"synthetic lookup unavailable","label":"cedar-71"}'


def check(condition):
    if not condition:
        raise ValueError("SDK tool history assertion failed")


def responses_completion(client, params, stream, status="completed"):
    if stream:
        with client.responses.stream(**params) as events:
            completed = 0
            terminal_response = None
            for event in events:
                if hasattr(event, "response"):
                    Response.model_validate(event.response.model_dump(by_alias=True, exclude_unset=True))
                check(event.type not in ("error", "response.failed"))
                if event.type in ("response.created", "response.in_progress"):
                    opening = event.response.model_dump(exclude_unset=True)
                    check(opening["reasoning"] == {"effort": "low", "summary": "auto"})
                    check(opening["service_tier"] == "auto")
                if event.type == f"response.{status}":
                    completed += 1
                    terminal_response = event.response
            # The pinned high-level helper only finalizes response.completed.
            # Incomplete continuation uses the SDK's typed response.incomplete event.
            value = events.get_final_response() if status == "completed" else terminal_response
        check(completed == 1)
    else:
        value = client.responses.create(**params)
    check(value.status == status)
    result = value.model_dump(exclude_unset=True)
    # The streaming accumulator adds SDK-only parsed=None attributes. They are
    # not wire observations and must not become fields in replayed input items.
    for item in result["output"]:
        if item["type"] == "function_call" and "parsed_arguments" in item:
            check(item.pop("parsed_arguments") is None)
        if item["type"] == "message":
            for part in item["content"]:
                if "parsed" in part:
                    check(part.pop("parsed") is None)
    check(result["reasoning"] == {"effort": "low", "summary": "detailed",
                                  "context": "all_turns", "mode": "standard"})
    check(result["service_tier"] == "default")
    return result



def run(base):
    function = {"name": "lookup", "description": "Synthetic lookup", "strict": False,
                "parameters": {"type": "object", "properties": {"key": {"type": "string"}},
                               "required": ["key"]}}
    requests = 0
    with openai.OpenAI(base_url=base, api_key=KEY, max_retries=0, timeout=5.0,
                      _strict_response_validation=True) as client:
        for stream in (False, True):
            for stored in (False, True):
                params = dict(model="gpt-6-luna", tools=[{"type": "function", **function}],
                              input=[{"role": "user", "content": "lookup"}], store=stored,
                              reasoning={"effort": "low", "summary": "auto"},
                              include=["reasoning.encrypted_content"], max_output_tokens=256)
                for turn in range(1, 4):
                    requests += 1
                    result = responses_completion(client, params, stream)
                    check(result["usage"]["total_tokens"] == 18)
                    check(result["usage"]["output_tokens_details"]["reasoning_tokens"] == 2)
                    output = result["output"]
                    if turn == 1:
                        reason, = [i for i in output if i["type"] == "reasoning"]
                        check(reason["summary"][0]["text"] == "Plan 🧪" and reason.get("encrypted_content"))
                        check(any(i.get("phase") == "commentary" for i in output))
                        call, second = [i for i in output if i["type"] == "function_call"]
                        check(call["call_id"] == "call-lookup" and call["name"] == "lookup"
                              and call["arguments"] == '{ "key": "A" }')
                        check(second["call_id"] == "call-second" and second["id"] != second["call_id"]
                              and second["name"] == "lookup" and second["arguments"] == '{"key":"B"}')
                        new = [{"type": "function_call_output", "call_id": second["call_id"], "output": "second-result"},
                               {"type": "function_call_output", "call_id": call["call_id"], "output": RESULT}]
                    else:
                        check(any(i.get("phase") == "final_answer" and
                                  i["content"][0]["text"] == "cedar-71" for i in output))
                        new = [{"role": "user", "content": "next"}]
                    if stored:
                        params.update(previous_response_id=result["id"], input=new)
                        retrieved = client.responses.retrieve(result["id"])
                        retained = retrieved.model_dump(exclude_unset=True)
                        check({k: v for k, v in retained.items() if k != "morphiecore"}
                              == {k: v for k, v in result.items() if k != "morphiecore"})
                        for item in client.responses.input_items.list(result["id"]):
                            TypeAdapter(ResponseItem).validate_python(item.model_dump(by_alias=True, exclude_unset=True))
                    else:
                        params["input"].extend([*output, *new])
        for stream in (False, True):
            params = dict(model="gpt-6-luna", tools=[{"type": "function", **function}],
                          input=[{"role": "user", "content": "incomplete"}], store=True,
                          reasoning={"effort": "low", "summary": "auto"},
                          include=["reasoning.encrypted_content"], max_output_tokens=256)
            result = responses_completion(client, params, stream, status="incomplete")
            requests += 1
            check(result["incomplete_details"]["reason"] == "max_output_tokens")
            reason, = [i for i in result["output"] if i["type"] == "reasoning"]
            check(reason["status"] == "incomplete" and reason["encrypted_content"].startswith("mc1."))
            retained = client.responses.retrieve(result["id"]).model_dump(exclude_unset=True)
            check({k: v for k, v in retained.items() if k != "morphiecore"}
                  == {k: v for k, v in result.items() if k != "morphiecore"})
            params.update(previous_response_id=result["id"], input=[
                {"type": "function_call_output", "call_id": "call-second", "output": "second-result"},
                {"type": "function_call_output", "call_id": "call-lookup", "output": RESULT},
            ])
            resumed = responses_completion(client, params, stream)
            requests += 1
            check(any(i.get("phase") == "final_answer" and i["content"][0]["text"] == "cedar-71"
                      for i in resumed["output"]))
    print(json.dumps({"requests": requests, "protocols": 1, "deliveries": 2,
                      "three_turn_history": True, "stored_stateless": True}))


if __name__ == "__main__":
    run(sys.argv[1])
