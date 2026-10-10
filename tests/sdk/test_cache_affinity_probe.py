"""Independent synthetic controls for the explicit implicit-affinity live matrix."""
import copy
from contextlib import nullcontext
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "examples"))
from probe_support.ledger import Run, closed_metrics
from probe_support.scenarios import matrix, plan_groups


class CacheAffinityProbeTests(unittest.TestCase):
    def test_finite_matrix_keeps_actual_history_and_never_supplies_grouping(self):
        cases = ("cache_affinity", "cache_affinity_tool")
        with tempfile.TemporaryDirectory() as temp:
            run = Run.create(Path(temp) / "run", providers="grok,opencode-go",
                             models=["grok-4.7", "hy4-preview"], limit=16, tokens=2048)
            groups = plan_groups(run, run.plan["models"], cases=cases)
            self.assertEqual(sum(g[5] for g in groups), 16)
            seen, outputs = [], []

            def send(client, transport, model, protocol, scenario, history, streaming, **options):
                controls = copy.deepcopy(options["extra"])
                self.assertNotIn("extra_headers", controls)
                self.assertNotIn("extra_body", controls)
                self.assertNotIn("prompt_cache_key", controls)
                self.assertNotIn("session_id", controls)
                self.assertEqual(options["cap"], 2048)
                tool = "cache_affinity_tool" in scenario
                first = scenario.endswith(":1")
                if tool and first:
                    call = ({"id": "actual-call", "type": "function", "function":
                             {"name": "lookup", "arguments": '{"key":"alpha"}'}}
                            if protocol == "chat" else
                            {"type": "function_call", "id": "actual-item", "call_id": "actual-call",
                             "name": "lookup", "arguments": '{"key":"alpha"}'})
                    output = ([{"role": "assistant", "content": None, "tool_calls": [call]}]
                              if protocol == "chat" else [call])
                    text, calls = "", [call]
                else:
                    text = "17" if tool else "7" if first else "8"
                    calls = []
                    output = ([{"role": "assistant", "content": text}] if protocol == "chat" else
                              [{"type": "message", "id": "actual-message", "role": "assistant",
                                "content": [{"type": "output_text", "text": text}]}])
                options["oracle"](text, calls, output)
                if not first:
                    self.assertEqual(history[1:-1], outputs[-1])
                    self.assertNotEqual(seen[-1][0], streaming)
                seen.append((streaming, copy.deepcopy(history), controls))
                outputs.append(copy.deepcopy(output))
                return output, text, calls

            with patch("probe_support.scenarios.session", return_value=nullcontext((None, None))), \
                 patch("probe_support.scenarios.call", side_effect=send):
                self.assertTrue(matrix(run, run.plan["models"], cases=cases))
            self.assertEqual(len(seen), 16)
            for stream, history, controls in seen:
                if "tools" in controls:
                    tool = controls["tools"][0]
                    self.assertFalse(tool.get("function", tool)["strict"])
                    self.assertEqual(controls["tool_choice"], "auto")

    def test_affinity_observations_are_closed_and_do_not_admit_identifiers(self):
        for source in ("miss", "hit", "ambiguous", "disabled", "unsupported", "unavailable",
                       "explicit", "explicit_or_unmapped"):
            self.assertEqual(closed_metrics({"affinity_source": source}), {"affinity_source": source})
        for key in ("affinity_key_sent", "affinity_session_sent"):
            self.assertEqual(closed_metrics({key: True}), {key: True})
        for facts in ({"affinity_source": "raw-session"}, {"cache_key": "private"},
                      {"prefix_hash": "private"}):
            with self.assertRaises(Exception):
                closed_metrics(facts)


if __name__ == "__main__":
    unittest.main()
