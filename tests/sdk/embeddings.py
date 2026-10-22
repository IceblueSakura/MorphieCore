"""Official SDK defaults, explicit float, explicit Base64; no retries or real egress."""
import json
import sys

import httpx2 as httpx
from openai import OpenAI

seen = []
with httpx.Client(trust_env=False, event_hooks={"request": [lambda request: seen.append(json.loads(request.content))]}) as http:
    client = OpenAI(base_url=sys.argv[1] + "/v1", api_key="synthetic-embedding-client-key-0001",
                    max_retries=0, timeout=5, http_client=http)
    for encoding in (None, "float", "base64"):
        kwargs = {} if encoding is None else {"encoding_format": encoding}
        response = client.embeddings.create(model="public-vector", input=["first", "second"], **kwargs)
        assert response.model == "reported-vector"
        assert [item.index for item in response.data] == [1, 0]
        assert response.usage.prompt_tokens == response.usage.total_tokens == 7
        for item in response.data:
            expected = "zczMPQAAAL4=" if encoding == "base64" else [
                0.1 if encoding == "float" and sys.argv[2] == "float" else 0.10000000149011612, -0.125]
            assert item.embedding == expected, (encoding, item.embedding, expected)
assert [request["encoding_format"] for request in seen] == ["base64", "float", "base64"]
print(json.dumps({"requests": len(seen)}))
