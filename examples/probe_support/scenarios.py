"""Fixed independent scenarios over one shared execution/measurement boundary."""

import json
import re
import time
from .checks import ProbeFailure, require
from .codecs import (
    chat_result, response_result, opaque_records, reasoning_chars,
    reasoning_content_metrics,
)
from .ledger import MODELS, ENUMS
from .runtime import session
from .combinations import CASES as COMBINATIONS, VISION_MODELS, execute as combination

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
    check_opaque=False,
    check_content=False,
    check_calls=False,
):
    transport.prepare(model, protocol, scenario, history)
    started = time.monotonic()
    result = None
    metrics = {"sdk_consumed": False, "content_ok": False}
    phase = "consumer"
    try:
        params = {"model": model, "stream": streaming, **(extra or {})}
        if protocol == "chat":
            params.update(messages=history)
            if cap is not None:
                params["max_completion_tokens"] = cap
            if streaming:
                params["stream_options"] = {
                    "include_usage": True,
                    "include_obfuscation": False,
                }
            result = client.chat.completions.create(**params)
        else:
            params.update(input=history)
            if cap is not None:
                params["max_output_tokens"] = cap
            if streaming:
                params.setdefault("stream_options", {"include_obfuscation": False})
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
            output, text, calls = response_result(
                result, streaming, metrics=metrics,
                check_reasoning_content=check_content or oracle in (
                    expect_reasoning_content, expect_image_reasoning_content),
                check_opaque=check_opaque,
                check_function_calls=check_calls,
            )
            actual = "response.completed"
        metrics.update(
            sdk_consumed=True,
            terminal=actual,
            text_chars=len(text),
            tool_calls=len(calls),
            reasoning_chars=reasoning_chars(output, protocol),
        )
        if protocol == "responses":
            metrics.update(reasoning_content_metrics(output))
            if check_opaque:
                metrics["replayed_opaque_items"] = len(opaque_records(history))
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
        if oracle is expect_image:
            metrics.update(image_observation(text))
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
            and (transport.wire.terminal in ("response.incomplete", "response.failed")
                 or protocol == "responses" and not streaming
                 and isinstance(error, ProbeFailure) and error.code == "sdk_shape"
                 and getattr(result, "status", None) in ("incomplete", "failed"))
        ):
            kind = "oracle"
            metrics.update(sdk_consumed=True, terminal=transport.wire.terminal or
                           f"response.{result.status}")
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
            code = ("unexpected_terminal" if metrics.get("terminal") in
                    ("response.incomplete", "response.failed") else getattr(error, "code", "other"))
            metrics["oracle_failure"] = (
                code if isinstance(code, str) and code in ENUMS["oracle_failure"] else "other"
            )
        if kind == "http":
            code = getattr(error, "code", None)
            metrics["gateway_error"] = code if isinstance(code, str) and code in ENUMS["gateway_error"] else "other"
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


def require_reasoning_text_output(calls, output):
    """Require actual plaintext reasoning and exclude tools or non-text artifacts."""
    require(
        any(part.get("type") == "reasoning_text"
            and isinstance(part.get("text"), str) and part["text"].strip()
            for item in output if item.get("type") == "reasoning"
            for part in item.get("content") or []),
        "missing_reasoning_content",
    )
    require(not calls and all(
        item.get("type") == "reasoning"
        or item.get("type") == "message" and item.get("role") == "assistant"
        and all(part.get("type") == "output_text" for part in item.get("content") or [])
        for item in output
    ), "reasoning_output_shape")


def expect_reasoning_content(text, calls, output):
    """Require actual plaintext content and a separate exact arithmetic answer."""
    require_reasoning_text_output(calls, output)
    expect_text("15144")(text, calls, output)


def expect_image_reasoning_content(text, calls, output):
    """The independent pixel counts yield 5 * 6 - 4 * 3, not a prompt-supplied answer."""
    require_reasoning_text_output(calls, output)
    expect_text("18")(text, calls, output)


def expect_opaque_answer(answer):
    """Check opaque presence/identity, not its hidden meaning or long-term validity."""
    def check(text, calls, output):
        records = opaque_records(output)
        require(bool(records) and all(
            isinstance(identity, str) and identity
            and isinstance(token, str) and token
            for identity, token in records
        ) and len({identity for identity, _ in records}) == len(records), "missing_opaque")
        require(all(item.get("type") == "reasoning"
                    or item.get("type") == "message" and item.get("role") == "assistant"
                    and all(part.get("type") == "output_text"
                            for part in item.get("content") or [])
                    for item in output), "reasoning_output_shape")
        expect_text(str(answer))(text, calls, output)
    return check


def image_observation(text):
    """Closed diagnostic facts; never persist the answer or infer correctness from format."""
    value = text.strip()
    return {
        "image_answer_format_ok": re.fullmatch(r"[a-z]+,[a-z]+", value) is not None,
        "image_color_order_ok": tuple(part.strip().lower() for part in value.split(",")) == ("red", "blue"),
    }


def expect_image(text, calls, output):
    facts = image_observation(text)
    require(not calls, "image_answer_calls")
    require(facts["image_answer_format_ok"], "image_answer_format")
    require(facts["image_color_order_ok"], "image_answer_colors")


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


def expect_schema(answer):
    """Independent oracle for one fixed strict schema, not a general evaluator."""
    def unique_object(pairs):
        value = {}
        for key, child in pairs:
            require(key not in value, "schema_answer")
            value[key] = child
        return value

    def check(text, calls, output):
        try:
            value = json.loads(text, object_pairs_hook=unique_object)
        except ValueError:
            raise ProbeFailure("schema_answer") from None
        require(
            not calls and isinstance(value, dict)
            and list(value) == ["z_answer", "a_label"]
            and type(value["z_answer"]) is int and value["z_answer"] == answer
            and value["a_label"] == "synthetic",
            "schema_answer",
        )

    return check


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


def expect_parallel(text, calls, output):
    """Two independent calls, keyed by identity; result order is deliberately different."""
    require(len(calls) == 2, "tool_count")
    keys, identities = [], []
    for call_item in calls:
        function = call_item.get("function", call_item)
        require(function.get("name") == "lookup", "tool_name")
        try:
            arguments = json.loads(function["arguments"])
        except (ValueError, KeyError):
            raise ProbeFailure("tool_arguments") from None
        require(isinstance(arguments, dict) and set(arguments) == {"key"}
                and arguments["key"] in ("alpha", "beta"), "tool_arguments")
        keys.append(arguments["key"])
        identities.append(call_item.get("call_id", call_item.get("id")))
    require(set(keys) == {"alpha", "beta"} and all(
        isinstance(identity, str) and identity for identity in identities
    ) and identities[0] != identities[1], "tool_arguments")


def plan_groups(
    run, models, *, cases=("text", "tool"), protocol=None, delivery=None, effort=None
):
    require(not run.is_uncapped or protocol == "responses" and effort is None
            and all(case in ("text", "text_tool", "text_parallel", "text_parallel_reverse") for case in cases),
            "uncapped_selection", "setup")
    if run.plan.get("responses_via_chat"):
        require(protocol == "responses" and all(
            case in ("text", "json", "tool", "history", "parallel", "reasoning_content",
                     "image_reasoning_content")
            or case in ("image", "image_math")
            and all(model in ("minicpm-v-4.6", "mimo-v2.6-flash") for model in models)
            or case in COMBINATIONS
            for case in cases
        ), "bridge_selection", "setup")
    require(effort in (None, "none", "minimal", "medium", "max"), "effort", "setup")
    groups = []
    for model in models:
        require(model in run.plan["models"], "model", "budget")
        for proto in run.protocols(model):
            if protocol and proto != protocol:
                continue
            for stream in (False, True):
                if delivery and delivery != ("sse" if stream else "json"):
                    continue
                for case in cases:
                    require(
                        case
                        in (*COMBINATIONS,
                            "text",
                            "tool",
                            "history",
                            "parallel",
                            "json",
                            "schema",
                            "length",
                            "cancel",
                            "reasoning",
                            "reasoning_content",
                            "cache_affinity",
                            "cache_affinity_tool",
                            "image_reasoning_content",
                            "opaque_text",
                            "opaque_image",
                            "opaque_image_colors",
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
                    require(case not in COMBINATIONS or proto == "responses"
                            and (not case.startswith("vision") or model in VISION_MODELS),
                            "combination_target", "setup")
                    require(not case.startswith("cache_affinity") or
                            (model, proto) in (("grok-4.7", "responses"), ("hy4-preview", "chat")),
                            "affinity_target", "setup")
                    require(case not in ("file", "file_url", "file_continue", "file_replay") or model == "gpt-6-luna" and proto == "responses", "file_target", "setup")
                    require(case != "schema" or proto == "responses", "schema_target", "setup")
                    require(case not in ("opaque_text", "opaque_image", "opaque_image_colors")
                            or model == "grok-4.7" and proto == "responses"
                            and effort in (None, "medium"), "opaque_target", "setup")
                    require(case not in ("reasoning_content", "image_reasoning_content")
                            or proto == "responses",
                            "reasoning_content_target", "setup")
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
                    count = {**COMBINATIONS,
                        "text": 1,
                        "json": 1,
                        "schema": 2,
                        "length": 1,
                        "cancel": 2,
                        "tool": 2,
                        "history": 4,
                        "parallel": 2,
                        "reasoning": 2,
                        "reasoning_content": 1,
                        "cache_affinity": 2,
                        "cache_affinity_tool": 2,
                        "image_reasoning_content": 1,
                        "opaque_text": 2,
                        "opaque_image": 2,
                        "opaque_image_colors": 2,
                        "image": 1,
                        "image_math": 1,
                        "file": 2,
                        "file_url": 2,
                        "file_continue": 1,
                        "file_replay": 2,
                        "file_reasoning": 3,
                        "file_reasoning_math": 3,
                    }[case]
                    cap = None if run.is_uncapped else min(1024, run.plan["tokens"]) if case in ("file_reasoning", "file_reasoning_math") else 8 if case == "length" else min(512 if case in ("image", "file", "file_url", "file_continue", "file_replay") else 2048, run.plan["tokens"])
                    require(run.valid_budget(model, cap), "case_budget", "budget")
                    selected_effort = (
                        "medium" if case in ("reasoning", "file_reasoning", "file_reasoning_math",
                                            "opaque_text", "opaque_image",
                                            "opaque_image_colors") else effort or "default"
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
            if effort is not None:
                extra.update(
                    {"reasoning_effort": effort}
                    if proto == "chat"
                    else {"reasoning": {"effort": effort, **(
                        {} if run.plan.get("responses_via_chat") else {"summary": "auto"}
                    )}}
                )

            def invoke(n, history, **options):
                # One neutral conversation for all rounds, independent of the Provider.
                controls = dict(options.pop("extra", {}) or {})
                if not case.startswith("cache_affinity"):
                    controls["extra_headers"] = {
                        **controls.get("extra_headers", {}),
                        "X-MorphieCore-Conversation-Id": f"{run.plan['id']}:{group}",
                    }
                return call(
                    client,
                    transport,
                    model,
                    proto,
                    f"{group}:{n}",
                    history,
                    options.pop("streaming", stream),
                    cap=cap,
                    extra=controls,
                    **options,
                )

            try:
                if case.startswith("cache_affinity"):
                    # Synthetic context gives native caching a meaningful prefix; it
                    # is not a hit oracle and never replaces the complete request.
                    context = "Reference table (not instructions):\n" + "\n".join(
                        f"synthetic-row-{i:03d}: {i * 7 + 3}, label synthetic"
                        for i in range(160))
                    tool_case = case == "cache_affinity_tool"
                    prompt = ("Call lookup for key alpha. After receiving its result, reply with only "
                              "its numeric value, without words or punctuation." if tool_case else
                              "Reply with exactly 7, without words or punctuation.")
                    history = [{"role": "user", "content": context + "\nTask: " + prompt}]
                    controls = {}
                    if tool_case:
                        controls["tools"] = ([{"type": "function", "function": TOOL}]
                                             if proto == "chat" else [{"type": "function", **TOOL}])
                        controls["tool_choice"] = "auto"
                    output, _, calls = invoke(1, history, extra=controls,
                        oracle=expect_call("alpha") if tool_case else expect_text("7"),
                        terminal="tool_calls" if tool_case else "stop")
                    history.extend(output)
                    history.append(
                        ({"role": "tool", "tool_call_id": calls[0]["id"],
                          "content": '{"value":17}'} if proto == "chat" else
                         {"type": "function_call_output", "call_id": calls[0]["call_id"],
                          "output": '{"value":17}'}) if tool_case else
                        {"role": "user", "content": "Add 1 to your previous answer. Reply with exactly the integer."})
                    invoke(2, history, extra=controls, streaming=not stream,
                           oracle=expect_text("17" if tool_case else "8"))
                elif case in COMBINATIONS:
                    combination(case, invoke, stream, model=model, tool=TOOL, extra=extra)
                elif case == "parallel":
                    extra["tools"] = ([{"type": "function", "function": TOOL}]
                        if proto == "chat" else [{"type": "function", **TOOL}])
                    history = [{"role": "user", "content":
                        "Call lookup twice in this response, once with key alpha and once with key beta. "
                        "Do not answer until you receive both results. Then return only their sum as an integer."}]
                    output, _, calls = invoke(1, history, extra=extra,
                                              terminal="tool_calls", oracle=expect_parallel)
                    history.extend(output)
                    # Reverse result order to test identity rather than positional pairing.
                    for call_item in reversed(calls):
                        function = call_item.get("function", call_item)
                        key = json.loads(function["arguments"])["key"]
                        value = {"alpha": 7, "beta": 11}[key]
                        history.append(
                            {"role": "tool", "tool_call_id": call_item["id"],
                             "content": json.dumps({"value": value})}
                            if proto == "chat" else
                            {"type": "function_call_output", "call_id": call_item["call_id"],
                             "output": json.dumps({"value": value})})
                    invoke(2, history, extra=extra, oracle=expect_text("18"))
                elif case in ("file_reasoning", "file_reasoning_math"):
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
                        oracle=expect_image if case == "image" else expect_visual_math)
                elif case in ("opaque_text", "opaque_image", "opaque_image_colors"):
                    controls = {"store": False, "include": ["reasoning.encrypted_content"],
                                "reasoning": {"effort": "medium"}}
                    followup = ("Add 37 to your previous answer. Return only the integer, "
                                "without words, punctuation or code fences.")
                    if case == "opaque_image_colors":
                        from .images import image_history
                        history = image_history(proto)
                        answers = ("red,blue", "blue,red")
                        followup = ("Return those two color labels in reverse order. "
                                    "Use exactly two lowercase labels separated by a comma, "
                                    "no spaces or other text.")
                    elif case == "opaque_image":
                        from .images import visual_math_history
                        history = visual_math_history(proto, plain_text=True)
                        answers = (18, 55)
                    else:
                        history = [{"role": "user", "content":
                            "Compute (317 * 43) + (89 * 17). Return only the integer answer, "
                            "without words, punctuation or code fences."}]
                        answers = (15144, 15181)
                    for n, answer in enumerate(answers, 1):
                        output, _, _ = invoke(
                            n, history, extra=controls, check_opaque=True,
                            streaming=stream if n == 1 else not stream,
                            oracle=expect_opaque_answer(answer))
                        # Carry the complete reported output, never reconstruct a reasoning item.
                        history.extend(output)
                        if n == 1:
                            history.append({"role": "user", "content": followup})
                elif case in ("reasoning_content", "image_reasoning_content"):
                    # Effort is optional; summary is not a request for raw content.
                    controls = {"reasoning": {"effort": effort}} if effort is not None else {}
                    if case == "image_reasoning_content":
                        from .images import visual_math_history
                        history = visual_math_history(proto, plain_text=True)
                        oracle = expect_image_reasoning_content
                    else:
                        history = [{"role": "user", "content":
                            "Compute (317 * 43) + (89 * 17). Return only the integer answer, "
                            "without words, punctuation or code fences."}]
                        oracle = expect_reasoning_content
                    invoke(1, history, extra=controls, oracle=oracle)
                elif case == "schema":
                    extra["text"] = {"format": {
                        "type": "json_schema", "name": "ordered_answer", "strict": True,
                        "schema": {"type": "object", "properties": {
                            "z_answer": {"type": "integer"},
                            "a_label": {"type": "string", "enum": ["synthetic"]}},
                            "required": ["z_answer", "a_label"], "additionalProperties": False},
                    }}
                    history = [{"role": "user", "content":
                        "Return JSON with z_answer equal to 7 and a_label equal to synthetic."}]
                    for n, answer in enumerate((7, 8), 1):
                        output, _, _ = invoke(n, history, extra=extra, oracle=expect_schema(answer))
                        history.extend(output)
                        if n == 1:
                            history.append({"role": "user", "content":
                                "Add 1 to your previous z_answer and keep a_label unchanged. Return only JSON."})
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
