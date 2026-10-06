"""Live selections fail before reading credentials or touching a socket."""

from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "examples"))
from probe_support.catalog import select_bindings


class SelectionTests(unittest.TestCase):
    def test_shared_provider_selection_can_be_narrowed_without_model_substitution(self):
        rows = select_bindings(
            "xiaomi,longcat", models=["mimo-v2.6-flash", "longcat-2.5-preview"]
        )
        self.assertEqual(
            [row[1] for row in rows], ["mimo-v2.6-flash", "longcat-2.5-preview"]
        )
        for models in ([], ["mimo-v2.6-flash"] * 2, ["deepseek-flash"]):
            with self.assertRaises(RuntimeError):
                select_bindings("xiaomi", models=models)

    def test_subscription_selection_is_explicit_and_does_not_borrow_metered_keys(self):
        rows = select_bindings("aliyun-tokenplan-cn", models=["qwen3.8-flash"])
        self.assertEqual(len(rows), 1)
        self.assertEqual(
            rows[0][2:4],
            ("aliyun-tokenplan-cn-api-key", None),
        )
        for provider, model in [
            ("aliyun-dashscope-cn", "qwen3.8-flash"),
            ("aliyun-tokenplan-cn", "qwen3.8-max"),
        ]:
            with self.assertRaises(RuntimeError):
                select_bindings(provider, models=[model])

    def test_go_subscription_selection_is_explicit_and_chat_only(self):
        rows = select_bindings("opencode-go", models=["hy4-preview"])
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0][4], ("chat",))
        self.assertEqual([row[1] for row in select_bindings("opencode-go")], ["hy4-preview"])
        with self.assertRaises(RuntimeError):
            select_bindings("opencode-go", models=["gpt-6-luna"])

    def test_modelbest_explicit_selection_uses_chat_bindings(self):
        from probe_support.catalog import select_protocol
        rows = select_bindings("modelbest", models=["minicpm5-2b", "minicpm-v-4.6"])
        self.assertEqual([row[1] for row in rows], ["minicpm5-2b", "minicpm-v-4.6"])
        self.assertTrue(all(row[2:5] == ("modelbest-api-key", None, ("chat",)) for row in rows))
        self.assertEqual(select_protocol("minicpm-v-4.6"), "chat")
        with self.assertRaises(RuntimeError):
            select_protocol("minicpm-v-4.6", "responses")

    def test_paused_unknown_and_duplicate_selections_fail_closed(self):
        rows = select_bindings()
        self.assertEqual(rows[0][0], "nvidia")
        self.assertFalse(
            {row[0] for row in rows}
            & {"kimi", "aliyun-tokenplan-cn", "opencode-go", "modelbest"}
        )
        for selection in ("kimi", "nvidia,kimi", "", "unknown", "nvidia,nvidia"):
            with self.assertRaises(RuntimeError):
                select_bindings(selection)
        self.assertEqual(
            list(dict.fromkeys(row[0] for row in select_bindings("zhipu,nvidia"))),
            ["zhipu", "nvidia"],
        )


if __name__ == "__main__":
    unittest.main()
