"""Explicit file activation stays in the binary; the probe never exports upstream tokens."""
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "examples"))
from probe_support.catalog import select_bindings
from probe_support.ledger import Run
from probe_support.runtime import gateway

class SubscriptionProbeTests(unittest.TestCase):
    def test_probe_passes_only_explicit_directory_and_caps_upstream_attempts(self):
        self.assertFalse({"openai-siwc", "grok"} & {row[0] for row in select_bindings()})
        for unsupported in ("codex", "openai-siwc"):
            with self.assertRaises(RuntimeError):
                select_bindings(unsupported)
        with tempfile.TemporaryDirectory() as directory:
            run = Run.create(Path(directory) / "run", providers="grok", limit=1)
            class Stop(Exception):
                pass
            def spawn(command, **kwargs):
                self.assertEqual(kwargs["env"], {})
                self.assertEqual(command[command.index("--credentials-dir") + 1], "/synthetic/owned/store")
                config = json.loads(Path(command[command.index("--config") + 1]).read_bytes())
                self.assertEqual(config["models"], run.plan["models"])
                self.assertEqual(config["max_attempts"], 1)
                self.assertNotIn("access_token", config)
                raise Stop()
            with patch.dict(os.environ, {
                "MORPHIECORE_PROBE_LIVE": "1",
                "MORPHIECORE_PROBE_CREDENTIALS_DIR": "/synthetic/owned/store",
            }, clear=True), patch("probe_support.runtime.subprocess.Popen", side_effect=spawn), patch(
                "pathlib.Path.read_text", side_effect=AssertionError("no upstream credential reads")
            ):
                with self.assertRaises(Stop):
                    with gateway(run):
                        self.fail("must not start a real binary")
