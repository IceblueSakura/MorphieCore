"""Explicit voice->transcription->LLM and native voice->text/history live gates."""
import argparse
import json
import os
from pathlib import Path
import signal
import time

from probe_support.checks import require, install_signals
from probe_support.ledger import Run
from probe_support.runtime import session, summary
from probe_support.scenarios import call
from probe_support.voice import (QUESTION, FOLLOWUP, SPEECH, ASR, synthesize,
                                 inspect_wav, audio_history, media_matrix)


def exact(number):
    def oracle(text, calls, output):
        require(not calls and text.strip()==str(number), "exact_text")
    return oracle


def register_cases(media, text_run, native=None):
    asr = next(m for m in media.plan["models"] if media.protocols(m)==("transcription",))
    media.register([(SPEECH,"voice:tts:question",None)]+[
        (asr,"voice:asr:"+label,None) for label in ("local-wav","local-mp3","real-tts-mp3")])
    text_run.register([("deepseek-flash",f"voice:asr-text:{index}",512) for index in range(3)])
    if native is not None:
        native.register([("gpt-audio-mini",f"voice:native:{stream}:{turn}",512)
            for stream in (False,True) for turn in (1,2)]+[("gpt-audio-mini","voice:native:wav",512)])


def native_matrix(native, wav, next_wav):
    from probe_support.voice import transcode
    with session(native) as (client,transport):
        mp3, next_mp3 = transcode(wav,"wav","mp3"), transcode(next_wav,"wav","mp3")
        for stream in (False,True):
            history=audio_history(mp3, "mp3")
            output,_,_=call(client,transport,"gpt-audio-mini","chat",
                f"voice:native:{stream}:1",history,stream,cap=512,
                extra={"modalities":["text"]},oracle=exact(16))
            history.extend(output)
            history.extend(audio_history(next_mp3, "mp3"))
            call(client,transport,"gpt-audio-mini","chat",f"voice:native:{stream}:2",
                history,not stream,cap=512,extra={"modalities":["text"]},oracle=exact(11))
        call(client,transport,"gpt-audio-mini","chat","voice:native:wav",
             audio_history(wav),False,cap=512,
             extra={"modalities":["text"]},oracle=exact(16))


def main():
    install_signals()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("plan","run","report"))
    parser.add_argument("directory", type=Path)
    parser.add_argument("--live", action="store_true")
    parser.add_argument("--espeak", type=Path)
    parser.add_argument("--asr-model", choices=(ASR,"qwen-audio-3.0-asr-flash-dashscope"), default=ASR)
    parser.add_argument("--asr-only", action="store_true",
                        help="Plan/run only the explicit ASR->text chain; never rerun native acceptance.")
    args = parser.parse_args()
    root = args.directory
    if args.action=="plan":
        root.mkdir(parents=True, mode=0o700, exist_ok=False)
        provider = "aliyun-tokenplan-cn" if args.asr_model==ASR else "aliyun-dashscope-cn"
        Run.create(root/"media", task="media", providers="openrouter,"+provider,
                   models=[SPEECH,args.asr_model], tokens=None, limit=4)
        Run.create(root/"text", providers="deepseek", models=["deepseek-flash"], limit=3, tokens=512)
        if not args.asr_only:
            Run.create(root/"native", providers="openrouter", models=["gpt-audio-mini"], limit=5, tokens=512)
        register_cases(Run(root/"media"),Run(root/"text"),None if args.asr_only else Run(root/"native"))
        print(json.dumps({"run":str(root.resolve()),"max_requests":7 if args.asr_only else 12,
              "media_requests":4,"text_requests":3,"native_requests":0 if args.asr_only else 5,
              "voice_bytes_limit":1<<20,"wire_bytes_limit":2<<20,"exchange_seconds":120,
              "global_seconds":1800,"retry":0}))
        return 0
    if args.action=="report":
        for name in ("media","text","native"):
            if not (root/name).is_dir():
                continue
            print(json.dumps({"phase":name,"attempts":summary(Run(root/name))}))
        return 0
    require(args.live and args.espeak and args.espeak.is_file(), "voice_live_selection", "setup")
    require(os.environ.get("MORPHIECORE_PROBE_CREDENTIALS_DIR"), "credential_directory", "setup")
    require(hasattr(signal,"SIGALRM"), "deadline", "setup")
    signal.signal(signal.SIGALRM, lambda *_: (_ for _ in ()).throw(SystemExit(124)))
    names = ("media","text") if args.asr_only else ("media","text","native")
    created = min(Run(root/name).plan["created"] for name in names)
    remaining = int(created + 1800 - time.time())
    require(remaining > 0, "voice_deadline", "budget")
    signal.alarm(remaining)
    os.environ["MORPHIECORE_PROBE_LIVE"]="1"
    wav, next_wav = synthesize(args.espeak,QUESTION), synthesize(args.espeak,FOLLOWUP)
    manifest = {"first":inspect_wav(wav),"followup":inspect_wav(next_wav)}
    (root/"fixture-metadata.json").write_text(json.dumps(manifest))
    started = time.monotonic()
    media, text_run = (Run(root/name) for name in ("media","text"))
    native = None if args.asr_only else Run(root/"native")
    phases = {}
    try:
        transcripts = media_matrix(media, wav)
        phases["media"] = "passed"
    except Exception:
        transcripts = []
        phases["media"] = "failed"
    if transcripts:
        try:
            with session(text_run) as (client,transport):
                for index, transcript in enumerate(transcripts):
                    history=[{"role":"user","content":[{"type":"input_text","text":
                        "Answer the following actually transcribed question. Return only decimal digits:\n"+transcript}]}]
                    call(client,transport,"deepseek-flash","responses",f"voice:asr-text:{index}",
                         history, bool(index%2), cap=512, oracle=exact(16))
            phases["text"] = "passed"
        except Exception:
            phases["text"] = "failed"
    else:
        phases["text"] = "not_run"
    if args.asr_only:
        for run in (media,text_run):
            summary(run)
        (root/"execution.json").write_text(json.dumps(phases))
        print(json.dumps({"phases":phases,"elapsed_seconds":round(time.monotonic()-started)}))
        return 0 if all(p=="passed" for p in phases.values()) else 1
    try:
        native_matrix(native,wav,next_wav)
        phases["native"] = "passed"
    except Exception:
        phases["native"] = "failed"
    for run in (media,text_run,native):
        summary(run)
    (root/"execution.json").write_text(json.dumps(phases))
    print(json.dumps({"closed":all(p=="passed" for p in phases.values()),
                     "phases":phases,"elapsed_seconds":round(time.monotonic()-started)}))
    return 0 if all(p=="passed" for p in phases.values()) else 1


if __name__ == "__main__":
    raise SystemExit(main())
