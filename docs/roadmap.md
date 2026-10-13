# Roadmap

This page owns direction and current scope. Code and tests define behavior; [Architecture](architecture.md) records design decisions. Remove completed slices here; Git keeps history.

## Current focus

First complete the IR's semantic support for all target protocols and modalities. Each slice must state its goal and exit condition before code changes.

IR completeness proceeds in two steps. First, the standard OpenAI Chat, OpenAI Responses and Anthropic Messages media fields, checked against the pinned SDK types (openai-python `5e7a23e`, anthropic-sdk-python `b4b7916`). Second, compatibility with client shapes beyond those standards, such as opencode's.

| Slice | Goal | Exit condition |
|---|---|---|
| 1B (selected) | Document names and instruction media | Messages document `title` maps to the file name both ways; Responses system and developer messages carry `input_image` and `input_file`; targets without a carrier reject them |
| 1C (needs design) | Document citations | Messages `citations`, `content` document sources, `search_result` blocks and cited response locations |

Messages document `context` stays rejected: it is model-visible text with no carrier outside Messages.

The coverage-improvement pass starts only after IR completeness is established. Focused contract tests, safety invariants and the offline baseline remain required for every implementation slice; deferring the coverage pass does not defer those checks.

IR completeness does not mean every upstream can carry every modality. Protocol-specific carrier limits and the [pending decisions](#pending-decisions) remain explicit. This phase does not open file services, generated-image delivery or new Provider bindings.

## Planned order

1. **IR modality completeness.** Complete semantic support for all target protocols and modalities, including input, output and history/replay concepts. Establish completeness against concrete protocol requirements, not just the existing implementation.
2. **Stateful extension and coverage improvement.** After IR completeness, extend Responses with gateway-managed, time-limited `previous_response_id` history and improve contract coverage. No order between these two tracks is selected.
3. **Failover.** The feature remains in early design. Its priority is undecided, lowest for now; design, implementation improvement and real acceptance follow the second stage.
4. **Other agent acceptance.** Codex, Claude Code and other agents follow failover. Their priority is undecided, lowest for now.

This sequence does not select implementation slices or start deferred acceptance runs. Each later stage stays deferred until scheduled.

## Target clients

pi and opencode are the primary clients and real-client gates. Codex, Claude Code and other agent acceptance belong to the final planned stage, with priority undecided and lowest for now.

Pinned client behavior is an acceptance oracle alongside the protocol schema: parsing, stream merging, returned history and error classification. See [References](references.md), [pi acceptance](development.md#pi-acceptance) and [opencode acceptance](development.md#opencode-acceptance).

## Frozen scope

Maintenance only, when a refactor requires it:

- Standalone Images, Speech, Transcription and Embeddings.
- New Providers and model bindings unless an approved slice reopens them. Alternative bindings for an existing public model belong to failover work; activating a registered binding is not adding one.
- New subscription credential features.
- Stateful Responses until the second planned stage is scheduled.
- File services and Realtime.

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

| Work | Scheduling | Exit condition when reopened |
|---|---|---|
| Coverage pass | Second stage, only after IR completeness | Independent wire→IR and IR→wire fixtures, synthetic upstreams and chunked SSE intake |
| Non-image multimodal real checks | Deferred acceptance after IR completeness | One bounded real check for each selected audio, video or document carrier, using model-issued history for tool results; no per-model or format matrix |
| Pinned-client image replay | Deferred, conditional acceptance | If the installed binary differs from the pinned source, run the bounded image tool round trip with a binary built from that revision; do not infer it from installed-client acceptance |
| Real failover | Third stage; priority undecided, lowest for now | One bounded real run for a model with two credentials or Providers and a reproducible advanceable failure |
| Codex and other agents | Final stage; priority undecided, lowest for now | Define the selected client's acceptance contract before reopening |
| Claude Code | Final stage; priority undecided, lowest for now | Sample real request shapes, then verify beta headers/query, token counting and user metadata hints |

The coverage pass covers text, tools, tool errors, reasoning replay, usage, truncation, error classes and media carriers (user and tool-result image, audio, video and document). It also checks client-facing fragment shapes and meaningful absent/null/empty distinctions. It does not authorize a variant matrix.

Codex, Claude Code and other agent acceptance are on hold until scheduled: no acceptance runs or request sampling before then.

Real failover needs a controlled failure source, such as an exhausted key in a scratch store. Functional tests and pi Messages acceptance do not prove Claude Code acceptance.

### Only if needed

- SSE keep-alive comments while initial frames are held.
- Prefix-derived executor affinity when no conversation is declared.
- An opt-in first-content timeout.

## Pending decisions

| Question | Current behavior | Options |
|---|---|---|
| File API (deferred) | Inline data or public URLs only. Inline media fit the IR budgets (12 MiB decoded per item, 32 MiB of IR per request), with narrower target limits where required. Client Provider file IDs and credential-owned file URIs are rejected; there is no gateway file service | Gateway store resolved to inline before lowering; upload pinned to one credential; or a gateway store with per-executor upstream upload. The first and last share one IR slot |

## Future work

- Stateful Responses in the second planned stage, after IR completeness.
- File services.
- Generated-image output with a selected semantic contract and downstream carrier; input support does not authorize this work.
- Resumed standalone media work.
- Realtime.
