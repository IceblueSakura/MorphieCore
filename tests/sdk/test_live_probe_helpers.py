"""Independent typed collector fixtures; not produced by the runtime encoder."""

from pathlib import Path
from types import SimpleNamespace
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "examples"))
from probe_support.codecs import chat_result, opaque_records, UnexpectedChatFinish


class CollectorTests(unittest.TestCase):
    def test_length_requires_an_explicit_scenario(self):
        message = {"role": "assistant", "content": "partial"}
        static = SimpleNamespace(
            choices=[
                SimpleNamespace(
                    finish_reason="length",
                    message=SimpleNamespace(model_dump=lambda **_: message),
                )
            ]
        )
        chunk = SimpleNamespace(
            choices=[True],
            model_dump=lambda **_: {
                "choices": [
                    {
                        "index": 0,
                        "delta": {"content": "partial"},
                        "finish_reason": "length",
                    }
                ]
            },
        )
        for value, stream in ((static, False), ([chunk], True)):
            with self.assertRaises(UnexpectedChatFinish):
                chat_result(value, stream)
            _, text, calls = chat_result(value, stream, allowed_finishes=("length",))
            self.assertEqual((text, calls), ("partial", []))

    def test_reported_usage_is_collected_without_synthesizing_missing_details(self):
        usage = {"prompt_tokens":10,"completion_tokens":4,"total_tokens":14,
            "prompt_tokens_details":{"image_tokens":7,"cached_tokens":0},
            "completion_tokens_details":{"reasoning_tokens":2}}
        message = {"role":"assistant","content":"x"}
        result = SimpleNamespace(
            choices=[SimpleNamespace(finish_reason="stop",
                message=SimpleNamespace(model_dump=lambda **_:message))],
            usage=SimpleNamespace(model_dump=lambda **_:usage))
        metrics = {}
        chat_result(result,False,metrics=metrics)
        self.assertEqual(metrics,{
            "reported_input_tokens":10,"reported_output_tokens":4,
            "reported_reasoning_tokens":2,"reported_image_tokens":7,"reported_cached_tokens":0})
        chunk = SimpleNamespace(choices=[True],model_dump=lambda **_: {
            "choices":[{"index":0,"delta":{"content":"x"},"finish_reason":"stop"}],
            "usage":usage})
        streamed = {}
        chat_result([chunk],True,metrics=streamed)
        self.assertEqual(streamed,metrics)

    def test_readable_reasoning_uses_protocol_owners_and_excludes_opaque_values(self):
        from probe_support.codecs import reasoning_chars, reported_usage
        chat = [{"reasoning_content":"abc","reasoning_details":[
            {"type":"reasoning.text","text":"abc"},
            {"type":"reasoning.encrypted","data":"synthetic-secret"}]}]
        responses = [{"type":"reasoning","summary":[{"type":"summary_text","text":"ab"}],
            "content":[{"type":"reasoning_text","text":"cd"}],"encrypted_content":"synthetic-secret"},
            {"type":"message","content":[{"type":"output_text","text":"not reasoning"}]}]
        self.assertEqual(reasoning_chars(chat,"chat"),3)
        self.assertEqual(reasoning_chars(responses,"responses"),4)
        missing = reported_usage({"input_tokens":9,"output_tokens":2},"responses")
        self.assertEqual(missing["reported_input_tokens"],9)
        self.assertIsNone(missing["reported_reasoning_tokens"])
        self.assertIsNone(missing["reported_image_tokens"])
        from probe_support.checks import ProbeFailure
        from probe_support.ledger import closed_metrics
        self.assertEqual(closed_metrics(missing),missing)
        for invalid in (True,-1,1.5,"synthetic-private"):
            with self.assertRaises(ProbeFailure):
                reported_usage({"prompt_tokens":invalid},"chat")

    def test_responses_static_and_stream_collect_actual_reported_usage(self):
        from probe_support.codecs import response_result
        usage = {"input_tokens":8,"output_tokens":5,"total_tokens":13,
            "input_tokens_details":{"cached_tokens":0},
            "output_tokens_details":{"reasoning_tokens":3}}
        message = {"type":"message","role":"assistant","content":[{"type":"output_text","text":"x"}]}
        result = SimpleNamespace(status="completed",
            usage=SimpleNamespace(model_dump=lambda **_:usage),
            output=[SimpleNamespace(model_dump=lambda **_:message)])
        event = SimpleNamespace(type="response.completed",response=result,model_dump_json=lambda:'{}')
        for value,stream in ((result,False),([event],True)):
            metrics = {}
            _,text,_ = response_result(value,stream,metrics=metrics)
            self.assertEqual(text,"x")
            self.assertEqual(metrics,{
                "reported_input_tokens":8,"reported_output_tokens":5,
                "reported_reasoning_tokens":3,"reported_image_tokens":None,"reported_cached_tokens":0})

    def test_non_success_response_usage_remains_reported_without_success(self):
        from probe_support.codecs import response_result
        from probe_support.checks import ProbeFailure
        from probe_support.ledger import closed_metrics
        usage={"input_tokens":8,"output_tokens":5,"total_tokens":13,
            "output_tokens_details":{"reasoning_tokens":5}}
        result=SimpleNamespace(status="incomplete",usage=SimpleNamespace(model_dump=lambda **_:usage))
        event=SimpleNamespace(type="response.incomplete",response=result,model_dump_json=lambda:'{}')
        for value,stream in ((result,False),([event],True)):
            metrics={}
            with self.assertRaises(ProbeFailure): response_result(value,stream,metrics=metrics)
            self.assertEqual(metrics["reported_reasoning_tokens"],5)
            self.assertEqual(metrics["reported_output_tokens"],5)
        with self.assertRaises(ProbeFailure): closed_metrics({"oracle_failure":"synthetic-private"})

    def test_scoped_opaque_values_are_not_confused_with_unrelated_fields(self):
        body = {
            "input": [
                {"type": "reasoning", "id": "rs1", "encrypted_content": "synthetic-a"}
            ],
            "messages": [
                {
                    "reasoning_details": [
                        {
                            "type": "reasoning.encrypted",
                            "id": "rs2",
                            "data": "synthetic-b",
                        }
                    ]
                }
            ],
            "data": "not-a-replay-token",
        }
        self.assertEqual(
            opaque_records(body), [("rs1", "synthetic-a"), ("rs2", "synthetic-b")]
        )


if __name__ == "__main__":
    unittest.main()
