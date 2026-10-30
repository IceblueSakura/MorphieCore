# Provider account redesign plan

**Status: frozen for human review, 2026-10-30. No implementation design is approved.**

This document records the user's requirements and the recommendations discussed before the freeze. Requirements describe the desired outcome; recommendations, example values and implementation slices remain proposals. Do not expand the design, implement it, change credentials or activate bindings from this document without the user reopening the work.

[Architecture](architecture.md) remains the authority for accepted contracts. [Roadmap](roadmap.md#temporary-routing-freeze) retains the current routing freeze and owns scheduling. This separately requested plan owns the frozen account-redesign proposal; it does not reopen routing through the active IR/codec migration. [Usage](usage.md#prepare-credentials) describes the currently implemented configuration, not the shapes discussed here.

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
3. **Optional account endpoint override.** Accounts inherit a Provider's default `base_url` unless they supply their own, including a deployment-specific API prefix.
4. **Global OAuth maintenance settings.** Configure the check interval and refresh-ahead window globally, without a separate timing configuration for every Provider.
5. **Cache-first execution.** Keep the serving account through recoverable failures where bounded retries can help. Switch promptly for failures that cannot recover within the request, such as confirmed exhausted credit. Repeated temporary errors may eventually justify switching.
6. **Incremental delivery.** Runtime key addition is optional; restart-based activation is acceptable. Assume accounts of a Provider have the same model capabilities; do not add capability-based routing.
7. **Maintainability and simplicity.** Prefer one authoritative owner per fact, deterministic behavior and a small configuration surface. Reference projects supply mechanisms and counterexamples, not a feature checklist.

An account name identifies a configured execution identity. Two keys are not necessarily independent billing accounts or rate-limit domains. Equal model capabilities also do not establish shared caches, interchangeable OAuth grants or portable private replay state.

## Current implementation and rewrite boundary

The reviewed implementation separates secret persistence, pool configuration, startup binding and runtime routing:

| Owner | Current behavior | Proposed change boundary |
|---|---|---|
| [Provider catalog](../src/provider/catalog.rs) and [metadata](../src/provider/providers.json) | A Provider fixes credential kind/profile, trusted origin, operation paths and default pool | Allow accounts using different supported authentication profiles under one Provider |
| [Credential pools](../src/credential/pool.rs) | Ordered references plus fallback/attempt policy; separate pool provisioning | Consider making the Provider's ordered accounts its default pool, without a second required membership list |
| [Startup model assembly](../src/gateway/startup/models.rs) and [binding](../src/gateway/startup/bind.rs) | Startup snapshots expand endpoint/pool combinations into bounded candidates | Bind explicit account identities and effective targets without copying secret state |
| [Execution chain](../src/gateway/routing/chain.rs) and [fallback](../src/execution/fallback.rs) | Bounded candidate advancement; pool attempts are counted per endpoint | Add explicit same-account retries and keep account/Provider transitions separately accountable |
| [Credential access](../src/credential/access.rs), [API keys](../src/credential/api_key.rs) and [renewal](../src/gateway/renewal.rs) | Pinned identities, request-time borrowing and automatic OAuth maintenance | Reuse lifecycle protections; make global timing configurable |

Preserve the existing secure store, OAuth drivers, verified identities, atomic publication/recovery, cancellation, trusted target selection and publication freeze where they fit. This is a scoped rewrite of account binding and execution policy, not a replacement of the semantic IR, codecs or transport stack.

Authentication variants may require different headers, paths or fixed request controls even when accounts have equal model capabilities. Handle those differences in the existing Provider/adapter boundaries, not a capability-scoring router.

Current Generation, manual upstream diagnostics and standalone tasks have different pool rules. The recommended first runtime scope is Generation; the exact rollout boundary remains a [review decision](#review-decisions). Do not silently enable account fallback for frozen standalone tasks.

### Review inputs to retain

The earlier review identified these existing issues; they are investigation inputs, not independently authorized repair work:

- Native failed Responses can complete delivery and then take the healthy-completion path. This was source-traced, not independently reproduced. Separate upstream task outcome, delivery completion, health and affinity decisions.
- First OAuth publication near the 64-namespace boundary writes a pending marker before enforcing capacity. An isolated CLI reproduction showed rejected creation at 63 namespaces and failed global pool enumeration after an attempted new namespace at 64. Preserve publication recovery while correcting capacity accounting if that storage path changes.
- Pool provisioning checks local references before Provider compatibility is checked at startup. The proposed offline validation should report incompatibility before activation.
- `fallback: false` currently limits within-request attempts, not subsequent health-based selection. Only the OpenAI SIWC profile currently has the special single-account restriction; the generic OAuth wording in Usage is broader than the code.

Revalidate these paths against the implementation at the start of an approved slice. Existing mapping or routing expectations are not an oracle for newly selected behavior.

## Proposed account and configuration model

Use three operator-facing concepts:

```text
Provider
  default target
  recovery policy
  ordered named accounts
    credential reference: API Key or OAuth registration
    optional base_url override
    enabled state

Model route
  ordered Provider / existing model-binding candidates
  request-wide execution bounds
```

This is a conceptual model, not a configuration schema or an implemented CLI.

### Ownership and defaults

- Keep secrets in the existing managed credential store. Account configuration references them; it does not duplicate tokens, expiry or verified principal metadata.
- Use stable configured account names, not file ordering, startup vector indexes or token values, as operator identities. Credential provenance still owns the security identity.
- Let declaration order select among new, unbound conversations. Do not add separate priority, weight and ordering fields.
- By default, use all enabled accounts of a Provider. Defer named sub-pools and account subsets until a concrete isolation requirement exists.
- Resolve `base_url` from the account override or Provider default. Normalize origin and API prefix once, then use the existing operation path contracts.
- Keep authentication kind/profile on the account binding and validate it against the Provider's supported authentication contracts.
- Validate targets before borrowing secrets. Business requests cannot change origins, credentials, auth headers or proxies; OAuth destinations must respect the selected authorization-resource contract.
- Introduce no new configuration-precedence stack. Recovery belongs to Provider policy, endpoint override and enablement to accounts, route order and total bounds to the route, and renewal timing to global service settings.

Exact field names, file ownership and provisioning commands remain open. If the old pool schema is replaced, follow the project's current-schema-only rule: no silent migration, historical aliases or permanent parallel legacy execution path. Existing credential records should be reused where compatible; an incompatible storage change needs an explicit operator procedure.

## Proposed execution policy

One execution owner should control sends, waits, refresh recovery and advancement. Avoid nested transport, account and Provider retry wrappers with independent counters.

```text
Select the conversation's healthy bound account, otherwise the first eligible account
  -> dispatch
  -> complete, wait and retry, refresh and retry, advance, or stop
  -> try another account only when the selected recovery policy permits
  -> try another route candidate only within the route policy
```

### Error facts and policy

Use a finite error vocabulary rather than a user-programmable rule language. Provider adapters classify HTTP status, structured error codes and in-band failed outcomes into normalized facts, including any known failure scope and retry delay. Execution applies the configured policy.

| Scenario | Recommended starting behavior, not selected policy |
|---|---|
| Connection establishment failure or temporary server error | Retry the same account with bounded backoff; advance after exhaustion |
| Short-lived rate limit | Respect applicable `Retry-After`; retain the account within a finite wait budget |
| Confirmed exhausted credit or quota unavailable for this request | Skip ineffective same-account retries; isolate at the known scope and consider fallback |
| Expired OAuth access | At most one coordinated renewal recovery before resending on the same verified identity |
| Invalid/revoked API key or OAuth registration | Mark unavailable and expose the recovery reason; retry/fallback semantics need selection |
| HTTP 419 or another Provider-specific status | Classify from that Provider's contract; do not universally treat it as token expiry |
| Invalid request or context overflow | Return the failure rather than rotate equivalent accounts |
| Failure after downstream publication | Finish with an honest failure; never regenerate and splice another answer |

Error labels, default counts and exact 401/403/419/429 behavior remain unresolved. Do not infer independent account limits from an unqualified 429. Do not treat every 5xx as transient or every 4xx as permanent. Arbitrary body regexes, JSONPath predicates, scripting and request-supplied retry overrides are outside this proposal.

### Accounting and bounds

- Recommended terminology: `N` retries means up to `N` additional dispatches after the initial one. The user must confirm this convention.
- Keep actual sends, same-account retries, account switches and Provider switches distinguishable.
- Enforce a request-wide attempt ceiling and deadline across all accounts, Providers, backoff waits and recovery work. Error-class changes must not reset that outer budget.
- Separate within-request retry counts from cross-request health/cooldown state. Start with explicit bounded cooldown behavior, not rolling failure percentages or adaptive load balancing.
- Preserve cancellation during borrowing, waiting, refresh coordination, upstream I/O and delivery. No hidden SDK or transport retries.
- A timeout after dispatch may have consumed upstream work or money. Select retry eligibility separately from connection failures; no general idempotency assumption is made.
- Retain publication freeze and semantic terminal honesty. Delivering a native failed response does not authorize healthy recovery or a successful affinity commit.

## Cache affinity

Recommend one behavior rather than a menu of session, turn, cost or cache-estimation modes:

1. Select new conversations deterministically from available configured accounts.
2. Prefer the conversation's healthy serving target over recovered higher-priority accounts.
3. Try bounded recovery on that account before advancing.
4. Commit a replacement binding only after the selected successful-completion condition.
5. Keep that successful replacement for subsequent turns. A recovered original account can serve new conversations without forcing existing ones back.

Use existing caller/conversation identity boundaries. Do not add new content-derived session guessing or let one caller's session label select another caller's identity. Preserve stale-result fencing so older in-flight work cannot undo a newer health or routing decision.

Affinity identifies the effective execution target, including credential provenance, endpoint and model/protocol scope. Normal OAuth token rotation preserves the verified account identity; replacement API keys or changed targets cannot inherit an incompatible scope. Required private replay state still follows the accepted replay trust rules.

Do not base routing on inferred cache value, guessed cache lifetime or missing cache-usage fields. Retain bounded idle expiry and capacity. The precise completion rule for incomplete responses, tool turns and native failed outcomes needs review.

### Optional restart continuity

Bounded affinity persistence is the preferred follow-up before hot reload. Store only private hashed conversation keys, target identity fingerprints and necessary expiry/success metadata, never prompts or credentials. Restore only entries whose configuration and credential identities still match, and bound file size, entry count and age.

Persistence is not selected for the first slice. Corrupt-file handling, stale configuration, privacy, shutdown behavior and whether persistence is opt-in must be decided before implementation. It does not make gateway-owned Responses history durable.

## OAuth maintenance

Keep one global maintenance policy and the existing account locks and lifecycle owner. Each rotation uses its own registered client and latest refresh material; expiry and principal facts come from the credential driver's verified contract.

The example of a five-minute scan with a three-minute refresh-ahead window can miss expiry: a scan seeing four minutes left skips renewal, and the next scan occurs one minute after expiry.

Two bounded approaches remain available:

- **Pure polling:** require the refresh-ahead window to cover the scan interval plus renewal/queueing allowance.
- **Recommended simple deadline-aware loop:** treat the configured interval as the maximum inspection interval and wake earlier for the nearest known renewal deadline. A bounded account scan and one timer are sufficient; no heap, distributed scheduler or task framework is proposed.

Either approach must:

- Check on startup, serialize renewal for a session and recheck identity/expiry under the lock.
- Bound renewal time and account for failed-renewal backoff when choosing the next wakeup.
- Persist replacement material safely and never reuse a potentially consumed refresh token after uncertain failure.
- Preserve account identity across normal rotation, and surface explicit reauthorization or persistence-recovery states.
- Treat unknown expiry as unknown; do not perform paid inference to test validity.
- Keep ordinary credential borrowing network-free. Any selected request-triggered 401 recovery calls the same maintenance owner instead of creating a second refresh implementation.

Numeric defaults, timer semantics and request-triggered renewal are review decisions. Global timing does not remove Provider-specific token verification or rotation requirements.

## Operational additions and exclusions

### Recommended additions

| Addition | Value and scope |
|---|---|
| Offline configuration validation | Check references, Provider/auth compatibility, normalized targets, duplicate names and budgets before activation. Use the real loader; no login, refresh, paid probe or automatic repair |
| Read-only account and route explanation | Show normalized reasons, applicable attempt budget, cooldown/renewal state and affinity action. Queries must not change selection, extend TTL or trigger renewal |
| Explicit account diagnostics | Extend the existing upstream CLI to address a named account with bounded requests and no hidden retries; no automatic sweep of all accounts |
| Bounded affinity persistence | Preserve cache affinity through restart-based configuration changes; follow the separate persistence review above |

Reuse existing CLI and diagnostic owners. Ordinary reports remain body-free and redacted, with bounded attempt breadcrumbs; do not expose upstream origins or credential locators to clients. This proposal does not activate the deferred diagnostic aggregation/public-extension work in the [roadmap](roadmap.md#observability-follow-up).

### Deferred unless a concrete need emerges

- Runtime account addition, file watching and configuration-management APIs.
- Account draining, per-account concurrency queues and shared quota groups.
- Active balance/quota polling; first use authoritative retry/reset information already observed in responses. Unknown quota is not zero.
- Persisted health across restart, distributed affinity or Redis coordination.
- Per-account proxy overrides beyond existing proxy configuration.

### Excluded from the proposed core

- Capability-, cost-, latency- or usage-based routing; weights and round-robin.
- Per-account/model/request recovery overrides and nested named fallback groups.
- Arbitrary error-rule DSLs and pluggable scheduling strategies.
- Automatic discovery, import or modification of third-party Agent credentials.
- Broad active account probes, a new dashboard or a second telemetry pipeline.
- New public APIs, model substitutions, semantic mappings or standalone-task activation justified only by this rewrite.

Deferred items do not need placeholder interfaces or configuration fields.

## Candidate implementation slices

These are an illustrative sequence for later selection, not scheduled work. Before any slice starts, the user must reopen design, resolve its relevant decisions and authorize the routing scope in [Roadmap](roadmap.md).

| Slice | Proposed content | Check that would close it |
|---|---|---|
| A: thin execution path | One Provider, two named accounts, one temporary failure category, explicit same-account retries and deterministic fallback; reuse storage/adapters | A synthetic request retries account A exactly as selected, switches to B only when permitted, and subsequent conversation turns remain on successful B |
| B: required account variants | API Key/OAuth coexistence, account target overrides, global renewal settings and the selected error table on the same execution path | Synthetic authorities and upstreams verify identity/target isolation, rotation, timing, bounded recovery and errors; no hidden extra sends |
| C: operation and continuity | Finish configuration checks and read-only explanations; select explicit account diagnostics and optional affinity persistence | Invalid configuration fails before activation; inspection is side-effect-free; restart restores only compatible, unexpired affinity |

Minimum startup validation and useful errors belong in every slice, not only C. The order can be adjusted after review if a dependency requires it. Retire the old runtime path as its replacement lands rather than maintaining two long-lived engines.

Prefer current project helpers, Tokio timers and the existing OAuth/HTTP dependencies. No new library or reference-project dependency is selected. Reconsider a dependency only for a concrete requirement the existing stack cannot meet.

## Review decisions

All entries below remain open. A recommendation in this document is not a default configuration or an accepted architecture decision.

| Decision | Recommendation or alternatives to resolve |
|---|---|
| Account/configuration ownership | Unified ordered named accounts with managed credential references; confirm file/schema ownership, stable naming and replacement of explicit pools |
| Activation scope | Generation first; separately decide changes to diagnostics and frozen standalone tasks, and qualify each newly activated authentication channel |
| Recovery table | Confirm status/error classification, counts, delays, exhausted-policy action and failure scope, especially 401, 403, 419 and ambiguous 429 |
| Accounting | Confirm additional-retry terminology, local resolution/refresh budget accounting, total attempt limit and maximum wait |
| OAuth recovery | Decide whether 401 triggers one coordinated refresh, when to mark reauthorization, and whether/when another account may be tried |
| Quota isolation | Decide handling when account/project/Provider scope or reset time is unknown; shared quota groups remain deferred |
| Affinity | Confirm success criteria, replacement commitment, no forced failback, idle expiry and behavior when no explicit conversation carrier is available |
| Renewal schedule | Select pure polling with validation or the maximum-interval/deadline-aware loop; choose global defaults and failure-backoff bounds |
| Persistence and operations | Select whether/when to persist affinity, its failure policy, and the minimum CLI/diagnostic surfaces |
| Rollout | Select slices, breaking configuration changes and operator provisioning steps; runtime mutation remains optional |

After approval, record accepted contracts in Architecture and scheduled slices/gates in Roadmap, linking rather than duplicating them here. Update Usage only when behavior is implemented. Reopen this frozen document explicitly before further design expansion.

## Verification and acceptance

Extend existing credential, gateway-routing, bootstrap and transport tests; no new framework is proposed. Test externally meaningful behavior, not a target test count or configuration snapshots.

- **Identity and targets:** mixed authentication bindings, named-account references, base URL inheritance/prefix handling, Provider/auth mismatches, origin restrictions and replay/cache isolation.
- **Retry accounting:** selected same-account counts, account/Provider transitions, mixed error sequences, exhausted global budgets, applicable `Retry-After`, cancellation during waits and no retry amplification.
- **Publication and outcomes:** failures before/after publication, valid native failed Responses, incomplete terminals, strict EOF and distinction between delivery completion and health success.
- **Affinity and health:** stable account selection, successful replacement without forced failback, concurrent stale completions, scoped cooldown and exclusive recovery probes.
- **OAuth lifecycle:** startup/due scheduling, timer-boundary cases, unknown expiry, serialized renewal, replaced tokens, failed/cancelled rotation, identity changes and explicit recovery.
- **Configuration/storage:** early incompatibility errors, duplicate/unknown fields, unsafe files, capacity-boundary publication, atomicity and any explicit configuration transition.
- **Operations:** status queries make no network requests or state changes; diagnostics are bounded/redacted; persisted affinity, if selected, rejects stale identities and respects privacy/resource limits.

Use the [full offline baseline](development.md#offline-baseline) and applicable gates before publishing implementation slices. Official contracts and selected source assets remain the protocol oracle. Run SDK and current installed-Agent acceptance separately for changed paths.

Real multi-account failover needs distinct authorized accounts and a bounded, reproducible failure scenario. Fix targets, request count, output bounds and timeout before a run, with no hidden retries and project-loader credential access. Synthetic success does not establish account entitlement, shared quota scope, cache hit rates or actual cost savings.

## Reference observations

Reviewed **2026-10-30**. The revisions below are public main-branch observations, not promoted dependency pins, selected stable releases or executed compatibility evidence. Existing [source guides](references.md#source-guides) remain the broader reference entry point.

| Project and revision | Relevant observation | Proposed use and boundary |
|---|---|---|
| [BerriAI/litellm, `ccc56d27adeeece2a1069bde587c76486ba9ab39`](https://github.com/BerriAI/litellm/blob/ccc56d27adeeece2a1069bde587c76486ba9ab39/litellm/router.py) | Retry can reselect a deployment; error policies, cooldown and fallback are distinct. [Affinity code](https://github.com/BerriAI/litellm/blob/ccc56d27adeeece2a1069bde587c76486ba9ab39/litellm/router_utils/pre_call_checks/deployment_affinity_check.py) supports successful replacement handling | Borrow policy separation and bounded explanation, not deployment reselection as the meaning of same-account retry or the load-balancing machinery |
| [yetone/magpie, `dca3e2d716fe21bbb3e239f769a8e7861b135c01`](https://github.com/yetone/magpie/blob/dca3e2d716fe21bbb3e239f769a8e7861b135c01/internal/gateway/affinity.go) | Persists the serving account across restarts; also uses cache reports, turn boundaries and timing heuristics | Consider bounded persistence, not inferred-cache routing. Its [account integration](https://github.com/yetone/magpie/blob/dca3e2d716fe21bbb3e239f769a8e7861b135c01/docs/subsystems/providers-accounts.md) includes third-party Agent stores, which this project does not adopt |
| [router-for-me/CLIProxyAPI, `90c447d3a8727d115aa7552ec93ce023736ed492`](https://github.com/router-for-me/CLIProxyAPI/blob/90c447d3a8727d115aa7552ec93ce023736ed492/config.example.yaml) | Retry settings describe additional credential rounds, not repeated sends on one account; [selection](https://github.com/router-for-me/CLIProxyAPI/blob/90c447d3a8727d115aa7552ec93ce023736ed492/sdk/cliproxy/auth/selector.go) separates fill-first, affinity and non-mutating lookup | Borrow deterministic selection and inspection. Do not copy round semantics, Auth-ID ordering, override layers or round-robin. No fork behavior is attributed to this upstream |

Primary protocol inputs:

- [OpenAI accounts and sessions](https://developers.openai.com/siwc/token-sharing-open-source/profiles-and-sessions): separate named registrations, matching issued clients and serialized refresh with latest replacement material. This does not by itself settle automatic quota-based account switching; qualify selected recovery against the [structured error contract](https://developers.openai.com/siwc/token-sharing-open-source/errors-and-recovery).
- [OpenAI SDK retry documentation](https://github.com/openai/openai-python#retries): automatic SDK retries can multiply gateway attempts. Characterize actual client transmissions separately.

Recheck the selected official recovery contract and relevant upstream revision before implementing a decision that depends on them. No research result here relaxes credential ownership, trust, publication or terminal-honesty boundaries.
