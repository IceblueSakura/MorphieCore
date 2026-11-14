# Client and proxy implementation sources

Checked **2026-10-27** against isolated public release checkouts. These projects explain consumer behavior and expose useful failure cases; they are not official-protocol authorities, imported implementations or newly accepted gateway policies. No project execution, Provider calls or host credential inspection formed part of this review.

## Contents

- [Source selection and reuse](#source-selection-and-reuse)
- [pi: actual consumer and history behavior](#pi-actual-consumer-and-history-behavior)
  - [Earlier client observations](#earlier-client-observations)
- [Magpie: cache affinity and failure ownership](#magpie-cache-affinity-and-failure-ownership)
- [LiteLLM: directional bridges and counterexamples](#litellm-directional-bridges-and-counterexamples)
- [CLIProxyAPI: bootstrap retries, late results and refresh](#cliproxyapi-bootstrap-retries-late-results-and-refresh)
- [Outcome and recovery recheck, 2026-11-02](#outcome-and-recovery-recheck-2026-11-02)
- [Applying these references](#applying-these-references)

## Source selection and reuse

| Project | Observed stable release | Inspected revision | License boundary |
|---|---|---|---|
| pi (`earendil-works/pi`) | [1.4.0](https://github.com/earendil-works/pi/releases/tag/v1.4.0) | [`4a08025`](https://github.com/earendil-works/pi/tree/4a080252f06caf63a525d7873c5d94d3c23887e0) | MIT; same source revision as the project pin, not a new installed-client acceptance claim |
| Magpie | [0.1.1734](https://github.com/yetone/magpie/releases/tag/v0.1.1734) | [`e056779`](https://github.com/yetone/magpie/tree/e056779a67d2024a09aa25edad8a4373931a5b64) | MIT |
| LiteLLM | [1.106.4](https://github.com/BerriAI/litellm/releases/tag/v1.106.4) | [`ea7ca16`](https://github.com/BerriAI/litellm/tree/ea7ca16dbc3904d9e59a9505f379a2ca3091821f) | Root [MIT license](https://github.com/BerriAI/litellm/blob/ea7ca16dbc3904d9e59a9505f379a2ca3091821f/LICENSE) excludes all `enterprise/` content; that directory has [separate restricted terms](https://github.com/BerriAI/litellm/blob/ea7ca16dbc3904d9e59a9505f379a2ca3091821f/enterprise/LICENSE.md) |
| CLIProxyAPI | [8.0.44](https://github.com/router-for-me/CLIProxyAPI/releases/tag/v8.0.44) | [`1f917e7`](https://github.com/router-for-me/CLIProxyAPI/tree/1f917e77c4562623aa49e9804d999524151a2f96) | MIT |

Release labels reflect GitHub metadata at the observation time. Check file-level notices before copying code/fixtures; dependencies were not license-audited. Newer source references do not change [test pins](../../tests/upstream/sources.lock.json). For protocol claims start with the [official SDK guide](2026-10-27-official-sdk-sources.md); for accepted MorphieCore behavior use [Architecture](../architecture/index.md).

## pi: actual consumer and history behavior

| Question | Source entry points |
|---|---|
| Protocol adapters and SDK options | [`packages/ai/src/api`](https://github.com/earendil-works/pi/tree/4a080252f06caf63a525d7873c5d94d3c23887e0/packages/ai/src/api): `openai-completions.ts`, `openai-responses.ts`, `openai-responses-shared.ts`, `anthropic-messages.ts`, `google-generative-ai.ts`, `google-shared.ts` |
| History/signature transfer | [`transform-messages.ts`](https://github.com/earendil-works/pi/blob/4a080252f06caf63a525d7873c5d94d3c23887e0/packages/ai/src/api/transform-messages.ts) |
| Thinking allocation | [`simple-options.ts`](https://github.com/earendil-works/pi/blob/4a080252f06caf63a525d7873c5d94d3c23887e0/packages/ai/src/api/simple-options.ts), plus model-specific branches in the adapters |
| Transport retry versus semantic fallback | [`provider-retry.ts`](https://github.com/earendil-works/pi/blob/4a080252f06caf63a525d7873c5d94d3c23887e0/packages/ai/src/utils/provider-retry.ts); `anthropic-messages.ts` strict-tool downgrade |
| Partial events versus final result | [`event-stream.ts`](https://github.com/earendil-works/pi/blob/4a080252f06caf63a525d7873c5d94d3c23887e0/packages/ai/src/utils/event-stream.ts) and [`agent-loop.ts`](https://github.com/earendil-works/pi/blob/4a080252f06caf63a525d7873c5d94d3c23887e0/packages/agent/src/agent-loop.ts) |
| Agent-owned summarization | [`compaction.md`](https://github.com/earendil-works/pi/blob/4a080252f06caf63a525d7873c5d94d3c23887e0/packages/coding-agent/docs/compaction.md) |
| Focused regression candidates | [`packages/ai/test`](https://github.com/earendil-works/pi/tree/4a080252f06caf63a525d7873c5d94d3c23887e0/packages/ai/test): `anthropic-strict-tool-fallback.test.ts`, `transform-messages-cross-model-thinking.test.ts`, `google-shared-signed-empty-blocks.test.ts` |

- **History transfer is a policy, not transparent carriage.** Replay is scoped to Provider/API/model; foreign reasoning and signatures are removed. The transform also normalizes paired IDs and manufactures error results for orphaned calls. Preserve provenance and pairing checks, not these repairs as default gateway semantics. Read the whole transform chain before interpreting an adapter's residual fallback branch.
- **Semantic retry survives transport-retry disabling.** The inspected Anthropic adapter disables SDK retries and permits one narrowly classified preferred-strictness downgrade before publishing the stream. Required strictness does not downgrade. The shared transport helper defaults to zero retries; neither setting proves that one Agent command sends one request.
- **Thinking presets are allocation policy.** Shared defaults are `minimal=1024`, `low=2048`, `medium=8192`, `high=16384`; shared ceilings reserve 1024 answer tokens. These corroborate the earlier `4a5d372` source observation, not universal protocol budgets. Adaptive effort and Google model-specific levels remain separate. MorphieCore's six-level policy belongs in [Architecture](../architecture/adaptation-and-dialects.md#reasoning-controls), not in pi's source contract.
- **Compaction is Agent-owned.** It summarizes projected history, retains raw session entries and call/result boundaries, omits thinking and truncates tool-result text. Its compact-and-retry behavior does not authorize gateway replay after publication. Test consumer delta visibility separately from `result()`/final-message replacement.

### Earlier client observations

Retained from source reviews on **2026-10-21/22**, not re-certified as current OpenCode behavior:

- pi [v1.3.0 source](https://github.com/earendil-works/pi/tree/71023aec1546d6d677a65669c5665c0927ce0dd0) separates strict-mode capability from emitted Responses `strict`; `supportsStrictMode:false` omits the field. Its summary request uses `cacheRetention:"none"`, which does not prove that a target's automatic cache default is disabled.
- OpenCode [`e5a557e`](https://github.com/anomalyco/opencode/tree/e5a557efeee02c94486d391afc48f24a49ebc9a5) prefers native compaction where available, otherwise summarization; missing summary headings can cause a nudge request. Its explicit `strict:false` differs from pi's omission.
- OpenCode core `/api/event` exposes text deltas while the inspected noninteractive consumer prints completed text; pi exposes `assistantMessageEvent.text_delta`. CLI stdout alone is not proof of incremental transport.
- The **2026-10-25** pi 1.4.0 consumer check found `allowEmptySignature:false` converts unsigned thinking to assistant text; enabling it returns an empty signature. Neither behavior supplies a valid native signature. Current MorphieCore rejects unsigned standard Messages reasoning rather than adopting this compatibility repair.

[Development](../development.md#installed-client-acceptance) describes current-client verification conventions. Always use current installed Agents with isolated state and safe unwrapped launchers; release-source pins are independent.

## Magpie: cache affinity and failure ownership

| Question | Source entry points |
|---|---|
| Cache/session affinity | [`affinity.go`](https://github.com/yetone/magpie/blob/e056779a67d2024a09aa25edad8a4373931a5b64/internal/gateway/affinity.go): `stickKey`, `affine`, `answered`, `unanswered` |
| Failure scope and cooldown | [`routing.go`](https://github.com/yetone/magpie/blob/e056779a67d2024a09aa25edad8a4373931a5b64/internal/gateway/routing.go): `failureOf`, `restAfterMarked` |
| Held stream and retries | [`fallback.go`](https://github.com/yetone/magpie/blob/e056779a67d2024a09aa25edad8a4373931a5b64/internal/gateway/fallback.go): `holdWriter`, `scan`, `mayAskAgain`; [`continuation.go`](https://github.com/yetone/magpie/blob/e056779a67d2024a09aa25edad8a4373931a5b64/internal/gateway/continuation.go) |
| Interleaved tools and accounting | [`chat.go`](https://github.com/yetone/magpie/blob/e056779a67d2024a09aa25edad8a4373931a5b64/internal/gateway/chat.go): `toolCall`, `giveHeld`; [`ir.go`](https://github.com/yetone/magpie/blob/e056779a67d2024a09aa25edad8a4373931a5b64/internal/gateway/ir.go): `Usage` |
| Rotating-token ownership | [`account.go`](https://github.com/yetone/magpie/blob/e056779a67d2024a09aa25edad8a4373931a5b64/internal/provider/account.go): `codexTokenRenewing`, `codexRefresh` |
| Small failure-oriented tests | [`affinity_cache_write_test.go`](https://github.com/yetone/magpie/blob/e056779a67d2024a09aa25edad8a4373931a5b64/internal/gateway/affinity_cache_write_test.go), [`translate_stream_test.go`](https://github.com/yetone/magpie/blob/e056779a67d2024a09aa25edad8a4373931a5b64/internal/gateway/translate_stream_test.go) |

Useful mechanisms: a newly written cache can motivate affinity before its first hit; credit/quota/rate/model/proxy failures need different scopes; tool-index switches must not create duplicate calls; token refresh locks, rereads and commits replacement credentials as one ownership operation. Tests distinguish cache write/read, expiry/restart and interleaved tool output.

**Do not transplant:** post-publication continuation resends delivered text as prefill and suppresses echoes; held-stream logic can emit HTTP 200/comments while still considering retries. Those boundaries differ from MorphieCore's no-replay rule. Substituting input tokens when cache counters are absent is inferred routing evidence, not reported cache usage. Writing rotated tokens into another client's store is not authorization for MorphieCore credential discovery. Weight-based routing is not the project's cache-affine failover policy.

## LiteLLM: directional bridges and counterexamples

| Question | Source entry points |
|---|---|
| Responses client → Chat backend | [`responses/litellm_completion_transformation`](https://github.com/BerriAI/litellm/tree/ea7ca16dbc3904d9e59a9505f379a2ca3091821f/litellm/responses/litellm_completion_transformation): request/result `transformation.py`, `handler.py`, `streaming_iterator.py` |
| Chat client → Responses backend | [`completion_extras/litellm_responses_transformation`](https://github.com/BerriAI/litellm/tree/ea7ca16dbc3904d9e59a9505f379a2ca3091821f/litellm/completion_extras/litellm_responses_transformation) |
| Messages bridges | [`llms/anthropic`](https://github.com/BerriAI/litellm/tree/ea7ca16dbc3904d9e59a9505f379a2ca3091821f/litellm/llms/anthropic): `chat/transformation.py`, `pass_through/adapters`, `pass_through/responses_adapters` |
| Google shared backend conversion | [`llms/vertex_ai/gemini`](https://github.com/BerriAI/litellm/tree/ea7ca16dbc3904d9e59a9505f379a2ca3091821f/litellm/llms/vertex_ai/gemini); AI Studio's `llms/gemini/chat/transformation.py` inherits this implementation |
| Shared chunks and routing | [`streaming_handler.py`](https://github.com/BerriAI/litellm/blob/ea7ca16dbc3904d9e59a9505f379a2ca3091821f/litellm/litellm_core_utils/streaming_handler.py), [`router.py`](https://github.com/BerriAI/litellm/blob/ea7ca16dbc3904d9e59a9505f379a2ca3091821f/litellm/router.py), [`cooldown_handlers.py`](https://github.com/BerriAI/litellm/blob/ea7ca16dbc3904d9e59a9505f379a2ca3091821f/litellm/router_utils/cooldown_handlers.py) |
| Directional fixture candidates | [`tests/unit/responses/litellm_completion_transformation`](https://github.com/BerriAI/litellm/tree/ea7ca16dbc3904d9e59a9505f379a2ca3091821f/tests/unit/responses/litellm_completion_transformation), [`tests/unit/completion_extras/litellm_responses_transformation`](https://github.com/BerriAI/litellm/tree/ea7ca16dbc3904d9e59a9505f379a2ca3091821f/tests/unit/completion_extras/litellm_responses_transformation) |

Useful mechanisms: request, result and stream conversion are different paths; dense Chat tool indices differ from mixed Responses output indices; `output_item.done` must not prematurely finish a multi-tool response. Reverse-bridge tests expose this distinction. Failure classification is useful independently of deployment-routing policy.

**Do not transplant:** shared stream handling can synthesize `stop`/`tool_calls` at iterator exhaustion; unknown finish reasons can become completed Responses. Other paths generate missing IDs, embed signatures in IDs, or reconstruct missing calls with a guessed tool and `{}` arguments. These are compatibility policies, not original source facts. Default `simple-shuffle`, weighted routing and permissive repair do not fit MorphieCore's accepted contracts. Neither bridge establishes preservation of interleaved text-part order; retain independent ordering expectations.

## CLIProxyAPI: bootstrap retries, late results and refresh

| Question | Source entry points |
|---|---|
| Session affinity | [`selector.go`](https://github.com/router-for-me/CLIProxyAPI/blob/1f917e77c4562623aa49e9804d999524151a2f96/sdk/cliproxy/auth/selector.go): `SessionAffinitySelector.Pick` |
| Failure scope and stale completions | [`conductor_cooldown.go`](https://github.com/router-for-me/CLIProxyAPI/blob/1f917e77c4562623aa49e9804d999524151a2f96/sdk/cliproxy/auth/conductor_cooldown.go): `MarkResult`, `isStaleExecutionResult` |
| Upstream versus deliverable bootstrap | [`conductor_stream.go`](https://github.com/router-for-me/CLIProxyAPI/blob/1f917e77c4562623aa49e9804d999524151a2f96/sdk/cliproxy/auth/conductor_stream.go), [`handlers_stream.go`](https://github.com/router-for-me/CLIProxyAPI/blob/1f917e77c4562623aa49e9804d999524151a2f96/sdk/api/handlers/handlers_stream.go) |
| Chat → Responses state | [`openai_openai-responses_response.go`](https://github.com/router-for-me/CLIProxyAPI/blob/1f917e77c4562623aa49e9804d999524151a2f96/internal/translator/openai/openai/responses/openai_openai-responses_response.go): `oaiToResponsesState`, sequence/item/call coordinates and late usage |
| Accounting and refresh ownership | [`usage_helpers.go`](https://github.com/router-for-me/CLIProxyAPI/blob/1f917e77c4562623aa49e9804d999524151a2f96/internal/runtime/executor/helps/usage_helpers.go): `UsageReporter`, `StreamUsageBuffer`; [`conductor_refresh.go`](https://github.com/router-for-me/CLIProxyAPI/blob/1f917e77c4562623aa49e9804d999524151a2f96/sdk/cliproxy/auth/conductor_refresh.go): `refreshAuthForRequestAtEpoch` |
| Focused safety tests | [`conductor_cooldown_monotonic_test.go`](https://github.com/router-for-me/CLIProxyAPI/blob/1f917e77c4562623aa49e9804d999524151a2f96/sdk/cliproxy/auth/conductor_cooldown_monotonic_test.go), [`handlers_stream_bootstrap_test.go`](https://github.com/router-for-me/CLIProxyAPI/blob/1f917e77c4562623aa49e9804d999524151a2f96/sdk/api/handlers/handlers_stream_bootstrap_test.go) |

Useful mechanisms:

- Healthy affinity outranks a recovered high-priority candidate; temporary substitution and rebinding the session's home are separate policies.
- An older in-flight success cannot clear a newer cooldown. Execution watermarks, credential versions and registration epochs distinguish stale outcomes from recovery.
- A received upstream chunk may be dropped by transformation. Bootstrap retries reset headers, chunk index and SSE validation state; delivery, not merely receipt, matters.
- Usage can arrive after finish or accompany failure. Refresh serializes by auth ID, then rechecks epoch/token replacement before persisting; persistence failure is not silently successful.

**Do not transplant:** default round-robin, inferred-prefix affinity, synthesized missing call IDs, `{}` argument repair or permissive EOF completion. `EnsurePublished` can publish zero-usage success without a report, and Claude accounting can fold cache creation into cached tokens. None supplies truthful semantic usage or terminal evidence for MorphieCore. Broad request echo is not a Provider-effective settings report.

## Routing source observations, 2026-10-30

Reviewed **2026-10-30**. The revisions below are public main-branch observations, not promoted dependency pins, selected stable releases or executed compatibility evidence. Existing [source guides](../references.md#source-guides) remain the broader reference entry point.

| Project and revision | Relevant observation | Proposed use and boundary |
|---|---|---|
| [BerriAI/litellm, `dbcacc7ab7880d5115376cd6c22d22898c50d08f`](https://github.com/BerriAI/litellm/blob/dbcacc7ab7880d5115376cd6c22d22898c50d08f/litellm/router.py) | Fallback groups wrap full retry loops with no total deadline; retries reselect deployments, so a pinned failing deployment absorbs them (issues #47482, #35303). A 429 `insufficient_quota` is treated as a transient rate limit (#32785). Session pins commit before the call. SDK retries are forced off | Borrow the single retry owner and `Retry-After`-driven cooldown. Avoid nested retry × fallback counts, pre-call commits and quota-as-rate-limit |
| [router-for-me/CLIProxyAPI, `90c447d3a8727d115aa7552ec93ce023736ed492`](https://github.com/router-for-me/CLIProxyAPI/blob/90c447d3a8727d115aa7552ec93ce023736ed492/sdk/cliproxy/auth/conductor_cooldown.go) | Provider-specific quota detection (Codex `usage_limit_reached`, Anthropic unified windows); extend-only deadlines that do not escalate inside an open window; fail fast past `max-retry-interval`. Pool rounds caused a retry storm (#5265); a stream defect cooled every key (#5283); one transient error rebinding sessions led to `rebind-on: quota` (#6537); multi-hour `Retry-After` stalls clients (#6646); encrypted reasoning crossed accounts (#6420) | Borrow typed scopes, extend-only deadlines, fail-fast waits and binding moves only on real account failure. Avoid rounds, shadow quota states, round-robin and derived sessions |
| [yetone/magpie, `eba7187368a8b05740326c90152db7b833cf0cc3`](https://github.com/yetone/magpie/blob/eba7187368a8b05740326c90152db7b833cf0cc3/internal/gateway/affinity.go) | Binding commits only after a complete error-free reply; a late stream error refreshing affinity was a bug (#733). Typed timed rests (rate, quota until reset, credit 30 min), ready-before-cooling with one probe per request (#1799), failover only before real content (#1418). A `strict` mode exists because mid-session account switches risk upstream bans (#1797). Earlier revision [`dca3e2d`](https://github.com/yetone/magpie/blob/dca3e2d716fe21bbb3e239f769a8e7861b135c01/internal/gateway/affinity.go) persists bindings across restarts | Post-success commit and typed timed cooldowns informed the design; probes and persistence were not selected. Avoid patience rounds, first-token hedging, usage spreading, cache-read heuristics and third-party Agent stores |
| [QuantumNous/new-api, `2d8e50bf36e94200b809dfb39e73624ec48b1e23`](https://github.com/QuantumNous/new-api/blob/2d8e50bf36e94200b809dfb39e73624ec48b1e23/middleware/distributor.go) (AGPL; ideas only) | Affinity records when the response status is below 400, so broken streams still refresh it; retries index weighted priority tiers (#6571, #6095); auto-disable is permanent and recovers only through scheduled tests (#7660) | Counterexamples only: time-bounded cooldowns and post-success commits instead |
| [earendil-works/pi, `4954d760504c11df9b2cbdc040346107b6a8153d`](https://github.com/earendil-works/pi/blob/4954d760504c11df9b2cbdc040346107b6a8153d/packages/coding-agent/src/core/settings-defaults.ts) | Agent retry enabled by default with three retries; [retryable failures](https://github.com/earendil-works/pi/blob/4954d760504c11df9b2cbdc040346107b6a8153d/packages/ai/src/utils/retry.ts) include 429 and 5xx | Input for [downstream signals](../architecture/runtime-and-routing.md#downstream-signals); installed-Agent behavior still needs acceptance |
| [anomalyco/opencode, `e5a557efeee02c94486d391afc48f24a49ebc9a5`](https://github.com/anomalyco/opencode/blob/e5a557efeee02c94486d391afc48f24a49ebc9a5/packages/ai/src/provider-error.ts) | `isRetryable` honors `x-should-retry` before its own classification; [session retry](https://github.com/anomalyco/opencode/blob/e5a557efeee02c94486d391afc48f24a49ebc9a5/packages/core/src/session/runner/retry.ts) caps `retry-after` at 15 minutes | Input for [downstream signals](../architecture/runtime-and-routing.md#downstream-signals) |

Primary protocol inputs:

- [OpenAI prompt caching](https://developers.openai.com/api/docs/guides/prompt-caching): caches are not shared across organizations; in-memory entries last about 5–10 idle minutes, extended retention up to 24 hours.
- [Anthropic prompt caching](https://platform.claude.com/docs/en/docs/build-with-claude/prompt-caching): caches are isolated by organization and, on the Claude API, by workspace; default lifetime 5 minutes, optional 1 hour, refreshed on use.
- [OpenAI accounts and sessions](https://developers.openai.com/siwc/token-sharing-open-source/profiles-and-sessions): apps manage several isolated registrations, each with its own issued client and credentials; email may be identical across registrations; the five-hour usage limit is shared across all apps of a ChatGPT user; refresh is serialized per session with the latest replacement. The page does not describe automatic account switching.
- [OpenAI errors and recovery](https://developers.openai.com/siwc/token-sharing-open-source/errors-and-recovery): `subscription_sharing_usage_limit_exceeded` (429) pauses new plan-based requests without inferring a reset time; `subscription_sharing_invalid_user` (401) leads to sign-in again after confirmed revocation or a terminal refresh error; OpenAI does not silently switch a request to another billing path.
- [OpenAI SDK retry documentation](https://github.com/openai/openai-python#retries): automatic SDK retries can multiply gateway attempts. Characterize actual client transmissions separately.

The SIWC and prompt-caching observations come from summarized page fetches on 2026-10-30, and the project observations from research passes over the cited revisions and issues. Recheck the verbatim official contract and relevant upstream revision before implementing a decision that depends on them. No research result here relaxes credential ownership, trust, publication or terminal-honesty boundaries.

## Outcome and recovery recheck, 2026-11-02

Bounded direct-source review informing the [account redesign's outcome decision](../architecture/runtime-and-routing.md#health-and-recovery). Rechecked the previously observed main-branch revisions below, not latest releases or dependency pins. Five files per project were inspected; no upstream project execution, Provider calls or installed-client acceptance was performed. These observations do not themselves approve a recovery policy; the user subsequently finalized the linked design on 2026-11-02. new-api is an AGPL mechanism reference only; no code was copied.

### Magpie

Revision [`eba7187`](https://github.com/yetone/magpie/tree/eba7187368a8b05740326c90152db7b833cf0cc3):

- [`sniff.go`](https://github.com/yetone/magpie/blob/eba7187368a8b05740326c90152db7b833cf0cc3/internal/gateway/sniff.go) records stream termination separately from failure. `response.failed` ends the stream and records an error; a terminal event alone does not establish success.
- [`gateway.go`](https://github.com/yetone/magpie/blob/eba7187368a8b05740326c90152db7b833cf0cc3/internal/gateway/gateway.go#L2818-L2853) checks both status and attempt error before `servedCandidate`/`answered`. Its earlier cancellation branch stops fallback when the request is cancelled before a terminal marker. [`routing.go`](https://github.com/yetone/magpie/blob/eba7187368a8b05740326c90152db7b833cf0cc3/internal/gateway/routing.go#L54-L91) clears health separately from [`affinity.go`](https://github.com/yetone/magpie/blob/eba7187368a8b05740326c90152db7b833cf0cc3/internal/gateway/affinity.go#L515-L558), which can remove the current binding after an unanswered attempt. This differs from MorphieCore's failure-preserves-binding decision.
- [`fallback.go`](https://github.com/yetone/magpie/blob/eba7187368a8b05740326c90152db7b833cf0cc3/internal/gateway/fallback.go) distinguishes held content from already-passing content. It can publish HTTP headers/keepalives before giving up retry rights. All-resting selection can retain one resting probe; usage-cap exclusion, quota exhaustion and ordinary retry exhaustion have different exits, not one universal retry header policy.

The earlier shorthand “complete error-free reply” must be read with these protocol-specific limits: the inspected passthrough path accepts normal Chat EOF without requiring `[DONE]`, while Responses/Anthropic/Gemini require their terminal evidence. Cross-protocol translation and account-specific execution paths were not exhaustively traced. Borrow the separation of terminal, error, health and affinity observations, not its definition of successful completion or binding removal policy.

### LiteLLM

Revision [`dbcacc7`](https://github.com/BerriAI/litellm/tree/dbcacc7ab7880d5115376cd6c22d22898c50d08f):

- [`streaming_handler.py`](https://github.com/BerriAI/litellm/blob/dbcacc7ab7880d5115376cd6c22d22898c50d08f/litellm/litellm_core_utils/streaming_handler.py#L2354-L2470) finalizes asynchronous streaming success at iterator exhaustion and separately dispatches stream failures. After a finish reason, `ReadError`/`RemoteProtocolError` can take the completion path. Do not import this exception into MorphieCore's validated-EOF requirement.
- [`router.py`](https://github.com/BerriAI/litellm/blob/dbcacc7ab7880d5115376cd6c22d22898c50d08f/litellm/router.py#L3841-L3990) pre-reads a first chunk for early failure handling. Mid-stream fallback continuation is off by default but can explicitly reuse delivered text as prefill for a new deployment. That option remains outside MorphieCore's publication boundary.
- [`utils.py`](https://github.com/BerriAI/litellm/blob/dbcacc7ab7880d5115376cd6c22d22898c50d08f/litellm/utils.py#L2203-L2288), [`litellm_logging.py`](https://github.com/BerriAI/litellm/blob/dbcacc7ab7880d5115376cd6c22d22898c50d08f/litellm/litellm_core_utils/litellm_logging.py#L2563-L2592) and [`cooldown_handlers.py`](https://github.com/BerriAI/litellm/blob/dbcacc7ab7880d5115376cd6c22d22898c50d08f/litellm/router_utils/cooldown_handlers.py#L464-L570) distinguish call callbacks, per-attempt failure reporting and cooldown decisions. Background callback dispatch is not proof of completed downstream delivery. Retry exhaustion raises the latest error for the outer fallback path; no-available-deployment can carry cooldown information.

The bounded review did not trace Proxy HTTP error mapping or all native Responses/Anthropic paths. It establishes no final downstream `x-should-retry` contract. Borrow attempt-scoped observations and explicit stream cleanup, not nested retry/fallback budgets, failure-rate cooling or post-publication continuation.

### new-api

Revision [`2d8e50b`](https://github.com/QuantumNous/new-api/tree/2d8e50bf36e94200b809dfb39e73624ec48b1e23):

- [`middleware/distributor.go`](https://github.com/QuantumNous/new-api/blob/2d8e50bf36e94200b809dfb39e73624ec48b1e23/middleware/distributor.go#L164-L169) records affinity when the writer's HTTP status is below 400. [`service/channel_affinity.go`](https://github.com/QuantumNous/new-api/blob/2d8e50bf36e94200b809dfb39e73624ec48b1e23/service/channel_affinity.go#L713-L739) adds configuration/context checks, not a protocol-terminal check.
- [`relay_responses.go`](https://github.com/QuantumNous/new-api/blob/2d8e50bf36e94200b809dfb39e73624ec48b1e23/relay/channel/openai/relay_responses.go#L88-L177) forwards events; its `response.failed`/`incomplete`/cancelled branch resets image-accounting state rather than returning a task failure. The handler's `nil` error alone cannot justify healthy completion.
- [`controller/relay.go`](https://github.com/QuantumNous/new-api/blob/2d8e50bf36e94200b809dfb39e73624ec48b1e23/controller/relay.go#L331-L370) bases retry decisions on error classification and retry state, without a publication check at this layer. [`service/channel.go`](https://github.com/QuantumNous/new-api/blob/2d8e50bf36e94200b809dfb39e73624ec48b1e23/service/channel.go#L19-L64) supplies configurable auto-disable decisions. Scanner, helper and transport paths were not fully traced; this does not establish an actual end-to-end post-publication replay.

Use this as a counterexample to HTTP-status or delivery-function-success affinity commits. Do not copy auto-disable as a replacement for time-bounded health.

### Design implications

The comparison supports recording task outcome, publication/delivery and failure cause separately. It does not supply a common policy for clearing health after a valid refusal, output-limit completion, downstream cancellation or a gateway-local encoding failure. Nor does it settle whether exhausted gateway recovery must prohibit a fresh client request.

The finalized [outcome contract](../architecture/runtime-and-routing.md#health-and-recovery) and [downstream signals](../architecture/runtime-and-routing.md#downstream-signals) own the user's subsequent selections, not these source observations. Success-only affinity, no-replay-after-publication and terminal honesty stay in force. Verify selected client retry signals with installed pi/OpenCode rather than infer their behavior from these gateways.

## Applying these references

Consult mechanisms to formulate independent counterexamples, not to copy a second point-to-point translation architecture. Keep `wire → typed IR → target projection → wire`, owner-local identity, bounded pending state and validated EOF. Select fixtures only after checking their expected repairs against official sources and licenses. Acceptance procedures remain in [Development](../development.md#verification-gates); routing scope remains frozen in [Roadmap](../roadmap/index.md#temporary-routing-freeze).
