# Roadmap

The single source for direction and the current slice. Delete finished items (Git keeps history). Principles and settled decisions live in [architecture](architecture.md); sources in [references](references.md).

## Frozen scope

Maintenance only, as refactors require:

- Standalone media and vector tasks: Images, Speech, Transcription, Embedding.
- New Providers or model bindings, except alternative bindings of an existing public model in Phase 3 and the Google phases. Activating an existing binding is not a new binding.
- New features for subscription credentials (SIWC, Grok).
- Stateful Responses, file services, Realtime.

Not planned: raw wire passthrough or post-encoding JSON patches, weakening schemas (`oneOf` → `anyOf`), synthesizing missing tool results or re-roling orphan results, first-message hashes as conversation identity, LLM intent classifiers for routing, plugin or middleware hosts, capabilities learned automatically from upstream errors. Load balancing in any form (weights, round robin, least load), per-credential concurrency or RPM lanes, budget reservations, turn-start routing rules, and retrying a refusal on another model: routing is failover only (architecture §5).

## Target agents

pi and opencode first; Codex and Claude Code second. A known client's observed behavior at a pinned revision is the acceptance oracle, beside the protocol schema: what it parses, merges, returns in history and classifies as retryable. pi's merged `reasoning_details` already shaped the Chat reasoning carrier.

Vendor stream shapes are a standing rule, not a phase: a Chat shape seen from a bound upstream (a continuation `function.name: ""`, `finish_reason: ""`, `tool_calls: []` opening the message owner early, `finish_reason: "stop"` beside tool calls) becomes a named dialect rule, like `empty_continuation_call_id`, once that upstream emits it. Likewise an OpenAI-compatible upstream's in-stream error (a Chat error chunk, a Responses `response.failed`) is classified like Anthropic `error` events once a bound upstream emits one. Frozen media decoders are not extended.

## Open from Phase 3

Failover routing is done (architecture §5). Not accepted: one bounded real failover run with two credentials or Providers for one model; it needs a reproducible advanceable failure, such as a zero-credit or exhausted key in a scratch credential store.

Deferred until a real need: SSE keep-alive comments while frames are held (the hold stays far below Codex's 300 s idle timeout), executor affinity from the prefix index for requests without a declared conversation (all target agents declare one), an opt-in first-content timeout, per-message effort to keep the cache when clients change effort mid-conversation.

## Current focus: Phase 4 — downstream Anthropic Messages for Claude Code

Reuse the Anthropic module from Phase 1; add request decoding and response/SSE encoding. Native `is_error`, thinking signatures and block order map directly. Toward OpenAI targets, keep tool result text and drop `is_error` by a named lossy rule. Claude Code's request shapes and error handling are the acceptance oracle.

## Later phases

### Phase 5 — Google generateContent upstream

Only when a target agent needs a Gemini model. IR questions to settle first:

- `thoughtSignature` attaches to parts (including `functionCall`); confirm `Part.replay` and `CallContext.replay` carry it directly.
- `functionCall.id` may be missing: the gateway must mint call IDs and map back by name and order. Check whether `NativeCallAlias` suffices; if not, report the IR gap with options first.
- Streaming returns `GenerateContentResponse` fragments, close to Chat chunks.
- More `finishReason` values (SAFETY, MALFORMED_FUNCTION_CALL, …) and usage details (`thoughtsTokenCount`, `cachedContentTokenCount`).
- Schemas use the OpenAPI subset or `parametersJsonSchema`.

### Phase 6 — Google Interactions upstream

Submit full history statelessly, without server-side storage; reuse the opaque envelope. Pin the official API version before starting.

### Coverage pass (after the feature phases)

Feature phases add functional tests only. Once they are done: one hand-written wire fixture per scenario per protocol, checked wire→IR and IR→wire against a synthetic upstream, each SSE fixture also fed in small fixed-size chunks, with consumer expectations from pi and opencode. Scenarios: text, single and parallel tools, tool errors, reasoning replay, usage, truncation, each error class; consumer shapes (a tool name only on the first fragment, no empty reasoning or tool-call deltas, null/empty/absent kept distinct).

### After that

Stateful Responses (gateway-managed, time-limited history), file services, resumed media work, Realtime.
