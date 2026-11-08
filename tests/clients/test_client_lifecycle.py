"""Held-stream authority oracles, independent of installed clients and Providers."""
import copy
import json
from pathlib import Path
import tempfile
from contextlib import closing
import http.client
import unittest
from unittest.mock import Mock
from urllib.parse import urlsplit

from lifecycle import (CANCEL, FIRST, RECOVER, State, ToolState, authority, frames, observed_held_content,
                       assert_recovered_tool)


class LifecycleOracles(unittest.TestCase):
    def test_recovery_cannot_execute_cancelled_calls_or_skip_tool_feedback(self):
        events = [
            {"type": "tool_execution_start", "toolCallId": "new|fc_recovered", "toolName": "write"},
            {"type": "tool_execution_end", "toolCallId": "new|fc_recovered", "toolName": "write",
             "isError": False, "result": "Successfully wrote the synthetic file"},
            {"type": "message_end", "message": {"role": "assistant", "stopReason": "toolUse"}},
            {"type": "message_end", "message": {"role": "assistant", "stopReason": "stop",
              "content": [{"type": "text", "text": "cedar-71"}]}},
        ]
        assert_recovered_tool(events, "new")
        cancelled = copy.deepcopy(events)
        cancelled[0]["toolCallId"] = "cancelled|fc_cancelled"
        with self.assertRaisesRegex(RuntimeError, "^cancelled_or_unrelated_call_executed$"):
            assert_recovered_tool(cancelled, "new")
        with self.assertRaisesRegex(RuntimeError, "^recovered_execution_count$"):
            assert_recovered_tool(events[:2] + events, "new")
        with self.assertRaisesRegex(RuntimeError, "^recovered_assistant_terminals$"):
            assert_recovered_tool(events[:-1], "new")
        with self.assertRaisesRegex(RuntimeError, "^unexpected_recovery_request$"):
            assert_recovered_tool(events + [{"type": "auto_retry_start"}], "new")

    def test_recovery_cannot_replay_cancelled_calls_or_misassociate_write_feedback(self):
        headers = {"authorization": "Bearer synthetic-client-primary"}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "recovered.txt"
            state = ToolState(path)
            state.admit(headers, {"stream": True, "input": [{"role": "user", "content": CANCEL}]})
            recovery = {"stream": True, "input": [{"role": "user", "content": RECOVER}]}
            cancelled = copy.deepcopy(recovery)
            cancelled["input"].append({"type": "function_call", "call_id": state.cancelled_call})
            with self.assertRaisesRegex(RuntimeError, "^cancelled_call_replayed$"):
                state.admit(headers, cancelled)
            feedback = copy.deepcopy(recovery)
            feedback["input"] += [
                {"type": "function_call", "call_id": state.recovered_call, "name": "write",
                 "arguments": json.dumps({"path": str(path), "content": state.content})},
                {"type": "function_call_output", "call_id": "unrelated", "output": "written"},
            ]
            with self.assertRaisesRegex(RuntimeError, "^recovered_call_result_identity$"):
                state.admit(headers, feedback)
            state = ToolState(path)
            state.admit(headers, {"stream": True, "input": [{"role": "user", "content": CANCEL}]})
            state.admit(headers, recovery)
            feedback["input"][-1]["call_id"] = state.recovered_call
            path.write_bytes(state.content.replace("\n", "\r\n").encode("utf-8"))
            with self.assertRaisesRegex(RuntimeError, "^recovered_write_bytes$"):
                state.admit(headers, feedback)

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
            self.assertNotIn("[DONE]", raw)
            values = [json.loads(line.removeprefix("data: ")) for line in raw.splitlines()
                      if line.startswith("data: ")]
            if native:
                self.assertEqual([v["type"] for v in values],
                                 ["message_start", "content_block_start"] + ["content_block_delta"] * 3)
                self.assertIsNone(values[0]["message"]["stop_reason"])
                fragments = [v["delta"]["text"] for v in values if v["type"] == "content_block_delta"]
            else:
                self.assertTrue(all(c["finish_reason"] is None for v in values for c in v["choices"]))
                fragments = [c["delta"]["content"] for v in values for c in v["choices"]]
            self.assertEqual(fragments[0], "prefix ")
            self.assertEqual("".join(fragments), "prefix " + FIRST)
            self.assertTrue(all(FIRST not in fragment for fragment in fragments))
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
