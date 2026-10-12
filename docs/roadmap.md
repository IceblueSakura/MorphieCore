# Roadmap

This page owns direction and current scope. Code and tests define behavior; [Architecture](architecture.md) records design decisions. Remove completed slices here; Git keeps history.

## Current focus

No feature slice is selected. The next planned work is the [coverage pass](#deferred-acceptance), now including the multimodal carriers. Deferred acceptance work stays deferred until the user reopens it. [Pending decisions](#pending-decisions) stay as described until the user decides them.

## Target clients

pi and opencode first; Codex and Claude Code second. pi and opencode are the real-client gates.

Pinned client behavior is an acceptance oracle alongside the protocol schema: parsing, stream merging, returned history and error classification. See [References](references.md), [pi acceptance](development.md#pi-acceptance) and [opencode acceptance](development.md#opencode-acceptance).

## Frozen scope

Maintenance only, when a refactor requires it:

- Standalone Images, Speech, Transcription and Embeddings.
- New Providers and model bindings unless an approved slice reopens them. Alternative bindings for an existing public model belong to failover work; activating a registered binding is not adding one.
- New subscription credential features.
- Stateful Responses, file services and Realtime.

### Not planned

- Raw wire passthrough, post-encoding JSON patches or weakened schemas.
- Invented tool results or changed roles for orphan results.
- Message-content hashes as conversation identity, or LLM classifiers for routing.
- Plugin hosts, middleware hosts or capabilities learned automatically from upstream errors.
- Load balancing, credential concurrency/RPM lanes or budget reservations.
- Retrying a refusal on another model.
- Routing below the declared conversation: turn detection, branch/subtask keys, per-turn holds or model switches, and per-message effort.

These boundaries follow [Failover](architecture.md#failover) and [Settled decisions](architecture.md#settled-decisions).

## Maintenance rules

Observed vendor stream shapes become named dialect rules only when a bound upstream emits them. Examples include empty continuation tool names or call IDs, empty finish reasons, and tool-call openers.

Treat a bound upstream's in-stream error as a classified failure, not normal output. Do not extend frozen media decoders as part of this work.

## Deferred acceptance

| Work | Exit condition when reopened |
|---|---|
| Real failover | One bounded real run for a model with two credentials or Providers and a reproducible advanceable failure |
| Coverage pass | Independent wire→IR and IR→wire fixtures, synthetic upstreams and chunked SSE intake |
| Multimodal real checks | One bounded real call for each [open question](refs/multimodal.md#open-questions), and one Responses tool-result image per enabled OpenAI-compatible upstream |
| Claude Code (on hold) | Sample real request shapes, then verify beta headers/query, token counting and user metadata hints |

The coverage pass covers text, tools, tool errors, reasoning replay, usage, truncation, error classes and media carriers (user and tool-result image, audio, video and document). It also checks client-facing fragment shapes and meaningful absent/null/empty distinctions. It does not authorize a variant matrix.

Claude Code acceptance is on hold until the user schedules it: no runs or request sampling before then.

Real failover needs a controlled failure source, such as an exhausted key in a scratch store. Functional tests and pi Messages acceptance do not prove Claude Code acceptance.

### Only if needed

- SSE keep-alive comments while initial frames are held.
- Prefix-derived executor affinity when no conversation is declared.
- An opt-in first-content timeout.

## Pending decisions

| Question | Current behavior | Options |
|---|---|---|
| Tool-result media on Chat upstreams | Chat tool messages carry text only, so Chat clients cannot send tool media. Tool media from Messages clients or through the Responses-to-Chat bridge fails representation on Chat upstreams | A named rule that moves the media into a following user message, as pi does client-side; or keep rejecting |
| File API | Inline data or public URLs only. Inline media fit the IR budgets (12 MiB decoded per item, 32 MiB of IR per request). Client Provider file IDs are rejected; there is no gateway file service | Gateway store resolved to inline before lowering; upload pinned to one credential; or a gateway store with per-executor upstream upload. The first and last share one IR slot |

## Future work

- Stateful Responses with gateway-managed, time-limited `previous_response_id` history.
- File services.
- Resumed standalone media work.
- Realtime.
