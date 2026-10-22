"""Held-stream authority oracles, independent of installed clients and Providers."""
import json
import unittest

from lifecycle import CANCEL, FIRST, RECOVER, State, frames


class LifecycleOracles(unittest.TestCase):
    def test_held_frames_have_content_without_any_success_terminal(self):
        for native in (False, True):
            raw = frames(native, FIRST, False).decode()
            self.assertIn(FIRST, raw)
            self.assertNotIn("[DONE]", raw)
            values = [json.loads(line.removeprefix("data: ")) for line in raw.splitlines()
                      if line.startswith("data: ")]
            if native:
                self.assertEqual([v["type"] for v in values],
                                 ["message_start", "content_block_start", "content_block_delta"])
                self.assertIsNone(values[0]["message"]["stop_reason"])
            else:
                self.assertTrue(all(c["finish_reason"] is None for v in values for c in v["choices"]))
            finished = frames(native, "cedar-71", True).decode()
            self.assertIn("message_stop" if native else "[DONE]", finished)

    def test_only_cancel_then_recovery_is_authorized_with_no_extra_attempt(self):
        headers = {"authorization": "Bearer synthetic-client-primary"}
        state = State(False)
        self.assertEqual(state.admit(headers, {"stream": True, "messages": [CANCEL]}), 1)
        self.assertEqual(state.admit(headers, {"stream": True, "messages": [CANCEL, RECOVER]}), 2)
        with self.assertRaisesRegex(RuntimeError, "^upstream_attempt_budget$"):
            state.admit(headers, {"stream": True, "messages": [RECOVER]})
        state = State(False)
        with self.assertRaisesRegex(RuntimeError, "^upstream_request_order$"):
            state.admit(headers, {"stream": True, "messages": [RECOVER]})


if __name__ == "__main__":
    unittest.main()
