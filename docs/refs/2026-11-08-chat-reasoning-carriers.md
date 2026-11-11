# Chat reasoning carrier evidence, 2026-11-08

This note records source and consumer evidence. [Architecture](../architecture/history-and-replay.md#chat-reasoning-extensions) owns the selected public contract; [Development](../development.md#overall-acceptance-status) summarizes overall acceptance and [Roadmap](../roadmap/unscheduled-issues.md#unscheduled-issues) owns remaining qualification.

## Sources and authority

- [OpenRouter reasoning details](https://openrouter.ai/docs/guides/best-practices/reasoning-tokens#reasoning-details-api-shape), fetched 2026-11-08: `choices[].message.reasoning_details` for JSON, `choices[].delta.reasoning_details` for SSE; typed summary, text and encrypted entries, with identities, format labels and optional indices. Its preservation section requires returning the complete reasoning sequence. This is an extension, not official OpenAI Chat baseline.
- Installed `@earendil-works/pi-ai` **1.5.0**: `dist/api/openai-completions.js` validates recognized detail types, stores their array in `thinkingSignature`, and serializes it into assistant history. `dist/api/transform-messages.js` retains protected thinking for the same provider/API/model. These installed artifacts are independent of the older [regression source pins](../references.md#regression-release-selection).
- OpenCode **2.0.32**, source revision [`843a6c514e606b436a7fe713422ace0a93ad3d61`](https://github.com/anomalyco/opencode/tree/843a6c514e606b436a7fe713422ace0a93ad3d61): `packages/ai/src/protocols/openai-chat.ts` retains recognized details through `providerMetadata.reasoningDetails`; the generic compatible Chat protocol reuses it. `packages/core/src/session/runner/{publish-llm-event,to-llm-message}.ts` and `message-updater.ts` persist and restore that metadata for the same model/provider and a successful, uninterrupted assistant message.

## Discriminating client checks

Six synthetic direct-client loops used current unmodified pi 1.5.0 and OpenCode 2.0.32, isolated state and three requests each: model-issued read, actual tool feedback, final answer and next-turn recall. No Provider or gateway activation was involved.

| Wire shape | pi | OpenCode |
|---|---|---|
| Summary/text owners separated by independent `reasoning.encrypted` envelopes | Complete details sequence returned unchanged | Complete details sequence returned unchanged |
| Two opaque-only encrypted owners | Both returned unchanged | Both returned unchanged |
| Two adjacent text details with different signatures | Merged into one text detail; second signature lost | Both retained independently |

The pi counterexample is concrete: text `A` with signature `S1`, followed by text `B` with signature `S2`, becomes `AB` with `S1`. Its accumulator merges adjacent text details without comparing signature/identity. Independent encrypted entries avoid this merge. The gateway does not patch the client or attach separate native signatures directly to adjacent text entries.

## Representation boundaries

`src/execution/replay.rs` places the native format, issuer and original payload inside the existing tagged `mc1.` envelope. The outer Chat transport uses the existing Responses-format encrypted carrier; opening restores the original native format. The tag is not encryption, authentication or permission to transfer a value to a different issuer.

Interactions thought replay is an internal typed payload containing a `thought` anchor and an optional signature. Its serialized JSON wrapper is not the native signature string. A native-wire oracle must compare the restored thought/signature, including absence, rather than search for the wrapper as a literal wire token. JSON tool arguments likewise compare semantically where the protocol carries objects; envelope and signature strings compare exactly.

Pure unsigned text needs no structured metadata. Gateway JSON and SSE checks independently retain that scalar-only shape. Late protected text can publish its already-retained structured metadata at real owner closure while its readable text continues streaming immediately. This neither buffers the visible response nor supplies a partial replay value or fabricated terminal event.
