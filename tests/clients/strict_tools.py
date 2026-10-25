"""Characterize pi's preferred-strict fallback, never a strict-capability success."""
from contextlib import ExitStack
import copy
import json
from pathlib import Path
import queue
import signal
import sys

from drivers import (Child, KEY, Pi, READ, TEXT, check, check_client_version,
                     environment, pi_configuration, private_json)
from recovery import State as RecoveryState, assert_profile, native_events, server

LIMIT = "The compiled grammar is too large"
ERROR = json.dumps({"type": "error", "error": {
    "type": "invalid_request_error", "message": LIMIT}}).encode()


class State(RecoveryState):
    def __init__(self, path, direct):
        super().__init__(True, path)
        self.direct = direct
        self.first = None

    def incoming(self, headers, body):
        assert_profile(body, True)
        with self.lock:
            self.ingress.append(True)
            check(len(self.ingress) == 1, "strict_gateway_extra_request")

    def upstream(self, headers, body):
        check(headers["x-api-key"] == (KEY if self.direct else "synthetic-client-primary"),
              "strict_unexpected_credential")
        with self.lock:
            self.attempts.append(True)
            count = len(self.attempts)
            check(count <= (2 if self.direct else 1), "strict_extra_attempt")
        if count == 1:
            assert_profile(body, True)
            self.first = copy.deepcopy(body)
            return 400, ERROR, "application/json"
        tools = body.get("tools", [])
        read = [t for t in tools if t.get("name") == "read"]
        check(len(read) == 1 and "strict" not in read[0], "strict_fallback_not_observed")
        # The client may change the schema when removing preferred strict sampling.
        # Everything outside that declaration must remain unchanged.
        check({k: v for k, v in body.items() if k != "tools"} ==
              {k: v for k, v in self.first.items() if k != "tools"}, "strict_fallback_changed_request")
        check(len(tools) == len(self.first["tools"])
              and [t for t in tools if t.get("name") != "read"] ==
              [t for t in self.first["tools"] if t.get("name") != "read"]
              and "path" in read[0]["input_schema"]["properties"],
              "strict_fallback_tool_identity")
        return 200, native_events(2, self.path), "text/event-stream"

    def finish(self):
        check(not self.errors, "strict_http_check_failed:" + ",".join(self.errors))
        check(len(self.ingress) == (0 if self.direct else 1)
              and len(self.attempts) == (2 if self.direct else 1), "strict_exchange_counts")


def run_client(binary, env, work, model, direct):
    with Pi(binary, env, work, model) as client:
        _, events = client.command("prompt", message=f"{READ}: Say {TEXT} without tools.")
        assistants = [e["message"] for e in events if e.get("type") == "message_end"
                      and e.get("message", {}).get("role") == "assistant"]
        check(len(assistants) == 1 and assistants[0]["stopReason"] == ("stop" if direct else "error"),
              "strict_client_outcome")
        diagnostics = assistants[0].get("diagnostics", [])
        check(sum(d.get("type") == "anthropic_strict_tools_fallback" for d in diagnostics) == int(direct),
              "strict_fallback_diagnostic")
        if direct:
            check(any(b.get("text") == TEXT for b in assistants[0]["content"]), "strict_direct_final")
        else:
            check(LIMIT not in assistants[0].get("errorMessage", ""), "strict_upstream_error_leaked")
        check(not any(e["type"].startswith(("auto_retry", "summarization_retry", "tool_execution"))
                      for e in events), "strict_unexpected_retry_or_tool")
        final, _ = client.command("get_state")
        check(not final["isStreaming"] and not final["isCompacting"], "strict_client_not_idle")
        client.process.stdin.close()
        client.wait(timeout=5)


def main():
    case, directory, gateway_binary, client_binary = sys.argv[1:]
    root = Path(directory)
    for direct in [True, False]:
        run = root / ("direct" if direct else "gateway")
        run.mkdir(mode=0o700)
        work = run / "work"
        work.mkdir(mode=0o700)
        env = environment(run)
        with Child([client_binary, "--version"], env, work) as child:
            version = child.wait(timeout=5).decode().strip()
        check_client_version(case, version)
        state = State(work / "unused.txt", direct)
        with ExitStack() as stack:
            origin = stack.enter_context(server(state, False, direct_client=direct))
            if not direct:
                private_json(run / "auth.json", {"version": 1, "agents": [{"agent_id": "fixture-agent", "key": KEY}]})
                private_json(run / "gateway.json", {"auth_file": "auth.json", "bind": "127.0.0.1:0",
                    "models_file": "models.json", "exchange_timeout_ms": 10000,
                    "diagnostics": "diagnostics.jsonl"})
                private_json(run / "models.json", {"models": {"claude-haiku-5-5": {
                    "max_attempts": 1, "total_timeout_ms": 8000, "candidates": [{
                        "binding": "opencode-go-haiku-messages", "base_url": origin + "/v1",
                        "attempt_timeout_ms": 3000}]}}})
                gateway = stack.enter_context(Child([gateway_binary, "--config", str(run / "gateway.json"),
                    "--credentials-dir", str(root / "credentials")], env, work))
                try:
                    ready = gateway.first_line.get(timeout=5).decode()
                except queue.Empty:
                    raise RuntimeError("gateway_readiness_deadline") from None
                check(len(ready) <= 127 and ready.startswith("MorphieCore listening on http://127.0.0.1:"),
                      "gateway_startup")
                state.gateway = ready.removeprefix("MorphieCore listening on ")
                origin = stack.enter_context(server(state, True))
            agent, model = pi_configuration(run, origin, True)
            try:
                run_client(client_binary, {**env, "PI_CODING_AGENT_DIR": str(agent),
                           "PI_CACHE_RETENTION": "short"}, work, model, direct)
            except Exception:
                print(json.dumps({"direct": direct, "errors": state.errors,
                                  "client_requests": len(state.ingress),
                                  "upstream_attempts": len(state.attempts)}), file=sys.stderr)
                raise
            if not direct:
                gateway.process.send_signal(signal.SIGINT)
                gateway.wait(timeout=5)
                records = [json.loads(line) for line in (run / "diagnostics.jsonl").read_text().splitlines()]
                check(all(r.get("event") in ("file", "request", "log", "span") for r in records), "strict_logging_loss")
                requests = [r for r in records if r.get("event") == "request"]
                check(len(requests) == 1 and requests[0]["outcome"] == "error"
                      and len(requests[0]["candidates"]) == 1
                      and requests[0].get("ignored_request_field_count", 0) == 0, "strict_gateway_outcome")
        state.finish()
    print(json.dumps({"case": case, "version": version, "direct_client_requests": 2,
                      "direct_strict_fallbacks": 1, "client_requests": 1, "upstream_attempts": 1,
                      "gateway_strict_fallbacks": 0, "honest_error": True}))


if __name__ == "__main__":
    main()
