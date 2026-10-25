"""OpenCode ordinary compaction, one retry/nudge and rebuilt tool history; no Provider."""
from contextlib import ExitStack, closing
import copy
import json
from pathlib import Path
import queue
import signal
import sqlite3
import sys
import time

from drivers import (Child, KEY, READ, TEXT, check, check_client_version, check_opencode_session,
                     environment, opencode_configuration, opencode_prompt, private_json)
from recovery import (CALL, PLAN, State as RecoveryState, assert_history, assert_profile,
                      chat_events, server, text)
from summary import AFTER, SESSION_HEADERS

SUMMARY = "## Objective\nRemember cedar-71.\n## Work State\nRead completed.\n## Next Move\nRead again."
NUDGE = "The previous response did not fill in the required summary template."
ORDER = ["initial", "feedback", "summary", "summary", "nudge", "rebuilt", "rebuilt-feedback"]


def classify(body):
    last = text([m for m in body["messages"] if m["role"] == "user"][-1]["content"])
    if NUDGE in last:
        return "nudge"
    if "structured summary" in last and "## Objective" in last:
        return "summary"
    rebuilt = "<conversation-checkpoint>" in text(body["messages"])
    results = any(m["role"] == "tool" for m in body["messages"])
    if results:
        return "rebuilt-feedback" if rebuilt else "feedback"
    check((AFTER if rebuilt else READ) in last, "opencode_unclassified_summary_request")
    return "rebuilt" if rebuilt else "initial"


def assert_summary(body, path, nudged=False):
    assert_profile(body, False)
    history = [m for m in body["messages"] if m["role"] != "system"]
    check([m["role"] for m in history] ==
          ["user", "assistant", "tool", "assistant", "user"] + (["user"] if nudged else []),
          "opencode_summary_history_order")
    # Native tool history is not replaced by an unverified text serialization.
    initial = {**body, "messages": history[:4] + [history[4]]}
    assert_history(initial, 3, False, path)
    check(READ in text(history[0]["content"])
          and "## Objective" in text(history[4]["content"])
          and "Do not continue the task or call tools." in text(history[4]["content"]),
          "opencode_summary_prompt")
    if nudged:
        check(NUDGE in text(history[-1]["content"]), "opencode_summary_nudge")


def assert_rebuilt(body, phase, path):
    assert_profile(body, False)
    history = [m for m in body["messages"] if m["role"] != "system"]
    check([m["role"] for m in history] == ["user", "user"] +
          (["assistant", "tool"] if phase == "rebuilt-feedback" else []),
          "opencode_rebuilt_history_order")
    checkpoint = text(history[0]["content"])
    check(SUMMARY in checkpoint and "<conversation-checkpoint>" in checkpoint
          and "<summary>" in checkpoint and READ not in text(history)
          and AFTER in text(history[1]["content"]), "opencode_rebuilt_checkpoint")
    if phase == "rebuilt-feedback":
        check(history[2]["reasoning_content"] == PLAN, "opencode_rebuilt_reasoning")
        calls = history[2]["tool_calls"]
        check(len(calls) == 1 and calls[0]["id"] == CALL
              and calls[0]["function"]["name"] == "read"
              and json.loads(calls[0]["function"]["arguments"]) == {"path": str(path)},
              "opencode_rebuilt_tool_identity")
        check(history[3]["tool_call_id"] == CALL and TEXT in text(history[3]["content"]),
              "opencode_rebuilt_tool_result")


class State(RecoveryState):
    def __init__(self, path):
        super().__init__(False, path)
        self.summary = None

    def incoming(self, headers, body):
        phase = classify(body)
        identity = next((headers[h] for h in SESSION_HEADERS if headers.get(h)), None)
        check(identity is not None, "opencode_summary_missing_session")
        with self.lock:
            self.ingress.append((phase, identity))
            check(len(self.ingress) <= len(ORDER), "opencode_extra_client_request")
            check([p for p, _ in self.ingress] == ORDER[:len(self.ingress)], "opencode_request_order")

    def upstream(self, headers, body):
        phase = classify(body)
        check(headers["authorization"] == "Bearer synthetic-client-primary", "opencode_summary_credential")
        if phase in ("initial", "feedback"):
            assert_history(body, 1 if phase == "initial" else 2, False, self.path)
        elif phase in ("summary", "nudge"):
            assert_summary(body, self.path, nudged=phase == "nudge")
        else:
            assert_rebuilt(body, phase, self.path)
        with self.lock:
            self.attempts.append(phase)
            check(len(self.attempts) <= len(ORDER), "opencode_extra_attempt")
            check(self.attempts == ORDER[:len(self.attempts)], "opencode_attempt_order")
        if phase == "summary":
            if self.summary is None:
                self.summary = copy.deepcopy(body)
                return 503, b"", "application/json"
            check(body == self.summary, "opencode_retry_changed_summary")
            answer = "synthetic summary without template headings"
        elif phase == "nudge":
            original = copy.deepcopy(body)
            original["messages"].pop()
            check(original == self.summary, "opencode_nudge_changed_summary")
            answer = SUMMARY
        else:
            answer = TEXT
        return 200, chat_events(1 if phase in ("initial", "rebuilt") else 2, self.path, answer), "text/event-stream"

    def finish(self, identity):
        check(not self.errors, "opencode_summary_http_failure:" + ",".join(self.errors))
        check(self.attempts == ORDER and len(self.ingress) == len(ORDER), "opencode_summary_counts")
        check(all(session == identity for _, session in self.ingress), "opencode_summary_session_changed")


def api(binary, env, work, origin, operation, identity, data=None):
    args = [binary, "api", "--server", origin, operation, "--param", f"sessionID={identity}"]
    if data is not None:
        args += ["--data", json.dumps(data)]
    with Child(args, env, work) as child:
        return child.wait(timeout=20)


def wait_for_checkpoint(database, identity):
    # The installed 2.0.32 API has no session.wait operation. Observe only the
    # test-owned read-only store, never issue additional Generation requests.
    deadline = time.monotonic() + 20
    with closing(sqlite3.connect(f"file:{database}?mode=ro", uri=True)) as connection:
        while time.monotonic() < deadline:
            rows = connection.execute(
                "SELECT data FROM session_message WHERE session_id=? AND type='compaction'", (identity,)).fetchall()
            check(len(rows) <= 1, "opencode_extra_compaction")
            if rows:
                record = json.loads(rows[0][0])
                check(record.get("status") != "failed", "opencode_compaction_failed")
                if record.get("status") == "completed":
                    check(record.get("summary") == SUMMARY, "opencode_checkpoint_summary")
                    return
            time.sleep(0.1)
    raise RuntimeError("opencode_compaction_deadline")


def main():
    case, directory, gateway_binary, client_binary = sys.argv[1:]
    root = Path(directory)
    work = root / "work"
    work.mkdir(mode=0o700)
    path = work / "sample.txt"
    path.write_text(TEXT + "\n", encoding="utf-8")
    env = environment(root)
    with Child([client_binary, "--version"], env, work) as child:
        version = child.wait(timeout=5).decode().strip()
    check_client_version(case, version)
    state = State(path)
    with ExitStack() as stack:
        upstream = stack.enter_context(server(state, False))
        private_json(root / "auth.json", {"version": 1, "agents": [{"agent_id": "fixture-agent", "key": KEY}]})
        private_json(root / "gateway.json", {"auth_file": "auth.json", "bind": "127.0.0.1:0",
            "models_file": "models.json", "exchange_timeout_ms": 10000, "diagnostics": "diagnostics.jsonl"})
        private_json(root / "models.json", {"models": {"qwen3.8-flash": {"max_attempts": 1,
            "chat_profile": "readable_reasoning",
            "total_timeout_ms": 8000, "candidates": [{"binding": "aliyun-tokenplan-cn-chat",
                "base_url": upstream + "/v1", "attempt_timeout_ms": 3000}]}}})
        gateway = stack.enter_context(Child([gateway_binary, "--config", str(root / "gateway.json"),
            "--credentials-dir", str(root / "credentials")], env, work))
        try:
            ready = gateway.first_line.get(timeout=5).decode()
        except queue.Empty:
            raise RuntimeError("gateway_readiness_deadline") from None
        check(len(ready) <= 127 and ready.startswith("MorphieCore listening on http://127.0.0.1:"), "gateway_startup")
        state.gateway = ready.removeprefix("MorphieCore listening on ")
        ingress = stack.enter_context(server(state, True))
        config = opencode_configuration(root, ingress, {"keep": 0})
        env.update(OPENCODE_CONFIG=str(config), OPENCODE_DISABLE_AUTOUPDATE="1",
                   OPENCODE_DISABLE_MODELS_FETCH="1", OPENCODE_DISABLE_PROJECT_CONFIG="1",
                   OPENCODE_SERVER_PASSWORD="synthetic-summary-server-password")
        client_server = stack.enter_context(Child([client_binary, "serve", "--hostname", "127.0.0.1",
            "--port", "0"], env, work))
        try:
            listening = client_server.first_line.get(timeout=10).decode()
        except queue.Empty:
            raise RuntimeError("opencode_readiness_deadline") from None
        check(len(listening) <= 127 and listening.startswith("server listening on http://127.0.0.1:"),
              "opencode_server_startup")
        origin = listening.removeprefix("server listening on ")
        try:
            identity, events = opencode_prompt(client_binary, env, work,
                f"{READ}: Read {path}, then answer.", "summary", origin=origin)
            check(sum(e.get("type") == "tool_use" for e in events) == 1, "opencode_initial_read_count")
            databases = list(Path(env["XDG_DATA_HOME"]).rglob("opencode.db"))
            check(len(databases) == 1, "opencode_database_count")
            api(client_binary, env, work, origin, "session.compact", identity, {})
            wait_for_checkpoint(databases[0], identity)
            resumed, events = opencode_prompt(client_binary, env, work,
                f"{AFTER}: Read {path} again, then answer.", "summary", identity, origin)
            check(resumed == identity and sum(e.get("type") == "tool_use" for e in events) == 1,
                  "opencode_rebuilt_read_count")
            client_server.process.send_signal(signal.SIGINT)
            client_server.wait(timeout=5, expected_status=130)
            gateway.process.send_signal(signal.SIGINT)
            gateway.wait(timeout=5)
            with closing(sqlite3.connect(f"file:{databases[0]}?mode=ro", uri=True)) as database:
                check_opencode_session(database, identity, ["tool-calls", "stop", "tool-calls", "stop"])
                rows = database.execute("SELECT data FROM session_message WHERE session_id=? AND type='compaction'",
                                        (identity,)).fetchall()
                check(len(rows) == 1 and json.loads(rows[0][0]).get("summary") == SUMMARY,
                      "opencode_saved_summary")
            records = [json.loads(line) for line in (root / "diagnostics.jsonl").read_text().splitlines()]
            check(all(r.get("event") in ("file", "request", "log", "span") for r in records), "opencode_summary_logging_loss")
            requests = [r for r in records if r.get("event") == "request"]
            check(len(requests) == len(ORDER) and [r["outcome"] for r in requests] ==
                  ["error" if i == 2 else "complete" for i in range(len(ORDER))]
                  and all(len(r["candidates"]) == 1 and r.get("ignored_request_field_count", 0) == 0
                          for r in requests), "opencode_summary_gateway_outcomes")
        except Exception:
            print(json.dumps({"errors": state.errors, "client_shapes": [p for p, _ in state.ingress],
                              "upstream_shapes": state.attempts}), file=sys.stderr)
            raise
    state.finish(identity)
    print(json.dumps({"case": case, "version": version, "client_requests": len(ORDER),
                      "upstream_attempts": len(ORDER), "summary_retries": 1, "summary_nudges": 1,
                      "rebuilt_tool_history": True}))


if __name__ == "__main__":
    main()
