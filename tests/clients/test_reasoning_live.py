"""Offline failure oracles for the bounded real-Agent runner; no Provider access."""
import copy
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest

import reasoning_live as live


class ReasoningLiveOracles(unittest.TestCase):
    def test_budget_and_history_are_checked_before_forwarding(self):
        owner = SimpleNamespace(address=SimpleNamespace(geturl=lambda: "http://127.0.0.1:1"),
                                key="synthetic", requests=[])
        path = Path("/synthetic/sample.txt")
        state = live.State(owner, path)
        headers = {"content-length": "200", "authorization": "Bearer synthetic"}
        body = {"model": live.MODEL, "stream": True, "max_completion_tokens": 2048,
                "reasoning_effort": "low", "messages": [{"role": "user", "content": "read"}]}
        invalid = {**body, "max_completion_tokens": 4096}
        with self.assertRaisesRegex(RuntimeError, "^fixed_live_controls$"):
            state.incoming(headers, invalid)
        self.assertEqual(owner.requests, [])
        state.incoming(headers, body)
        feedback = copy.deepcopy(body)
        feedback["messages"] += [{"role": "assistant", "reasoning_content": "Plan", "tool_calls": [
            {"id": "actual-call", "type": "function", "function": {"name": "read",
                "arguments": json.dumps({"path": str(path)})}}]},
            {"role": "tool", "tool_call_id": "actual-call", "content": live.TEXT}]
        broken = copy.deepcopy(feedback)
        broken["messages"][1].pop("reasoning_content")
        with self.assertRaisesRegex(RuntimeError, "^live_reasoning_not_observed$"):
            state.incoming(headers, broken)
        self.assertEqual(len(owner.requests), 1)
        state.incoming(headers, feedback)
        next_turn = copy.deepcopy(feedback)
        next_turn["messages"] += [{"role": "assistant", "content": live.TEXT, "reasoning_content": "Check"},
                                   {"role": "user", "content": "recall"}]
        state.incoming(headers, next_turn)
        with self.assertRaisesRegex(RuntimeError, "^live_request_budget$"):
            state.incoming(headers, next_turn)
        self.assertEqual(len(owner.requests), 3)
        self.assertEqual(state.received_requests, 6)

    def test_capture_requires_complete_terminal_and_exact_reasoning_replay(self):
        def chunk(delta, finish=None):
            return {"exchange": "one", "boundary": "client_response", "status": "body_unit",
                    "value": {"choices": [{"delta": delta, "finish_reason": finish}]}}
        records = [chunk({"reasoning_content": "Plan "}), chunk({"reasoning_content": "🧪"}),
                   chunk({"content": "answer"}), chunk({}, "stop"),
                   {"exchange": "one", "boundary": "client_response", "status": "source_eof"}]
        observed, terminal = live.observed_message(records, "one")
        self.assertEqual(terminal, "stop")
        self.assertEqual(observed["reasoning_content"], "Plan 🧪")
        actual = {"role": "assistant", "content": "answer", "reasoning_content": "Plan 🧪"}
        live.same_message(actual, observed)
        with self.assertRaisesRegex(RuntimeError, "^reasoning_changed$"):
            live.same_message({**actual, "reasoning_content": "changed"}, observed)
        with self.assertRaisesRegex(RuntimeError, "^client_capture_eof$"):
            live.observed_message(records[:-1], "one")
        with self.assertRaisesRegex(RuntimeError, "^honest_terminal$"):
            live.observed_message(records[:-2] + records[-1:], "one")

    def test_failure_reporting_keeps_only_fixed_classes_and_counts(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            owner = SimpleNamespace(root=root, requests=[{}],
                address=SimpleNamespace(geturl=lambda: "http://127.0.0.1:1"), key="synthetic")
            state = live.State(owner, Path("/synthetic/sample.txt"))
            state.received_requests = 1
            (root / "diagnostics.jsonl").write_text(json.dumps({"event": "request",
                "outcome": "error", "stage": "response_head", "upstream_status": 400,
                "candidates": [{}], "request_id": "not-for-output"}) + "\n")
            (root / "capture.jsonl").write_text(json.dumps({"boundary": "upstream_response",
                "status": "body_unit", "value": {"error": {"code": "untrusted-private-code",
                    "message": "model unknown with synthetic-private-marker"}}}) + "\n")
            result = state.failure_report()
            self.assertEqual(result["upstream_attempts"], 1)
            self.assertEqual(result["provider_errors"][0]["mentioned_fields"], ["model"])
            self.assertEqual(result["provider_errors"][0]["code"], "unclassified")
            self.assertNotIn("synthetic-private-marker", json.dumps(result))
            self.assertNotIn("not-for-output", json.dumps(result))


if __name__ == "__main__":
    unittest.main()
