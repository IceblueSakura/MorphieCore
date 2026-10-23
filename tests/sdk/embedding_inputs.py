"""Native Google reports and scoped tokens through the OpenAI SDK, no real egress."""
import json
import sys
from pathlib import Path

import httpx2 as httpx
from jsonschema import Draft202012Validator
from openai import OpenAI

document = json.loads((Path(__file__).resolve().parents[2] / "docs/openapi.json").read_text())
schema = Draft202012Validator({
    "$ref": "#/components/schemas/EmbeddingResponse",
    "components": document["components"],
})


def validate_wire(response):
    response.read()
    schema.validate(response.json())


seen = []
with httpx.Client(trust_env=False, event_hooks={
    "request": [lambda request: seen.append(json.loads(request.content))],
    "response": [validate_wire],
}) as http:
    settings = dict(base_url=sys.argv[1] + "/v1", api_key="synthetic-embedding-client-key-0001",
                    max_retries=0, timeout=5, http_client=http)
    client = OpenAI(**settings)
    strict = OpenAI(**settings, _strict_response_validation=True)
    if sys.argv[2] == "google":
        for encoding in (None, "float", "base64"):
            kwargs = {} if encoding is None else {"encoding_format": encoding}
            # This pinned SDK types embedding as List[float], even for Base64 wire.
            selected = strict if encoding == "float" else client
            result = selected.embeddings.create(model="public-vector", input=["first", "second"], user="private-user", **kwargs)
            assert result.model == "public-vector"
            assert [item.index for item in result.data] == [0, 1]
            assert result.usage.prompt_tokens == result.usage.total_tokens == 7
            expected = "zczMPQAAAL4=" if encoding == "base64" else [
                0.1 if encoding == "float" else 0.10000000149011612, -0.125]
            assert all(item.embedding == expected for item in result.data)
        for text, tokens in (("no-usage", 2), ("details-only", 3)):
            result = strict.embeddings.create(model="public-vector", input=text, encoding_format="float")
            assert result.data[0].embedding == [0.1, -0.125]
            assert result.usage.prompt_tokens == result.usage.total_tokens == tokens
        result = client.embeddings.create(model="public-vector", input="no-usage")
        assert result.data[0].embedding == [0.10000000149011612, -0.125]
        assert result.usage.prompt_tokens == result.usage.total_tokens == 2
        expected_count = 6
    else:
        for tokens in ([1, 2, 3], [[1, 2], [3]]):
            result = strict.embeddings.create(model="public-vector", input=tokens, dimensions=2, encoding_format="float")
            assert len(result.data) == (2 if isinstance(tokens[0], list) else 1)
            assert all(len(item.embedding) == 2 for item in result.data)
            assert result.usage.prompt_tokens == result.usage.total_tokens == 3
        expected_count = 2
assert len(seen) == expected_count
print(json.dumps({"requests": len(seen)}))
