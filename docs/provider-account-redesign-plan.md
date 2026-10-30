# Provider account redesign plan

**Status: reopened for requirements revision, 2026-10-30. Requirements come first; implementation is a later rewrite. No implementation design or schedule is approved.**

This document records the user's requirements, the user's decisions and the recommendations still under review. Requirements and dated user decisions describe the desired outcome; recommendations, example values and implementation slices remain proposals. Do not implement the design, change credentials or activate bindings from this document until the user approves a rewrite scope.

[Architecture](architecture.md) remains the authority for accepted contracts. [Roadmap](roadmap.md#temporary-routing-freeze) retains the current routing freeze and owns scheduling. This separately requested plan owns the account-redesign requirements and proposal; it does not reopen routing through the active IR/codec migration. [Usage](usage.md#prepare-credentials) describes the currently implemented configuration, not the shapes discussed here.

## Contents

- [User requirements](#user-requirements)
- [Current implementation and rewrite boundary](#current-implementation-and-rewrite-boundary)
- [Proposed account and configuration model](#proposed-account-and-configuration-model)
- [Proposed execution policy](#proposed-execution-policy)
- [Cache affinity](#cache-affinity)
- [OAuth maintenance](#oauth-maintenance)
- [Operational additions and exclusions](#operational-additions-and-exclusions)
- [Candidate implementation slices](#candidate-implementation-slices)
- [Review decisions](#review-decisions)
- [Verification and acceptance](#verification-and-acceptance)
- [Reference observations](#reference-observations)

## User requirements

1. **Several named accounts per Provider.** A Provider such as OpenAI must support multiple named API keys and multiple named OAuth2 registrations together. Account fallback can distinguish exhausted credit, rate limits and other failures.
2. **Provider-owned recovery configuration.** Configure retries before fallback by failure scenario. The discussed values, such as five retries for 5xx, two for 419 and one for 401, are examples to refine, not selected defaults or universal classifications.
3. **API-key endpoint override.** API-key accounts inherit the Provider's default `base_url` unless they supply their own, including a deployment-specific API prefix. OAuth accounts never override it (user decision, 2026-10-30).
4. **Global OAuth maintenance settings.** Configure the check interval and refresh-ahead window globally, without a separate timing configuration for every Provider.
5. **Cache-first execution.** Keep the serving account through recoverable failures where bounded retries can help. Switch promptly for failures that cannot recover within the request, such as confirmed exhausted credit. Repeated temporary errors may eventually justify switching.
6. **Incremental delivery.** Runtime key addition is optional; restart-based activation is acceptable. Assume accounts of a Provider have the same model capabilities; do not add capability-based routing.
7. **Maintainability and simplicity.** Prefer one authoritative owner per fact, deterministic behavior and a small configuration surface. Reference projects supply mechanisms and counterexamples, not a feature checklist.
8. **No implicit switches beyond API keys.** By default, automatic switching between OAuth registrations of any profile (SIWC, Grok and others), fallback between OAuth and API-key accounts, and fallback across Providers are disabled; the operator can enable each explicitly. Only switching between API-key accounts of one Provider follows recovery policy without an opt-in. MorphieCore does not manage billing, and its usage accounting counts tokens only (user decisions, 2026-10-30).
9. **Requirements before rewrite.** The current implementation deviates substantially from these requirements. Complete the requirements first, then rewrite account binding and execution policy instead of incrementally reshaping the current path (user decision, 2026-10-30).

An account name identifies a configured execution identity. Two keys are not necessarily independent billing accounts or rate-limit domains. Equal model capabilities also do not establish shared caches, interchangeable OAuth grants or portable private replay state.

## Current implementation and rewrite boundary

The current code is evidence of existing mechanisms and protections, not the target design. It separates secret persistence, pool configuration, startup binding and runtime routing:

| Owner | Current behavior | Rewrite boundary |
|---|---|---|
| [Provider catalog](../src/provider/catalog.rs) and [metadata](../src/provider/providers.json) | Compile-time trusted origin, operation paths, one credential kind/profile per Provider with per-endpoint header placement, and default pool | Keep compile-time default origins and paths; declare header placement per credential kind; admit API-key and OAuth accounts under one Provider |
| [Credential pools](../src/credential/pool.rs) | Ordered references plus fallback/attempt policy; separate pool provisioning; SIWC pools limited to one member | Replace with the Provider's ordered accounts; generalize the SIWC restriction into the default OAuth switching policy |
| [Startup model assembly](../src/gateway/startup/models.rs) and [binding](../src/gateway/startup/bind.rs) | Expands endpoint/pool combinations into bounded candidates; replay/cache scopes hash provenance, origin, path and model | Bind explicit account identities and effective targets without copying secret state; keep the scope inputs |
| [Route policy](../src/topology/route.rs) and [model routes](../src/gateway/startup/models.rs) | Declaring more than one route candidate implicitly enables before-commit fallback; a single-account OAuth binding disables it | Cross-Provider fallback only through an explicit route opt-in |
| [Executor health and affinity](../src/gateway/routing/executors.rs) | Conversation affinity, three-scope cooling, generation fencing, single foreground probe, `Retry-After` cooling | Retain these contracts; see [Cache affinity](#cache-affinity) |
| [Execution chain](../src/gateway/routing/chain.rs) and [fallback](../src/execution/fallback.rs) | One send per candidate, forward-only advancement; pool attempts counted per endpoint | Replace with one owner for same-account retries, account switches and Provider switches |
| [Failure classification](../src/protocol/failure.rs) | One status table plus global quota substrings shared by all Providers | Classify per Provider contract into a finite fact vocabulary |
| [Credential access](../src/credential/access.rs), [API keys](../src/credential/api_key.rs) and [renewal](../src/gateway/renewal.rs) | Pinned identities, request-time borrowing, automatic OAuth maintenance with fixed 15 s scan, 120 s refresh-ahead and 90 s renewal timeout | Reuse lifecycle protections; make global timing configurable |

Preserve the existing secure store, OAuth drivers, verified identities, atomic publication/recovery, cancellation, trusted target selection and publication freeze where they fit. This is a rewrite of account binding and execution policy, not a replacement of the semantic IR, codecs or transport stack.

Current Generation, manual upstream diagnostics and standalone tasks have different pool rules. The recommended first runtime scope is Generation; the exact rollout boundary remains a [review decision](#review-decisions). Do not silently enable account fallback for frozen standalone tasks.

### Review inputs to retain

These existing issues are inputs to the rewrite, not independently scheduled repair work:

- **Native failed Responses are recorded as healthy completions.** Source-traced on 2026-10-30, not reproduced: when the client protocol carries failed Responses, [intake](../src/gateway/generation/intake.rs) returns success for a valid failed task, and [exchange](../src/gateway/generation/exchange.rs) then calls `Lease::completed`, clearing cooling in all three scopes and committing conversation affinity to the failing account. Separate upstream task outcome, delivery completion, health and affinity decisions.
- **Quota classification is global.** The quota substring list matches any 4xx body text, including the broad `billing`, and does not recognize SIWC's `subscription_sharing_usage_limit_exceeded`, which therefore classifies as a rate limit.
- **Timeouts rotate credentials.** `Timeout` currently permits credential advancement, although a timed-out dispatch may already have consumed upstream work.
- **First OAuth publication near the 64-namespace boundary** writes a pending marker before enforcing capacity. An isolated CLI reproduction showed rejected creation at 63 namespaces and failed global pool enumeration after an attempted new namespace at 64. Preserve publication recovery while correcting capacity accounting if that storage path changes.
- **Pool provisioning checks local references before Provider compatibility**, which is only checked at startup. Offline validation should report incompatibility before activation.
- **`fallback: false` limits within-request attempts**, not subsequent health-based selection.

Revalidate these paths against the implementation when the rewrite starts. Existing mapping or routing expectations are not an oracle for newly selected behavior.

## Proposed account and configuration model

Use three operator-facing concepts:

```text
Provider
  catalog facts: default origin, operation paths, header placement per credential kind
  recovery policy, including explicit switching opt-ins
  ordered named accounts
    credential: API key, or OAuth registration of a profile
    base_url override (API-key accounts only)
    enabled state

Model route
  ordered Provider / existing model-binding candidates
  cross-Provider fallback opt-in (default off)
  request-wide execution bounds
```

This is a conceptual model, not a configuration schema or an implemented CLI.

### Ownership and defaults

- Keep secrets in the existing managed credential store. Account configuration references them; it does not duplicate tokens, expiry or verified principal metadata.
- Use stable configured account names, not file ordering, startup vector indexes or token values, as operator identities. Credential provenance still owns the security identity.
- Let declaration order select among new, unbound conversations. Do not add separate priority, weight and ordering fields.
- By default, the eligible set is the Provider's enabled accounts, narrowed by the [switching boundaries](#switching-boundaries). Defer named sub-pools and account subsets until a concrete isolation requirement exists.
- Introduce no new configuration-precedence stack. Recovery and account-switching opt-ins belong to Provider policy, endpoint override and enablement to accounts, route order, total bounds and the cross-Provider opt-in to the route, and renewal timing to global service settings.

### Credential kind and header placement

The account owns its credential kind: API key, or OAuth registration of a named profile. The Provider and its endpoints own how each kind is placed on the wire and which fixed request controls it needs. The two cannot share one field: OpenCode Go sends the same API key as a bearer token to Chat Completions and as `x-api-key` to Messages, while SIWC adds profile-specific request controls.

Validate each account's credential kind against the kinds its Provider declares. Handle authentication differences in the Provider/adapter boundary, not a capability-scoring router.

### Targets and trust

- Default origins and operation paths remain compile-time catalog facts.
- An API-key account may override `base_url`. The override moves origin trust into operator configuration, so it must pass the same rules as catalog origins (HTTPS, no userinfo, query or fragment) and normalize origin and API prefix once before the existing operation path contracts apply.
- OAuth accounts reject an override. Their tokens go only to the origin of the profile's authorization-resource contract.
- Validate targets before borrowing secrets. Business requests cannot change origins, credentials, auth headers or proxies.
- Record the override trust rule as an Architecture decision when the design is approved.

### OAuth registration identity

Several OAuth registrations of one profile may belong to the same upstream user; SIWC documents that email may be identical across registrations and that usage limits are per ChatGPT user across apps. Provenance currently includes the issued client, so two registrations of one user look like independent accounts. Once OAuth switching is enabled, switching between them cannot escape a shared limit. Recommended: reject two registrations with the same verified subject under one Provider.

Exact field names, file ownership and provisioning commands remain open. Replacing the pool schema follows the project's current-schema-only rule: no silent migration, historical aliases or parallel legacy execution path. Reuse existing credential records where compatible; an incompatible storage change needs an explicit operator procedure.

## Proposed execution policy

One execution owner should control sends, waits, refresh recovery and advancement. Avoid nested transport, account and Provider retry wrappers with independent counters.

```text
Select the conversation's healthy bound account, otherwise the first eligible account
  -> dispatch
  -> complete, wait and retry, refresh and retry, advance, or stop
  -> try another account only within the switching boundaries and recovery policy
  -> try another route candidate only when the route enables cross-Provider fallback
```

### Switching boundaries

These defaults follow [requirement 8](#user-requirements):

| Switch | Default | Explicit opt-in |
|---|---|---|
| Between API-key accounts of one Provider | Allowed by recovery policy | None needed |
| Between OAuth registrations of one Provider, for every profile | Disabled | Provider policy flag |
| Between OAuth and API-key accounts of one Provider, in either direction | Disabled | Provider policy flag |
| Between route candidates of different Providers | Disabled | Route policy flag |

With switching disabled, the account selected for a new conversation is the only account that can serve it, and failures end after same-account recovery. Declaring several route candidates no longer enables fallback by itself. An opt-in enables only its own row.

### Error facts and policy

Use a finite error vocabulary rather than a user-programmable rule language. Provider adapters classify HTTP status, structured error codes and in-band failed outcomes into normalized facts, including any known failure scope and retry delay. Execution applies the configured policy.

| Scenario | Recommended starting behavior, not selected policy |
|---|---|
| Connection establishment failure or temporary server error | Retry the same account with bounded backoff; advance after exhaustion |
| Short-lived rate limit | Respect applicable `Retry-After`; retain the account within a finite wait budget |
| Confirmed exhausted credit or quota unavailable for this request | Skip ineffective same-account retries; isolate at the known scope and consider fallback within the switching boundaries |
| SIWC `subscription_sharing_usage_limit_exceeded` (429) | Pause plan-based requests on that registration; infer no reset time and no plan-wide exhaustion. Another registration only when OAuth switching is enabled |
| Expired OAuth access | Per profile contract: at most one coordinated renewal recovery before resending on the same verified identity |
| SIWC `subscription_sharing_invalid_user` (401) or a terminal refresh error | Require reauthorization; no refresh loop |
| Invalid/revoked API key | Mark unavailable and expose the recovery reason; retry/fallback semantics need selection |
| HTTP 419 or another Provider-specific status | Classify from that Provider's contract; do not universally treat it as token expiry |
| Timeout after dispatch | Do not resend by default; the upstream may have consumed work. Resend eligibility needs selection |
| Invalid request or context overflow | Return the failure rather than rotate equivalent accounts |
| Native failed outcome before publication | Classify like the equivalent HTTP failure; never a healthy completion |
| Failure after downstream publication | Finish with an honest failure; never regenerate and splice another answer |

Error labels, default counts and exact 401/403/419/429 behavior remain unresolved. Do not infer independent account limits from an unqualified 429. Do not treat every 5xx as transient or every 4xx as permanent. Arbitrary body regexes, JSONPath predicates, scripting and request-supplied retry overrides are outside this proposal.

### Accounting and bounds

- Recommended terminology: `N` retries means up to `N` additional dispatches after the initial one. The user must confirm this convention.
- Recommended counting: per request, count retries per account and error class cumulatively, plus one request-wide attempt ceiling. A class change does not reset either. Example: with five 5xx retries configured, the sequence 5xx, 5xx, 429, 5xx leaves two 5xx retries for that account.
- Keep actual sends, same-account retries, account switches and Provider switches distinguishable.
- Enforce a request-wide deadline across all accounts, Providers, backoff waits and recovery work.
- Separate within-request retry counts from cross-request health/cooldown state. Start with explicit bounded cooldown behavior, not rolling failure percentages or adaptive load balancing.
- Preserve cancellation during borrowing, waiting, refresh coordination, upstream I/O and delivery. No hidden SDK or transport retries.
- Retain publication freeze and semantic terminal honesty. Delivering a native failed response does not authorize healthy recovery or a successful affinity commit.

### Downstream retry contract

Clients retry on their own: pi's agent loop retries 429 and 5xx three times by default, and OpenCode retries rate limits and provider failures unless the response carries `x-should-retry: false`. Gateway retries multiply with these; five gateway retries under pi's default can produce 24 upstream sends for one turn.

Recommended: after the gateway exhausts its own recovery for a failure, answer with `x-should-retry: false`; while the failure is a cooldown the client may outwait, answer with `Retry-After` from the cooling state. Both are safe-to-ignore headers. Verify the actual effect on currently installed pi and OpenCode during acceptance.

## Cache affinity

The [current executor](../src/gateway/routing/executors.rs) already implements the intended affinity semantics; the rewrite retains them as contracts:

- Conversation affinity keyed by model and declared conversation, bounded by idle expiry and capacity.
- Preference for the conversation's healthy serving target over recovered higher-priority accounts; no forced failback.
- Replacement binding committed only after successful completion.
- Credential, target and pool cooling scopes, generation fencing against stale completions, and a single foreground recovery probe.

Conversation carriers come from [ingress admission](../src/gateway/ingress/admission.rs): `X-MorphieCore-Conversation-Id`, then OpenCode `X-Session-Id`, pi `session_id`, Codex `session-id` and pi `x-session-affinity`. Do not add content-derived session guessing or let one caller's session label select another caller's identity. Without a carrier, compiled order applies, so traffic returns to a recovered first account.

Changes for the rewrite:

- Affinity identifies the effective execution target: credential provenance, account endpoint and model/protocol scope. Normal OAuth token rotation preserves it; replacement API keys or changed targets cannot inherit it.
- Fix the success condition: native failed outcomes, incomplete responses and tool turns need an explicit rule; failed outcomes never commit.
- Switching boundaries also bound affinity: a replacement binding can only move within the switches the Provider permits.

Do not base routing on inferred cache value, guessed cache lifetime or missing cache-usage fields. Required private replay state still follows the accepted replay trust rules.

### Optional restart continuity

Bounded affinity persistence is the preferred follow-up before hot reload. Store only private hashed conversation keys, target identity fingerprints and necessary expiry/success metadata, never prompts or credentials. Restore only entries whose configuration and credential identities still match, and bound file size, entry count and age.

Persistence is not selected for the first rewrite. Corrupt-file handling, stale configuration, privacy, shutdown behavior and whether persistence is opt-in must be decided before implementation. It does not make gateway-owned Responses history durable.

## OAuth maintenance

Keep one global maintenance policy and the existing account locks and lifecycle owner. Each rotation uses its own registered client and latest refresh material; expiry and principal facts come from the credential driver's verified contract.

Recommended: keep pure polling and validate its configuration. The due check is local (account lock and store read, no network), so frequent scans are cheap. Require `refresh_ahead >= scan_interval + renewal_timeout`; the current 15 s scan, 90 s timeout and 120 s window satisfy it. A five-minute scan with a three-minute window fails it: a scan seeing four minutes left skips renewal, and the next scan occurs one minute after expiry. A deadline-aware timer is the alternative only if polling cost becomes a concrete problem.

The maintenance loop must:

- Check on startup, serialize renewal for a session and recheck identity/expiry under the lock.
- Bound renewal time and account for failed-renewal backoff.
- Persist replacement material safely and never reuse a potentially consumed refresh token after uncertain failure.
- Preserve account identity across normal rotation, and surface explicit reauthorization or persistence-recovery states.
- Treat unknown expiry as unknown; do not perform paid inference to test validity.
- Keep ordinary credential borrowing network-free. Any selected request-triggered 401 recovery calls the same maintenance owner instead of creating a second refresh implementation, and only where the profile's contract makes refresh the right recovery.

Numeric defaults and request-triggered renewal are review decisions. Global timing does not remove Provider-specific token verification or rotation requirements.

## Operational additions and exclusions

### Recommended additions

| Addition | Value and scope |
|---|---|
| Offline configuration validation | Check references, Provider/credential-kind compatibility, override trust rules, duplicate names and OAuth subjects, switching opt-ins and budgets before activation. Use the real loader; no login, refresh, paid probe or automatic repair |
| Read-only account and route explanation | Show normalized reasons, applicable attempt budget, switching boundary, cooldown/renewal state and affinity action. Queries must not change selection, extend TTL or trigger renewal |
| Explicit account diagnostics | Extend the existing upstream CLI to address a named account with bounded requests and no hidden retries; no automatic sweep of all accounts |
| Bounded affinity persistence | Preserve cache affinity through restart-based configuration changes; follow the separate persistence review above |

Reuse existing CLI and diagnostic owners. Ordinary reports remain body-free and redacted, with bounded attempt breadcrumbs; do not expose upstream origins or credential locators to clients. Usage reporting counts tokens per account and attempt; it computes no prices or balances. This proposal does not activate the deferred diagnostic aggregation/public-extension work in the [roadmap](roadmap.md#observability-follow-up).

### Deferred unless a concrete need emerges

- Runtime account addition, file watching and configuration-management APIs.
- Account draining, per-account concurrency queues and shared quota groups.
- Active balance/quota polling; first use authoritative retry/reset information already observed in responses. Unknown quota is not zero.
- Persisted health across restart, distributed affinity or Redis coordination.
- Per-account proxy overrides beyond existing proxy configuration.

### Excluded from the proposed core

- Billing management, price tables and cost accounting.
- Capability-, cost-, latency- or usage-based routing; weights and round-robin.
- Per-account/model/request recovery overrides and nested named fallback groups.
- Arbitrary error-rule DSLs and pluggable scheduling strategies.
- Automatic discovery, import or modification of third-party Agent credentials.
- Broad active account probes, a new dashboard or a second telemetry pipeline.
- New public APIs, model substitutions, semantic mappings or standalone-task activation justified only by this rewrite.

Deferred items do not need placeholder interfaces or configuration fields.

## Candidate implementation slices

These are an illustrative sequence for the later rewrite, not scheduled work. Before any slice starts, the user must approve the requirements, resolve the slice's decisions and authorize the routing scope in [Roadmap](roadmap.md).

| Slice | Proposed content | Check that would close it |
|---|---|---|
| A: thin execution path | One Provider, two named API-key accounts, one temporary failure category, explicit same-account retries and deterministic fallback; reuse storage/adapters | A synthetic request retries account A exactly as selected, switches to B only when permitted, and subsequent conversation turns remain on successful B |
| B: required account variants | API Key/OAuth coexistence with default-disabled OAuth, cross-kind and cross-Provider switching, API-key target overrides, global renewal settings, the selected error table and the downstream retry contract | Synthetic authorities and upstreams verify identity/target isolation, switching boundaries, rotation, timing, bounded recovery and errors; no hidden extra sends |
| C: operation and continuity | Finish configuration checks and read-only explanations; select explicit account diagnostics and optional affinity persistence | Invalid configuration fails before activation; inspection is side-effect-free; restart restores only compatible, unexpired affinity |

Minimum startup validation and useful errors belong in every slice, not only C. Retire the old runtime path as its replacement lands rather than maintaining two long-lived engines.

Prefer current project helpers, Tokio timers and the existing OAuth/HTTP dependencies. No new library or reference-project dependency is selected.

## Review decisions

### Decided

| Decision | Date | Outcome |
|---|---|---|
| Sequencing | 2026-10-30 | Complete requirements first; rewrite account binding and execution policy afterwards |
| Switching defaults | 2026-10-30 | OAuth registration switching for every profile, OAuth/API-key fallback and cross-Provider fallback are disabled by default and enabled explicitly; API-key switching within a Provider follows recovery policy; MorphieCore manages no billing and counts tokens only |
| Endpoint override | 2026-10-30 | API-key accounts may override `base_url`; OAuth accounts may not |

### Open

A recommendation below is not a default configuration or an accepted architecture decision.

| Decision | Recommendation or alternatives to resolve |
|---|---|
| Account/configuration ownership | Unified ordered named accounts with managed credential references; confirm file/schema ownership, stable naming and replacement of explicit pools |
| Opt-in granularity | Recommended: two Provider flags (OAuth switching, cross-kind fallback) and one route flag (cross-Provider fallback) |
| Unrepresentable candidates | Whether pre-dispatch skipping of candidates that cannot represent a request still selects a later Provider while cross-Provider fallback is off. Recommended: keep it, since nothing was dispatched, and report the selection |
| OAuth subject uniqueness | Recommended: reject duplicate verified subjects under one Provider; alternative is a shared quota domain |
| Activation scope | Generation first; separately decide changes to diagnostics and frozen standalone tasks, and qualify each newly activated authentication channel |
| Recovery table | Confirm status/error classification, counts, delays, exhausted-policy action and failure scope, especially 401, 403, 419, ambiguous 429 and post-dispatch timeout |
| Accounting | Confirm additional-retry terminology, cumulative per-class counting, local resolution/refresh budget accounting, total attempt limit and maximum wait |
| Downstream retry contract | Confirm `x-should-retry: false` after exhausted gateway recovery and `Retry-After` from cooling |
| OAuth recovery | Decide per profile whether 401 triggers one coordinated refresh or reauthorization |
| Quota isolation | Decide handling when account/project/Provider scope or reset time is unknown; shared quota groups remain deferred |
| Affinity | Confirm the success rule for failed, incomplete and tool turns, and carrier-less behavior |
| Renewal schedule | Recommended: validated pure polling; choose global defaults and failure-backoff bounds |
| Persistence and operations | Select whether/when to persist affinity, its failure policy, and the minimum CLI/diagnostic surfaces |
| Rollout | Select slices, breaking configuration changes and operator provisioning steps; runtime mutation remains optional |

After approval, record accepted contracts in Architecture and scheduled slices/gates in Roadmap, linking rather than duplicating them here. Update Usage only when behavior is implemented.

## Verification and acceptance

Extend existing credential, gateway-routing, bootstrap and transport tests; no new framework is proposed. Test externally meaningful behavior, not a target test count or configuration snapshots.

- **Identity and targets:** mixed credential kinds, header placement per kind, named-account references, API-key base URL inheritance/prefix handling, rejected OAuth overrides, duplicate OAuth subjects, origin restrictions and replay/cache isolation.
- **Switching boundaries:** no OAuth, cross-kind or cross-Provider switch by default, several route candidates not enabling fallback by themselves, and each opt-in enabling only its own switch.
- **Retry accounting:** selected same-account counts, cumulative mixed error sequences, account/Provider transitions, exhausted global budgets, applicable `Retry-After`, downstream retry headers, cancellation during waits and no retry amplification.
- **Publication and outcomes:** failures before/after publication, native failed Responses never committing health or affinity, incomplete terminals, strict EOF and distinction between delivery completion and health success.
- **Affinity and health:** stable account selection, successful replacement without forced failback, concurrent stale completions, scoped cooldown and exclusive recovery probes.
- **OAuth lifecycle:** startup/due scheduling, rejected polling configurations, unknown expiry, serialized renewal, replaced tokens, failed/cancelled rotation, identity changes and explicit reauthorization.
- **Configuration/storage:** early incompatibility errors, duplicate/unknown fields, unsafe files, capacity-boundary publication, atomicity and any explicit configuration transition.
- **Operations:** status queries make no network requests or state changes; diagnostics are bounded/redacted; persisted affinity, if selected, rejects stale identities and respects privacy/resource limits.

Use the [full offline baseline](development.md#offline-baseline) and applicable gates before publishing implementation slices. Official contracts and selected source assets remain the protocol oracle. Run SDK and current installed-Agent acceptance separately for changed paths.

Real multi-account failover needs distinct authorized accounts and a bounded, reproducible failure scenario. Fix targets, request count, output bounds and timeout before a run, with no hidden retries and project-loader credential access. Synthetic success does not establish account entitlement, shared quota scope or cache hit rates.

## Reference observations

Reviewed **2026-10-30**. The revisions below are public main-branch observations, not promoted dependency pins, selected stable releases or executed compatibility evidence. Existing [source guides](references.md#source-guides) remain the broader reference entry point.

| Project and revision | Relevant observation | Proposed use and boundary |
|---|---|---|
| [BerriAI/litellm, `ccc56d27adeeece2a1069bde587c76486ba9ab39`](https://github.com/BerriAI/litellm/blob/ccc56d27adeeece2a1069bde587c76486ba9ab39/litellm/router.py) | Retry can reselect a deployment; error policies, cooldown and fallback are distinct. [Affinity code](https://github.com/BerriAI/litellm/blob/ccc56d27adeeece2a1069bde587c76486ba9ab39/litellm/router_utils/pre_call_checks/deployment_affinity_check.py) supports successful replacement handling | Borrow policy separation and bounded explanation, not deployment reselection as the meaning of same-account retry or the load-balancing machinery |
| [yetone/magpie, `dca3e2d716fe21bbb3e239f769a8e7861b135c01`](https://github.com/yetone/magpie/blob/dca3e2d716fe21bbb3e239f769a8e7861b135c01/internal/gateway/affinity.go) | Persists the serving account across restarts; also uses cache reports, turn boundaries and timing heuristics | Consider bounded persistence, not inferred-cache routing. Its [account integration](https://github.com/yetone/magpie/blob/dca3e2d716fe21bbb3e239f769a8e7861b135c01/docs/subsystems/providers-accounts.md) includes third-party Agent stores, which this project does not adopt |
| [router-for-me/CLIProxyAPI, `90c447d3a8727d115aa7552ec93ce023736ed492`](https://github.com/router-for-me/CLIProxyAPI/blob/90c447d3a8727d115aa7552ec93ce023736ed492/config.example.yaml) | Retry settings describe additional credential rounds, not repeated sends on one account; [selection](https://github.com/router-for-me/CLIProxyAPI/blob/90c447d3a8727d115aa7552ec93ce023736ed492/sdk/cliproxy/auth/selector.go) separates fill-first, affinity and non-mutating lookup | Borrow deterministic selection and inspection. Do not copy round semantics, Auth-ID ordering, override layers or round-robin. No fork behavior is attributed to this upstream |
| [earendil-works/pi, `4954d760504c11df9b2cbdc040346107b6a8153d`](https://github.com/earendil-works/pi/blob/4954d760504c11df9b2cbdc040346107b6a8153d/packages/coding-agent/src/core/settings-defaults.ts) | Agent retry enabled by default with three retries; [retryable failures](https://github.com/earendil-works/pi/blob/4954d760504c11df9b2cbdc040346107b6a8153d/packages/ai/src/utils/retry.ts) include 429 and 5xx | Input for the downstream retry contract; installed-Agent behavior still needs acceptance |
| [anomalyco/opencode, `e5a557efeee02c94486d391afc48f24a49ebc9a5`](https://github.com/anomalyco/opencode/blob/e5a557efeee02c94486d391afc48f24a49ebc9a5/packages/ai/src/provider-error.ts) | `isRetryable` honors `x-should-retry` before its own classification; [session retry](https://github.com/anomalyco/opencode/blob/e5a557efeee02c94486d391afc48f24a49ebc9a5/packages/core/src/session/runner/retry.ts) caps `retry-after` at 15 minutes | Input for the downstream retry contract |

Primary protocol inputs:

- [OpenAI accounts and sessions](https://developers.openai.com/siwc/token-sharing-open-source/profiles-and-sessions): apps manage several isolated registrations, each with its own issued client and credentials; email may be identical across registrations; the five-hour usage limit is shared across all apps of a ChatGPT user; refresh is serialized per session with the latest replacement. The page does not describe automatic account switching.
- [OpenAI errors and recovery](https://developers.openai.com/siwc/token-sharing-open-source/errors-and-recovery): `subscription_sharing_usage_limit_exceeded` (429) pauses new plan-based requests without inferring a reset time; `subscription_sharing_invalid_user` (401) leads to sign-in again after confirmed revocation or a terminal refresh error; OpenAI does not silently switch a request to another billing path.
- [OpenAI SDK retry documentation](https://github.com/openai/openai-python#retries): automatic SDK retries can multiply gateway attempts. Characterize actual client transmissions separately.

The SIWC observations come from a summarized page fetch on 2026-10-30. Recheck the verbatim official contract and relevant upstream revision before implementing a decision that depends on them. No research result here relaxes credential ownership, trust, publication or terminal-honesty boundaries.
