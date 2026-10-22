"""Observer ordering oracles; no Provider or wall-clock performance assertion."""
from types import SimpleNamespace as Event
import unittest

from text_timing_live import GateError, Timing


class TimingOracles(unittest.TestCase):
    def test_only_nonempty_source_text_marks_first_content(self):
        timing = Timing()
        timing.observe(Event(type="message_start"), 0)
        timing.observe(Event(type="content_block_delta", delta=Event(type="thinking_delta", thinking="plan")), 5)
        timing.observe(Event(type="text", text="SDK cumulative snapshot"), 6)
        timing.observe(Event(type="content_block_delta", delta=Event(type="text_delta", text="")), 7)
        self.assertIsNone(timing.first)
        timing.observe(Event(type="content_block_delta", delta=Event(type="text_delta", text="hello")), 10)
        timing.observe(Event(type="content_block_delta", delta=Event(type="text_delta", text=" world")), 20)
        timing.observe(Event(type="message_stop"), 30)
        self.assertEqual(timing.report(40), {"client_first_text_ms": 10, "client_terminal_ms": 30,
                                            "client_elapsed_ms": 40})

    def test_no_text_missing_terminal_and_unvalidated_calls_reject(self):
        timing = Timing()
        timing.observe(Event(type="message_start"), 0)
        with self.assertRaisesRegex(GateError, "^timing_content_or_terminal_order$"):
            timing.report(10)
        timing.observe(Event(type="content_block_delta", delta=Event(type="text_delta", text="partial")), 5)
        with self.assertRaisesRegex(GateError, "^timing_content_or_terminal_order$"):
            timing.report(10)
        for kind in ("tool_use", "server_tool_use"):
            with self.assertRaisesRegex(GateError, "^unexpected_unvalidated_call$"):
                timing.observe(Event(type="content_block_start", content_block=Event(type=kind)), 6)


if __name__ == "__main__":
    unittest.main()
