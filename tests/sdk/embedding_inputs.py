"""Native Google reports and scoped tokens through the OpenAI SDK, no real egress."""
import json
import sys

import httpx2 as httpx
from openai import APIResponseValidationError, OpenAI

seen = []
with httpx.Client(trust_env=False, event_hooks={"request": [lambda request: seen.append(json.loads(request.content))]}) as http:
    settings = dict(base_url=sys.argv[1] + "/v1", api_key="synthetic-embedding-client-key-0001",
                    max_retries=0, timeout=5, http_client=http)
    client = OpenAI(**settings)
    if sys.argv[2] == "google":
        for encoding in (None, "float", "base64"):
            kwargs = {} if encoding is None else {"encoding_format": encoding}
            result = client.embeddings.create(model="public-vector", input=["first", "second"], user="private-user", **kwargs)
            assert result.model == "public-vector"
            assert [item.index for item in result.data] == [0, 1]
            assert result.usage.prompt_tokens == result.usage.total_tokens == 7
            expected = "zczMPQAAAL4=" if encoding == "base64" else [
                0.1 if encoding == "float" else 0.10000000149011612, -0.125]
            assert all(item.embedding == expected for item in result.data)
        for text in ("no-usage", "details-only"):
            result = client.embeddings.create(model="public-vector", input=text, encoding_format="float")
            assert result.data[0].embedding == [0.1, -0.125]
            assert getattr(result, "usage", None) is None
        strict = OpenAI(**settings, _strict_response_validation=True)
        try:
            strict.embeddings.create(model="public-vector", input="no-usage", encoding_format="float")
        except APIResponseValidationError:
            pass
        else:
            raise AssertionError("strict OpenAI validation must not certify a missing usage report")
        expected_count = 6
    else:
        for tokens in ([1, 2, 3], [[1, 2], [3]]):
            result = client.embeddings.create(model="public-vector", input=tokens, dimensions=2)
            assert len(result.data) == (2 if isinstance(tokens[0], list) else 1)
            assert all(len(item.embedding) == 2 for item in result.data)
        expected_count = 2
assert len(seen) == expected_count
print(json.dumps({"requests": len(seen)}))
