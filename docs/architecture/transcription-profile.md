# Standard transcription JSON profile

This profile owns the independent `POST /v1/audio/transcriptions` file-upload branch, not Generation input audio, translation, Realtime or a file service. The [task](../../src/semantic/task/speech_recognition.rs) owns input audio, language intent, recognized text and actual reports; the [multipart codec](../../src/protocol/openai/transcription/multipart.rs) owns wire admission. HTTP activation belongs to the [Gateway contract](../http-gateway.md#独立语音识别).

## Sources and input

- Standard [Create transcription](https://developers.openai.com/api/reference/resources/audio/subresources/transcriptions/methods/create), with the [fixed SDK sources](../references/upstream-sync.md).
- Native [Qwen ASR Flash HTTP operation](https://help.aliyun.com/en/model-studio/fun-asr-flash-recorded-speech-recognition-http-api) and [Token Plan admission](https://help.aliyun.com/en/model-studio/token-plan-personal-overview). A listed model does not establish account permission.

The admitted request contains one bounded WAV/MP3 file, public model label and optional language. JSON is the only response format; absent streaming and explicit `false` select the same synchronous operation. Unsupported controls, including prompt, temperature, timestamp requests, non-JSON output and streaming, fail before I/O. Empty transmitted fields do not bypass rejection.

Multipart fields are unique, including the file. Canonical boundaries, bounded headers and field sizes are checked; extra envelopes, postambles, content encodings and ambiguous formats are rejected. A filename is upload metadata, never a path to open. Known filename extensions and MIME declarations must agree. Neither declaration proves the bytes are decodable audio.

File bytes are retained unchanged in the task. The trusted [native profile](../../src/protocol/aliyun_asr.rs) encodes them as a Base64 Data URI and maps a supported language to one language hint; absent language stays absent. It never selects a URL, workspace, credential, hotword resource or provider from business data. The upload byte ceiling is deliberately tighter than the service's advertised maximum; encoded bytes also satisfy endpoint limits. Duration/format decoding remains upstream responsibility, not a local audio decoder or remote-fee guarantee.

## Reports, completion and projection

The full recognized text is authoritative. The native current-sentence object reports only the last sentence, not an invented complete segmentation of the full text. Reported sentence/word order, punctuation, channel and millisecond timestamps remain typed. A reported unfinished sentence or unstable word cannot become a complete result; inconsistent text/timing is rejected rather than repaired.

Reported processed seconds retain their usage meaning. They can map to standard duration usage, but do not become an independently measured precise file duration or a language report. No usage is fabricated when absent. A static successful HTTP response still requires bounded strict JSON and transport EOF; late read errors, excess bytes and cancellation fail the entire response.

The standard JSON projection emits full text and representable duration usage. Omission of ancillary reports is limited to the [named audio projection](protocol-and-lowering.md#独立音频的附属报告投影); typed omission flags distinguish loss from absent reports. The original result remains available to library consumers. Editing text invalidates its associated request, timing and usage reports; no retained wire record restores them.

## Execution and deferred boundaries

The [compiled route](../../src/topology/transcription.rs) fixes one trusted target and API-key source without retry/fallback. Request and response buffers, concurrency, body/exchange deadlines and cancellation remain bounded. No URL fetch, local file access, temporary upload service, model polling or automatic continuation is introduced.

Verbose JSON, subtitles, full word/segment timing delivery, diarization, URL inputs, translation, streaming recognition and realtime protocols require separate contracts. Standard JSON success and synthetic SDK consumption do not establish real recognition quality, remote availability, pricing or load behavior.
