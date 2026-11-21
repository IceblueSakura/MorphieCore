# MorphieCore Hardening Plan

This plan tracks repairs selected from the 2026-11-21 whole-project review at `b4fd6ee`. Each slice fixes a verified defect without changing [frozen scope](roadmap/backlog-and-frozen.md#frozen-scope), the [routing freeze](roadmap/index.md#temporary-routing-freeze) or public API activation. The [roadmap](roadmap/index.md) owns scope and gates, [architecture](architecture/index.md) owns accepted contracts and [development](development.md) owns verification policy. Completed slices are removed when the plan closes; Git preserves history.

## Status

| Slice | Defect | Status |
|---|---|---|
| 1 | Quadratic stream metadata and history-edit checks | Done |
| 2 | Unbounded connection heads and shutdown drain | Done |
| 3 | Post-publication failures abort without protocol terminals | Done |
| 4 | Data-less SSE blocks dispatched as events | Done |
| 5 | Chat-upstream calls withheld from Messages clients until finish | Planned |
| 6 | Messages delivery rejects upstream call IDs after dispatch | Planned |
| 7 | OAuth renewal withdraws valid access and over-quarantines | Planned |
| 8 | Low-impact contract mismatches | Planned |

Every slice ends with focused tests at the responsible layer, the [offline baseline](development.md#offline-baseline), contract documentation updates where behavior changes, and one commit.

## Slice 1 — Linear stream and history accounting

**Defect.** `StreamEvent::Delta` with logprobs clones, revalidates and re-serializes the whole accumulated logprob vector ([reduce](../crates/morphiecore-semantic/src/task/generation/stream/reduce.rs)); annotation events re-sum every annotation. Decoder, projection and encoder each reduce, so a 4000-token `top_logprobs: 20` stream costs over 10 s of async-worker CPU per pass. `check_call_edits` ([identity](../crates/morphiecore-semantic/src/task/generation/identity.rs)) compares every source operation against every edited item.

**Repair.** Validate and size only appended logprobs and annotations, keeping running per-part annotation/logprob bytes and a stream-wide annotation count. Index item positions once for call-edit, protected-edit, envelope and participant checks; instruction order uses prefix/suffix bounds instead of pairwise scans.

**Result.** A 16,000-delta `top_logprobs: 5` stream reduces in under a second in a debug build (previously over 60 s), and `retain_items` over 32,000 call/result pairs takes 58 ms in release (previously 5.7 s). Existing exact-budget and lifecycle assertions pass unchanged; a new test pins instruction-crossing rejection.

## Slice 2 — Ingress connection bounds and bounded shutdown

**Defect.** `axum::serve` installs no hyper timer, so the header-read timeout is inactive; connections are unlimited, and graceful shutdown waits forever for a partially received request head. A hung shutdown is later killed before storage drains.

**Repair.** Serve with an explicit accept loop over `hyper_util` with a Tokio timer, a header-read timeout, and a connection cap. Bound the post-signal connection drain, then continue to request-task and storage drains.

**Result.** A gateway-owned HTTP/1 accept loop applies a 90 s header-read timeout, a 512-connection cap and a 5 s drain bound ([listener bounds](architecture/runtime-and-routing.md#reverse-proxy-delivery)). Tests show a stalled head is closed, a connection beyond the cap waits until a slot is released, and shutdown completes with a stalled head open; removing the timer or the drain abort fails them.

## Slice 3 — Honest post-publication terminals

**Defect.** After publication, transport, malformed, limit and timeout failures return errors without a protocol error frame, and even a delivered classified error frame is followed by a transport abort. This contradicts the [publication freeze](architecture/runtime-and-routing.md#streaming-and-delivery) contract.

**Repair.** Route every post-publication failure through one in-band error frame path, then end the body cleanly after that frame is accepted. Outer attempt, recovery and exchange deadlines and shutdown ask a published attempt to frame their error within a 2 s handoff bound before dropping it. The runtime delivery lifecycle now documents that it only gates completion; the gateway publication flag owns retry and failover legality.

**Result.** Chat, Responses and Messages clients each receive exactly one error frame and a clean end for transport reset, malformed SSE, byte limit and deadline after publication; disabling deadline framing fails the test. Integration oracles that expected a transport abort now require the error frame.

## Slice 4 — WHATWG SSE dispatch

**Defect.** [`SseDecoder`](../crates/morphiecore-protocol/src/sse.rs) dispatches blocks with an empty data buffer. Messages, generateContent and Interactions decoders then fail on spec-legal `retry:`, `id:` or `event:`-only keep-alives.

**Repair.** Dispatch only blocks with data lines; a field-only block discards its event type and carries `id`/`retry` to the next event. The OpenAI decoders keep skipping empty `data:` payloads, which remain spec events.

**Result.** The framing test pins WHATWG dispatch, and all five upstream decoders produce the same semantic response with `retry:`, `id:` and `event: ping` blocks interleaved between every frame; the old framing fails that test.

## Slice 5 — Incremental Chat calls for Messages clients

**Defect.** The Chat decoder closes the assistant text part and item only at `finish_reason`; the Messages client schedule holds later owners behind the open text block, so tool-use blocks and their argument deltas are withheld until the response finishes.

**Repair.** Close the open text part and message item when the first tool call opens; later content opens a new message item.

**Exit.** A text-then-call Chat stream delivers the Messages tool-use start and argument deltas before the finish chunk; existing ordering and history tests pass.

## Slice 6 — Messages call-ID delivery

**Defect.** Messages delivery requires call IDs matching `[A-Za-z0-9_-]{1,64}` and fails mid-stream on upstream IDs such as `functions.get_weather:0`, after tokens were spent. Request-side projection covers only Messages upstreams.

**Repair.** Apply a named reversible call-ID projection on Messages delivery and invert it on Messages intake, sharing one mapping with the existing request-side rule.

**Exit.** Independent wire→IR and IR→wire tests show an invalid upstream ID reaches the client as a valid ID and returns upstream unchanged in the next turn; valid IDs pass unchanged.

## Slice 7 — OAuth renewal availability

**Defect.** [Renewal preparation](../crates/morphiecore-runtime/src/credential/manager/refresh.rs) clears the whole credential and marks the account `NeedsReauthorization` before the token exchange. Valid access is unavailable during every renewal, and failures that provably sent nothing (cancelled preparation, connection failure) leave the account permanently unusable.

**Repair.** Keep the [quarantine of dispatched, uncertain refresh](usage/credentials-and-diagnostics.md#oauth) unchanged. Quarantine only the refresh material during the exchange, keep unexpired access borrowable until expiry, and restore the refresh material when the exchange provably did not dispatch.

**Exit.** Lifecycle tests cover borrowing during renewal, connect failure before dispatch, cancellation during preparation, and post-dispatch network failure retaining quarantine while access stays usable until expiry.

## Slice 8 — Low-impact contract mismatches

- Standalone tasks forward `Retry-After` only for rate-limit and overload failures within the retry-hint horizon.
- A second attempt that never sends returns the original failure instead of a recovery timeout.
- Messages renders gateway timeouts as `timeout_error`.
- Transport classifies unlisted connect-phase I/O errors as connect failures.
- Chat treats `max_tokens: null` beside `max_completion_tokens` as absent.
- Responses SSE emits `content_part.added`/`done` for `reasoning_text` parts.
- Native Messages, generateContent and Interactions encoders omit unsealed reasoning instead of rendering it as text.
- Direct API-key provenance uses a digest rather than a raw key copy.
- Embedded topology data owns Messages and Images endpoint identities, dialects and defaults, matching the [shared Provider metadata](architecture/index.md#shared-provider-metadata) claim.

**Exit.** Each item has a focused assertion at its owner layer.

## Outside this plan

These review observations need a separate design selection and move to [Unscheduled issues](roadmap/unscheduled-issues.md#unscheduled-issues) when the plan closes: shared standalone-task topology types, validated constructors for standalone task requests, a single `LocalScope::DERIVED` allocator, centralized rule IDs, shared-lock credential reads, and test coupling to catalog order.
