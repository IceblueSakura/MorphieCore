"""Fixed independent scenarios over one shared execution/measurement boundary."""

import json
import time
from .checks import ProbeFailure, require
from .codecs import chat_result, response_result, opaque_records, reasoning_chars
from .ledger import MODELS, ENUMS
from .runtime import session

TOOL = {
    "name": "lookup",
    "description": "Look up a synthetic stored value.",
    "parameters": {
        "type": "object",
        "properties": {"key": {"type": "string"}},
        "required": ["key"],
        "additionalProperties": False,
    },
    "strict": False,
}


def call(
    client,
    transport,
    model,
    protocol,
    scenario,
    history,
    streaming,
    *,
    cap=2048,
    terminal="stop",
    extra=None,
    oracle=None,
    cancel=False,
):
    transport.prepare(model, protocol, scenario, history)
    started = time.monotonic()
    result = None
    metrics = {"sdk_consumed": False, "content_ok": False}
    phase = "consumer"
    try:
        params = {"model": model, "stream": streaming, **(extra or {})}
        if protocol == "chat":
            params.update(messages=history, max_completion_tokens=cap)
            if streaming:
                params["stream_options"] = {
                    "include_usage": True,
                    "include_obfuscation": False,
                }
            result = client.chat.completions.create(**params)
        else:
            params.update(input=history, max_output_tokens=cap)
            result = client.responses.create(**params)
        if cancel:
            require(streaming and protocol == "chat", "cancel_shape", "setup")
            found = False
            for index, chunk in enumerate(result):
                require(index < 32, "cancel_frame_budget", "wire")
                if chunk.choices:
                    choice = chunk.choices[0]
                    require(choice.finish_reason is None, "cancel_too_late")
                    delta = choice.delta.model_dump(mode="json", exclude_unset=True)
                    if delta.get("content") or delta.get("reasoning_content"):
                        found = True
                        break
            require(found, "cancel_no_content")
            result.close()
            metrics["client_closed_before_terminal"] = True
            identity = transport.complete("cancelled", metrics)
            print(json.dumps({"attempt": identity, "state": "cancelled"}), flush=True)
            return [], "", []
        if protocol == "chat":
            output, text, calls = chat_result(
                result,
                streaming,
                allowed_finishes=("stop", "tool_calls", "length", "content_filter"),
                metrics=metrics,
            )
            actual = (
                transport.wire.terminal
                if streaming
                else result.choices[0].finish_reason
            )
        else:
            output, text, calls = response_result(result, streaming, metrics=metrics)
            actual = "response.completed"
        metrics.update(
            sdk_consumed=True,
            terminal=actual,
            text_chars=len(text),
            tool_calls=len(calls),
            reasoning_chars=reasoning_chars(output, protocol),
        )
        if protocol == "responses":
            # Shape counters diagnose replay admission without capturing values.
            metrics["reported_logprob_slots"] = sum(
                "logprobs" in part for item in output if item.get("type") == "message"
                for part in item.get("content") or []
            )
            metrics["reported_opaque_items"] = sum(
                bool(item.get("encrypted_content"))
                for item in output if item.get("type") == "reasoning"
            )
        require(transport.wire.closed, "missing_wire_eof", "wire")
        if streaming:
            require(text == transport.wire.text, "consumer_wire_mismatch", "wire")
        require(
            actual == (terminal if protocol == "chat" else "response.completed"),
            "unexpected_terminal",
        )
        phase = "oracle"
        if oracle:
            oracle(text, calls, output)
        metrics["content_ok"] = True
        metrics["elapsed_ms"] = round((time.monotonic() - started) * 1000)
        identity = transport.complete("passed", metrics)
        print(
            json.dumps({"attempt": identity, "state": "passed", "terminal": actual}),
            flush=True,
        )
        return output, text, calls
    except Exception as error:
        kind = (
            error.kind
            if isinstance(error, ProbeFailure)
            else (
                "http"
                if transport.last_status and transport.last_status >= 400
                else phase
                if transport.wire and transport.wire.closed
                else "transport"
            )
        )
        if (
            transport.wire
            and transport.wire.closed
            and transport.wire.terminal in ("response.incomplete", "response.failed")
        ):
            kind = "oracle"
            metrics.update(sdk_consumed=True, terminal=transport.wire.terminal)
        if kind not in (
            "http",
            "transport",
            "consumer",
            "wire",
            "oracle",
            "budget",
            "setup",
        ):
            kind = "unknown"
        if kind == "oracle":
            code = getattr(error, "code", "other")
            metrics["oracle_failure"] = (
                code if isinstance(code, str) and code in ENUMS["oracle_failure"] else "other"
            )
        metrics.update(
            failure=kind, elapsed_ms=round((time.monotonic() - started) * 1000)
        )
        if transport.attempt is not None:
            identity = transport.complete(
                "oracle_failed" if kind == "oracle" else "failed", metrics
            )
            print(
                json.dumps(
                    {
                        "attempt": identity,
                        "state": "oracle_failed" if kind == "oracle" else "failed",
                        "failure": kind,
                    }
                ),
                flush=True,
            )
        raise ProbeFailure("scenario_failed", kind) from None
    finally:
        if streaming and result is not None:
            result.close()


def expect_text(value):
    def check(text, calls, output):
        require(not calls and text.strip() == value, "exact_text")

    return check


def expect_json(text, calls, output):
    try:
        value = json.loads(text)
    except ValueError:
        raise ProbeFailure("json_answer") from None
    require(
        not calls
        and isinstance(value, dict)
        and set(value) == {"answer"}
        and type(value["answer"]) is int
        and value["answer"] == 7,
        "json_answer",
    )


def expect_visual_math(text, calls, output):
    def unique_object(pairs):
        value = {}
        for key, child in pairs:
            require(key not in value, "visual_math_format")
            value[key] = child
        return value

    try:
        value = json.loads(text, object_pairs_hook=unique_object)
    except ValueError:
        raise ProbeFailure("visual_math_format") from None
    require(not calls, "visual_math_calls")
    require(isinstance(value, dict) and set(value) == {"answer"}
        and type(value["answer"]) is int, "visual_math_format")
    require(value["answer"] == 18, "visual_math_value")


def expect_file_marker(expected):
    def check(text, calls, output):
        require(not calls and text.strip() == expected, "file_marker")
    return check


def expect_call(key):
    def check(text, calls, output):
        require(len(calls) == 1, "tool_count")
        fn = calls[0].get("function", calls[0])
        require(fn.get("name") == "lookup", "tool_name")
        try:
            args = json.loads(fn["arguments"])
        except (ValueError, KeyError):
            raise ProbeFailure("tool_arguments") from None
        require(args == {"key": key}, "tool_arguments")

    return check


def plan_groups(
    run, models, *, cases=("text", "tool"), protocol=None, delivery=None, effort=None
):
    require(effort in (None, "none", "minimal", "medium", "max"), "effort", "setup")
    groups = []
    for model in models:
        require(model in run.plan["models"], "model", "budget")
        for proto in MODELS[model][4]:
            if protocol and proto != protocol:
                continue
            for stream in (False, True):
                if delivery and delivery != ("sse" if stream else "json"):
                    continue
                for case in cases:
                    require(
                        case
                        in (
                            "text",
                            "tool",
                            "history",
                            "json",
                            "length",
                            "cancel",
                            "reasoning",
                            "image",
                            "image_math",
                            "file",
                            "file_url",
                            "file_continue",
                            "file_replay",
                            "file_reasoning",
                            "file_reasoning_math",
                        ),
                        "case",
                        "setup",
                    )
                    require(case not in ("file", "file_url", "file_continue", "file_replay") or model == "gpt-6-luna" and proto == "responses", "file_target", "setup")
                    require(case not in ("file_reasoning", "file_reasoning_math") or model == "gpt-6-luna" and proto == "responses" and effort in (None, "medium"), "file_reasoning_target", "setup")
                    if case in ("file_replay", "file_reasoning", "file_reasoning_math") and not stream:
                        continue
                    if case in ("length", "cancel") and proto != "chat":
                        continue
                    if case == "cancel" and not stream:
                        continue
                    require(
                        case != "reasoning" or model == "gpt-6-luna",
                        "reasoning_target",
                        "setup",
                    )
                    require(
                        case != "reasoning" or effort in (None, "medium"),
                        "reasoning_preset",
                        "setup",
                    )
                    count = {
                        "text": 1,
                        "json": 1,
                        "length": 1,
                        "cancel": 2,
                        "tool": 2,
                        "history": 4,
                        "reasoning": 2,
                        "image": 1,
                        "image_math": 1,
                        "file": 2,
                        "file_url": 2,
                        "file_continue": 1,
                        "file_replay": 2,
                        "file_reasoning": 3,
                        "file_reasoning_math": 3,
                    }[case]
                    cap = min(1024, run.plan["tokens"]) if case in ("file_reasoning", "file_reasoning_math") else 8 if case == "length" else min(512 if case in ("image", "file", "file_url", "file_continue", "file_replay") else 2048, run.plan["tokens"])
                    require(cap <= run.plan["tokens"], "case_budget", "budget")
                    selected_effort = (
                        "medium" if case in ("reasoning", "file_reasoning", "file_reasoning_math") else effort or "default"
                    )
                    group = f"sdk:{model}:{proto}:{'sse' if stream else 'json'}:{case}:{selected_effort}"
                    groups.append((model, proto, stream, case, group, count, cap))
    require(bool(groups), "empty_matrix", "setup")
    return groups


def matrix(
    run, models, *, cases=("text", "tool"), protocol=None, delivery=None, effort=None
):
    groups = plan_groups(
        run, models, cases=cases, protocol=protocol, delivery=delivery, effort=effort
    )
    run.register(
        [
            (model, f"{group}:{n}", cap)
            for model, proto, stream, case, group, count, cap in groups
            for n in range(1, count + 1)
        ]
    )
    success = True
    stopped = set()
    with session(run, models) as (client, transport):
        for model, proto, stream, case, group, count, cap in groups:
            if model in stopped:
                continue
            extra = {}
            if MODELS[model][0] == "opencode-go":
                # The group is one synthetic conversation, including all tool-result rounds.
                extra["extra_body"] = {"session_id": f"{run.plan['id']}:{group}"}
            if effort is not None:
                extra.update(
                    {"reasoning_effort": effort}
                    if proto == "chat"
                    else {"reasoning": {"effort": effort, "summary": "auto"}}
                )

            def invoke(n, history, **options):
                return call(
                    client,
                    transport,
                    model,
                    proto,
                    f"{group}:{n}",
                    history,
                    options.pop("streaming", stream),
                    cap=cap,
                    **options,
                )

            try:
                if case in ("file_reasoning", "file_reasoning_math"):
                    from .files import file_history
                    history = file_history()
                    math_case = case == "file_reasoning_math"
                    if math_case:
                        history[0]["content"][-1]["text"] = ("Extract the numeric suffix of each build and patch marker from the PDF, multiply them, and return only JSON with integer field answer. Do not omit either factor.")
                    controls = {"reasoning": {"effort": "medium", "summary": "auto"},
                                "include": ["reasoning.encrypted_content"]}
                    if math_case:
                        controls["text"] = {"format": {"type": "json_object"}}
                    for n, marker in enumerate(("BUILD-A17", "PATCH-B29", "BUILD-A17"), 1):
                        def check(text, calls, output, marker=marker, n=n):
                            if math_case:
                                value = json.loads(text)
                                require(not calls and isinstance(value,dict) and set(value) == {"answer"} and type(value["answer"]) is int and value["answer"] == (493,510,539)[n-1], "file_math")
                            else:
                                expect_file_marker(marker)(text, calls, output)
                            records = opaque_records(output)
                            require(bool(records) and all(
                                isinstance(identity,str) and identity and
                                isinstance(token,str) and token for identity,token in records),
                                "missing_opaque")
                        output, _, _ = invoke(n, history, extra=controls,
                            streaming=n != 2, oracle=check)
                        history.extend(output)
                        if n < 3:
                            field = "patch" if n == 1 else "build"
                            history.append({"role":"user","content":
                                (f"Add the numeric suffix of the {'build' if n == 1 else 'patch'} marker from the original PDF to your previous answer. Return only JSON with integer field answer."
                                 if math_case else f"From the same document, return only the exact {field} marker, without quotes or explanation.")})
                elif case == "file_url":
                    from .files import file_url_history
                    history = file_url_history()
                    output, _, _ = invoke(1, history, extra=extra, oracle=expect_text("Dummy PDF file"))
                    history.extend(output)
                    history.append({"role":"user","content":"How many words are in the visible text of the original PDF? Return only the integer."})
                    invoke(2, history, extra=extra, oracle=expect_text("3"))
                elif case == "file_continue":
                    from .files import file_continuation_history
                    invoke(1, file_continuation_history(), extra=extra,
                        oracle=expect_file_marker("PATCH-B29"))
                elif case in ("file", "file_replay"):
                    from .files import file_history
                    history = file_history()
                    output, _, _ = invoke(1, history, extra=extra,
                        oracle=expect_file_marker("BUILD-A17"))
                    history.extend(output)
                    history.append({"role": "user", "content": "From the same document, return only the exact patch marker, without quotes or explanation."})
                    invoke(2, history, extra=extra, oracle=expect_file_marker("PATCH-B29"),
                        streaming=False if case == "file_replay" else stream)
                elif case in ("image", "image_math"):
                    from .images import image_history, visual_math_history

                    # The explicit vision preset uses effort only, no unrelated summary control.
                    controls = (
                        {"reasoning_effort": effort} if proto == "chat"
                        else {"reasoning": {"effort": effort}}
                    ) if effort is not None else {}
                    invoke(1,
                        image_history(proto) if case == "image" else visual_math_history(proto),
                        extra=controls,
                        oracle=expect_text("red,blue") if case == "image" else expect_visual_math)
                elif case in ("text", "json", "length", "cancel"):
                    prompt = {
                        "text": "Reply with exactly pong.",
                        "json": "Return only JSON with exactly one integer field answer equal to 7.",
                        "length": "Write alpha 200 times separated by spaces. Do not summarize.",
                        "cancel": "Write alpha 200 times separated by spaces. Do not summarize.",
                    }[case]
                    if case == "json":
                        extra.update(
                            {"response_format": {"type": "json_object"}}
                            if proto == "chat"
                            else {"text": {"format": {"type": "json_object"}}}
                        )
                    invoke(
                        1,
                        [{"role": "user", "content": prompt}],
                        extra=extra,
                        terminal="length" if case == "length" else "stop",
                        cancel=case == "cancel",
                        oracle=expect_json
                        if case == "json"
                        else expect_text("pong")
                        if case == "text"
                        else lambda t, c, o: require(not c, "unexpected_call"),
                    )
                    if case == "cancel":
                        invoke(
                            2,
                            [{"role": "user", "content": "Reply with exactly pong."}],
                            extra=extra,
                            oracle=expect_text("pong"),
                        )
                elif case == "reasoning":
                    controls = (
                        {
                            "reasoning_effort": "medium",
                            "response_format": {"type": "json_object"},
                        }
                        if proto == "chat"
                        else {
                            "reasoning": {"effort": "medium", "summary": "auto"},
                            "include": ["reasoning.encrypted_content"],
                            "text": {"format": {"type": "json_object"}},
                        }
                    )
                    history = [
                        {
                            "role": "user",
                            "content": "Compute (317 * 43) + (89 * 17). Return only JSON with integer field answer.",
                        }
                    ]
                    for n in (1, 2):

                        def check(text, calls, output):
                            value = json.loads(text)
                            require(
                                not calls
                                and type(value.get("answer")) is int
                                and value["answer"] == 15144 + (37 if n == 2 else 0),
                                "reasoning_answer",
                            )
                            if n == 1:
                                records = opaque_records(output)
                                require(
                                    bool(records)
                                    and all(
                                        isinstance(identity, str)
                                        and identity
                                        and isinstance(token, str)
                                        and token
                                        for identity, token in records
                                    ),
                                    "missing_opaque",
                                )

                        output, _, _ = invoke(n, history, extra=controls, oracle=check)
                        history.extend(output)
                        if n == 1:
                            history.append(
                                {
                                    "role": "user",
                                    "content": "Add 37 to that answer. Return only JSON with integer field answer.",
                                }
                            )
                else:
                    history = []
                    extra["tools"] = (
                        [{"type": "function", "function": TOOL}]
                        if proto == "chat"
                        else [{"type": "function", **TOOL}]
                    )
                    extra["tool_choice"] = "auto"
                    for n, (key, value) in enumerate(
                        (("alpha", 17), ("beta", 29))[: 1 if case == "tool" else 2]
                    ):
                        history.append(
                            {
                                "role": "user",
                                "content": f"Call lookup for key {key}. After receiving its result, reply with only its numeric value, without words or punctuation.",
                            }
                        )
                        output, _, calls = invoke(
                            n * 2 + 1,
                            history,
                            terminal="tool_calls",
                            extra=extra,
                            oracle=expect_call(key),
                        )
                        history.extend(output)
                        history.append(
                            {
                                "role": "tool",
                                "tool_call_id": calls[0]["id"],
                                "content": json.dumps({"value": value}),
                            }
                            if proto == "chat"
                            else {
                                "type": "function_call_output",
                                "call_id": calls[0]["call_id"],
                                "output": json.dumps({"value": value}),
                            }
                        )
                        output, _, _ = invoke(
                            n * 2 + 2,
                            history,
                            extra=extra,
                            oracle=expect_text(str(value)),
                        )
                        history.extend(output)
            except ProbeFailure as error:
                success = False
                if error.kind != "oracle" or not run.plan["continue_oracle"]:
                    stopped.add(model)
    return success
