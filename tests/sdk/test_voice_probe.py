"""Independent voice fixture, consumer and finite-plan guards; no real credentials."""
import base64
import io
import json
from pathlib import Path
import struct
import sys
import tempfile
import unittest
from unittest.mock import patch
from contextlib import nullcontext
import wave
import httpx2

sys.path.insert(0, str(Path(__file__).resolve().parents[2]/"examples"))
from probe_support.checks import ProbeFailure
from probe_support.ledger import Run
from probe_support.catalog import select_bindings
from probe_support.voice import inspect_wav, audio_history, recognized_question, validate_media, SPEECH, ASR, MediaClient, MediaWire, audio_tool
import voice_probe


def wav():
    output=io.BytesIO()
    with wave.open(output,"wb") as audio:
        audio.setnchannels(1);audio.setsampwidth(2);audio.setframerate(16000)
        audio.writeframes(struct.pack("<hhhh",0,100,-100,0)*4000)
    return output.getvalue()


class VoiceTests(unittest.TestCase):
    def test_audio_tool_bounds_output_and_stderr_and_reaps(self):
        self.assertEqual(audio_tool([sys.executable,"-c","print('synthetic')"]),b"synthetic\n")
        for destination, code in [("stdout","audio_tool_output"),("stderr","audio_tool_diagnostics")]:
            with self.assertRaises(ProbeFailure) as failure:
                audio_tool([sys.executable,"-c",
                    f"import os\nwhile True: os.write({1 if destination=='stdout' else 2},b'x'*8192)"],limit=1024)
            self.assertEqual(failure.exception.code,code)

    def test_native_failure_still_reports_all_registered_not_run_cases(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)/"run"
            with patch.object(sys,"argv",["voice_probe","plan",str(root)]):
                self.assertEqual(voice_probe.main(),0)
            with patch.object(sys,"argv",["voice_probe","run",str(root),"--live","--espeak",sys.executable]), \
                 patch.object(voice_probe,"install_signals") as signals, \
                 patch.object(voice_probe.signal,"signal"),patch.object(voice_probe.signal,"alarm"), \
                 patch.dict("os.environ",{"MORPHIECORE_PROBE_CREDENTIALS_DIR":"synthetic-only"}), \
                 patch.object(voice_probe,"synthesize",return_value=wav()), \
                 patch.object(voice_probe,"media_matrix",return_value=["What is seven plus nine?"]*3), \
                 patch.object(voice_probe,"session",return_value=nullcontext((None,None))), \
                 patch.object(voice_probe,"call"), \
                 patch.object(voice_probe,"native_matrix",side_effect=ProbeFailure("synthetic")):
                self.assertEqual(voice_probe.main(),1)
                signals.assert_called_once()
            self.assertEqual(json.loads((root/"execution.json").read_text())["native"],"failed")
            self.assertEqual([r["state"] for r in Run(root/"native").snapshot()],["not_run"]*5)
            self.assertEqual([r["state"] for r in Run(root/"media").snapshot()],["not_run"]*4)

    def test_media_prepare_cannot_inherit_previous_http_or_eof(self):
        with tempfile.TemporaryDirectory() as temp:
            run=Run.create(Path(temp)/"media",task="media",
                providers="openrouter,aliyun-tokenplan-cn",models=[SPEECH,ASR],limit=4,tokens=None)
            with MediaClient("http://127.0.0.1:1",run) as client:
                client.last_status=200
                client.wire=MediaWire()
                client.wire.push(b"old");client.wire.finish()
                client.prepare(ASR,"transcription","synthetic-next",{})
                self.assertIsNone(client.last_status)
                self.assertIsNone(client.wire)

    def test_wav_structure_and_samples_are_independent_of_provider_claims(self):
        result=inspect_wav(wav())
        self.assertEqual(result["audio_rate"],16000)
        self.assertEqual(result["audio_samples"],16000)
        for value in (b"",b"not audio"):
            with self.assertRaises(ProbeFailure):inspect_wav(value)
        with self.assertRaises(ProbeFailure):inspect_wav(b"x"*((1<<20)+1))

    def test_audio_history_does_not_include_spoken_question_or_expected_answer(self):
        body=audio_history(wav())
        part=body[0]["content"][1]
        self.assertEqual(base64.b64decode(part["input_audio"]["data"]),wav())
        self.assertEqual(part["input_audio"]["format"],"wav")
        self.assertNotIn("seven",body[0]["content"][0]["text"])
        self.assertNotIn("16",body[0]["content"][0]["text"])

    def test_asr_oracle_preserves_semantic_anchors_but_does_not_accept_text_only_claims(self):
        for text in ("What is seven plus nine?","What is 7 plus 9?"):
            recognized_question(text)
        for text in ("sixteen","seven minus nine","", "This audio says nothing."):
            with self.assertRaises(ProbeFailure):recognized_question(text)

    def test_media_plan_is_explicit_and_rejects_caps_and_changed_fields(self):
        with tempfile.TemporaryDirectory() as temp:
            run=Run.create(Path(temp)/"media",task="media",
                providers="openrouter,aliyun-tokenplan-cn",models=[SPEECH,ASR],limit=4,tokens=None)
            self.assertTrue(run.is_media)
            self.assertEqual(run.protocols(ASR),("transcription",))
            self.assertTrue(run.valid_budget(ASR,None))
            self.assertFalse(run.valid_budget(ASR,512))
            with self.assertRaises(ProbeFailure):
                Run.create(Path(temp)/"bad",task="media",providers="openrouter",
                           models=[SPEECH],tokens=512)
            with self.assertRaises(ProbeFailure):
                Run.create(Path(temp)/"bridge",task="media",providers="openrouter",
                           models=[SPEECH],tokens=None,responses_via_chat=True)
        self.assertNotIn("gpt-audio-mini",[r[1] for r in select_bindings("openrouter")])

    def test_media_request_rejects_redirect_prompt_substitution_and_audio_replacement(self):
        origin="http://127.0.0.1:1"
        body={"model":SPEECH,"input":"synthetic","voice":"loongjohn","response_format":"mp3"}
        request=httpx2.Request("POST",origin+"/v1/audio/speech",json=body)
        validate_media(request,origin,SPEECH,"speech",body)
        with self.assertRaises(ProbeFailure):
            validate_media(request,origin,SPEECH,"speech",{**body,"input":"changed"})
        with self.assertRaises(ProbeFailure):
            validate_media(httpx2.Request("POST","https://example.invalid/v1/audio/speech",json=body),
                           origin,SPEECH,"speech",body)
        request=httpx2.Request("POST",origin+"/v1/audio/transcriptions",
            data={"model":ASR,"language":"en","response_format":"json"},
            files={"file":("synthetic.wav",wav(),"audio/wav")})
        expected={"model":ASR.encode(),"language":b"en","response_format":b"json","file":wav()}
        validate_media(request,origin,ASR,"transcription",expected)
        with self.assertRaises(ProbeFailure):
            validate_media(request,origin,ASR,"transcription",{**expected,"file":b"changed"})


if __name__=="__main__":unittest.main()
