# Client and proxy implementation sources

Checked **2026-10-27** against isolated public release checkouts. These projects explain consumer behavior and expose useful failure cases; they are not official-protocol authorities, imported implementations or newly accepted gateway policies. No project execution, Provider calls or host credential inspection formed part of this review.

## Contents

- [Source selection and reuse](#source-selection-and-reuse)
- [pi: actual consumer and history behavior](#pi-actual-consumer-and-history-behavior)
  - [Earlier client observations](#earlier-client-observations)
- [Magpie: cache affinity and failure ownership](#magpie-cache-affinity-and-failure-ownership)
- [LiteLLM: directional bridges and counterexamples](#litellm-directional-bridges-and-counterexamples)
- [CLIProxyAPI: bootstrap retries, late results and refresh](#cliproxyapi-bootstrap-retries-late-results-and-refresh)
- [Applying these references](#applying-these-references)

## Source selection and reuse

| Project | Observed stable release | Inspected revision | License boundary |
|---|---|---|---|
| pi (`earendil-works/pi`) | [1.4.0](https://github.com/earendil-works/pi/releases/tag/v1.4.0) | [`4a08025`](https://github.com/earendil-works/pi/tree/4a080252f06caf63a525d7873c5d94d3c23887e0) | MIT; same source revision as the project pin, not a new installed-client acceptance claim |
| Magpie | [0.1.1734](https://github.com/yetone/magpie/releases/tag/v0.1.1734) | [`e056779`](https://github.com/yetone/magpie/tree/e056779a67d2024a09aa25edad8a4373931a5b64) | MIT |
| LiteLLM | [1.106.4](https://github.com/BerriAI/litellm/releases/tag/v1.106.4) | [`ea7ca16`](https://github.com/BerriAI/litellm/tree/ea7ca16dbc3904d9e59a9505f379a2ca3091821f) | Root [MIT license](https://github.com/BerriAI/litellm/blob/ea7ca16dbc3904d9e59a9505f379a2ca3091821f/LICENSE) excludes all `enterprise/` content; that directory has [separate restricted terms](https://github.com/BerriAI/litellm/blob/ea7ca16dbc3904d9e59a9505f379a2ca3091821f/enterprise/LICENSE.md) |
| CLIProxyAPI | [8.0.44](https://github.com/router-for-me/CLIProxyAPI/releases/tag/v8.0.44) | [`1f917e7`](https://github.com/router-for-me/CLIProxyAPI/tree/1f917e77c4562623aa49e9804d999524151a2f96) | MIT |

Release labels reflect GitHub metadata at the observation time. Check file-level notices before copying code/fixtures; dependencies were not license-audited. Newer source references do not change [test pins](../../tests/upstream/sources.lock.json). For protocol claims start with the [official SDK guide](2026-10-27-official-sdk-sources.md); for accepted MorphieCore behavior use [Architecture](../architecture.md).

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
- **Thinking presets are allocation policy.** Shared defaults are `minimal=1024`, `low=2048`, `medium=8192`, `high=16384`; shared ceilings reserve 1024 answer tokens. These corroborate the earlier `4a5d372` source observation, not universal protocol budgets. Adaptive effort and Google model-specific levels remain separate. MorphieCore's six-level policy belongs in [Architecture](../architecture.md#reasoning-controls), not in pi's source contract.
- **Compaction is Agent-owned.** It summarizes projected history, retains raw session entries and call/result boundaries, omits thinking and truncates tool-result text. Its compact-and-retry behavior does not authorize gateway replay after publication. Test consumer delta visibility separately from `result()`/final-message replacement.

### Earlier client observations

Retained from source reviews on **2026-10-21/22**, not re-certified as current OpenCode behavior:

- pi [v1.3.0 source](https://github.com/earendil-works/pi/tree/71023aec1546d6d677a65669c5665c0927ce0dd0) separates strict-mode capability from emitted Responses `strict`; `supportsStrictMode:false` omits the field. Its summary request uses `cacheRetention:"none"`, which does not prove that a target's automatic cache default is disabled.
- OpenCode [`e5a557e`](https://github.com/anomalyco/opencode/tree/e5a557efeee02c94486d391afc48f24a49ebc9a5) prefers native compaction where available, otherwise summarization; missing summary headings can cause a nudge request. Its explicit `strict:false` differs from pi's omission.
- OpenCode core `/api/event` exposes text deltas while the inspected noninteractive consumer prints completed text; pi exposes `assistantMessageEvent.text_delta`. CLI stdout alone is not proof of incremental transport.
- The **2026-10-25** pi 1.4.0 consumer check found `allowEmptySignature:false` converts unsigned thinking to assistant text; enabling it returns an empty signature. Neither behavior supplies a valid native signature. Current MorphieCore rejects unsigned standard Messages reasoning rather than adopting this compatibility repair.

[Development](../development.md#production-profile-acceptance) owns installed-version profiles and acceptance. Always use current installed Agents with isolated state and safe unwrapped launchers; release-source pins are independent.

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

## Applying these references

Consult mechanisms to formulate independent counterexamples, not to copy a second point-to-point translation architecture. Keep `wire → typed IR → target projection → wire`, owner-local identity, bounded pending state and validated EOF. Select fixtures only after checking their expected repairs against official sources and licenses. Acceptance procedures remain in [Development](../development.md#verification-gates); routing scope remains frozen in [Roadmap](../roadmap.md#temporary-routing-freeze).
