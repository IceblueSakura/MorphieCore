"""Independent failure-oracle checks for the synthetic installed-client authority."""
from contextlib import closing
import copy
import http.client
import json
from pathlib import Path
import socket
import sqlite3
import unittest

import recovery
import drivers


class ClientRecoveryOracles(unittest.TestCase):
    path = Path("/synthetic/sample.txt")

    def test_installed_clients_are_gated_by_major_not_minor_or_patch(self):
        for case, versions in [
            ("pi-chat", ["1.3.0", "1.99.12", "1.4.0-dev.1"]),
            ("pi-messages", ["1.3.0+local", "1.12.3"]),
            ("opencode-chat", ["opencode v2.0.6+e5ecb57", "opencode v2.0.32",
                               "opencode v2.99.12-dev.1+local", "2.1.0"]),
        ]:
            for version in versions:
                with self.subTest(case=case, version=version):
                    drivers.check_client_version(case, version)
        for case, versions in [
            ("pi-chat", ["0.99.0", "2.0.0", "11.3.0", "1", "1.x", "1.3.0\nextra"]),
            ("opencode-chat", ["opencode v1.0.0", "opencode v3.0.0", "opencode v20.0.0",
                               "opencode v2", "unknown", ""]),
        ]:
            for version in versions:
                with self.subTest(case=case, version=version), self.assertRaisesRegex(
                    RuntimeError, "^unaccepted_client_major_version$"
                ):
                    drivers.check_client_version(case, version)

    def chat(self, final=False):
        history = [
            {"role": "user", "content": recovery.READ},
            {"role": "assistant", "content": None, "reasoning_content": recovery.PLAN,
             "tool_calls": [{"id": recovery.CALL, "type": "function", "function": {
                 "name": "read", "arguments": json.dumps({"path": str(self.path)})}}]},
            {"role": "tool", "tool_call_id": recovery.CALL, "content": recovery.TEXT},
        ]
        if final:
            history.extend([{"role": "assistant", "content": recovery.TEXT,
                             "reasoning_content": "Synthetic final check"},
                            {"role": "user", "content": recovery.NEXT}])
        return {"model": "qwen3.8-flash", "stream": True, "max_completion_tokens": 2048,
                "reasoning_effort": "low", "tools": [{"type": "function", "function": {
                    "name": "read", "parameters": {"properties": {"path": {"type": "string"}}}}}],
                "messages": history}

    def native(self):
        return {"model": "claude-haiku-5-5", "stream": True, "max_tokens": 2048,
                "thinking": {"type": "adaptive"}, "output_config": {"effort": "high"},
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

    def test_history_failures_reach_the_intended_oracle(self):
        for native, original, cases in [
            (False, self.chat(), [
                (("messages", 1, "reasoning_content"), "changed", "chat_readable_reasoning"),
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

    def test_duplicate_client_transmission_and_missing_identity_reject(self):
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
                          (2, False, "group", {}), (0, True, "probe-group", {}), (3, False, "group", {})]
        state.finish(("original", "probe"))
        with self.assertRaisesRegex(RuntimeError, "^client_session_carrier_mismatch$"):
            state.finish(("unrelated", "probe"))
        state.attempts[3] = (0, True, None, {})
        with self.assertRaisesRegex(RuntimeError, "^native_recovery_session_not_independent$"):
            state.finish(("original", "probe"))

    def test_opencode_final_text_and_probe_completion_are_independent_oracles(self):
        drivers.check_opencode_output([{"type": "text", "part": {"text": recovery.TEXT}}])
        for events in ([], [{"type": "text", "part": {"text": "changed"}}]):
            with self.assertRaisesRegex(RuntimeError, "^opencode_final_answer$"):
                drivers.check_opencode_output(events)
        with closing(sqlite3.connect(":memory:")) as database:
            database.execute("CREATE TABLE session_message(id TEXT, session_id TEXT, type TEXT, time_created INT, data TEXT)")
            database.execute("CREATE TABLE session_v2(id TEXT, idle_outcome TEXT)")
            database.execute("INSERT INTO session_message VALUES ('message','probe','assistant',1,?)",
                             (json.dumps({"finish": "stop"}),))
            database.execute("INSERT INTO session_v2 VALUES ('probe','succeeded')")
            drivers.check_opencode_session(database, "probe", ["stop"])
            database.execute("UPDATE session_v2 SET idle_outcome='failed'")
            with self.assertRaisesRegex(RuntimeError, "^opencode_idle_outcome$"):
                drivers.check_opencode_session(database, "probe", ["stop"])

    def test_server_teardown_releases_an_incomplete_header_worker(self):
        state = recovery.State(False, self.path)
        with recovery.server(state, False) as origin:
            address = recovery.urlsplit(origin)
            connection = socket.create_connection((address.hostname, address.port), timeout=3)
            self.addCleanup(connection.close)
            connection.sendall(b"POST /v1/chat/completions HTTP/1.1\r\nHost: unfinished")
            self.assertTrue(state.accepted.wait(timeout=2))
        self.assertEqual(connection.recv(1), b"")

    def test_authority_advertises_connection_close_before_retry(self):
        state = recovery.State(False, self.path)
        initial = self.chat()
        initial["messages"] = initial["messages"][:1]
        with recovery.server(state, False) as origin:
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
                    response.read()
                finally:
                    connection.close()
        self.assertFalse(state.errors)


if __name__ == "__main__":
    unittest.main()
