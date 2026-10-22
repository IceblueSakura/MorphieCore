"""Offline constrained-profile, stream-terminal and history oracles; no Provider."""
import copy
import base64
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest

import messages_capabilities_live as live


def sealed(format, payload):
    return "mc1." + base64.urlsafe_b64encode(json.dumps({
        "o": "synthetic-issuer", "f": format, "d": payload
    }).encode()).decode().rstrip("=")


class Reply:
    def __init__(self, value):
        self.value = value

    def model_dump(self, **_):
        return copy.deepcopy(self.value)


class Stream:
    def __init__(self, value, complete=True):
        self.value = value
        self.events = [SimpleNamespace(type="message_start"),
                       SimpleNamespace(type="message_delta", delta=SimpleNamespace(stop_reason=value["stop_reason"]))]
        if complete:
            self.events.append(SimpleNamespace(type="message_stop"))

    def __iter__(self):
        return iter(self.events)

    def get_final_message(self):
        return Reply(self.value)

    def __enter__(self):
        return self

    def __exit__(self, *_):
        pass


class Client:
    def __init__(self):
        self.messages = self
        self.seen = []
        text = lambda value: [{"type": "text", "text": value}]
        call = lambda identity, name, record: [{"type": "tool_use", "id": identity,
                                                "name": name, "input": {"record": record}}]
        self.replies = [
            {"stop_reason": "end_turn", "content": [{"type": "thinking", "thinking": "synthetic thought",
                "signature": sealed("anthropic-thinking", "synthetic-signature")},
                {"type": "redacted_thinking", "data": sealed("anthropic-redacted-thinking", "synthetic-redacted")},
                *text('{"record":71,"ok":true}')]},
            {"stop_reason": "end_turn", "content": text('{"record":82,"ok":true}')},
            {"stop_reason": "max_tokens", "content": text("{")},
            {"stop_reason": "tool_use", "content": call("issued-required", "lookup", 71)},
            {"stop_reason": "end_turn", "content": text("cedar-71")},
            {"stop_reason": "tool_use", "content": call("issued-named", "inspect", 82)},
            {"stop_reason": "end_turn", "content": text("birch-82")},
        ]

    def take(self, params):
        self.seen.append(copy.deepcopy(params))
        return self.replies[len(self.seen) - 1]

    def create(self, **params):
        return Reply(self.take(params))

    def stream(self, **params):
        return Stream(self.take(params))


class ConstrainedProfileOracles(unittest.TestCase):
    def gateway(self):
        return SimpleNamespace(requests=[], budget=7)

    def test_pinned_sdk_keeps_schema_and_effort_in_actual_json_and_sse_requests(self):
        seen = []
        def handle(request):
            body = json.loads(request.content)
            seen.append(body)
            message = {"id": "synthetic", "type": "message", "role": "assistant", "model": live.MODEL,
                       "content": [{"type": "thinking", "thinking": "synthetic thought",
                                    "signature": sealed("anthropic-thinking", "synthetic-signature")},
                                   {"type": "text", "text": "synthetic"}], "stop_reason": "end_turn",
                       "stop_sequence": None, "usage": {"input_tokens": 1, "output_tokens": 1}}
            if not body.get("stream"):
                return live.httpx.Response(200, json=message)
            frames = [{"type": "message_start", "message": {**message, "content": [], "stop_reason": None}}]
            for index, block in enumerate(message["content"]):
                frames += [{"type": "content_block_start", "index": index, "content_block": block},
                           {"type": "content_block_stop", "index": index}]
            frames += [
                {"type": "message_delta", "delta": {"stop_reason": "end_turn", "stop_sequence": None},
                 "usage": {"output_tokens": 1}}, {"type": "message_stop"},
            ]
            raw = "".join(f"event: {f['type']}\ndata: {json.dumps(f)}\n\n" for f in frames)
            return live.httpx.Response(200, content=raw.encode(), headers={"content-type": "text/event-stream"})
        with live.httpx.Client(transport=live.httpx.MockTransport(handle), trust_env=False) as http:
            with live.anthropic.Anthropic(api_key="synthetic", base_url="http://127.0.0.1",
                                         max_retries=0, http_client=http) as client:
                messages = [live.user("synthetic"), copy.deepcopy(live.EFFORT)]
                delivered = []
                for streamed in (False, True):
                    reply = live.sdk_generate(self.gateway(), client,
                        live.profile(messages, structured=True), streamed, "synthetic")
                    delivered.append(reply)
                    messages = messages + [{"role": "assistant", "content": reply["content"]},
                                           live.user("next"), copy.deepcopy(live.EFFORT)]
        self.assertEqual(len(seen), 2)
        for body in seen:
            self.assertEqual(body["output_config"], {"effort": "high",
                "format": {"type": "json_schema", "schema": live.OUTPUT_SCHEMA}})
            self.assertEqual(body["thinking"], live.THINKING)
            self.assertEqual(body["messages"][1], live.EFFORT)
        self.assertEqual(seen[1]["messages"][2]["content"], delivered[0]["content"])

    def test_failure_report_counts_attempts_without_business_or_routing_fields(self):
        with tempfile.TemporaryDirectory() as directory:
            gateway = SimpleNamespace(root=Path(directory), requests=[{}])
            (gateway.root / "diagnostics.jsonl").write_text(json.dumps({
                "event": "request", "outcome": "error", "stage": "upstream_head",
                "upstream_status": 400, "candidates": [{"endpoint": "private-marker"}],
                "body": "private-marker", "request_id": "private-marker"}))
            report = live.failure_summary(gateway)
            self.assertEqual(report["client_requests_started"], 1)
            self.assertEqual(report["upstream_attempts"], 1)
            self.assertEqual(report["diagnostics"][0]["upstream_status"], 400)
            self.assertNotIn("private-marker", json.dumps(report))

    def test_seven_request_plan_keeps_strict_controls_and_model_history(self):
        gateway, client = self.gateway(), Client()
        histories = live.structured(gateway, client)
        histories.update(live.forced_tools(gateway, client))
        prepared = copy.deepcopy(gateway.requests)
        for index in (0, 3, 4):
            prepared[index].pop("stream")
        prepared[1]["messages"][2]["content"][0]["signature"] = "synthetic-signature"
        prepared[1]["messages"][2]["content"][1]["data"] = "synthetic-redacted"
        live.verify_wires(prepared, histories)
        self.assertEqual(histories[1], prepared[1]["messages"])
        self.assertEqual(len(client.seen), 7)
        self.assertEqual([p["stream"] for p in gateway.requests], [False, True, True, False, False, True, True])
        self.assertEqual(client.seen[1]["messages"][2]["content"], client.replies[0]["content"])
        self.assertEqual(client.seen[4]["messages"][3]["content"][0]["tool_use_id"], "issued-required")
        self.assertEqual(client.seen[6]["messages"][3]["content"][0]["tool_use_id"], "issued-named")
        self.assertEqual([p["extra_headers"]["x-morphiecore-conversation-id"] for p in client.seen],
                         ["constrained-json", "constrained-json", "constrained-truncation",
                          "forced-required", "forced-required", "forced-named", "forced-named"])
        for mutate, error in [
            (lambda b: b[5]["tool_choice"].update(type="auto"), "forced_tool_choice_changed"),
            (lambda b: b[3]["tools"][0].update(strict=False), "forced_strictness_or_schema_changed"),
            (lambda b: b[0]["thinking"].update(type="disabled"), "constrained_thinking_or_root_effort_changed"),
            (lambda b: b[0].update(stream=None), "constrained_stream_changed"),
            (lambda b: b[1].update(stream=1), "constrained_stream_changed"),
        ]:
            changed = copy.deepcopy(prepared)
            mutate(changed)
            with self.assertRaisesRegex(live.GateError, f"^{error}$"):
                live.verify_wires(changed, histories)
        # The oracle comes from returned replies, not from these mutated prepared bodies.
        for mutate in [
            lambda b: b[1]["messages"].pop(2),
            lambda b: b[1]["messages"][2]["content"].reverse(),
            lambda b: b[1]["messages"][2]["content"][0].update(thinking="changed"),
            lambda b: b[1]["messages"][2]["content"][0].update(signature="changed"),
            lambda b: b[1]["messages"][2]["content"][1].update(data="changed"),
            lambda b: b[1]["messages"][2]["content"][2].update(text="changed"),
            lambda b: b[4]["messages"][2]["content"][0].update(name="changed", input={"record": 82}),
            lambda b: b[6]["messages"][3]["content"][0].update(content="changed"),
        ]:
            changed = copy.deepcopy(prepared)
            mutate(changed)
            with self.assertRaisesRegex(live.GateError, "^prepared_returned_history_changed$"):
                live.verify_wires(changed, histories)
        changed = copy.deepcopy(prepared)
        for index in (4, 6):
            changed[index]["messages"][2]["content"][0]["id"] = "rewritten"
            changed[index]["messages"][3]["content"][0]["tool_use_id"] = "rewritten"
        with self.assertRaisesRegex(live.GateError, "^prepared_returned_history_changed$"):
            live.verify_wires(changed, histories)
        for index in (0, 2, 3, 5):
            changed = copy.deepcopy(prepared)
            changed[index]["messages"][0]["content"][0]["text"] = "Use inspect for record 71."
            with self.assertRaisesRegex(live.GateError, "^prepared_initial_messages_changed$"):
                live.verify_wires(changed, histories)

    def test_conflicting_prompts_require_schema_values_without_coercing_types(self):
        for value in (93, 94, True, 71.0, "71"):
            gateway, client = self.gateway(), Client()
            client.replies[0]["content"] = [{"type": "text", "text": json.dumps({"record": value, "ok": True})}]
            with self.assertRaisesRegex(live.GateError, "^structured_json_answer$"):
                live.structured(gateway, client)
            gateway, client = self.gateway(), Client()
            client.replies = client.replies[3:]
            client.replies[0]["content"][0]["input"] = {"record": value}
            with self.assertRaisesRegex(live.GateError, "^strict_tool_schema_answer$"):
                live.forced_tools(gateway, client)
        gateway, client = self.gateway(), Client()
        live.structured(gateway, client)
        for index, disallowed in [(0, "93"), (1, "94")]:
            self.assertIn(disallowed, gateway.requests[index]["messages"][-2]["content"][0]["text"])
        gateway, client = self.gateway(), Client()
        client.replies = client.replies[3:]
        # Either enum alternative is valid; feedback follows the actual model argument.
        client.replies[0]["content"][0]["input"] = {"record": 82}
        client.replies[1]["content"][0]["text"] = "birch-82"
        live.forced_tools(gateway, client)
        self.assertEqual(gateway.requests[1]["messages"][3]["content"][0]["content"], "birch-82")

    def test_history_envelopes_fail_closed_without_rewriting_client_history(self):
        for value in ("mca1.unassessed", "mc1.!!!", sealed("openai-responses", "synthetic"),
                      sealed("anthropic-thinking", "")):
            messages = [{"role": "assistant", "content": [
                {"type": "thinking", "thinking": "synthetic", "signature": value}]}]
            before = copy.deepcopy(messages)
            with self.assertRaises(live.GateError):
                live.native_history(messages)
            self.assertEqual(messages, before)

    def test_schema_boolean_never_accepts_numeric_one_in_json_or_sse(self):
        for turn, record, error in [(0, 71, "structured_json_answer"), (1, 82, "structured_sse_history_answer")]:
            gateway, client = self.gateway(), Client()
            client.replies[turn]["content"] = [{"type": "text", "text": json.dumps({"record": record, "ok": 1})}]
            with self.assertRaisesRegex(live.GateError, f"^{error}$"):
                live.structured(gateway, client)

    def test_truncation_and_missing_stream_terminal_never_become_success(self):
        gateway, client = self.gateway(), Client()
        client.replies[2]["stop_reason"] = "end_turn"
        with self.assertRaisesRegex(live.GateError, "^truncation_not_observed$"):
            live.structured(gateway, client)
        class BrokenClient:
            def __init__(self):
                self.messages = self

            def stream(self, **_):
                return Stream({"stop_reason": "end_turn", "content": []}, complete=False)
        with self.assertRaisesRegex(live.GateError, "^sdk_stream_terminal$"):
            live.sdk_generate(self.gateway(), BrokenClient(), live.profile([live.user("synthetic")]), True, "test")


if __name__ == "__main__":
    unittest.main()
