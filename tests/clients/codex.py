"""Isolated Codex CLI installed-client loopback driver for Responses Code Mode and cross-protocol loops."""
from contextlib import ExitStack
import copy
import json
from pathlib import Path
import subprocess
import sys
import threading

from drivers import (Child, KEY, TEXT, check, check_client_version, environment,
                     gateway_configuration, shutdown_gateway)
from recovery import server

CODEX_CALL = "call_codex_exec_1"
CODE_MODE_PAYLOAD = 'exit({ ok: true, label: "codex-ok" });'
NS_CALL = "call_codex_ns_1"
NS_ARGS = {"id": "nonexistent-agent"}


def codex_profile(case):
    if case == "codex-responses":
        return {
            "native": False,
            "model": "gpt-6.2-sol",
            "provider": "openai",
            "binding": "openai-sol-responses",
            "upstream_path": "/v1/responses",
            "upstream_model": "gpt-6.2-sol",
            "protocol": "responses",
        }
    if case == "codex-messages":
        return {
            "native": True,
            "model": "claude-haiku-5-5",
            "provider": "opencode-go",
            "binding": "opencode-go-haiku-messages",
            "upstream_path": "/v1/messages",
            "upstream_model": "claude-haiku-5-5",
            "protocol": "messages",
        }
    if case == "codex-chat":
        return {
            "native": False,
            "model": "claude-haiku-5-5",
            "provider": "openrouter",
            "binding": "openrouter-haiku-chat",
            "upstream_path": "/v1/chat/completions",
            "upstream_model": "claude-haiku-5-5",
            "protocol": "chat",
        }
    raise RuntimeError(f"unknown_codex_case:{case}")


def declared_custom_exec_in_responses_wire(body):
    """Check whether Codex declared a custom `exec` tool directly or inside `namespace: functions`."""
    candidates = list(body.get("tools", []))
    for item in body.get("input", []):
        if item.get("type") == "additional_tools":
            candidates.extend(item.get("tools", []))
    for tool in candidates:
        if tool.get("type") == "namespace" and tool.get("name") == "functions":
            for leaf in tool.get("tools", []):
                if leaf.get("name") == "exec":
                    return leaf
        if tool.get("name") == "exec":
            return tool
    return None


def declared_multi_agent_in_responses_wire(body):
    """Check whether Codex declared `multi_agent_v1::close_agent` on the incoming wire."""
    candidates = list(body.get("tools", []))
    for item in body.get("input", []):
        if item.get("type") == "additional_tools":
            candidates.extend(item.get("tools", []))
    for tool in candidates:
        if tool.get("type") == "namespace" and tool.get("name") == "multi_agent_v1":
            for leaf in tool.get("tools", []):
                if leaf.get("name") == "close_agent":
                    return leaf
    return None


def find_aliased_multi_agent_tool(tools):
    """Locate the `mcns_*` alias for `multi_agent_v1::close_agent` on Messages or Chat wires."""
    for tool in tools:
        fn = tool.get("function", tool)
        name = fn.get("name") or ""
        desc = fn.get("description") or ""
        if (
            name.startswith("mcns_")
            and "multi_agent_v1_close_agent" in name
            and "[namespace: multi_agent_v1]" in desc
        ):
            return name
    return None


def find_wrapped_exec_name_in_responses(body):
    """Find the wire name of `functions::exec` after Responses or alias projection."""
    candidates = list(body.get("tools", []))
    for item in body.get("input", []):
        if item.get("type") == "additional_tools":
            candidates.extend(item.get("tools", []))
    for tool in candidates:
        if tool.get("type") == "namespace" and tool.get("name") == "functions":
            for leaf in tool.get("tools", []):
                if leaf.get("name") == "exec":
                    return "exec"
        if "[namespace: functions]" in (tool.get("description") or "") and tool.get("name"):
            return tool["name"]
    return None


def responses_stage(body):
    for item in body.get("input", []):
        if item.get("type") in ("function_call_output", "custom_tool_call_output"):
            return 2
    return 1


def messages_stage(body):
    for msg in body.get("messages", []):
        content = msg.get("content")
        if isinstance(content, list) and any(
            isinstance(block, dict) and block.get("type") == "tool_result" for block in content
        ):
            return 2
    return 1


def chat_stage(body):
    for msg in body.get("messages", []):
        if msg.get("role") == "tool":
            return 2
    return 1


def responses_events(phase, model, wire_exec_name="exec"):
    items = []
    if phase == 1:
        items.append(
            {
                "id": "ct_responses_1",
                "type": "custom_tool_call",
                "call_id": CODEX_CALL,
                "name": wire_exec_name,
                "input": CODE_MODE_PAYLOAD,
            }
        )
    else:
        items.append(
            {
                "id": f"msg_responses_{phase}",
                "type": "message",
                "role": "assistant",
                "phase": "final_answer",
                "status": "completed",
                "content": [{"type": "output_text", "text": TEXT, "annotations": [], "logprobs": []}],
            }
        )
    response = {
        "id": f"resp_codex_{phase}",
        "object": "response",
        "created_at": 1,
        "model": model,
        "status": "in_progress",
        "output": [],
    }
    events = [{"type": "response.created", "response": response}]
    for index, item in enumerate(items):
        kind = item["type"]
        opening = copy.deepcopy(item)
        if kind == "custom_tool_call":
            opening["input"] = ""
        else:
            opening["status"] = "in_progress"
            opening["content"] = []
        events.append({"type": "response.output_item.added", "output_index": index, "item": opening})
        owner = {"output_index": index, "item_id": item["id"]}
        if kind == "custom_tool_call":
            events += [
                {**owner, "type": "response.custom_tool_call_input.delta", "delta": item["input"]},
                {**owner, "type": "response.custom_tool_call_input.done", "input": item["input"]},
            ]
        else:
            events += [
                {
                    **owner,
                    "type": "response.content_part.added",
                    "content_index": 0,
                    "part": {"type": "output_text", "text": "", "annotations": []},
                },
                {**owner, "type": "response.output_text.delta", "content_index": 0, "delta": TEXT, "logprobs": []},
                {**owner, "type": "response.output_text.done", "content_index": 0, "text": TEXT, "logprobs": []},
                {
                    **owner,
                    "type": "response.content_part.done",
                    "content_index": 0,
                    "part": item["content"][0],
                },
            ]
        events.append({"type": "response.output_item.done", "output_index": index, "item": item})
    events.append(
        {
            "type": "response.completed",
            "response": {
                **response,
                "status": "completed",
                "output": items,
                "usage": {"input_tokens": 11, "output_tokens": 7, "total_tokens": 18},
            },
        }
    )
    return "".join(
        f"event: {ev['type']}\ndata: {json.dumps({**ev, 'sequence_number': i})}\n\n"
        for i, ev in enumerate(events)
    ).encode()


def messages_events(phase, wire_name, model):
    values = [
        {
            "type": "message_start",
            "message": {
                "id": f"msg_codex_{phase}",
                "type": "message",
                "role": "assistant",
                "model": model,
                "content": [],
                "stop_reason": None,
                "stop_sequence": None,
                "usage": {"input_tokens": 11, "output_tokens": 0},
            },
        }
    ]
    if phase == 1:
        payload = json.dumps(NS_ARGS)
        values += [
            {
                "type": "content_block_start",
                "index": 0,
                "content_block": {"type": "tool_use", "id": NS_CALL, "name": wire_name, "input": {}},
            },
            {
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "input_json_delta", "partial_json": payload},
            },
            {"type": "content_block_stop", "index": 0},
            {
                "type": "message_delta",
                "delta": {"stop_reason": "tool_use", "stop_sequence": None},
                "usage": {"output_tokens": 9},
            },
        ]
    else:
        values += [
            {"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}},
            {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": TEXT}},
            {"type": "content_block_stop", "index": 0},
            {
                "type": "message_delta",
                "delta": {"stop_reason": "end_turn", "stop_sequence": None},
                "usage": {"output_tokens": 5},
            },
        ]
    values.append({"type": "message_stop"})
    return "".join(f"event: {v['type']}\ndata: {json.dumps(v)}\n\n" for v in values).encode()


def chat_events(phase, wire_name, model):
    def chunk(delta, finish=None):
        return {
            "id": f"chat_codex_{phase}",
            "object": "chat.completion.chunk",
            "created": 1,
            "model": model,
            "choices": [{"index": 0, "delta": delta, "finish_reason": finish}],
        }

    values = [chunk({"role": "assistant"})]
    if phase == 1:
        payload = json.dumps(NS_ARGS)
        values.append(
            chunk(
                {
                    "tool_calls": [
                        {
                            "index": 0,
                            "id": NS_CALL,
                            "type": "function",
                            "function": {"name": wire_name, "arguments": payload},
                        }
                    ]
                }
            )
        )
        values.append(chunk({}, finish="tool_calls"))
    else:
        values.append(chunk({"content": TEXT}))
        values.append(chunk({}, finish="stop"))
    values.append(
        {
            "id": f"chat_codex_{phase}",
            "object": "chat.completion.chunk",
            "created": 1,
            "model": model,
            "choices": [],
            "usage": {"prompt_tokens": 11, "completion_tokens": 7, "total_tokens": 18},
        }
    )
    return ("".join(f"data: {json.dumps(v)}\n\n" for v in values) + "data: [DONE]\n\n").encode()


class CodexState:
    def __init__(self, case):
        self.case = case
        self.profile = codex_profile(case)
        self.native = self.profile["native"]
        self.endpoint_path = "/v1/responses"
        self.upstream_endpoint_path = self.profile["upstream_path"]
        self.lock = threading.Lock()
        self.ingress = []
        self.attempts = []
        self.deliveries = []
        self.errors = []
        self.wire_tool_name = None
        self.gateway = None
        self.accepted = threading.Event()
        self.retry_after = "0"

    def observe_wire(self, _raw):
        pass

    def observe_failure_hint(self, _before):
        pass

    def incoming(self, headers, body):
        auth = headers.get("authorization")
        check(auth == f"Bearer {KEY}", "codex_ingress_auth")
        phase = responses_stage(body)
        if self.profile["protocol"] == "responses":
            if phase == 1:
                check(declared_custom_exec_in_responses_wire(body) is not None, "codex_missing_exec_tool")
            else:
                check(
                    any(
                        item.get("type") == "custom_tool_call"
                        and item.get("call_id") == CODEX_CALL
                        and item.get("name") == "exec"
                        and item.get("namespace") is None
                        for item in body.get("input", [])
                    ),
                    "codex_turn2_missing_unqualified_custom_call",
                )
                check(
                    any(
                        item.get("type") == "custom_tool_call_output" and item.get("call_id") == CODEX_CALL
                        for item in body.get("input", [])
                    ),
                    "codex_turn2_missing_custom_output",
                )
        else:
            if phase == 1:
                check(declared_multi_agent_in_responses_wire(body) is not None, "codex_missing_multi_agent_tool")
            else:
                check(
                    any(
                        item.get("type") == "function_call"
                        and item.get("call_id") == NS_CALL
                        and item.get("name") == "close_agent"
                        and item.get("namespace") == "multi_agent_v1"
                        for item in body.get("input", [])
                    ),
                    "codex_turn2_missing_restored_namespaced_call",
                )
                check(
                    any(
                        item.get("type") == "function_call_output" and item.get("call_id") == NS_CALL
                        for item in body.get("input", [])
                    ),
                    "codex_turn2_missing_function_output",
                )
        with self.lock:
            self.ingress.append(phase)
            check(self.ingress == [1, 2][: len(self.ingress)], "codex_ingress_phase_order")

    def upstream(self, headers, body):
        protocol = self.profile["protocol"]
        model = self.profile["upstream_model"]
        if protocol == "messages":
            check(headers.get("x-api-key") == "synthetic-client-primary", "codex_upstream_auth")
        else:
            check(headers.get("authorization") == "Bearer synthetic-client-primary", "codex_upstream_auth")
        if protocol == "responses":
            phase = responses_stage(body)
            wire_name = find_wrapped_exec_name_in_responses(body)
            check(wire_name == "exec", "responses_missing_exec_declaration")
            if phase == 2:
                check(
                    any(
                        item.get("type") == "custom_tool_call"
                        and item.get("call_id") == CODEX_CALL
                        and item.get("name") == "exec"
                        and item.get("namespace") is None
                        for item in body.get("input", [])
                    ),
                    "responses_turn2_unqualified_custom_call",
                )
            with self.lock:
                self.wire_tool_name = wire_name
                self.attempts.append(phase)
                check(self.attempts == [1, 2][: len(self.attempts)], "responses_attempt_order")
            return 200, responses_events(phase, model, wire_name), "text/event-stream"

        if protocol == "messages":
            phase = messages_stage(body)
            wire_name = find_aliased_multi_agent_tool(body.get("tools", []))
            check(wire_name is not None and wire_name.startswith("mcns_"), "messages_missing_namespace_alias")
            check(
                not any(
                    isinstance(t, dict) and t.get("type") in ("namespace", "custom")
                    for t in body.get("tools", [])
                ),
                "messages_leaked_namespace_or_custom",
            )
            if phase == 2:
                tool_uses = [
                    b
                    for m in body.get("messages", [])
                    for b in (m.get("content") or [])
                    if isinstance(b, dict) and b.get("type") == "tool_use"
                ]
                check(
                    any(
                        b.get("id") == NS_CALL
                        and b.get("name") == wire_name
                        and b.get("input") == NS_ARGS
                        for b in tool_uses
                    ),
                    "messages_turn2_aliased_tool_use",
                )
            with self.lock:
                self.wire_tool_name = wire_name
                self.attempts.append(phase)
                check(self.attempts == [1, 2][: len(self.attempts)], "messages_attempt_order")
            return 200, messages_events(phase, wire_name, model), "text/event-stream"

        phase = chat_stage(body)
        wire_name = find_aliased_multi_agent_tool(body.get("tools", []))
        check(wire_name is not None and wire_name.startswith("mcns_"), "chat_missing_namespace_alias")
        check(
            not any(
                isinstance(t, dict) and t.get("type") in ("namespace", "custom")
                for t in body.get("tools", [])
            ),
            "chat_leaked_namespace_or_custom",
        )
        if phase == 2:
            calls = [
                c
                for m in body.get("messages", [])
                for c in (m.get("tool_calls") or [])
                if isinstance(c, dict)
            ]
            check(
                any(
                    c.get("id") == NS_CALL
                    and c.get("function", {}).get("name") == wire_name
                    and json.loads(c.get("function", {}).get("arguments", "{}")) == NS_ARGS
                    for c in calls
                ),
                "chat_turn2_aliased_tool_call",
            )
        with self.lock:
            self.wire_tool_name = wire_name
            self.attempts.append(phase)
            check(self.attempts == [1, 2][: len(self.attempts)], "chat_attempt_order")
        return 200, chat_events(phase, wire_name, model), "text/event-stream"

    def observe_response(self, _body, status, content_type, _payload):
        check(status == 200, f"codex_delivery_status_{status}")
        check(
            content_type is not None and content_type.split(";")[0] == "text/event-stream",
            "codex_delivery_sse",
        )
        with self.lock:
            self.deliveries.append(status)

    def finish(self):
        check(not self.errors, "codex_authority_errors:" + ",".join(self.errors))
        check(self.ingress == [1, 2], f"codex_ingress_incomplete:{self.ingress}")
        check(self.attempts == [1, 2], f"codex_attempts_incomplete:{self.attempts}")
        check(self.deliveries == [200, 200], f"codex_deliveries_incomplete:{self.deliveries}")


def codex_configuration(root, origin, model):
    codex_home = root / "codex-home"
    codex_home.mkdir(mode=0o700)
    # Disable unscheduled hosted web_search; use model defaults (gpt-6.2-sol enables Code Mode natively,
    # while non-OpenAI models expose multi_agent_v1 functions without grammar-constrained custom tools).
    config = (
        f'model = "{model}"\n'
        'model_provider = "morphiecore"\n'
        'web_search = "disabled"\n'
        "\n"
        "[model_providers.morphiecore]\n"
        'name = "MorphieCore"\n'
        f'base_url = "{origin}/v1"\n'
        'env_key = "INGRESS_KEY"\n'
        'wire_api = "responses"\n'
        "supports_websockets = false\n"
        "request_max_retries = 0\n"
        "stream_max_retries = 0\n"
    )
    (codex_home / "config.toml").write_text(config, encoding="utf-8")
    return codex_home


def check_codex_events(output):
    events = [json.loads(line) for line in output.decode("utf-8").splitlines() if line.strip()]
    texts = []
    for event in events:
        if event.get("type") == "item.completed":
            item = event.get("item", {})
            if item.get("type") == "agent_message" and isinstance(item.get("text"), str):
                texts.append(item["text"])
        if event.get("type") == "error":
            raise RuntimeError(f"codex_error_event:{event.get('message')}")
    check(any(TEXT in t for t in texts), f"codex_final_text_missing:{texts}")


def main():
    case, root, gateway, binary = sys.argv[1:5]
    root = Path(root)
    profile = codex_profile(case)
    env = environment(root)
    version = subprocess.check_output([binary, "--version"], env=env, text=True, timeout=10).strip()
    check_client_version(case, version)
    work = root / "work"
    work.mkdir(mode=0o700)
    state = CodexState(case)

    with ExitStack() as stack:
        upstream_origin = stack.enter_context(server(state, False))
        ingress_origin = stack.enter_context(server(state, True))
        gateway_configuration(
            root,
            profile["native"],
            upstream_origin,
            model=profile["model"],
            provider=profile["provider"],
            binding=profile["binding"],
        )
        child = stack.enter_context(
            Child(
                [gateway, "--config", str(root / "gateway.json"), "--credentials-dir", str(root / "credentials")],
                env,
                work,
            )
        )
        failed = False
        try:
            ready = child.first_line.get(timeout=10).decode().strip()
            check(ready.startswith("MorphieCore listening on http://127.0.0.1:"), f"gateway_startup:{ready}")
            state.gateway = ready.removeprefix("MorphieCore listening on ")
            codex_home = codex_configuration(root, ingress_origin, profile["model"])
            codex_env = {**env, "CODEX_HOME": str(codex_home)}
            with Child(
                [
                    binary,
                    "exec",
                    "--json",
                    "--ephemeral",
                    "--skip-git-repo-check",
                    "--ignore-rules",
                    "--dangerously-bypass-approvals-and-sandbox",
                    "-C",
                    str(work),
                    "-m",
                    profile["model"],
                    "Call the available tool once, then reply with the exact marker.",
                ],
                codex_env,
                work,
            ) as cli:
                output = cli.wait(timeout=60, expected_status=0)
            check_codex_events(output)
            state.finish()
        except BaseException:
            failed = True
            print(
                json.dumps(
                    {
                        "codex_failure": {
                            "case": case,
                            "ingress": state.ingress,
                            "attempts": state.attempts,
                            "deliveries": state.deliveries,
                            "errors": state.errors,
                        }
                    }
                ),
                file=sys.stderr,
            )
            raise
        finally:
            shutdown_gateway(child, failed=failed)

    print(json.dumps({"case": case, "version": version, "wire_tool": state.wire_tool_name}))


if __name__ == "__main__":
    main()
