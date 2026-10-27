# Downstream client profiles and host acceptance

Records downstream Agent client profiles (pi and OpenCode) and current-host acceptance evidence. [Development](../development.md#production-profile-acceptance) owns test procedures, and [Usage](../usage.md#send-a-request) owns client configuration.

## Contents

- [Production client profiles (2026-10-21)](#production-client-profiles-2026-10-21)
- [Sustained-operation boundaries (2026-10-22)](#sustained-operation-boundaries-2026-10-22)
- [Current-host reacceptance (2026-10-24)](#current-host-reacceptance-2026-10-24)
- [Current-host closure recheck (2026-10-25)](#current-host-closure-recheck-2026-10-25)

## Production client profiles, 2026-10-21

Source inspection of pi `71023ae` (v1.3.0) and OpenCode `e5a557e` (npm `opencode-ai`).

- **pi Responses**: Tool builder separates model strict-mode capability, constrained sampling, and emitted `strict` field. Outer adapter default omits the field.
- **pi Messages**: Adapter has an internal grammar-limit downgrade loop independent of SDK transport retries. Default cache retention is short.
- **pi Compaction**: Standalone generation request with `cacheRetention: "none"`. Conversation text serializes into a summary request; history is then rebuilt. Messages retains automatic cache defaults.
- **OpenCode Compaction**: Prefers native compaction if supported, falling back to ordinary summarization. Missing template headings in summary output can trigger follow-up nudge requests.

Production acceptance requires an explicit profile covering version, model flags, schema policy, session behavior, and retry bounds. Tested against synthetic upstreams with separate tracking of client transmissions and gateway attempts.

## Sustained-operation boundaries, 2026-10-22

- OpenCode emits text deltas on core event subscriptions (`/api/event`), while `noninteractive.ts` reports text only on `session.text.ended`. pi exposes `assistantMessageEvent` type `text_delta`.
- Lifecycle gates verify consumer delta arrival before upstream EOF, peer-close handling on abort/interrupt, single-permit concurrency recovery, and file log rotation (1 MiB boundary).

## Current-host reacceptance, 2026-10-24

Evaluated against host-installed Agent binaries: pi **1.4.0** and OpenCode **2.0.32**.

- **Synthetic Scenarios**: All 12 current-host scenarios pass. pi uses `anthropic-version: 2026-10-21` via configured `model.headers`. Compaction prefix summaries retain focus sections while excluding thinking prose.
- **Billing Diagnosis and Target Selection**: Initial DashScope calls failed with HTTP 400 (`Arrearage`), indicating upstream account billing state. Testing switched to `aliyun-tokenplan-cn-chat` (`qwen3.8-flash`), preserving model reasoning semantics.
- **Real Provider Acceptance**: Both pi 1.4.0 and OpenCode 2.0.32 completed real `aliyun-tokenplan-cn-chat` loops: 3 client requests and 3 attempts per client, capped at 2048 output tokens and 120 seconds, verifying reasoning delivery, tool call/result exchange, and same-session history recall.
- **Offline / Pinned Baselines**: All 16 pinned SDK scenarios and 12 current-host synthetic Agent scenarios pass.

## Current-host closure recheck, 2026-10-25

- **Oracle Maintenance**: Admitted normal local OpenTelemetry trace and log records. pi Messages mid-conversation effort verified as `output_config.effort: low` at root with ordered system message effort.
- **Installed Agent Verification**: All 12 synthetic scenarios pass under isolated state. Real Token Plan Chat gates pass for both clients at 3 requests each.
- **Unsigned Messages Consumer Recheck**: Installed pi 1.4.0 `convertMessages` defaults `compat.allowEmptySignature` to false, returning unsigned thinking blocks as assistant text. Setting the compatibility option returns original thinking bytes with `signature: ""`.
