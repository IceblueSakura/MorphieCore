"""Finite synthetic voice fixtures and standard Speech/Transcription consumers."""
import base64
import copy
import email.policy
from email.parser import BytesParser
import io
import json
import os
import selectors
import subprocess
import time
import wave

from openai import DefaultHttpxClient, OpenAI
from .checks import ProbeFailure, require
from .runtime import gateway, ObservedStream

QUESTION = "What is seven plus nine? Reply with just the number."
FOLLOWUP = "Take your previous answer and subtract five. Reply with just the number."
SPEECH = "qwen-audio-3.0-tts-flash"
ASR = "qwen-audio-3.0-asr-flash"

def audio_tool(command, data=None, *, timeout=20, limit=1<<20):
    """Bound both pipes during execution and reap on timeout, overflow or cancellation."""
    child = subprocess.Popen(command, stdin=subprocess.PIPE if data is not None else subprocess.DEVNULL,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    output, errors, position = bytearray(), 0, 0
    deadline = time.monotonic()+timeout
    try:
        with selectors.DefaultSelector() as selector:
            for pipe in (child.stdout, child.stderr):
                os.set_blocking(pipe.fileno(),False)
                selector.register(pipe,selectors.EVENT_READ)
            if data is not None:
                os.set_blocking(child.stdin.fileno(),False)
                selector.register(child.stdin,selectors.EVENT_WRITE)
            while selector.get_map():
                remaining = deadline-time.monotonic()
                require(remaining>0,"audio_tool_timeout","setup")
                for key,_ in selector.select(remaining):
                    pipe=key.fileobj
                    if pipe is child.stdin:
                        try:
                            position+=os.write(pipe.fileno(),data[position:position+65536])
                        except BrokenPipeError:
                            position=len(data)
                        if position==len(data):
                            selector.unregister(pipe);pipe.close()
                        continue
                    raw=os.read(pipe.fileno(),8192)
                    if not raw:
                        selector.unregister(pipe);pipe.close()
                    elif pipe is child.stdout:
                        require(len(output)+len(raw)<=limit,"audio_tool_output","setup")
                        output.extend(raw)
                    else:
                        errors+=len(raw)
                        require(errors<=8192,"audio_tool_diagnostics","setup")
            require(child.wait(timeout=max(0.001,deadline-time.monotonic()))==0
                    and output,"audio_tool_failed","setup")
        return bytes(output)
    finally:
        if child.poll() is None:
            child.kill()
        child.wait()
        for pipe in (child.stdin,child.stdout,child.stderr):
            if pipe is not None:
                pipe.close()


def inspect_wav(data):
    require(isinstance(data, bytes) and 0 < len(data) <= 1 << 20, "audio_bytes")
    try:
        with wave.open(io.BytesIO(data), "rb") as audio:
            channels, width, rate, frames = (audio.getnchannels(), audio.getsampwidth(),
                                            audio.getframerate(), audio.getnframes())
            require(channels == 1 and width == 2 and rate == 16000
                    and 0 < frames <= rate * 15 and audio.getcomptype() == "NONE", "audio_format")
            pcm = audio.readframes(frames + 1)
            require(len(pcm) == frames * 2 and any(pcm), "audio_samples")
        return {"audio_decoded": True, "audio_bytes": len(data),
                "audio_samples": frames, "audio_rate": rate}
    except (wave.Error, EOFError):
        raise ProbeFailure("audio_format") from None


def transcode(data, source, target):
    require(source in ("wav", "mp3") and target in ("wav", "mp3")
            and 0 < len(data) <= 1 << 20, "audio_format", "setup")
    encoded = audio_tool(
        ["ffmpeg", "-v", "error", "-f", source, "-i", "pipe:0", "-t", "15",
         "-ac", "1", "-ar", "16000", "-f", target,
         *(["-codec:a","pcm_s16le"] if target=="wav" else ["-codec:a","libmp3lame","-b:a","48k"]),
         "pipe:1"], data)
    # Pipe WAV headers have unknown lengths; canonicalize the actual bounded PCM.
    if target == "wav":
        with wave.open(io.BytesIO(encoded), "rb") as raw:
            pcm = raw.readframes(16000 * 15 + 1)
        output = io.BytesIO()
        with wave.open(output, "wb") as audio:
            audio.setnchannels(1); audio.setsampwidth(2); audio.setframerate(16000)
            audio.writeframes(pcm)
        return output.getvalue()
    return encoded


def synthesize(executable, text):
    require(text in (QUESTION, FOLLOWUP), "voice_phrase", "setup")
    encoded = audio_tool([str(executable), "--stdout", "-v", "en-us", "-s", "175", text],timeout=15)
    wav = transcode(encoded, "wav", "wav")
    inspect_wav(wav)
    return wav


def audio_history(data, format="wav"):
    require(format in ("wav", "mp3") and 0 < len(data) <= 1 << 20, "audio_format")
    return [{"role":"user","content":[
        {"type":"text","text":"Answer the question you hear. Return only decimal digits."},
        {"type":"input_audio","input_audio":{"format":format,
                                            "data":base64.b64encode(data).decode()}}]}]


def recognized_question(text):
    # Check semantic anchors, not punctuation/case or whether ASR writes digits.
    words = text.lower().replace("?", "").replace(".", "").split()
    require(("seven" in words or "7" in words) and ("nine" in words or "9" in words)
            and "plus" in words, "transcription_content")


class MediaWire:
    def __init__(self):
        self.bytes, self.closed, self.done, self.terminal = 0, False, False, None
    def push(self, data):
        self.bytes += len(data)
        require(self.bytes <= 2 << 20, "wire_bytes", "wire")
    def finish(self):
        self.closed = True


def validate_media(request, origin, model, protocol, expected):
    require(request.method == "POST" and str(request.url) == origin + {
        "speech":"/v1/audio/speech","transcription":"/v1/audio/transcriptions"}[protocol],
        "destination", "budget")
    raw = request.read()
    require(len(raw) <= 2 << 20, "request_bytes", "budget")
    if protocol == "speech":
        require(json.loads(raw) == expected, "speech_changed", "budget")
        return
    media = request.headers.get("content-type", "")
    require(media.startswith("multipart/form-data;"), "multipart", "budget")
    message = BytesParser(policy=email.policy.default).parsebytes(
        ("Content-Type: "+media+"\r\nMIME-Version: 1.0\r\n\r\n").encode()+raw)
    require(message.is_multipart(), "multipart", "budget")
    fields = {}
    for part in message.iter_parts():
        name = part.get_param("name", header="content-disposition")
        require(name in ("model","file","language","response_format") and name not in fields,
                "multipart_fields", "budget")
        fields[name] = part.get_payload(decode=True)
    require(fields == expected and fields["model"] == model.encode(),
            "audio_changed", "budget")


class MediaClient(DefaultHttpxClient):
    def __init__(self, origin, run):
        super().__init__(trust_env=False, follow_redirects=False)
        self.origin, self.run, self.context = origin, run, None
        self.attempt, self.wire, self.last_status = None, None, None
    def prepare(self, model, protocol, scenario, expected):
        require(self.run.is_media and model in self.run.plan["models"]
                and self.run.protocols(model) == (protocol,) and self.attempt is None,
                "media_selection", "budget")
        self.context = (model, protocol, scenario, copy.deepcopy(expected))
        self.wire, self.last_status = None, None
    def send(self, request, **kwargs):
        require(self.context is not None and self.attempt is None, "unplanned_send", "budget")
        model, protocol, scenario, expected = self.context
        validate_media(request, self.origin, model, protocol, expected)
        self.attempt = self.run.reserve(model, scenario, None)
        self.run.dispatched(self.attempt)
        request.headers["x-morphiecore-probe-id"] = self.attempt
        kwargs.pop("stream", None)
        response = super().send(request, stream=True, **kwargs)
        self.last_status = response.status_code
        self.wire = MediaWire()
        response.stream = ObservedStream(response.stream, self.wire)
        return response
    def complete(self, state, metrics):
        if self.attempt is None:
            return
        metrics.update(http=self.last_status, sdk_consumed=state!="failed", history_ok=True,
                       wire_closed=bool(self.wire and self.wire.closed),
                       wire_bytes=self.wire.bytes if self.wire else 0)
        self.run.finish(self.attempt, state, metrics)
        print(json.dumps({"attempt":self.attempt,"state":state}), flush=True)
        self.attempt, self.context = None, None


def media_matrix(run, wav):
    """One real TTS and three ASR inputs; output bytes remain only in memory."""
    models = [m for m in run.plan["models"] if run.protocols(m)==("transcription",)]
    require(len(models)==1, "asr_selection", "setup")
    asr = models[0]
    with gateway(run) as (origin, key):
        transport = MediaClient(origin, run)
        with OpenAI(base_url=origin+"/v1", api_key=key, max_retries=0,
                    timeout=130, http_client=transport) as client:
            try:
                request = {"model":SPEECH,"input":QUESTION,"voice":"loongjohn","response_format":"mp3"}
                transport.prepare(SPEECH, "speech", "voice:tts:question", request)
                response = client.audio.speech.create(**request)
                try:
                    mp3 = response.read()
                finally:
                    response.close()
                decoded = transcode(mp3, "mp3", "wav")
                metrics = inspect_wav(decoded)
                require(transport.wire.closed, "missing_wire_eof", "wire")
                transport.complete("passed", metrics)
                inputs = [("local-wav",wav,"wav"), ("local-mp3",transcode(wav,"wav","mp3"),"mp3"),
                          ("real-tts-mp3",mp3,"mp3")]
                transcripts = []
                for label, data, format in inputs:
                    expected = {"model":asr.encode(),"file":data,"language":b"en","response_format":b"json"}
                    transport.prepare(asr,"transcription","voice:asr:"+label,expected)
                    result = client.audio.transcriptions.create(model=asr,
                        file=("synthetic."+format,data,"audio/wav" if format=="wav" else "audio/mpeg"),
                        language="en",response_format="json")
                    require(transport.wire.closed, "missing_wire_eof", "wire")
                    recognized_question(result.text)
                    transport.complete("passed", {"content_ok":True,"transcription_ok":True,"text_chars":len(result.text)})
                    transcripts.append(result.text)
                return transcripts
            except Exception as error:
                transport.complete("oracle_failed" if isinstance(error,ProbeFailure)
                    and error.kind=="oracle" else "failed", {"content_ok":False})
                raise ProbeFailure("voice_media_failed", "oracle") from None
