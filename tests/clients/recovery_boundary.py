"""Installed pi observes final failures, next-request failover and recovery cancellation."""
from contextlib import ExitStack
import json
from pathlib import Path
import queue
import sys
import threading
import time

from drivers import (Child, NEXT, PROBE, TEXT, Pi, check, check_client_version,
                     environment, gateway_configuration, pi_configuration, shutdown_gateway)
from recovery import State as RecoveryState, assert_profile, chat_events, server, text


def wait_for_diagnostic(root, predicate, timeout, code):
    deadline = time.monotonic() + timeout
    tick = threading.Event()
    while True:
        raw = (root / "diagnostics.jsonl").read_bytes()
        check(len(raw) <= 64 << 10, "boundary_diagnostic_bound")
        records = [json.loads(line) for line in raw.split(b"\n")[:-1] if line]
        found = next((row for row in records if predicate(row)), None)
        if found is not None:
            return found
        check(time.monotonic() < deadline, code)
        tick.wait(timeout=0.01)


class State(RecoveryState):
    def __init__(self, scenario):
        super().__init__(False, Path("/unused"))
        check(scenario in ("next-request", "retry-limit", "cancel-wait"), "recovery_scenario")
        self.scenario = scenario
        self.retry_after = {"next-request": "60", "retry-limit": "0", "cancel-wait": "2"}[scenario]
        self.failure_sent = threading.Event()
        self.extra_send = threading.Event()
        self.responses = []

    def incoming(self, headers, body):
        assert_profile(body, False)
        prompt = text([m for m in body["messages"] if m["role"] == "user"][-1]["content"])
        phase = 0 if PROBE in prompt else 3
        check((PROBE if phase == 0 else NEXT) in prompt, "boundary_prompt")
        identity = next((headers[h] for h in ("session_id", "x-session-id",
                        "x-morphiecore-conversation-id") if headers.get(h)), None)
        check(identity is not None, "boundary_session")
        with self.lock:
            self.ingress.append((phase, identity))
            check([p for p, _ in self.ingress] == [0, 3, 3][:len(self.ingress)]
                  and len(self.ingress) <= 3, "boundary_client_budget")

    def upstream(self, headers, body):
        assert_profile(body, False)
        primary = headers.get("authorization") == "Bearer synthetic-client-primary"
        check(primary or headers.get("authorization") == "Bearer synthetic-client-secondary",
              "boundary_credential")
        with self.lock:
            self.attempts.append((primary, body))
            ordinal = len(self.attempts)
        failures = 2 if self.scenario == "retry-limit" else 1
        if ordinal > failures + 2:
            self.extra_send.set()
        check(ordinal <= failures + 2, "boundary_send_budget")
        if ordinal <= failures:
            check(primary and (self.scenario == "cancel-wait" or len(self.ingress) == 1),
                  "boundary_request_switched_target")
            return 503, b"", "application/json"
        if self.scenario != "cancel-wait":
            check(len(self.ingress) == ordinal - failures + 1, "boundary_hidden_retry")
        # Cancellation preserves the already observed 503 health evidence.
        check(not primary, "boundary_next_request_target")
        check(all(m["role"] != "assistant" or m.get("content") != TEXT
                  for m in body["messages"]) if ordinal == failures + 1 else
              any(m.get("content") == TEXT for m in body["messages"]),
              "boundary_terminal_history")
        return 200, chat_events(3, self.path), "text/event-stream"

    def observe_response(self, body, status, content_type, raw):
        self.responses.append(status)

    def finish(self, identity):
        check(not self.errors, "boundary_authority_errors:" + ",".join(self.errors))
        failures = 2 if self.scenario == "retry-limit" else 1
        check(len(self.ingress) == (0 if self.scenario == "cancel-wait" else 3)
              and len(self.attempts) == len(self.wires) == failures + 2,
              "boundary_exchange_counts")
        check(all(session == identity for _, session in self.ingress), "boundary_session_changed")
        if failures == 2:
            check(self.wires[0][0] == self.wires[1][0], "boundary_retry_changed_wire")
        check(self.responses == ([] if self.scenario == "cancel-wait" else [503, 200, 200]),
              "boundary_downstream_status")


def run(binary, env, root, work, origin, state):
    agent, model = pi_configuration(root, origin, False, provider_timeout_ms=10000)
    env = {**env, "PI_CODING_AGENT_DIR": str(agent)}
    with Pi(binary, env, work, model) as client:
        initial, _ = client.command("get_state")
        if state.scenario == "cancel-wait":
            prompt = client.submit("prompt", message=f"{PROBE}: Say {TEXT} without tools.")
            check(state.failure_sent.wait(timeout=5), "boundary_failure_not_sent")
            # Synchronize on the gateway's failed-attempt receipt, not a sleep
            # after the authority write (which can race upstream classification).
            receipt = wait_for_diagnostic(root,
                lambda r: r.get("telemetry", {}).get("event_name") == "gateway.attempt.finished"
                and r["telemetry"]["attributes"].get("outcome") == "failed",
                1.5, "boundary_failure_not_observed")
            request_id = receipt["telemetry"]["attributes"]["request_id"]
            check(time.monotonic() - state.hints[0][0] < 2, "boundary_hint_elapsed_before_cancel")
            abort = client.submit("abort")
            acknowledged, terminals, settled = set(), [], False
            deadline = time.monotonic() + 5
            while len(acknowledged) != 2 or not settled:
                event = client.record(deadline)
                if event.get("type") == "response" and event.get("id") in (prompt, abort):
                    check(event.get("success") is True, "boundary_cancel_rejected")
                    acknowledged.add(event["id"])
                if event.get("type") == "message_end" and event.get("message", {}).get("role") == "assistant":
                    terminals.append(event["message"]["stopReason"])
                if event.get("type") == "agent_settled":
                    check(event.get("aborted") is True, "boundary_false_cancel_success")
                    settled = True
                check(not event.get("type", "").startswith(("auto_retry", "compaction")),
                      "boundary_client_retry")
            check(terminals == ["aborted"], "boundary_cancel_terminal")
            receipt = wait_for_diagnostic(root,
                lambda r: r.get("event") == "request" and r.get("request_id") == request_id,
                3, "boundary_gateway_cancel_not_completed")
            check(receipt["outcome"] != "complete" and len(receipt["candidates"]) == 1,
                  "boundary_gateway_cancel_not_honest")
        else:
            _, events = client.command("prompt", message=f"{PROBE}: Say {TEXT} without tools.")
            assistants = [e["message"] for e in events if e.get("type") == "message_end"
                          and e.get("message", {}).get("role") == "assistant"]
            check([m["stopReason"] for m in assistants] == ["error"]
                  and not any(e["type"].startswith(("auto_retry", "tool_execution", "compaction"))
                              for e in events), "boundary_failure_not_honest")
        client.prompt(f"{NEXT}: Say {TEXT} without tools.", ["stop"])
        client.prompt(f"{NEXT}: Recall the answer without tools.", ["stop"])
        final, _ = client.command("get_state")
        check(final["sessionId"] == initial["sessionId"] and not final["isStreaming"],
              "boundary_client_not_idle")
        if state.scenario == "cancel-wait":
            check(not state.extra_send.wait(timeout=2.1), "boundary_late_retry_after_cancel")
    return initial["sessionId"]


def main():
    case, directory, gateway_binary, binary, scenario = sys.argv[1:]
    root = Path(directory)
    work = root / "work"
    work.mkdir(mode=0o700)
    env = environment(root)
    with Child([binary, "--version"], env, work) as child:
        version = child.wait(timeout=5).decode().strip()
    check_client_version(case, version)
    state = State(scenario)
    with ExitStack() as stack:
        upstream = stack.enter_context(server(state, False))
        gateway_configuration(root, False, upstream, accounts=("primary", "secondary"),
            recovery={"recovery_timeout_ms": 5000, "transient_cooldown_ms": 60000})
        gateway = stack.enter_context(Child(
            [gateway_binary, "--config", str(root / "gateway.json")], env, work))
        try:
            ready = gateway.first_line.get(timeout=5).decode().strip()
        except queue.Empty:
            raise RuntimeError("gateway_readiness_deadline") from None
        check(ready.startswith("MorphieCore listening on http://127.0.0.1:"), "gateway_startup")
        state.gateway = ready.removeprefix("MorphieCore listening on ")
        # Cancellation must reach the gateway directly, not a blocking admission forwarder.
        origin = state.gateway if scenario == "cancel-wait" else stack.enter_context(server(state, True))
        failed = False
        try:
            identity = run(binary, env, root, work, origin, state)
        except Exception:
            failed = True
            print(json.dumps({"errors": state.errors, "primary_sends": [p for p, _ in state.attempts],
                              "client_requests": len(state.ingress), "responses": state.responses}),
                  file=sys.stderr)
            raise
        finally:
            shutdown_gateway(gateway, failed=failed)
        records = [json.loads(line) for line in (root / "diagnostics.jsonl").read_text().splitlines()]
        check(all(r.get("event") in ("file", "request", "log", "span") for r in records),
              "boundary_logging_loss")
        requests = [r for r in records if r.get("event") == "request"]
        check(len(requests) == 3 and requests[0]["outcome"] != "complete"
              and all(r["outcome"] == "complete" for r in requests[1:])
              and all(r.get("ignored_request_field_count", 0) == 0 for r in requests),
              "boundary_diagnostics")
        check([len(r["candidates"]) for r in requests] ==
              ([2, 1, 1] if scenario == "retry-limit" else [1, 1, 1]), "boundary_attempt_records")
    state.finish(identity)
    print(json.dumps({"case": case, "scenario": scenario, "version": version,
        "client_requests": 3, "upstream_attempts": len(state.attempts),
        "honest_failure": True, "next_request_boundary": True}))


if __name__ == "__main__":
    main()
