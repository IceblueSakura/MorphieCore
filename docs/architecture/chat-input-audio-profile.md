# Standard Chat voice input / text output

This request-based profile admits standard user `input_audio` parts, not Realtime,
audio synthesis or an implicit transcription pipeline. Sources are the
[standard Chat operation](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create)
and [OpenRouter Chat](https://openrouter.ai/docs/api/api-reference/chat/create-a-chat-completion).
Fixed SDK versions belong to [upstream sources](../references/upstream-sync.md).

## Owner and carrier

Audio remains a Generation input `ResourceKind::Audio` with an inline source and
input purpose. It is not an assistant AudioArtifact, transcript, filename or URL.
The [codec](../../src/protocol/openai/input_audio.rs) maps WAV/MP3 to exact MIME
labels, preserves encoded bytes and order, and rejects malformed Base64, unknown
formats/keys and non-user placement. Neither labels nor Base64 prove decodable
speech, sample layout, duration or model understanding.

[Lowering](../../src/lowering/generation.rs) checks final resources and target
admission. Edits/deletion cannot restore original audio through fidelity. Standard
Responses has no input-audio carrier here and still rejects the request; no private
extension or transcript substitution is introduced. Text responses may separately
project through Responses under its existing output/usage contracts.

## Product and execution

An audio declaration does not activate every model or confer audio output/history
permission. [Canonical contracts](../../src/topology/catalog/models.rs) and
[bindings](../../src/topology/catalog/bindings.rs) fix the selected input-only slice.
[Bootstrap](../../src/gateway/bootstrap.rs) requires explicit selection even when
the corresponding text credential pool exists. No persistent activation changes
follow from a library registration or a probe.

Clients can retain actual text output and original audio input for a request-based
follow-up with new audio. This is ClientManaged history, not assistant audio-ID
replay, automatic continuation, a voice resource service or a remote session.
Input/response bytes, deadlines, cancellation and publication remain bounded.

Independent [Transcription](transcription-profile.md) can feed actual recognized
text into Generation through an explicit client step. It never replaces native
audio input or proves that a subsequent model listened to the original recording.
Tests and real-model observations report those two chains separately.
