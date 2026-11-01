"""Installed native Messages defaults through the real gateway, with no header/compat overrides."""
from contextlib import ExitStack, closing
import json
from pathlib import Path
import queue
import signal
import sqlite3
import sys

from drivers import (Child, KEY, NEXT, READ, TEXT, Pi, check, check_client_version,
                     check_opencode_session, environment, opencode_configuration,
                     opencode_prompt, private_json)
from recovery import State as RecoveryState, assert_history, native_events, server, stage

MODEL = "claude-haiku-5-5"
PROVIDER = "opencode-go"


class State(RecoveryState):
    def __init__(self, path):
        super().__init__(True, path)
        self.beta_seen = False
        self.block_binding_seen = False

    def incoming(self, headers, body):
        phase = stage(body)
        check(headers.get_all("anthropic-version") == ["2023-06-01"], "default_messages_version")
        check(body["model"] == MODEL and body["stream"] is True, "default_messages_model")
        check(body.get("thinking", {}).get("type") == "adaptive", "default_adaptive_thinking")
        check(body["max_tokens"] == 2048, "default_operational_output_cap")
        with self.lock:
            self.beta_seen |= bool(headers.get("anthropic-beta"))
            self.block_binding_seen |= "block_binding" in body.get("thinking", {})
            self.ingress.append(phase)
            check(len(self.ingress) <= 3 and self.ingress == [1, 2, 3][:len(self.ingress)],
                  "default_extra_or_reordered_client_request")

    def upstream(self, headers, body):
        phase = stage(body)
        check(headers.get("x-api-key") == "synthetic-client-primary", "default_upstream_credential")
        check(headers.get("anthropic-version") == "2026-10-21", "trusted_upstream_version")
        check(body["max_tokens"] == 2048 and body["thinking"]["type"] == "adaptive",
              "default_upstream_controls")
        check("block_binding" not in body["thinking"], "client_binding_not_omitted")
        assert_history(body, phase, True, self.path, profile=False)
        with self.lock:
            self.attempts.append(phase)
            check(len(self.attempts) <= 3 and self.attempts == [1, 2, 3][:len(self.attempts)],
                  "default_extra_or_reordered_upstream_attempt")
        return 200, native_events(phase, self.path), "text/event-stream"

    def finish(self):
        check(not self.errors, "default_authority_errors:" + ",".join(self.errors))
        check(self.ingress == self.attempts == [1, 2, 3], "default_exchange_counts")


def run_pi(binary, env, root, work, origin, path):
    agent = root / "pi-agent"
    agent.mkdir(mode=0o700)
    private_json(agent / "models.json", {"providers": {PROVIDER: {
        "baseUrl": origin, "apiKey": "$INGRESS_KEY",
        "modelOverrides": {MODEL: {"maxTokens": 2048}}}}})
    private_json(agent / "settings.json", {"retry": {"enabled": False,
        "provider": {"maxRetries": 0, "timeoutMs": 5000}},
        "compaction": {"enabled": False}, "cacheWarming": "off"})
    env = {**env, "PI_CODING_AGENT_DIR": str(agent), "PI_CACHE_RETENTION": "short"}
    with Pi(binary, env, work, MODEL, provider=PROVIDER, thinking=None) as client:
        initial, _ = client.command("get_state")
        model = initial["model"]
        check(model["id"] == MODEL and model["provider"] == PROVIDER
              and model["api"] == "anthropic-messages" and model["reasoning"]
              and model["compat"]["forceAdaptiveThinking"] is True
              and model["compat"]["supportsTemperature"] is False,
              "pi_resolved_builtin_metadata")
        check(initial["thinkingLevel"] == "medium", "pi_default_thinking_level")
        client.prompt(f"{READ}: Read {path}, then answer.", ["toolUse", "stop"])
        client.prompt(f"{NEXT}: Repeat the answer without reading again.", ["stop"])
        final, _ = client.command("get_state")
        check(final["sessionId"] == initial["sessionId"] and not final["isStreaming"],
              "pi_default_session_changed")
        messages, _ = client.command("get_messages")
        check([m["role"] for m in messages["messages"]] ==
              ["system", "user", "assistant", "toolResult", "assistant", "user", "assistant"],
              "pi_default_final_history")


def run_opencode(binary, env, root, work, origin, path):
    config = opencode_configuration(root, origin)
    value = json.loads(config.read_text())
    value["providers"] = {PROVIDER: {"env": ["INGRESS_KEY"],
        "settings": {"baseURL": origin + "/v1"}, "body": {"max_tokens": 2048},
        "models": {MODEL: {"limit": {"output": 2048},
            "settings": {"baseURL": origin + "/v1"}}}}}
    config.unlink()
    private_json(config, value)
    env = {**env, "OPENCODE_CONFIG": str(config), "OPENCODE_DISABLE_AUTOUPDATE": "1",
           "OPENCODE_DISABLE_MODELS_FETCH": "1", "OPENCODE_DISABLE_PROJECT_CONFIG": "1"}
    session, events = opencode_prompt(binary, env, work,
        f"{READ}: Read {path}, then answer.", "messages-default", model=f"{PROVIDER}/{MODEL}")
    tools = [e["part"] for e in events if e.get("type") == "tool_use"]
    check(len(tools) == 1 and tools[0]["tool"] == "read"
          and tools[0]["state"]["status"] == "completed"
          and TEXT in tools[0]["state"]["output"], "opencode_default_actual_read")
    resumed, _ = opencode_prompt(binary, env, work,
        f"{NEXT}: Repeat the answer without reading again.", "messages-default", session,
        model=f"{PROVIDER}/{MODEL}")
    check(resumed == session, "opencode_default_session_changed")
    databases = list(Path(env["XDG_DATA_HOME"]).rglob("opencode.db"))
    check(len(databases) == 1, "opencode_database_count")
    with closing(sqlite3.connect(f"file:{databases[0]}?mode=ro", uri=True)) as database:
        check_opencode_session(database, session, ["tool-calls", "stop", "stop"])


def main():
    case, directory, gateway_binary, binary = sys.argv[1:]
    root = Path(directory)
    work = root / "work"
    work.mkdir(mode=0o700)
    path = work / "sample.txt"
    path.write_text(TEXT + "\n", encoding="utf-8")
    state = State(path)
    env = environment(root)
    with Child([binary, "--version"], env, work) as client:
        version = client.wait(timeout=5).decode().strip()
    check_client_version(case, version)
    private_json(root / "auth.json", {"version": 1, "agents": [{"agent_id": "fixture-agent", "key": KEY}]})
    private_json(root / "gateway.json", {"auth_file": "auth.json", "bind": "127.0.0.1:0",
        "models_file": "models.json", "exchange_timeout_ms": 10000, "diagnostics": "diagnostics.jsonl"})
    with ExitStack() as stack:
        origin = stack.enter_context(server(state, False))
        private_json(root / "models.json", {"models": {MODEL: {"max_attempts": 1,
            "total_timeout_ms": 8000, "candidates": [{"binding": "opencode-go-haiku-messages",
                "base_url": origin + "/v1", "attempt_timeout_ms": 3000}]}}})
        gateway = stack.enter_context(Child([gateway_binary, "--config", str(root / "gateway.json"),
            "--credentials-dir", str(root / "credentials")], env, work))
        try:
            ready = gateway.first_line.get(timeout=5).decode()
        except queue.Empty:
            raise RuntimeError("gateway_readiness_deadline") from None
        check(len(ready) <= 127 and ready.startswith("MorphieCore listening on http://127.0.0.1:"),
              "gateway_startup")
        state.gateway = ready.removeprefix("MorphieCore listening on ")
        ingress = stack.enter_context(server(state, True))
        try:
            (run_pi if case.startswith("pi-") else run_opencode)(
                binary, env, root, work, ingress, path)
            gateway.process.send_signal(signal.SIGINT)
            gateway.wait(timeout=5)
            records = [json.loads(line) for line in (root / "diagnostics.jsonl").read_text().splitlines()]
            check(all(r.get("event") in ("file", "request", "log", "span") for r in records),
                  "default_gateway_logging_loss")
            requests = [r for r in records if r.get("event") == "request"]
            check(len(requests) == 3 and all(r["outcome"] == "complete" and len(r["candidates"]) == 1
                for r in requests), "default_gateway_completion")
        except Exception:
            print(json.dumps({"errors": state.errors, "ingress_phases": state.ingress,
                              "upstream_phases": state.attempts}), file=sys.stderr)
            raise
    state.finish()
    print(json.dumps({"case": case, "version": version, "client_requests": 3,
        "upstream_attempts": 3, "extra_client_requests": 0, "default_headers": True,
        "tool_history": True, "beta_header_seen": state.beta_seen,
        "block_binding_seen": state.block_binding_seen}))


if __name__ == "__main__":
    main()
