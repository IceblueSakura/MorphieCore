"""Offline failure oracles for the explicit live runner; no loader or Provider access."""
from contextlib import contextmanager
import copy
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
from pathlib import Path
import tempfile
import threading
from types import SimpleNamespace
import unittest
from urllib.parse import urlsplit
from unittest.mock import patch

import messages_inputs_live as live


@contextmanager
def authority(status, reply):
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            self.connection.settimeout(2)
            self.rfile.read(int(self.headers["content-length"]))
            body = json.dumps(reply).encode()
            self.send_response(status)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    with HTTPServer(("127.0.0.1", 0), Handler) as server:
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            yield urlsplit(f"http://127.0.0.1:{server.server_port}")
        finally:
            server.shutdown()
            thread.join(timeout=3)
            assert not thread.is_alive()


class MessagesInputRunnerOracles(unittest.TestCase):
    def test_exact_capture_requires_unredacted_distinct_exchanges(self):
        with tempfile.TemporaryDirectory() as directory:
            gateway = live.Gateway.__new__(live.Gateway)
            gateway.root = Path(directory)
            gateway.process = SimpleNamespace(returncode=0)
            gateway.requests = [{}]
            gateway.budget = 1
            diagnostic = {"event": "request", "outcome": "complete", "candidates": [{}]}
            capture = {"exchange": "synthetic-1", "attempt": 1, "boundary": "upstream_request",
                       "status": "body_unit", "dropped_before": 0, "redacted": False, "value": {}}
            (gateway.root / "diagnostics.jsonl").write_text(json.dumps(diagnostic) + "\n")
            path = gateway.root / "capture.jsonl"
            path.write_text(json.dumps(capture) + "\n")
            self.assertEqual(gateway.verify(exact=True), [{}])
            path.write_text(json.dumps({**capture, "redacted": True}) + "\n")
            with self.assertRaisesRegex(live.GateError, "^redacted_history_observation$"):
                gateway.verify(exact=True)
            gateway.requests *= 2
            gateway.budget = 2
            (gateway.root / "diagnostics.jsonl").write_text((json.dumps(diagnostic) + "\n") * 2)
            path.write_text((json.dumps(capture) + "\n") * 2)
            with self.assertRaisesRegex(live.GateError, "^duplicate_captured_exchange$"):
                gateway.verify(exact=True)

    def gateway(self, address):
        gateway = live.Gateway.__new__(live.Gateway)
        gateway.address = address
        gateway.key = "synthetic-ingress-key"
        gateway.requests = []
        gateway.budget = 1
        return gateway

    def test_standard_http_exchange_has_no_retry_and_checks_native_completion(self):
        request = live.request([live.user("synthetic")])
        reply = {"type": "message", "role": "assistant", "stop_reason": "end_turn", "content": []}
        with authority(200, reply) as address:
            gateway = self.gateway(address)
            self.assertEqual(gateway.generate(request), reply)
            with self.assertRaisesRegex(live.GateError, "^client_request_budget$"):
                gateway.generate(request)
        with authority(503, {"error": {"code": "untrusted-code-and-private-prose"}}) as address:
            gateway = self.gateway(address)
            with self.assertRaisesRegex(live.GateError, "^http_503$"):
                gateway.generate(request)
            self.assertEqual(len(gateway.requests), 1)
        with authority(200, {**reply, "stop_reason": "max_tokens"}) as address:
            with self.assertRaisesRegex(live.GateError, "^incomplete_native_outcome$"):
                self.gateway(address).generate(request)

    def test_example_plan_preserves_model_issued_ids_and_exact_definition_owners(self):
        class Gateway:
            def __init__(self):
                self.requests = []

            def generate(self, body):
                self.requests.append(copy.deepcopy(body))
                turn = len(self.requests)
                if turn <= 2:
                    return {"stop_reason": "tool_use", "content": [{"type": "tool_use",
                        "id": f"model-issued-{turn}", "name": "lookup" if turn == 1 else "inspect",
                        "input": {"label": "cedar" if turn == 1 else "birch"}}]}
                return {"stop_reason": "end_turn", "content": [{"type": "text", "text": "cedar-71 birch-82"}]}

        gateway = Gateway()
        root, inline = live.examples(gateway)
        self.assertEqual(len(gateway.requests), 3)
        for body in gateway.requests:
            self.assertEqual(body["tools"][0], root)
            self.assertEqual(body["max_tokens"], 2048)
        self.assertEqual(gateway.requests[1]["messages"][2]["content"][0]["tool_use_id"], "model-issued-1")
        self.assertEqual(gateway.requests[2]["messages"][5]["content"][0]["tool_use_id"], "model-issued-2")
        self.assertEqual(gateway.requests[2]["messages"][3]["content"][0]["tool"]["definition"], inline)
        with self.assertRaisesRegex(live.GateError, "^expected_model_call_not_observed$"):
            live.call({"stop_reason": "end_turn", "content": []}, "lookup", {"label": "cedar"})

    def test_context_plan_keeps_separate_uses_and_actual_tool_result_identity(self):
        class Gateway:
            def __init__(self):
                self.requests = []

            def generate(self, body):
                self.requests.append(copy.deepcopy(body))
                turn = len(self.requests)
                if turn == 2:
                    return {"stop_reason": "tool_use", "content": [{"type": "tool_use",
                        "id": "model-issued-fetch", "name": "fetch", "input": {"label": "birch"}}]}
                return {"stop_reason": "end_turn", "content": [{"type": "text",
                    "text": "ALPHA cedar-71\nBETA cedar-71" if turn == 1 else "GAMMA birch-82"}]}

        gateway = Gateway()
        first, second, result = live.contexts(gateway)
        self.assertEqual(first["source"], second["source"])
        self.assertNotEqual(first["context"], second["context"])
        self.assertEqual(gateway.requests[0]["messages"][0]["content"][:2], [first, second])
        feedback = gateway.requests[2]["messages"][-1]["content"][0]
        self.assertEqual(feedback["tool_use_id"], "model-issued-fetch")
        self.assertEqual(feedback["content"], [result])

    def test_cache_plan_requires_reported_writes_reads_and_ttl_classes(self):
        class Gateway:
            def __init__(self, missing=False):
                self.requests = []
                self.missing = missing

            def generate(self, body, session):
                self.requests.append((copy.deepcopy(body), session))
                writing = len(self.requests) % 2 == 1
                ttl = body["cache_control"]["ttl"]
                usage = {"cache_creation_input_tokens": 17 if writing else 0,
                         "cache_read_input_tokens": 0 if writing else 17}
                if not self.missing:
                    usage["cache_creation"] = {
                        "ephemeral_5m_input_tokens": 17 if writing and ttl == "5m" else 0,
                        "ephemeral_1h_input_tokens": 17 if writing and ttl == "1h" else 0}
                return {"stop_reason": "end_turn", "content": [{"type": "text", "text": "CACHE_OK"}],
                        "usage": usage}

        gateway = Gateway()
        with patch.object(live.secrets, "token_hex", side_effect=["first", "second"]):
            reports = live.automatic_cache(gateway)
        self.assertEqual([r["ttl"] for r in reports], ["5m", "1h"])
        self.assertEqual(gateway.requests[0], gateway.requests[1])
        self.assertEqual(gateway.requests[2], gateway.requests[3])
        self.assertNotEqual(gateway.requests[0][1], gateway.requests[2][1])
        missing = Gateway(missing=True)
        with self.assertRaisesRegex(live.GateError, "^cache_ttl_write_class_not_observed$"):
            live.automatic_cache(missing)
        self.assertEqual(len(missing.requests), 2)

    def test_cache_controls_allow_only_the_documented_five_minute_default(self):
        wires = [{"cache_control": {"type": "ephemeral"}},
                 {"cache_control": {"type": "ephemeral", "ttl": "5m"}},
                 {"cache_control": {"type": "ephemeral", "ttl": "1h"}},
                 {"cache_control": {"type": "ephemeral", "ttl": "1h"}}]
        live.verify_cache_controls(wires)
        for changed in [None, {"type": "ephemeral"}, {"type": "ephemeral", "ttl": None},
                        {"type": "ephemeral", "ttl": "1h", "extra": True}]:
            broken = copy.deepcopy(wires)
            broken[-1]["cache_control"] = changed
            with self.assertRaisesRegex(live.GateError, "^automatic_cache_ttl_changed$"):
                live.verify_cache_controls(broken)


if __name__ == "__main__":
    unittest.main()
