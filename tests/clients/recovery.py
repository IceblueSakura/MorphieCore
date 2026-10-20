"""Bounded installed-client routing acceptance against the CLI loader and synthetic HTTP."""
from contextlib import ExitStack, closing, contextmanager
import copy
import http.client
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import signal
import queue
import socket
import sys
import threading
import time
from urllib.parse import urlsplit

from drivers import Child, KEY, NEXT, PROBE, READ, TEXT, check, environment, private_json, run_opencode, run_pi

PLAN = "Synthetic read plan"
SIGNATURE = "synthetic-client-thinking-signature"
REDACTED = "synthetic-client-redacted-thinking"
CALL = "call-client-read"


def text(value):
    if isinstance(value, str):
        return value
    if isinstance(value, list):
        return "\n".join(text(child) for child in value)
    if isinstance(value, dict):
        return "\n".join(text(child) for child in value.values())
    return ""


def stage(body):
    users = [m for m in body["messages"] if m["role"] == "user"]
    last = text(users[-1]["content"])
    if PROBE in last:
        return 0
    if NEXT in last:
        return 3
    if any(m["role"] == "tool" or any(b.get("type") == "tool_result"
            for b in (m.get("content") or []) if isinstance(b, dict)) for m in body["messages"]):
        return 2
    check(READ in last, "unexpected_client_prompt")
    return 1


def assert_history(body, phase, native, path):
    check(body["model"] == ("claude-haiku-5-5" if native else "qwen3.8-flash"), "upstream_model")
    check(body["stream"] is True, "client_not_streaming")
    check(body["max_tokens" if native else "max_completion_tokens"] == 2048, "wire_output_cap")
    tools = body.get("tools", [])
    read = [t if native else t["function"] for t in tools
            if (t.get("name") if native else t.get("function", {}).get("name")) == "read"]
    check(len(read) == 1, "read_tool_declaration")
    schema = read[0]["input_schema" if native else "parameters"]
    check("path" in schema["properties"], "read_path_schema")
    if native:
        check(body["thinking"]["type"] == "adaptive", "native_adaptive_thinking")
        check(body["output_config"]["effort"] == "high", "native_root_effort")
        check(read[0]["strict"] is True and read[0]["eager_input_streaming"] is True, "native_tool_profile")
    else:
        check(body["reasoning_effort"] == "low", "chat_effort")
        check(read[0].get("strict") in (None, False), "chat_tool_strictness")
    if phase in (0, 1):
        check(not any(m["role"] == "assistant" for m in body["messages"]), "unexpected_initial_history")
        return
    if native:
        history = [m for m in body["messages"] if m["role"] != "system"]
        check([m["role"] for m in history] == (["user", "assistant", "user"] if phase == 2 else
              ["user", "assistant", "user", "assistant", "user"]), "native_history_roles")
        check(all(m["content"] == [] and m["output_config"]["effort"] == "low"
                  for m in body["messages"] if m["role"] == "system"), "native_ordered_effort")
        assistants = [m for m in body["messages"] if m["role"] == "assistant"]
        check(assistants[0]["content"] == [
            {"type": "thinking", "thinking": PLAN, "signature": SIGNATURE},
            {"type": "redacted_thinking", "data": REDACTED},
            {"type": "tool_use", "id": CALL, "name": "read", "input": {"path": str(path)}},
        ], "native_exact_signed_history")
        results = [b for m in body["messages"] if m["role"] == "user"
                   for b in m["content"] if b.get("type") == "tool_result"]
        check(len(results) == 1 and results[0]["tool_use_id"] == CALL,
              "native_tool_result_identity")
        check(results[0].get("is_error", False) is False and TEXT in text(results[0]["content"]),
              "native_tool_result_content")
        if phase == 3:
            check(assistants[1]["content"] == [{"type": "text", "text": TEXT}], "native_final_history")
    else:
        history = [m for m in body["messages"] if m["role"] != "system"]
        check([m["role"] for m in history] == (["user", "assistant", "tool"] if phase == 2 else
              ["user", "assistant", "tool", "assistant", "user"]), "chat_history_roles")
        check(history[1]["reasoning_content"] == PLAN, "chat_readable_reasoning")
        calls = history[1]["tool_calls"]
        check(len(calls) == 1 and calls[0]["id"] == CALL and calls[0]["function"]["name"] == "read",
              "chat_tool_call_identity")
        check(json.loads(calls[0]["function"]["arguments"]) == {"path": str(path)}, "chat_tool_call_arguments")
        check(history[2]["tool_call_id"] == CALL and TEXT in text(history[2]["content"]), "chat_tool_result")
        if phase == 3:
            check(history[3]["content"] == TEXT and history[3]["reasoning_content"] == "Synthetic final check",
                  "chat_final_history")


def chat_events(phase, path):
    def chunk(delta, finish=None):
        return {"id": f"chat-client-{phase}", "object": "chat.completion.chunk", "created": 1,
                "model": "qwen3.8-flash", "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]}
    values = [chunk({"role": "assistant"}), chunk({"reasoning_content": PLAN if phase == 1 else "Synthetic final check"})]
    if phase == 1:
        values.append(chunk({"tool_calls": [{"index": 0, "id": CALL, "type": "function",
            "function": {"name": "read", "arguments": json.dumps({"path": str(path)})}}]}))
    else:
        values.append(chunk({"content": TEXT}))
    values.append(chunk({}, "tool_calls" if phase == 1 else "stop"))
    values.append({"id": f"chat-client-{phase}", "object": "chat.completion.chunk", "created": 1,
                   "model": "qwen3.8-flash", "choices": [],
                   "usage": {"prompt_tokens": 11, "completion_tokens": 7, "total_tokens": 18}})
    return ("".join(f"data: {json.dumps(v)}\n\n" for v in values) + "data: [DONE]\n\n").encode()


def native_events(phase, path):
    blocks = ([{"type": "thinking", "thinking": PLAN, "signature": SIGNATURE},
               {"type": "redacted_thinking", "data": REDACTED},
               {"type": "tool_use", "id": CALL, "name": "read", "input": {"path": str(path)}}]
              if phase == 1 else [{"type": "text", "text": TEXT}])
    values = [{"type": "message_start", "message": {"id": f"msg-client-{phase}", "type": "message",
        "role": "assistant", "model": "claude-haiku-5-5", "content": [], "stop_reason": None,
        "stop_sequence": None, "usage": {"input_tokens": 11, "output_tokens": 0}}}]
    for index, block in enumerate(blocks):
        kind = block["type"]
        start = ({"type": "thinking", "thinking": "", "signature": ""} if kind == "thinking" else
                 {"type": "tool_use", "id": CALL, "name": "read", "input": {}} if kind == "tool_use" else
                 {"type": "text", "text": ""} if kind == "text" else block)
        values.append({"type": "content_block_start", "index": index, "content_block": start})
        deltas = ([{"type": "thinking_delta", "thinking": PLAN}, {"type": "signature_delta", "signature": SIGNATURE}]
                  if kind == "thinking" else [{"type": "input_json_delta", "partial_json": json.dumps(block["input"])}]
                  if kind == "tool_use" else [{"type": "text_delta", "text": TEXT}] if kind == "text" else [])
        values.extend({"type": "content_block_delta", "index": index, "delta": delta} for delta in deltas)
        values.append({"type": "content_block_stop", "index": index})
    values.extend([{"type": "message_delta", "delta": {"stop_reason": "tool_use" if phase == 1 else "end_turn"},
                    "usage": {"output_tokens": 7}}, {"type": "message_stop"}])
    return "".join(f"event: {v['type']}\ndata: {json.dumps(v)}\n\n" for v in values).encode()


class State:
    def __init__(self, native, path):
        self.native = native
        self.path = path
        self.lock = threading.Lock()
        self.ingress = []
        self.attempts = []
        self.errors = []
        self.gateway = None
        self.accepted = threading.Event()

    def incoming(self, headers, body):
        phase = stage(body)
        # Preserve the actual wire-selected conversation, including header precedence.
        identity = next((headers[name] for name in ["x-morphiecore-conversation-id", "x-session-id",
                        "session_id", "session-id", "x-session-affinity"] if headers.get(name)), None)
        check(identity is not None, "missing_client_session_carrier")
        with self.lock:
            self.ingress.append((phase, identity))
            check(len(self.ingress) <= 4, "extra_client_request")
            check([p for p, _ in self.ingress] == [1, 2, 0, 3][:len(self.ingress)], "client_request_order")

    def upstream(self, headers, body):
        phase = stage(body)
        assert_history(body, phase, self.native, self.path)
        secret = headers["x-api-key"] if self.native else headers["authorization"].removeprefix("Bearer ")
        check(secret in ("synthetic-client-primary", "synthetic-client-secondary"), "unexpected_credential")
        primary = secret == "synthetic-client-primary"
        for field in ["x-session-id", "session_id", "x-session-affinity", "x-morphiecore-conversation-id"]:
            check(field not in headers, "ingress_header_forwarded")
        session = headers.get("x-opencode-session")
        with self.lock:
            self.attempts.append((phase, primary, session, copy.deepcopy(body)))
            expected = [(1, True), (2, True), (2, False), (0, True), (3, False)]
            check([(p, k) for p, k, _, _ in self.attempts] == expected[:len(self.attempts)], "upstream_attempt_order")
            check(len(self.attempts) <= 5, "extra_upstream_attempt")
        if phase == 2 and primary:
            return 503, b"", "application/json"
        return 200, (native_events if self.native else chat_events)(phase, self.path), "text/event-stream"

    def finish(self, client_sessions):
        check(not self.errors, "synthetic_http_check_failed:" + ",".join(self.errors))
        check(len(self.ingress) == 4 and len(self.attempts) == 5, "exchange_counts")
        original = self.ingress[0][1]
        check(all(identity == original for phase, identity in self.ingress if phase != 0), "client_session_changed")
        check(self.ingress[2][1] != original, "recovery_session_not_independent")
        check((original, self.ingress[2][1]) == client_sessions, "client_session_carrier_mismatch")
        check(self.attempts[1][3] == self.attempts[2][3], "retry_changed_tool_history")
        if self.native:
            group = self.attempts[0][2]
            check(group is not None, "missing_native_session")
            check(all(session == group for phase, _, session, _ in self.attempts if phase != 0), "native_session_changed")
            check(bool(self.attempts[3][2]) and self.attempts[3][2] != group,
                  "native_recovery_session_not_independent")


def handler(state, ingress):
    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, *_):
            pass

        def do_POST(self):
            try:
                self.connection.settimeout(12)
                expected = "/v1/messages" if state.native else "/v1/chat/completions"
                route = urlsplit(self.path)
                check(route.path == expected, "unexpected_http_path")
                check(not route.query or (ingress and state.native and route.query == "beta=true"),
                      "unexpected_http_query")
                length = int(self.headers.get("content-length", "0"))
                check(0 < length <= 2 * 1024 * 1024, "request_body_limit")
                raw = self.rfile.read(length)
                check(len(raw) == length, "truncated_request")
                body = json.loads(raw)
                if ingress:
                    state.incoming(self.headers, body)
                    upstream = urlsplit(state.gateway)
                    check(upstream.hostname == "127.0.0.1", "nonlocal_gateway")
                    with closing(http.client.HTTPConnection(upstream.hostname, upstream.port, timeout=3)) as connection:
                        connection.connect()
                        outgoing = connection.sock
                        self.server.track(outgoing)
                        headers = {k: v for k, v in self.headers.items() if k.lower() not in ("host", "connection")}
                        try:
                            connection.request("POST", self.path, raw, headers)
                            response = connection.getresponse()
                            self.send_response(response.status)
                            for key, value in response.getheaders():
                                if key.lower() not in ("connection", "transfer-encoding", "content-length"):
                                    self.send_header(key, value)
                            self.send_header("connection", "close")
                            self.end_headers()
                            received = 0
                            while chunk := response.read1(4096):
                                received += len(chunk)
                                check(received <= 2 * 1024 * 1024, "response_body_limit")
                                self.wfile.write(chunk)
                                self.wfile.flush()
                        finally:
                            self.server.untrack(outgoing)
                else:
                    status, payload, content_type = state.upstream(self.headers, body)
                    self.send_response(status)
                    self.send_header("content-type", content_type)
                    self.send_header("content-length", str(len(payload)))
                    self.send_header("connection", "close")
                    if status == 503:
                        self.send_header("retry-after", "0")
                    self.end_headers()
                    self.wfile.write(payload)
                self.close_connection = True
            except Exception as error:
                with state.lock:
                    state.errors.append(str(error) if isinstance(error, RuntimeError) else type(error).__name__)
                self.close_connection = True
                try:
                    self.send_error(400, "synthetic_acceptance_failure")
                except (OSError, ValueError):
                    pass
    return Handler


class OwnedServer(ThreadingHTTPServer):
    """Bound sockets before parsing; explicitly release every request worker."""
    daemon_threads = True

    def __init__(self, state, ingress):
        self.guard = threading.Lock()
        self.sockets = set()
        self.workers = set()
        self.closing = False
        self.state = state
        super().__init__(("127.0.0.1", 0), handler(state, ingress))

    def track(self, connection):
        with self.guard:
            if self.closing:
                connection.close()
                raise RuntimeError("server_closing")
            self.sockets.add(connection)

    def untrack(self, connection):
        with self.guard:
            self.sockets.discard(connection)

    def process_request(self, request, address):
        request.settimeout(3)
        self.track(request)
        worker = threading.Thread(target=self.work, args=(request, address), daemon=True)
        with self.guard:
            check(len(self.workers) < 8, "http_worker_limit")
            self.workers.add(worker)
        worker.start()
        self.state.accepted.set()

    def work(self, request, address):
        try:
            super().process_request_thread(request, address)
        finally:
            with self.guard:
                self.sockets.discard(request)
                self.workers.discard(threading.current_thread())

    def server_close(self):
        super().server_close()
        with self.guard:
            self.closing = True
            sockets, workers = list(self.sockets), list(self.workers)
        for connection in sockets:
            try:
                connection.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            connection.close()
        deadline = time.monotonic() + 4
        for worker in workers:
            worker.join(timeout=max(0, deadline - time.monotonic()))
            check(not worker.is_alive(), "http_worker_not_released")


@contextmanager
def server(state, ingress):
    with OwnedServer(state, ingress) as http:
        thread = threading.Thread(target=http.serve_forever, daemon=True)
        thread.start()
        try:
            yield f"http://127.0.0.1:{http.server_port}"
        finally:
            http.shutdown()
            thread.join(timeout=3)
            check(not thread.is_alive(), "listener_not_released")


def main():
    case, directory, gateway_binary, client_binary = sys.argv[1:]
    root = Path(directory)
    native = case == "pi-messages"
    work = root / "work"
    work.mkdir(mode=0o700)
    path = work / "sample.txt"
    path.write_text(TEXT + "\n", encoding="utf-8")
    state = State(native, path)
    env = environment(root)
    with Child([client_binary, "--version"], env, work) as client:
        version = client.wait(timeout=5).decode().strip()
    check(version == ("opencode v2.0.6+e5ecb57" if case == "opencode-chat" else "1.3.0"),
          "unaccepted_client_version")
    with ExitStack() as stack:
        origin = stack.enter_context(server(state, False))
        binding = "opencode-go-haiku-messages" if native else "dashscope-flash-chat"
        model = "claude-haiku-5-5" if native else "qwen3.8-flash"
        private_json(root / "gateway.json", {"client_key": KEY, "bind": "127.0.0.1:0",
            "models_file": "models.json", "exchange_timeout_ms": 10000,
            "diagnostics": "diagnostics.jsonl"})
        private_json(root / "models.json", {"models": {model: {"max_attempts": 2,
            "total_timeout_ms": 8000, "candidates": [{"binding": binding,
                "base_url": origin + "/v1", "attempt_timeout_ms": 3000}]}}})
        gateway = stack.enter_context(Child([gateway_binary, "--config", str(root / "gateway.json"),
            "--credentials-dir", str(root / "credentials")], env, work))
        try:
            ready = gateway.first_line.get(timeout=5).decode()
        except queue.Empty:
            raise RuntimeError("gateway_readiness_deadline") from None
        check(len(ready) <= 127, "gateway_readiness_limit")
        check(ready.startswith("MorphieCore listening on http://127.0.0.1:"), "gateway_startup")
        state.gateway = ready.removeprefix("MorphieCore listening on ")
        ingress = stack.enter_context(server(state, True))
        try:
            if case.startswith("pi-"):
                sessions = run_pi(client_binary, env, root, work, ingress, native, path)
            else:
                sessions = run_opencode(client_binary, env, root, work, ingress, path)
            state.finish(sessions)
            gateway.process.send_signal(signal.SIGINT)
            gateway.wait(timeout=5)
            records = [json.loads(line) for line in (root / "diagnostics.jsonl").read_text().splitlines()]
            check(len(records) == 4 and all(r["outcome"] == "complete" for r in records),
                  "gateway_completion_records")
            check(all(r.get("ignored_request_field_count", 0) == 0 for r in records),
                  "unclassified_ignored_request_fields")
            check([len(r["candidates"]) for r in records] == [1, 2, 1, 1], "gateway_attempt_records")
            check(records[2]["candidates"][0].get("recovery_probe") is True, "missing_foreground_recovery")
        except Exception:
            # Fixed structural diagnoses only; no bodies, credentials or raw client stderr.
            if gateway.process.poll() is None:
                gateway.process.send_signal(signal.SIGINT)
                gateway.wait(timeout=5)
            diagnostic_path = root / "diagnostics.jsonl"
            if diagnostic_path.exists():
                records = [json.loads(line) for line in diagnostic_path.read_text().splitlines()]
                fields = ["stage", "outcome", "stop_reason", "selection_skips", "candidates",
                          "executor_order", "ignored_request_field_count", "upstream_status"]
                print(json.dumps({"diagnostics": [{k: r[k] for k in fields if k in r} for r in records]}),
                      file=sys.stderr)
            print(json.dumps({"errors": state.errors, "ingress_phases": [p for p, _ in state.ingress],
                              "attempts": [[p, k] for p, k, _, _ in state.attempts]}), file=sys.stderr)
            raise
    print(json.dumps({"case": case, "version": version, "client_requests": 4, "upstream_attempts": 5,
        "extra_client_requests": 0, "sticky_after_recovery": True}))


if __name__ == "__main__":
    main()
