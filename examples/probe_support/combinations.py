"""Independent visual/function/history conversations; no tools execute business code."""
import json
from .checks import require, ProbeFailure
from .images import _grid_png, visual_math_history
from .codecs import reasoning_content_metrics

CASES = {"vision_tool": 2, "vision_tool_named": 2, "vision_parallel": 3, "vision_parallel_reverse": 3,
         "vision_replay": 2, "text_tool": 2, "text_parallel": 3,
         "text_parallel_reverse": 3}
VISION_MODELS = {"mimo-v2.6-flash", "mimo-v2.6-pro", "deepseek-flash",
                 "gpt-6-luna", "grok-4.7"}
VISIBLE_MODELS = {"mimo-v2.6-flash", "mimo-v2.6-pro", "deepseek-flash"}


def arguments(raw):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, "tool_arguments")
            result[key] = value
        return result
    try:
        require(isinstance(raw, str) and len(raw) <= 16384, "tool_arguments")
        value = json.loads(raw, object_pairs_hook=unique)
    except (ValueError, TypeError, RecursionError):
        raise ProbeFailure("tool_arguments") from None
    require(isinstance(value, dict) and set(value) == {"key"}
            and isinstance(value["key"], str), "tool_arguments")
    return value["key"]


def shape(output, visible):
    require(all(item.get("type") in ("reasoning", "message", "function_call")
                and (item.get("type") != "message" or item.get("role") == "assistant"
                     and all(part.get("type") == "output_text"
                             for part in item.get("content") or []))
                for item in output), "combination_shape")
    if visible:
        require(reasoning_content_metrics(output)["reasoning_content_chars"] > 0
                and any(part.get("type") == "reasoning_text"
                        and isinstance(part.get("text"), str) and part["text"].strip()
                        for item in output if item.get("type") == "reasoning"
                        for part in item.get("content") or []), "missing_reasoning_content")


class CallOracle:
    def __init__(self, keys, *, namespace=None, visible=False):
        self.keys, self.namespace, self.visible = tuple(keys), namespace, visible

    def __call__(self, text, calls, output):
        shape(output, self.visible)
        require(len(calls) == len(self.keys), "tool_count")
        identities, keys = [], []
        for item in calls:
            require(item.get("type") == "function_call" and item.get("name") == "lookup",
                    "tool_name")
            require(item.get("namespace") == self.namespace, "tool_namespace")
            identity = item.get("call_id")
            require(isinstance(identity, str) and identity, "tool_identity")
            identities.append(identity)
            keys.append(arguments(item.get("arguments")))
        require(len(set(identities)) == len(identities), "tool_identity")
        require(sorted(keys) == sorted(self.keys), "tool_arguments")


def answer(expected, visible):
    def check(text, calls, output):
        shape(output, visible)
        require(not calls and text.strip() == str(expected), "exact_text")
    return check


def execute(case, invoke, stream, *, model, tool, extra=None):
    """Alternate deliveries and replay only complete actual outputs on this target."""
    vision = case.startswith("vision")
    reverse = case.endswith("_reverse")
    visible = model in VISIBLE_MODELS
    namespace = "probe" if model == "gpt-6.1-sol" else None
    controls = dict(extra or {})
    definition = {"type": "function", **tool}
    controls.update(tools=[{"type":"namespace", "name":namespace,
                           "description":"Synthetic local observations", "tools":[definition]}]
                    if namespace else [definition], tool_choice="auto")
    history = visual_math_history("responses", plain_text=True) if vision else [
        {"role":"user", "content":[{"type":"input_text",
          "text":"Panel one contains 5 red and 4 blue squares; panel two contains 3 red and 6 blue squares."}]}]
    if reverse:
        parts = history[0]["content"]
        if vision:
            parts[1], parts[3] = parts[3], parts[1]
        else:
            parts[0]["text"] = "Panel one contains 3 red and 6 blue squares; panel two contains 5 red and 4 blue squares."
    if case == "vision_replay":
        output, _, _ = invoke(1, history, streaming=stream, extra=dict(extra or {}), oracle=answer(18, visible),
                               check_content=True, check_opaque=True, check_calls=True)
        history.extend(output)
        history.append({"role":"user", "content":[
            {"type":"input_text","text":"Count red R and blue B in this NEW image. Add R*B to your previous answer. Return only the integer."},
            {"type":"input_image","image_url":_grid_png(("RRB","BBB","BBB"))}]})
        invoke(2, history, streaming=not stream, extra=dict(extra or {}), oracle=answer(32, visible),
               check_content=True, check_opaque=True, check_calls=True)
        return
    parallel = "parallel" in case
    prompt = (
        "Read both panels. Call lookup twice in this response, using one key per panel in the form r<R>b<B>, where R and B are its red and blue counts. "
        "After BOTH results, let V1 be the value for panel ONE and V2 for panel TWO. Compute V1*B2 - V2*R1. Return only the integer. Do not invent lookup results."
        if parallel else
        "Read both panels. Call lookup exactly once with key r<R1>b<B1>:r<R2>b<B2> using their red/blue counts. "
        "After its result, compute R1*B2 - B1*R2 + its value. Return only the integer. Do not invent the lookup result."
    )
    history[0]["content"][0]["text"] = (
        ("Read the following images. " if vision else history[0]["content"][0]["text"] + " ")
        + prompt)
    keys = ("r5b4", "r3b6") if parallel else ("r5b4:r3b6",)
    first_controls = dict(controls)
    if case == "vision_tool_named":
        first_controls["tool_choice"] = {"type":"function","name":"lookup"}
        if model in VISIBLE_MODELS:
            first_controls["parallel_tool_calls"] = False
    output, _, calls = invoke(1, history, streaming=stream, extra=first_controls,
                             oracle=CallOracle(keys, namespace=namespace, visible=visible),
                             check_content=True, check_opaque=True, check_calls=True)
    history.extend(output)
    for item in reversed(calls):
        value = {"r5b4":17, "r3b6":29}[arguments(item["arguments"])] if parallel else 37
        history.append({"type":"function_call_output", "call_id":item["call_id"],
                        "output":json.dumps({"value":value})})
    expected = (65 if reverse else -43) if parallel else 55
    output, _, _ = invoke(2, history, streaming=not stream, extra=controls,
                          oracle=answer(expected, visible), check_content=True,
                          check_opaque=True, check_calls=True)
    if parallel:
        history.extend(output)
        history.append({"role":"user", "content":[
            {"type":"input_text", "text":(
                "Use BOTH actual lookup values from this conversation. Count red R and blue B in this NEW image. Return only V1+V2+R*B as an integer."
                if vision else "Using BOTH actual lookup values from this conversation, add 14 to their sum. Return only the integer.")},
            *([{"type":"input_image","image_url":_grid_png(("RRB","BBB","BBB"))}] if vision else [])]})
        invoke(3, history, streaming=stream, extra=controls, oracle=answer(60, visible),
               check_content=True, check_opaque=True, check_calls=True)
