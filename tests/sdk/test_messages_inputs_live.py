"""Offline failure oracles for the explicit live runner; no loader or Provider access."""
from contextlib import contextmanager
import copy
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import threading
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


if __name__ == "__main__":
    unittest.main()
