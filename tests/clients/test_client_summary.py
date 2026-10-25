"""Independent summary/retained-history failure oracles, not client round trips."""
import copy
import http.client
import json
from pathlib import Path
import unittest

import summary
from recovery import CALL, PLAN, REDACTED, SIGNATURE, TEXT, server, urlsplit


class ClientSummaryOracles(unittest.TestCase):
    path = Path("/synthetic/sample.txt")

    def base(self, native):
        tool = {"name": "read", "input_schema": {"properties": {"path": {"type": "string"}}},
                "strict": True, "eager_input_streaming": True}
        if not native:
            tool = {"type": "function", "function": {"name": "read", "parameters": tool["input_schema"]}}
        body = {"model": "claude-haiku-5-5" if native else "qwen3.8-flash",
                "stream": True, "tools": [tool], "messages": []}
        if native:
            body.update(max_tokens=2048, thinking={"type": "adaptive", "display": "summarized",
                "block_binding": {"prefix_mismatch_behavior": "drop_block"}}, output_config={"effort": "low"})
            body["messages"] = [{"role": "system", "content": [], "output_config": {"effort": "low"}}]
        else:
            body.update(max_completion_tokens=2048, reasoning_effort="low")
        return body

    def standalone(self, native):
        body = self.base(native)
        body["tools"] = []
        body["messages"] = [{"role": "user", "content":
            f"<conversation>\n[User]: {summary.READ}: Read {self.path}, then answer.\n\n"
            f'[Assistant tool calls]: read(path={json.dumps(str(self.path))})\n\n'
            f"[Tool result]: {TEXT}\n\n[Assistant]: {TEXT}\n</conversation>\n{summary.FOCUS}"}]
        if native:
            body["cache_control"] = {"type": "ephemeral", "ttl": "5m"}
            body["messages"].insert(0, {"role": "system", "content": [], "output_config": {"effort": "low"}})
        return body

    def rebuilt(self, native):
        body = self.base(native)
        body["messages"] = [
            {"role": "user", "content": summary.SUMMARY},
            {"role": "user", "content": summary.RECENT_PROMPT},
            {"role": "assistant", "content": [{"type": "text", "text": TEXT}] if native else TEXT},
            {"role": "user", "content": summary.AFTER},
        ]
        if native:
            body["messages"] += [
                {"role": "system", "content": [], "output_config": {"effort": "low"}},
                {"role": "assistant", "content": [
                    {"type": "thinking", "thinking": PLAN, "signature": SIGNATURE},
                    {"type": "redacted_thinking", "data": REDACTED},
                    {"type": "tool_use", "id": CALL, "name": "read", "input": {"path": str(self.path)}}]},
                {"role": "user", "content": [{"type": "tool_result", "tool_use_id": CALL,
                                               "content": TEXT}]},
            ]
        else:
            body["messages"] += [
                {"role": "assistant", "reasoning_content": PLAN, "tool_calls": [
                    {"id": CALL, "type": "function", "function": {
                        "name": "read", "arguments": json.dumps({"path": str(self.path)})}}]},
                {"role": "tool", "tool_call_id": CALL, "content": TEXT},
            ]
        return body

    def test_summary_requires_standalone_serialized_history_and_bounded_controls(self):
        for native in [False, True]:
            original = self.standalone(native)
            self.assertEqual(summary.classify(original), "summary")
            summary.assert_summary(original, native, self.path)
            for key, value, error in [
                ("tools", [{}], "summary_declares_tools"),
                ("max_tokens" if native else "max_completion_tokens", 4096, "summary_output_cap"),
                ("messages", [{"role": "user", "content": summary.FOCUS}], "summary_missing_serialized_history"),
            ]:
                body = {**original, key: value}
                with self.subTest(native=native, error=error), self.assertRaisesRegex(RuntimeError, f"^{error}$"):
                    summary.assert_summary(body, native, self.path)
            leaked = copy.deepcopy(original)
            leaked["messages"][-1]["content"] += SIGNATURE
            with self.assertRaisesRegex(RuntimeError, "^summary_serialized_reasoning$"):
                summary.assert_summary(leaked, native, self.path)
            for marker in ["[Assistant tool calls]:", "[Tool result]:"]:
                missing = copy.deepcopy(original)
                missing["messages"][-1]["content"] = missing["messages"][-1]["content"].replace(marker, "")
                with self.assertRaisesRegex(RuntimeError, "^summary_serialized_exchange$"):
                    summary.assert_summary(missing, native, self.path)
            included = copy.deepcopy(original)
            included["messages"][-1]["content"] = included["messages"][-1]["content"].replace(
                "</conversation>", summary.RECENT + "\n</conversation>")
            with self.assertRaisesRegex(RuntimeError, "^summary_serialized_exchange$"):
                summary.assert_summary(included, native, self.path)
            if native:
                original["messages"].pop(0)
                with self.assertRaisesRegex(RuntimeError, "^summary_ordered_effort$"):
                    summary.assert_summary(original, native, self.path)

    def test_rebuilt_history_requires_order_and_exact_new_tool_feedback(self):
        for native in [False, True]:
            body = self.rebuilt(native)
            self.assertEqual(summary.classify(body), "rebuilt-feedback")
            summary.assert_rebuilt(body, "rebuilt-feedback", native, self.path)
            reordered = copy.deepcopy(body)
            reordered["messages"][2:4] = reversed(reordered["messages"][2:4])
            with self.subTest(native=native), self.assertRaisesRegex(RuntimeError, "^rebuilt_history_order$"):
                summary.assert_rebuilt(reordered, "rebuilt-feedback", native, self.path)
            corrupted = copy.deepcopy(body)
            if native:
                corrupted["messages"][5]["content"][0]["signature"] = "changed"
            else:
                corrupted["messages"][4]["tool_calls"][0]["function"]["arguments"] = "{}"
            error = "rebuilt_signed_tool_history" if native else "rebuilt_chat_tool_call"
            with self.assertRaisesRegex(RuntimeError, f"^{error}$"):
                summary.assert_rebuilt(corrupted, "rebuilt-feedback", native, self.path)
            if native:
                for remove in [False, True]:
                    corrupted = copy.deepcopy(body)
                    if remove:
                        corrupted["messages"].pop(4)
                    else:
                        corrupted["messages"][4]["output_config"]["effort"] = "high"
                    with self.assertRaisesRegex(RuntimeError, "^rebuilt_ordered_effort$"):
                        summary.assert_rebuilt(corrupted, "rebuilt-feedback", native, self.path)

    def test_cache_marker_detection_never_parses_business_strings(self):
        self.assertFalse(summary.has_cache_marker({"content": '{"cache_control": "business data"}'}))
        self.assertTrue(summary.has_cache_marker({"content": [{"cache_control": {"type": "ephemeral"}}]}))

    def test_split_summary_keeps_only_the_selected_turn_prefix(self):
        body = self.standalone(True)
        body["messages"][-1]["content"] = (
            f"# Conversation\n[User]: {summary.RECENT_PROMPT}\n\n"
            "# Instructions\n## Original Request\nSummarize only earlier context.\n\n"
            f"Additional focus: {summary.FOCUS}")
        self.assertEqual(summary.classify(body), "prefix-summary")
        summary.assert_summary(body, True, self.path, prefix=True)
        for leak in [summary.FOCUS, str(self.path), "[Assistant]: fabricated continuation"]:
            changed = copy.deepcopy(body)
            changed["messages"][-1]["content"] += leak
            with self.assertRaisesRegex(RuntimeError, "^prefix_summary_serialized_exchange$"):
                summary.assert_summary(changed, True, self.path, prefix=True)
        rebuilt = self.rebuilt(True)
        rebuilt["messages"].pop(1)
        rebuilt["messages"][0]["content"] += (
            "\n\n---\n\n**Turn Context (split turn):**\n\n" + summary.PREFIX_SUMMARY)
        summary.assert_rebuilt(rebuilt, "rebuilt-feedback", True, self.path, split=True)
        rebuilt["messages"][0]["content"] += summary.RECENT_PROMPT
        with self.assertRaisesRegex(RuntimeError, "^rebuilt_split_prefix$"):
            summary.assert_rebuilt(rebuilt, "rebuilt-feedback", True, self.path, split=True)

    def test_summary_retry_is_exact_and_has_no_extra_attempt_budget(self):
        state = summary.State(False, self.path, "retry")
        state.attempts = [(phase, None) for phase in summary.ORDER[:3]]
        headers = {"authorization": "Bearer synthetic-client-primary"}
        body = self.standalone(False)
        self.assertEqual(state.upstream(headers, body)[0], 503)
        self.assertEqual(state.upstream(headers, copy.deepcopy(body))[0], 200)
        # A third transmission fails at the count/order guard, not a permissive retry loop.
        with self.assertRaisesRegex(RuntimeError, "^summary_attempt_order$"):
            state.upstream(headers, body)
        changed = summary.State(False, self.path, "retry")
        changed.attempts = [(phase, None) for phase in summary.ORDER[:3]]
        changed.upstream(headers, body)
        body["messages"][-1]["content"] += "\nchanged retry"
        with self.assertRaisesRegex(RuntimeError, "^summary_retry_changed_request$"):
            changed.upstream(headers, body)

    def test_final_snapshot_includes_late_request_failure(self):
        state = summary.State(False, self.path)
        state.ingress = [(p, None if p == "summary" else "client") for p in summary.ORDER]
        state.attempts = [(p, None) for p in summary.ORDER]
        state.finish("client")
        with server(state, True) as origin:
            address = urlsplit(origin)
            connection = http.client.HTTPConnection(address.hostname, address.port, timeout=3)
            try:
                connection.request("POST", "/v1/chat/completions", json.dumps(self.rebuilt(False)),
                                   {"session_id": "client"})
                response = connection.getresponse()
                self.assertEqual(response.status, 400)
                response.read()
            finally:
                connection.close()
        with self.assertRaisesRegex(RuntimeError, "^summary_http_check_failed:extra_client_request$"):
            state.finish("client")

    def test_unsupported_http_method_is_a_recorded_failure(self):
        state = summary.State(False, self.path)
        with server(state, False) as origin:
            address = urlsplit(origin)
            connection = http.client.HTTPConnection(address.hostname, address.port, timeout=3)
            try:
                connection.request("GET", "/v1/models")
                response = connection.getresponse()
                self.assertEqual(response.status, 501)
                response.read()
            finally:
                connection.close()
        self.assertEqual(state.errors, ["unsupported_http_method"])


if __name__ == "__main__":
    unittest.main()
