"""Independent binary Speech consumption; synthetic loopback content only."""
import io
import wave

import openai
from sdk_support import check


def check_speech(client: openai.OpenAI) -> int:
    """Exercise eager/lazy bytes, real WAV decoding and sanitized failure consumption."""
    params = dict(model="public-speech", input="synthetic speech", voice="alloy",
                  instructions="", speed=1.25, response_format="wav", stream_format="audio")
    eager = client.audio.speech.create(**params)
    try:
        content = eager.read()
    finally:
        eager.close()
    with client.audio.speech.with_streaming_response.create(**params) as response:
        check(response.headers["content-type"] == "audio/wav")
        streamed = b"".join(response.iter_bytes(chunk_size=7))
    check(content == streamed)
    with wave.open(io.BytesIO(content), "rb") as audio:
        check((audio.getnchannels(), audio.getsampwidth(), audio.getframerate(),
               audio.getnframes()) == (1, 2, 24000, 2))
        check(audio.readframes(2) == b"\x00\x00\x01\x00")
    try:
        client.audio.speech.create(**params)
    except openai.APIStatusError as error:
        check(error.status_code == 502)
        check(error.response.json()["error"]["code"] == "upstream_error")
        check("synthetic-private" not in error.response.text)
    else:
        check(False, "an upstream JSON error must not become successful audio")
    for controls in ({"stream_format": "sse"}, {"voice": {"id": "voice_fixture"}}):
        try:
            client.audio.speech.create(**{**params, **controls})
        except openai.APIStatusError as error:
            check(error.status_code == 400)
        else:
            check(False, "unadmitted Speech branches must fail before dispatch")
    router = dict(model="qwen-audio-3.0-tts-flash", input="synthetic router speech", voice="loongjohn")
    eager = client.audio.speech.create(**router)
    try:
        check(eager.read() == b"synthetic-router-audio")
    finally:
        eager.close()
    with client.audio.speech.with_streaming_response.create(
        **router, response_format="mp3", stream_format="audio"
    ) as response:
        check(response.headers["content-type"] == "audio/mpeg")
        check(b"".join(response.iter_bytes(chunk_size=3)) == b"synthetic-router-audio")
    try:
        client.audio.speech.create(**router)
    except openai.APIStatusError as error:
        check(error.status_code == 502)
    else:
        check(False, "PCM cannot satisfy an MP3 request")
    for controls in (
        {"speed": 1}, {"instructions": ""}, {"extra_body": {"provider": {}}},
    ):
        try:
            client.audio.speech.create(**{**router, **controls})
        except openai.APIStatusError as error:
            check(error.status_code == 400)
        else:
            check(False, "OpenRouter target admission must precede I/O")
    return 11
