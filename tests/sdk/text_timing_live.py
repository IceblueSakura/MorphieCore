"""Explicit one-request no-tool Go/Haiku SDK timing; not a latency benchmark."""
import argparse
import copy
import json
from pathlib import Path
import sys
import tempfile
import time
from sdk_support import require_sdk

from messages_capabilities_live import (anthropic, httpx, CAP, DEADLINE_MS, EFFORT, THINKING,
    Gateway, MODEL, BINDING, GateError, answer, failure_summary, profile, require, user)


class Timing:
    def __init__(self):
        self.first = None
        self.terminal = None
        self.started = self.ended = self.events = self.bytes = 0

    def observe(self, event, milliseconds):
        self.events += 1
        require(self.events <= 8192, "timing_event_limit")
        if event.type == "message_start":
            self.started += 1
        elif event.type == "content_block_start":
            require(event.content_block.type not in ("tool_use", "server_tool_use"), "unexpected_unvalidated_call")
        elif event.type == "content_block_delta":
            delta = event.delta
            for name in ("text", "thinking", "signature", "partial_json"):
                value = getattr(delta, name, None)
                if isinstance(value, str):
                    self.bytes += len(value.encode())
            require(self.bytes <= 8 << 20, "timing_delta_byte_limit")
            if delta.type == "text_delta" and delta.text and self.first is None:
                self.first = milliseconds
        elif event.type == "message_stop":
            self.ended += 1
            self.terminal = milliseconds

    def report(self, elapsed):
        require(self.started == self.ended == 1 and self.first is not None and self.terminal is not None
                and 0 <= self.first < self.terminal <= elapsed, "timing_content_or_terminal_order")
        return {"client_first_text_ms": round(self.first, 3), "client_terminal_ms": round(self.terminal, 3),
                "client_elapsed_ms": round(elapsed, 3)}


def measure(gateway):
    body = profile([user("Write about 200 words describing a fictional cedar garden. Use plain text, not tools."),
                    copy.deepcopy(EFFORT)], choice={"type": "none"})
    body["stream"] = True
    require(gateway.budget == 1 and not gateway.requests, "fixed_timing_budget")
    gateway.requests.append(copy.deepcopy(body))
    params = {key: value for key, value in body.items() if key != "stream"}
    origin = f"http://127.0.0.1:{gateway.address.port}"
    observer = Timing()
    with httpx.Client(trust_env=False, timeout=130) as http:
        with anthropic.Anthropic(api_key=gateway.key, base_url=origin, max_retries=0, http_client=http) as client:
            start = time.monotonic()
            with client.messages.stream(**params, extra_headers={
                    "x-morphiecore-conversation-id": "no-tool-timing"}) as stream:
                for event in stream:
                    observer.observe(event, (time.monotonic() - start) * 1000)
                final = stream.get_final_message().model_dump(exclude_none=True)
            elapsed = (time.monotonic() - start) * 1000
    require(bool(answer(final).strip()) and not any(block.get("type") == "tool_use"
            for block in final["content"]), "timing_honest_no_tool_completion")
    return observer.report(elapsed)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--gateway-bin", required=True, type=Path)
    args = parser.parse_args()
    report = {"model": MODEL, "binding": BINDING, "request_budget": 1,
              "max_tokens": CAP, "attempt_timeout_ms": DEADLINE_MS}
    try:
        require_sdk("anthropic")
        require(args.gateway_bin.is_absolute() and args.gateway_bin.is_file(), "explicit_gateway_binary")
        with tempfile.TemporaryDirectory(prefix="morphiecore-text-timing-") as directory:
            gateway = Gateway(args.gateway_bin, Path(directory), 1)
            try:
                try:
                    values = measure(gateway)
                finally:
                    gateway.close()
                wire, = gateway.verify()
                require(wire["thinking"] == THINKING and wire["output_config"] == {"effort": "high"}
                        and wire["tool_choice"] == {"type": "none"} and not wire.get("tools")
                        and wire["messages"][-1] == EFFORT, "timing_profile_changed")
                records = [json.loads(line) for line in (gateway.root / "diagnostics.jsonl").read_text().splitlines()]
                request, = [record for record in records if record.get("event") == "request"]
                report.update(values, source_first_delta_ms=request["first_upstream_delta_ms"],
                              source_elapsed_ms=request["elapsed_ms"])
            except Exception:
                report.update(failure_summary(gateway))
                raise
        report.update(status="passed", client_requests=1, upstream_attempts=1,
                      surface="pinned_sdk_text_delta", real_cancellation_and_latency_slo="not_asserted")
    except Exception as error:
        report.update(status="failed", error=str(error) if isinstance(error, GateError) else type(error).__name__)
        print(json.dumps(report))
        return 1
    print(json.dumps(report))
    return 0


if __name__ == "__main__":
    sys.exit(main())
