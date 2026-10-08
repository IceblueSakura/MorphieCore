"""Synthetic plan and oracle checks for an explicitly selected Chat upstream."""
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "examples"))
from probe_support.ledger import Run
from probe_support.checks import ProbeFailure
from probe_support.scenarios import plan_groups


class BridgeTests(unittest.TestCase):
    def test_usage_presence_diagnostics_are_closed_and_never_accept_values_or_names(self):
        from probe_support.ledger import closed_metrics
        for value in (0, 2, 96, 127):
            self.assertEqual(closed_metrics({"reported_usage_detail_mask": value}),
                             {"reported_usage_detail_mask": value})
        for value in (-1, 128, True, "private", None):
            with self.assertRaises(ProbeFailure):
                closed_metrics({"reported_usage_detail_mask": value})
        with self.assertRaises(ProbeFailure):
            closed_metrics({"projection_failure": "private-field-name"})

    def test_parallel_oracle_requires_two_distinct_calls_and_exact_arguments(self):
        from probe_support.scenarios import expect_parallel
        calls = [
            {"call_id": "a", "name": "lookup", "arguments": '{"key":"alpha"}'},
            {"call_id": "b", "name": "lookup", "arguments": '{"key":"beta"}'},
        ]
        expect_parallel("", calls, [])
        for invalid in [
            calls[:1], [calls[0], calls[0]],
            [calls[0], {**calls[1], "call_id": "a"}],
            [calls[0], {**calls[1], "arguments": '{"key":"other"}'}],
        ]:
            with self.assertRaises(ProbeFailure):
                expect_parallel("", invalid, [])

    def test_bridge_selection_is_persisted_and_requires_responses_scenarios(self):
        with tempfile.TemporaryDirectory() as temp:
            run = Run.create(Path(temp) / "bridge", providers="openrouter",
                             models=["gpt-6-luna"], limit=20, tokens=1024,
                             responses_via_chat=True)
            self.assertTrue(Run(run.directory).plan["responses_via_chat"])
            groups = plan_groups(run, run.plan["models"],
                                 cases=("text", "tool", "history", "parallel"),
                                 protocol="responses")
            self.assertEqual(sum(g[5] for g in groups), 18)
            for protocol, cases in [("chat", ("text",)), (None, ("text",)),
                                    ("responses", ("schema",))]:
                with self.assertRaises(ProbeFailure):
                    plan_groups(run, run.plan["models"], cases=cases, protocol=protocol)

    def test_bridge_plan_needs_explicit_models_and_cannot_be_an_image_plan(self):
        with tempfile.TemporaryDirectory() as temp:
            for kwargs in [
                {"providers": "openrouter"},
                {"providers": "openrouter", "models": ["gpt-image-2.5-flare"],
                 "task": "images", "tokens": None},
            ]:
                with self.assertRaises(ProbeFailure):
                    Run.create(Path(temp) / "bad", responses_via_chat=True, **kwargs)
