"""Pinned OpenAI SDK loopback checks for Speech, Transcription, and Images SSE streams."""
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

    # 1. Speech SSE (openai-python 3.37.0 exposes stream_format="sse" via raw/streaming response)
    with client.audio.speech.with_streaming_response.create(
        model="public-speech",
        input="stream me",
        voice="alloy",
        response_format="wav",
        stream_format="sse",
    ) as sp_resp:
        assert sp_resp.headers["content-type"] == "text/event-stream"
        sp_text = b"".join(sp_resp.iter_bytes()).decode("utf-8")
        assert "speech.audio.delta" in sp_text
        assert "speech.audio.done" in sp_text

    # 2. Transcription SSE
    tr_events = list(
        client.audio.transcriptions.create(
            model="standard-transcription",
            file=("clip.wav", b"RIFF", "audio/wav"),
            stream=True,
        )
    )
    assert [(e.type, getattr(e, "delta", None), getattr(e, "text", None)) for e in tr_events] == [
        ("transcript.text.delta", "Hel", None),
        ("transcript.text.delta", "lo", None),
        ("transcript.text.done", None, "Hello"),
    ]
    assert tr_events[-1].usage is not None
    assert tr_events[-1].usage.total_tokens == 7

    # 3. Images Generate Preview SSE
    gen_events = list(
        client.images.generate(
            model="public-image",
            prompt="stream cat",
            stream=True,
            partial_images=1,
            output_format="png",
        )
    )
    assert [(e.type, getattr(e, "partial_image_index", None), e.b64_json) for e in gen_events] == [
        ("image_generation.partial_image", 0, "AQID"),
        ("image_generation.completed", None, "BAUG"),
    ]

    # 4. Images Edit Preview SSE
    edit_events = list(
        client.images.edit(
            model="public-image",
            image=("src.png", PNG_1X1, "image/png"),
            prompt="stream edit",
            stream=True,
            partial_images=1,
            output_format="webp",
        )
    )
    assert [(e.type, getattr(e, "partial_image_index", None), e.b64_json) for e in edit_events] == [
        ("image_edit.partial_image", 0, "AQID"),
        ("image_edit.completed", None, "BAUG"),
    ]

print(json.dumps({"requests": 4}))
