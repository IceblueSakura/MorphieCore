# MorphieCore Hardening Plan

This plan tracks repairs selected from the 2026-11-21 whole-project review at `b4fd6ee`. Each slice fixes a verified defect without changing [frozen scope](roadmap/backlog-and-frozen.md#frozen-scope), the [routing freeze](roadmap/index.md#temporary-routing-freeze) or public API activation. The [roadmap](roadmap/index.md) owns scope and gates, [architecture](architecture/index.md) owns accepted contracts and [development](development.md) owns verification policy. Completed slices are removed when the plan closes; Git preserves history.

## Status

| Slice | Defect | Status |
|---|---|---|
| 1 | Quadratic stream metadata and history-edit checks | Planned |
| 2 | Unbounded connection heads and shutdown drain | Planned |
| 3 | Post-publication failures abort without protocol terminals | Planned |
| 4 | Data-less SSE blocks dispatched as events | Planned |
| 5 | Chat-upstream calls withheld from Messages clients until finish | Planned |
| 6 | Messages delivery rejects upstream call IDs after dispatch | Planned |
| 7 | OAuth renewal withdraws valid access and over-quarantines | Planned |
| 8 | Low-impact contract mismatches | Planned |

Every slice ends with focused tests at the responsible layer, the [offline baseline](development.md#offline-baseline), contract documentation updates where behavior changes, and one commit.

## Slice 1 — Linear stream and history accounting

**Defect.** `StreamEvent::Delta` with logprobs clones, revalidates and re-serializes the whole accumulated logprob vector ([reduce](../crates/morphiecore-semantic/src/task/generation/stream/reduce.rs)); annotation events re-sum every annotation. Decoder, projection and encoder each reduce, so a 4000-token `top_logprobs: 20` stream costs over 10 s of async-worker CPU per pass. `check_call_edits` ([identity](../crates/morphiecore-semantic/src/task/generation/identity.rs)) compares every source operation against every edited item.

**Repair.** Validate and size only appended logprobs and annotations, keeping running per-part metadata bytes and stream-wide annotation counts. Index source operations once by item ID and alias for edit checks.

**Exit.** Existing limit and lifecycle assertions pass unchanged; a regression test bounds a long logprob stream and a large call/result history to linear work.

## Slice 2 — Ingress connection bounds and bounded shutdown

**Defect.** `axum::serve` installs no hyper timer, so the header-read timeout is inactive; connections are unlimited, and graceful shutdown waits forever for a partially received request head. A hung shutdown is later killed before storage drains.

**Repair.** Serve with an explicit accept loop over `hyper_util` with a Tokio timer, a header-read timeout, and a connection cap. Bound the post-signal connection drain, then continue to request-task and storage drains.

**Exit.** Tests show a stalled request head is closed, excess connections wait or close without admitting requests, and shutdown completes with a stalled head open.

## Slice 3 — Honest post-publication terminals

**Defect.** After publication, transport, malformed, limit and timeout failures return errors without a protocol error frame, and even a delivered classified error frame is followed by a transport abort. This contradicts the [publication freeze](architecture/runtime-and-routing.md#streaming-and-delivery) contract.

**Repair.** Route every post-publication failure through the in-band error frame path with its classified public error, then end the body cleanly after that frame is accepted. A clean end after an error frame is not a success terminal. Remove the unused runtime publication fence so the gateway flag is the single owner.

**Exit.** Tests for Chat, Responses and Messages streams show each post-publication failure class yields exactly one protocol error frame, no success terminal and a clean body end.

## Slice 4 — WHATWG SSE dispatch

**Defect.** [`SseDecoder`](../crates/morphiecore-protocol/src/sse.rs) dispatches blocks with an empty data buffer. Messages, generateContent and Interactions decoders then fail on spec-legal `retry:`, `id:` or `event:`-only keep-alives.

**Repair.** Dispatch only when the data buffer is non-empty, retaining `id` and `retry` state; remove per-decoder empty-data special cases.

**Exit.** Framing tests cover field-only blocks; each upstream decoder accepts interleaved keep-alive blocks.

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
