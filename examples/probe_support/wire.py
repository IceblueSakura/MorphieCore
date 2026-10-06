"""Bound raw gateway bytes before SDK parsing; not a replacement codec.

The final SSE-bearing chunk is held until actual HTTP EOF so an SDK stopping
at DONE cannot hide extra frames. Early client close never drains or succeeds.
"""

import codecs
import json
import re
from .checks import require, ProbeFailure


class Wire:
    def __init__(self, protocol, streaming, limit=2 << 20):
        self.protocol, self.streaming, self.limit = protocol, streaming, limit
        self.bytes = 0
        self.frames = 0
        self.closed = False
        self.failed = False
        self.terminal = None
        self.done = False
        self.text = ""
        self.parts = {}
        self.snapshot = None
        self.finish_seen = False
        self.pending = ""
        self.decoder = codecs.getincrementaldecoder("utf-8")("strict")

    def push(self, raw):
        try:
            self._push(raw)
        except ProbeFailure:
            self.failed = True
            raise
        except (ValueError, TypeError, KeyError, AttributeError, RecursionError):
            self.failed = True
            raise ProbeFailure("wire_shape", "wire") from None

    def _push(self, raw):
        require(not self.closed and not self.failed, "wire_closed", "wire")
        self.bytes += len(raw)
        require(self.bytes <= self.limit, "wire_budget", "wire")
        if not self.streaming:
            return
        try:
            self.pending += self.decoder.decode(raw)
        except UnicodeError:
            raise ProbeFailure("wire_utf8", "wire") from None
        while (delimiter := re.search(r"\r?\n\r?\n", self.pending)) is not None:
            frame, self.pending = (
                self.pending[: delimiter.start()],
                self.pending[delimiter.end() :],
            )
            require(len(frame.encode()) <= 1 << 20, "frame_budget", "wire")
            self.frames += 1
            require(self.frames <= 65536, "frame_count", "wire")
            data = "\n".join(
                line[5:].removeprefix(" ")
                for line in re.split(r"\r?\n", frame)
                if line.startswith("data:")
            )
            if not data:
                continue
            require(not self.done, "post_terminal_data", "wire")
            if data == "[DONE]":
                require(
                    self.protocol == "chat" and self.finish_seen,
                    "premature_done",
                    "wire",
                )
                self.done = True
                continue
            try:
                value = json.loads(data)
            except (ValueError, TypeError):
                raise ProbeFailure("wire_json", "wire") from None
            require(isinstance(value, dict), "wire_object", "wire")
            if self.protocol == "chat":
                require("error" not in value, "wire_error", "wire")
                choices = value.get("choices", [])
                require(
                    isinstance(choices, list) and len(choices) <= 1,
                    "wire_choices",
                    "wire",
                )
                for choice in choices:
                    require(not self.finish_seen, "post_finish_choice", "wire")
                    require(choice.get("index") == 0, "wire_index", "wire")
                    delta = choice.get("delta", {})
                    text = delta.get("content")
                    if text is not None:
                        require(isinstance(text, str), "wire_text", "wire")
                        self.text += text
                    finish = choice.get("finish_reason")
                    if finish is not None:
                        require(
                            finish
                            in ("stop", "tool_calls", "length", "content_filter"),
                            "wire_finish",
                            "wire",
                        )
                        self.terminal = finish
                        self.finish_seen = True
            else:
                kind = value.get("type")
                if kind == "response.output_text.delta":
                    coords = (value.get("output_index"), value.get("content_index"))
                    require(
                        all(type(n) is int and n >= 0 for n in coords),
                        "wire_index",
                        "wire",
                    )
                    text = value.get("delta")
                    require(isinstance(text, str), "wire_text", "wire")
                    self.parts[coords] = self.parts.get(coords, "") + text
                if kind in (
                    "response.completed",
                    "response.incomplete",
                    "response.failed",
                    "error",
                ):
                    self.terminal = kind
                    self.done = True
                    if kind == "response.completed":
                        try:
                            self.snapshot = "".join(
                                part["text"]
                                for item in value["response"]["output"]
                                if item["type"] == "message"
                                for part in item["content"]
                                if part["type"] == "output_text"
                            )
                            require(
                                len(self.snapshot) <= 65536, "wire_text_budget", "wire"
                            )
                        except (KeyError, TypeError):
                            raise ProbeFailure("wire_snapshot", "wire") from None
            require(
                len(self.text) + sum(map(len, self.parts.values())) <= 65536,
                "wire_text_budget",
                "wire",
            )
        require(len(self.pending.encode()) <= (1 << 20) + 3, "frame_budget", "wire")

    def finish(self):
        try:
            self._finish()
        except ProbeFailure:
            self.failed = True
            raise

    def _finish(self):
        require(not self.closed and not self.failed, "wire_closed", "wire")
        if self.streaming:
            try:
                self.pending += self.decoder.decode(b"", final=True)
            except UnicodeError:
                raise ProbeFailure("wire_utf8", "wire") from None
            require(not self.pending.strip() and self.done, "missing_terminal", "wire")
            if self.protocol == "responses":
                self.text = "".join(self.parts[key] for key in sorted(self.parts))
                if self.snapshot is not None:
                    require(
                        self.text == self.snapshot, "delta_snapshot_mismatch", "wire"
                    )
        self.closed = True
