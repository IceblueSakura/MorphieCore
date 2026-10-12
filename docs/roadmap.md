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

## Current focus: Phase 3 — failover routing with cache affinity

A public model may have several bindings (Provider × upstream model × credential pool). The compiled order is the failover order; runtime policy only skips unhealthy executors and keeps a conversation where its cache and its account-bound thinking live. No load balancing.

- **Alternative bindings.** Add one only where a target agent's model needs it; the first is `glm-5.3` via OpenRouter beside Zhipu.
- **Execution identity**: Endpoint × credential identity × upstream model. Credential identity is the binding plus an in-memory API key fingerprint (never logged), or the verified OAuth principal. Cooldown, affinity and diagnostics key on it, never on ordinals or display names.
- **Cooldown.** Rate limits, quota or credit exhaustion and authentication failures, including errors inside a held stream, demote the executor for its scope (credential or credential × model) until the reported reset, capped. Hard exclusions (disabled, unauthorized, unsupported by the request) are never revived. Refusals and context overflow cause no cooldown.
- **Cache affinity first.** A conversation (its declared ID or agent session header, or the prefix index) binds to the executor that last completed honestly. Later turns and tool round trips go there while it is legal and not cooling down. After a failover the conversation stays on the new executor until that one fails; it does not drift back, since each move loses the cache and the account-bound thinking. Feedback only from honest completed terminals; in memory, bounded, disposable.
- **Held streams.** Today the first lifecycle event (`response.created`, the Chat role chunk, `message_start`) is published at once and freezes the candidate, so an upstream that opens a stream and then reports overload cannot fail over. While another candidate could still answer, hold lifecycle-only frames until the first content (text, tool call, reasoning, terminal) or a byte or time cap, releasing rather than dropping. A pre-content upstream error advances. After headers, a final failure uses the agent error classes (architecture §8). Keep-alive comments only where a target agent needs them: they do not reset Codex's 300 s idle timeout, so the total hold stays below it. Follow the structure of Magpie's held-stream tests (`TestSilentHeldStreamStillFailsOver`, `TestSilentHeldStreamKeptAlive`, `TestQuietKeptAliveOnlySoLong`, `TestNotAnAPIReplyFailsOver`).
- **Non-API 2xx** (HTML, empty, or non-JSON under a JSON type) is a 5xx-class failure decided from the first bytes; its cooldown scope is the Endpoint. Diagnostics name the reason without copying the body.
- **Diagnostics**: the affinity decision, skipped executors and why, each physical attempt with its executor. Missing cache reports stay missing.

Exit: synthetic two-binding routes show failover on 5xx, rate limit and a pre-content stream error; a conversation staying on its executor across turns and tool round trips, and on the new one after a failover; cooldown expiry; removed or disabled targets not revived; no binding reuse after a key change; no sharing across ingress keys for one conversation ID; state released on concurrent first requests and cancellation; compiled order kept without affinity. One bounded real failover run with two credentials or Providers for one model.

Deferred until a real need: an opt-in first-content timeout, per-message effort to keep the cache when clients change effort mid-conversation.

## Later phases

### Phase 4 — downstream Anthropic Messages for Claude Code

Reuse the Anthropic module from Phase 1; add request decoding and response/SSE encoding. Native `is_error`, thinking signatures and block order map directly. Toward OpenAI targets, keep tool result text and drop `is_error` by a named lossy rule. Claude Code's request shapes and error handling are the acceptance oracle.

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
