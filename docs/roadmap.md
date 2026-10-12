# Roadmap

The single source for direction and the current slice. Delete finished items (Git keeps history). Principles and settled decisions live in [architecture](architecture.md); sources in [references](references.md).

## Frozen scope

Maintenance only, as refactors require:

- Standalone media and vector tasks: Images, Speech, Transcription, Embedding.
- New Providers or model bindings, except alternative bindings of an existing public model in Phase 3 and the Google phases. Activating an existing binding is not a new binding.
- New features for subscription credentials (SIWC, Grok).
- Stateful Responses, file services, Realtime.

Not planned: raw wire passthrough or post-encoding JSON patches, weakening schemas (`oneOf` → `anyOf`), synthesizing missing tool results or re-roling orphan results, first-message hashes as conversation identity, LLM intent classifiers for routing, plugin or middleware hosts, capabilities learned automatically from upstream errors. Load balancing in any form (weights, round robin, least load), per-credential concurrency or RPM lanes, budget reservations, and retrying a refusal on another model: routing is failover only (architecture §5). Routing units below the conversation: turn-boundary detection, branch or subtask identity, per-turn holds, rules or model switches, and per-message effort; the declared conversation is the routing unit (architecture §8).

## Target agents

pi and opencode first; Codex and Claude Code second. Real-client acceptance runs use pi first. A known client's observed behavior at a pinned revision is the acceptance oracle, beside the protocol schema: what it parses, merges, returns in history and classifies as retryable. pi's merged `reasoning_details` already shaped the Chat reasoning carrier.

Vendor stream shapes are a standing rule, not a phase: a Chat shape seen from a bound upstream (a continuation `function.name: ""`, `finish_reason: ""`, `tool_calls: []` opening the message owner early, `finish_reason: "stop"` beside tool calls) becomes a named dialect rule, like `empty_continuation_call_id`, once that upstream emits it. Likewise an OpenAI-compatible upstream's in-stream error (a Chat error chunk, a Responses `response.failed`) is classified like Anthropic `error` events once a bound upstream emits one. Frozen media decoders are not extended.

## Deferred

Not current work; each returns only when the user reopens it.

- **Real failover acceptance.** Failover routing is done (architecture §5); one bounded real run with two credentials or Providers for one model is not. It needs a reproducible advanceable failure, such as a zero-credit or exhausted key in a scratch credential store.
- **Coverage pass.** Feature phases add functional tests only. The pass adds one hand-written wire fixture per scenario per protocol, checked wire→IR and IR→wire against a synthetic upstream, each SSE fixture also fed in small fixed-size chunks, with consumer expectations from pi and opencode. Scenarios: text, single and parallel tools, tool errors, reasoning replay, usage, truncation, each error class; consumer shapes (a tool name only on the first fragment, no empty reasoning or tool-call deltas, null/empty/absent kept distinct).
- **Claude Code acceptance.** Downstream Messages is covered by functional tests and pi's `anthropic-messages` provider. When reopened: sample Claude Code's request shapes first; then `anthropic-beta`, `?beta=true`, `count_tokens`, `metadata.user_id` as a conversation hint.

Until a real need: SSE keep-alive comments while frames are held (the hold stays far below Codex's 300 s idle timeout), executor affinity from the prefix index for requests without a declared conversation (all target agents declare one), an opt-in first-content timeout.

## Current focus: Phase 6 — replay representation cleanup

Google signatures now use reasoning-item carriers. Remove part- and call-level replay slots with no runtime consumer, keeping Provider-tool replay and the safety assertions for surviving owners. Exit: affected tests and the full offline baseline pass, with the source coverage change reported. Protocol and real-client acceptance do not substitute for this cleanup.

## Later phases

### After that

Stateful Responses (gateway-managed, time-limited history), file services, resumed media work, Realtime.
