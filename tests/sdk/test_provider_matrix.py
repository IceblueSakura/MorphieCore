"""Live selections fail before reading credentials or touching a socket."""

from pathlib import Path
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "examples"))
from probe_support import catalog


class SelectionTests(unittest.TestCase):
    def test_selection_preserves_provider_order_subsets_and_credential_domains(self):
        rows = (
            ("alpha", "one", "alpha-key", None, ("chat",)),
            ("alpha", "two", "alpha-key", None, ("responses",)),
            ("beta", "three", "beta-key", None, ("chat", "responses")),
        )
        with patch.object(catalog, "BINDINGS", rows):
            self.assertEqual(catalog.select_bindings(), list(rows))
            self.assertEqual(
                catalog.select_bindings("beta,alpha", models=["two", "three"]),
                [rows[2], rows[1]],
            )
            for models in ([], ["one", "one"], ["three"], ["unknown"]):
                with self.subTest(models=models), self.assertRaises(RuntimeError):
                    catalog.select_bindings("alpha", models=models)
            for selection in ("", "unknown", "alpha,unknown", "alpha,alpha"):
                with self.subTest(selection=selection), self.assertRaises(RuntimeError):
                    catalog.select_bindings(selection)

    def test_restricted_provider_classes_require_deliberate_selection(self):
        # These identifiers are selection policy, not a model inventory.
        restricted = ("aliyun-tokenplan-cn", "opencode-go", "modelbest", "grok")
        rows = (
            ("metered", "metered-model", "metered-key", None, ("chat",)),
            *[(name, f"model-{index}", f"key-{index}", None, ("chat",))
              for index, name in enumerate(restricted)],
        )
        with patch.object(catalog, "BINDINGS", rows):
            self.assertEqual(catalog.select_bindings(), [rows[0]])
            for row in rows[1:]:
                self.assertEqual(catalog.select_bindings(row[0], models=[row[1]]), [row])
                with self.assertRaises(RuntimeError):
                    catalog.select_bindings("metered", models=[row[1]])
                with self.assertRaises(RuntimeError):
                    catalog.select_bindings(row[0], models=["metered-model"])


if __name__ == "__main__":
    unittest.main()
