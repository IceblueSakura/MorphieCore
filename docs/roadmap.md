# Roadmap

The single source for direction and the current slice. Delete finished items (Git keeps history). Principles live in [architecture](architecture.md).

## Frozen scope

Maintenance only, as refactors require:

- Standalone media and vector tasks: Images, Speech, Transcription, Embedding.
- New Providers or model bindings (except during the Google phases).
- New features for subscription credentials (SIWC, Grok).
- Stateful Responses, file services, Realtime.

## Current focus: Phase 1 — protocol boundaries and product bindings

- Decide how unknown OpenAI SSE event types are handled. Ignoring unknown fields must not widen into skipping unknown content or terminals. Frozen media decoders are not extended.
- Add the default CLI binding for native Messages, enabled explicitly. Verify model release metadata first; never invent dates. `MESSAGES_BINDINGS` compiles explicit topology only; directory bootstrap does not activate it yet.
- Model the `signature` of upstream Chat `reasoning.text` details, which is currently ignored. Extend cross-protocol carriers for multiple thinking blocks and interleaved message/block order, and express tool errors downstream.

Further trimming of diagnostics/billing switches and fidelity records is assessed with the affected modules, not as a prerequisite. `morphiecore-auth` stays independent.

Exit: `Responses / Chat × Anthropic`, JSON and SSE, covers text, tool round trips, tool errors, thinking replay, truncation and upstream errors, with synthetic tests and one bounded real call (opencode-go `claude-haiku-5-5` or another available target).

## Later phases

### Phase 2 — cross-protocol consistency matrix

One independent fixture per scenario per protocol, run against a synthetic upstream for every downstream × upstream pair: text, single and parallel tools, tool errors, reasoning replay, usage, truncation, upstream errors, post-commit stream loss. New protocols then mostly add fixtures.

### Phase 3 — Google generateContent upstream

IR questions to settle first:

- `thoughtSignature` attaches to parts (including `functionCall`); confirm `Part.replay` and `CallContext.replay` carry it directly.
- `functionCall.id` may be missing: the gateway must mint call IDs and map back by name and order. Check whether `NativeCallAlias` suffices; if not, report the IR gap with options first.
- Streaming returns `GenerateContentResponse` fragments, close to Chat chunks.
- More `finishReason` values (SAFETY, MALFORMED_FUNCTION_CALL, …) and usage details (`thoughtsTokenCount`, `cachedContentTokenCount`).
- Schemas use the OpenAPI subset or `parametersJsonSchema`.

### Phase 4 — Google Interactions upstream

Submit full history statelessly, without server-side storage; reuse the opaque envelope. Pin the official API version before starting.

### Phase 5 — downstream Anthropic Messages

Reuse the bidirectional Anthropic module from Phase 1; add request decoding and response/SSE encoding.

### After that

Stateful Responses (gateway-managed, time-limited history), file services, resumed media work, Realtime.

## Open questions

- The full set of Chat extension fields: beyond reasoning, do tool errors or refusal details need carriers?
- Unknown fields in downstream requests: ignore and record, or reject fields that exist in the standard but are not implemented?
- When switching Provider or model, should visible thinking be passed to the new target as assistant text?
