"""Explicit bounded Go/Haiku input acceptance through the CLI credential loader.

Never used by offline discovery. Only synthetic business inputs are sent; output
contains fixed status codes/counts, never credentials, captures or model prose.
"""
import argparse
from contextlib import closing
import copy
import http.client
import json
from pathlib import Path
import secrets
import select
import signal
import subprocess
import sys
import tempfile
from urllib.parse import urlsplit

MODEL = "claude-haiku-5-5"
BINDING = "opencode-go-haiku-messages"
CAP = 2048
DEADLINE_MS = 120000
REQUEST_BYTES = 256 << 10
RESPONSE_BYTES = 8 << 20
BUDGETS = {"examples": 3, "context": 3, "cache": 4}


class GateError(Exception):
    pass


def require(condition, code):
    if not condition:
        raise GateError(code)


def private_json(path, value):
    import os
    with open(path, "x", encoding="utf-8", opener=lambda p, f: os.open(p, f, 0o600)) as output:
        json.dump(value, output)


class Gateway:
    def __init__(self, binary, directory, budget, *, model=MODEL, binding=BINDING,
                 chat_profile=None, capture_response=False):
        self.root = directory
        self.budget = budget
        self.requests = []
        self.key = secrets.token_urlsafe(32)
        capture = {"path": str(directory / "capture.jsonl"),
                   "upstream_request": {"body": True}, "max_unit_bytes": 1048576}
        if capture_response:
            capture["client_response"] = {"body": True}
            capture["upstream_response"] = {"body": True}
        private_json(directory / "auth.json", {"version": 1, "agents": [{"agent_id": "fixture-agent", "key": self.key}]})
        private_json(directory / "gateway.json", {
            "auth_file": "auth.json", "bind": "127.0.0.1:0", "models_file": "models.json",
            "exchange_timeout_ms": DEADLINE_MS, "diagnostics": "diagnostics.jsonl",
            "capture": capture})
        candidate = {"binding": binding, "attempt_timeout_ms": DEADLINE_MS,
                     "maximum_output_tokens": CAP}
        if chat_profile is None:
            candidate["default_output_tokens"] = None
        selection = {
            "max_attempts": 1, "total_timeout_ms": DEADLINE_MS,
            "candidates": [candidate]}
        if chat_profile is not None:
            selection["chat_profile"] = chat_profile
        private_json(directory / "models.json", {"models": {model: selection}})
        # Inherit HOME only for the application loader's ordinary store resolution.
        # Do not read private configuration, credentials or environment values here.
        self.process = subprocess.Popen([str(binary), "--config", str(directory / "gateway.json")],
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        try:
            require(bool(select.select([self.process.stdout], [], [], 10)[0]), "gateway_readiness_deadline")
            ready = self.process.stdout.readline(128).decode().strip()
            require(ready.startswith("MorphieCore listening on http://127.0.0.1:"), "gateway_startup")
            self.address = urlsplit(ready.removeprefix("MorphieCore listening on "))
            require(self.address.hostname == "127.0.0.1" and self.address.port is not None, "gateway_address")
        except Exception:
            self.close()
            raise

    def close(self):
        if self.process.poll() is None:
            self.process.send_signal(signal.SIGINT)
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=3)
        self.process.stdout.close()

    def exchange(self, method, path, body=None, session="messages-input-acceptance"):
        raw = None if body is None else json.dumps(body).encode()
        require(raw is None or len(raw) <= REQUEST_BYTES, "request_byte_limit")
        headers = {"authorization": "Bearer " + self.key, "content-type": "application/json",
                   "x-morphiecore-conversation-id": session}
        if path == "/v1/messages":
            headers["anthropic-version"] = "2026-10-21"
        with closing(http.client.HTTPConnection(self.address.hostname, self.address.port, timeout=130)) as connection:
            connection.request(method, path, raw, headers)
            response = connection.getresponse()
            data = response.read(RESPONSE_BYTES + 1)
            require(len(data) <= RESPONSE_BYTES, "response_byte_limit")
            value = json.loads(data)
            if response.status != 200:
                code = value.get("error", {}).get("code")
                # Only fixed gateway codes are eligible for body-free reporting.
                known = {"invalid_request", "unsupported_request", "upstream_error", "upstream_unavailable",
                         "rate_limit_exceeded", "server_is_overloaded", "insufficient_quota"}
                raise GateError("http_" + str(response.status) + (":" + code if code in known else ""))
            return value

    def generate(self, body, session="messages-input-acceptance"):
        require(len(self.requests) < self.budget, "client_request_budget")
        require(body["model"] == MODEL and body["max_tokens"] == CAP and body["stream"] is False,
                "fixed_generation_bounds")
        self.requests.append(copy.deepcopy(body))
        reply = self.exchange("POST", "/v1/messages", body, session)
        require(reply.get("type") == "message" and reply.get("role") == "assistant", "native_response_shape")
        require(isinstance(reply.get("content"), list), "native_response_content")
        require(reply.get("stop_reason") in ("tool_use", "end_turn"), "incomplete_native_outcome")
        return reply

    def verify(self, exact=False):
        require(self.process.returncode == 0, "gateway_shutdown")
        records = [json.loads(line) for line in (self.root / "diagnostics.jsonl").read_text().splitlines()]
        require(all(record.get("event") in ("file", "request", "log", "span") for record in records), "diagnostic_loss")
        requests = [record for record in records if record.get("event") == "request"]
        require(len(requests) == len(self.requests) == self.budget, "actual_request_count")
        require(all(record["outcome"] == "complete" and len(record["candidates"]) == 1
                    and record.get("ignored_request_field_count", 0) == 0 for record in requests),
                "gateway_outcome_or_attempt_count")
        capture = [json.loads(line) for line in (self.root / "capture.jsonl").read_text().splitlines()]
        require(all(record["dropped_before"] == 0 and record["status"] == "body_unit"
                    and record["boundary"] == "upstream_request" for record in capture), "capture_incomplete")
        require(len(capture) == self.budget, "captured_request_count")
        require(len({record["exchange"] for record in capture}) == self.budget,
                "duplicate_captured_exchange")
        if exact:
            require(all(record["redacted"] is False for record in capture), "redacted_history_observation")
        return [record["value"] for record in capture]


def request(messages, tools=None):
    value = {"model": MODEL, "max_tokens": CAP, "stream": False, "messages": messages}
    if tools is not None:
        value["tools"] = tools
    return value


def user(text):
    return {"role": "user", "content": [{"type": "text", "text": text}]}


def call(reply, name, arguments):
    tools = [block for block in reply["content"] if block.get("type") == "tool_use"]
    require(reply["stop_reason"] == "tool_use" and len(tools) == 1
            and tools[0].get("name") == name and tools[0].get("input") == arguments,
            "expected_model_call_not_observed")
    return tools[0]["id"]


def examples(gateway):
    schema = {"type": "object", "properties": {"label": {"type": "string"}},
              "required": ["label"], "additionalProperties": False}
    root = {"name": "lookup", "description": "Look up one synthetic label.", "strict": False,
            "input_schema": schema, "input_examples": [{"label": "cedar"}, {"label": "birch"}]}
    placeholder = {"name": "__morphiecore_deferred__", "strict": False,
                   "input_schema": {"type": "object"}, "defer_loading": True}
    tools = [root, placeholder]
    messages = [user("Call lookup exactly once with label cedar. Wait for its result before answering.")]
    first = gateway.generate(request(messages, tools))
    first_id = call(first, "lookup", {"label": "cedar"})
    inline = {"name": "inspect", "description": "Inspect one synthetic label.", "strict": False,
              "input_schema": schema, "input_examples": [{"label": "birch"}, {"label": "cedar"}]}
    messages += [{"role": "assistant", "content": first["content"]},
                 {"role": "user", "content": [{"type": "tool_result", "tool_use_id": first_id,
                    "content": "cedar-71"}, {"type": "text", "text":
                    "Now call inspect exactly once with label birch. Do not call lookup again."}]},
                 {"role": "system", "content": [{"type": "tool_addition", "tool": {
                    "type": "tool_definition", "definition": inline}}]}]
    second = gateway.generate(request(messages, tools))
    second_id = call(second, "inspect", {"label": "birch"})
    messages += [{"role": "assistant", "content": second["content"]},
                 {"role": "user", "content": [{"type": "tool_result", "tool_use_id": second_id,
                    "content": "birch-82"}, {"type": "text", "text":
                    "Return cedar-71 and birch-82 as text without more tool calls."}]}]
    final = gateway.generate(request(messages, tools))
    answer = "".join(block.get("text", "") for block in final["content"] if block.get("type") == "text")
    require(final["stop_reason"] == "end_turn" and "cedar-71" in answer and "birch-82" in answer,
            "examples_final_answer")
    return root, inline


def document(value, context):
    return {"type": "document", "source": {"type": "text", "media_type": "text/plain",
                                          "data": "The value is " + value + "."}, "context": context}


def answer(reply):
    require(reply["stop_reason"] == "end_turn", "expected_text_completion")
    return "".join(block.get("text", "") for block in reply["content"] if block.get("type") == "text")


def contexts(gateway):
    first = document("cedar-71", "Record label is ALPHA.")
    second = document("cedar-71", "Record label is BETA.")
    direct = gateway.generate(request([{"role": "user", "content": [
        first, second, {"type": "text", "text":
            "Read both document uses. For each one, return its context's record label and its document value "
            "on a separate line. Do not combine the records or omit either value."}]}]))
    text = answer(direct)
    require("ALPHA" in text and "BETA" in text and text.count("cedar-71") >= 2,
            "direct_context_answer")
    tools = [{"name": "fetch", "description": "Fetch one synthetic record.", "strict": False,
              "input_schema": {"type": "object", "properties": {"label": {"type": "string"}},
                               "required": ["label"], "additionalProperties": False}}]
    messages = [user("Call fetch once with label birch. Wait for its result before answering.")]
    issued = gateway.generate(request(messages, tools))
    identity = call(issued, "fetch", {"label": "birch"})
    result = document("birch-82", "Record label is GAMMA.")
    messages += [{"role": "assistant", "content": issued["content"]},
                 {"role": "user", "content": [{"type": "tool_result", "tool_use_id": identity,
                    "content": [result]}, {"type": "text", "text":
                    "Return the document context's record label and the document value. Do not call tools."}]}]
    final = answer(gateway.generate(request(messages, tools)))
    require("GAMMA" in final and "birch-82" in final, "tool_result_context_answer")
    return first, second, result


def automatic_cache(gateway):
    reports = []
    for ttl, category in [("5m", "ephemeral_5m_input_tokens"), ("1h", "ephemeral_1h_input_tokens")]:
        nonce = secrets.token_hex(12)
        prefix = " ".join(f"synthetic{i:05}" for i in range(2000))
        body = request([user(f"Synthetic cache fixture {nonce}.\n{prefix}\n"
                             "Ignore the noisy labels and return exactly CACHE_OK as text.")])
        body["cache_control"] = {"type": "ephemeral", "ttl": ttl}
        session = f"cache-{ttl}-{nonce}"
        written = gateway.generate(body, session)
        read = gateway.generate(body, session)
        require(answer(written).strip() == answer(read).strip() == "CACHE_OK", "cache_text_answer")
        write = written.get("usage", {}).get("cache_creation_input_tokens")
        hit = read.get("usage", {}).get("cache_read_input_tokens")
        classified = written.get("usage", {}).get("cache_creation", {}).get(category)
        require(type(write) is int and write > 0, "cache_write_not_observed")
        require(type(hit) is int and hit > 0, "cache_read_not_observed")
        require(type(classified) is int and classified > 0, "cache_ttl_write_class_not_observed")
        reports.append({"ttl": ttl, "write_tokens": write, "read_tokens": hit,
                        "ttl_class_write_tokens": classified})
    return reports


def verify_cache_controls(wires):
    require(len(wires) == 4, "automatic_cache_request_count")
    for wire, ttl in zip(wires, ["5m", "5m", "1h", "1h"], strict=True):
        control = wire.get("cache_control")
        # Native omission is the documented five-minute default, not an erased
        # one-hour intent. Reject null, other values and unknown prepared keys.
        require(isinstance(control, dict) and set(control) <= {"type", "ttl"}
                and control.get("type") == "ephemeral" and control.get("ttl", "5m") == ttl,
                "automatic_cache_ttl_changed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--gateway-bin", type=Path, required=True)
    parser.add_argument("case", choices=list(BUDGETS))
    args = parser.parse_args()
    require(args.gateway_bin.is_absolute() and args.gateway_bin.is_file(), "explicit_gateway_binary")
    report = {"case": args.case, "model": MODEL, "binding": BINDING,
              "request_budget": BUDGETS[args.case], "max_tokens": CAP, "attempt_timeout_ms": DEADLINE_MS}
    try:
        with tempfile.TemporaryDirectory(prefix="morphiecore-messages-input-") as directory:
            gateway = Gateway(args.gateway_bin, Path(directory), BUDGETS[args.case])
            try:
                if args.case == "examples":
                    root, inline = examples(gateway)
                elif args.case == "context":
                    first, second, result = contexts(gateway)
                else:
                    cache = automatic_cache(gateway)
                    report["cache_reports"] = cache
            finally:
                gateway.close()
            wires = gateway.verify()
            if args.case == "examples":
                require(all(wire["tools"][0] == root for wire in wires), "root_examples_or_schema_changed")
                for wire in wires[1:]:
                    additions = [block["tool"]["definition"] for message in wire["messages"]
                                 for block in message["content"] if isinstance(block, dict)
                                 and block.get("type") == "tool_addition"]
                    require(additions == [inline], "inline_examples_or_schema_changed")
                report.update(root_and_inline_examples=True, model_issued_tool_history=True)
            elif args.case == "context":
                require(wires[0]["messages"][0]["content"][:2] == [first, second], "direct_context_changed")
                results = [block for message in wires[2]["messages"] for block in message["content"]
                           if isinstance(block, dict) and block.get("type") == "tool_result"]
                require(len(results) == 1 and results[0]["content"] == [result], "tool_context_changed")
                report.update(per_use_document_context=True, model_issued_tool_history=True)
            else:
                verify_cache_controls(wires)
                report.update(cache_reports=cache, retention_and_invoices="not_asserted")
            report.update(status="passed", client_requests=BUDGETS[args.case], upstream_attempts=BUDGETS[args.case])
    except Exception as error:
        report.update(status="failed", error=str(error) if isinstance(error, GateError) else type(error).__name__)
        print(json.dumps(report))
        return 1
    print(json.dumps(report))
    return 0


if __name__ == "__main__":
    sys.exit(main())
