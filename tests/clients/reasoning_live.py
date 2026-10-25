"""Explicit bounded host-Agent/Provider Chat reasoning acceptance; never auto-run.

Three requests per run, 2048 output tokens each, one attempt, no retries or
payload rewriting. Credentials are resolved only by the ordinary gateway loader.
All business inputs are test-owned; only fixed verdicts are printed. Delivered
responses are observed in bounded memory, never persisted as raw SSE fragments.
"""
import argparse
import copy
import json
from pathlib import Path
import re
import sys
import tempfile
import threading

from drivers import (Child, Pi, TEXT, check, check_client_version, environment,
                     opencode_configuration, opencode_prompt, pi_configuration)
from recovery import server, text

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "sdk"))
from messages_inputs_live import Gateway, GateError

MODEL = "qwen3.8-flash"
BINDING = "aliyun-tokenplan-cn-chat"
CAP = 2048
BUDGET = 3


def calls(message):
    return message.get("tool_calls") or []


def plain_content(value):
    """Compare text-part framing without changing delivered or replayed text."""
    if value is None:
        return ""
    if isinstance(value, str):
        return value
    check(isinstance(value, list) and all(isinstance(part, dict) and part.get("type") == "text"
          and isinstance(part.get("text"), str) for part in value), "live_nontext_history")
    return "".join(part["text"] for part in value)


def opencode_final_output(events):
    # A real tool turn can publish a pre-tool text block. The last text is the
    # final answer; verify its whole delivered message separately in State.
    texts = [event["part"]["text"] for event in events if event.get("type") == "text"]
    check(bool(texts) and texts[-1].strip() == TEXT, "live_client_final_answer")


def same_message(actual, observed):
    check(actual.get("role") == "assistant", "assistant_owner")
    check((actual.get("content") or "") == observed["content"], "answer_changed")
    check(actual.get("reasoning_content") == observed["reasoning_content"], "reasoning_changed")
    left, right = calls(actual), observed["tool_calls"]
    check(len(left) == len(right), "call_count_changed")
    for call, source in zip(left, right):
        check(call.get("id") == source["id"] and call.get("type") == "function"
              and call["function"]["name"] == source["function"]["name"]
              and json.loads(call["function"]["arguments"]) == json.loads(source["function"]["arguments"]),
              "call_identity_or_arguments_changed")


def observed_message(payload):
    """Independent finite SSE oracle over the unchanged, fully drained delivery."""
    check(len(payload) <= 2 << 20, "response_body_limit")
    wire = payload.decode("utf-8").replace("\r\n", "\n").replace("\r", "\n")
    check(wire.endswith("\n\n"), "client_stream_eof")
    message = {"content": "", "reasoning_content": "", "tool_calls": []}
    by_index = {}
    terminals = []
    done = False
    for frame in wire.split("\n\n"):
        check(len(frame.encode()) <= 1 << 20, "client_event_limit")
        data = []
        for line in frame.split("\n"):
            field, _, value = line.partition(":")
            if field == "data":
                data.append(value.removeprefix(" "))
        if not data:
            continue
        check(not done, "client_data_after_done")
        data = "\n".join(data)
        if data == "[DONE]":
            done = True
            continue
        value = json.loads(data)
        check("error" not in value, "published_error")
        for choice in value["choices"]:
            check(choice["index"] == 0 and not terminals, "client_choice_after_terminal")
            delta = choice["delta"]
            check("reasoning_details" not in delta and "reasoning" not in delta, "unexpected_reasoning_carrier")
            for key in ("content", "reasoning_content"):
                if delta.get(key) is not None:
                    message[key] += delta[key]
            for part in delta.get("tool_calls") or []:
                check(part.get("type", "function") == "function", "stream_call_type")
                call = by_index.setdefault(part["index"], {"id": "", "function": {"name": "", "arguments": ""}})
                if part.get("id"):
                    check(not call["id"] or call["id"] == part["id"], "stream_call_identity")
                    call["id"] = part["id"]
                for key in ("name", "arguments"):
                    call["function"][key] += part.get("function", {}).get(key) or ""
            if choice.get("finish_reason") is not None:
                terminals.append(choice["finish_reason"])
    check(done, "client_stream_done")
    check(len(terminals) == 1 and terminals[0] in ("tool_calls", "stop"), "honest_terminal")
    check(sorted(by_index) == list(range(len(by_index))), "stream_call_indices")
    message["tool_calls"] = [by_index[index] for index in sorted(by_index)]
    for call in message["tool_calls"]:
        check(bool(call["id"]) and bool(call["function"]["name"]), "stream_call_identity")
        check(isinstance(json.loads(call["function"]["arguments"]), dict), "stream_call_arguments")
    check(bool(message["tool_calls"]) == (terminals[0] == "tool_calls"), "stream_call_terminal")
    return message, terminals[0]


class State:
    native = False
    forward_timeout = 130

    def __init__(self, gateway, path):
        self.owner = gateway
        self.gateway = gateway.address.geturl()
        self.path = path
        self.lock = threading.Lock()
        self.accepted = threading.Event()
        self.errors = []
        self.received_requests = 0
        self.outputs = {}

    def observe_response(self, body, status, content_type, payload):
        check(status == 200, "live_response_status")
        check(content_type is not None and content_type.split(";")[0] == "text/event-stream",
              "live_response_content_type")
        output = observed_message(payload)
        with self.lock:
            ordinal = self.owner.requests.index(body)
            check(ordinal not in self.outputs, "live_duplicate_response")
            self.outputs[ordinal] = output

    def incoming(self, headers, body):
        with self.lock:
            self.received_requests += 1
        check(int(headers["content-length"]) <= 256 << 10, "live_request_bytes")
        check(headers.get("authorization") == "Bearer " + self.owner.key, "live_ingress_auth")
        check(body.get("model") == MODEL and body.get("stream") is True
              and body.get("max_completion_tokens") == CAP and body.get("reasoning_effort") == "low",
              "fixed_live_controls")
        with self.lock:
            ordinal = len(self.owner.requests) + 1
            check(ordinal <= BUDGET, "live_request_budget")
            history = body["messages"]
            assistants = [message for message in history if message["role"] == "assistant"]
            results = [message for message in history if message["role"] == "tool"]
            check(len(assistants) == ordinal - 1 and len(results) == int(ordinal > 1), "live_history_shape")
            if ordinal > 1:
                call = calls(assistants[0])
                check(len(call) == 1 and call[0]["function"]["name"] == "read"
                      and json.loads(call[0]["function"]["arguments"]).get("path") == str(self.path),
                      "live_read_call")
                check(results[0]["tool_call_id"] == call[0]["id"] and TEXT in text(results[0]["content"]),
                      "live_read_result")
                check(bool(assistants[0].get("reasoning_content")), "live_reasoning_not_observed")
            if ordinal == 3:
                check(not calls(assistants[1]) and TEXT in text(assistants[1].get("content")), "live_final_history")
                check(bool(assistants[1].get("reasoning_content")), "live_final_reasoning_not_observed")
            self.owner.requests.append(copy.deepcopy(body))

    def verify(self):
        check(not self.errors, "live_authority_failure")
        check(self.owner.process.returncode == 0, "live_gateway_shutdown")
        root = self.owner.root
        diagnostics = [json.loads(line) for line in (root / "diagnostics.jsonl").read_text().splitlines()]
        check(all(record.get("event") in ("file", "request", "log", "span") for record in diagnostics), "live_diagnostic_loss")
        requests = [record for record in diagnostics if record.get("event") == "request"]
        check(len(requests) == len(self.owner.requests) == self.received_requests == BUDGET,
              "live_actual_request_count")
        check(all(record["outcome"] == "complete" and len(record["candidates"]) == 1
                  and record.get("ignored_request_field_count", 0) == 0 for record in requests),
              "live_outcomes_or_attempts")
        capture = [json.loads(line) for line in (root / "capture.jsonl").read_text().splitlines()]
        # Response capture may intentionally reject unsafe argument fragments.
        # Only complete prepared requests are capture evidence; delivery uses the
        # in-memory observer, without weakening production credential redaction.
        prepared = [record for record in capture if record["boundary"] == "upstream_request"]
        check(len(prepared) == BUDGET and len({record["exchange"] for record in prepared}) == BUDGET,
              "live_prepared_count")
        check(all(record["dropped_before"] == 0 and not record["redacted"]
                  and record["status"] == "body_unit" for record in prepared), "live_capture_loss")
        check(sorted(self.outputs) == list(range(BUDGET)), "live_response_count")
        outputs = [self.outputs[index] for index in range(BUDGET)]
        check([terminal for _, terminal in outputs] == ["tool_calls", "stop", "stop"], "live_tool_loop_terminals")
        check(all(message["reasoning_content"] for message, _ in outputs), "live_reasoning_not_observed")
        check(all(message["content"].strip() == TEXT for message, _ in outputs[1:]), "live_final_answer")
        for index in range(BUDGET):
            wire = prepared[index]["value"]
            check(wire["model"] == MODEL and wire["max_completion_tokens"] == CAP
                  and wire["stream"] is True and wire["reasoning_effort"] == "low", "live_prepared_controls")
            source = self.owner.requests[index]["messages"]
            check([message["role"] for message in wire["messages"]] ==
                  [message["role"] for message in source], "live_prepared_history_shape")
            for actual, expected in zip(wire["messages"], source):
                if expected["role"] != "assistant":
                    check(plain_content(actual.get("content")) == plain_content(expected.get("content")),
                          "live_prepared_history_content")
                    check(actual.get("name") == expected.get("name"), "live_prepared_participant")
                if expected["role"] == "tool":
                    check(actual["tool_call_id"] == expected["tool_call_id"] ==
                          outputs[0][0]["tool_calls"][0]["id"], "live_prepared_result_identity")
            history = [message for message in wire["messages"] if message["role"] == "assistant"]
            incoming = [message for message in source if message["role"] == "assistant"]
            for earlier in range(index):
                same_message(incoming[earlier], outputs[earlier][0])
                same_message(history[earlier], outputs[earlier][0])
        return {"client_requests": BUDGET, "upstream_attempts": BUDGET,
                "readable_reasoning_replayed": True, "model_issued_tool_history": True,
                "next_turn_history": True, "failover": "not_asserted"}

    def failure_report(self):
        # Counts and fixed diagnostic classifications only; never bodies/IDs.
        path = self.owner.root / "diagnostics.jsonl"
        records = [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []
        requests = [record for record in records if record.get("event") == "request"]
        path = self.owner.root / "capture.jsonl"
        capture = [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []
        provider_errors = []
        for record in capture:
            if record.get("boundary") != "upstream_response" or record.get("status") != "body_unit":
                continue
            error = record.get("value", {}).get("error")
            if not isinstance(error, dict):
                continue
            message = str(error.get("message", "")).lower()
            code = error.get("code")
            known_codes = {"InvalidParameter", "InvalidParameterValue", "InvalidParameter.Unsupported",
                           "InvalidApiKey", "ModelNotFound", "Arrearage",
                           "invalid_parameter_error", "invalid_request_error"}
            fields = [field for field in ("model", "enable_thinking", "reasoning_effort",
                      "max_completion_tokens", "max_tokens", "tools", "tool_choice", "messages",
                      "temperature", "top_p", "preserve_thinking", "stream_options") if field in message]
            provider_errors.append({"code": code if code in known_codes else "unclassified",
                                    "mentioned_fields": fields,
                                    "unavailability_phrase_observed": any(value in message for value in
                                        ("does not exist", "not exist", "not found", "no access"))})
        return {"client_requests": self.received_requests,
                "admitted_requests": len(self.owner.requests),
                "upstream_attempts": sum(len(record.get("candidates", [])) for record in requests),
                "provider_errors": provider_errors,
                "authority_errors": [value if re.fullmatch(r"[a-z0-9_:.]+", value)
                                     else "authority_error" for value in self.errors],
                "gateway_failures": [{key: record.get(key) for key in
                    ("stage", "outcome", "upstream_status", "decode_failure", "projection_failure", "stop_reason")}
                    for record in requests]}


def run_client(case, binary, env, root, work, origin, path):
    first = f"Read {path} exactly once using read. Then reply with only the exact file contents, without quotes or markup."
    next_turn = "Repeat the exact file contents from our conversation. Do not use any tool. Reply with only those contents."
    if case == "pi-chat":
        directory, model = pi_configuration(root, origin, False, provider_timeout_ms=120000)
        env = {**env, "PI_CODING_AGENT_DIR": str(directory), "PI_CACHE_RETENTION": "short"}
        with Pi(binary, env, work, model) as client:
            client.prompt(first, ["toolUse", "stop"], timeout=130)
            client.prompt(next_turn, ["stop"], timeout=130)
    else:
        configuration = opencode_configuration(root, origin)
        env = {**env, "OPENCODE_CONFIG": str(configuration), "OPENCODE_DISABLE_AUTOUPDATE": "1",
               "OPENCODE_DISABLE_MODELS_FETCH": "1", "OPENCODE_DISABLE_PROJECT_CONFIG": "1"}
        identity, events = opencode_prompt(binary, env, work, first, "reasoning-acceptance", timeout=130,
                                          check_output=opencode_final_output)
        tools = [event for event in events if event.get("type") == "tool_use"]
        check(len(tools) == 1 and tools[0]["part"]["tool"] == "read"
              and tools[0]["part"]["state"]["status"] == "completed", "live_client_tool_execution")
        resumed, _ = opencode_prompt(binary, env, work, next_turn, "reasoning-acceptance", identity, timeout=130,
                                    check_output=opencode_final_output)
        check(resumed == identity, "live_client_session_changed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("case", choices=("pi-chat", "opencode-chat"))
    parser.add_argument("--gateway", type=Path, required=True)
    parser.add_argument("--client-binary", type=Path, required=True)
    args = parser.parse_args()
    report = {"case": args.case, "model": MODEL, "binding": BINDING,
              "request_budget": BUDGET, "output_cap": CAP, "attempt_timeout_ms": 120000}
    try:
        check(args.gateway.is_absolute() and args.client_binary.is_absolute(), "absolute_binary_paths")
        with tempfile.TemporaryDirectory(prefix="morphiecore-reasoning-live-") as directory:
            root = Path(directory)
            work = root / "work"
            work.mkdir(mode=0o700)
            path = work / "sample.txt"
            path.write_text(TEXT + "\n")
            env = environment(root)
            with Child([str(args.client_binary), "--version"], env, work) as child:
                version = child.wait(timeout=5).decode().strip()
            check_client_version(args.case, version)
            report["version"] = version
            gateway_root = root / "gateway"
            gateway_root.mkdir(mode=0o700)
            gateway = Gateway(args.gateway, gateway_root, BUDGET, model=MODEL, binding=BINDING,
                              chat_profile="readable_reasoning", capture_response=True)
            state = State(gateway, path)
            try:
                try:
                    with server(state, True) as origin:
                        run_client(args.case, str(args.client_binary), {**env, "INGRESS_KEY": gateway.key},
                                   root, work, origin, path)
                finally:
                    gateway.close()
                verdict = state.verify()
            except Exception:
                report.update(state.failure_report())
                raise
            report.update(verdict, status="passed")
    except Exception as error:
        message = str(error)
        report.update(status="failed", error=message if isinstance(error, (RuntimeError, GateError))
                      and re.fullmatch(r"[a-z0-9_:.]+", message) else type(error).__name__)
        print(json.dumps(report))
        return 1
    print(json.dumps(report))
    return 0


if __name__ == "__main__":
    sys.exit(main())
