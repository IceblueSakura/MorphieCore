"""Independent negative oracles for installed protocol-default characterization."""
import copy
import json
from pathlib import Path
import unittest

import protocol_defaults as defaults


class ProtocolDefaultOracles(unittest.TestCase):
    path = Path("/synthetic/sample.txt")

    def test_pi_rejection_cannot_be_replaced_by_an_unrelated_parser_or_transport_failure(self):
        for protocol, prefix in [("chat", "400 "), ("responses", "opencode API error (400): ")]:
            message = {"stopReason": "error", "errorMessage": prefix + defaults.REJECTION_MESSAGE}
            defaults.assert_pi_rejection([message], protocol)
            for error in ("connection closed", "Unexpected end of JSON input", "400 invalid JSON",
                          "500 " + defaults.REJECTION_MESSAGE):
                with self.subTest(protocol=protocol, error=error), self.assertRaisesRegex(
                    RuntimeError, "^pi_default_error_identity$"
                ):
                    defaults.assert_pi_rejection([{**message, "errorMessage": error}], protocol)
        payload = json.dumps({"error": {"type": "invalid_request_error", "code": "invalid_request",
                                      "message": defaults.REJECTION_MESSAGE, "param": None}}).encode()
        defaults.assert_rejection_response(400, "application/json", payload)
        with self.assertRaises(json.JSONDecodeError):
            defaults.assert_rejection_response(400, "application/json", payload[:-1])
        for status, content_type, body, error in [
            (502, "application/json", payload, "default_rejection_http"),
            (400, "text/plain", payload, "default_rejection_http"),
            (400, "application/json", b'{"error":{"code":"upstream_error"}}', "default_rejection_envelope"),
        ]:
            with self.assertRaisesRegex(RuntimeError, "^" + error + "$"):
                defaults.assert_rejection_response(status, content_type, body)

    def responses_body(self, pi):
        tool = {"type": "function", "name": "read", "parameters": {
            "type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}}
        body = {"model": "gpt-6.1-sol", "stream": True, "max_output_tokens": 2048,
                "store": False, "include": ["reasoning.encrypted_content"],
                "input": [{"role": "user", "content": defaults.READ}]}
        if pi:
            body["reasoning"] = {"effort": "medium", "summary": "auto"}
            body["input"].append({"type": "additional_tools", "tools": [tool]})
        else:
            body["tools"] = [{**tool, "strict": False}]
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
            tool = original["input"][-1]["tools"][0] if pi else original["tools"][0]
            tool["strict"] = True
            with self.assertRaisesRegex(RuntimeError, "^default_read_strict$"):
                defaults.assert_responses_profile(original, pi)
            if pi:
                del tool["strict"]
            else:
                tool["strict"] = False
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
        with self.assertRaisesRegex(RuntimeError, "^default_responses_missing_annotations_changed$"):
            defaults.assert_responses_history(body, 3, self.path)

    def test_rejections_require_the_intended_boundary_and_zero_dispatch(self):
        failures = [
            ("responses", True, {"failure": {"boundary": "request.decode",
                                             "code": "codec.unsupported.unavailable"}}),
            ("responses", False, {"failure": {"boundary": "request.decode",
                                              "code": "codec.invalid.invalid_value"}}),
        ]
        for protocol, pi, generation in failures:
            final = {"stage": "admission", "outcome": "error", "candidates": [], "generation": generation}
            records = [final] if pi else [
                {"outcome": "complete", "candidates": [{}]},
                {"outcome": "complete", "candidates": [{}]}, final]
            defaults.assert_diagnostics(records, protocol, pi)
            for key, value, error in [
                ("stage", "intake", "default_rejection_stage"),
                ("candidates", [{}], "default_gateway_completion"),
                ("outcome", "complete", "default_gateway_completion"),
                ("generation", {}, "default_responses_rejection_reason"),
            ]:
                changed = copy.deepcopy(records)
                changed[-1][key] = value
                with self.subTest(protocol=protocol, pi=pi, key=key), self.assertRaisesRegex(
                    RuntimeError, "^" + error + "$"
                ):
                    defaults.assert_diagnostics(changed, protocol, pi)

    def test_failed_continuation_cannot_claim_a_client_answer(self):
        failure = {"type": "error", "error": {"type": "provider.invalid-request", "status": 400}}
        defaults.assert_opencode_rejection([failure])
        for events in ([], [{**failure, "error": {"type": "provider.invalid-output", "status": 502}}]):
            with self.assertRaisesRegex(RuntimeError, "^default_responses_client_rejection$"):
                defaults.assert_opencode_rejection(events)
        with self.assertRaisesRegex(RuntimeError, "^default_responses_false_success$"):
            defaults.assert_opencode_rejection([{"type": "text", "part": {"text": defaults.TEXT}}, failure])


if __name__ == "__main__":
    unittest.main()
