"""Official Decision SDK consumers; all network authorities are synthetic."""
import sys

from httpx2 import Client
from openai import NotFoundError, OpenAI
from typesafe_sdk import (
    Noul, NoulAnswer, RetryPolicy, Score, ScoreAnswer, TypeSafeClient,
    TypeSafeUnprocessableEntityError,
)

root = sys.argv[1]
key = "synthetic-gateway-client-token-0001"

with OpenAI(base_url=root + "/v1", api_key=key, max_retries=0,
            http_client=Client(trust_env=False)) as client:
    models = client.models.list().data
    assert [m.id for m in models] == ["gpt-6-luna"]
    assert client.models.retrieve(models[0].id).id == models[0].id
    result = client.decisions.create(
        model=models[0].id,
        input=[{"role": "user", "content": [
            {"type": "input_json", "data": {"role": "system", "item": "parcel"}},
        ]}],
        questions=[{"name": "action", "type": "choice", "instructions": "Pick an action.",
                    "choices": [{"value": True}, {"value": "true"}]}],
    )
    answer, = result.answers
    assert result.usage.input_tokens == 24 and answer.confidence == 0.41
    assert answer.type == "choice" and answer.choice == "true"
    assert [(type(p.value), p.value, p.probability) for p in answer.probabilities] == [
        (bool, True, 0.2), (str, "true", 0.8),
    ]
    for model in ("gemini-decision-1", "gpt-6-luna-noul"):
        try:
            client.decisions.create(model=model, input="parcel", questions=[
                {"type": "predicate", "instructions": "Did it arrive?"},
            ])
        except NotFoundError:
            pass
        else:
            raise AssertionError("projected Noul bindings must be usable only through TypeSafe")

encodings = []
with TypeSafeClient(
    base_url=root + "/typesafe", api_key=key, retry=RetryPolicy(max_retries=0),
    http_client=Client(trust_env=False, event_hooks={
        "request": [lambda request: encodings.append(request.headers.get("content-encoding"))],
    }),
) as client:
    models = client.models.list().models
    assert [(m.name, str(m.release_date)) for m in models] == [
        ("gemini-decision-1", "2026-11-11"), ("gpt-6-luna-noul", "2026-10-22"),
        ("jev-1.14.0", "2026-10-14"),
    ]
    result = client.system_one(
        {"record": "Synthetic parcel. " * 100},
        {"priority": Score(criteria=["routine", "urgent"])},
        model=models[2].name,
    )
    assert encodings == [None, "zstd"]
    assert list(result.answers) == ["priority"] and result.usage.input_tokens == 29
    answer = result.answers["priority"]
    assert isinstance(answer, ScoreAnswer) and answer.score == 0.8
    assert answer.legend == {0: "routine", 1: "urgent"}
    assert answer.probabilities == {0: 0.2, 1: 0.8}

    result = client.system_one(
        "Synthetic parcel. " * 100,
        {"z_arrived": Noul(instructions="Did it arrive?"),
         "a_damaged": Noul(instructions="Was it damaged?")},
        model=models[0].name,
    )
    assert encodings == [None, "zstd", "zstd"]
    assert result.model == "gemini-decision-1"
    assert result.usage.input_tokens == 71 and result.usage.output_tokens == 0
    assert set(result.answers) == {"z_arrived", "a_damaged"}
    for name, probability in {"z_arrived": 0.95, "a_damaged": 0.08}.items():
        assert isinstance(result.answers[name], NoulAnswer)
        assert result.answers[name].noul == probability

    for state, questions, blocked in (
        ({"record": "not plain text"}, {"arrived": Noul(instructions="Did it arrive?")}, False),
        ("parcel", {"arrived": Noul(instructions="Did it arrive?"),
                    "priority": Score(criteria=["routine", "urgent"])}, False),
        ("blocked", {"arrived": Noul(instructions="Did it arrive?")}, True),
    ):
        try:
            client.system_one(state, questions, model=models[0].name)
        except TypeSafeUnprocessableEntityError as error:
            detail = error.body["detail"]
            if blocked:
                assert detail == "The request could not be completed (decision_blocked)."
            else:
                assert detail[0]["type"] == "value_error"
        else:
            raise AssertionError("unsupported request or block must fail atomically")

    questions = {"z_arrived": Noul(instructions="Did it arrive?"),
                 "a_wet": Noul(instructions="Was it wet?")}
    model = models[1].name
    result = client.system_one("  Parcel arrived.\n", questions, model=model)
    assert result.model == model
    assert result.usage.input_tokens == 35 and result.usage.output_tokens == 0
    assert {key: answer.noul for key, answer in result.nouls.items()} == {
        "z_arrived": 0.96, "a_wet": 0.07,
    }
    try:
        client.system_one("refused", questions, model=model)
    except TypeSafeUnprocessableEntityError as error:
        assert error.status == 422
        assert error.body == {"detail": "The request could not be completed (decision_refused)."}
    else:
        raise AssertionError("a single OpenAI refusal must not publish a partial result")
