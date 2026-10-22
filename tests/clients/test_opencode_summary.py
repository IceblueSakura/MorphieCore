"""Independent OpenCode summary retry/nudge and rebuilt-checkpoint expectations."""
import copy
import json
from pathlib import Path
import unittest

import opencode_summary as summary
from recovery import CALL, PLAN, READ, TEXT
import test_client_recovery


class OpenCodeSummaryOracles(unittest.TestCase):
    path = Path("/synthetic/sample.txt")

    def summary_body(self):
        body = test_client_recovery.ClientRecoveryOracles().chat(final=True)
        body["messages"][-1]["content"] = (
            "You MUST summarize the conversation above into a structured summary.\n"
            "## Objective\nDo not continue the task or call tools.")
        return body

    def rebuilt(self):
        body = self.summary_body()
        body["messages"] = [
            {"role": "user", "content": f"<conversation-checkpoint>\n<summary>{summary.SUMMARY}</summary>"},
            {"role": "user", "content": summary.AFTER},
            {"role": "assistant", "reasoning_content": PLAN, "tool_calls": [{
                "id": CALL, "type": "function", "function": {
                    "name": "read", "arguments": json.dumps({"path": str(self.path)})}}]},
            {"role": "tool", "tool_call_id": CALL, "content": TEXT},
        ]
        return body

    def test_retry_and_nudge_are_bounded_independent_requests(self):
        state = summary.State(self.path)
        state.attempts = summary.ORDER[:2]
        headers = {"authorization": "Bearer synthetic-client-primary"}
        body = self.summary_body()
        self.assertEqual(summary.classify(body), "summary")
        self.assertEqual(state.upstream(headers, body)[0], 503)
        self.assertEqual(state.upstream(headers, copy.deepcopy(body))[0], 200)
        nudged = copy.deepcopy(body)
        nudged["messages"].append({"role": "user", "content": summary.NUDGE})
        self.assertEqual(summary.classify(nudged), "nudge")
        self.assertEqual(state.upstream(headers, nudged)[0], 200)
        with self.assertRaisesRegex(RuntimeError, "^opencode_attempt_order$"):
            state.upstream(headers, nudged)
        state = summary.State(self.path)
        state.attempts = summary.ORDER[:2]
        state.upstream(headers, body)
        body["messages"][-1]["content"] += "\nchanged retry"
        with self.assertRaisesRegex(RuntimeError, "^opencode_retry_changed_summary$"):
            state.upstream(headers, body)

    def test_summary_requires_native_history_and_nudge_preserves_its_prefix(self):
        body = self.summary_body()
        summary.assert_summary(body, self.path)
        broken = copy.deepcopy(body)
        broken["messages"].pop(2)
        with self.assertRaisesRegex(RuntimeError, "^opencode_summary_history_order$"):
            summary.assert_summary(broken, self.path)
        state = summary.State(self.path)
        state.summary = copy.deepcopy(body)
        state.attempts = summary.ORDER[:4]
        body["messages"][0]["content"] += "\nchanged prefix"
        body["messages"].append({"role": "user", "content": summary.NUDGE})
        with self.assertRaisesRegex(RuntimeError, "^opencode_nudge_changed_summary$"):
            state.upstream({"authorization": "Bearer synthetic-client-primary"}, body)

    def test_rebuilt_checkpoint_excludes_summarized_tool_owners(self):
        body = self.rebuilt()
        self.assertEqual(summary.classify(body), "rebuilt-feedback")
        summary.assert_rebuilt(body, "rebuilt-feedback", self.path)
        for mutate, error in [
            (lambda b: b["messages"][0].update(content=READ), "opencode_rebuilt_checkpoint"),
            (lambda b: b["messages"][-1].update(tool_call_id="wrong-call"), "opencode_rebuilt_tool_result"),
        ]:
            broken = copy.deepcopy(body)
            mutate(broken)
            with self.assertRaisesRegex(RuntimeError, f"^{error}$"):
                summary.assert_rebuilt(broken, "rebuilt-feedback", self.path)


if __name__ == "__main__":
    unittest.main()
