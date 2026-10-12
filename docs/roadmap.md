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

## Current focus: Phases 5 and 6 — Google upstreams

Target: `gemini-3.8-flash` on Google AI Studio (`generativelanguage.googleapis.com`, `x-goog-api-key`), one public model with two bindings in failover order: generateContent first, Interactions v1 second. Gemini 3 only, so `functionCall.id` is always present; a missing ID is a protocol error and the gateway mints none. Real runs wait for the user's Google key; offline work does not.

Phase 5 — generateContent:

- Always `:streamGenerateContent?alt=sse` upstream; JSON clients get the aggregate.
- Each `thoughtSignature` becomes a reasoning item whose `GoogleGenerateContentPart` value names its anchor: the function call ID, its own thought part, or the content's tail. The encoder restores it by anchor, so existing downstream reasoning carriers return it and downstream reordering (Chat puts reasoning first) cannot misplace it. Thought parts always carry such a value, so the same target gets them back as thoughts while other targets get readable text.
- Effort maps to `thinkingLevel` `low`/`medium`/`high`; `none` and `minimal` become `low` by a named rule (3.8 Flash rejects `minimal`). Tool schemas go as `parametersJsonSchema`; tool results as `functionResponse` with the call ID.
- Finish reasons, `usageMetadata` (`thoughtsTokenCount`, `cachedContentTokenCount`) and Google error bodies map onto the existing stop, usage and error classes.

Phase 6 — Interactions v1:

- Stateless: `store: false`, no `previous_interaction_id`, full history as steps, always streamed.
- `thought` steps map to reasoning items with `GoogleInteractionsV1Thought` values; `function_call` and `function_result` steps to calls and results.
- The two Google formats are separate replay domains: a value of the other format is dropped as foreign, so failover between the bindings loses reasoning instead of failing.

Exit: hand-written wire→IR and IR→wire tests for both codecs, gateway tests against a synthetic upstream for JSON and SSE delivery, then pi acceptance through all three of its providers on each binding: a read-tool loop with thinking where the second turn replays the signature without a 400. Afterwards delete the IR's part- and call-level replay slots if nothing uses them.

## Later phases

### After that

Stateful Responses (gateway-managed, time-limited history), file services, resumed media work, Realtime.
