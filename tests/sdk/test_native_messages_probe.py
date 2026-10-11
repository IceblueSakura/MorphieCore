"""Native targets never silently enter the OpenAI SDK/default matrix."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "examples"))
from probe_support.catalog import select_bindings
from probe_support.ledger import Run, closed_metrics


class NativeMessagesProbeTests(unittest.TestCase):
    def test_explicit_native_selection_and_shared_finite_budget(self):
        self.assertNotIn("claude-haiku-5-5", [r[1] for r in select_bindings("opencode-go")])
        rows = select_bindings("opencode-go", models=["claude-haiku-5-5"])
        self.assertEqual(rows[0][4], ("messages",))
        with tempfile.TemporaryDirectory() as directory:
            run = Run.create(Path(directory) / "run", providers="opencode-go",
                             models=["claude-haiku-5-5"], limit=14, tokens=2048)
            self.assertEqual(run.protocols("claude-haiku-5-5"), ("messages",))
            run.register([("claude-haiku-5-5", "native:claude-haiku-5-5:text-json:1", 2048)])
            identifier = run.reserve("claude-haiku-5-5", "native:claude-haiku-5-5:text-json:1", 2048)
            run.dispatched(identifier)
            run.finish(identifier, "passed", {"native_consumed": True, "reported_opaque_items": 1})
            output = subprocess.run([sys.executable, str(ROOT / "examples/probe.py"),
                                     "run", str(run.directory), "--dry-run"],
                                    cwd=ROOT, capture_output=True, timeout=10)
            self.assertNotEqual(output.returncode, 0)
            self.assertEqual(len([r for r in run.snapshot() if r["state"] != "not_run"]), 1)

    def test_native_metadata_is_closed_and_contains_no_signatures(self):
        self.assertEqual(closed_metrics({"native_consumed": True}), {"native_consumed": True})
        for invalid in ({"native_consumed": "yes"}, {"signature": "private"},
                        {"body": "private"}, {"terminal": "unknown-native"}):
            with self.assertRaises(Exception):
                closed_metrics(invalid)


if __name__ == "__main__":
    unittest.main()
