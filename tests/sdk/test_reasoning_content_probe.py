"""Independent plaintext reasoning oracles; synthetic inputs only."""

from copy import deepcopy
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "examples"))
from probe_support.checks import ProbeFailure
from probe_support.codecs import response_result
from probe_support.ledger import Run
from probe_support.scenarios import plan_groups


class Value:
    def __init__(self, value):
        self.value = value

    def model_dump(self, **_):
        return deepcopy(self.value)


def output():
    return [
        {"id": "r", "type": "reasoning", "summary": [
            {"type": "summary_text", "text": "short"}],
         "content": [{"type": "reasoning_text", "text": "考考"}]},
        {"id": "m", "type": "message", "role": "assistant",
         "content": [{"type": "output_text", "text": "15144"}]},
    ]


def snapshot(items):
    return SimpleNamespace(status="completed", usage=None,
                           output=[Value(item) for item in items])


class Event(Value):
    def __init__(self, value, response=None):
        super().__init__(value)
        self.type = value["type"]
        self.response = response

    def model_dump_json(self):
        import json
        return json.dumps(self.value)


def events():
    def event(kind, **fields):
        return Event({"type": f"response.reasoning_text.{kind}",
                      "output_index": 0, "content_index": 0, "item_id": "r", **fields})
    return [
        event("delta", delta="考"), event("delta", delta="考"),
        event("done", text="考考"),
        Event({"type": "response.completed"}, snapshot(output())),
    ]


class ReasoningContentTests(unittest.TestCase):
    def test_oracle_requires_content_not_summary_opaque_or_answer_alone(self):
        from probe_support.scenarios import expect_reasoning_content
        expect_reasoning_content("15144", [], output())
        for field in ("content",):
            invalid = output()
            invalid[0].pop(field)
            invalid[0]["encrypted_content"] = "synthetic-opaque"
            with self.assertRaises(ProbeFailure):
                expect_reasoning_content("15144", [], invalid)
        for text, calls, items in [
            ("15145", [], output()), ("15144", [{}], output()),
            ("15144", [], output()[1:]),
            ("15144", [], output() + [{"type": "image_generation_call"}]),
            ("15144", [], [{"type": "reasoning", "content": [
                {"type": "reasoning_text", "text": " \n"}]}] + output()[1:]),
        ]:
            with self.assertRaises(ProbeFailure):
                expect_reasoning_content(text, calls, items)

    def test_counts_keep_content_summary_and_opaque_separate(self):
        from probe_support.codecs import reasoning_content_metrics
        items = output()
        items[0]["encrypted_content"] = "synthetic-opaque"
        self.assertEqual(reasoning_content_metrics(items), {
            "reasoning_content_chars": 2, "reasoning_summary_chars": 5})

    def test_stream_requires_matching_deltas_done_and_final_without_deduplication(self):
        metrics = {}
        history, text, calls = response_result(
            events(), True, metrics=metrics, check_reasoning_content=True)
        self.assertEqual(history, output())
        self.assertEqual((text, calls), ("15144", []))
        self.assertTrue(metrics["reasoning_content_stream_ok"])
        variants = []
        for index in (0, 2):
            variant = events()
            del variant[index]
            variants.append(variant)
        variant = events()
        variant.insert(3, variant[0])
        variants.append(variant)
        variant = events()
        variant.insert(3, variant[2])
        variants.append(variant)
        variant = events()
        variant[0].value["item_id"] = "other"
        variants.append(variant)
        variant = events()
        variant[2].value["text"] = "wrong"
        variants.append(variant)
        variant = events()
        variant[-1].response.output[0].value["content"][0]["text"] = "wrong"
        variants.append(variant)
        for variant in variants:
            with self.assertRaises(ProbeFailure):
                response_result(variant, True, check_reasoning_content=True)

    def test_explicit_responses_only_matrix_is_two_bounded_single_requests(self):
        with tempfile.TemporaryDirectory() as temp:
            run = Run.create(Path(temp) / "run", providers="grok",
                             models=["grok-4.7"], limit=2, tokens=2048)
            groups = plan_groups(run, run.plan["models"],
                                 cases=("reasoning_content",), protocol="responses")
            self.assertEqual([(g[2], g[5], g[6]) for g in groups],
                             [(False, 1, 2048), (True, 1, 2048)])
            with self.assertRaises(ProbeFailure):
                plan_groups(run, run.plan["models"],
                            cases=("reasoning_content",), protocol="chat")

    def test_matrix_sends_no_summary_or_strict_control_and_runs_content_oracle(self):
        from contextlib import nullcontext
        from unittest.mock import patch
        from probe_support.scenarios import matrix, expect_reasoning_content
        observed = []

        def send(*args, **kwargs):
            observed.append((args[1:], kwargs))
            self.assertIs(kwargs["oracle"], expect_reasoning_content)
            kwargs["oracle"]("15144", [], output())
            return output(), "15144", []

        with tempfile.TemporaryDirectory() as temp:
            run = Run.create(Path(temp) / "run", providers="grok",
                             models=["grok-4.7"], limit=2, tokens=2048)
            with patch("probe_support.scenarios.session",
                       return_value=nullcontext((None, None))), patch(
                       "probe_support.scenarios.call", side_effect=send):
                self.assertTrue(matrix(run, run.plan["models"],
                    cases=("reasoning_content",), protocol="responses", effort="medium"))
            self.assertEqual(len(observed), 2)
            for args, kwargs in observed:
                self.assertEqual(kwargs["cap"], 2048)
                self.assertEqual(kwargs["extra"], {"reasoning": {"effort": "medium"},
                    "extra_headers": {"X-MorphieCore-Conversation-Id":
                        f"{run.plan['id']}:{args[3].rsplit(':', 1)[0]}"}})

    def test_new_metrics_remain_closed_and_payload_free(self):
        from probe_support.ledger import closed_metrics
        metrics = {"reasoning_content_chars": 2, "reasoning_summary_chars": 5,
                   "reasoning_content_stream_ok": True,
                   "oracle_failure": "missing_reasoning_content"}
        self.assertEqual(closed_metrics(metrics), metrics)
        for metrics in ({"reasoning_content_chars": "synthetic-text"},
                        {"reasoning_content": "synthetic-text"}):
            with self.assertRaises((RuntimeError, ProbeFailure)):
                closed_metrics(metrics)

    def test_image_content_oracle_requires_plain_answer_and_real_reasoning(self):
        from probe_support.scenarios import expect_image_reasoning_content
        expect_image_reasoning_content("18", [], output())
        for text, calls, items in [
            ("17", [], output()), ('{"answer":18}', [], output()),
            ("18", [{}], output()), ("18", [], output()[1:]),
            ("18", [], [{"type": "reasoning", "summary": [
                {"type": "summary_text", "text": "only summary"}]}] + output()[1:]),
            ("18", [], output() + [{"type": "image_generation_call"}]),
        ]:
            with self.assertRaises(ProbeFailure):
                expect_image_reasoning_content(text, calls, items)

    def test_image_content_reuses_verified_pixels_and_only_changes_answer_format(self):
        from probe_support.images import visual_math_history
        original = visual_math_history("responses")[0]["content"]
        plain = visual_math_history("responses", plain_text=True)[0]["content"]
        self.assertEqual([part["type"] for part in plain],
                         ["input_text", "input_image", "input_text", "input_image"])
        self.assertEqual(plain[1:], original[1:])
        self.assertIn("R1 * B2 - B1 * R2", plain[0]["text"])
        self.assertIn("only the integer", plain[0]["text"])
        self.assertNotIn("18", plain[0]["text"])
        self.assertNotIn("JSON", plain[0]["text"])

    def test_image_content_native_and_bridge_keep_explicit_two_request_matrix(self):
        from contextlib import nullcontext
        from unittest.mock import patch
        from probe_support.images import visual_math_history
        from probe_support.scenarios import matrix, expect_image_reasoning_content
        with tempfile.TemporaryDirectory() as temp:
            for bridge in (False, True):
                run = Run.create(Path(temp) / str(bridge), providers="xiaomi",
                    models=["mimo-v2.6-pro"], limit=2, tokens=2048,
                    responses_via_chat=bridge)
                run = Run(run.directory)
                self.assertEqual(bool(run.plan.get("responses_via_chat")), bridge)
                groups = plan_groups(run, run.plan["models"],
                    cases=("image_reasoning_content",), protocol="responses")
                self.assertEqual([(g[2], g[5], g[6]) for g in groups],
                                 [(False, 1, 2048), (True, 1, 2048)])
                with self.assertRaises(ProbeFailure):
                    plan_groups(run, run.plan["models"],
                        cases=("image_reasoning_content",), protocol="chat")
                if bridge:
                    for rejected in ("image", "image_math", "file", "schema"):
                        with self.assertRaises(ProbeFailure):
                            plan_groups(run, run.plan["models"],
                                cases=("image_reasoning_content", rejected),
                                protocol="responses")
                with patch("probe_support.scenarios.session",
                           return_value=nullcontext((None, None))), patch(
                           "probe_support.scenarios.call", return_value=([], "", [])) as send:
                    self.assertTrue(matrix(run, run.plan["models"],
                        cases=("image_reasoning_content",), protocol="responses"))
                self.assertEqual(send.call_count, 2)
                for call in send.call_args_list:
                    self.assertEqual(call.args[5],
                                     visual_math_history("responses", plain_text=True))
                    self.assertEqual(call.kwargs["cap"], 2048)
                    self.assertEqual(call.kwargs["extra"], {"extra_headers": {
                        "X-MorphieCore-Conversation-Id":
                            f"{run.plan['id']}:{call.args[4].rsplit(':', 1)[0]}"}})
                    self.assertIs(call.kwargs["oracle"], expect_image_reasoning_content)

    def test_image_content_call_enables_stream_check_and_records_only_metrics(self):
        from contextlib import redirect_stdout
        from io import StringIO
        from unittest.mock import Mock, patch
        from probe_support.scenarios import call, expect_image_reasoning_content
        client, transport = Mock(), Mock()
        transport.wire = SimpleNamespace(closed=True, text="18")
        transport.complete.return_value = "synthetic:1"
        history = output()
        history[1]["content"][0]["text"] = "18"
        with patch("probe_support.scenarios.response_result",
                   return_value=(history, "18", [])) as collect, redirect_stdout(StringIO()):
            call(client, transport, "mimo-v2.6-pro", "responses", "synthetic", [],
                 True, oracle=expect_image_reasoning_content)
        self.assertTrue(collect.call_args.kwargs["check_reasoning_content"])
        self.assertEqual(transport.complete.call_args.args[0], "passed")
        metrics = transport.complete.call_args.args[1]
        self.assertEqual(metrics["reasoning_content_chars"], 2)
        self.assertEqual(metrics["reasoning_summary_chars"], 5)
        self.assertTrue(metrics["content_ok"])
        from probe_support.ledger import closed_metrics
        self.assertEqual(closed_metrics(metrics), metrics)


if __name__ == "__main__":
    unittest.main()
