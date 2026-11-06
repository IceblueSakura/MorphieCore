# AR5 Review

This user-requested record owns the current AR5 observations awaiting joint review and a concise summary of passing tests. It is separate from the implementation roadmap and unrelated backlogs. **Additional tests are still expected; combined analysis, adaptation choices and repair ordering are deferred until the user reviews the collected evidence.** No root-cause diagnosis or repair policy is finalized here.

## Contents

- [Test context](#test-context)
- [Pending observations](#pending-observations)
- [Passing tests](#passing-tests)
- [Updating this record](#updating-this-record)

## Test context

Baseline date: **2026-11-06**. Installed clients: unwrapped **pi 1.5.0** and **OpenCode 2.0.32**; SDKs use the project pins. Real tests use isolated state, the project credential loader, serial requests, a **2048-token output ceiling**, **120-second deadlines**, and a fixed request budget per profile without harness retries.

Cross-protocol profiles reuse bundled model metadata with an explicitly selected serializer. They do not qualify native Google client APIs. DeepSeek uses `deepseek-beta-chat`: public `deepseek-v4.1-flash-1017`, canonical/upstream `deepseek-flash`; OpenCode's passing built-in profile uses the canonical label.

Counts below are scoped to the stated tests. Local admission, client configuration, observer failures, upstream rejections and unexercised prerequisites remain distinct. [Development](development.md#production-profile-acceptance) retains the detailed baseline and verification procedures; [Roadmap](roadmap.md#ordered-work-and-exit-gates) owns AR5's exit boundary.

## Pending observations

All entries are **awaiting joint analysis**, not an implementation queue. Identifiers are for this review record only.

| ID | Observation and affected path | Current evidence and unresolved boundary |
|---|---|---|
| R01 | Readable-Chat replay projection: OpenCode→Haiku, Grok and Interactions | `representation.replay_origin` at `event.project` after upstream HTTP 200. Haiku: 3 attempts / 2 complete; Grok and Interactions: 1 attempt / 0 complete each. The ownership/carriage cause remains unconfirmed; origin validation remains intact. |
| R02 | Stateless generateContent signature carriage | OpenCode Chat/Responses and the Responses SDK smoke reach upstream HTTP 200, then fail projection with `codec.unsupported.replay`; ordinary text can carry `thoughtSignature`. Managed continuation passes separately. No disposition for discarding or substituting these attachments is selected. |
| R03 | Luna cross-protocol continuation | Chat feedback gets `400 recovery.request` after 1 complete delivery / 2 attempts. pi Messages recall gets the same rejection after 2 complete deliveries / 3 attempts. The exact rejected upstream field remains unidentified. |
| R04 | DeepSeek empty carriers and returned history | After one complete delivery, pi omits an unsigned `{"type":"text","text":""}` block; visible text, tool-call values and thinking blocks compare equal. OpenCode omits a zero-length assistant message; its count guard stops later checks. Neither next request is forwarded. Generated ownership versus legitimate normalization remains unresolved; nonempty content loss is not established and history guards were not relaxed. |
| R05 | pi strict-profile qualification: Luna and both Google bindings | Configured Responses profiles reject locally with `strict:true` and 0 upstream attempts. Native Luna characterization also records medium/auto reasoning and inline `additional_tools`; its catalog lacks a strict-tools declaration. Endpoint/model qualification remains open; references include the official [Luna features](https://developers.openai.com/api/docs/models/gpt-6-luna) and [strict function-calling contract](https://developers.openai.com/api/docs/guides/function-calling). No strict opt-out was used. |
| R06 | pi Google Messages admission | generateContent and Interactions reject locally with 0 upstream attempts. The profile carries adaptive thinking and `block_binding.prefix_mismatch_behavior:drop_block`; the exact admission cause remains unqualified. |
| R07 | OpenCode DeepSeek public-label configuration | The isolated custom-model declaration for `deepseek-v4.1-flash-1017` fails with `provider.no-route` before reaching the gateway. Built-in `deepseek-flash` acceptance does not qualify this declaration; the three model identities remain separate. |
| R08 | OpenCode error-path resubmission | Some failed cross-protocol profiles autonomously submit another request despite disabled SDK retries. Test admission latches failure and blocks excess submissions before forwarding; none exceeds its three-attempt ceiling. This is a client error-handling observation, not an attributed gateway defect. |
| R09 | Real replay isolation and failover prerequisites | Separate JSON and SSE Luna→Grok probes each stop after one successful Luna source request because no opaque replay is returned. No transfer request is sent. No independent compatible account pair is available for same-model replay/failover; same-key aliases and different-model switching do not qualify that gate. These are unexercised prerequisites, not observed isolation failures. |

## Passing tests

These results are retained independently of the pending observations. They qualify only the listed profiles, not AR5 closure or untested protocol combinations.

| Test group | Passing result | Scope |
|---|---|---|
| Fresh accounts and basic delivery | Five account references validate; Haiku, Grok OAuth, DeepSeek beta and Luna each complete a one-request smoke | Google basic delivery is included in the managed SDK checks below |
| Native Agent tool/history loops | pi and OpenCode: Haiku Messages, DeepSeek beta readable Chat, Grok OAuth Responses; OpenCode: Luna Responses | Each loop completes 3 requests / 3 attempts, real read/result/answer and same-session recall. [Detailed profiles](development.md#production-profile-acceptance) preserve their native controls and model-label qualifications |
| Additional Agent protocol loops | OpenCode Responses→Haiku, Responses→Interactions; pi Messages→Grok | Each completes 3 requests / 3 attempts with tool history and same-session recall |
| generateContent managed Responses SDK | Smoke 1/1; JSON loop 3/3; SSE loop 3/3 | Explicit `store:true` / `previous_response_id`; signed call/text attachments are retained. Stateless carriage remains R02 |
| Interactions managed Responses SDK | 7 requests / 7 attempts / 7 complete deliveries | JSON smoke and JSON/SSE tool loops; four unchanged thought signatures and two native call/result associations verified. Upstream remains stateless. Signed-call replay is unqualified: no signed call was observed |
| Ordinary Generation summaries and rebuilt tools | pi: 6/6 complete; OpenCode: 5/5 complete, on real Haiku | Same-session summary and another real read/result cycle; pi retains the recent turn, OpenCode produces one completed checkpoint. No native-compaction operation or retry/nudge case is claimed |
| Real first-text cancellation and recovery | Each Agent: 3 requests / 3 attempts; `complete → interrupted → complete`, gateway concurrency 1 | pi `--no-tools`; OpenCode `tool_choice:none`. Cancel after visible text, honest aborted/interrupted outcome and same-session single-permit recovery. Remote computation/billing termination is not asserted |
| Independent synthetic regression | 21 installed-client scenarios, 19 SDK scenarios, 8 session-history cases and the Interactions terminal service-tier regression pass | Separate from real Provider and actual-account acceptance |

Interactions capture qualification: the original runner exits nonzero only on an overbroad complete-capture assertion. Post-run safe-boundary verification establishes the listed signatures and associations; optional unsafe argument fragments are excluded, and complete capture coverage is not claimed.

Single normal-request timing observations were pi RPC **1580 / 3685 ms** and OpenCode core SSE **1811 / 4453 ms** for first text / terminal. These are individual client-clock observations, not latency thresholds or comparable transport benchmarks. Expected broken-pipe/reset notes belong to the cancelled forwarding connection.

## Updating this record

Append evidence to the relevant observation or add an identifier for a distinct phenomenon. Record the date, actual client version, selected model/binding/protocol, bounds, counts and failure boundary; retain only redacted summaries, not credentials or private payloads. Keep passing tests separate from unresolved findings and mark unmet prerequisites explicitly.

Do not infer a product defect from an observer/configuration failure, silently relax a guard to obtain a pass, or promote a passing sample into broader acceptance. Once supplementary testing is complete, review the observations together before selecting adaptations or repairs. Accepted design decisions belong in [Architecture](architecture.md), and approved execution sequencing belongs in [Roadmap](roadmap.md).
