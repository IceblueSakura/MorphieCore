"""Exercise Chat create, typed chunks and derived-view replay on the fixed listener."""
import json
import sys
from sdk_support import check, client_for

from pydantic import BaseModel
from openai.types.chat import (
    ChatCompletionSystemMessageParam, ChatCompletionDeveloperMessageParam,
    ChatCompletionUserMessageParam,
)


class Answer(BaseModel):
    answer: str


def run(base_url: str, stream: bool) -> dict[str, object]:
    """Use synthetic tools, no execution, no external target and no automatic retry."""
    client = client_for(base_url)
    history = [
        ChatCompletionSystemMessageParam(role="system", name="policy", content="Be precise"),
        ChatCompletionDeveloperMessageParam(role="developer", name="author", content="Use tools"),
        ChatCompletionUserMessageParam(role="user", name="requester", content="hello 🧪"),
    ]
    tools = [{"type": "function", "function": {"name": "lookup", "parameters": {"type": "object", "properties": {"n": {"type": "integer"}}, "required": ["n"], "additionalProperties": False}, "strict": True}}]
    allowed = {"type": "allowed_tools", "allowed_tools": {"mode": "required", "tools": [{"type": "function", "function": {"name": "lookup"}}]}}
    counts = []
    try:
        for turn in (1, 2, 3):
            if turn in (1, 2):
                # Consume through the pinned parser so later turns replay real derived views.
                extra = {} if turn == 1 else {"response_format": Answer}
                message, finish, usage = None, None, None
                if stream:
                    content = ""
                    calls = {}
                    count = 0
                    with client.chat.completions.stream(
                            model="fixture-model", messages=history, tools=tools,
                            tool_choice=allowed if turn == 1 else "none", n=1, **extra,
                            stream_options={"include_usage": True, "include_obfuscation": False}) as events:
                        for event in events:
                            if event.type != "chunk":
                                continue
                            count += 1
                            chunk = event.chunk
                            check(chunk.id == "chat-local" and chunk.model == "fixture-model")
                            if not chunk.choices:
                                check(finish is not None)
                                continue
                            check(finish is None)
                            choice = chunk.choices[0]
                            check(choice.index == 0)
                            delta = choice.delta
                            content += delta.content or ""
                            for call in delta.tool_calls or []:
                                if call.index not in calls:
                                    check(call.id and call.function.name)
                                    calls[call.index] = ""
                                calls[call.index] += call.function.arguments or ""
                            finish = choice.finish_reason
                        final = events.get_final_completion()
                    check(list(calls.values()) in ([], ['{"n":1}']))
                    if turn == 2:
                        check(content == '{"answer":"new 🧪"}')
                    message, usage = final.choices[0].message, final.usage
                    counts.append(count)
                else:
                    result = client.chat.completions.parse(
                        model="fixture-model", messages=history, tools=tools,
                        tool_choice=allowed if turn == 1 else "none", n=1, **extra)
                    check(len(result.choices) == 1)
                    choice = result.choices[0]
                    finish, usage = choice.finish_reason, result.usage
                    message = choice.message
                message = message.model_dump(exclude_none=True)
            else:
                params = dict(model="fixture-model", messages=history, tools=tools,
                              tool_choice="none", n=1, stream=stream)
                if stream:
                    content = ""
                    finish = None
                    usage = None
                    count = 0
                    params["stream_options"] = {"include_usage": True, "include_obfuscation": False}
                    with client.chat.completions.create(**params) as chunks:
                        for chunk in chunks:
                            count += 1
                            check(chunk.id == "chat-local" and chunk.model == "fixture-model")
                            if not chunk.choices:
                                check(finish is not None and usage is None)
                                usage = chunk.usage
                                continue
                            check(finish is None)
                            choice = chunk.choices[0]
                            check(choice.index == 0)
                            delta = choice.delta
                            content += delta.content or ""
                            finish = choice.finish_reason
                    message = {"role": "assistant", "content": content or None}
                    counts.append(count)
                else:
                    result = client.chat.completions.create(**params)
                    check(len(result.choices) == 1)
                    choice = result.choices[0]
                    finish, usage = choice.finish_reason, result.usage
                    message = choice.message.model_dump(exclude_none=True)
            check(usage is not None and usage.total_tokens == 6)
            check(usage.prompt_tokens == 4 and usage.completion_tokens == 2)
            check(usage.prompt_tokens_details.image_tokens == 1)
            check(usage.prompt_tokens_details.text_tokens == 3)
            check(usage.completion_tokens_details.text_tokens == 2)
            check(usage.completion_tokens_details.accepted_prediction_tokens == 1)
            check(usage.completion_tokens_details.rejected_prediction_tokens == 0)
            if turn == 1:
                check(finish == "tool_calls")
                call = message["tool_calls"][0]
                check(call["id"] == "call-local")
                check(call["function"]["name"] == "lookup")
                check(call["function"]["arguments"] == '{"n":1}')
                check(call["function"]["parsed_arguments"] == {"n": 1}, "dump must carry the derived view")
                check("name" not in message, "outputs have no official participant report")
                message["name"] = "planner"  # Explicit caller-supplied history label.
                history.extend([message, {"role": "tool", "tool_call_id": "call-local", "content": [{"type": "text", "text": "synthetic result"}, {"type": "text", "text": ""}]}])
            elif turn == 2:
                check(finish == "stop" and message["content"] == '{"answer":"new 🧪"}')
                check(json.loads(message["content"]) == {"answer": "new 🧪"})
                check(message["parsed"] == {"answer": "new 🧪"}, "dump must carry the derived view")
                check("name" not in message, "outputs have no official participant report")
                message["name"] = "writer"
                history.append(message)
            else:
                check(finish == "stop" and message["content"] == "old 🧪")
        custom_turns = 0
        if not stream:
            custom_history = [{"role": "user", "content": "synthetic custom call"}]
            custom_tools = [{"type": "custom", "custom": {"name": "sql",
                "format": {"type": "grammar", "grammar": {
                    "syntax": "regex", "definition": "SELECT [0-9]+"}}}}]
            result = client.chat.completions.create(model="fixture-model",
                messages=custom_history, tools=custom_tools,
                tool_choice={"type": "custom", "custom": {"name": "sql"}})
            check(result.choices[0].finish_reason == "tool_calls")
            message = result.choices[0].message
            call = message.tool_calls[0]
            check(call.type == "custom" and call.id == "call-custom")
            check(call.custom.name == "sql" and call.custom.input == "SELECT 1")
            custom_history.extend([message.model_dump(exclude_none=True),
                {"role": "tool", "tool_call_id": call.id, "content": [
                    {"type": "text", "text": "1"}, {"type": "text", "text": ""}]}])
            result = client.chat.completions.create(model="fixture-model",
                messages=custom_history, tools=custom_tools, tool_choice="none")
            check(result.choices[0].finish_reason == "stop")
            check(result.choices[0].message.content == "custom done")
            custom_turns = 2
        return {"turns": 3, "custom_turns": custom_turns, "stream": stream, "event_counts": counts}
    finally:
        client.close()


if __name__ == "__main__":
    if len(sys.argv) != 3 or sys.argv[2] not in ("json", "sse"):
        raise SystemExit("usage: chat_text_loop.py LOOPBACK_BASE_URL json|sse")
    try:
        print(json.dumps(run(sys.argv[1], sys.argv[2] == "sse")))
    except Exception as exc:
        print(json.dumps({"error": type(exc).__name__, "status": getattr(exc, "status_code", None)}), file=sys.stderr)
        raise SystemExit(1) from None
