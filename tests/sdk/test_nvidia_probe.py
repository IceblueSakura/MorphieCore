"""The shared actual-send guard, raw stream boundary and no-network SDK path."""

import copy
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import httpx2
from openai import DefaultHttpxClient, OpenAI

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "examples"))
from probe_support.ledger import Run
from probe_support.runtime import ProbeClient
from probe_support.checks import ProbeFailure


class ClientGuardTests(unittest.TestCase):
    def test_destination_budget_and_exact_history_fail_before_send(self):
        with tempfile.TemporaryDirectory() as temp:
            run = Run.create(Path(temp) / "run", limit=1)
            history = [
                {
                    "role": "assistant",
                    "content": None,
                    "reasoning_content": "synthetic thought",
                    "reasoning_details": [
                        {
                            "type": "reasoning.encrypted",
                            "id": "rs",
                            "data": "synthetic-opaque",
                        }
                    ],
                }
            ]
            with (
                ProbeClient("http://127.0.0.1:1234", run) as client,
                patch.object(DefaultHttpxClient, "send") as send,
            ):
                for change in ("port", "path", "cap", "missing", "rebound"):
                    client.prepare("nemotron-3-super", "chat", change, history)
                    body = {
                        "model": "nemotron-3-super",
                        "messages": copy.deepcopy(history),
                        "max_completion_tokens": 2048,
                        "stream": False,
                    }
                    url = "http://127.0.0.1:1234/v1/chat/completions"
                    if change == "port":
                        url = url.replace(":1234", ":1235")
                    if change == "path":
                        url = url.replace("chat/completions", "responses")
                    if change == "cap":
                        body["max_completion_tokens"] = 2049
                    if change == "missing":
                        del body["messages"][0]["reasoning_content"]
                    if change == "rebound":
                        body["messages"][0]["reasoning_details"][0]["id"] = "different"
                    with self.assertRaises(ProbeFailure):
                        client.send(client.build_request("POST", url, json=body))
                send.assert_not_called()
                self.assertEqual(run.snapshot(), [])

    def test_sdk_cannot_hide_trailing_data_after_done_or_bypass_http_budget(self):
        finish = b'data: {"id":"x","object":"chat.completion.chunk","created":1,"model":"nemotron-3-super","choices":[{"index":0,"delta":{"content":"pong"},"finish_reason":"stop"}]}\n\n'
        for bad in (False, True):
            with self.subTest(bad=bad), tempfile.TemporaryDirectory() as temp:
                run = Run.create(Path(temp) / "run", limit=1)
                transport = ProbeClient("http://127.0.0.1:1234", run)

                class Bytes(httpx2.SyncByteStream):
                    def __iter__(self):
                        yield finish
                        yield b"data: [DONE]\n\n"
                        if bad:
                            yield b"data: {}\n\n"

                def respond(request, **kwargs):
                    return httpx2.Response(
                        200,
                        headers={"content-type": "text/event-stream"},
                        stream=Bytes(),
                        request=request,
                    )

                with (
                    OpenAI(
                        base_url=transport.origin + "/v1",
                        api_key="synthetic-client",
                        max_retries=0,
                        http_client=transport,
                    ) as client,
                    patch.object(
                        DefaultHttpxClient, "send", side_effect=respond
                    ) as send,
                ):
                    history = [{"role": "user", "content": "synthetic"}]
                    transport.prepare("nemotron-3-super", "chat", "first", history)
                    stream = client.chat.completions.create(
                        model="nemotron-3-super",
                        messages=history,
                        max_completion_tokens=8,
                        stream=True,
                    )
                    if bad:
                        with self.assertRaises(ProbeFailure):
                            list(stream)
                    else:
                        list(stream)
                        self.assertTrue(transport.wire.closed)
                    self.assertEqual(send.call_count, 1)
                    self.assertEqual(len(run.snapshot()), 1)
                    self.assertIn(
                        "x-morphiecore-probe-id", send.call_args.args[0].headers
                    )
                    stream.close()


if __name__ == "__main__":
    unittest.main()
