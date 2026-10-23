"""Explicit Go/Haiku constrained-profile SDK acceptance; seven requests, no retries."""
import argparse
import base64
import copy
import json
from pathlib import Path
import sys
import tempfile

import anthropic
import httpx2 as httpx
from sdk_support import require_sdk

from messages_inputs_live import (CAP, DEADLINE_MS, GateError, Gateway, MODEL, BINDING,
                                 answer, call, request, require, user)

BUDGET = 7
OUTPUT_SCHEMA = {"type": "object", "properties": {
    "record": {"type": "integer", "enum": [71, 82]}, "ok": {"type": "boolean"}},
    "required": ["record", "ok"], "additionalProperties": False}
TOOL_SCHEMA = {"type": "object", "properties": {
    "record": {"type": "integer", "enum": [71, 82]}},
    "required": ["record"], "additionalProperties": False}
THINKING = {"type": "adaptive", "display": "summarized",
            "block_binding": {"prefix_mismatch_behavior": "drop_block"}}
EFFORT = {"role": "system", "content": [], "output_config": {"effort": "low"}}
RESULTS = {71: "cedar-71", 82: "birch-82"}


def valid_record(value):
    return type(value) is int and value in RESULTS


def native_history(messages):
    """Independent observation oracle; never used to construct a client request."""
    native = copy.deepcopy(messages)
    for message in native:
        for block in message["content"]:
            kind = block.get("type")
            if kind not in ("thinking", "redacted_thinking"):
                continue
            field = "signature" if kind == "thinking" else "data"
            sealed = block[field]
            require(isinstance(sealed, str) and sealed.startswith("mc1.") and len(sealed) <= 1 << 20,
                    "unassessed_history_carrier")
            try:
                encoded = sealed[4:]
                envelope = json.loads(base64.b64decode(encoded + "=" * (-len(encoded) % 4),
                                                       altchars=b"-_", validate=True))
            except (ValueError, UnicodeError):
                raise GateError("invalid_history_envelope") from None
            require(isinstance(envelope, dict) and set(envelope) == {"o", "f", "d"}
                    and isinstance(envelope["o"], str) and bool(envelope["o"])
                    and envelope["f"] == ("anthropic-thinking" if kind == "thinking"
                                          else "anthropic-redacted-thinking")
                    and isinstance(envelope["d"], str) and bool(envelope["d"]),
                    "invalid_history_envelope")
            block[field] = envelope["d"]
    return native


def profile(messages, tools=None, choice=None, structured=False, cap=CAP):
    body = request(messages, tools)
    body.update(max_tokens=cap, thinking=copy.deepcopy(THINKING), output_config={"effort": "high"})
    if choice is not None:
        body["tool_choice"] = choice
    if structured:
        body["output_config"]["format"] = {"type": "json_schema", "schema": OUTPUT_SCHEMA}
    return body


def sdk_generate(gateway, client, body, streamed, session):
    require(len(gateway.requests) < gateway.budget, "client_request_budget")
    require(body["model"] == MODEL and body["max_tokens"] in (1, CAP), "fixed_generation_bounds")
    wire = {**body, "stream": streamed}
    gateway.requests.append(copy.deepcopy(wire))
    params = {k: v for k, v in wire.items() if k != "stream"}
    params["extra_headers"] = {"x-morphiecore-conversation-id": session}
    if not streamed:
        return client.messages.create(**params).model_dump(exclude_none=True)
    started = ended = 0
    last_stop = None
    count = size = 0
    with client.messages.stream(**params) as stream:
        for event in stream:
            count += 1
            require(count <= 8192, "sdk_event_limit")
            if event.type == "message_start":
                started += 1
            elif event.type == "message_stop":
                ended += 1
            elif event.type == "message_delta":
                last_stop = event.delta.stop_reason
            elif event.type == "content_block_delta":
                delta = event.delta
                # Count source deltas, not SDK cumulative display snapshots.
                for field in ("text", "thinking", "signature", "partial_json"):
                    value = getattr(delta, field, None)
                    if isinstance(value, str):
                        size += len(value.encode())
                require(size <= 8 << 20, "sdk_delta_byte_limit")
        final = stream.get_final_message().model_dump(exclude_none=True)
    require(started == ended == 1 and final.get("stop_reason") == last_stop, "sdk_stream_terminal")
    return final


def structured(gateway, client):
    messages = [user("Return a JSON object with record 93 and ok true. No commentary."),
                copy.deepcopy(EFFORT)]
    first = sdk_generate(gateway, client, profile(messages, structured=True), False, "constrained-json")
    value = json.loads(answer(first))
    require(isinstance(value, dict) and set(value) == {"record", "ok"}
            and valid_record(value["record"]) and value["ok"] is True, "structured_json_answer")
    history = messages + [{"role": "assistant", "content": first["content"]},
                          user("Now return record 94 and ok true, using the same output format."),
                          copy.deepcopy(EFFORT)]
    expected = native_history(history)
    second = sdk_generate(gateway, client, profile(history, structured=True), True, "constrained-json")
    value = json.loads(answer(second))
    require(isinstance(value, dict) and set(value) == {"record", "ok"}
            and valid_record(value["record"]) and value["ok"] is True, "structured_sse_history_answer")
    truncated = sdk_generate(gateway, client, profile(messages, structured=True, cap=1), True, "constrained-truncation")
    require(truncated.get("stop_reason") == "max_tokens", "truncation_not_observed")
    return {1: expected}


def forced_tools(gateway, client):
    histories = {}
    tools = [{"name": name, "description": "Retrieve one synthetic record.",
              "strict": True, "input_schema": TOOL_SCHEMA} for name in ("lookup", "inspect")]
    for named in (False, True):
        selection = {"type": "tool", "name": "inspect"} if named else {"type": "any"}
        selection["disable_parallel_tool_use"] = True
        prompt = ("Use lookup for record 94." if named else
                  "Return record 93. Prefer plain text and avoid tools.")
        messages = [user(prompt), copy.deepcopy(EFFORT)]
        session = "forced-named" if named else "forced-required"
        first = sdk_generate(gateway, client, profile(messages, tools, selection), named, session)
        calls = [block for block in first.get("content", []) if block.get("type") == "tool_use"]
        require(len(calls) == 1 and calls[0].get("name") in ("lookup", "inspect"), "forced_call_count_or_name")
        arguments = calls[0].get("input")
        require(isinstance(arguments, dict) and set(arguments) == {"record"}
                and valid_record(arguments["record"]), "strict_tool_schema_answer")
        identity = call(first, "inspect" if named else calls[0]["name"], arguments)
        value = RESULTS[arguments["record"]]
        messages += [{"role": "assistant", "content": first["content"]},
                     {"role": "user", "content": [{"type": "tool_result", "tool_use_id": identity,
                        "content": value}, {"type": "text", "text": "Return the given result as plain text."}]},
                     copy.deepcopy(EFFORT)]
        histories[6 if named else 4] = native_history(messages)
        final = sdk_generate(gateway, client, profile(messages, tools, {"type": "none"}), named, session)
        require(value in answer(final) and not any(block.get("type") == "tool_use"
                for block in final["content"]), "forced_tool_final")
    return histories


def verify_wires(wires, histories):
    require(len(wires) == BUDGET, "constrained_request_count")
    for body in wires:
        require(body["thinking"] == THINKING and body["output_config"]["effort"] == "high",
                "constrained_thinking_or_root_effort_changed")
        controls = [message for message in body["messages"] if message["role"] == "system"]
        require(bool(controls) and all(control == EFFORT for control in controls), "constrained_message_effort_changed")
    for body in wires[:3]:
        require(body["output_config"]["format"] == {"type": "json_schema", "schema": OUTPUT_SCHEMA},
                "structured_schema_changed")
    require([body["max_tokens"] for body in wires] == [CAP, CAP, 1, CAP, CAP, CAP, CAP], "constrained_cap_changed")
    # The SDK omits stream for JSON create; native absence means false.
    require(all(body.get("stream", False) is expected for body, expected in zip(
        wires, [False, True, True, False, False, True, True], strict=True)), "constrained_stream_changed")
    require([body.get("tool_choice") for body in wires[3:]] == [
        {"type": "any", "disable_parallel_tool_use": True}, {"type": "none"},
        {"type": "tool", "name": "inspect", "disable_parallel_tool_use": True}, {"type": "none"}],
        "forced_tool_choice_changed")
    for body in wires[3:]:
        require([tool["name"] for tool in body["tools"]] == ["lookup", "inspect"]
                and all(tool.get("strict") is True and tool["input_schema"] == TOOL_SCHEMA
                        for tool in body["tools"]), "forced_strictness_or_schema_changed")
    require(set(histories) == {1, 4, 6}, "missing_returned_history_oracle")
    for index, expected in histories.items():
        require(wires[index]["messages"] == expected, "prepared_returned_history_changed")
    for index, continuation in [(0, 1), (2, 1), (3, 4), (5, 6)]:
        require(wires[index]["messages"] == histories[continuation][:2], "prepared_initial_messages_changed")


def failure_summary(gateway):
    """Read only application-owned, body-free records before scratch cleanup."""
    path = gateway.root / "diagnostics.jsonl"
    records = [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []
    requests = [record for record in records if record.get("event") == "request"]
    fields = ("outcome", "stage", "upstream_status", "decode_failure", "projection_failure")
    return {"client_requests_started": len(gateway.requests),
            "gateway_requests": len(requests),
            "upstream_attempts": sum(len(record.get("candidates", [])) for record in requests),
            "diagnostics": [{field: record.get(field) for field in fields} for record in requests]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--gateway-bin", required=True, type=Path)
    args = parser.parse_args()
    require_sdk("anthropic")
    require(args.gateway_bin.is_absolute() and args.gateway_bin.is_file(), "explicit_gateway_binary")
    report = {"model": MODEL, "binding": BINDING, "sdk_version": anthropic.__version__,
              "request_budget": BUDGET, "max_tokens": CAP, "truncation_cap": 1,
              "attempt_timeout_ms": DEADLINE_MS}
    try:
        with tempfile.TemporaryDirectory(prefix="morphiecore-messages-capabilities-") as directory:
            gateway = Gateway(args.gateway_bin, Path(directory), BUDGET)
            try:
                origin = f"http://127.0.0.1:{gateway.address.port}"
                with httpx.Client(trust_env=False, timeout=130) as http:
                    with anthropic.Anthropic(api_key=gateway.key, base_url=origin,
                            max_retries=0, http_client=http) as client:
                        histories = structured(gateway, client)
                        histories.update(forced_tools(gateway, client))
            except Exception:
                gateway.close()
                report.update(failure_summary(gateway))
                raise
            finally:
                gateway.close()
            try:
                verify_wires(gateway.verify(exact=True), histories)
            except Exception:
                report.update(failure_summary(gateway))
                raise
            report.update(status="passed", client_requests=BUDGET, upstream_attempts=BUDGET,
                          structured_json_sse_history=True, honest_truncation=True,
                          strict_required_and_named_tools=True, model_issued_history=True,
                          numeric_schema_conflicts=True,
                          signed_history_blocks=sum(block["type"] in ("thinking", "redacted_thinking")
                              for messages in histories.values() for message in messages
                              for block in message["content"]))
    except Exception as error:
        report.update(status="failed", error=str(error) if isinstance(error, GateError) else type(error).__name__)
        print(json.dumps(report))
        return 1
    print(json.dumps(report))
    return 0


if __name__ == "__main__":
    sys.exit(main())
