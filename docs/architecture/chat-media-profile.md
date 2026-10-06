# Chat citations and generated audio

This bounded library profile extends the shared Chat codec, not a Provider dialect or a second IR. Product admission remains independently controlled by semantic contracts and compiled topology. No audio model, voice account service or real request is activated by these mappings. This fixed Chat value/closure contract is not the universal audio model; general content/artifact/reference design belongs to [Semantic Model](semantic-ir.md#5-内容产物与引用). The [Chat loss policy](protocol-and-lowering.md#semantic-loss) does not silently relax these existing checks.

## Sources and ownership

- Complete messages, URL citations, audio response and audio history use the [fixed Python SDK/OpenAI sources](../references/upstream-sync.md).
- The audio chunk shape is cross-checked against the official [Node stream helper at 5ee5d86](https://github.com/openai/openai-node/blob/5ee5d86/src/lib/ChatCompletionStream.ts), not inferred from the complete message. Its missing-finish inference is **not** adopted.
- Semantic owners are [text](../../src/semantic/task/generation/text.rs), [audio values](../../src/semantic/task/generation/audio.rs), [audio accumulation](../../src/semantic/task/generation/audio_stream.rs) and [generation settings](../../src/semantic/task/generation/request.rs). Source bindings live in [fidelity](../../src/protocol/fidelity.rs); target decisions remain in [lowering](../../src/lowering/generation.rs).

## URL citations

Chat's nested `url_citation` maps to the existing flat `Annotation::UrlCitation` on one text part. The fixed profile retains its existing Unicode-scalar range validation; it does not infer byte/UTF-16 or source-document coordinates. Unknown annotation kinds, extra fields, malformed ranges and active annotations without a text owner fail. Absent/null/empty annotation carriers mean no citations under this profile.

Responses and Chat static output can project this shared URL citation. Replacing text invalidates its dependent citations and probabilities; source JSON cannot restore them. Other annotation kinds stay unrepresentable in Chat.

The fixed Chat request-param and chunk schemas do not declare annotations. SDK-dumped annotated messages may be decoded for inspection, but request lowering and SSE projection reject active annotations rather than erase them or invent a delta field. This is not a web-search tool implementation.

## Audio values and output intent

Generated audio owns bounded Base64 data, transcript and a remote audio reference. Transcript is not an ordinary message text substitute. The reference owns its opaque ID and optionally reported expiry; a complete Chat audio output requires all four wire fields. The response reports no format: request format/voice are output intent, never manufactured response metadata. No sniffing, transcoding, upload, download or playback occurs.

The selected controls admit text/audio modality selection and a named voice string with one declared output format. Audio selection requires audio options, and options require audio selection. Absent/null controls remain distinct inactive forms. Unsupported modalities, duplicate selections and custom-voice object references fail. Stream requests require PCM16; JSON uses the declared format vocabulary. Audio output and audio history are separate semantic capabilities; neither follows from audio input or image support.

One Chat assistant message can project one optional ordinary text part followed by one audio artifact/reference. Refusal with audio, multiple artifacts or a different ordered grouping fail. Reference-only values are history, never complete response output.

## Stream and resource closure

Audio deltas produce typed byte/transcript/identity/expiry updates on an audio part. Continuous Base64 may split at arbitrary character boundaries; the codec retains only an incomplete quartet, validates padding, and forwards decoded bytes. The encoder retains at most two pending bytes before emitting canonical continuous Base64. Bytes, transcript, aggregate semantic state, JSON and SSE budgets are independently bounded.

A terminal expiry-only audio update closes further audio updates; it is not a response finish or transport terminal. ID/data/transcript/expiry must be complete before value closure. The current profile still requires explicit `finish_reason`, then optional usage and real `[DONE]` plus clean EOF. It rejects preview streams that omit finish_reason even if an SDK helper would infer stop. It never repairs truncated audio, invents missing transcript/expiry, or returns transcript-only success.

Text must open before audio when both are present. An audio-first stream cannot add a later text owner and silently reorder it into the static profile. Unfinished audio buffers cannot materialize as complete artifacts. Cancellation and errors do not fabricate terminal success.

## Reference history and expiry

Chat history uses `audio: {id}`, not an upload of generated bytes. Full SDK-dumped audio can be inspected and projected to that reference only after source/dependency checks. The sidecar binds stable part identity, reference and (when present) artifact bytes/transcript; changing a body while retaining an old ID fails reuse. Deletion cannot recover the audio from fidelity.

A trusted caller must supply compatible source/target scope. Scope is an intake association, **not issuer authentication**. Business JSON cannot declare it; a standalone unbound ID fails target projection. ID-only history has unknown expiry and cannot prove availability. When expiry is reported, the caller supplies `resource_time` for pure validation; absent clock or expired reference fails. Execution integrations must refresh that clock before use, not treat a stored time as a timeless authorization.

This library API does not add a stateless issuer-proof carrier, registry, retention service or automatic resource refresh to the public Gateway. Existing text/image product contracts remain unchanged. New audio activation must explicitly select its trusted reference flow; synthetic scoped loopback does not prove general Gateway/client replay.

## Usage and unsupported targets

Reported prompt/completion `audio_tokens` retain independent typed counters. Unknown is not zero; counts must fit their respective reported totals. Named zero-modality rules do not erase reported audio facts. Ordinary Responses usage has no such positions; projection rejects them rather than moving them into a private attachment.

Complete audio values, audio history and audio output controls have no admitted Responses carrier. They fail rather than becoming text, ordinary resources or arbitrary metadata. The fixed SDK consumes complete audio/citations as typed values; raw stream chunks retain audio as an extra field. No claim is made about every SDK stream-accumulation helper, real audio quality or upstream availability.

Independent behavior owners: [semantic media tests](../../tests/semantic/chat_media.rs), [SDK boundary](../../tests/sdk/chat_media.rs) and [SDK consumer](../../tests/sdk/chat_media_loop.py).
