"""Synthetic opaque ownership and client-return checks; never real ciphertext."""

from contextlib import nullcontext
from copy import deepcopy
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "examples"))
from probe_support.checks import ProbeFailure
from probe_support.codecs import response_result
from probe_support.ledger import Run
from probe_support.scenarios import plan_groups
from test_reasoning_content_probe import Event, snapshot


def output(identity="r1"):
    return [
        {"type": "reasoning", "id": identity, "summary": [],
         "encrypted_content": "synthetic-opaque-" + identity},
        {"type": "message", "id": "m-" + identity, "role": "assistant",
         "content": [{"type": "output_text", "text": "18"}]},
    ]


def events():
    items = output()
    return [
        Event({"type": "response.output_item.done", "output_index": 0, "item": items[0]}),
        Event({"type": "response.completed"}, snapshot(deepcopy(items))),
    ]


class OpaqueProbeTests(unittest.TestCase):
    def test_oracle_requires_actual_opaque_with_identity_on_every_round(self):
        from probe_support.scenarios import expect_opaque_answer
        check = expect_opaque_answer(18)
        check("18", [], output())
        for mutate in (
            lambda item: item.pop("encrypted_content"),
            lambda item: item.update(encrypted_content=""),
            lambda item: item.update(encrypted_content=None),
            lambda item: item.update(id=""),
        ):
            items = output()
            mutate(items[0])
            with self.assertRaises(ProbeFailure):
                check("18", [], items)
        for text, calls, items in [
            ("19", [], output()), ("18", [{}], output()),
            ("18", [], output() + [output()[0]]),
            ("18", [], output() + [{"type": "image_generation_call"}]),
        ]:
            with self.assertRaises(ProbeFailure):
                check(text, calls, items)

    def test_sse_opaque_done_is_not_replaced_or_repaired_by_final_snapshot(self):
        metrics = {}
        history, _, _ = response_result(
            events(), True, metrics=metrics, check_opaque=True)
        self.assertEqual(history, output())
        self.assertTrue(metrics["opaque_stream_ok"])
        variants = [events()[1:]]
        duplicate = events()
        duplicate.insert(1, duplicate[0])
        variants.append(duplicate)
        for change in ("value", "owner", "absent", "null"):
            sequence = events()
            item = sequence[-1].response.output[0].value
            if change == "value":
                item["encrypted_content"] = "synthetic-other"
            elif change == "owner":
                item["id"] = "other"
            elif change == "absent":
                item.pop("encrypted_content")
            else:
                item["encrypted_content"] = None
            variants.append(sequence)
        late = events()
        late[0].value["item"].pop("encrypted_content")
        variants.append(late)
        for sequence in variants:
            with self.assertRaises(ProbeFailure):
                response_result(sequence, True, check_opaque=True)

    def test_eight_requests_preserve_actual_outputs_images_and_cross_delivery(self):
        from probe_support.scenarios import matrix
        from probe_support.images import visual_math_history
        observed, returned = [], []

        def send(*args, **kwargs):
            case = args[4].split(":")[-3]
            round_number = int(args[4].rsplit(":", 1)[1])
            first = 18 if case == "opaque_image" else 15144
            text = str(first + (37 if round_number == 2 else 0))
            items = output(f"r{len(returned)}")
            items[1]["content"][0]["text"] = text
            kwargs["oracle"](text, [], items)
            observed.append((deepcopy(args[5]), args[6], deepcopy(kwargs)))
            returned.append(deepcopy(items))
            return items, text, []

        with tempfile.TemporaryDirectory() as temp:
            run = Run.create(Path(temp) / "run", providers="grok",
                             models=["grok-4.7"], limit=8, tokens=2048)
            cases = ("opaque_text", "opaque_image")
            groups = plan_groups(run, run.plan["models"], cases=cases, protocol="responses")
            self.assertEqual(len(groups), 4)
            self.assertEqual(sum(g[5] for g in groups), 8)
            self.assertTrue(all(g[6] == 2048 for g in groups))
            with patch("probe_support.scenarios.session",
                       return_value=nullcontext((None, None))), patch(
                       "probe_support.scenarios.call", side_effect=send):
                self.assertTrue(matrix(run, run.plan["models"],
                                       cases=cases, protocol="responses"))
            self.assertEqual([stream for _, stream, _ in observed],
                             [False, True, False, True, True, False, True, False])
            for index in (1, 3, 5, 7):
                self.assertEqual(observed[index][0][1:-1], returned[index - 1])
                self.assertEqual(observed[index][0][0], observed[index - 1][0][0])
            for index in (2, 6):
                self.assertEqual(observed[index][0], visual_math_history("responses", plain_text=True))
            for _, _, options in observed:
                self.assertTrue(options["check_opaque"])
                self.assertEqual(options["extra"], {
                    "store": False, "include": ["reasoning.encrypted_content"],
                    "reasoning": {"effort": "medium"}})
            for protocol, effort in (("chat", None), ("responses", "none")):
                with self.assertRaises(ProbeFailure):
                    plan_groups(run, run.plan["models"], cases=cases,
                                protocol=protocol, effort=effort)

    def test_missing_first_round_opaque_stops_the_dependent_return(self):
        from probe_support.scenarios import matrix
        with tempfile.TemporaryDirectory() as temp:
            run = Run.create(Path(temp) / "run", providers="grok",
                             models=["grok-4.7"], limit=2, tokens=2048)

            def send(*args, **kwargs):
                kwargs["oracle"]("15144", [], [])

            with patch("probe_support.scenarios.session",
                       return_value=nullcontext((None, None))), patch(
                       "probe_support.scenarios.call", side_effect=send) as call:
                self.assertFalse(matrix(run, run.plan["models"], cases=("opaque_text",),
                                        protocol="responses", delivery="json"))
            self.assertEqual(call.call_count, 1)

    def test_return_metrics_do_not_store_ciphertext_or_identity(self):
        from contextlib import redirect_stdout
        from io import StringIO
        from types import SimpleNamespace
        from unittest.mock import Mock
        from probe_support.scenarios import call, expect_opaque_answer
        from probe_support.ledger import closed_metrics
        client, transport = Mock(), Mock()
        client.responses.create.return_value = snapshot(output("r2"))
        transport.wire = SimpleNamespace(closed=True)
        transport.complete.return_value = "synthetic:2"
        saved = output()
        before = deepcopy(saved)
        with redirect_stdout(StringIO()):
            call(client, transport, "grok-4.7", "responses", "synthetic", saved,
                 False, check_opaque=True, oracle=expect_opaque_answer(18))
        self.assertEqual(saved, before)
        self.assertEqual(client.responses.create.call_args.kwargs["input"], before)
        metrics = transport.complete.call_args.args[1]
        self.assertEqual(metrics["replayed_opaque_items"], 1)
        self.assertEqual(metrics["reported_opaque_items"], 1)
        self.assertEqual(closed_metrics(metrics), metrics)
        for value in ("r1", "synthetic-opaque-r1", "synthetic-opaque-r2"):
            self.assertNotIn(value, str(metrics))

    def test_opaque_cases_do_not_enable_other_models_or_chat_bridge(self):
        with tempfile.TemporaryDirectory() as temp:
            for bridge in (False, True):
                run = Run.create(Path(temp) / str(bridge), providers="xiaomi",
                                 models=["mimo-v2.6-pro"], limit=8, tokens=2048,
                                 responses_via_chat=bridge)
                with self.assertRaises(ProbeFailure):
                    plan_groups(run, run.plan["models"], cases=("opaque_text", "opaque_image"),
                                protocol="responses")

    def test_color_control_preserves_images_and_opaque_without_changing_math_oracle(self):
        from probe_support.images import image_history
        from probe_support.scenarios import matrix, expect_opaque_answer
        sent, returned = [], []

        def send(*args, **kwargs):
            text = "red,blue" if len(sent) % 2 == 0 else "blue,red"
            items = output(f"c{len(sent)}")
            items[1]["content"][0]["text"] = text
            kwargs["oracle"](text, [], items)
            with self.assertRaises(ProbeFailure):
                kwargs["oracle"]("blue,red" if text == "red,blue" else "red,blue", [], items)
            sent.append((deepcopy(args[5]), args[6]))
            returned.append(deepcopy(items))
            return items, text, []

        with tempfile.TemporaryDirectory() as temp:
            run = Run.create(Path(temp) / "run", providers="grok",
                             models=["grok-4.7"], limit=4, tokens=2048)
            groups = plan_groups(run, run.plan["models"],
                                 cases=("opaque_image_colors",), protocol="responses")
            self.assertEqual(sum(g[5] for g in groups), 4)
            with patch("probe_support.scenarios.session",
                       return_value=nullcontext((None, None))), patch(
                       "probe_support.scenarios.call", side_effect=send):
                self.assertTrue(matrix(run, run.plan["models"],
                    cases=("opaque_image_colors",), protocol="responses"))
        self.assertEqual([stream for _, stream in sent], [False, True, True, False])
        for index in (0, 2):
            self.assertEqual(sent[index][0], image_history("responses"))
            self.assertEqual(sent[index + 1][0][0], sent[index][0][0])
            self.assertEqual(sent[index + 1][0][1:-1], returned[index])
        with self.assertRaises(ProbeFailure):
            expect_opaque_answer(18)("red,blue", [], output())


if __name__ == "__main__":
    unittest.main()
