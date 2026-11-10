"""Bounded test-owned OTLP/HTTP JSON receiver for failure synchronization."""
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import threading
import time


def attribute(record, key):
    for field in record.get("attributes", []):
        if field["key"] == key:
            value = field["value"]
            for kind in ("stringValue", "boolValue", "intValue"):
                if kind in value:
                    return int(value[kind]) if kind == "intValue" else value[kind]
            raise RuntimeError("unsupported_otlp_attribute")
    return None


class Collector:
    def __init__(self):
        self.records = []
        self.condition = threading.Condition()
        owner = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_POST(self):
                self.connection.settimeout(2)
                length = int(self.headers["content-length"])
                if self.path != "/v1/logs" or not 0 < length <= 1 << 20:
                    self.send_error(400)
                    return
                payload = json.loads(self.rfile.read(length))
                with owner.condition:
                    for resource in payload["resourceLogs"]:
                        for scope in resource["scopeLogs"]:
                            for record in scope["logRecords"]:
                                if len(owner.records) >= 1024:
                                    raise RuntimeError("otlp_record_limit")
                                owner.records.append(record)
                    owner.condition.notify_all()
                self.send_response(200)
                self.send_header("content-type", "application/json")
                self.send_header("content-length", "2")
                self.end_headers()
                self.wfile.write(b"{}")

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.server.daemon_threads = False
        self.endpoint = f"http://127.0.0.1:{self.server.server_port}/v1/logs"
        self.thread = threading.Thread(target=self.server.serve_forever)

    def wait(self, predicate, timeout, code):
        deadline = time.monotonic() + timeout
        with self.condition:
            while True:
                for record in self.records:
                    if predicate(record):
                        return record
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise RuntimeError(code)
                self.condition.wait(remaining)

    def __enter__(self):
        self.thread.start()
        return self

    def __exit__(self, *_):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=3)
        if self.thread.is_alive():
            raise RuntimeError("otlp_receiver_not_stopped")
