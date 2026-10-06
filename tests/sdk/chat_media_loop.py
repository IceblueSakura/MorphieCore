"""Synthetic SDK Chat citations/audio; raw chunk consumption, not a guessed typed delta."""
import base64
import sys
from sdk_support import check, client_for


def run(base_url):
    with client_for(base_url) as client:
        result = client.chat.completions.create(
            model="fixture-model", messages=[{"role": "user", "content": "cite"}]
        )
        citation = result.choices[0].message.annotations[0]
        check(citation.type == "url_citation")
        check(citation.url_citation.url == "https://example.invalid/")
        for stream in (False, True):
            history = [{"role": "user", "content": "hello"}]
            result = client.chat.completions.create(
                model="fixture-model", messages=history, stream=stream,
                modalities=["text", "audio"],
                audio={"format": "pcm16" if stream else "wav", "voice": "alloy"},
            )
            if stream:
                audio, finish = {"data": "", "transcript": ""}, None
                with result:
                    for chunk in result:
                        for choice in chunk.choices:
                            update = choice.delta.model_dump().get("audio")
                            if update:
                                for key in ("id", "expires_at"):
                                    if key in update:
                                        audio[key] = update[key]
                                for key in ("data", "transcript"):
                                    audio[key] += update.get(key, "")
                            if choice.finish_reason:
                                finish = choice.finish_reason
                check(finish == "stop")
            else:
                audio = result.choices[0].message.audio.model_dump()
            check(audio["id"] == "synthetic-audio")
            check(audio["expires_at"] == 2000)
            check(audio["transcript"] == "hello")
            check(base64.b64decode(audio["data"], validate=True) == bytes([1, 2, 3, 4]))
            history.extend([
                {"role": "assistant", "audio": {"id": audio["id"]}},
                {"role": "user", "content": "repeat"},
            ])
            follow = client.chat.completions.create(model="fixture-model", messages=history)
            check(follow.choices[0].message.content == "continued")
    print('{"requests":5}')


if __name__ == "__main__":
    run(sys.argv[1])
