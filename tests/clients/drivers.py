"""Isolated installed-client drivers; no host config, credential wrappers or Provider calls."""
from contextlib import closing
import json
import os
from pathlib import Path
import queue
import re
import sqlite3
import subprocess
import threading
import time

KEY = "synthetic-installed-client-ingress-000001"
READ = "synthetic-client-read"
NEXT = "synthetic-client-next"
PROBE = "synthetic-client-recover"
TEXT = "cedar-71"


def check(condition, code):
    if not condition:
        raise RuntimeError(code)


def check_client_version(case, version):
    prefix, major = ("(?:opencode v)?", "2") if case == "opencode-chat" else ("", "1")
    check(re.fullmatch(prefix + major + r"\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?", version)
          is not None, "unaccepted_client_major_version")


def private_json(path, value):
    with open(path, "x", encoding="utf-8", opener=lambda p, f: os.open(p, f, 0o600)) as file:
        json.dump(value, file)


class Child:
    """Continuously drain both pipes, with bounded output and an owned lifetime."""
    def __init__(self, args, env, cwd, rpc=False):
        self.process = subprocess.Popen(args, cwd=cwd, env=env,
            stdin=subprocess.PIPE if rpc else subprocess.DEVNULL,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.output = bytearray()
        self.errors = bytearray()
        self.records = queue.Queue(maxsize=4096)
        self.first_line = queue.Queue(maxsize=1)
        self.failure = None
        self.threads = [
            threading.Thread(target=self.drain, args=(self.process.stdout, self.output, rpc), daemon=True),
            threading.Thread(target=self.drain, args=(self.process.stderr, self.errors, False), daemon=True),
        ]
        for thread in self.threads:
            thread.start()

    def drain(self, pipe, buffer, rpc):
        line_sent = False
        try:
            while True:
                value = pipe.readline(256 * 1024 + 1) if rpc else pipe.read1(4096)
                if not value:
                    return
                check(len(buffer) + len(value) <= 4 * 1024 * 1024, "client_output_limit")
                buffer.extend(value)
                if buffer is self.output and not rpc and not line_sent and b"\n" in buffer:
                    self.first_line.put_nowait(bytes(buffer).split(b"\n", 1)[0])
                    line_sent = True
                if rpc:
                    check(value.endswith(b"\n"), "rpc_record_limit")
                    self.records.put_nowait(json.loads(value))
        except Exception:
            self.failure = "child_pipe_or_record_failure"
            self.process.kill()
        finally:
            pipe.close()

    def record(self, deadline):
        check(self.failure is None, "child_pipe_failure")
        remaining = deadline - time.monotonic()
        check(remaining > 0, "rpc_deadline")
        try:
            return self.records.get(timeout=min(remaining, 15))
        except queue.Empty:
            raise RuntimeError("rpc_no_record") from None

    def wait(self, timeout=30, expected_status=0):
        try:
            status = self.process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            raise RuntimeError("client_deadline") from None
        for thread in self.threads:
            thread.join(timeout=2)
            check(not thread.is_alive(), "child_pipe_not_released")
        check(self.failure is None, "child_pipe_failure")
        # Never echo raw client diagnostics or request payloads.
        if status != expected_status:
            raise RuntimeError(f"client_exit_{status}")
        return bytes(self.output)

    def close(self):
        if self.process.stdin and not self.process.stdin.closed:
            self.process.stdin.close()
        try:
            self.process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(timeout=3)
        for thread in self.threads:
            thread.join(timeout=2)

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()


class Pi(Child):
    def __init__(self, binary, env, work, model):
        super().__init__([binary, "--mode", "rpc", "--offline", "--no-session",
            "--no-extensions", "--no-skills", "--no-context-files", "--no-mcp",
            "--no-prompt-templates", "--no-themes", "--tools", "read",
            "--provider", "recovery", "--model", model, "--thinking", "low"],
            env, work, rpc=True)
        self.serial = 0

    def command(self, kind, **fields):
        self.serial += 1
        identity = str(self.serial)
        self.process.stdin.write(json.dumps({"id": identity, "type": kind, **fields}).encode() + b"\n")
        self.process.stdin.flush()
        deadline = time.monotonic() + 20
        events = []
        acknowledged = False
        settled = kind != "prompt"
        data = None
        while not acknowledged or not settled:
            event = self.record(deadline)
            if event.get("type") == "response" and event.get("id") == identity:
                check(event.get("success") is True, "rpc_command_rejected")
                data = event.get("data")
                acknowledged = True
            else:
                events.append(event)
            if kind == "prompt" and event.get("type") == "agent_settled":
                check(event.get("aborted") is False, "pi_aborted")
                settled = True
        return data, events

    def prompt(self, message, expected):
        _, events = self.command("prompt", message=message)
        check(not any(e["type"].startswith(("auto_retry", "compaction", "summarization_retry"))
                      for e in events), "pi_automatic_retry_or_compaction")
        finished = [e["message"] for e in events if e.get("type") == "message_end"
                    and e.get("message", {}).get("role") == "assistant"]
        check([m["stopReason"] for m in finished] == expected, "pi_assistant_termination")
        tools = [e for e in events if e.get("type") == "tool_execution_end"]
        check(len(tools) == int("toolUse" in expected), "pi_tool_count")
        if tools:
            check(tools[0]["toolName"] == "read" and tools[0]["isError"] is False,
                  "pi_read_outcome")
            check(TEXT in json.dumps(tools[0]["result"]), "pi_read_result")
        check(any(b.get("type") == "text" and b.get("text") == TEXT
                  for b in finished[-1]["content"]), "pi_final_answer")


def environment(root):
    env = {"PATH": os.environ.get("PATH", ""), "HOME": str(root), "INGRESS_KEY": KEY,
           "NO_COLOR": "1", "TERM": "dumb"}
    for name in ["XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME", "XDG_STATE_HOME"]:
        path = root / name.lower()
        path.mkdir(mode=0o700)
        env[name] = str(path)
    return env


def pi_configuration(root, origin, native, compaction=None, retry=None):
    directory = root / "pi-agent"
    directory.mkdir(mode=0o700)
    compat = ({"supportsMidConvoEffort": True, "supportsMidConvoSystemMessages": True,
        "supportsMidConvoToolChanges": True, "forceAdaptiveThinking": True,
        "supportsStrictTools": True, "supportsTemperature": False,
        "sendSessionAffinityHeaders": True} if native else {
        "supportsDeveloperRole": False, "supportsStrictMode": False, "supportsStore": False,
        "supportsReasoningEffort": True, "thinkingFormat": "openai",
        "maxTokensField": "max_completion_tokens", "sendSessionAffinityHeaders": True,
        "sessionAffinityFormat": "openai"})
    model = "claude-haiku-5-5" if native else "qwen3.8-flash"
    private_json(directory / "models.json", {"providers": {"recovery": {
        "baseUrl": origin if native else origin + "/v1",
        "api": "anthropic-messages" if native else "openai-completions", "apiKey": "$INGRESS_KEY",
        "models": [{"id": model, "reasoning": True, "contextWindow": 128000,
                    "maxTokens": 2048, "compat": compat}]}}})
    private_json(directory / "settings.json", {"retry": {"enabled": False,
        **(retry or {}), "provider": {"maxRetries": 0, "timeoutMs": 5000}},
        "compaction": {"enabled": False, **(compaction or {})}, "cacheWarming": "off"})
    return directory, model


def run_pi(binary, env, root, work, origin, native, path):
    directory, model = pi_configuration(root, origin, native)
    env = {**env, "PI_CODING_AGENT_DIR": str(directory), "PI_CACHE_RETENTION": "short"}
    with Pi(binary, env, work, model) as original:
        state, _ = original.command("get_state")
        original.prompt(f"{READ}: Read {path}, then answer.", ["toolUse", "stop"])
        with Pi(binary, env, work, model) as probe:
            probe_state, _ = probe.command("get_state")
            check(probe_state["sessionId"] != state["sessionId"], "pi_probe_identity")
            probe.prompt(f"{PROBE}: Say {TEXT} without tools.", ["stop"])
        original.prompt(f"{NEXT}: Repeat the answer without reading again.", ["stop"])
        final, _ = original.command("get_state")
        check(final["sessionId"] == state["sessionId"] and not final["isStreaming"], "pi_session_changed")
        messages, _ = original.command("get_messages")
        check([m["role"] for m in messages["messages"]] ==
              ["system", "user", "assistant", "toolResult", "assistant", "user", "assistant"], "pi_final_history")
    return state["sessionId"], probe_state["sessionId"]


def opencode_configuration(root, origin, compaction=None):
    path = root / "opencode.json"
    private_json(path, {"update": "disable", "share": "disabled", "snapshots": False,
        "warming": False, "compaction": {"auto": False, **(compaction or {})},
        "permissions": [{"action": "*", "resource": "*", "effect": "deny"},
                        {"action": "read", "resource": "*", "effect": "allow"}],
        "providers": {"recovery": {"package": "@opencode/ai/providers/openai-compatible",
            "env": ["INGRESS_KEY"], "settings": {"baseURL": origin + "/v1"},
            "body": {"max_completion_tokens": 2048}, "models": {"qwen3.8-flash": {
                "limit": {"context": 128000, "output": 2048},
                "capabilities": {"tools": True, "input": ["text"], "output": ["text"]},
                "variants": [{"id": "low", "body": {"reasoning_effort": "low"}}]}}}}})
    return path


def check_opencode_output(events):
    answer = "".join(e["part"]["text"] for e in events if e.get("type") == "text")
    check(answer == TEXT, "opencode_final_answer")


def check_opencode_session(database, identity, expected):
    rows = database.execute("SELECT data FROM session_message WHERE session_id=? AND type='assistant' ORDER BY time_created, id", (identity,)).fetchall()
    check([json.loads(row[0]).get("finish") for row in rows] == expected, "opencode_assistant_termination")
    idle = database.execute("SELECT idle_outcome FROM session_v2 WHERE id=?", (identity,)).fetchone()
    check(idle == ("succeeded",), "opencode_idle_outcome")


def opencode_prompt(binary, env, work, prompt, title, session=None, origin=None):
    args = [binary, "run", *(["--server", origin] if origin else ["--standalone"]),
            "--format", "json", "--title", title, "-m", "recovery/qwen3.8-flash#low"]
    if session:
        args.extend(["--session", session])
    with Child([*args, prompt], env, work) as child:
        output = child.wait()
    events = [json.loads(line) for line in output.split(b"\n") if line]
    identities = {e["sessionID"] for e in events if "sessionID" in e}
    check(len(identities) == 1, "opencode_session_event_identity")
    check(not any(e.get("type") == "error" for e in events), "opencode_error_event")
    check_opencode_output(events)
    return identities.pop(), events


def run_opencode(binary, env, root, work, origin, path):
    config = opencode_configuration(root, origin)
    env = {**env, "OPENCODE_CONFIG": str(config), "OPENCODE_DISABLE_AUTOUPDATE": "1",
           "OPENCODE_DISABLE_MODELS_FETCH": "1", "OPENCODE_DISABLE_PROJECT_CONFIG": "1"}

    def run(prompt, title, session=None):
        return opencode_prompt(binary, env, work, prompt, title, session)

    original, events = run(f"{READ}: Read {path}, then answer.", "recovery-a")
    tools = [e for e in events if e.get("type") == "tool_use"]
    check(len(tools) == 1, "opencode_tool_count")
    part = tools[0]["part"]
    check(part["tool"] == "read" and part["state"]["status"] == "completed", "opencode_read_outcome")
    check(TEXT in part["state"]["output"], "opencode_read_result")
    probe, _ = run(f"{PROBE}: Say {TEXT} without tools.", "recovery-b")
    check(probe != original, "opencode_probe_identity")
    resumed, _ = run(f"{NEXT}: Repeat the answer without reading again.", "recovery-a", original)
    check(resumed == original, "opencode_resumed_wrong_session")
    databases = list(Path(env["XDG_DATA_HOME"]).rglob("opencode.db"))
    check(len(databases) == 1, "opencode_database_count")
    with closing(sqlite3.connect(f"file:{databases[0]}?mode=ro", uri=True)) as database:
        check_opencode_session(database, original, ["tool-calls", "stop", "stop"])
        check_opencode_session(database, probe, ["stop"])
    return original, probe
