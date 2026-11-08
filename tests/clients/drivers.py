"""Isolated installed-client drivers; no host config, credential wrappers or Provider calls."""
from contextlib import closing
import json
import os
from pathlib import Path
import queue
import re
import signal
import sqlite3
import subprocess
import sys
import threading
import time
from urllib.parse import urlsplit

KEY = "synthetic-installed-client-ingress-000001"
READ = "synthetic-client-read"
NEXT = "synthetic-client-next"
PROBE = "synthetic-client-recover"
TEXT = "cedar-71"


def check(condition, code):
    if not condition:
        raise RuntimeError(code)


def messages_content_equal(actual, expected):
    """Compare replay without unsigned, unbound zero-byte text shells."""
    if not isinstance(actual, list) or not isinstance(expected, list):
        return False
    empty = {"type": "text", "text": ""}
    return ([block for block in actual if block != empty]
            == [block for block in expected if block != empty])


def check_client_version(case, version):
    # The caller supplies the current host's unwrapped binary. Validate its
    # version record, not equality with an independent protocol source lock.
    prefix = "(?:opencode v)?" if case.startswith("opencode-") else ""
    check(re.fullmatch(prefix + r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?", version)
          is not None, "invalid_client_version")


def check_pi_output(content):
    check(any(block.get("type") == "text" and block.get("text") == TEXT
              for block in content), "pi_final_answer")


def private_json(path, value):
    with open(path, "x", encoding="utf-8", opener=lambda p, f: os.open(p, f, 0o600)) as file:
        json.dump(value, file)


def gateway_configuration(root, native, origin, *, credential_root=None,
                          recovery=None, accounts=("primary",),
                          total_timeout_ms=8000, exchange_timeout_ms=10000,
                          concurrency=None, model=None, provider=None, binding=None,
                          api_root="/v1"):
    """Current production-loader inputs with explicitly local synthetic accounts."""
    target = urlsplit(origin)
    check(target.scheme == "http" and target.hostname == "127.0.0.1"
          and target.port is not None and not target.path
          and not target.query and not target.fragment, "synthetic_upstream_origin")
    model = model or ("claude-haiku-5-5" if native else "qwen3.8-flash")
    provider = provider or ("opencode-go" if native else "aliyun-tokenplan-cn")
    binding = binding or ("opencode-go-haiku-messages" if native else "aliyun-tokenplan-cn-chat")
    private_json(root / "accounts.json", {"version": 1, "providers": {provider: {
        "recovery": {"recovery_timeout_ms": 0, **(recovery or {})},
        "accounts": [{"name": name, "enabled": True,
                      "credential": {"alias": name},
                      "base_url": origin + api_root} for name in accounts]}}})
    private_json(root / "auth.json", {"version": 1, "agents": [
        {"agent_id": "fixture-agent", "key": KEY}]})
    service = {"version": 1, "auth_file": "auth.json", "bind": "127.0.0.1:0",
        "accounts_file": "accounts.json", "models_file": "models.json",
        "credentials_dir": str((credential_root or root) / "credentials"),
        "delivery": {"exchange_timeout_ms": exchange_timeout_ms}, "diagnostics": "diagnostics.jsonl"}
    if concurrency is not None:
        service["resources"] = {"concurrency": concurrency}
    private_json(root / "gateway.json", service)
    private_json(root / "models.json", {"version": 1, "models": {model: {
        "chat_profile": "standard" if native else "readable_reasoning",
        "total_timeout_ms": total_timeout_ms, "candidates": [{"binding": binding}]}}})


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
            if rpc:
                # Wake the command waiter on EOF or reader failure, without an
                # idle timeout shorter than the command's actual deadline.
                try:
                    self.records.put_nowait(None)
                except queue.Full:
                    self.failure = "child_pipe_or_record_failure"

    def record(self, deadline):
        check(self.failure is None, "child_pipe_failure")
        remaining = deadline - time.monotonic()
        check(remaining > 0, "rpc_deadline")
        try:
            record = self.records.get(timeout=remaining)
        except queue.Empty:
            check(self.failure is None, "child_pipe_failure")
            raise RuntimeError("rpc_deadline") from None
        check(self.failure is None, "child_pipe_failure")
        check(record is not None, "rpc_no_record")
        return record

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


def shutdown_gateway(child, *, failed=False):
    """Keep an active scenario failure authoritative if bounded shutdown also fails."""
    try:
        child.process.send_signal(signal.SIGINT)
        child.wait(timeout=5)
    except (RuntimeError, OSError, subprocess.TimeoutExpired) as error:
        if not failed:
            raise
        code = str(error) if isinstance(error, RuntimeError) else type(error).__name__
        print(json.dumps({"gateway_cleanup_error": code}), file=sys.stderr)


class Pi(Child):
    def __init__(self, binary, env, work, model, *, provider="recovery", thinking="low", tools="read"):
        super().__init__([binary, "--mode", "rpc", "--offline", "--no-session",
            "--no-extensions", "--no-skills", "--no-context-files", "--no-mcp",
            "--no-prompt-templates", "--no-themes", "--tools", tools,
            "--provider", provider, "--model", model,
            *([] if thinking is None else ["--thinking", thinking])],
            env, work, rpc=True)
        self.serial = 0

    def submit(self, kind, **fields):
        self.serial += 1
        identity = str(self.serial)
        self.process.stdin.write(json.dumps({"id": identity, "type": kind, **fields}).encode() + b"\n")
        self.process.stdin.flush()
        return identity

    def command(self, kind, *, timeout=20, **fields):
        identity = self.submit(kind, **fields)
        deadline = time.monotonic() + timeout
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

    def prompt(self, message, expected, *, timeout=20, check_output=check_pi_output):
        _, events = self.command("prompt", message=message, timeout=timeout)
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
        check_output(finished[-1]["content"])


def environment(root):
    env = {"PATH": os.environ.get("PATH", ""), "HOME": str(root), "INGRESS_KEY": KEY,
           "NO_COLOR": "1", "TERM": "dumb"}
    for name in ["XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME", "XDG_STATE_HOME"]:
        path = root / name.lower()
        path.mkdir(mode=0o700)
        env[name] = str(path)
    return env


def pi_configuration(root, origin, native, compaction=None, retry=None, provider_timeout_ms=5000,
                     model=None):
    directory = root / "pi-agent"
    directory.mkdir(mode=0o700)
    compat = ({"supportsMidConvoEffort": False, "supportsMidConvoSystemMessages": True,
        "supportsMidConvoToolChanges": False, "forceAdaptiveThinking": True,
        "supportsStrictTools": True, "supportsTemperature": False,
        "sendSessionAffinityHeaders": True} if native else {
        "supportsDeveloperRole": False, "supportsStrictMode": False, "supportsStore": False,
        "supportsReasoningEffort": True, "thinkingFormat": "openai",
        "maxTokensField": "max_completion_tokens", "sendSessionAffinityHeaders": True,
        "sessionAffinityFormat": "openai"})
    model = model or ("claude-haiku-5-5" if native else "qwen3.8-flash")
    private_json(directory / "models.json", {"providers": {"recovery": {
        "baseUrl": origin if native else origin + "/v1",
        "api": "anthropic-messages" if native else "openai-completions", "apiKey": "$INGRESS_KEY",
        "models": [{"id": model, "reasoning": True, "contextWindow": 128000,
                    "maxTokens": 2048, "compat": compat,
                    # Explicit client opt-out; never filter beta headers in transit.
                    "headers": {"anthropic-version": "2026-10-21",
                                "anthropic-beta": ""} if native else {}}]}}})
    private_json(directory / "settings.json", {"retry": {"enabled": False,
        **(retry or {}), "provider": {"maxRetries": 0, "timeoutMs": provider_timeout_ms}},
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


def opencode_configuration(root, origin, compaction=None, *, model="qwen3.8-flash"):
    path = root / "opencode.json"
    private_json(path, {"update": "disable", "share": "disabled", "snapshots": False,
        "warming": False, "compaction": {"auto": False, **(compaction or {})},
        "permissions": [{"action": "*", "resource": "*", "effect": "deny"},
                        {"action": "read", "resource": "*", "effect": "allow"}],
        "providers": {"recovery": {"package": "@opencode/ai/providers/openai-compatible",
            "env": ["INGRESS_KEY"], "settings": {"baseURL": origin + "/v1"},
            "body": {"max_completion_tokens": 2048}, "models": {model: {
                "limit": {"context": 128000, "output": 2048},
                "capabilities": {"tools": True, "input": ["text"], "output": ["text"]},
                "variants": [{"id": "low", "body": {"reasoning_effort": "low"}}]}}}}})
    return path


def check_opencode_output(events):
    answer = "".join(e["part"]["text"] for e in events if e.get("type") == "text")
    check(answer == TEXT, "opencode_final_answer")


def check_opencode_session(database, identity, expected, *, idle_outcome="succeeded"):
    rows = database.execute("SELECT data FROM session_message WHERE session_id=? AND type='assistant' ORDER BY time_created, id", (identity,)).fetchall()
    check([json.loads(row[0]).get("finish") for row in rows] == expected, "opencode_assistant_termination")
    idle = database.execute("SELECT idle_outcome FROM session_v2 WHERE id=?", (identity,)).fetchone()
    check(idle == (idle_outcome,), "opencode_idle_outcome")


def opencode_prompt(binary, env, work, prompt, title, session=None, origin=None, timeout=30,
                    check_output=check_opencode_output, model="recovery/qwen3.8-flash#low",
                    expected_status=0):
    args = [binary, "run", *(["--server", origin] if origin else ["--standalone"]),
            "--format", "json", "--title", title, "-m", model]
    if session:
        args.extend(["--session", session])
    with Child([*args, prompt], env, work) as child:
        output = child.wait(timeout=timeout, expected_status=expected_status)
    events = [json.loads(line) for line in output.split(b"\n") if line]
    identities = {e["sessionID"] for e in events if "sessionID" in e}
    check(len(identities) == 1, "opencode_session_event_identity")
    errors = [event for event in events if event.get("type") == "error"]
    check(not errors if expected_status == 0 else bool(errors), "opencode_error_event")
    check_output(events)
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


def opencode_final_output(events):
    # A real tool turn can publish a pre-tool text block. The last text is the
    # final answer; verify its whole delivered message separately in State.
    texts = [event["part"]["text"] for event in events if event.get("type") == "text"]
    check(bool(texts) and texts[-1].strip() == TEXT, "live_client_final_answer")


def pi_final_output(content):
    answer = "".join(block["text"] for block in content if block.get("type") == "text")
    check(answer.strip() == TEXT, "live_client_final_answer")


def run_reasoning_client(case, binary, env, root, work, origin, path, *, model="qwen3.8-flash"):
    first = f"Read {path} exactly once using read. Then reply with only the exact file contents, without quotes or markup."
    next_turn = "Repeat the exact file contents from our conversation. Do not use any tool. Reply with only those contents."
    if case == "pi-chat":
        directory, model = pi_configuration(root, origin, False, provider_timeout_ms=120000, model=model)
        env = {**env, "PI_CODING_AGENT_DIR": str(directory), "PI_CACHE_RETENTION": "short"}
        with Pi(binary, env, work, model) as client:
            client.prompt(first, ["toolUse", "stop"], timeout=250, check_output=pi_final_output)
            client.prompt(next_turn, ["stop"], timeout=130, check_output=pi_final_output)
    else:
        configuration = opencode_configuration(root, origin, model=model)
        env = {**env, "OPENCODE_CONFIG": str(configuration), "OPENCODE_DISABLE_AUTOUPDATE": "1",
               "OPENCODE_DISABLE_MODELS_FETCH": "1", "OPENCODE_DISABLE_PROJECT_CONFIG": "1"}
        identity, events = opencode_prompt(binary, env, work, first, "reasoning-acceptance", timeout=250,
                                          check_output=opencode_final_output, model=f"recovery/{model}#low")
        tools = [event for event in events if event.get("type") == "tool_use"]
        check(len(tools) == 1 and tools[0]["part"]["tool"] == "read"
              and tools[0]["part"]["state"]["status"] == "completed", "live_client_tool_execution")
        resumed, _ = opencode_prompt(binary, env, work, next_turn, "reasoning-acceptance", identity, timeout=130,
                                    check_output=opencode_final_output, model=f"recovery/{model}#low")
        check(resumed == identity, "live_client_session_changed")
        databases = list(Path(env["XDG_DATA_HOME"]).rglob("opencode.db"))
        check(len(databases) == 1, "opencode_database_count")
        with closing(sqlite3.connect(f"file:{databases[0]}?mode=ro", uri=True)) as database:
            check_opencode_session(database, identity, ["tool-calls", "stop", "stop"])
