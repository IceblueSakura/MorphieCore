"""Independent negative oracles for installed protocol-default characterization."""
import copy
from pathlib import Path
import unittest

import protocol_defaults as defaults


class ProtocolDefaultOracles(unittest.TestCase):
    path = Path("/synthetic/sample.txt")

    def responses_body(self, pi):
        tool = {"type": "function", "name": "read", "parameters": {
            "type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}}
        body = {"model": "gpt-6.1-sol", "stream": True, "max_output_tokens": 2048,
                "store": False, "include": ["reasoning.encrypted_content"],
                "input": [{"role": "user", "content": defaults.READ}]}
        if pi:
            body["reasoning"] = {"effort": "medium", "summary": "auto"}
            body["input"].insert(0, {"type": "additional_tools", "role": "developer", "tools": [tool]})
        else:
            body["tools"] = [{**tool, "strict": False}, {
                "type": "function", "name": "auxiliary", "parameters": {"type": "object"}}]
        return body

    def test_responses_defaults_cannot_be_simplified_into_a_passing_profile(self):
        for pi in (True, False):
            original = self.responses_body(pi)
            defaults.assert_responses_profile(original, pi)
            for field, value, error in [
                ("max_output_tokens", 4096, "default_responses_bounds_and_storage"),
                ("store", True, "default_responses_bounds_and_storage"),
                ("include", [], "default_responses_include"),
                ("reasoning", {} if pi else {"effort": "low"}, "default_responses_reasoning"),
                ("previous_response_id", "invented", "default_responses_full_history"),
            ]:
                body = copy.deepcopy(original)
                body[field] = value
                with self.subTest(pi=pi, field=field), self.assertRaisesRegex(RuntimeError, "^" + error + "$"):
                    defaults.assert_responses_profile(body, pi)
            tool = original["input"][0]["tools"][0] if pi else original["tools"][0]
            tool["strict"] = True
            with self.assertRaisesRegex(RuntimeError, "^default_read_strict$"):
                defaults.assert_responses_profile(original, pi)
            if pi:
                del tool["strict"]
            else:
                tool["strict"] = False
            if pi:
                moved = copy.deepcopy(original)
                moved["input"].append(moved["input"].pop(0))
                wrong_role = copy.deepcopy(original)
                wrong_role["input"][0]["role"] = "system"
                for changed in (moved, wrong_role):
                    with self.assertRaisesRegex(RuntimeError, "^default_responses_declaration_position$"):
                        defaults.assert_responses_profile(changed, pi)
            del tool["parameters"]["properties"]["path"]
            with self.assertRaisesRegex(RuntimeError, "^default_read_path_schema$"):
                defaults.assert_responses_profile(original, pi)

    def test_chat_default_strictness_is_client_owned_and_cannot_be_downgraded(self):
        for pi in (True, False):
            body = {"model": "deepseek-flash", "stream": True, "max_tokens": 2048,
                    "stream_options": {"include_usage": True}, "tools": [{"type": "function",
                    "function": {"name": "read", "strict": pi, "parameters": {"type": "object",
                        "properties": {"path": {"type": "string"}}, "required": ["path"]}}}]}
            if pi:
                body.update(thinking={"type": "enabled"}, reasoning_effort="high")
            else:
                body["tools"].append({"type": "function", "function": {
                    "name": "auxiliary", "parameters": {"type": "object"}}})
            defaults.assert_chat_profile(body, pi)
            body["tools"][0]["function"]["strict"] = not pi
            with self.assertRaisesRegex(RuntimeError, "^default_read_strict$"):
                defaults.assert_chat_profile(body, pi)
            body["tools"][0]["function"]["strict"] = pi
            body["reasoning_effort"] = "low"
            with self.assertRaisesRegex(RuntimeError, "^default_effort$"):
                defaults.assert_chat_profile(body, pi)

    def test_responses_history_keeps_order_reasoning_call_results_and_final_phase(self):
        body = {"input": [
            {"role": "user", "content": defaults.READ},
            {"type": "reasoning", "id": "rs_default_1",
             "summary": [{"type": "summary_text", "text": defaults.PLAN}]},
            {"type": "function_call", "id": "fc_default_read", "call_id": defaults.CALL,
             "name": "read", "arguments": '{"path":"/synthetic/sample.txt"}'},
            {"type": "function_call_output", "call_id": defaults.CALL, "output": defaults.TEXT},
            {"type": "reasoning", "id": "rs_default_2",
             "summary": [{"type": "summary_text", "text": defaults.PLAN}]},
            {"type": "message", "role": "assistant", "id": "msg_default_2", "status": "completed",
             "phase": "final_answer", "content": [{"type": "output_text", "text": defaults.TEXT}]},
            {"role": "user", "content": defaults.NEXT},
        ]}
        defaults.assert_responses_history(body, 3, self.path)
        mutations = [
            (2, "call_id", "changed", "default_responses_call_identity"),
            (2, "arguments", "{}", "default_responses_call_identity"),
            (2, "namespace", "invented", "default_responses_direct_call"),
            (2, "async", True, "default_responses_direct_call"),
            (2, "caller", {"type": "code_execution_20260120", "tool_id": "invented"},
             "default_responses_direct_call"),
            (3, "call_id", "changed", "default_responses_result_identity"),
            (3, "output", "changed", "default_responses_result_identity"),
            (1, "summary", [], "default_responses_reasoning_content"),
            (5, "phase", "commentary", "default_responses_final_history"),
        ]
        for index, key, value, error in mutations:
            changed = copy.deepcopy(body)
            changed["input"][index][key] = value
            with self.subTest(key=key, index=index), self.assertRaisesRegex(RuntimeError, "^" + error + "$"):
                defaults.assert_responses_history(changed, 3, self.path)
        reordered = copy.deepcopy(body)
        reordered["input"][2], reordered["input"][3] = reordered["input"][3], reordered["input"][2]
        with self.assertRaisesRegex(RuntimeError, "^default_responses_history_order$"):
            defaults.assert_responses_history(reordered, 3, self.path)
        body["input"][5]["content"][0]["annotations"] = []
        defaults.assert_responses_history(body, 3, self.path)
        defaults.assert_responses_history(body, 3, self.path, upstream=True)
        defaults.assert_responses_history(body, 3, self.path, pi=True)
        del body["input"][5]["content"][0]["annotations"]
        with self.assertRaisesRegex(RuntimeError, "^default_responses_annotations$"):
            defaults.assert_responses_history(body, 3, self.path, pi=True)
        with self.assertRaisesRegex(RuntimeError, "^default_responses_annotations$"):
            defaults.assert_responses_history(body, 3, self.path, upstream=True)

    def test_default_completion_requires_dispatch_and_no_ignored_controls(self):
        records = [{"outcome": "complete", "candidates": [{}]} for _ in range(3)]
        defaults.assert_diagnostics(records, "responses", False)
        for key, value, error in [
            ("candidates", [], "default_gateway_completion"),
            ("candidates", [{}, {}], "default_gateway_completion"),
            ("outcome", "error", "default_gateway_completion"),
            ("ignored_request_field_count", 1, "default_gateway_ignored_fields"),
        ]:
            changed = copy.deepcopy(records)
            changed[-1][key] = value
            with self.subTest(key=key), self.assertRaisesRegex(RuntimeError, "^" + error + "$"):
                defaults.assert_diagnostics(changed, "responses", False)
        with self.assertRaisesRegex(RuntimeError, "^default_gateway_request_count$"):
            defaults.assert_diagnostics(records[:2], "responses", False)


if __name__ == "__main__":
    unittest.main()
