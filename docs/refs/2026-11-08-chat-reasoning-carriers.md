# Chat reasoning carrier evidence, 2026-11-08

This dated note records source and consumer evidence for a former extension. The standard-only decision supersedes that public transport; [Architecture](../architecture/history-and-replay.md#standard-chat-reasoning) owns the current contract; [Development](../development.md#overall-acceptance-status) summarizes overall acceptance and [Roadmap](../roadmap/unscheduled-issues.md#unscheduled-issues) owns remaining qualification.

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

## Cross-source thinking transfer, 2026-11-12

The installed `@earendil-works/pi-ai` source, `dist/api/transform-messages.js`, was inspected separately from the former Chat-extension checks above. Its `isSameModel` comparison requires equal `provider`, `api` and model ID. Any mismatch drops the entire `thinking` block, including readable text and signed/redacted forms; it does not convert reasoning into ordinary answer text. Ordinary text remains visible, and tool-call/result associations are retained through coordinated ID normalization.

pi can compare these fields because its assistant session records own their source metadata. A standard stateless API body does not provide equivalent Provider provenance for every unsigned reasoning owner. MorphieCore adopts the permission to discard cross-source reasoning, not a requirement to reproduce that session store or pi's separate synthetic-tool-result repair. [Architecture](../architecture/history-and-replay.md#cross-protocol-and-cross-provider-history) owns the selected loss and safety boundaries.

## OpenAI cross-model reasoning, 2026-11-12

The official [reasoning guide](https://developers.openai.com/api/docs/guides/reasoning#preserve-reasoning-across-calls), inspected 2026-11-12, explicitly permits persisted reasoning reuse among `gpt-5.6-sol`, `gpt-5.6-terra` and `gpt-5.6-luna`. It excludes reuse between GPT-5.6 and GPT-5.5 families and says the service omits incompatible reasoning from model context. Thus a different model ID alone does not establish incompatibility. Payload acceptance and actual context reuse are separate observations.

The [GPT-6 guide](https://developers.openai.com/api/docs/guides/latest-model) and reasoning guide document GPT-6.1 Sol's `all_turns` support, but do not establish an explicit Astra-to-Sol reasoning compatibility pair. The [function-calling guide](https://developers.openai.com/api/docs/guides/function-calling) requires returning reasoning items alongside associated tool results; an unfinished tool cycle needs its own concrete compatibility evidence.

The gateway therefore retains eligible same-source native reasoning and delegates model validation, without copying pi's universal model-equality filter or claiming a model-family compatibility matrix. This decision does not widen compaction, cross-account or cross-Provider replay. [Architecture](../architecture/history-and-replay.md#cross-protocol-and-cross-provider-history) owns the accepted contract.
