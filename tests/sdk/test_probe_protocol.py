"""Protocol selection is trusted catalog policy, never inferred by a JS model list."""
import unittest
from unittest.mock import patch
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "examples"))
from probe_support import catalog


class ProtocolTests(unittest.TestCase):
    def test_selection_uses_declared_order_and_rejects_unadmitted_protocols(self):
        rows = (("synthetic", "one", None, "UNUSED", ("responses",)),
                ("synthetic", "two", None, "UNUSED", ("chat", "responses")))
        with patch.object(catalog, "BINDINGS", rows):
            self.assertEqual(catalog.select_protocol("one"), "responses")
            self.assertEqual(catalog.select_protocol("two"), "chat")
            self.assertEqual(catalog.select_protocol("two", "responses"), "responses")
            for model, protocol in (("one", "chat"), ("missing", None), ("two", "other")):
                with self.assertRaises(RuntimeError):
                    catalog.select_protocol(model, protocol)
