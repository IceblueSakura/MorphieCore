"""Offline failure oracles for the bounded real-Agent runner; no Provider access."""
import copy
import http.client
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from urllib.parse import urlsplit

import reasoning_live as live


class ReasoningLiveOracles(unittest.TestCase):
    def test_live_final_answer_is_distinct_from_pretool_text_and_preserves_replay_bytes(self):
        events = [{"type": "text", "part": {"text": "I will read the fixture."}},
                  {"type": "text", "part": {"text": live.TEXT + "\n"}}]
        live.opencode_final_output(events)
        for invalid in ([], events[:1], events + [{"type": "text", "part": {"text": "wrong"}}]):
            with self.assertRaisesRegex(RuntimeError, "^live_client_final_answer$"):
                live.opencode_final_output(invalid)
        observed = {"content": live.TEXT + "\n", "reasoning_content": "Check", "tool_calls": []}
        with self.assertRaisesRegex(RuntimeError, "^answer_changed$"):
            live.same_message({"role": "assistant", "content": live.TEXT, "reasoning_content": "Check"}, observed)

    def test_budget_and_history_are_checked_before_forwarding(self):
        owner = SimpleNamespace(address=SimpleNamespace(geturl=lambda: "http://127.0.0.1:1"),
                                key="synthetic", requests=[])
        path = Path("/synthetic/sample.txt")
        state = live.State(owner, path)
        headers = {"content-length": "200", "authorization": "Bearer synthetic"}
        body = {"model": live.MODEL, "stream": True, "max_completion_tokens": 2048,
                "reasoning_effort": "low", "messages": [{"role": "user", "content": "read"}]}
        invalid = {**body, "max_completion_tokens": 4096}
        with self.assertRaisesRegex(RuntimeError, "^fixed_live_controls$"):
            state.incoming(headers, invalid)
        self.assertEqual(owner.requests, [])
        state.incoming(headers, body)
        feedback = copy.deepcopy(body)
        feedback["messages"] += [{"role": "assistant", "reasoning_content": "Plan", "tool_calls": [
            {"id": "actual-call", "type": "function", "function": {"name": "read",
                "arguments": json.dumps({"path": str(path)})}}]},
            {"role": "tool", "tool_call_id": "actual-call", "content": live.TEXT}]
        broken = copy.deepcopy(feedback)
        broken["messages"][1].pop("reasoning_content")
        with self.assertRaisesRegex(RuntimeError, "^live_reasoning_not_observed$"):
            state.incoming(headers, broken)
        self.assertEqual(len(owner.requests), 1)
        state.incoming(headers, feedback)
        next_turn = copy.deepcopy(feedback)
        next_turn["messages"] += [{"role": "assistant", "content": live.TEXT, "reasoning_content": "Check"},
                                   {"role": "user", "content": "recall"}]
        state.incoming(headers, next_turn)
        with self.assertRaisesRegex(RuntimeError, "^live_request_budget$"):
            state.incoming(headers, next_turn)
        self.assertEqual(len(owner.requests), 3)
        self.assertEqual(state.received_requests, 6)

    def test_delivery_requires_complete_terminal_and_exact_reasoning_replay(self):
        def chunk(delta, finish=None):
            return ("data: " + json.dumps({"choices": [{"index": 0, "delta": delta,
                                                       "finish_reason": finish}]}) + "\n\n").encode()
        records = [chunk({"reasoning_content": "Plan "}), chunk({"reasoning_content": "🧪"}),
                   chunk({"content": "answer"}), chunk({}, "stop"),
                   b"data: [DONE]\n\n"]
        observed, terminal = live.observed_message(b"".join(records))
        self.assertEqual(terminal, "stop")
        self.assertEqual(observed["reasoning_content"], "Plan 🧪")
        actual = {"role": "assistant", "content": "answer", "reasoning_content": "Plan 🧪"}
        live.same_message(actual, observed)
        with self.assertRaisesRegex(RuntimeError, "^reasoning_changed$"):
            live.same_message({**actual, "reasoning_content": "changed"}, observed)
        with self.assertRaisesRegex(RuntimeError, "^client_stream_eof$"):
            live.observed_message(b"".join(records)[:-1])
        with self.assertRaisesRegex(RuntimeError, "^client_stream_done$"):
            live.observed_message(b"".join(records[:-1]))
        with self.assertRaisesRegex(RuntimeError, "^honest_terminal$"):
            live.observed_message(b"".join(records[:-2] + records[-1:]))
        with self.assertRaisesRegex(RuntimeError, "^client_data_after_done$"):
            live.observed_message(b"".join(records + records[:1]))
        with self.assertRaisesRegex(RuntimeError, "^response_body_limit$"):
            live.observed_message(b" " * ((2 << 20) + 1))

    def test_tool_fragments_are_folded_in_memory_without_losing_identity_or_arguments(self):
        def chunk(delta, finish=None):
            return ("data: " + json.dumps({"choices": [{"index": 0, "delta": delta,
                                                       "finish_reason": finish}]}) + "\r\n\r\n").encode()
        payload = (chunk({"reasoning_content": "Read 🧪"}) +
                   chunk({"tool_calls": [{"index": 0, "id": "issued-call", "type": "function",
                       "function": {"name": "read", "arguments": '{ "path": '}}]}) +
                   chunk({"tool_calls": [{"index": 0, "function": {"arguments": '"/synthetic/file" }'}}]}) +
                   chunk({}, "tool_calls") + b"data: [DONE]\r\n\r\n")
        observed, terminal = live.observed_message(payload)
        self.assertEqual(terminal, "tool_calls")
        self.assertEqual(observed, {"content": "", "reasoning_content": "Read 🧪", "tool_calls": [
            {"id": "issued-call", "function": {"name": "read", "arguments": '{ "path": "/synthetic/file" }'}}]})
        actual = {"role": "assistant", "content": None, "reasoning_content": "Read 🧪",
                  "tool_calls": [{"id": "issued-call", "type": "function", "function": {
                      "name": "read", "arguments": '{"path":"/synthetic/file"}'}}]}
        live.same_message(actual, observed)
        for owner, key, value, error in [
            (actual["tool_calls"][0], "id", "changed", "call_identity_or_arguments_changed"),
            (actual["tool_calls"][0]["function"], "arguments", "{}", "call_identity_or_arguments_changed"),
        ]:
            original = owner[key]
            owner[key] = value
            with self.assertRaisesRegex(RuntimeError, "^" + error + "$"):
                live.same_message(actual, observed)
            owner[key] = original
        with self.assertRaises(json.JSONDecodeError):
            live.observed_message(payload.replace(b'file\\" }', b'file\\"'))

    def test_forwarder_observes_the_unchanged_delivery_only_after_full_drain(self):
        import threading
        payload = (b'data: {"choices":[{"index":0,"delta":{"reasoning_content":"Plan"},'
                   b'"finish_reason":null}]}\n\n'
                   b'data: {"choices":[{"index":0,"delta":{"content":"cedar-71"},'
                   b'"finish_reason":"stop"}]}\n\ndata: [DONE]\n\n')
        authority = SimpleNamespace(native=False, lock=threading.Lock(), accepted=threading.Event(), errors=[],
            upstream=lambda *_: (200, payload, "text/event-stream"))
        with live.server(authority, False) as gateway:
            owner = SimpleNamespace(address=urlsplit(gateway), key="synthetic", requests=[])
            state = live.State(owner, Path("/synthetic/sample.txt"))
            with live.server(state, True) as ingress:
                address = urlsplit(ingress)
                connection = http.client.HTTPConnection(address.hostname, address.port, timeout=3)
                try:
                    body = {"model": live.MODEL, "stream": True, "max_completion_tokens": live.CAP,
                            "reasoning_effort": "low", "messages": [{"role": "user", "content": "read"}]}
                    connection.request("POST", "/v1/chat/completions", json.dumps(body),
                                       {"authorization": "Bearer synthetic", "content-type": "application/json"})
                    response = connection.getresponse()
                    self.assertEqual(response.status, 200)
                    self.assertEqual(response.read(), payload)
                finally:
                    connection.close()
        self.assertFalse(state.errors)
        self.assertFalse(authority.errors)
        self.assertEqual(state.outputs[0][0]["reasoning_content"], "Plan")
        self.assertEqual(state.outputs[0][1], "stop")

    def test_verdict_uses_memory_delivery_and_complete_prepared_requests_not_unsafe_fragments(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tool = {"role": "assistant", "content": None, "reasoning_content": "Plan",
                    "tool_calls": [{"id": "issued-call", "type": "function", "function": {
                        "name": "read", "arguments": '{"path":"/synthetic/file"}'}}]}
            final = {"role": "assistant", "content": live.TEXT, "reasoning_content": "Check"}
            first = {"model": live.MODEL, "max_completion_tokens": live.CAP, "stream": True,
                     "reasoning_effort": "low",
                     "messages": [{"role": "user", "content": "read"}]}
            second = {**first, "messages": [*first["messages"], tool,
                       {"role": "tool", "tool_call_id": "issued-call", "content": live.TEXT}]}
            third = {**first, "messages": [*second["messages"], final,
                      {"role": "user", "content": "recall"}]}
            owner = SimpleNamespace(root=root, requests=[first, second, third], key="synthetic",
                process=SimpleNamespace(returncode=0), address=urlsplit("http://127.0.0.1:1"))
            state = live.State(owner, Path("/synthetic/file"))
            state.received_requests = 3
            state.outputs = {0: ({key: copy.deepcopy(tool[key])
                                 for key in ("content", "reasoning_content", "tool_calls")}, "tool_calls"),
                             1: ({**final, "tool_calls": []}, "stop"),
                             2: ({**final, "tool_calls": []}, "stop")}
            state.outputs[0][0]["content"] = ""
            prepared = [{"exchange": index, "boundary": "upstream_request", "status": "body_unit",
                         "redacted": False, "dropped_before": 0, "value": body}
                        for index, body in enumerate(copy.deepcopy(owner.requests))]
            capture = [*prepared,
                       {"boundary": "upstream_response", "status": "unsafe_fragment",
                        "redacted": False, "dropped_before": 0},
                       {"boundary": "client_response", "status": "incomplete",
                        "redacted": False, "dropped_before": 0}]
            capture_path = root / "capture.jsonl"
            capture_path.write_text("".join(json.dumps(record) + "\n" for record in capture))
            (root / "diagnostics.jsonl").write_text("".join(json.dumps({
                "event": "request", "outcome": "complete", "candidates": [{}]}) + "\n" for _ in range(3)))
            self.assertTrue(state.verify()["readable_reasoning_replayed"])
            owner.requests[1]["messages"][1]["reasoning_content"] = "changed"
            with self.assertRaisesRegex(RuntimeError, "^reasoning_changed$"):
                state.verify()
            owner.requests[1]["messages"][1]["reasoning_content"] = "Plan"
            prepared[2]["value"]["messages"][1]["tool_calls"][0]["id"] = "changed"
            capture_path.write_text("".join(json.dumps(record) + "\n" for record in capture))
            with self.assertRaisesRegex(RuntimeError, "^call_identity_or_arguments_changed$"):
                state.verify()
            prepared[2]["value"]["messages"][1]["tool_calls"][0]["id"] = "issued-call"
            for key, value, error in [
                ("tool_call_id", "changed", "live_prepared_result_identity"),
                ("content", "changed", "live_prepared_history_content"),
            ]:
                result = prepared[1]["value"]["messages"][2]
                original = result[key]
                result[key] = value
                capture_path.write_text("".join(json.dumps(record) + "\n" for record in capture))
                with self.assertRaisesRegex(RuntimeError, "^" + error + "$"):
                    state.verify()
                result[key] = original
            prepared[2]["value"]["messages"].append(copy.deepcopy(final))
            capture_path.write_text("".join(json.dumps(record) + "\n" for record in capture))
            with self.assertRaisesRegex(RuntimeError, "^live_prepared_history_shape$"):
                state.verify()
            del state.outputs[2]
            with self.assertRaisesRegex(RuntimeError, "^live_response_count$"):
                state.verify()

    def test_failure_reporting_keeps_only_fixed_classes_and_counts(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            owner = SimpleNamespace(root=root, requests=[{}],
                address=SimpleNamespace(geturl=lambda: "http://127.0.0.1:1"), key="synthetic")
            state = live.State(owner, Path("/synthetic/sample.txt"))
            state.received_requests = 1
            (root / "diagnostics.jsonl").write_text(json.dumps({"event": "request",
                "outcome": "error", "stage": "response_head", "upstream_status": 400,
                "candidates": [{}], "request_id": "not-for-output"}) + "\n")
            (root / "capture.jsonl").write_text(json.dumps({"boundary": "upstream_response",
                "status": "body_unit", "value": {"error": {"code": "untrusted-private-code",
                    "message": "model unknown with synthetic-private-marker"}}}) + "\n")
            result = state.failure_report()
            self.assertEqual(result["upstream_attempts"], 1)
            self.assertEqual(result["provider_errors"][0]["mentioned_fields"], ["model"])
            self.assertEqual(result["provider_errors"][0]["code"], "unclassified")
            self.assertNotIn("synthetic-private-marker", json.dumps(result))
            self.assertNotIn("not-for-output", json.dumps(result))


if __name__ == "__main__":
    unittest.main()
