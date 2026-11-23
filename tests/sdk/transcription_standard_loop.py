"""Standard multipart Transcription SDK loopback check (json and text delivery)."""
import io
import json
import sys
import wave

import httpx2 as httpx
from openai import OpenAI

buffer = io.BytesIO()
with wave.open(buffer, "wb") as audio:
    audio.setnchannels(1)
    audio.setsampwidth(2)
    audio.setframerate(24000)
    audio.writeframes(b"\x00\x00\x01\x00")
wav_bytes = buffer.getvalue()

with httpx.Client(trust_env=False) as http:
    client = OpenAI(
        base_url=sys.argv[1],
        api_key="synthetic-gateway-client-token-0001",
        max_retries=0,
        timeout=10,
        http_client=http,
    )
    json_res = client.audio.transcriptions.create(
        model="standard-transcription",
        file=("clip.wav", wav_bytes, "audio/wav"),
        languages=["en", "zh"],
        keywords=["MorphieCore", "IR"],
        prompt="Technical.",
        temperature=0.25,
        response_format="json",
    )
    assert json_res.text == "Hello standard."
    assert [lang.code for lang in (json_res.languages or [])] == ["en"]
    assert json_res.usage is not None
    assert json_res.usage.type == "tokens"
    assert json_res.usage.total_tokens == 13

    text_res = client.audio.transcriptions.create(
        model="standard-transcription",
        file=("clip.wav", wav_bytes, "audio/wav"),
        language="en",
        response_format="text",
    )
    assert text_res == "Plain standard."

print(json.dumps({"requests": 2}))
