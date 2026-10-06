"""Standard SDK audio calls against synthetic native Token Plan operations only."""
import io
import wave

import openai
from sdk_support import check


def check_native_audio(client: openai.OpenAI) -> int:
    """Preserve binary/text results and reject unadmitted controls before dispatch."""
    speech = dict(model="qwen-audio-3.0-tts-plus", input="synthetic plan speech",
                  voice="longanlingxin")
    eager = client.audio.speech.create(**speech)
    try:
        check(eager.read() == b"native-audio")
    finally:
        eager.close()
    with client.audio.speech.with_streaming_response.create(**speech) as response:
        check(response.headers["content-type"] == "application/octet-stream")
        check(b"".join(response.iter_bytes(chunk_size=2)) == b"native-audio")
    for control in ({"speed": 1}, {"instructions": ""}):
        try:
            client.audio.speech.create(**{**speech, **control})
        except openai.APIStatusError as error:
            check(error.status_code == 400)
        else:
            check(False, "unsupported native controls cannot dispatch")
    try:
        client.audio.speech.create(**speech)
    except openai.APIStatusError as error:
        check(error.status_code == 502)
    else:
        check(False, "native speech failure is not a complete artifact")

    buffer = io.BytesIO()
    with wave.open(buffer, "wb") as audio:
        audio.setnchannels(1)
        audio.setsampwidth(2)
        audio.setframerate(24000)
        audio.writeframes(b"\x00\x00\x01\x00")
    params = dict(model="qwen-audio-3.0-asr-flash", language="en",
                  file=("clip.wav", buffer.getvalue(), "audio/wav"))
    result = client.audio.transcriptions.create(**params)
    check(result.to_dict() == {"text": "Hi.", "usage": {"type": "duration", "seconds": 1}})
    raw = client.audio.transcriptions.with_raw_response.create(
        **params, response_format="json", stream=False)
    check(raw.parse().to_dict() == result.to_dict())
    # Keep zero/boolean multipart serialization and streaming error consumption here;
    # the complete control and target-value matrices belong to Rust codec/admission tests.
    for control in ({"temperature": 0}, {"stream": True}):
        try:
            client.audio.transcriptions.create(**{**params, **control})
        except openai.APIStatusError as error:
            check(error.status_code == 400)
        else:
            check(False, "unsupported transcription controls cannot dispatch")
    try:
        client.audio.transcriptions.create(**params)
    except openai.APIStatusError as error:
        check(error.status_code == 502)
    else:
        check(False, "partial sentence must not become a completed transcription")
    return 10
