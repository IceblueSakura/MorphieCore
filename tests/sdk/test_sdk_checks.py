"""SDK scenario oracles must survive optimized Python without any HTTP."""
import subprocess
import sys
import unittest
from pathlib import Path


class SdkOracleTests(unittest.TestCase):
    def test_google_object_result_types_retain_exact_values_and_error_presence(self):
        from google.genai import types
        from google.genai._gaos.types.interactions.functionresultstep import FunctionResultStep
        for value in ({}, {"n": 9007199254740993, "nested": [None, {"$ref": "literal"}]}):
            for error in (None, False, True):
                data = {"type": "function_result", "call_id": "c", "name": "read", "result": value}
                if error is not None:
                    data["is_error"] = error
                result = FunctionResultStep.model_validate(data)
                self.assertEqual(result.model_dump(exclude_unset=True), data)
                envelope = {"error" if error is True else "output": value}
                response = types.FunctionResponse(id="c", name="read", response=envelope)
                self.assertEqual(response.model_dump(exclude_unset=True)["response"], envelope)

    def test_reasoning_replay_oracle_keeps_readable_and_opaque_facts_separate(self):
        code = """
from copy import deepcopy
from responses_text_loop import check_reasoning
from sdk_support import SdkCheckFailure
readable = {"type": "reasoning", "id": "reasoning", "status": "completed",
            "summary": [{"type": "summary_text", "text": "Plan 🧪"}],
            "content": [{"type": "reasoning_text", "text": "Check"}]}
encrypted = {**readable, "encrypted_content": "synthetic-final-token"}
check_reasoning([readable], False)
check_reasoning([encrypted], True)
bad_summary = deepcopy(encrypted)
bad_summary["summary"][0]["text"] = "changed"
bad_content = deepcopy(encrypted)
bad_content["content"][0]["text"] = "changed"
for output, opaque in [
    ([], True), ([encrypted, encrypted], True),
    ([readable], True), ([encrypted], False),
    ([{**encrypted, "encrypted_content": "synthetic-partial-token"}], True),
    ([{**encrypted, "encrypted_content": "changed"}], True),
    ([{**readable, "encrypted_content": ""}], False),
    ([bad_summary], True), ([bad_content], True),
]:
    try:
        check_reasoning(output, opaque)
    except SdkCheckFailure:
        continue
    raise RuntimeError("wrong reasoning result passed its oracle")
"""
        for flags in ([], ["-O"]):
            with self.subTest(flags=flags):
                result = subprocess.run([sys.executable, *flags, "-c", code],
                    cwd=Path(__file__).parent, capture_output=True, text=True, timeout=15)
                self.assertEqual(result.returncode, 0, result.stderr)

    def test_wrong_sdk_results_fail_in_normal_and_optimized_modes(self):
        code = """
from types import SimpleNamespace
from unittest.mock import MagicMock, patch
import responses_text_loop as responses
import chat_text_loop as chat
import gateway_text_loop as gateway
from sdk_support import SdkCheckFailure
client = MagicMock()
client.__enter__.return_value = client
client.responses.parse.return_value = SimpleNamespace(status="failed", usage=SimpleNamespace(total_tokens=8))
client.chat.completions.parse.return_value = SimpleNamespace(choices=[], model="wrong", usage=SimpleNamespace(total_tokens=5))
with patch.object(responses, "client_for", return_value=client), patch.object(chat, "client_for", return_value=client), patch.object(gateway.openai, "OpenAI", return_value=client):
    for run in (lambda: responses.run("unused", False), lambda: chat.run("unused", False), lambda: gateway.run("http://127.0.0.1:1/v1")):
        try:
            run()
        except SdkCheckFailure:
            continue
        raise RuntimeError("wrong SDK result passed its oracle")
"""
        for flags in ([], ["-O"]):
            with self.subTest(flags=flags):
                result = subprocess.run([sys.executable, *flags, "-c", code],
                    cwd=Path(__file__).parent, capture_output=True, text=True, timeout=15)
                self.assertEqual(result.returncode, 0, result.stderr)
