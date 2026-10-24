"""Ordinary pi summarization through the real gateway, without Provider calls."""
from contextlib import ExitStack
import copy
import json
from pathlib import Path
import queue
import signal
import sys

from drivers import (Child, KEY, Pi, READ, TEXT, check, check_client_version,
                     environment, pi_configuration, private_json)
from recovery import (CALL, PLAN, REDACTED, SIGNATURE, State as RecoveryState,
                      assert_history, assert_profile, chat_events, native_events, server, text)

FOCUS = "synthetic-summary-focus"
AFTER = "synthetic-summary-next-read"
RECENT = "synthetic-summary-retained-turn"
RECENT_PROMPT = f"{RECENT}: Say {TEXT} without tools.\n" + "x" * 4096
SUMMARY = "## Goal\nPreserve the synthetic result cedar-71.\n## Next Steps\nRead the sample again."
PREFIX_SUMMARY = "## Original Request\nRemember the recent synthetic user turn.\n## Progress So Far\nNo tools used."
ORDER = ["initial", "feedback", "recent", "summary", "rebuilt", "rebuilt-feedback"]
SESSION_HEADERS = ["x-morphiecore-conversation-id", "x-session-id",
                   "session_id", "session-id", "x-session-affinity"]


def has_cache_marker(value):
    if isinstance(value, list):
        return any(has_cache_marker(child) for child in value)
    if isinstance(value, dict):
        return "cache_control" in value or any(has_cache_marker(child) for child in value.values())
    return False


def classify(body):
    messages = body["messages"]
    users = [m for m in messages if m["role"] == "user"]
    last = text(users[-1]["content"])
    if "<conversation>" in last and FOCUS in last:
        return "summary"
    if "# Conversation\n" in last and RECENT in last and "## Original Request" in last:
        return "prefix-summary"
    rebuilt = SUMMARY in text(messages)
    if not rebuilt and RECENT in last:
        return "recent"
    results = any(m["role"] == "tool" or any(b.get("type") == "tool_result"
                  for b in (m.get("content") or []) if isinstance(b, dict)) for m in messages)
    if results:
        return "rebuilt-feedback" if rebuilt else "feedback"
    check((AFTER if rebuilt else READ) in last, "unclassified_summary_request")
    return "rebuilt" if rebuilt else "initial"


def assert_summary(body, native, path, prefix=False):
    messages = body["messages"]
    history = [m for m in messages if m["role"] != "system"]
    check([m["role"] for m in history] == ["user"], "summary_not_standalone")
    check(not body.get("tools"), "summary_declares_tools")
    prompt = text(history[0]["content"])
    if prefix:
        check("# Conversation\n" in prompt and "\n# Instructions\n" in prompt,
              "prefix_summary_envelope")
        conversation = prompt.split("# Conversation\n", 1)[1].split("\n# Instructions\n", 1)[0]
        check(conversation.strip() == f"[User]: {RECENT_PROMPT}"
              and FOCUS not in conversation and prompt.count(FOCUS) == 1
              and f"Additional focus: {FOCUS}" in prompt
              and all(value not in prompt for value in [READ, str(path), "[Assistant]:"]),
              "prefix_summary_serialized_exchange")
    else:
        check(all(value in prompt for value in [
            "<conversation>", "</conversation>", FOCUS, READ, str(path), "read", TEXT,
        ]), "summary_missing_serialized_history")
        conversation = prompt.split("<conversation>", 1)[1].split("</conversation>", 1)[0]
        markers = ["[User]:", "[Assistant tool calls]:", "[Tool result]:", "[Assistant]:"]
        check(all(conversation.count(marker) == 1 for marker in markers)
              and [conversation.index(marker) for marker in markers] ==
              sorted(conversation.index(marker) for marker in markers)
              and f"read(path={json.dumps(str(path))})" in conversation
              and f"[Assistant]: {TEXT}" in conversation
              and TEXT in conversation.split("[Tool result]:", 1)[1].split("[Assistant]:", 1)[0]
              and RECENT not in conversation, "summary_serialized_exchange")
    check(all(value not in prompt for value in [PLAN, SIGNATURE, REDACTED]),
          "summary_serialized_reasoning")
    check(body["model"] == ("claude-haiku-5-5" if native else "qwen3.8-flash"), "summary_model")
    check(body["stream"] is True, "summary_not_streaming")
    check(body["max_tokens" if native else "max_completion_tokens"] == 2048, "summary_output_cap")
    if native:
        check(body["thinking"] == {"type": "adaptive", "display": "summarized",
                                  "block_binding": {"prefix_mismatch_behavior": "drop_block"}},
              "summary_thinking")
        check(body["output_config"] == {"effort": "high"}, "summary_root_effort")
        controls = [m for m in messages if m["role"] == "system"]
        check(bool(controls) and all(m["content"] == [] and m["output_config"] == {"effort": "low"}
                                    for m in controls), "summary_ordered_effort")
        # No caller markers are not an instruction to disable the trusted binding default.
        cache = body.get("cache_control", {})
        check(cache.get("type") == "ephemeral" and cache.get("ttl", "5m") == "5m",
              "summary_trusted_cache_default")
    else:
        check(body["reasoning_effort"] == "low", "summary_chat_effort")


def assert_rebuilt(body, phase, native, path, split=False):
    # The same declaration/control profile must survive context replacement.
    assert_profile(body, native)
    if native:
        controls = [m for m in body["messages"] if m["role"] == "system"]
        check(bool(controls) and all(m["content"] == [] and m["output_config"] == {"effort": "low"}
                                    for m in controls), "rebuilt_ordered_effort")
    check(SUMMARY in text(body["messages"]) and AFTER in text(body["messages"]), "rebuilt_summary_missing")
    check(READ not in text(body["messages"]), "pre_summary_prompt_retained")
    history = [m for m in body["messages"] if m["role"] != "system"]
    first_assistant = next((i for i, m in enumerate(history) if m["role"] == "assistant"), -1)
    prefix = history[:first_assistant]
    check(first_assistant > 0 and all(m["role"] == "user" for m in prefix)
          and SUMMARY in text(prefix), "rebuilt_retained_prefix")
    if split:
        check(PREFIX_SUMMARY in text(prefix) and RECENT not in text(prefix)
              and "**Turn Context (split turn):**" in text(prefix)
              and text(prefix).index(SUMMARY) < text(prefix).index(PREFIX_SUMMARY),
              "rebuilt_split_prefix")
    else:
        check(RECENT_PROMPT in text(prefix)
              and text(prefix).index(SUMMARY) < text(prefix).index(RECENT), "rebuilt_retained_prefix")
    roles = ["assistant", "user"]
    if phase == "rebuilt-feedback":
        roles += ["assistant", "user" if native else "tool"]
    check([m["role"] for m in history[first_assistant:]] == roles
          and AFTER in text(history[first_assistant + 1]["content"]), "rebuilt_history_order")
    assistants = [m for m in body["messages"] if m["role"] == "assistant"]
    retained = assistants.pop(0)
    check(retained["content"] == ([{"type": "text", "text": TEXT}] if native else TEXT),
          "rebuilt_retained_answer")
    if phase == "rebuilt":
        check(not assistants, "pre_summary_assistant_retained")
        return
    check(len(assistants) == 1, "rebuilt_assistant_count")
    if native:
        check(assistants[0]["content"] == [
            {"type": "thinking", "thinking": PLAN, "signature": SIGNATURE},
            {"type": "redacted_thinking", "data": REDACTED},
            {"type": "tool_use", "id": CALL, "name": "read", "input": {"path": str(path)}},
        ], "rebuilt_signed_tool_history")
        results = [b for m in body["messages"] if m["role"] == "user"
                   for b in m["content"] if isinstance(b, dict) and b.get("type") == "tool_result"]
        check(len(results) == 1 and results[0]["tool_use_id"] == CALL
              and not results[0].get("is_error", False) and TEXT in text(results[0]["content"]),
              "rebuilt_native_tool_result")
    else:
        assistant = assistants[0]
        check(assistant["reasoning_content"] == PLAN, "rebuilt_readable_reasoning")
        calls = assistant["tool_calls"]
        check(len(calls) == 1 and calls[0]["id"] == CALL and calls[0]["function"]["name"] == "read"
              and json.loads(calls[0]["function"]["arguments"]) == {"path": str(path)},
              "rebuilt_chat_tool_call")
        results = [m for m in body["messages"] if m["role"] == "tool"]
        check(len(results) == 1 and results[0]["tool_call_id"] == CALL
              and TEXT in text(results[0]["content"]), "rebuilt_chat_tool_result")


class State(RecoveryState):
    def __init__(self, native, path, scenario="ordinary"):
        super().__init__(native, path)
        check(scenario in ("ordinary", "split", "retry"), "unknown_summary_scenario")
        self.scenario = scenario
        self.order = ORDER.copy()
        if scenario == "split":
            self.order.insert(4, "prefix-summary")
        elif scenario == "retry":
            self.order.insert(3, "summary")
        self.first_summary = None

    def incoming(self, headers, body):
        phase = classify(body)
        identity = next((headers[name] for name in SESSION_HEADERS if headers.get(name)), None)
        if phase in ("summary", "prefix-summary"):
            check(identity is None and not has_cache_marker(body), "summary_caller_cache_or_affinity")
        else:
            check(identity is not None, "missing_client_session_carrier")
        with self.lock:
            self.ingress.append((phase, identity))
            check(len(self.ingress) <= len(self.order), "extra_client_request")
            check([p for p, _ in self.ingress] == self.order[:len(self.ingress)], "summary_request_order")

    def upstream(self, headers, body):
        phase = classify(body)
        secret = headers["x-api-key"] if self.native else headers["authorization"].removeprefix("Bearer ")
        check(secret == "synthetic-client-primary", "summary_unexpected_credential")
        check(not any(headers.get(name) for name in SESSION_HEADERS), "summary_ingress_header_forwarded")
        if phase in ("summary", "prefix-summary"):
            assert_summary(body, self.native, self.path, prefix=phase == "prefix-summary")
        elif phase.startswith("rebuilt"):
            assert_rebuilt(body, phase, self.native, self.path, split=self.scenario == "split")
        else:
            assert_history(body, {"initial": 1, "feedback": 2, "recent": 3}[phase], self.native, self.path)
        with self.lock:
            self.attempts.append((phase, headers.get("x-opencode-session")))
            check(len(self.attempts) <= len(self.order), "extra_upstream_attempt")
            check([p for p, _ in self.attempts] == self.order[:len(self.attempts)], "summary_attempt_order")
        if phase == "summary" and self.scenario == "retry":
            if self.first_summary is None:
                self.first_summary = copy.deepcopy(body)
                return 503, b"", "application/json"
            check(body == self.first_summary, "summary_retry_changed_request")
        answer = SUMMARY if phase == "summary" else PREFIX_SUMMARY if phase == "prefix-summary" else TEXT
        payload = (native_events if self.native else chat_events)(
            1 if phase in ("initial", "rebuilt") else 2, self.path, answer)
        return 200, payload, "text/event-stream"

    def finish(self, identity):
        check(not self.errors, "summary_http_check_failed:" + ",".join(self.errors))
        check(len(self.ingress) == len(self.attempts) == len(self.order), "summary_exchange_counts")
        check(all(session == identity for phase, session in self.ingress
                  if phase not in ("summary", "prefix-summary")),
              "summary_client_session_changed")
        if self.native:
            ordinary = [session for phase, session in self.attempts
                        if phase not in ("summary", "prefix-summary")]
            check(bool(ordinary[0]) and len(set(ordinary)) == 1, "summary_native_session_changed")
            check(all(bool(session) and session != ordinary[0] for phase, session in self.attempts
                      if phase in ("summary", "prefix-summary")),
                  "summary_native_affinity_not_independent")


def main():
    case, directory, gateway_binary, client_binary, *options = sys.argv[1:]
    scenario = options[0] if options else "ordinary"
    root = Path(directory)
    native = case == "pi-messages"
    work = root / "work"
    work.mkdir(mode=0o700)
    path = work / "sample.txt"
    path.write_text(TEXT + "\n", encoding="utf-8")
    env = environment(root)
    with Child([client_binary, "--version"], env, work) as child:
        version = child.wait(timeout=5).decode().strip()
    check_client_version(case, version)
    state = State(native, path, scenario)
    with ExitStack() as stack:
        origin = stack.enter_context(server(state, False))
        model = "claude-haiku-5-5" if native else "qwen3.8-flash"
        binding = "opencode-go-haiku-messages" if native else "aliyun-tokenplan-cn-chat"
        private_json(root / "gateway.json", {"client_key": KEY, "bind": "127.0.0.1:0",
            "models_file": "models.json", "exchange_timeout_ms": 10000, "diagnostics": "diagnostics.jsonl"})
        private_json(root / "models.json", {"models": {model: {"max_attempts": 1,
            "chat_profile": "standard" if native else "readable_reasoning",
            "total_timeout_ms": 8000, "candidates": [{"binding": binding,
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
        agent, model = pi_configuration(root, ingress, native,
            {"keepRecentTokens": 0 if scenario == "split" else 512},
            {"enabled": True, "maxRetries": 1, "baseDelayMs": 1} if scenario == "retry" else None)
        env = {**env, "PI_CODING_AGENT_DIR": str(agent), "PI_CACHE_RETENTION": "short"}
        try:
            with Pi(client_binary, env, work, model) as client:
                initial, _ = client.command("get_state")
                client.prompt(f"{READ}: Read {path}, then answer.", ["toolUse", "stop"])
                client.prompt(RECENT_PROMPT, ["stop"])
                result, events = client.command("compact", customInstructions=FOCUS)
                check(result["summary"].startswith(SUMMARY), "summary_rpc_result")
                retry_events = [e for e in events if e["type"].startswith("summarization_retry")]
                check(not any(e["type"].startswith("auto_retry") for e in events),
                      "summary_unexpected_agent_retry")
                if scenario == "retry":
                    check([e["type"] for e in retry_events] ==
                          ["summarization_retry_scheduled", "summarization_retry_attempt_start",
                           "summarization_retry_finished"],
                          "summary_retry_events")
                    check(retry_events[0]["attempt"] == retry_events[0]["maxAttempts"] == 1,
                          "summary_retry_budget")
                else:
                    check(not retry_events, "summary_unexpected_retry")
                check((PREFIX_SUMMARY in result["summary"]) == (scenario == "split"),
                      "summary_split_result")
                check(any(e["type"] == "compaction_end" and not e.get("aborted")
                          and e.get("result", {}).get("summary", "").startswith(SUMMARY)
                          for e in events), "summary_no_completion")
                messages, _ = client.command("get_messages")
                check([m["role"] for m in messages["messages"]] ==
                      ["system", "compactionSummary", *(["user"] if scenario != "split" else []), "assistant"],
                      "summary_projected_history")
                client.prompt(f"{AFTER}: Read {path} again, then answer.", ["toolUse", "stop"])
                messages, _ = client.command("get_messages")
                check([m["role"] for m in messages["messages"]] ==
                      ["system", "compactionSummary", *(["user"] if scenario != "split" else []), "assistant",
                       "user", "assistant", "toolResult", "assistant"],
                      "summary_rebuilt_client_history")
                final, _ = client.command("get_state")
                check(final["sessionId"] == initial["sessionId"] and not final["isStreaming"]
                      and not final["isCompacting"], "summary_client_not_idle")
                client.process.stdin.close()
                client.wait(timeout=5)
            gateway.process.send_signal(signal.SIGINT)
            gateway.wait(timeout=5)
            records = [json.loads(line) for line in (root / "diagnostics.jsonl").read_text().splitlines()]
            check(all(r.get("event") in ("file", "request") for r in records), "summary_logging_loss")
            requests = [r for r in records if r.get("event") == "request"]
            check(len(requests) == len(state.order)
                  and [r["outcome"] for r in requests] ==
                  ["error" if scenario == "retry" and i == 3 else "complete"
                   for i in range(len(state.order))]
                  and all(len(r["candidates"]) == 1 and r.get("ignored_request_field_count", 0) == 0
                  for r in requests), "summary_gateway_completion")
        except Exception:
            print(json.dumps({"errors": state.errors, "ingress_shapes": [p for p, _ in state.ingress],
                              "upstream_shapes": [p for p, _ in state.attempts]}), file=sys.stderr)
            raise
    # The verdict includes late client traffic and errors from every joined HTTP worker.
    state.finish(initial["sessionId"])
    print(json.dumps({"case": case, "version": version, "scenario": scenario,
                      "client_requests": len(state.order), "upstream_attempts": len(state.order),
                      "ordinary_summaries": 2 if scenario == "split" else 1,
                      "summary_retries": int(scenario == "retry"), "rebuilt_tool_history": True}))


if __name__ == "__main__":
    main()
