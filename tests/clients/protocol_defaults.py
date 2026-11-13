"""Installed protocol defaults and an explicit runtime-selected Chat thinking-off profile."""
from contextlib import ExitStack, closing
import base64
import copy
import json
from pathlib import Path
import queue
import signal
import sqlite3
import subprocess
import sys

from drivers import (Child, NEXT, READ, TEXT, Pi, check, check_client_version,
                     check_opencode_session, environment, opencode_configuration,
                     opencode_prompt, private_json, gateway_configuration, shutdown_gateway)
from recovery import (State as RecoveryState, PLAN, SIGNATURE, REDACTED, CALL, assert_history, chat_events,
                      native_events, server, stage, text)

MODEL = "claude-haiku-5-5"
PROVIDER = "opencode-go"
PROFILES = {
    "messages": (MODEL, PROVIDER, "opencode-go-haiku-messages", "anthropic-messages"),
    "chat": ("deepseek-v4.1-flash-1017", "deepseek", "deepseek-chat", "openai-completions"),
    "responses": ("gpt-6.1-sol", "opencode", "openai-sol-responses", "openai-responses"),
}


class DefaultState(RecoveryState):
    def __init__(self, native, path, *, pi):
        super().__init__(native, path)
        self.pi = pi
        self.deliveries = []
        self.tool_inventories = {}

    def observe_response(self, body, status, content_type, payload):
        check(status == 200, "default_delivery_status")
        check(content_type is not None and content_type.split(";")[0] == "text/event-stream",
              "default_delivery_content_type")
        with self.lock:
            self.deliveries.append(status)


def assert_default_profile(body, pi):
    check(body["model"] == MODEL and body["stream"] is True, "default_messages_model")
    thinking = body.get("thinking", {})
    check(thinking.get("type") == "adaptive", "default_adaptive_thinking")
    check(body["max_tokens"] == 2048, "default_operational_output_cap")
    tools = body.get("tools", [])
    read = [tool for tool in tools if tool.get("name") == "read"]
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
    check(effort.get("effort") == "low" if pi else "effort" not in effort,
          "default_effort")
    if pi:
        check(body["messages"][-1] == {"role": "system", "content": [],
              "output_config": {"effort": "medium"}}, "default_message_effort")
    check(thinking.get("display") == "summarized" if pi else "display" not in thinking,
          "default_thinking_display")
    return tools


class MessagesState(DefaultState):
    def __init__(self, path, *, pi):
        super().__init__(True, path, pi=pi)
        self.beta_seen = False
        self.block_binding_seen = False

    def incoming(self, headers, body):
        phase = stage(body)
        check(headers.get_all("anthropic-version") == ["2023-06-01"], "default_messages_version")
        tools = assert_default_profile(body, self.pi)
        with self.lock:
            self.tool_inventories[phase] = copy.deepcopy(tools)
            self.beta_seen |= bool(headers.get("anthropic-beta"))
            self.block_binding_seen |= "block_binding" in body.get("thinking", {})
            self.ingress.append(phase)
            check(len(self.ingress) <= 3 and self.ingress == [1, 2, 3][:len(self.ingress)],
                  "default_extra_or_reordered_client_request")

    def upstream(self, headers, body):
        phase = stage(body)
        check(headers.get("x-api-key") == "synthetic-client-primary", "default_upstream_credential")
        check(headers.get("anthropic-version") == "2026-10-21", "trusted_upstream_version")
        tools = assert_default_profile(body, self.pi)
        check("block_binding" not in body["thinking"], "client_binding_not_omitted")
        assert_history(body, phase, True, self.path, profile=False)
        with self.lock:
            check(tools == self.tool_inventories.get(phase), "default_tool_inventory_changed")
            self.attempts.append(phase)
            check(len(self.attempts) <= 3 and self.attempts == [1, 2, 3][:len(self.attempts)],
                  "default_extra_or_reordered_upstream_attempt")
        return 200, native_events(phase, self.path), "text/event-stream"

    def finish(self):
        check(not self.errors, "default_authority_errors:" + ",".join(self.errors))
        check(self.ingress == self.attempts == [1, 2, 3], "default_exchange_counts")
        check(self.deliveries == [200, 200, 200], "default_deliveries")


def assert_chat_profile(body, pi, *, upstream=False, gateway_profile=False):
    check(body["model"] == ("deepseek-flash" if upstream else PROFILES["chat"][0])
          and body["stream"] is True, "default_chat_model")
    cap = "max_tokens"
    check(body[cap] == 2048,
          "default_operational_output_cap")
    check("store" not in body and body.get("stream_options") == {"include_usage": True},
          "default_chat_storage_and_usage")
    check(body.get("thinking") == {"type": "enabled"} if pi and not upstream and not gateway_profile
          else "thinking" not in body, "default_chat_thinking")
    check(body.get("reasoning_effort") == "none" if gateway_profile else
          (body.get("reasoning_effort") == "high" if pi else "reasoning_effort" not in body),
          "default_effort")
    tools = body.get("tools", [])
    read = [tool["function"] for tool in tools if tool.get("type") == "function"
            and tool.get("function", {}).get("name") == "read"]
    check(len(read) == 1, "default_read_tool_declaration")
    tool = read[0]
    check(tool.get("strict") is pi, "default_read_strict")
    schema = tool.get("parameters", {})
    check(schema.get("type") == "object"
          and schema.get("properties", {}).get("path", {}).get("type") == "string"
          and "path" in schema.get("required", []), "default_read_path_schema")
    return tools


class ChatState(DefaultState):
    def __init__(self, path, *, pi, gateway_profile=False):
        super().__init__(False, path, pi=pi)
        self.gateway_profile = gateway_profile
        # pi's strict lane dispatches through the DeepSeek beta endpoint binding.
        self.upstream_endpoint_path = "/beta/chat/completions" if pi else "/v1/chat/completions"

    def incoming(self, headers, body):
        tools = assert_chat_profile(body, self.pi, gateway_profile=self.gateway_profile)
        phase = stage(body)
        assert_history(body, phase, False, self.path, profile=False)
        with self.lock:
            self.tool_inventories[phase] = copy.deepcopy(tools)
            self.ingress.append(phase)
            check(len(self.ingress) <= 3 and self.ingress == [1, 2, 3][:len(self.ingress)],
                  "default_extra_or_reordered_client_request")

    def upstream(self, headers, body):
        check(self.gateway_profile, "default_chat_must_not_dispatch")
        check(headers.get("authorization") == "Bearer synthetic-client-primary",
              "default_upstream_credential")
        tools = assert_chat_profile(body, self.pi, upstream=True, gateway_profile=True)
        phase = stage(body)
        assert_history(body, phase, False, self.path, profile=False)
        # Built-in client compatibility may add empty reasoning fields. Standard
        # ingress discards them; the upstream must not receive private history.
        check(all("reasoning_content" not in message and "reasoning_details" not in message
                  for message in body["messages"]), "default_upstream_standard_history")
        with self.lock:
            check(tools == self.tool_inventories.get(phase), "default_tool_inventory_changed")
            self.attempts.append(phase)
            check(len(self.attempts) <= 3 and self.attempts == [1, 2, 3][:len(self.attempts)],
                  "default_extra_or_reordered_upstream_attempt")
        return 200, chat_events(phase, self.path, model="deepseek-flash"), "text/event-stream"

    def observe_response(self, body, status, content_type, payload):
        if self.gateway_profile:
            return super().observe_response(body, status, content_type, payload)
        check(status == 400 and content_type.split(";")[0] == "application/json",
              "required_history_pre_stream_error")
        error = json.loads(payload)["error"]
        check(error["type"] == "invalid_request_error"
              and error["code"] == "reasoning_history_required"
              and "will not disable thinking" in error["message"], "required_history_error")
        with self.lock:
            self.deliveries.append(status)

    def finish(self):
        check(not self.errors, "default_authority_errors:" + ",".join(self.errors))
        if self.gateway_profile:
            check(self.ingress == self.attempts == [1, 2, 3], "default_exchange_counts")
            check(self.deliveries == [200, 200, 200], "default_deliveries")
        else:
            check(self.ingress == [1] and not self.attempts and self.deliveries == [400],
                  "required_history_no_dispatch_or_retry")


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


def responses_events(phase, path, *, items=None):
    # Same finite source-derived event contract as tests/support/responses_profile.rs.
    items = response_items(phase, path) if items is None else items
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
            for content_index, part in enumerate(item["content"]):
                position = {**owner, "content_index": content_index}
                events.append({**position, "type": "response.content_part.added",
                               "part": {"type": "output_text", "text": "", "annotations": []}})
                if part["text"]:
                    events.append({**position, "type": "response.output_text.delta",
                                   "delta": part["text"], "logprobs": []})
                events += [
                    {**position, "type": "response.output_text.done",
                     "text": part["text"], "logprobs": []},
                    {**position, "type": "response.content_part.done", "part": part},
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
        check(body["input"][0] is additions[0] and additions[0].get("role") == "developer",
              "default_responses_declaration_position")
        tools = additions[0]["tools"]
    else:
        check(not additions, "default_responses_additional_tools")
        tools = body.get("tools", [])
    read = [tool for tool in tools if tool.get("type") == "function" and tool.get("name") == "read"]
    check(len(read) == 1, "default_read_tool_declaration")
    tool = read[0]
    check("strict" not in tool if pi else tool.get("strict") is False, "default_read_strict")
    schema = tool.get("parameters", {})
    check(schema.get("type") == "object"
          and schema.get("properties", {}).get("path", {}).get("type") == "string"
          and "path" in schema.get("required", []), "default_read_path_schema")
    return tools


def assert_responses_history(body, phase, path, *, pi=False, upstream=False):
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
    # Pi declares its actual read tool in-band; it keeps its conversation position.
    order = [kind for kind in order if kind != "additional_tools"]
    check(order == expected_order, "default_responses_history_order")
    check(len(calls) == len(results) == int(phase > 1), "default_responses_call_count")
    if phase > 1:
        call = calls[0]
        check(call.get("id") == "fc_default_read" and call.get("call_id") == CALL
              and call.get("name") == "read" and json.loads(call["arguments"]) == {"path": str(path)},
              "default_responses_call_identity")
        check(call.get("namespace") is None and call.get("async") in (None, False)
              and call.get("caller") in (None, {"type": "direct"}),
              "default_responses_direct_call")
        check(results[0].get("call_id") == CALL and TEXT in text(results[0]["output"]),
              "default_responses_result_identity")
    if phase == 3:
        answer = messages[1]
        check(answer.get("id") == "msg_default_2" and answer.get("phase") == "final_answer"
              and "".join(part["text"] for part in answer["content"]) == TEXT,
              "default_responses_final_history")
        check(answer.get("status") == "completed" and
              all(part.get("annotations") == [] if pi or upstream else part.get("annotations", []) == []
                  for part in answer["content"]), "default_responses_annotations")
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
        tools = assert_responses_profile(body, self.pi)
        phase = responses_stage(body)
        assert_responses_history(body, phase, self.path, pi=self.pi)
        with self.lock:
            self.tool_inventories[phase] = copy.deepcopy(tools)
            self.ingress.append(phase)
            check(len(self.ingress) <= 3 and self.ingress == [1, 2, 3][:len(self.ingress)],
                  "default_extra_or_reordered_client_request")

    def upstream(self, headers, body):
        check(headers.get("authorization") == "Bearer synthetic-client-primary",
              "default_upstream_credential")
        tools = assert_responses_profile(body, self.pi)
        phase = responses_stage(body)
        assert_responses_history(body, phase, self.path, pi=self.pi, upstream=True)
        with self.lock:
            check(tools == self.tool_inventories.get(phase), "default_tool_inventory_changed")
            self.attempts.append(phase)
            check(len(self.attempts) <= 3 and self.attempts == [1, 2, 3][:len(self.attempts)],
                  "default_extra_or_reordered_upstream_attempt")
        return 200, responses_events(phase, self.path), "text/event-stream"

    def finish(self):
        check(not self.errors, "default_authority_errors:" + ",".join(self.errors))
        check(self.ingress == [1, 2, 3]
              and self.attempts == [1, 2, 3], "default_exchange_counts")
        check(self.deliveries == [200, 200, 200], "default_deliveries")


def run_pi(binary, env, root, work, origin, path, protocol, *, gateway_profile=False):
    model_id, provider, _, api = PROFILES[protocol]
    agent = root / "pi-agent"
    agent.mkdir(mode=0o700)
    override = {"maxTokens": 2048}
    configuration = {"baseUrl": origin if protocol == "messages" else origin + "/v1",
                     "apiKey": "$INGRESS_KEY", "modelOverrides": {model_id: override}}
    if protocol == "chat":
        package = Path(binary).resolve().parents[1] / "lib/pi/node_modules/@earendil-works/pi-ai"
        captured = subprocess.run(["node", str(Path(__file__).with_name("pi_default_shapes.mjs")),
                                   str(package), "deepseek-model"],
                                  capture_output=True, check=True, timeout=5)
        selected = json.loads(captured.stdout)
        selected.update(id=model_id, baseUrl=origin + "/v1", maxTokens=2048)
        configuration.pop("modelOverrides")
        configuration.update(api=api, models=[selected])
    if gateway_profile:
        profile = json.loads((Path(__file__).resolve().parents[2] /
                              "config/pi-deepseek.example.json").read_text())
        selected.update(profile["providers"]["deepseek"]["models"][0], maxTokens=2048)
    private_json(agent / "models.json", {"providers": {provider: configuration}})
    private_json(agent / "settings.json", {"retry": {"enabled": False,
        "provider": {"maxRetries": 0, "timeoutMs": 5000}},
        "compaction": {"enabled": False}, "cacheWarming": "off"})
    env = {**env, "PI_CODING_AGENT_DIR": str(agent), "PI_CACHE_RETENTION": "short"}
    with Pi(binary, env, work, model_id, provider=provider,
            thinking="off" if gateway_profile else None) as client:
        initial, _ = client.command("get_state")
        model = initial["model"]
        check(model["id"] == model_id and model["provider"] == provider
              and model["api"] == api and model["reasoning"],
              "pi_resolved_builtin_metadata")
        if protocol == "messages":
            check(model["compat"]["forceAdaptiveThinking"] is True
                  and model["compat"]["supportsTemperature"] is False, "pi_resolved_messages_metadata")
        check(initial["thinkingLevel"] ==
              ("off" if gateway_profile else "high" if protocol == "chat" else "medium"),
              "pi_default_thinking_level")
        if protocol == "chat" and not gateway_profile:
            _, events = client.command("prompt", message=f"{READ}: Read {path}, then answer.")
            assistants = [event["message"] for event in events if event.get("type") == "message_end"
                          and event.get("message", {}).get("role") == "assistant"]
            check(len(assistants) == 1 and assistants[0]["stopReason"] == "error"
                  and "will not disable thinking" in assistants[0].get("errorMessage", ""),
                  "pi_required_history_error")
            check(not any(event["type"].startswith(("tool_execution", "auto_retry"))
                          for event in events), "pi_required_history_no_execution_or_retry")
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
    if protocol == "chat":
        def rejected(events):
            check(not any(event.get("type") == "tool_use" for event in events)
                  and any(event.get("type") == "error"
                          and "will not disable thinking" in json.dumps(event)
                          for event in events), "opencode_required_history_error")
        opencode_prompt(binary, env, work, f"{READ}: Read {path}, then answer.",
                        "protocol-default", model=f"{provider}/{model_id}",
                        expected_status=1, check_output=rejected)
        return
    session, events = opencode_prompt(binary, env, work,
        f"{READ}: Read {path}, then answer.", "protocol-default", model=f"{provider}/{model_id}")
    tools = [e["part"] for e in events if e.get("type") == "tool_use"]
    check(len(tools) == 1 and tools[0]["tool"] == "read"
          and tools[0]["state"]["status"] == "completed"
          and TEXT in tools[0]["state"]["output"], "opencode_default_actual_read")
    resumed, _ = opencode_prompt(binary, env, work,
        f"{NEXT}: Repeat the answer without reading again.", "protocol-default", session,
        model=f"{provider}/{model_id}")
    check(resumed == session, "opencode_default_session_changed")
    databases = list(Path(env["XDG_DATA_HOME"]).rglob("opencode.db"))
    check(len(databases) == 1, "opencode_database_count")
    with closing(sqlite3.connect(f"file:{databases[0]}?mode=ro", uri=True)) as database:
        check_opencode_session(database, session,
            ["tool-calls", "stop", "stop"])


def main():
    case, directory, gateway_binary, binary = sys.argv[1:]
    root = Path(directory)
    work = root / "work"
    work.mkdir(mode=0o700)
    path = work / "sample.txt"
    path.write_text(TEXT + "\n", encoding="utf-8")
    protocol = case.split("-")[1]
    pi = case.startswith("pi-")
    gateway_profile = case == "pi-chat-gateway-profile"
    rejected = protocol == "chat" and not gateway_profile
    count = 1 if rejected else 3
    model_id, provider, binding, _ = PROFILES[protocol]
    if pi and protocol == "chat":
        # pi's builtin strict read tool needs the beta endpoint binding.
        provider, binding = "deepseek-beta", "deepseek-beta-chat"
    state = (ChatState(path, pi=pi, gateway_profile=gateway_profile) if protocol == "chat" else
             {"messages": MessagesState, "responses": ResponsesState}[protocol](path, pi=pi))
    env = environment(root)
    with Child([binary, "--version"], env, work) as client:
        version = client.wait(timeout=5).decode().strip()
    check_client_version(case, version)
    with ExitStack() as stack:
        origin = stack.enter_context(server(state, False))
        gateway_configuration(root, protocol == "messages", origin,
                              model=model_id, provider="openai" if protocol == "responses" else provider,
                              binding=binding, api_root="/beta" if pi and protocol == "chat" else "/v1")
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
            (run_pi if pi else run_opencode)(
                binary, env, root, work, ingress, path, protocol,
                **({"gateway_profile": gateway_profile} if pi else {}))
            gateway.process.send_signal(signal.SIGINT)
            gateway.wait(timeout=5)
        except Exception:
            shutdown_gateway(gateway, failed=True)
            print(json.dumps({"errors": state.errors, "ingress_phases": state.ingress,
                              "upstream_phases": state.attempts}), file=sys.stderr)
            raise
    state.finish()
    print(json.dumps({"case": case, "version": version, "client_requests": count,
        "upstream_attempts": len(state.attempts), "extra_client_requests": 0,
        "tool_history": not rejected,
        "next_turn_history": not rejected,
        **({"rejection": "reasoning_history_required"} if rejected else {}),
        **({"default_headers": True, "beta_header_seen": state.beta_seen,
            "block_binding_seen": state.block_binding_seen} if protocol == "messages" else {})}))


if __name__ == "__main__":
    main()
