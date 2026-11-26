"""Official native Decision SDK consumers; all network authorities are synthetic."""
import sys

from httpx2 import Client
from openai import OpenAI
from typesafe_sdk import RetryPolicy, Score, ScoreAnswer, TypeSafeClient

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

encodings = []
with TypeSafeClient(
    base_url=root + "/typesafe", api_key=key, retry=RetryPolicy(max_retries=0),
    http_client=Client(trust_env=False, event_hooks={
        "request": [lambda request: encodings.append(request.headers.get("content-encoding"))],
    }),
) as client:
    models = client.models.list().models
    assert [(m.name, str(m.release_date)) for m in models] == [("jev-1.14.0", "2026-10-14")]
    result = client.system_one(
        {"record": "Synthetic parcel. " * 100},
        {"priority": Score(criteria=["routine", "urgent"])},
        model=models[0].name,
    )
    assert encodings == [None, "zstd"]
    assert list(result.answers) == ["priority"] and result.usage.input_tokens == 29
    answer = result.answers["priority"]
    assert isinstance(answer, ScoreAnswer) and answer.score == 0.8
    assert answer.legend == {0: "routine", 1: "urgent"}
    assert answer.probabilities == {0: 0.2, 1: 0.8}
