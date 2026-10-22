"""Independent preferred-strict fallback and gateway isolation oracles."""
import copy
from pathlib import Path
import unittest

from drivers import KEY
import strict_tools
import test_client_summary


class StrictToolOracles(unittest.TestCase):
    def body(self):
        body = test_client_summary.ClientSummaryOracles().base(True)
        body["tools"].append({"name": "synthetic-deferred", "defer_loading": True})
        return body

    def test_direct_fallback_keeps_non_tool_request_and_placeholder_unchanged(self):
        original = self.body()
        headers = {"x-api-key": KEY}
        state = strict_tools.State(Path("/synthetic/unused"), True)
        self.assertEqual(state.upstream(headers, original)[0], 400)
        changed = copy.deepcopy(original)
        changed["tools"][0].pop("strict")
        self.assertEqual(state.upstream(headers, changed)[0], 200)
        state.finish()
        with self.assertRaisesRegex(RuntimeError, "^strict_extra_attempt$"):
            state.upstream(headers, changed)
        for mutate, error in [
            (lambda b: b["messages"].append({"role": "user", "content": "changed"}),
             "strict_fallback_changed_request"),
            (lambda b: b["tools"].pop(), "strict_fallback_tool_identity"),
        ]:
            state = strict_tools.State(Path("/synthetic/unused"), True)
            state.upstream(headers, original)
            corrupted = copy.deepcopy(changed)
            mutate(corrupted)
            with self.assertRaisesRegex(RuntimeError, f"^{error}$"):
                state.upstream(headers, corrupted)

    def test_gateway_profile_never_accepts_a_second_client_transmission(self):
        state = strict_tools.State(Path("/synthetic/unused"), False)
        body = self.body()
        state.incoming({}, body)
        self.assertEqual(state.upstream({"x-api-key": "synthetic-client-primary"}, body)[0], 400)
        state.finish()
        with self.assertRaisesRegex(RuntimeError, "^strict_gateway_extra_request$"):
            state.incoming({}, body)


if __name__ == "__main__":
    unittest.main()
