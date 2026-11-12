"""Independent failure-oracle checks for the synthetic installed-client authority."""
from contextlib import contextmanager, redirect_stderr, redirect_stdout
import copy
import http.client
import io
import json
from pathlib import Path
import socket
import queue
import sys
import tempfile
import threading
import time
import unittest
from email.message import Message
from unittest.mock import MagicMock, patch

import recovery
import drivers
import protocol_defaults
import recovery_boundary


class ClientRecoveryOracles(unittest.TestCase):
    path = Path("/synthetic/sample.txt")

    def test_rpc_wait_honors_the_command_deadline_after_a_long_silence(self):
        child = object.__new__(drivers.Child)
        child.failure = None
        child.records = MagicMock()

        def delayed_record(*, timeout):
            if timeout < 16:
                raise queue.Empty
            return {"type": "agent_settled"}

        child.records.get.side_effect = delayed_record
        with patch.object(drivers.time, "monotonic", return_value=100):
            self.assertEqual(child.record(125), {"type": "agent_settled"})
            with self.assertRaisesRegex(RuntimeError, "^rpc_deadline$"):
                child.record(100)
            child.records.get.side_effect = queue.Empty
            with self.assertRaisesRegex(RuntimeError, "^rpc_deadline$"):
                child.record(125)
        child.failure = "child_pipe_or_record_failure"
        with self.assertRaisesRegex(RuntimeError, "^child_pipe_failure$"):
            child.record(125)

    def test_rpc_eof_and_invalid_records_wake_the_waiter(self):
        for source, error in [
            ("print('{\"type\":\"response\"}', flush=True)", "rpc_no_record"),
            ("print('invalid-json', flush=True)", "child_pipe_failure"),
        ]:
            with self.subTest(source=source), drivers.Child(
                [sys.executable, "-c", source], {}, Path.cwd(), rpc=True
            ) as child:
                if error == "rpc_no_record":
                    self.assertEqual(child.record(time.monotonic() + 3), {"type": "response"})
                with self.assertRaisesRegex(RuntimeError, "^" + error + "$"):
                    child.record(time.monotonic() + 3)

    def test_main_cannot_publish_success_before_authority_teardown_verdict(self):
        states = []
        @contextmanager
        def authority(state, ingress):
            states.append(state)
            try:
                yield "http://127.0.0.1:1"
            finally:
                if ingress:
                    state.errors.append("late_worker_failure")

        def run(*_):
            state = states[0]
            state.ingress = [(1, "original"), (2, "original"), (0, "probe"), (3, "original")]
            state.attempts = [(1, True, None, {}), (2, True, None, {}),
                              (2, True, None, {}), (0, True, None, {}), (3, True, None, {})]
            state.wires = [(b"synthetic", ordinal) for ordinal in range(5)]
            state.hints = [(1, 1)]
            state.finish(("original", "probe"))
            return "original", "probe"

        version, gateway = MagicMock(), MagicMock()
        for child in (version, gateway):
            child.__enter__.return_value = child
        version.wait.return_value = b"1.4.0"
        gateway.first_line = queue.Queue()
        gateway.first_line.put(b"MorphieCore listening on http://127.0.0.1:1\n")
        with tempfile.TemporaryDirectory() as directory:
            output = io.StringIO()
            with patch.object(recovery.sys, "argv", ["recovery.py", "pi-chat", directory, "/gateway", "/pi"]), \
                    patch.object(recovery, "Child", side_effect=[version, gateway]), \
                    patch.object(recovery, "server", side_effect=authority), \
                    patch.object(recovery, "run_pi", side_effect=run), redirect_stdout(output):
                with self.assertRaisesRegex(RuntimeError,
                        "^synthetic_http_check_failed:late_worker_failure$"):
                    recovery.main()
            self.assertEqual(output.getvalue(), "")


    def chat(self, final=False):
        history = [
            {"role": "user", "content": recovery.READ},
            {"role": "assistant", "content": None,
             "tool_calls": [{"id": recovery.CALL, "type": "function", "function": {
                 "name": "read", "arguments": json.dumps({"path": str(self.path)})}}]},
            {"role": "tool", "tool_call_id": recovery.CALL, "content": recovery.TEXT},
        ]
        if final:
            history.extend([{"role": "assistant", "content": recovery.TEXT},
                            {"role": "user", "content": recovery.NEXT}])
        return {"model": "qwen3.8-flash", "stream": True, "max_completion_tokens": 2048,
                "reasoning_effort": "low", "tools": [{"type": "function", "function": {
                    "name": "read", "parameters": {"properties": {"path": {"type": "string"}}}}}],
                "messages": history}

    def native(self):
        return {"model": "claude-haiku-5-5", "stream": True, "max_tokens": 2048,
                "thinking": {"type": "adaptive"}, "output_config": {"effort": "low"},
                "tools": [{"name": "read", "strict": True, "eager_input_streaming": True,
                           "input_schema": {"properties": {"path": {"type": "string"}}}}],
                "messages": [
                    {"role": "user", "content": [{"type": "text", "text": recovery.READ}]},
                    {"role": "assistant", "content": [
                        {"type": "thinking", "thinking": recovery.PLAN, "signature": recovery.SIGNATURE},
                        {"type": "redacted_thinking", "data": recovery.REDACTED},
                        {"type": "tool_use", "id": recovery.CALL, "name": "read", "input": {"path": str(self.path)}}]},
                    {"role": "user", "content": [{"type": "tool_result", "tool_use_id": recovery.CALL,
                                                   "content": recovery.TEXT}]}]}

    def native_defaults(self, pi):
        body = self.native()
        body["messages"] = body["messages"][:1]
        tool = body["tools"][0]
        tool.pop("strict")
        tool["input_schema"].update(type="object", required=["path"])
        if pi:
            body["thinking"]["display"] = "summarized"
            body["messages"].append({"role": "system", "content": [],
                                     "output_config": {"effort": "medium"}})
        else:
            body.pop("output_config")
            tool.pop("eager_input_streaming")
        return body

    def test_history_failures_reach_the_intended_oracle(self):
        for native, original, cases in [
            (False, self.chat(), [
                (("messages", 1, "reasoning_content"), "changed", "chat_standard_history"),
                (("messages", 2, "tool_call_id"), "changed", "chat_tool_result"),
                (("messages", 1, "tool_calls", 0, "function", "arguments"), '{}', "chat_tool_call_arguments"),
            ]),
            (True, self.native(), [
                (("messages", 1, "content", 0, "signature"), "changed", "native_exact_signed_history"),
                (("messages", 1, "content", 1, "data"), "changed", "native_exact_signed_history"),
                (("messages", 2, "content", 0, "tool_use_id"), "changed", "native_tool_result_identity"),
            ]),
        ]:
            self.assertEqual(recovery.stage(original), 2)
            recovery.assert_history(original, 2, native, self.path)
            for keys, value, error in cases:
                body = copy.deepcopy(original)
                owner = body
                for key in keys[:-1]:
                    owner = owner[key]
                owner[keys[-1]] = value
                with self.subTest(native=native, error=error), self.assertRaisesRegex(RuntimeError, f"^{error}$"):
                    recovery.assert_history(body, 2, native, self.path)
        final = self.chat(True)
        self.assertEqual(recovery.stage(final), 3)
        recovery.assert_history(final, 3, False, self.path)
        final["messages"][3]["content"] = "changed"
        with self.assertRaisesRegex(RuntimeError, "^chat_final_history$"):
            recovery.assert_history(final, 3, False, self.path)

    def test_default_messages_read_schema_and_strictness_cannot_disappear(self):
        for pi in (True, False):
            original = self.native_defaults(pi)
            cases = [
                (("tools",), [], "default_read_tool_declaration"),
                (("tools",), original["tools"] * 2, "default_read_tool_declaration"),
                (("tools", 0, "input_schema", "properties"), {}, "default_read_path_schema"),
                (("tools", 0, "input_schema", "properties", "path", "type"), "number",
                 "default_read_path_schema"),
                (("tools", 0, "input_schema", "required"), [], "default_read_path_schema"),
                (("tools", 0, "strict"), True, "default_read_strict"),
            ]
            protocol_defaults.assert_default_profile(original, pi)
            for keys, value, error in cases:
                body = copy.deepcopy(original)
                owner = body
                for key in keys[:-1]:
                    owner = owner[key]
                owner[keys[-1]] = value
                with self.subTest(pi=pi, error=error), self.assertRaisesRegex(RuntimeError, f"^{error}$"):
                    protocol_defaults.assert_default_profile(body, pi)

    def test_current_tool_inventory_preserves_auxiliary_tools_and_optional_parameters(self):
        for pi in (True, False):
            original = self.native_defaults(pi)
            original["tools"].append({"name": "auxiliary", "input_schema": {
                "type": "object", "properties": {"id": {"type": "string"}}, "required": ["id"]}})
            original["tools"][0]["input_schema"]["properties"]["offset"] = {"type": "integer"}
            headers = Message()
            headers["anthropic-version"] = "2023-06-01"
            state = protocol_defaults.MessagesState(self.path, pi=pi)
            state.incoming(headers, original)
            upstream = {"anthropic-version": "2026-10-21", "x-api-key": "synthetic-client-primary"}
            changed = copy.deepcopy(original)
            changed["tools"].pop()
            with self.assertRaisesRegex(RuntimeError, "^default_tool_inventory_changed$"):
                state.upstream(upstream, changed)
            changed = copy.deepcopy(original)
            del changed["tools"][0]["input_schema"]["properties"]["offset"]
            with self.assertRaisesRegex(RuntimeError, "^default_tool_inventory_changed$"):
                state.upstream(upstream, changed)
            state.upstream(upstream, original)

    def test_duplicate_client_transmission_and_missing_identity_reject(self):
        headers = Message()
        headers["anthropic-version"] = "2023-06-01"
        headers["anthropic-beta"] = "interleaved-thinking-2025-05-14"
        initial = self.native_defaults(True)
        state = protocol_defaults.MessagesState(self.path, pi=True)
        state.incoming(headers, initial)
        self.assertTrue(state.beta_seen)
        with self.assertRaisesRegex(RuntimeError, "^default_extra_or_reordered_client_request$"):
            state.incoming(headers, initial)
        headers.replace_header("anthropic-version", "2026-10-21")
        with self.assertRaisesRegex(RuntimeError, "^default_messages_version$"):
            protocol_defaults.MessagesState(self.path, pi=True).incoming(headers, initial)
        state.ingress = state.attempts = [1, 2, 3]
        state.errors.append("late_worker_failure")
        with self.assertRaisesRegex(RuntimeError, "^default_authority_errors:late_worker_failure$"):
            state.finish()
        state = recovery.State(False, self.path)
        initial = self.chat()
        initial["messages"] = initial["messages"][:1]
        with self.assertRaisesRegex(RuntimeError, "^missing_client_session_carrier$"):
            state.incoming({}, initial)
        headers = {"session_id": "synthetic-session"}
        state.incoming(headers, initial)
        state.incoming(headers, self.chat())
        with self.assertRaisesRegex(RuntimeError, "^client_request_order$"):
            state.incoming(headers, self.chat())

    def test_native_history_order_and_cardinality_are_not_inferred_from_contents(self):
        for extra in (False, True):
            body = self.native()
            if extra:
                body["messages"].append(copy.deepcopy(body["messages"][1]))
            else:
                body["messages"][1:3] = reversed(body["messages"][1:3])
            with self.subTest(extra=extra), self.assertRaisesRegex(RuntimeError, "^native_history_roles$"):
                recovery.assert_history(body, 2, True, self.path)

    def test_native_probe_needs_a_carrier_and_client_identity_must_match(self):
        state = recovery.State(True, self.path)
        state.ingress = [(1, "original"), (2, "original"), (0, "probe"), (3, "original")]
        state.attempts = [(1, True, "group", {}), (2, True, "group", {}),
                          (2, True, "group", {}), (0, True, "probe-group", {}), (3, True, "group", {})]
        state.wires = [(b"synthetic", 0)] * 5
        state.hints = [(0, 0)]
        state.finish(("original", "probe"))
        with self.assertRaisesRegex(RuntimeError, "^client_session_carrier_mismatch$"):
            state.finish(("unrelated", "probe"))
        state.attempts[3] = (0, True, None, {})
        with self.assertRaisesRegex(RuntimeError, "^native_recovery_session_not_independent$"):
            state.finish(("original", "probe"))

    def test_recovery_requires_identical_wire_waited_hint_and_unchanged_target(self):
        state = recovery.State(False, self.path)
        state.retry_after = "1"
        state.ingress = [(1, "original"), (2, "original"), (0, "probe"), (3, "original")]
        state.attempts = [(1, True, None, {}), (2, True, None, {}),
                          (2, True, None, {}), (0, True, None, {}), (3, True, None, {})]
        state.wires = [(b"initial", 0), (b"feedback", 1), (b"feedback", 2),
                       (b"probe", 3), (b"recall", 4)]
        state.hints = [(1, 1)]
        state.finish(("original", "probe"))
        for wires, attempts, error in [
            ([], state.attempts, "retry_changed_wire"),
            ([*state.wires[:2], (b"changed", 2), *state.wires[3:]],
             state.attempts, "retry_changed_wire"),
            ([*state.wires[:2], (b"feedback", 1.9), *state.wires[3:]],
             state.attempts, "retry_hint_not_waited"),
            (state.wires, [*state.attempts[:2], (2, False, None, {}), *state.attempts[3:]],
             "recovery_switched_target"),
        ]:
            changed = copy.copy(state)
            changed.wires, changed.attempts = wires, attempts
            with self.subTest(error=error), self.assertRaisesRegex(RuntimeError, f"^{error}$"):
                changed.finish(("original", "probe"))
        # Slow authority work cannot count as waiting after Retry-After is sent.
        state.hints = [(1.5, 1.5)]
        with self.assertRaisesRegex(RuntimeError, "^retry_hint_not_waited$"):
            state.finish(("original", "probe"))
        # Sampling after publication may be delayed even though the gateway
        # waited correctly. Preserve that uncertainty, including a sample
        # recorded only after the retry, rather than fabricating a short wait.
        for after in (1.15, 2.15):
            state.hints = [(1, after)]
            wait = state.finish(("original", "probe"))
            self.assertLess(wait["minimum"], 1)
            self.assertGreaterEqual(wait["maximum"], 1)
        state.wires[2] = (b"feedback", 1.9)
        with self.assertRaisesRegex(RuntimeError, "^retry_hint_not_waited$"):
            state.finish(("original", "probe"))

    def test_gateway_shutdown_preserves_an_active_failure_but_is_otherwise_fatal(self):
        child = MagicMock()
        child.wait.side_effect = RuntimeError("client_deadline")
        errors = io.StringIO()
        with redirect_stderr(errors), self.assertRaisesRegex(RuntimeError, "^primary_failure$"):
            try:
                raise RuntimeError("primary_failure")
            finally:
                drivers.shutdown_gateway(child, failed=True)
        self.assertEqual(json.loads(errors.getvalue()), {"gateway_cleanup_error": "client_deadline"})
        with self.assertRaisesRegex(RuntimeError, "^client_deadline$"):
            drivers.shutdown_gateway(child)

    def test_only_a_new_client_request_can_select_the_backup(self):
        initial = self.chat()
        initial["messages"] = [{"role": "user", "content": recovery.PROBE}]
        headers = {"session_id": "original"}
        for scenario in ("next-request", "retry-limit"):
            state = recovery_boundary.State(scenario)
            state.incoming(headers, initial)
            failures = 2 if scenario == "retry-limit" else 1
            for _ in range(failures):
                status, _, _ = state.upstream({"authorization": "Bearer synthetic-client-primary"}, initial)
                self.assertEqual(status, 503)
            followup = {**initial, "messages": [*initial["messages"],
                        {"role": "user", "content": recovery.NEXT}]}
            with self.assertRaisesRegex(RuntimeError, "^boundary_hidden_retry$"):
                state.upstream({"authorization": "Bearer synthetic-client-secondary"}, followup)
            state.attempts.pop()
            state.incoming(headers, followup)
            self.assertEqual(state.upstream(
                {"authorization": "Bearer synthetic-client-secondary"}, followup)[0], 200)

    def test_server_teardown_releases_an_incomplete_header_worker(self):
        state = recovery.State(False, self.path)
        with recovery.server(state, False) as origin:
            address = recovery.urlsplit(origin)
            connection = socket.create_connection((address.hostname, address.port), timeout=3)
            self.addCleanup(connection.close)
            connection.sendall(b"POST /v1/chat/completions HTTP/1.1\r\nHost: unfinished")
            self.assertTrue(state.accepted.wait(timeout=2))
        self.assertEqual(connection.recv(1), b"")

    def test_authority_advertises_close_and_brackets_header_publication(self):
        state = recovery.State(False, self.path)
        initial = self.chat()
        initial["messages"] = initial["messages"][:1]
        published, release = threading.Event(), threading.Event()
        samples = []
        end_headers = recovery.BaseHTTPRequestHandler.end_headers

        def delayed_sample(handler):
            end_headers(handler)
            if len(state.attempts) == 2:
                samples.append(time.monotonic())
                published.set()
                drivers.check(release.wait(timeout=3), "header_sample_release")

        with patch.object(recovery.BaseHTTPRequestHandler, "end_headers", delayed_sample), \
                recovery.server(state, False) as origin:
            address = recovery.urlsplit(origin)
            for body, status in [(initial, 200), (self.chat(), 503)]:
                connection = http.client.HTTPConnection(address.hostname, address.port, timeout=3)
                try:
                    connection.request("POST", "/v1/chat/completions", json.dumps(body),
                                       {"authorization": "Bearer synthetic-client-primary"})
                    response = connection.getresponse()
                    self.assertEqual(response.status, status)
                    self.assertTrue(response.will_close)
                    if status == 503:
                        self.assertEqual(response.getheader("retry-after"), "0")
                        self.assertTrue(published.wait(timeout=3))
                    response.read()
                    if status == 503:
                        retry_at = time.monotonic()
                finally:
                    if status == 503:
                        release.set()
                    connection.close()
        self.assertFalse(state.errors)
        before, after = state.hints[0]
        self.assertLessEqual(before, samples[0])
        self.assertLessEqual(samples[0], retry_at)
        self.assertLessEqual(retry_at, after)


if __name__ == "__main__":
    unittest.main()
