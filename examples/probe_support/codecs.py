"""Pinned SDK collectors; raw framing is validated before these run."""

import json
from .checks import require


class UnexpectedChatFinish(AssertionError):
    """Closed terminal diagnostics without message content or raw error text."""

    def __init__(self, finish):
        self.finish = (
            finish
            if finish
            in ("stop", "tool_calls", "length", "content_filter", "function_call")
            else "missing_or_unknown"
        )
        super().__init__("unexpected Chat terminal")


def reported_usage(usage, protocol):
    """Extract bounded reported counters; no estimates, sums or missing-to-zero defaults."""
    require(protocol in ("chat", "responses") and (usage is None or isinstance(usage, dict)),
        "usage_metric", "wire")
    usage = usage or {}
    input_key, output_key, input_details, output_details = (
        ("prompt_tokens", "completion_tokens", "prompt_tokens_details", "completion_tokens_details")
        if protocol == "chat"
        else ("input_tokens", "output_tokens", "input_tokens_details", "output_tokens_details")
    )
    require(all(usage.get(key) is None or isinstance(usage[key], dict)
        for key in (input_details, output_details)), "usage_metric", "wire")
    fields = {
        "reported_input_tokens": usage.get(input_key),
        "reported_output_tokens": usage.get(output_key),
        "reported_reasoning_tokens": (usage.get(output_details) or {}).get("reasoning_tokens"),
        "reported_image_tokens": (usage.get(input_details) or {}).get("image_tokens"),
        "reported_cached_tokens": (usage.get(input_details) or {}).get("cached_tokens"),
    }
    require(all(value is None or type(value) is int and 0 <= value <= 10**12
        for value in fields.values()), "usage_metric", "wire")
    return fields


def reasoning_chars(history, protocol):
    """Count admitted readable views, not opaque values or an estimate of token usage."""
    if protocol == "chat":
        return sum(
            len(item["reasoning_content"])
            if item.get("reasoning_content") else sum(
                len(part.get("text") or part.get("summary") or "")
                for part in item.get("reasoning_details") or []
                if part.get("type") in ("reasoning.text", "reasoning.summary")
            ) for item in history
        )
    return sum(len(part.get("text") or "")
        for item in history if item.get("type") == "reasoning"
        for field in ("summary", "content") for part in item.get(field) or []
        if part.get("type") in ("summary_text", "reasoning_text"))


def chat_result(result, streaming, *, allowed_finishes=("stop", "tool_calls"), metrics=None):
    """Accumulate only the admitted typed deltas, including scoped replay fields."""
    if metrics is not None:
        metrics.update(reported_usage(None, "chat"))
    if not streaming:
        if metrics is not None and getattr(result, "usage", None) is not None:
            metrics.update(reported_usage(result.usage.model_dump(mode="json", exclude_unset=True), "chat"))
        if result.choices[0].finish_reason not in allowed_finishes:
            raise UnexpectedChatFinish(result.choices[0].finish_reason)
        message = result.choices[0].message.model_dump(mode="json", exclude_unset=True)
        return (
            [message],
            message.get("content") or "",
            message.get("tool_calls") or [],
        )
    message = {"role": "assistant", "content": None}
    calls, details = ({}, {})
    finish = None
    frames = total = 0
    for chunk in result:
        frames += 1
        wire = chunk.model_dump(mode="json", exclude_unset=True)
        total += len(json.dumps(wire))
        require(frames <= 65536 and total <= 2 * 1024 * 1024, "sdk_shape", "wire")
        if metrics is not None and wire.get("usage") is not None:
            metrics.update(reported_usage(wire["usage"], "chat"))
        if not chunk.choices:
            continue
        choice = wire["choices"][0]
        if choice.get("finish_reason"):
            finish = choice["finish_reason"]
        delta = choice["delta"]
        for field in ("content", "reasoning_content"):
            if delta.get(field) is not None:
                message[field] = (message.get(field) or "") + delta[field]
        for part in delta.get("reasoning_details") or []:
            target = details.setdefault(part["index"], {})
            for key, value in part.items():
                if key in ("summary", "text"):
                    target[key] = target.get(key, "") + value
                else:
                    require(
                        key not in target or target[key] == value, "sdk_shape", "wire"
                    )
                    target[key] = value
        for call in delta.get("tool_calls") or []:
            target = calls.setdefault(
                call["index"], {"type": "function", "function": {"arguments": ""}}
            )
            if call.get("id"):
                require(
                    "id" not in target or target["id"] == call["id"],
                    "sdk_shape",
                    "wire",
                )
                target["id"] = call["id"]
            fn = call.get("function") or {}
            if fn.get("name"):
                target["function"]["name"] = fn["name"]
            target["function"]["arguments"] += fn.get("arguments") or ""
    if finish not in allowed_finishes:
        raise UnexpectedChatFinish(finish)
    if details:
        message["reasoning_details"] = [details[i] for i in sorted(details)]
    if calls:
        message["tool_calls"] = [calls[i] for i in sorted(calls)]
    return ([message], message.get("content") or "", message.get("tool_calls") or [])


def response_result(result, streaming, *, metrics=None):
    def record_usage(snapshot):
        if metrics is not None:
            usage = getattr(snapshot, "usage", None)
            metrics.update(reported_usage(
                usage.model_dump(mode="json", exclude_unset=True) if usage is not None else None,
                "responses"))

    if streaming:
        final = None
        total = frames = 0
        for event in result:
            frames += 1
            total += len(event.model_dump_json())
            require(frames <= 65536 and total <= 2 * 1024 * 1024, "sdk_shape", "wire")
            if event.type in ("response.completed", "response.incomplete", "response.failed"):
                record_usage(event.response)
            if event.type == "response.completed":
                require(final is None, "sdk_shape", "wire")
                final = event.response
            require(
                event.type not in ("response.failed", "response.incomplete", "error"),
                "sdk_shape",
                "wire",
            )
        require(final is not None, "sdk_shape", "wire")
        result = final
    record_usage(result)
    require(result.status == "completed", "sdk_shape", "wire")
    history = [
        item.model_dump(mode="json", exclude_unset=True) for item in result.output
    ]
    text = "".join(
        (
            part["text"]
            for item in history
            if item["type"] == "message"
            for part in item["content"]
            if part["type"] == "output_text"
        )
    )
    return (
        history,
        text,
        [item for item in history if item["type"] == "function_call"],
    )


def opaque_records(value):
    """Keep ciphertext and issuer identity in memory only, never in diagnostics."""
    records = []
    if isinstance(value, dict):
        if value.get("type") == "reasoning.encrypted":
            records.append((value.get("id"), value.get("data")))
        elif value.get("type") == "reasoning" and value.get("encrypted_content"):
            records.append((value.get("id"), value["encrypted_content"]))
        for child in value.values():
            records.extend(opaque_records(child))
    elif isinstance(value, list):
        for child in value:
            records.extend(opaque_records(child))
    return records
