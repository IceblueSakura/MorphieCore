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

    def test_chat_only_bridge_has_responses_client_without_native_registration(self):
        from probe_support.ledger import MODELS
        from probe_support.runtime import ProbeClient

        with tempfile.TemporaryDirectory() as temp:
            bridge = Run.create(Path(temp) / "bridge", providers="modelbest",
                                models=["minicpm-v-4.6"], limit=18, tokens=1024,
                                responses_via_chat=True)
            bridge = Run(bridge.directory)
            self.assertEqual(MODELS["minicpm-v-4.6"][4], ("chat",))
            self.assertEqual(bridge.protocols("minicpm-v-4.6"), ("responses",))
            groups = plan_groups(bridge, bridge.plan["models"],
                                 cases=("text", "history", "parallel", "image", "image_math"),
                                 protocol="responses")
            self.assertEqual(sum(g[5] for g in groups), 18)
            self.assertTrue(all(g[1] == "responses" for g in groups))
            self.assertEqual({g[2] for g in groups}, {False, True})
            with ProbeClient("http://127.0.0.1:1", bridge) as client:
                client.prepare("minicpm-v-4.6", "responses", "synthetic", [])
                with self.assertRaises(ProbeFailure):
                    client.prepare("minicpm-v-4.6", "chat", "synthetic", [])
            native = Run.create(Path(temp) / "native", providers="modelbest",
                                models=["minicpm-v-4.6"], limit=1, tokens=1024)
            self.assertEqual(native.protocols("minicpm-v-4.6"), ("chat",))
            with ProbeClient("http://127.0.0.1:1", native) as client:
                with self.assertRaises(ProbeFailure):
                    client.prepare("minicpm-v-4.6", "responses", "synthetic", [])
            with self.assertRaises(ProbeFailure):
                plan_groups(native, native.plan["models"], cases=("text",),
                            protocol="responses")
            with self.assertRaises(ProbeFailure):
                bridge.protocols("minicpm5-1b")

    def test_bridge_visual_cases_are_limited_to_explicit_targets(self):
        with tempfile.TemporaryDirectory() as temp:
            run = Run.create(Path(temp) / "bridge", providers="modelbest",
                             models=["minicpm-v-4.6"], limit=2, tokens=1024,
                             responses_via_chat=True)
            for cases in [("schema",), ("opaque_image",), ("file",)]:
                with self.assertRaises(ProbeFailure):
                    plan_groups(run, run.plan["models"], cases=cases, protocol="responses")
            for index, kwargs in enumerate([
                {"providers": "grok", "models": ["grok-4.7"]},
                {"providers": "modelbest", "models": ["minicpm5-1b"]},
                {"providers": "aliyun-tokenplan-cn", "models": ["qwen3.8-flash"]},
                {"providers": "xiaomi", "models": ["mimo-v2.6-pro"]},
                {"providers": "openrouter", "models": ["gpt-6-luna"]},
            ]):
                if kwargs["providers"] == "grok":
                    with self.assertRaises(ProbeFailure):
                        Run.create(Path(temp) / str(index), responses_via_chat=True, **kwargs)
                else:
                    other = Run.create(Path(temp) / str(index), responses_via_chat=True, **kwargs)
                    with self.assertRaises(ProbeFailure):
                        plan_groups(other, other.plan["models"], cases=("image",),
                                    protocol="responses")

    def test_mimo_flash_visual_bridge_and_native_plans_keep_separate_scopes(self):
        from probe_support.ledger import MODELS

        cases = ("text", "image", "image_math", "reasoning_content",
                 "image_reasoning_content", "history", "parallel")
        with tempfile.TemporaryDirectory() as temp:
            for index, (provider, model, bridge) in enumerate([
                ("xiaomi", "mimo-v2.6-flash", True),
                ("xiaomi", "mimo-v2.6-flash", False),
                ("openrouter", "gpt-6-luna", False),
            ]):
                with self.subTest(provider=provider, bridge=bridge):
                    run = Run.create(Path(temp) / str(index), providers=provider,
                                     models=[model], limit=22, tokens=2048,
                                     responses_via_chat=bridge)
                    run = Run(run.directory)
                    self.assertEqual(bool(run.plan.get("responses_via_chat")), bridge)
                    self.assertEqual(run.protocols(model),
                                     ("responses",) if bridge else MODELS[model][4])
                    groups = plan_groups(run, [model], cases=cases, protocol="responses")
                    self.assertEqual(sum(g[5] for g in groups), 22)
                    self.assertEqual(len(groups), 14)
                    self.assertEqual(len({g[4] for g in groups}), 14)
                    self.assertTrue(all(g[1] == "responses" and g[0] == model for g in groups))
                    self.assertEqual({g[2] for g in groups}, {False, True})
                    self.assertTrue(all(g[6] == (512 if g[3] == "image" else 2048)
                                        for g in groups))
                    if bridge:
                        for forbidden in ("schema", "opaque_image", "file"):
                            with self.assertRaises(ProbeFailure):
                                plan_groups(run, [model], cases=(forbidden,), protocol="responses")

    def test_chat_only_bridge_cli_dry_run_does_not_create_or_access_a_store(self):
        import json
        import subprocess

        root = Path(__file__).resolve().parents[2]
        with tempfile.TemporaryDirectory() as temp:
            destination = Path(temp) / "not-created"
            result = subprocess.run(
                [sys.executable, "examples/probe.py", "plan", str(destination),
                 "--providers", "modelbest", "--model", "minicpm-v-4.6",
                 "--responses-via-chat", "--limit", "6", "--tokens", "1024", "--dry-run"],
                cwd=root, capture_output=True, timeout=10, check=False)
            self.assertEqual(result.returncode, 0)
            self.assertEqual(json.loads(result.stdout),
                             {"models": ["minicpm-v-4.6"], "limit": 6,
                              "tokens": 1024, "responses_via_chat": True})
            self.assertFalse(destination.exists())

    def test_chat_only_visual_bridge_uses_existing_oracles_and_one_send_per_group(self):
        from contextlib import nullcontext
        from unittest.mock import patch
        from probe_support.scenarios import matrix, expect_image, expect_visual_math

        with tempfile.TemporaryDirectory() as temp:
            run = Run.create(Path(temp) / "bridge", providers="modelbest",
                             models=["minicpm-v-4.6"], limit=6, tokens=1024,
                             responses_via_chat=True)
            with patch("probe_support.scenarios.session",
                       return_value=nullcontext((None, None))), patch(
                       "probe_support.scenarios.call", return_value=([], "", [])) as send:
                self.assertTrue(matrix(run, run.plan["models"], protocol="responses",
                                       cases=("text", "image", "image_math")))
            self.assertEqual(send.call_count, 6)
            for invocation in send.call_args_list:
                self.assertEqual(invocation.args[2:4], ("minicpm-v-4.6", "responses"))
            for index in (1, 4):
                self.assertEqual(send.call_args_list[index].kwargs["cap"], 512)
                self.assertIs(send.call_args_list[index].kwargs["oracle"], expect_image)
            for index in (2, 5):
                self.assertEqual(send.call_args_list[index].kwargs["cap"], 1024)
                self.assertIs(send.call_args_list[index].kwargs["oracle"], expect_visual_math)

    def test_reasoning_content_bridge_keeps_two_requests_and_shared_oracle(self):
        from contextlib import nullcontext
        from unittest.mock import patch
        from probe_support.scenarios import matrix, expect_reasoning_content

        with tempfile.TemporaryDirectory() as temp:
            created = Run.create(Path(temp) / "bridge", providers="xiaomi",
                                 models=["mimo-v2.6-pro"], limit=2, tokens=2048,
                                 responses_via_chat=True)
            run = Run(created.directory)
            self.assertTrue(run.plan["responses_via_chat"])
            groups = plan_groups(run, run.plan["models"],
                                 cases=("reasoning_content",), protocol="responses")
            self.assertEqual([(g[1], g[2], g[5], g[6]) for g in groups],
                             [("responses", False, 1, 2048),
                              ("responses", True, 1, 2048)])
            for protocol, cases in [
                ("chat", ("reasoning_content",)), (None, ("reasoning_content",)),
                ("responses", ("reasoning_content", "image")),
                ("responses", ("reasoning_content", "schema")),
                ("responses", ("reasoning_content", "reasoning")),
            ]:
                with self.assertRaises(ProbeFailure):
                    plan_groups(run, run.plan["models"], cases=cases, protocol=protocol)
            with patch("probe_support.scenarios.session",
                       return_value=nullcontext((None, None))), patch(
                       "probe_support.scenarios.call", return_value=([], "", [])) as send:
                self.assertTrue(matrix(run, run.plan["models"],
                                       cases=("reasoning_content",), protocol="responses"))
            self.assertEqual(send.call_count, 2)
            for call in send.call_args_list:
                self.assertEqual(call.args[2:4], ("mimo-v2.6-pro", "responses"))
                self.assertEqual(call.kwargs["cap"], 2048)
                self.assertEqual(call.kwargs["extra"], {})
                self.assertIs(call.kwargs["oracle"], expect_reasoning_content)
