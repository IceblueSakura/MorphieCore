"""Installed native protocol defaults through the CLI, without reasoning/compat overrides."""
from contextlib import ExitStack, closing
import json
from pathlib import Path
import queue
import signal
import sqlite3
import sys

from drivers import (Child, NEXT, READ, TEXT, Pi, check, check_client_version,
                     check_opencode_session, environment, opencode_configuration,
                     opencode_prompt, private_json, gateway_configuration)
from recovery import (State as RecoveryState, PLAN, CALL, assert_history, chat_events,
                      native_events, server, stage, text)

MODEL = "claude-haiku-5-5"
PROVIDER = "opencode-go"
PROFILES = {
    "messages": (MODEL, PROVIDER, "opencode-go-haiku-messages", "anthropic-messages"),
    "chat": ("deepseek-flash", "deepseek", "deepseek-chat", "openai-completions"),
    "responses": ("gpt-6.1-sol", "opencode", "openai-sol-responses", "openai-responses"),
}
REJECTION_MESSAGE = "The request could not be completed (invalid_request)."


def assert_rejection_response(status, content_type, payload):
    check(status == 400 and content_type is not None
          and content_type.split(";")[0] == "application/json", "default_rejection_http")
    check(json.loads(payload) == {"error": {"type": "invalid_request_error",
          "code": "invalid_request", "message": REJECTION_MESSAGE, "param": None}},
          "default_rejection_envelope")


def assert_pi_rejection(messages, protocol):
    check(len(messages) == 1 and messages[0]["stopReason"] == "error", "pi_default_request_rejection")
    message = messages[0].get("errorMessage", "")
    prefix = "400" if protocol == "chat" else "opencode API error (400):"
    check(message.startswith(prefix) and REJECTION_MESSAGE in message, "pi_default_error_identity")


class DefaultState(RecoveryState):
    def __init__(self, native, path, *, pi):
        super().__init__(native, path)
        self.pi = pi
        self.deliveries = []

    def observe_response(self, body, status, content_type, payload):
        if status != 200:
            assert_rejection_response(status, content_type, payload)
        else:
            check(content_type is not None and content_type.split(";")[0] == "text/event-stream",
                  "default_delivery_content_type")
        with self.lock:
            self.deliveries.append(status)


def assert_default_profile(body, pi):
    check(body["model"] == MODEL and body["stream"] is True, "default_messages_model")
    thinking = body.get("thinking", {})
    check(thinking.get("type") == "adaptive", "default_adaptive_thinking")
    check(body["max_tokens"] == 2048, "default_operational_output_cap")
    read = [tool for tool in body.get("tools", []) if tool.get("name") == "read"]
    check(len(read) == 1, "default_read_tool_declaration")
    tool = read[0]
    schema = tool.get("input_schema", {})
    check(schema.get("type") == "object"
          and schema.get("properties", {}).get("path", {}).get("type") == "string"
          and "path" in schema.get("required", []), "default_read_path_schema")
    check("strict" not in tool, "default_read_strict")
    check(tool.get("eager_input_streaming") is True if pi
          else "eager_input_streaming" not in tool, "default_read_eager_input")
    effort = body.get("output_config", {})
    check(effort.get("effort") == "medium" if pi else "effort" not in effort,
          "default_effort")
    check(thinking.get("display") == "summarized" if pi else "display" not in thinking,
          "default_thinking_display")


class MessagesState(DefaultState):
    def __init__(self, path, *, pi):
        super().__init__(True, path, pi=pi)
        self.beta_seen = False
        self.block_binding_seen = False

    def incoming(self, headers, body):
        phase = stage(body)
        check(headers.get_all("anthropic-version") == ["2023-06-01"], "default_messages_version")
        assert_default_profile(body, self.pi)
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
        assert_default_profile(body, self.pi)
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
        check(self.deliveries == [200, 200, 200], "default_deliveries")


def assert_chat_profile(body, pi, *, upstream=False):
    check(body["model"] == "deepseek-flash" and body["stream"] is True, "default_chat_model")
    cap = "max_completion_tokens" if upstream else "max_tokens"
    check(body[cap] == 2048,
          "default_operational_output_cap")
    check("store" not in body and body.get("stream_options") == {"include_usage": True},
          "default_chat_storage_and_usage")
    check(body.get("thinking") == {"type": "enabled"} if pi else "thinking" not in body,
          "default_chat_thinking")
    check(body.get("reasoning_effort") == "high" if pi else "reasoning_effort" not in body,
          "default_effort")
    tools = body.get("tools", [])
    check(len(tools) == 1 and tools[0].get("type") == "function"
          and tools[0].get("function", {}).get("name") == "read", "default_read_tool_declaration")
    tool = tools[0]["function"]
    check(tool.get("strict") is pi, "default_read_strict")
    schema = tool.get("parameters", {})
    check(schema.get("type") == "object"
          and schema.get("properties", {}).get("path", {}).get("type") == "string"
          and "path" in schema.get("required", []), "default_read_path_schema")


class ChatState(DefaultState):
    def __init__(self, path, *, pi):
        super().__init__(False, path, pi=pi)

    def incoming(self, headers, body):
        assert_chat_profile(body, self.pi)
        phase = stage(body)
        assert_history(body, phase, False, self.path, profile=False)
        with self.lock:
            self.ingress.append(phase)
            expected = [1] if self.pi else [1, 2, 3]
            check(len(self.ingress) <= len(expected) and self.ingress == expected[:len(self.ingress)],
                  "default_extra_or_reordered_client_request")

    def upstream(self, headers, body):
        check(not self.pi, "default_strict_guarantee_dispatched")
        check(headers.get("authorization") == "Bearer synthetic-client-primary",
              "default_upstream_credential")
        assert_chat_profile(body, False, upstream=True)
        phase = stage(body)
        assert_history(body, phase, False, self.path, profile=False)
        with self.lock:
            self.attempts.append(phase)
            check(len(self.attempts) <= 3 and self.attempts == [1, 2, 3][:len(self.attempts)],
                  "default_extra_or_reordered_upstream_attempt")
        return 200, chat_events(phase, self.path, model="deepseek-flash"), "text/event-stream"

    def finish(self):
        check(not self.errors, "default_authority_errors:" + ",".join(self.errors))
        check(self.ingress == ([1] if self.pi else [1, 2, 3])
              and self.attempts == ([] if self.pi else [1, 2, 3]), "default_exchange_counts")
        check(self.deliveries == ([400] if self.pi else [200, 200, 200]), "default_deliveries")


def response_items(phase, path):
    items = [{"id": f"rs_default_{phase}", "type": "reasoning", "status": "completed",
              "summary": [{"type": "summary_text", "text": PLAN}]}]
    if phase == 1:
        items.append({"id": "fc_default_read", "type": "function_call", "status": "completed",
                      "call_id": CALL, "name": "read", "arguments": json.dumps({"path": str(path)})})
    else:
        items.append({"id": f"msg_default_{phase}", "type": "message", "role": "assistant",
                      "phase": "final_answer", "status": "completed",
                      "content": [{"type": "output_text", "text": TEXT, "annotations": [], "logprobs": []}]})
    return items


def responses_events(phase, path):
    # Same finite source-derived event contract as tests/support/responses_profile.rs.
    items = response_items(phase, path)
    response = {"id": f"resp_default_{phase}", "object": "response", "created_at": 1,
                "model": PROFILES["responses"][0], "status": "in_progress", "output": []}
    events = [{"type": "response.created", "response": response}]
    for index, item in enumerate(items):
        kind = item["type"]
        opening = {**item, "status": "in_progress"}
        opening["summary" if kind == "reasoning" else "arguments" if kind == "function_call" else "content"] = (
            "" if kind == "function_call" else [])
        events.append({"type": "response.output_item.added", "output_index": index, "item": opening})
        owner = {"output_index": index, "item_id": item["id"]}
        if kind == "reasoning":
            part = item["summary"][0]
            events += [
                {**owner, "type": "response.reasoning_summary_part.added", "summary_index": 0,
                 "part": {"type": "summary_text", "text": ""}},
                {**owner, "type": "response.reasoning_summary_text.delta", "summary_index": 0, "delta": PLAN},
                {**owner, "type": "response.reasoning_summary_text.done", "summary_index": 0, "text": PLAN},
                {**owner, "type": "response.reasoning_summary_part.done", "summary_index": 0, "part": part},
            ]
        elif kind == "function_call":
            events += [
                {**owner, "type": "response.function_call_arguments.delta", "delta": item["arguments"]},
                {**owner, "type": "response.function_call_arguments.done", "arguments": item["arguments"]},
            ]
        else:
            part = item["content"][0]
            events += [
                {**owner, "type": "response.content_part.added", "content_index": 0,
                 "part": {"type": "output_text", "text": "", "annotations": []}},
                {**owner, "type": "response.output_text.delta", "content_index": 0,
                 "delta": TEXT, "logprobs": []},
                {**owner, "type": "response.output_text.done", "content_index": 0,
                 "text": TEXT, "logprobs": []},
                {**owner, "type": "response.content_part.done", "content_index": 0, "part": part},
            ]
        events.append({"type": "response.output_item.done", "output_index": index, "item": item})
    events.append({"type": "response.completed", "response": {
        **response, "status": "completed", "output": items,
        "usage": {"input_tokens": 11, "output_tokens": 7, "total_tokens": 18}}})
    return "".join(f"event: {event['type']}\ndata: {json.dumps({**event, 'sequence_number': index})}\n\n"
                   for index, event in enumerate(events)).encode()


def responses_stage(body):
    items = body["input"]
    users = [item for item in items if item.get("role") == "user"]
    check(bool(users), "default_user_history")
    last = text(users[-1].get("content"))
    if NEXT in last:
        return 3
    if any(item.get("type") == "function_call_output" for item in items):
        return 2
    check(READ in last, "unexpected_client_prompt")
    return 1


def assert_responses_profile(body, pi):
    check(body["model"] == PROFILES["responses"][0] and body["stream"] is True,
          "default_responses_model")
    check(body["max_output_tokens"] == 2048 and body.get("store") is False,
          "default_responses_bounds_and_storage")
    check(body.get("include") == ["reasoning.encrypted_content"], "default_responses_include")
    reasoning = body.get("reasoning", {})
    check(reasoning == {"effort": "medium", "summary": "auto"} if pi else not reasoning,
          "default_responses_reasoning")
    check("previous_response_id" not in body and "prompt_cache_retention" not in body,
          "default_responses_full_history")
    additions = [item for item in body["input"] if item.get("type") == "additional_tools"]
    if pi:
        check(len(additions) == 1 and not body.get("tools"), "default_responses_additional_tools")
        tools = additions[0]["tools"]
    else:
        check(not additions, "default_responses_additional_tools")
        tools = body.get("tools", [])
    check(len(tools) == 1 and tools[0].get("type") == "function"
          and tools[0].get("name") == "read", "default_read_tool_declaration")
    tool = tools[0]
    check("strict" not in tool if pi else tool.get("strict") is False, "default_read_strict")
    schema = tool.get("parameters", {})
    check(schema.get("type") == "object"
          and schema.get("properties", {}).get("path", {}).get("type") == "string"
          and "path" in schema.get("required", []), "default_read_path_schema")


def assert_responses_history(body, phase, path):
    items = body["input"]
    calls = [item for item in items if item.get("type") == "function_call"]
    results = [item for item in items if item.get("type") == "function_call_output"]
    messages = [item for item in items if item.get("role") in ("user", "assistant")]
    expected_roles = ["user"] if phase < 3 else ["user", "assistant", "user"]
    check([item["role"] for item in messages] == expected_roles, "default_responses_history_roles")
    order = [item.get("role") if item.get("role") in ("user", "assistant")
             else item.get("type") for item in items if item.get("role") not in ("system", "developer")]
    expected_order = ["user"] if phase == 1 else ["user", "reasoning", "function_call", "function_call_output"]
    if phase == 3:
        expected_order += ["reasoning", "assistant", "user"]
    # Pi's rejected initial request declares its actual read tool in-band.
    order = [kind for kind in order if kind != "additional_tools"]
    check(order == expected_order, "default_responses_history_order")
    check(len(calls) == len(results) == int(phase > 1), "default_responses_call_count")
    if phase > 1:
        call = calls[0]
        check(call.get("id") == "fc_default_read" and call.get("call_id") == CALL
              and call.get("name") == "read" and json.loads(call["arguments"]) == {"path": str(path)},
              "default_responses_call_identity")
        check(results[0].get("call_id") == CALL and TEXT in text(results[0]["output"]),
              "default_responses_result_identity")
    if phase == 3:
        answer = messages[1]
        check(answer.get("id") == "msg_default_2" and answer.get("phase") == "final_answer"
              and "".join(part["text"] for part in answer["content"]) == TEXT,
              "default_responses_final_history")
        check(answer.get("status") == "completed" and
              all("annotations" not in part for part in answer["content"]),
              "default_responses_missing_annotations_changed")
    reasoning = [item for item in items if item.get("type") == "reasoning"]
    check(len(reasoning) == phase - 1,
          "default_responses_reasoning_history")
    for index, item in enumerate(reasoning, 1):
        check(item.get("id") == f"rs_default_{index}"
              and item.get("summary") == [{"type": "summary_text", "text": PLAN}],
              "default_responses_reasoning_content")


class ResponsesState(DefaultState):
    endpoint_path = "/v1/responses"

    def __init__(self, path, *, pi):
        super().__init__(False, path, pi=pi)

    def incoming(self, headers, body):
        assert_responses_profile(body, self.pi)
        phase = responses_stage(body)
        assert_responses_history(body, phase, self.path)
        with self.lock:
            self.ingress.append(phase)
            check(len(self.ingress) <= 3 and self.ingress == [1, 2, 3][:len(self.ingress)],
                  "default_extra_or_reordered_client_request")

    def upstream(self, headers, body):
        check(not self.pi, "default_additional_tools_dispatched")
        check(headers.get("authorization") == "Bearer synthetic-client-primary",
              "default_upstream_credential")
        assert_responses_profile(body, self.pi)
        phase = responses_stage(body)
        assert_responses_history(body, phase, self.path)
        with self.lock:
            self.attempts.append(phase)
            check(len(self.attempts) <= 3 and self.attempts == [1, 2, 3][:len(self.attempts)],
                  "default_extra_or_reordered_upstream_attempt")
        return 200, responses_events(phase, self.path), "text/event-stream"

    def finish(self):
        check(not self.errors, "default_authority_errors:" + ",".join(self.errors))
        check(self.ingress == ([1] if self.pi else [1, 2, 3])
              and self.attempts == ([] if self.pi else [1, 2]), "default_exchange_counts")
        check(self.deliveries == ([400] if self.pi else [200, 200, 400]), "default_deliveries")


def assert_opencode_rejection(events):
    errors = [event["error"] for event in events if event.get("type") == "error"]
    check(len(errors) == 1 and errors[0].get("type") == "provider.invalid-request"
          and errors[0].get("status") == 400, "default_responses_client_rejection")
    check(not any(event.get("type") in ("text", "tool_use") for event in events),
          "default_responses_false_success")


def assert_diagnostics(requests, protocol, pi):
    initial_rejection = pi and protocol != "messages"
    check(len(requests) == (1 if initial_rejection else 3), "default_gateway_request_count")
    for index, record in enumerate(requests):
        rejected = initial_rejection or protocol == "responses" and index == 2
        check(record["outcome"] == ("error" if rejected else "complete")
              and len(record["candidates"]) == (0 if rejected else 1),
              "default_gateway_completion")
        if not rejected:
            check(record.get("ignored_request_field_count", 0) == 0, "default_gateway_ignored_fields")
            continue
        check(record["stage"] == "admission", "default_rejection_stage")
        generation = record["generation"]
        if protocol == "chat":
            check(generation.get("planning_failure", {}).get("reasons") == ["admission.strict_tools"],
                  "default_strict_rejection_reason")
        else:
            check(generation.get("failure") == {
                "boundary": "request.decode",
                "code": "codec.unsupported.unavailable" if pi else "codec.invalid.invalid_value"},
                "default_responses_rejection_reason")


def run_pi(binary, env, root, work, origin, path, protocol):
    model_id, provider, _, api = PROFILES[protocol]
    agent = root / "pi-agent"
    agent.mkdir(mode=0o700)
    private_json(agent / "models.json", {"providers": {provider: {
        "baseUrl": origin if protocol == "messages" else origin + "/v1", "apiKey": "$INGRESS_KEY",
        "modelOverrides": {model_id: {"maxTokens": 2048}}}}})
    private_json(agent / "settings.json", {"retry": {"enabled": False,
        "provider": {"maxRetries": 0, "timeoutMs": 5000}},
        "compaction": {"enabled": False}, "cacheWarming": "off"})
    env = {**env, "PI_CODING_AGENT_DIR": str(agent), "PI_CACHE_RETENTION": "short"}
    with Pi(binary, env, work, model_id, provider=provider, thinking=None) as client:
        initial, _ = client.command("get_state")
        model = initial["model"]
        check(model["id"] == model_id and model["provider"] == provider
              and model["api"] == api and model["reasoning"],
              "pi_resolved_builtin_metadata")
        if protocol == "messages":
            check(model["compat"]["forceAdaptiveThinking"] is True
                  and model["compat"]["supportsTemperature"] is False, "pi_resolved_messages_metadata")
        check(initial["thinkingLevel"] == ("high" if protocol == "chat" else "medium"),
              "pi_default_thinking_level")
        if protocol != "messages":
            _, events = client.command("prompt", message=f"{READ}: Read {path}, then answer.")
            finished = [event["message"] for event in events if event.get("type") == "message_end"
                        and event.get("message", {}).get("role") == "assistant"]
            assert_pi_rejection(finished, protocol)
            check(not any(event["type"].startswith(("auto_retry", "compaction", "tool_execution",
                                                   "summarization_retry")) for event in events),
                  "pi_default_request_rejection_retried")
            return
        client.prompt(f"{READ}: Read {path}, then answer.", ["toolUse", "stop"])
        client.prompt(f"{NEXT}: Repeat the answer without reading again.", ["stop"])
        final, _ = client.command("get_state")
        check(final["sessionId"] == initial["sessionId"] and not final["isStreaming"],
              "pi_default_session_changed")
        messages, _ = client.command("get_messages")
        check([m["role"] for m in messages["messages"]] ==
              ["system", "user", "assistant", "toolResult", "assistant", "user", "assistant"],
              "pi_default_final_history")


def run_opencode(binary, env, root, work, origin, path, protocol):
    model_id, provider, _, _ = PROFILES[protocol]
    if protocol == "responses":
        provider = "openai"
    config = opencode_configuration(root, origin)
    value = json.loads(config.read_text())
    cap = "max_output_tokens" if protocol == "responses" else "max_tokens"
    value["providers"] = {provider: {"env": ["INGRESS_KEY"],
        "settings": {"baseURL": origin + "/v1"}, "body": {cap: 2048},
        "models": {model_id: {"limit": {"output": 2048},
            "settings": {"baseURL": origin + "/v1"}}}}}
    if protocol == "responses":
        value["providers"][provider]["settings"]["transport"] = "http"
    config.unlink()
    private_json(config, value)
    env = {**env, "OPENCODE_CONFIG": str(config), "OPENCODE_DISABLE_AUTOUPDATE": "1",
           "OPENCODE_DISABLE_MODELS_FETCH": "1", "OPENCODE_DISABLE_PROJECT_CONFIG": "1"}
    session, events = opencode_prompt(binary, env, work,
        f"{READ}: Read {path}, then answer.", "protocol-default", model=f"{provider}/{model_id}")
    tools = [e["part"] for e in events if e.get("type") == "tool_use"]
    check(len(tools) == 1 and tools[0]["tool"] == "read"
          and tools[0]["state"]["status"] == "completed"
          and TEXT in tools[0]["state"]["output"], "opencode_default_actual_read")
    resumed, _ = opencode_prompt(binary, env, work,
        f"{NEXT}: Repeat the answer without reading again.", "protocol-default", session,
        model=f"{provider}/{model_id}",
        **({"expected_status": 1, "check_output": assert_opencode_rejection}
           if protocol == "responses" else {}))
    check(resumed == session, "opencode_default_session_changed")
    databases = list(Path(env["XDG_DATA_HOME"]).rglob("opencode.db"))
    check(len(databases) == 1, "opencode_database_count")
    with closing(sqlite3.connect(f"file:{databases[0]}?mode=ro", uri=True)) as database:
        check_opencode_session(database, session,
            ["tool-calls", "stop", "error"] if protocol == "responses" else ["tool-calls", "stop", "stop"],
            idle_outcome="failed" if protocol == "responses" else "succeeded")


def main():
    case, directory, gateway_binary, binary = sys.argv[1:]
    root = Path(directory)
    work = root / "work"
    work.mkdir(mode=0o700)
    path = work / "sample.txt"
    path.write_text(TEXT + "\n", encoding="utf-8")
    protocol = case.split("-")[1]
    pi = case.startswith("pi-")
    rejected = pi and protocol != "messages"
    count = 1 if rejected else 3
    attempts = 0 if rejected else 2 if protocol == "responses" else 3
    model_id, provider, binding, _ = PROFILES[protocol]
    state = {"messages": MessagesState, "chat": ChatState, "responses": ResponsesState}[protocol](path, pi=pi)
    env = environment(root)
    with Child([binary, "--version"], env, work) as client:
        version = client.wait(timeout=5).decode().strip()
    check_client_version(case, version)
    with ExitStack() as stack:
        origin = stack.enter_context(server(state, False))
        gateway_configuration(root, protocol == "messages", origin,
                              model=model_id, provider="openai" if protocol == "responses" else provider,
                              binding=binding)
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
                binary, env, root, work, ingress, path, protocol)
            gateway.process.send_signal(signal.SIGINT)
            gateway.wait(timeout=5)
            records = [json.loads(line) for line in (root / "diagnostics.jsonl").read_text().splitlines()]
            check(all(r.get("event") in ("file", "request", "log", "span") for r in records),
                  "default_gateway_logging_loss")
            requests = [r for r in records if r.get("event") == "request"]
            assert_diagnostics(requests, protocol, pi)
        except Exception:
            print(json.dumps({"errors": state.errors, "ingress_phases": state.ingress,
                              "upstream_phases": state.attempts}), file=sys.stderr)
            raise
    state.finish()
    print(json.dumps({"case": case, "version": version, "client_requests": count,
        "upstream_attempts": attempts, "extra_client_requests": 0,
        "tool_history": not rejected,
        "default_rejection": rejected or protocol == "responses",
        "next_turn_history": not rejected and protocol != "responses",
        **({"default_headers": True, "beta_header_seen": state.beta_seen,
            "block_binding_seen": state.block_binding_seen} if protocol == "messages" else {})}))


if __name__ == "__main__":
    main()
