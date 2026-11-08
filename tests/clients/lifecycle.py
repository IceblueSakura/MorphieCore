"""Installed consumers observe held content, cancel without replay, and recover."""
import base64
from contextlib import ExitStack, closing, contextmanager
import http.client
from http.server import BaseHTTPRequestHandler
import json
from pathlib import Path
import queue
import re
import select
import signal
import socket
import sys
import threading
import time
from urllib.parse import urlsplit

from drivers import (TEXT, Child, Pi, check, check_client_version, environment,
                     gateway_configuration, opencode_configuration, opencode_prompt, pi_configuration,
                     private_json)
from recovery import OwnedServer
from protocol_defaults import PROFILES, responses_events
from recovery_boundary import wait_for_diagnostic

FIRST = "synthetic-client-first-content"
CANCEL = "synthetic-hold-and-cancel"
RECOVER = "synthetic-after-cancel"


def frames(native, text, final):
    fragments = [text] if final else ["prefix ", text[:len(text) // 2], text[len(text) // 2:]]
    if native:
        values = [
            {"type": "message_start", "message": {"id": "synthetic-message", "type": "message",
                "role": "assistant", "model": "claude-haiku-5-5", "content": [],
                "stop_reason": None, "stop_sequence": None,
                "usage": {"input_tokens": 1, "output_tokens": 0}}},
            {"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}},
        ]
        values.extend({"type": "content_block_delta", "index": 0,
                       "delta": {"type": "text_delta", "text": fragment}} for fragment in fragments)
        if final:
            values += [{"type": "content_block_stop", "index": 0},
                       {"type": "message_delta", "delta": {"stop_reason": "end_turn", "stop_sequence": None},
                        "usage": {"output_tokens": 1}}, {"type": "message_stop"}]
        return "".join(f"event: {value['type']}\ndata: {json.dumps(value)}\n\n" for value in values).encode()
    def chunk(delta, finish=None):
        return {"id": "synthetic-chat", "object": "chat.completion.chunk", "created": 1,
                "model": "qwen3.8-flash", "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]}
    values = [chunk({"role": "assistant", "content": fragments[0]})]
    values.extend(chunk({"content": fragment}) for fragment in fragments[1:])
    if final:
        values.append({**chunk({}, "stop"), "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}})
    raw = "".join(f"data: {json.dumps(value)}\n\n" for value in values).encode()
    return raw + (b"data: [DONE]\n\n" if final else b"")


class State:
    count = 2

    def __init__(self, native):
        self.native = native
        self.accepted = threading.Event()
        self.sent = threading.Event()
        self.dropped = threading.Event()
        self.guard = threading.Lock()
        self.attempts = 0
        self.errors = []

    def admit(self, headers, body):
        check(body.get("stream") is True, "upstream_stream_required")
        auth = headers.get("authorization", "").removeprefix("Bearer ") or headers.get("x-api-key", "")
        check(auth in ("synthetic-client-primary", "synthetic-client-secondary"), "upstream_auth")
        with self.guard:
            self.attempts += 1
            ordinal = self.attempts
        check(ordinal <= self.count, "upstream_attempt_budget")
        check((CANCEL if ordinal == 1 else RECOVER) in json.dumps(body), "upstream_request_order")
        return ordinal

    @property
    def endpoint_path(self):
        return "/v1/messages" if self.native else "/v1/chat/completions"

    def frames(self, ordinal):
        return frames(self.native, FIRST if ordinal == 1 else TEXT, ordinal == 2)


class ToolState(State):
    count = 3
    endpoint_path = "/v1/responses"
    cancelled_call = "call_cancelled"
    recovered_call = "call_recovered"

    def __init__(self, path):
        super().__init__(False)
        self.path = path
        self.content = TEXT + "\n"
        self.unfinished_arguments = f'{{"path": {json.dumps(str(path))}, "content": "'

    def admit(self, headers, body):
        ordinal = super().admit(headers, body)
        if ordinal >= 2:
            check(not any(item.get("call_id") == self.cancelled_call
                          or item.get("id") == "fc_cancelled" for item in body["input"]),
                  "cancelled_call_replayed")
        if ordinal == 3:
            calls = [item for item in body["input"] if item.get("type") == "function_call"]
            results = [item for item in body["input"] if item.get("type") == "function_call_output"]
            check(len(calls) == len(results) == 1, "recovered_call_result_count")
            check(calls[0].get("call_id") == results[0].get("call_id") == self.recovered_call,
                  "recovered_call_result_identity")
            check(calls[0].get("name") == "write" and json.loads(calls[0]["arguments"]) == {
                "path": str(self.path), "content": self.content} and results[0].get("output"),
                "recovered_call_result")
            check(self.path.read_bytes() == self.content.encode("utf-8"), "recovered_write_bytes")
        return ordinal

    def frames(self, ordinal):
        if ordinal == 1:
            response = {"id": "resp_cancelled", "object": "response", "created_at": 1,
                        "model": PROFILES["responses"][0], "status": "in_progress", "output": []}
            events = [
                {"type": "response.created", "response": response},
                {"type": "response.output_item.added", "output_index": 0, "item": {
                    "id": "fc_cancelled", "type": "function_call", "status": "in_progress",
                    "call_id": self.cancelled_call, "name": "write", "arguments": ""}},
                {"type": "response.function_call_arguments.delta", "output_index": 0,
                 "item_id": "fc_cancelled", "delta": self.unfinished_arguments},
            ]
            return "".join(f"event: {event['type']}\ndata: "
                           f"{json.dumps({**event, 'sequence_number': index})}\n\n"
                           for index, event in enumerate(events)).encode()
        items = ([{"id": "fc_recovered", "type": "function_call", "status": "completed",
                   "call_id": self.recovered_call, "name": "write",
                   "arguments": json.dumps({"path": str(self.path), "content": self.content})}] if ordinal == 2 else
                 [{"id": "msg_recovered", "type": "message", "role": "assistant",
                   "status": "completed", "content": [{"type": "output_text", "text": TEXT,
                                                       "annotations": [], "logprobs": []}]}])
        return responses_events(ordinal, self.path, items=items)


def observed_held_content(state):
    # The consumer may run between the authority's flush and its acknowledgement.
    check(state.sent.wait(timeout=3) and not state.dropped.is_set(),
          "content_not_observed_while_held")


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *_):
        pass

    def send_error(self, code, message=None, explain=None):
        if code == 501:
            self.server.state.errors.append("unsupported_http_method")
        super().send_error(code, message, explain)

    def do_POST(self):
        try:
            state = self.server.state
            check(self.path == state.endpoint_path, "upstream_path")
            size = int(self.headers.get("content-length", "-1"))
            check(0 < size <= 256 << 10, "upstream_request_bytes")
            body = json.loads(self.rfile.read(size))
            ordinal = state.admit(self.headers, body)
            self.send_response(200)
            self.send_header("content-type", "text/event-stream")
            self.send_header("transfer-encoding", "chunked")
            self.end_headers()
            data = state.frames(ordinal)
            self.wfile.write(f"{len(data):x}\r\n".encode() + data + b"\r\n")
            self.wfile.flush()
            if ordinal == 1:
                # No terminal or EOF is available to the consumer. Observe peer close
                # passively, before server teardown can manufacture cancellation.
                state.sent.set()
                check(bool(select.select([self.connection], [], [], 10)[0]), "upstream_cancel_deadline")
                try:
                    closed = self.connection.recv(1, socket.MSG_PEEK) == b""
                except ConnectionResetError:
                    closed = True
                check(closed, "upstream_not_cancelled")
                state.dropped.set()
            else:
                self.wfile.write(b"0\r\n\r\n")
                self.wfile.flush()
            self.close_connection = True
        except Exception:
            self.server.state.errors.append("upstream_handler_failure")
            self.close_connection = True


class Server(OwnedServer):
    def __init__(self, state):
        super().__init__(state, False)
        self.RequestHandlerClass = Handler

    def handle_error(self, *_):
        self.state.errors.append("upstream_worker_failure")


@contextmanager
def authority(state):
    with Server(state) as server:
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            yield f"http://127.0.0.1:{server.server_port}"
        finally:
            server.shutdown()
            thread.join(timeout=3)
            check(not thread.is_alive(), "upstream_listener_not_released")


def pi_case(binary, env, root, work, origin, state):
    tools = isinstance(state, ToolState)
    if tools:
        model, provider, _, _ = PROFILES["responses"]
        directory = root / "pi-agent"
        directory.mkdir(mode=0o700)
        private_json(directory / "models.json", {"providers": {provider: {
            "baseUrl": origin + "/v1", "apiKey": "$INGRESS_KEY",
            "modelOverrides": {model: {"maxTokens": 2048}}}}})
        private_json(directory / "settings.json", {"retry": {"enabled": False,
            "provider": {"maxRetries": 0, "timeoutMs": 10000}},
            "compaction": {"enabled": False}, "cacheWarming": "off"})
    else:
        directory, model = pi_configuration(root, origin, state.native)
    env = {**env, "PI_CODING_AGENT_DIR": str(directory), "PI_CACHE_RETENTION": "short"}
    with Pi(binary, env, work, model,
            **({"provider": provider, "thinking": None, "tools": "write"} if tools else {})) as pi:
        initial, _ = pi.command("get_state")
        prompt_id = pi.submit("prompt", message=f"{CANCEL}: " +
                             (f"Write {state.path}." if tools else f"Say {FIRST} without tools."))
        deadline = time.monotonic() + 5
        acknowledged = set()
        visible = ""
        while True:
            event = pi.record(deadline)
            if event.get("type") == "response" and event.get("id") == prompt_id:
                check(event.get("success") is True, "prompt_rejected")
                acknowledged.add(prompt_id)
            assistant_end = event.get("type") == "message_end" and event.get("message", {}).get("role") == "assistant"
            check(not assistant_end and event.get("type") not in ("agent_settled", "tool_execution_start"),
                  "client_finished_before_visible_content")
            delta = event.get("assistantMessageEvent", {})
            if tools and event.get("type") == "message_update" and delta.get("type") == "toolcall_delta":
                visible += delta.get("delta", "")
                check(len(visible) <= 1024, "visible_arguments_bound")
                if visible == state.unfinished_arguments:
                    break
            if event.get("type") == "message_update" and delta.get("type") == "text_delta":
                check(not tools, "cancelled_call_became_text")
                visible += delta.get("delta", "")
                check(len(visible) <= 1024, "visible_text_bound")
                if FIRST in visible:
                    check(FIRST in delta.get("delta", ""), "default_coalescing_not_observed")
                    break
        observed_held_content(state)
        abort_id = pi.submit("abort")
        deadline = time.monotonic() + 5
        settled = False
        aborted = []
        while len(acknowledged) < 2 or not settled:
            event = pi.record(deadline)
            if event.get("type") == "response" and event.get("id") in (prompt_id, abort_id):
                check(event.get("success") is True, "cancel_command_rejected")
                acknowledged.add(event["id"])
            if event.get("type") == "message_end" and event.get("message", {}).get("role") == "assistant":
                aborted.append(event["message"].get("stopReason"))
            if event.get("type") == "agent_settled":
                check(event.get("aborted") is True, "cancelled_turn_claimed_success")
                settled = True
            check(not event.get("type", "").startswith(("auto_retry", "compaction", "summarization_retry")),
                  "unexpected_auxiliary_request")
            if tools:
                check(not event.get("type", "").startswith("tool_execution"), "cancelled_call_executed")
        check(aborted == ["aborted"], "cancelled_assistant_terminal")
        check(state.dropped.wait(timeout=3), "upstream_not_released_before_recovery")
        if tools:
            check(not state.path.exists(), "cancelled_write_side_effect")
            receipt = wait_for_diagnostic(root, lambda row: row.get("event") == "request",
                                          3, "cancelled_gateway_receipt_missing")
            check(receipt["outcome"] != "complete" and len(receipt["candidates"]) == 1,
                  "cancelled_gateway_receipt")
            _, events = pi.command("prompt", message=(
                f"{RECOVER}: Write {state.content!r} to {state.path}, then answer only {TEXT}."))
            assert_recovered_tool(events, state.recovered_call)
            check(state.path.read_bytes() == state.content.encode("utf-8"), "recovered_write_bytes")
        else:
            pi.prompt(f"{RECOVER}: Say {TEXT} without tools.", ["stop"])
        final, _ = pi.command("get_state")
        check(final["sessionId"] == initial["sessionId"] and not final["isStreaming"], "recovery_session_or_idle")
    return "rpc_toolcall_delta" if tools else "rpc_text_delta"


def assert_recovered_tool(events, call):
    check(not any(e.get("type", "").startswith(("auto_retry", "compaction", "summarization_retry"))
                  for e in events), "unexpected_recovery_request")
    executions = [e for e in events if e.get("type") in ("tool_execution_start", "tool_execution_end")]
    check([e["type"] for e in executions] == ["tool_execution_start", "tool_execution_end"],
          "recovered_execution_count")
    # pi Responses IDs pair the native call ID with its item ID for replay.
    check(all(e.get("toolCallId") == call + "|fc_recovered" and e.get("toolName") == "write"
              for e in executions), "cancelled_or_unrelated_call_executed")
    check(executions[1].get("isError") is False, "recovered_write_result")
    assistants = [e["message"] for e in events if e.get("type") == "message_end"
                  and e.get("message", {}).get("role") == "assistant"]
    check([m.get("stopReason") for m in assistants] == ["toolUse", "stop"],
          "recovered_assistant_terminals")
    check(any(p.get("type") == "text" and p.get("text") == TEXT
              for p in assistants[-1]["content"]), "recovered_final_text")


class Events:
    """Read the installed OpenCode core's SSE, not its block-completion stdout."""
    def __init__(self, origin):
        address = urlsplit(origin)
        check(address.hostname == "127.0.0.1" and address.port is not None, "opencode_event_origin")
        self.address = address
        self.headers = {"authorization": "Basic " + base64.b64encode(
            b"opencode:synthetic-lifecycle-password").decode()}
        with closing(http.client.HTTPConnection(address.hostname, address.port, timeout=5)) as connection:
            connection.request("GET", "/openapi.json", headers=self.headers)
            response = connection.getresponse()
            raw = response.read((2 << 20) + 1)
            check(response.status == 200 and len(raw) <= 2 << 20, "opencode_public_api_bound")
            document = json.loads(raw)
            check(document["paths"]["/api/event"]["get"]["operationId"] == "event.subscribe",
                  "opencode_event_surface_unassessed")
        self.connection = http.client.HTTPConnection(address.hostname, address.port, timeout=10)
        try:
            self.connection.request("GET", "/api/event", headers=self.headers)
            self.response = self.connection.getresponse()
            check(self.response.status == 200 and
                  self.response.getheader("content-type", "").startswith("text/event-stream"), "opencode_event_stream")
            self.socket = self.connection.sock
            check(self.socket is not None, "opencode_event_socket_ownership")
        except Exception:
            self.connection.close()
            raise
        self.records = queue.Queue(maxsize=1024)
        self.failure = None
        self.closing = False
        self.thread = threading.Thread(target=self.read, daemon=True)
        self.thread.start()

    def read(self):
        try:
            size = 0
            data = []
            while True:
                line = self.response.readline((256 << 10) + 1)
                check(bool(line), "opencode_event_eof")
                size += len(line)
                check(len(line) <= 256 << 10 and size <= 4 << 20, "opencode_event_byte_limit")
                line = line.rstrip(b"\r\n")
                if not line and data:
                    self.records.put_nowait(json.loads(b"\n".join(data)))
                    data = []
                elif line.startswith(b"data:"):
                    data.append(line[5:].lstrip(b" "))
        except Exception:
            if not self.closing:
                self.failure = "opencode_event_reader_failure"

    def record(self, deadline):
        check(self.failure is None, "opencode_event_reader_failure")
        remaining = deadline - time.monotonic()
        check(remaining > 0, "opencode_event_deadline")
        try:
            return self.records.get(timeout=remaining)
        except queue.Empty:
            raise RuntimeError("opencode_no_event") from None

    def __enter__(self):
        try:
            check(self.record(time.monotonic() + 5).get("type") == "server.connected", "opencode_subscription_not_ready")
        except Exception:
            self.__exit__(None, None, None)
            raise
        return self

    def __exit__(self, *_):
        self.closing = True
        if self.socket is not None:
            try:
                self.socket.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
        self.thread.join(timeout=2)
        check(not self.thread.is_alive(), "opencode_event_reader_not_released")
        self.response.close()
        self.connection.close()


def opencode_case(binary, env, root, work, origin, state):
    config = opencode_configuration(root, origin)
    env = {**env, "OPENCODE_CONFIG": str(config), "OPENCODE_DISABLE_AUTOUPDATE": "1",
           "OPENCODE_DISABLE_MODELS_FETCH": "1", "OPENCODE_DISABLE_PROJECT_CONFIG": "1",
           "OPENCODE_SERVER_PASSWORD": "synthetic-lifecycle-password"}
    with Child([binary, "serve", "--hostname", "127.0.0.1", "--port", "0"], env, work) as server:
        try:
            ready = server.first_line.get(timeout=10).decode()
        except queue.Empty:
            raise RuntimeError("opencode_readiness_deadline") from None
        check(len(ready) <= 127 and ready.startswith("server listening on http://127.0.0.1:"), "opencode_server_startup")
        server_origin = ready.removeprefix("server listening on ")
        with Events(server_origin) as events:
            args = [binary, "run", "--server", server_origin, "--format", "json", "--title", "lifecycle",
                    "-m", "recovery/qwen3.8-flash#low", f"{CANCEL}: Say {FIRST} without tools."]
            with Child(args, env, work) as client:
                deadline = time.monotonic() + 5
                visible = ""
                while True:
                    event = events.record(deadline)
                    check(event.get("type") not in ("session.execution.succeeded", "session.execution.failed",
                                                   "session.execution.interrupted"), "opencode_finished_before_visible_content")
                    if event.get("type") == "session.text.delta":
                        visible += event.get("data", {}).get("delta", "")
                        check(len(visible) <= 1024, "visible_text_bound")
                        if FIRST in visible:
                            check(FIRST in event["data"]["delta"], "default_coalescing_not_observed")
                            identity = event["data"]["sessionID"]
                            check(isinstance(identity, str) and identity, "opencode_visible_session")
                            break
                observed_held_content(state)
                client.process.send_signal(signal.SIGINT)
                output = client.wait(timeout=5, expected_status=130)
                seen = [json.loads(line) for line in output.splitlines() if line]
                check({e["sessionID"] for e in seen if "sessionID" in e} == {identity}, "opencode_cancelled_session")
            deadline = time.monotonic() + 5
            while True:
                event = events.record(deadline)
                if event.get("data", {}).get("sessionID") != identity:
                    continue
                check(event.get("type") not in ("session.execution.succeeded", "session.execution.failed"),
                      "opencode_cancelled_execution_outcome")
                if event.get("type") == "session.execution.interrupted":
                    check(event["data"]["reason"] == "user", "opencode_interrupt_reason")
                    break
            check(state.dropped.wait(timeout=3), "upstream_not_released_before_recovery")
            resumed, _ = opencode_prompt(binary, env, work, f"{RECOVER}: Say {TEXT} without tools.",
                                        "lifecycle", identity, server_origin)
            check(resumed == identity, "opencode_recovery_session")
        server.process.send_signal(signal.SIGINT)
        server.wait(timeout=5, expected_status=130)
    return "core_sse_text_delta"


@contextmanager
def nginx_proxy(binary, root, env, origin, report):
    """Optional real proxy; foreground, loopback-only and test-owned."""
    directory = root / "nginx"
    directory.mkdir(mode=0o700)
    with Child([binary, "-v"], env, directory) as version:
        version.wait(timeout=5)
        match = re.fullmatch(r"nginx version: nginx/([0-9.]+)\s*", version.errors.decode())
        check(match is not None, "nginx_version")
        report["version"] = match.group(1)
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    # A port race fails startup; never fall back to another target or host service.
    (directory / "nginx.conf").write_text(f"""
daemon off;
master_process off;
pid nginx.pid;
error_log stderr warn;
events {{ worker_connections 64; }}
http {{
    log_format delivery '$sent_http_content_encoding';
    access_log access.log delivery;
    client_body_temp_path client_body;
    proxy_temp_path proxy_body;
    server {{
        listen 127.0.0.1:{port};
        underscores_in_headers on;
        client_max_body_size 32m;
        client_body_timeout 10s;
        send_timeout 30s;
        gzip on;
        gzip_comp_level 1;
        gzip_min_length 0;
        gzip_types application/json text/event-stream;
        gzip_vary on;
        location = /_ready {{
            access_log off;
            return 204;
        }}
        location /v1/ {{
            proxy_pass {origin};
            proxy_http_version 1.1;
            proxy_set_header Connection "";
            proxy_set_header Accept-Encoding "";
            proxy_request_buffering on;
            proxy_buffering off;
            proxy_cache off;
            proxy_next_upstream off;
            proxy_intercept_errors off;
            proxy_ignore_client_abort off;
            proxy_connect_timeout 3s;
            proxy_read_timeout 30s;
        }}
    }}
}}
""")
    with Child([binary, "-p", str(directory) + "/", "-c", "nginx.conf"], env, directory) as proxy:
        deadline = time.monotonic() + 5
        while True:
            check(proxy.process.poll() is None, "nginx_startup")
            probe = http.client.HTTPConnection("127.0.0.1", port, timeout=0.1)
            try:
                probe.request("GET", "/_ready")
                check(probe.getresponse().status == 204, "nginx_readiness_status")
                break
            except OSError:
                check(time.monotonic() < deadline, "nginx_readiness_deadline")
                time.sleep(0.02)
            finally:
                probe.close()
        try:
            yield f"http://127.0.0.1:{port}"
        finally:
            proxy.process.send_signal(signal.SIGQUIT)
            proxy.wait(timeout=5)
    rows = (directory / "access.log").read_text().splitlines()
    # The test-only readiness endpoint has its access log disabled.
    check(rows == ["gzip", "gzip"], "nginx_gzip_delivery")
    report["gzip_responses"] = len(rows)


def main():
    case, directory, gateway_binary, client_binary, *proxy_binary = sys.argv[1:]
    check(len(proxy_binary) <= 1, "proxy_arguments")
    root = Path(directory)
    work = root / "work"
    work.mkdir(mode=0o700)
    env = environment(root)
    with Child([client_binary, "--version"], env, work) as child:
        version = child.wait(timeout=5).decode().strip()
    check_client_version(case, version)
    tools = case == "pi-responses-tool-cancel"
    if tools:
        path = work / "sample.txt"
    state = ToolState(path) if tools else State(case == "pi-messages")
    with ExitStack() as stack:
        upstream = stack.enter_context(authority(state))
        gateway_configuration(root, state.native, upstream, total_timeout_ms=25000,
                              exchange_timeout_ms=30000, concurrency=1,
                              accounts=("primary", "secondary"),
                              recovery={"recovery_timeout_ms": 2500},
                              **({"model": PROFILES["responses"][0], "provider": "openai",
                                  "binding": PROFILES["responses"][2]} if tools else {}))
        gateway = stack.enter_context(Child([gateway_binary, "--config", str(root / "gateway.json"),
            "--credentials-dir", str(root / "credentials")], env, work))
        try:
            ready = gateway.first_line.get(timeout=5).decode()
        except queue.Empty:
            raise RuntimeError("gateway_readiness_deadline") from None
        check(len(ready) <= 127 and ready.startswith("MorphieCore listening on http://127.0.0.1:"), "gateway_startup")
        origin = ready.removeprefix("MorphieCore listening on ")
        proxy_report = None
        if proxy_binary:
            check(not tools, "tool_proxy_profile_not_selected")
            proxy_report = {}
            origin = stack.enter_context(nginx_proxy(proxy_binary[0], root, env, origin, proxy_report))
        try:
            run = pi_case if case.startswith("pi-") else opencode_case
            surface = run(client_binary, env, root, work, origin, state)
            check(state.attempts == state.count and not state.errors and state.dropped.is_set(),
                  "upstream_lifecycle_accounting")
        except Exception:
            print(json.dumps({"upstream_attempts": state.attempts, "content_sent": state.sent.is_set(),
                              "peer_closed": state.dropped.is_set(), "authority_errors": state.errors}), file=sys.stderr)
            raise
        finally:
            gateway.process.send_signal(signal.SIGINT)
            gateway.wait(timeout=5)
        records = [json.loads(line) for line in (root / "diagnostics.jsonl").read_text().splitlines()]
        check(all(r.get("event") in ("file", "request", "log", "span") for r in records), "lifecycle_diagnostic_loss")
        requests = [r for r in records if r.get("event") == "request"]
        check(len(requests) == state.count and requests[0]["outcome"] != "complete"
              and all(r["outcome"] == "complete" for r in requests[1:]),
              "cancelled_or_recovered_gateway_outcome")
        check(all(len(r["candidates"]) == 1 and r["candidates"][0]["advanced"] is False
                  and r.get("ignored_request_field_count", 0) == 0 for r in requests), "post_publication_replay")
    check(state.attempts == state.count and not state.errors, "late_upstream_lifecycle_failure")
    print(json.dumps({"case": case, "version": version,
        "client_requests": state.count, "upstream_attempts": state.count,
        "consumer_surface": surface, "content_before_terminal_and_eof": True,
        **({"cancelled_call_executions": 0, "new_call_executions": 1,
            "recovered_write_verified": True, "recovery_feedback_completed": True} if tools else
           {"coalesced_before_cancel": True}),
        "upstream_cancelled": True, "single_permit_recovery": True, "post_publication_replays": 0,
        "proxy": proxy_report}))


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(json.dumps({"error": str(error) if isinstance(error, RuntimeError) else type(error).__name__}), file=sys.stderr)
        sys.exit(1)
