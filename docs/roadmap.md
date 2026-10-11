# Roadmap

The single source for direction and the current slice. Delete finished items (Git keeps history). Principles live in [architecture](architecture.md).

## Frozen scope

Maintenance only, as refactors require:

- Standalone media and vector tasks: Images, Speech, Transcription, Embedding.
- New Providers or model bindings, except during the Google phases. Activating an existing binding is not a new binding.
- New features for subscription credentials (SIWC, Grok).
- Stateful Responses, file services, Realtime.

## Current focus: Phase 1 — protocol boundaries and product bindings

Goal: OpenAI clients reach a native Messages upstream through the normal CLI startup, with no silent loss of reasoning, tool or terminal semantics.

1. **Unknown OpenAI SSE event types.** Responses intake currently rejects any unregistered `type`. Decide which unknown events may be skipped; ignoring unknown fields must not widen into skipping unknown content or terminals. Frozen media decoders are not extended.
2. **Activate the native Messages binding.** `MESSAGES_BINDINGS` compiles only an explicit topology, and directory bootstrap rejects the `AnthropicMessages` protocol. Activate it only when listed in `models`, never by default. Verify model release metadata first; never invent dates.
3. **Chat reasoning carriers.** Model the `signature` of Chat `reasoning.text` details (currently rejected) through the envelope. Carry multiple thinking blocks and interleaved thinking/text/tool order instead of rejecting them where the Chat shape allows it.
4. **Tool errors.** Neither OpenAI downstream protocol has an error flag on tool results, so `is_error` is rejected there. Decide whether a Chat/Responses extension carrier is needed (open question below) before implementing; a failed tool reported as plain text already round-trips.

Further trimming of diagnostics/billing switches and fidelity records is assessed with the affected modules, not as a prerequisite. `morphiecore-auth` stays independent.

Exit: `Responses / Chat × Anthropic`, JSON and SSE, covers text, tool round trips, tool failures, thinking replay, truncation and upstream errors, with synthetic tests and one bounded real call (opencode-go `claude-haiku-5-5` or another available target) through the CLI-started gateway.

## Later phases

### Phase 2 — cross-protocol scenario fixtures

Scenarios: text, single and parallel tools, tool errors, reasoning replay, usage, truncation, upstream errors, post-commit stream loss. Each protocol gets one hand-written wire fixture per scenario it can express, checked wire→IR and IR→wire against a synthetic upstream. Pair-level tests exist only where a projection rule is specific to that downstream × upstream pair; no full pair matrix (see the test rules in [AGENTS.md](../AGENTS.md#tests)). A new protocol then mostly adds its own fixtures.

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

Reuse the Anthropic module from Phase 1; add request decoding and response/SSE encoding. Native `is_error`, thinking signatures and block order map directly.

### After that

Stateful Responses (gateway-managed, time-limited history), file services, resumed media work, Realtime.

## Open questions

- Tool errors and refusal details in Chat/Responses: add extension carriers (following existing OpenRouter-style shapes where one exists), or keep failures as plain tool output? Gates Phase 1 item 4.
- Unknown fields in downstream requests are currently rejected (closed request shapes). Keep that, or ignore and record standard fields that are not implemented, so newer SDK parameters do not fail?
- When switching Provider or model, should visible thinking be passed to the new target as assistant text?
