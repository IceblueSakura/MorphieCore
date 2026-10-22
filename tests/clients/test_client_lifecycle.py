"""Held-stream authority oracles, independent of installed clients and Providers."""
import json
from contextlib import closing
import http.client
import unittest
from unittest.mock import Mock
from urllib.parse import urlsplit

from lifecycle import CANCEL, FIRST, RECOVER, State, authority, frames, observed_held_content


class LifecycleOracles(unittest.TestCase):
    def test_consumer_receipt_can_precede_flush_acknowledgement(self):
        state = State(False)
        def acknowledge(timeout):
            self.assertEqual(timeout, 3)
            state.sent.set()
            return True
        state.sent.wait = Mock(side_effect=acknowledge)
        self.assertFalse(state.sent.is_set())
        observed_held_content(state)
        state.sent.wait.assert_called_once_with(timeout=3)
        state.sent.wait = Mock(return_value=False)
        with self.assertRaisesRegex(RuntimeError, "^content_not_observed_while_held$"):
            observed_held_content(state)
        state.sent.wait = Mock(return_value=True)
        state.dropped.set()
        with self.assertRaisesRegex(RuntimeError, "^content_not_observed_while_held$"):
            observed_held_content(state)

    def test_unsupported_method_is_recorded_before_authority_drains(self):
        state = State(False)
        with authority(state) as origin:
            address = urlsplit(origin)
            with closing(http.client.HTTPConnection(address.hostname, address.port, timeout=3)) as connection:
                connection.request("GET", "/v1/chat/completions")
                response = connection.getresponse()
                self.assertEqual(response.status, 501)
                response.read()
        self.assertEqual(state.errors, ["unsupported_http_method"])

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
