"""Owned listener, single SDK send boundary and bounded operator diagnostics."""

import contextlib
import copy
import json
import os
from pathlib import Path
import re
import secrets
import selectors
import signal
import subprocess
import uuid

import httpx2
from openai import DefaultHttpxClient, OpenAI, __version__
from .checks import require
from .ledger import MODELS
from .wire import Wire

ROOT = Path(__file__).resolve().parents[2]


class ObservedStream(httpx2.SyncByteStream):
    def __init__(self, source, wire):
        self.source, self.wire = source, wire

    def __iter__(self):
        iterator = iter(self.source)
        try:
            for raw in iterator:
                self.wire.push(raw)
                if self.wire.done:
                    # Check the remainder before exposing a terminal to consumers
                    # which intentionally stop parsing at DONE/completed.
                    for tail in iterator:
                        self.wire.push(tail)
                    self.wire.finish()
                    yield raw
                    return
                yield raw
            self.wire.finish()
        finally:
            self.source.close()

    def close(self):
        self.source.close()


class ProbeClient(DefaultHttpxClient):
    def __init__(self, origin, run):
        super().__init__(trust_env=False, follow_redirects=False)
        require(
            re.fullmatch(r"http://127\.0\.0\.1:\d+", origin) is not None,
            "origin",
            "setup",
        )
        self.origin, self.run = origin, run
        self.context = None
        self.attempt = None
        self.wire = None
        self.last_status = None
        self.expected_history = None
        self.history_ok = False

    def prepare(self, model, protocol, scenario, history):
        require(self.attempt is None, "unfinished_attempt", "setup")
        require(
            model in self.run.plan["models"] and protocol in MODELS[model][4]
            and (protocol == "images") == self.run.is_images,
            "selection",
            "budget",
        )
        self.context = (model, protocol, scenario)
        self.expected_history = copy.deepcopy(history)
        self.wire = None
        self.last_status = None
        self.history_ok = False

    def send(self, request, **kwargs):
        require(
            self.context is not None and self.attempt is None,
            "unplanned_send",
            "budget",
        )
        model, protocol, scenario = self.context
        path = {"responses": "/v1/responses", "chat": "/v1/chat/completions", "images": "/v1/images/generations"}[protocol]
        require(
            str(request.url) == self.origin + path and request.method == "POST",
            "destination",
            "budget",
        )
        require(len(request.content) <= 256 * 1024, "request_bytes", "budget")
        body = json.loads(request.content)
        if protocol == "images":
            cap, field = None, "prompt"
            require(
                set(body) <= {"model", "prompt", "n", "stream", "output_format"}
                and body.get("model") == model
                and isinstance(body.get("prompt"), str)
                and 0 < len(body["prompt"]) <= 32000
                and (body.get("n") is None and self.run.plan["images_per_request"] == 1
                     or type(body.get("n")) is int and body["n"] == self.run.plan["images_per_request"])
                and (body.get("stream") is None or body["stream"] is False)
                and body.get("output_format") is None,
                "controls", "budget",
            )
        else:
            cap = body.get("max_output_tokens" if protocol == "responses" else "max_completion_tokens")
            require(body.get("model") == model and self.run.valid_budget(model, cap), "controls", "budget")
            require("max_tokens" not in body and type(body.get("stream", False)) is bool, "controls", "budget")
            field = "input" if protocol == "responses" else "messages"
        require(body.get(field) == self.expected_history, "history_changed", "budget")
        self.history_ok = True
        self.attempt = self.run.reserve(model, scenario, cap)
        request.headers["x-morphiecore-probe-id"] = self.attempt
        self.run.dispatched(self.attempt)
        streaming = kwargs.pop("stream", False)
        response = super().send(request, stream=True, **kwargs)
        self.last_status = response.status_code
        is_sse = response.status_code < 300 and body.get("stream", False)
        self.wire = Wire(protocol, is_sse, limit=4 << 20 if protocol == "images" else 2 << 20)
        response.stream = ObservedStream(response.stream, self.wire)
        if not streaming:
            try:
                response.read()
            except BaseException:
                response.close()
                raise
        return response

    def complete(self, state, metrics):
        require(self.attempt is not None, "missing_attempt", "setup")
        metrics.update(
            http=self.last_status,
            history_ok=self.history_ok,
            wire_closed=bool(self.wire and self.wire.closed),
            wire_bytes=self.wire.bytes if self.wire else 0,
        )
        if self.wire and self.wire.terminal:
            metrics["terminal"] = self.wire.terminal
        identity = self.attempt
        self.run.finish(identity, state, metrics)
        self.attempt = None
        self.context = None
        return identity


def diagnostics(run):
    result = {}
    for path in run.directory.glob("gateway-*.jsonl"):
        require(path.stat().st_size <= 1 << 20, "diagnostic_size", "setup")
        for line in path.read_text().splitlines():
            try:
                record = json.loads(line)
            except ValueError:
                continue  # A crashed writer may leave a partial final record.
            identity = record.get("attempt", "")
            if re.fullmatch(run.plan["id"] + r":[1-9][0-9]{0,5}", identity):
                # Reports expose only the sink's closed scalar schema.
                allowed = {
                    "stage",
                    "upstream_status",
                    "retry_after_seconds",
                    "received_bytes",
                    "handed_off_bytes",
                    "reported_input_tokens", "reported_output_tokens", "reported_image_cost_usd",
                    "image_usage_omitted", "image_billing_omitted",
                    "elapsed_ms",
                    "upstream_head_ms",
                    "first_upstream_bytes_ms",
                    "decode_failure", "event_items", "event_reasoning_items",
                    "event_parts", "event_deltas", "event_item_closures",
                }
                from .ledger import closed_metrics

                facts = {key: value for key, value in record.items() if key in allowed}
                facts["operator_outcome"] = record.get("outcome")
                result[identity] = closed_metrics(facts)
    return result


def summary(run):
    records = run.snapshot()
    observed = diagnostics(run)
    for row in records:
        row["operator"] = observed.get(row["attempt"])
    # Atomic replace of a derived view; the SQLite ledger remains authoritative.
    path = run.directory / "summary.json"
    temp = run.directory / f".summary-{uuid.uuid4().hex}"
    temp.write_text(json.dumps({"plan": run.plan, "attempts": records}, indent=2))
    os.replace(temp, path)
    return records


@contextlib.contextmanager
def gateway(run, models=None, *, synthetic=False, proxy=None):
    models = models or run.plan["models"]
    require(all(model in run.plan["models"] for model in models), "selection", "budget")
    require(__version__ == "3.24.0", "sdk_version", "setup")
    if not synthetic:
        require(os.environ.get("MORPHIECORE_PROBE_LIVE") == "1", "live_not_enabled", "setup")
        require(
            bool(os.environ.get("MORPHIECORE_PROBE_CREDENTIALS_DIR")),
            "credential_directory", "setup",
        )
    key = secrets.token_urlsafe(32)
    private = run.directory / f"bootstrap-{uuid.uuid4().hex}"
    private.mkdir(mode=0o700)

    def save(name, value):
        descriptor = os.open(private / name, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, "w") as output:
            json.dump(value, output)

    if synthetic:
        documents = {}
        for model in models:
            provider, _, binding, profile, _ = MODELS[model]
            require(profile is None, "oauth_requires_owned_store", "setup")
            document = documents.setdefault(provider, {
                "provider": provider, "revision": 1, "oauth": {}, "api_keys": {}, "pools": {},
            })
            document["api_keys"]["synthetic"] = {
                "kind": "api_key", "domain": provider, "alias": "synthetic",
                "record_id": uuid.uuid4().hex, "epoch": uuid.uuid4().hex,
                "revision": 1, "generation": 1, "state": "enabled",
                "secret": "synthetic-upstream-credential",
            }
            document["pools"][binding] = {"revision": 1, "config": {
                "members": [{"kind": "api_key", "alias": "synthetic"}],
                "fallback": False, "max_attempts": 1,
            }}
        for provider, document in documents.items():
            save(provider + ".json", document)
    credential_directory = (
        str(private) if synthetic else os.environ["MORPHIECORE_PROBE_CREDENTIALS_DIR"]
    )
    config = {
        "bind": "127.0.0.1:0", "client_key": key, "models": models, "max_attempts": 1,
        "diagnostics": str(run.directory / f"gateway-{uuid.uuid4().hex}.jsonl"),
    }
    if proxy:
        config["proxy"] = proxy
    elif not synthetic:
        for name in (
            "MORPHIECORE_PROXY",
            "https_proxy",
            "HTTPS_PROXY",
            "http_proxy",
            "HTTP_PROXY",
            "all_proxy",
            "ALL_PROXY",
        ):
            if os.environ.get(name):
                config["proxy"] = os.environ[name]
                break
    require(
        not synthetic or proxy is not None,
        "synthetic_requires_rejecting_proxy",
        "setup",
    )
    save("gateway.json", config)
    server = subprocess.Popen(
        [
            str(ROOT / "target/debug/morphiecore"),
            "--credentials-dir", credential_directory,
            "--config", str(private / "gateway.json"),
        ],
        cwd=ROOT,
        env={},
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
    )
    try:
        with selectors.DefaultSelector() as selector:
            selector.register(server.stdout, selectors.EVENT_READ)
            require(bool(selector.select(timeout=15)), "startup_timeout", "setup")
            line = server.stdout.readline(256).strip()
        match = re.fullmatch(r"MorphieCore listening on (http://127\.0\.0\.1:\d+)", line)
        require(match is not None, "startup", "setup")
        yield match[1], key
    finally:
        if server.poll() is None:
            server.send_signal(signal.SIGINT)
            try:
                server.wait(timeout=12)
            except subprocess.TimeoutExpired:
                server.kill()
                server.wait()
        summary(run)


@contextlib.contextmanager
def session(run, models=None):
    with gateway(run, models) as (origin, key):
        transport = ProbeClient(origin, run)
        with OpenAI(
            base_url=origin + "/v1",
            api_key=key,
            max_retries=0,
            organization="",
            project="",
            timeout=130,
            http_client=transport,
        ) as client:
            yield client, transport
