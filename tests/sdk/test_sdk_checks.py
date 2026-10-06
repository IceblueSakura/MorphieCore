"""SDK scenario oracles must survive optimized Python without any HTTP."""
import subprocess
import sys
import unittest
from pathlib import Path


class SdkOracleTests(unittest.TestCase):
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
