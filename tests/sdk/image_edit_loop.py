"""Standard multipart Images Edit SDK loopback check."""
import json
import sys

import httpx2 as httpx
from openai import OpenAI

PNG_1X1 = (
    b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01"
    b"\x08\x06\x00\x00\x00\x1f\x15c4\x00\x00\x00\rIDATx\x9cc`\x00\x00\x00"
    b"\x02\x00\x01H\xaf\xa4q\x00\x00\x00\x00IEND\xaeB`\x82"
)

with httpx.Client(trust_env=False) as http:
    client = OpenAI(
        base_url=sys.argv[1],
        api_key="synthetic-gateway-client-token-0001",
        max_retries=0,
        timeout=10,
        http_client=http,
    )
    res = client.images.edit(
        model="public-image",
        image=[("a.png", PNG_1X1, "image/png"), ("b.webp", b"RIFFwebp", "image/webp")],
        mask=("mask.png", PNG_1X1, "image/png"),
        prompt="combine these layers",
        n=1,
        input_fidelity="high",
        output_format="webp",
        output_compression=80,
        background="transparent",
        quality="high",
        size="1024x1024",
        safety_identifier="sid-1",
    )
    assert res.created == 123
    assert len(res.data) == 1
    assert res.data[0].b64_json == "AQID"
    assert res.output_format == "webp"
    assert res.usage is not None
    assert res.usage.total_tokens == 13

print(json.dumps({"requests": 1}))
