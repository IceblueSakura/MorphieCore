# Provider account redesign plan

**Status: design finalized on 2026-11-02. The user accepted a fully breaking account-system rewrite, fresh manual credential provisioning and subsequent implementation; [Roadmap](roadmap.md#account-rewrite-execution-order) owns current execution status. Production deployment remains separately approved.**

This document owns the finalized account-redesign specification. It supersedes the earlier multi-target retry/fallback draft and interim single-send choice with request-boundary account selection and at most one bounded same-target retry. Final review selected conservative retry eligibility, a 30 s recovery budget, merged health evidence, material-aware OAuth recovery and short advisory `Retry-After` without `x-should-retry`. File/field names and implementation structure remain engineering choices within this contract, not new behavior decisions.

[Architecture](architecture.md#routing-and-security-decisions) records the accepted decisions and links here for their detailed contract. [Roadmap](roadmap.md#account-rewrite-execution-order) owns execution order, dependencies and gates; its [routing boundary](roadmap.md#temporary-routing-freeze) separates this work from unrelated changes. [Usage](usage.md#prepare-credentials) describes currently implemented configuration. Development uses fresh test-owned state without modifying private credentials or deployed bindings.

## Contents

- [User requirements](#user-requirements)
- [Current implementation and rewrite boundary](#current-implementation-and-rewrite-boundary)
- [Account and configuration model](#account-and-configuration-model)
- [Execution policy](#execution-policy)
- [Cache affinity](#cache-affinity)
- [OAuth maintenance](#oauth-maintenance)
- [Operational additions and exclusions](#operational-additions-and-exclusions)
- [Review decisions](#review-decisions)
- [Design acceptance criteria](#design-acceptance-criteria)
- [Reference observations](#reference-observations)

## User requirements

1. **Several named accounts per Provider.** Each Provider supports several named accounts of its declared credential kinds. `openai` denotes OpenAI API-key access; `chatgpt` denotes SIWC OAuth access. They have separate model bindings and service contracts, rather than requiring mixed key/OAuth accounts under one OpenAI Provider. Account recovery can distinguish exhausted credit, rate limits and other failures (Provider split selected on 2026-11-02).
2. **Provider-owned recovery configuration.** Configure cooldown durations, backoff, reset-hint bounds and the short recovery-time budget (default 30 s). Permit at most one additional inference send for the selected network/server failure set before content publication, always to the same Provider, account, upstream model and effective endpoint. Do not restore per-class retry counters or within-request target switching. Provider classifiers replace operator-defined per-status rules (finalized on 2026-11-02).
3. **API-key endpoint override.** API-key accounts inherit the Provider's default `base_url` unless they supply their own, including a deployment-specific API prefix. OAuth accounts never override it (user decision, 2026-10-30).
4. **Global OAuth maintenance settings.** Configure the check interval and refresh-ahead window globally, without a separate timing configuration for every Provider.
5. **Cache-first execution.** Preserve cache affinity ahead of keeping every request available. The operator expects normally no more than three accounts and accepts returning an error instead of immediately switching after dispatch. Select accounts at request boundaries; a failed attempt may receive the one same-target retry, but never switches targets or moves the binding. Final recovery outcome updates health; only successful completion can commit affinity (selected on 2026-11-02).
6. **Incremental delivery.** Runtime key addition is optional; restart-based activation is acceptable. Assume accounts within one Provider have the same model capabilities; this does not equate the separate `openai` and `chatgpt` model catalogs. Do not add capability-scoring routing.
7. **Maintainability and simplicity.** Prefer one authoritative owner per fact, deterministic behavior and a small configuration surface. Reference projects supply mechanisms and counterexamples, not a feature checklist.
8. **Automatic selection across route candidates.** Every enabled, validly bound API-key or OAuth account in the current model route may serve a new request, including accounts of other Providers. Account and route configuration authorize these destinations and billing paths; there are no separate fallback switches or switch domains. Stored credentials alone, disabled accounts and Providers outside the route are not candidates. Health and affinity determine request-boundary selection; a failed dispatched request is not resent to another candidate. MorphieCore does not manage billing, and its usage accounting counts tokens only (user decisions, 2026-11-02, replacing the 2026-10-30 switching defaults and the earlier within-request fallback draft).
9. **Requirements before rewrite.** The current implementation deviates substantially from these requirements. Complete the requirements first, then rewrite account binding and execution policy instead of incrementally reshaping the current path (user decision, 2026-10-30).
10. **Operator-readable CLI.** Provisioning, validation and explanation commands print human-readable output by default and offer structured JSON as an option (user decision, 2026-10-30).
11. **Configurable policy parameters.** Wait, recovery-time and timeout budgets, cooldown backoff, reset-hint bounds, affinity cache lifetime and capacity, downstream retry-hint horizon and OAuth maintenance timing must be configurable at their authoritative owner. One additional inference attempt is the selected structural ceiling, not a generic retry-count matrix. Configuration cannot relax identity, publication, terminal-honesty or credential-lifecycle safety rules (2026-10-30 decision refined on 2026-11-02).
12. **Defaults and operator responsibility.** Retain the stated defaults for settings that remain in the selected design; removed retry allowances and send ceilings are not dormant configuration. Do not add policy-value range or cross-parameter relationship validation at this stage; the operator selects suitable values. Retain schema/type parsing and credential, identity and target security checks (2026-10-30 decision refined on 2026-11-02).
13. **Shared account binding, Generation-first recovery.** Replace credential loading, account references and trusted target binding across existing entrances together; initially integrate the new recovery execution policy in Generation. Existing diagnostics retain bounded named-account requests, and standalone tasks retain their existing execution capabilities without gaining multi-account recovery. Frozen features remain inactive; no parallel legacy pool path remains (user decision, 2026-11-02, refining the 2026-10-30 replacement scope).
14. **Fully breaking replacement.** No old application/credential schema, CLI spelling, account binding or pool execution compatibility is required. Do not add migrations, importers, aliases, compatibility facades or dual runtime paths. The user will provision fresh API keys manually; OAuth access uses a fresh managed login where needed. Preserve security and official protocol contracts, not historical configuration or stored-record formats. This does not authorize deleting or changing existing private credential stores (user decision, 2026-11-02).

An account name identifies a configured execution identity. Two keys are not necessarily independent billing accounts or rate-limit domains. Equal model capabilities also do not establish shared caches, interchangeable OAuth grants or portable private replay state.

## Current implementation and rewrite boundary

The current code is evidence of existing mechanisms and protections, not the target design. It separates secret persistence, pool configuration, startup binding and runtime routing:

| Owner | Current behavior | Rewrite boundary |
|---|---|---|
| [Provider catalog](../src/provider/catalog.rs) and [metadata](../src/provider/providers.json) | Compile-time trusted origin, operation paths, one credential kind/profile per Provider with per-endpoint header placement, and default pool; current `openai` is SIWC OAuth | Keep trusted catalog contracts and named accounts; split target `openai` API-key and `chatgpt` SIWC Providers, with independent model bindings |
| [Credential pools](../src/credential/pool.rs) | Ordered references plus fallback/attempt policy; separate pool provisioning; SIWC pools limited to one member | Replace with the Provider's ordered accounts; remove the single-member routing restriction without weakening session lifecycle protections |
| [Startup model assembly](../src/gateway/startup/models.rs) and [binding](../src/gateway/startup/bind.rs) | Expands endpoint/pool combinations into bounded candidates; replay/cache scopes hash provenance, origin, path and model | Bind explicit account identities and effective targets without copying secret state; keep the scope inputs |
| [Route policy](../src/topology/route.rs) and [model routes](../src/gateway/startup/models.rs) | Declaring more than one route candidate implicitly enables before-commit fallback; a single-account OAuth binding disables it | All eligible route candidates participate in request-boundary selection, without credential-kind or Provider opt-ins |
| [Executor health and affinity](../src/gateway/routing/executors.rs) | Conversation affinity, three-scope cooling, generation fencing, single foreground probe, `Retry-After` cooling | Retain these contracts; see [Cache affinity](#cache-affinity) |
| [Execution chain](../src/gateway/routing/chain.rs) and [fallback](../src/execution/fallback.rs) | One send per candidate, forward-only advancement; pool attempts counted per endpoint | Select the target once; permit at most one serial, bounded same-target retry. Remove within-request candidate advancement and per-target/class retry ledgers |
| [Failure classification](../src/protocol/failure.rs) | One status table plus global quota substrings shared by all Providers | Classify per Provider contract into a finite fact vocabulary |
| [Credential access](../src/credential/access.rs), [API keys](../src/credential/api_key.rs) and [renewal](../src/gateway/renewal.rs) | Pinned identities, request-time borrowing, automatic OAuth maintenance with fixed 15 s scan, 120 s refresh-ahead and 90 s renewal timeout | Reuse lifecycle protections; make global timing configurable |

Reuse secure storage mechanisms, OAuth drivers, identity verification, atomic publication/recovery, cancellation, trusted target selection and the publication fence where they fit. Existing credential record formats and persisted data are not compatibility requirements. This is a rewrite of account binding and execution policy, not a replacement of the semantic IR, codecs or transport stack.

Current Generation, manual upstream diagnostics and standalone tasks have different pool rules. The selected boundary replaces their shared credential loading and account/target binding together, while the new recovery executor initially serves Generation. Diagnostics retain explicit bounded account requests; standalone tasks retain their existing execution capabilities without new multi-account recovery. This design selection does not activate frozen features or approve implementation slices.

### Review inputs to retain

These existing issues are inputs to the rewrite, not independently scheduled repair work:

- **Native failed Responses are recorded as healthy completions.** Source-traced on 2026-10-30, not reproduced: when the client protocol carries failed Responses, [intake](../src/gateway/generation/intake.rs) returns success for a valid failed task, and [exchange](../src/gateway/generation/exchange.rs) then calls `Lease::completed`, clearing cooling in all three scopes and committing conversation affinity to the failing account. Separate upstream task outcome, delivery completion, health and affinity decisions.
- **Quota classification is global.** The quota substring list matches any 4xx body text, including the broad `billing`, and does not recognize SIWC's `subscription_sharing_usage_limit_exceeded`, which therefore classifies as a rate limit.
- **Timeouts rotate credentials.** `Timeout` currently permits credential advancement, although a timed-out dispatch may already have consumed upstream work.
- **First OAuth publication near the 64-namespace boundary** previously wrote a pending marker before enforcing capacity. The AR1 prerequisite now checks capacity before marker publication and recognizes an existing recovery namespace. Regressions cover admission at 63, clean overflow rejection at 64 and interrupted first-publication recovery at capacity; publication fault/quarantine assertions remain.
- **Pool provisioning checks local references before Provider compatibility**, which is only checked at startup. Offline validation should report incompatibility before activation.
- **`fallback: false` limits within-request attempts**, not subsequent health-based selection.

Revalidate these paths against the implementation when the rewrite starts. Existing mapping or routing expectations are not an oracle for newly selected behavior.

## Account and configuration model

Use three operator-facing concepts:

```text
Provider
  catalog facts: default origin, operation paths, header placement per credential kind
  recovery policy
  ordered named accounts
    credential: API key, or OAuth registration of a profile
    base_url override (API-key accounts only)
    enabled state

Model route
  ordered Provider / existing model-binding candidates
  automatic request-boundary selection across eligible candidates
  request-wide execution bounds
```

This defines configuration ownership and behavior; exact schema/CLI spelling belongs to the separately authorized rewrite.

### Ownership and defaults

- Keep secrets in the existing managed credential store under `$HOME/.local/share/morphiecore/credentials`. Account configuration references them by alias; it does not duplicate tokens, expiry or verified principal metadata.
- Keep account lists and Provider/route policy with the other private configuration under `$HOME/.local/share/morphiecore`, outside the credential store. Each Provider lists its ordered named accounts; explicit pools are removed.
- Use stable configured account names, not file ordering, startup vector indexes or token values, as operator identities. Credential provenance still owns the security identity.
- Let declaration order select among new, unbound conversations. Do not add separate priority, weight and ordering fields.
- Each route candidate expands the Provider's enabled, validly bound accounts in declaration order. Route candidate order precedes account order; credential kind adds no priority. Defer named sub-pools and account subsets until a concrete isolation requirement exists.
- Account `enabled` controls routing participation only. Configured Active OAuth sessions remain maintained when disabled or unreferenced by routes; logout explicitly ends the session. Credential invalidation, logout and quarantine remain independent safety vetoes, never overridden by account enablement.
- Introduce no new configuration-precedence stack. Recovery belongs to Provider policy, endpoint override and enablement to accounts, candidate order and total bounds to the route, and renewal timing to global service settings.

### Provider identity and model bindings

Selected on 2026-11-02:

| Provider | Access | Binding contract |
|---|---|---|
| `openai` | OpenAI API keys | OpenAI API model availability and supported operations |
| `chatgpt` | SIWC OAuth | ChatGPT account's SIWC model availability, fixed request controls and SIWC response interpretation |

Provider denotes the configured service access contract, not merely the vendor company or hostname. These Providers may share standard codec machinery without sharing model catalogs, request restrictions or credential provenance. Do not construct a Provider × credential-kind model matrix for this pair; select their protocol/adapter contracts through separate Provider bindings. Catalog defaults remain authoritative; account data cannot select arbitrary dialects.

A public model route includes only explicitly declared Provider/model bindings that preserve its public promise. Adding a `chatgpt` account does not automatically make every `openai` model available there, or authorize substituting another upstream model. Route-wide account eligibility remains, but only within those declared bindings.

The current catalog calls its SIWC Provider `openai`. New configuration declares API-key `openai` and SIWC `chatgpt` directly; old references are not reinterpreted or migrated. Internal OAuth profile identifiers remain protocol facts rather than compatibility aliases. Fresh credential provisioning establishes new managed provenance; do not inherit old affinity or private replay identity by name.

### Credential kind and header placement

The account owns its credential kind: API key, or OAuth registration of a named profile. The Provider and its endpoints own how each kind is placed on the wire and which fixed request controls it needs. The two cannot share one field: OpenCode Go sends the same API key as a bearer token to Chat Completions and as `x-api-key` to Messages, while SIWC adds profile-specific request controls.

Validate each account's credential kind against the kinds its Provider declares. Handle authentication differences in the Provider/adapter boundary, not a capability-scoring router.

### Targets and trust

- Default origins and operation paths remain compile-time catalog facts.
- An API-key account may override `base_url`. The override moves origin trust into operator configuration, so it must pass the same rules as catalog origins (HTTPS, no userinfo, query or fragment). Treat it as the complete API root and append the catalog-declared operation-relative suffix after normalizing the joining slash. For example, `https://relay.example/custom/v1` plus `responses` gives `https://relay.example/custom/v1/responses`; plus `models` gives `https://relay.example/custom/v1/models`. Do not guess or strip version prefixes. Without an override, retain each operation's full catalog target.
- OAuth accounts reject an override. Their tokens go only to the origin of the profile's authorization-resource contract.
- Validate targets before borrowing secrets. Business requests cannot change origins, credentials, auth headers or proxies.
- The override trust decision is recorded in [Architecture](architecture.md#routing-and-security-decisions); it remains unimplemented until the rewrite.

### OAuth registration identity

Several OAuth registrations of one profile may belong to the same upstream user; SIWC documents that email may be identical across registrations and that usage limits are per ChatGPT user across apps. Provenance currently includes the issued client, so two registrations of one user look like independent accounts. Switching between them cannot escape a shared limit. Configuration validation therefore rejects two registrations with the same verified subject under one Provider.

Each configured OAuth account has one managed login session. Do not support duplicate or concurrent logins for an already active account, multiple token sets under one account, or another alias for the same verified subject within the Provider. Normal refresh updates that session. Explicit reauthorization after invalidation or login after logout remains supported; it replaces the unusable or ended session rather than adding another one. These are MorphieCore-managed account rules, not a restriction on the user's sessions in unrelated applications.

Enforce verified-subject uniqueness atomically when publishing a login, including concurrent publishers, as well as during offline configuration validation. Issued-client provenance must not bypass subject uniqueness. Disabled, signed-out and reauthorization-required accounts retain the subject reservation; explicit account removal releases it. Reauthorization uses the existing account rather than creating another alias.

Exact file and field names are chosen with the rewrite. Replacing the pool schema follows the project's current-schema-only rule: no migration, historical aliases or parallel legacy execution path. Fresh provisioning replaces the earlier requirement to reuse compatible stored credential records.

### Configuration transition and CLI output

The rewrite is fully breaking. Operators create current-schema configuration and provision new accounts through the documented CLI. The user will enter new API keys later; OAuth accounts require the managed login flow. Startup fails closed on unsupported configuration/credential documents and directs the operator to fresh setup, without retaining a special old-schema parser.

Remove pool metadata from the new account/store model rather than extracting it from old mixed documents. No upgrade, token import or recovery-state conversion tool is required. All existing entrances use the new account binding rather than retaining a legacy reader; see the [rewrite boundary](#current-implementation-and-rewrite-boundary). Leave existing private stores untouched during development; fresh test-owned storage and synthetic credentials suffice for offline and loopback verification.

Provisioning, validation and explanation commands print concise human-readable output by default: what was checked or changed, each problem with its account or Provider name, and the next step. An explicit JSON option prints one structured document with stable field names and the same redaction. Neither form prints secret values or credential locators, and errors keep a non-zero exit status in both. The current `morphiecore-auth` prints JSON by default; commands the rewrite touches follow the new rule.

## Execution policy

**Finalized on 2026-11-02.** One bounded same-target retry replaces the interim single-send choice without restoring within-request account/Provider fallback. Numeric defaults are configurable at the owners in the [configuration surface](#configuration-surface), within the selected structural and safety rules.

“Next request” means a new gateway HTTP request, including a client retry of the failed turn; it does not require a new user message. MorphieCore may absorb one brief failure on the selected target; clients may still retry a returned failure. Their actual behavior must be verified, not assumed; a fresh request does not authorize the gateway to splice or replay an already-published response.

### Principles

1. **Cache affinity first.** A new request prefers its bound target, including a short bounded cooldown wait. Recovering the same target preserves affinity; a failure never preemptively rebinds the conversation.
2. **Selection before dispatch.** Choose another ready candidate only when the bound target cannot serve within the wait budget or is otherwise ineligible. After dispatch, any recovery stays on that exact target; a final failure returns without switching accounts or Providers.
3. **One ordered list, no balancing.** Selection is deterministic route order. No weights, tiers, round-robin, usage spreading, hedging or cache-value estimates.
4. **One retry owner, two sends at most.** The gateway owns the initial attempt and at most one additional serial attempt, including when probing. SDK and transport retries stay off. The unchanged request deadline bounds the whole exchange; a short recovery budget additionally bounds the retry phase. Shared OAuth maintenance has its own lifecycle and timeout.
5. **Typed, time-bounded health.** Cooldowns are absolute deadlines at a known scope. No error disables a target permanently, and request-shaped or malformed-output errors never cool or rotate anything.

### Terms

- **Target:** one account serving one route candidate (Provider, account/security identity, upstream model, operation/protocol and effective origin/path). It is the unit of sending, affinity and target-scoped cooldown; matching hostnames alone does not make two targets identical.
- **Candidate list:** the current model route's ordered targets. Every eligible target may be selected before dispatch; neither credential kind nor Provider identity introduces a separate switching boundary.
- **Binding:** a conversation's current target, keyed by explicit conversation carriers (see [Cache affinity](#cache-affinity)).
- **Cooldown:** an absolute deadline on a target, or on an account across all its targets.

### Selection

```text
candidates = eligible route targets in declaration order
bound = the fresh binding's target, if still eligible
ready = targets not cooling; an expired cooldown requires exclusive probe ownership

if bound is ready: send to bound
else if bound is cooling and its deadline is within max_wait:
    wait within the remaining request deadline, then recheck eligibility and acquire
else: send to the first ready candidate
no ready candidate: fail fast (see Downstream signals)
```

Only a valid conversation binding earns a short cooldown wait. New conversations choose the first ready route candidate without waiting for an unbound higher-priority target. Without a conversation carrier every request is new, and fill-first still keeps traffic on one target while it is healthy.

Check cancellation and remaining deadlines before waiting and before either dispatch. Recheck health and probe ownership after a pre-dispatch wait; if the preferred target cannot be acquired, select another ready eligible candidate before the first send or fail without sending. Do not loop through additional cooldown waits. Once a send begins, only the same-target retry below is permitted; candidate selection runs again only for a new gateway request.

A candidate-local inability to represent the request before dispatch permits checking later candidates, including other Providers. Invalid client input still rejects at ingress; trust, replay and explicit-guarantee rules remain in force. If no candidate can represent the request, return the appropriate failure. An upstream `request` or `malformed` failure remains terminal under the failure table rather than triggering candidate search.

### Failure classes

Provider classifiers map HTTP status, structured error codes, reset headers and in-band failed outcomes into exactly one class with an optional reset deadline, independently of publication. Codes a Provider classifier does not know fall back to the status column. There is no operator rule language. Only the explicitly retryable network/server subset of `transient` can receive the additional attempt; a health classification alone does not authorize a retry.

| Class | Typical evidence | Health effect |
|---|---|---|
| `request` | 400, 404, 413, 422; context overflow; refused or unsupported input reported as an error | None |
| `malformed` | Upstream output that fails strict parsing or protocol validation | None |
| `transient` | Connect failure, 408, 5xx, 529, premature upstream EOF or upstream attempt timeout | Target: 1 s doubling to 60 s |
| `rate_limit` | 429 without quota evidence; Provider capacity codes | Target: hint capped at 1 h, else 60 s doubling to 30 min |
| `quota` | 402; `insufficient_quota`; SIWC `subscription_sharing_usage_limit_exceeded`; Codex `usage_limit_reached`; Anthropic unified-window rejection; OpenCode Go usage limits | Account: reset hint capped at 7 days, else 30 min |
| `auth` | 401; terminal OAuth refresh errors | Account: 30 min; an OAuth reauthorization state lasts until login |
| `permission` | 403 that the Provider classifier does not map elsewhere | Target: 30 min |

- Provider-specific statuses such as 419 map into one of these classes in that Provider's classifier.
- All numeric cooldown values in this table are retained configurable defaults. Failure classes and health scopes remain semantic rules, not configurable retry permissions.
- Quota and auth cool the whole account because every model on it fails. Transient and rate-limit failures cool only that account and model.
- `malformed` exists because a deterministic Provider protocol defect would otherwise cool every account in turn.
- A timeout after the response head but before content remains `transient` health evidence, but an ambiguous post-dispatch timeout is not automatically retried. The upstream may already have performed work; even an explicit connection interruption can leave execution uncertain, so gateway/client retries do not promise exactly-once execution.
- User cancellation, downstream disconnection and gateway-local failures do not become upstream health evidence. Preserve any upstream failure already observed independently; release probe ownership without declaring recovery.

### Waits and bounds

- At most two inference sends per gateway request is a fixed ceiling: the original attempt and one retry. There are no per-target/class retry ledgers, independent retry rounds or configurable route send ceilings.
- The request clock starts when the gateway begins handling the request, before body admission. Resolving a route may tighten the deadline relative to that same start time, never restart it. The deadline covers admission, cooldown waiting, both attempts and recovery cleanup; each attempt remains bounded by the applicable remaining deadlines.
- `max_wait` (global, default 10 s) caps the pre-dispatch wait for a cooling bound target. A longer wait means selecting another ready eligible candidate or failing fast. It is separate from the short recovery-time budget and cannot extend that budget.
- Waits are cancellable. No SDK or transport retries, inference hedges, exponential retry-backoff sequence or OAuth refresh-and-resend path.

### Bounded same-target retry

Selected on 2026-11-02:

- Retry only before content publication and only once, for an explicitly allowed transient network/server failure. Keep Provider, account/security identity, upstream model, effective endpoint and admitted request meaning fixed. “Same Provider” does not permit trying another account.
- Eligible server failures are HTTP 408/500/502/503/504 and Provider-confirmed equivalent overload failures, only when structured evidence has not reclassified them as quota, rate limit, auth or another excluded class. A valid in-band error may qualify only through the same Provider-owned classification and before publication; HTTP 2xx alone supplies no retry evidence.
- Eligible network failures require positive evidence of a transient connection or I/O interruption, including a known connection-phase timeout or an independently identified premature transport termination. Do not retry an ambiguous post-dispatch timeout, an unclassified `Send`/`Connect` failure, or merely a missing protocol terminal. Preserve enough safe transport facts to distinguish these cases without retaining private URLs or raw third-party errors.
- Rate limits (429), quota, auth, permission, invalid requests, malformed output, TLS/certificate/trust or local configuration faults, cancellation and gateway-local failures do not qualify. Unlisted 5xx statuses do not gain retry permission through their status family alone.
- Start one short recovery deadline when the first retryable failure is observed; it covers cleanup, any bounded delay, preparation, the additional send and recovery to deliverable output. Clip it to the original request deadline. Do not merely bound the sleep or grant a new full attempt timeout.
- Before retrying, close/release the failed upstream and discard its unpublished attempt-local frames, parser state, response IDs and output metadata. Preserve observed usage and attempt diagnostics separately; do not mix first-attempt output into the second response. No attempts overlap.
- Once the retry reaches validated content eligible for delivery, or a valid no-content terminal, normal processing continues under the original request deadline. Headers, keepalives and lifecycle-only frames do not establish recovery. For buffered JSON, eligible content is an internal readiness milestone, not permission to publish partial JSON; successful completion still requires validated terminal/EOF.
- The recovery deadline is an independently cancellable constraint, not a replacement full-response transport timeout. Reaching deliverable content removes only that short constraint, never resets the request or attempt clock. If any frame is actually committed downstream before that milestone, the publication fence still forbids resending.
- Keep any foreground probe ownership throughout the bounded recovery. The first failure is retained as an observation but does not independently release the probe or commit affinity. A successful completed retry recovers health subject to fencing; final failure applies the relevant health scopes without double-counting the initial failure. Cancellation releases ownership while preserving independently observed upstream failures.
- The recovery budget defaults to 30 s. After the old attempt is released, retry immediately unless a valid upstream retry/reset hint requires waiting. That wait uses the same budget; if it leaves no recovery time, return the original failure without sending again. Do not shorten a valid hint to fit the budget, add an artificial delay or allow SDK/transport retries.

This is the initial selected execution policy, not a later mode. Client retries remain possible, so acceptance must count actual gateway and client transmissions together.

### Outcome and health resolution

Delivery, upstream outcome and health evidence have separate meanings:

- A successful completion under [Cache affinity](#cache-affinity), including output-limit incomplete completion or a completed tool-call/refusal turn, can recover health and conditionally commit affinity. Successful HTTP transport, valid failure delivery or a raw EOF alone cannot.
- If recovery succeeds, discard the first failure's pending health penalty while retaining its attempt diagnostics and reported usage. Existing generation fencing still prevents an older success from clearing newer health.
- Otherwise retain every independently observed health-affecting failure. A second request/malformed error, local timeout or cancellation does not erase the first upstream failure. Apply different account/target constraints at their own scopes; do not collapse them into the error shown to the client.
- Advance each affected scope's backoff at most once per gateway request. For multiple constraints on that scope, retain the latest resulting deadline without incrementing the level for each attempt. Failure timestamps, not delayed cleanup time, anchor observed reset information.
- The final failed attempt supplies the client-visible failure through the existing protocol mapping. If no second attempt starts, retain the first failure. A recovery deadline expiring during the second attempt is a timeout result, not a fabricated upstream error. Do not concatenate two failure bodies or retract published output.
- A cancelled or undeliverable response never commits or renews affinity. Release probe/attempt resources and preserve already-observed upstream failure evidence; do not infer an upstream failure from the downstream disconnect.

For example, `503 → quota` retains target-transient and account-quota constraints; `503 → malformed` or `503 → cancellation` retains the transient observation without assigning a new health penalty to the second outcome.

### Cooldown and recovery

- Failure updates only extend deadlines. A failure inside an open cooldown window does not escalate the backoff. Verified credential replacement has the narrowly scoped auth-probe exception described under [OAuth maintenance](#oauth-maintenance).
- The backoff level resets after a success or a configurable failure-free interval (default 30 min).
- Hints come from valid `Retry-After` delta-seconds/HTTP-date or Provider-declared reset fields. Missing, invalid or already-expired hints supply no future deadline. Cooldown hints use the configured floor (default 1 s) and class cap; those health bounds do not permit retrying earlier than a valid hint within the current recovery phase.
- An expired cooldown admits one foreground probe. Only a validated completion eligible under the success-only completion rule clears the relevant scopes, subject to health fencing; HTTP 2xx or a successfully delivered failure is insufficient. Neutral outcomes release probe ownership without clearing health. There are no background probes, permanent disables or health persistence; a restart clears health.
- Acquire probe ownership atomically across all relevant account/target scopes or acquire none. Account-scoped probing excludes concurrent probes on every model of that account; target-scoped probing excludes only that target. Ownership lasts through same-target recovery and is released on completion/cancellation. Releasing a neutral probe does not certify recovery.
- Generation fencing keeps stale completions from overwriting newer health or bindings.
- The current pool-wide rate-limit scope is dropped. Caches and rate limits follow upstream organizations or workspaces, so operators configure one account per independent organization or workspace; several keys of one organization add no failover value.

### Publication

The one same-target retry is permitted only before content publication. Lifecycle frames stay held as today, and terminal success still requires validated upstream completion/EOF. After publication a failure ends with an honest terminal error, never a resend or splice. Classified upstream failures still update health; the binding stays unchanged.

### Downstream signals

The gateway may absorb one short network/server failure before returning an error. The cited pi/OpenCode versions also retry selected rate-limit/server failures, so attempts can multiply. Native failed outcomes, HTTP failures and published stream errors can have different client behavior. Verify the currently installed clients and actual request counts during acceptance.

- Do not emit downstream `x-should-retry` or forward an upstream copy. Clients retain their own retry policy; gateway exhaustion, long cooldown or unusable credentials do not add a blanket automatic-retry prohibition.
- Do not turn one account's long `Retry-After` into a route-wide delay when another eligible candidate is ready.
- When all eligible targets are blocked, return a gateway admission failure: 429 for rate/quota-only blocking, 503 otherwise. For each lifecycle-usable, non-probing target, its earliest probe time is the latest deadline of its blocking scopes; the route hint is the earliest such time. Emit `Retry-After` only if this is a positive known interval within the 60 s horizon, rounded up to whole seconds.
- No ready target and no credible short deadline means no `Retry-After`: probe ownership, renewal in progress, reauthorization, quarantine and long cooldowns must not acquire an invented reset time. Return the failure honestly; clients may still retry under their own limits.
- Preserve existing protocol error meanings for dispatched failures, including native failed Responses. Do not turn a native failed outcome into a different HTTP result solely to trigger retries. Upstream credential failure must not masquerade as failed downstream authentication. Ordinary error messages remain redacted and body-free.
- Headers must be chosen before HTTP headers are committed. Published streams end through their protocol error mechanism, not revised HTTP status or headers.

Retry hints are advisory and do not authorize the gateway to reconstruct or replay a failed published response.

### Configuration surface

These are conceptual settings, not final field names or a configuration schema. Each parameter has one owner; no per-account/model/request override stack is introduced.

| Owner | Setting | Selected default |
|---|---|---|
| Global | `max_wait` | 10 s |
| Global | Affinity idle TTL | 1 h |
| Global | Affinity cache capacity | 4096 entries |
| Global | Downstream retry-hint horizon | 60 s |
| Global | Service request ceiling / Generation attempt timeout | 3600 s / 3600 s |
| Global | Upstream connection timeout | 10 s |
| Global | OAuth scan interval / refresh-ahead / renewal timeout | 15 s / 120 s / 90 s |
| Global | Safe failed-renewal delay | Fixed 60 s; no additional backoff curve |
| Provider | Short recovery-time budget for the one additional attempt | 30 s; zero disables the retry |
| Provider | Transient cooldown: initial delay, multiplier and cap | 1 s / 2 / 60 s |
| Provider | Rate-limit fallback cooldown: initial delay, multiplier and cap | 60 s / 2 / 30 min |
| Provider | Reset-hint floor and class caps | Floor 1 s; rate limit 1 h; quota 7 days |
| Provider | Quota fallback / auth / permission cooldown | 30 min each |
| Provider | Failure-free interval resetting backoff | 30 min |
| Route | Request deadline | Service ceiling |

Capacity and ordinary timeouts reuse current values in [executor affinity](../src/gateway/routing/executors.rs), [service limits](../src/gateway/startup/limits.rs), [Generation catalog](../src/topology/catalog.rs) and [HTTP transport](../src/transport/http.rs). The selected renewal delay uses the existing [60 s maintenance interval](../src/gateway/renewal.rs) as a starting point, but applies only where lifecycle evidence permits another refresh attempt; it cannot release quarantined material. The 30 s recovery budget is a selected policy, not a measured latency guarantee. No first-content timeout is added to the initial attempt; a retry with no deliverable output for the whole recovery budget fails even if the model might still be thinking.

Provider classifiers, failure scopes and the narrow retryable network/server set change with Provider contracts, not operator rules. The one-extra-attempt ceiling replaces the old per-class retry counts and route send limit; there is no auth resend allowance. Terminal OAuth errors still require reauthorization; no timing setting permits reuse of uncertain refresh material.

Do not add policy-value range checks or cross-parameter relationship checks at this stage; operators are responsible for suitable values. Schema/type parsing and security validation remain in place. Zero in-request wait disables cooldown waiting; zero recovery time disables the extra attempt; zero affinity capacity retains no bindings. Zero never means unlimited execution. Configured wait, recovery and timeout limits still govern execution, with runtime deadlines clipped to their owners' remaining budgets.

Cache lifetime here means gateway-owned affinity state. It does not change an upstream prompt-cache lifetime, promise a cache hit or change gateway-owned Responses history retention. Any protocol-supported upstream cache-retention choice remains governed by that protocol's contract.

### Worked examples

1. **Transient failure.** A conversation is bound to API key A, which returns a retry-eligible server error before publication. Within the short recovery budget, close that attempt and retry A once. If the retry completes successfully, the client sees the successful response and affinity stays on A. If recovery fails, return the failure and apply health effects; do not try B in this request.
2. **Short and long rate limits.** A returns 429 with `Retry-After: 5`: record the cooldown and return the failure. The next request may wait for A within `max_wait`. With `Retry-After: 120`, a promptly retried request selects ready key B instead. Only B's success moves the binding; later turns stay there after A recovers.
3. **SIWC usage limit.** A conversation on `chatgpt` account A hits `subscription_sharing_usage_limit_exceeded` without a reset hint. The request fails and account A cools for 30 min. A later request selects another ready candidate explicitly bound to that public model route. No ready candidate means admission failure, not an additional inference send.
4. **Published failure.** A releases content, then the upstream stream fails. End the stream honestly and record classified upstream health evidence. Do not resend, switch, commit affinity or fabricate completion. Any subsequent client retry is a separate request; partial delivered content is not a gateway continuation prefill.
5. **Recovery deadline.** A fails before publication, but cleanup or the retry cannot reach deliverable output within the short recovery budget. Stop recovery within the original request deadline, release upstream/probe ownership and return failure. Do not reset the budget, grant a third send or select B.

### Rejected mechanisms

- **LiteLLM:**
  - Retries nested inside fallback hops with independent counts and no total deadline.
  - Retries that reselect the failed deployment.
  - Failure-rate cooldowns.
  - Pre-call affinity commits.
- **CLIProxyAPI:**
  - Pool-wide retry rounds layered on per-credential cooldowns.
  - Credential-quota propagation through shadow model states.
  - Round-robin as the default.
  - Prefix- or hash-derived sessions.
- **Magpie:**
  - Group patience rounds.
  - First-token hedging.
  - Usage-share spreading.
  - Cache-read and idle-time heuristics for dropping affinity.
- **new-api:**
  - Permanent auto-disable with scheduled test recovery.
  - Weighted priority tiers indexed by retry count.

## Cache affinity

The [current executor](../src/gateway/routing/executors.rs) keeps conversation affinity and prefers the bound target over recovered higher-priority targets. Its completion hook and health-generation fence do not independently establish successful task outcome or concurrent binding ownership. The rewrite uses the following explicit contracts.

- **Key:** Agent identity, public model and conversation carrier. Carriers come from [ingress admission](../src/gateway/ingress/admission.rs): `X-MorphieCore-Conversation-Id`, then OpenCode `X-Session-Id`, pi `session_id`, Codex `session-id` and pi `x-session-affinity`. Do not derive sessions from content, prefixes or first messages, and do not let one caller's label select another caller's binding.
- **Identity:** a binding names the effective target: credential provenance, account endpoint and model/protocol scope. Normal OAuth token rotation preserves it; replacement API keys or changed targets cannot inherit it.
- **Commit:** only after successful completion. That is a valid terminal with a completed outcome, or an outcome that is incomplete only because of the output limit; tool-call turns count. Failed, cancelled, errored or published-then-failed attempts never commit.
- **Concurrent ownership:** commit conditionally against the binding version observed when the request began, including an absent binding. Replacement, expiry and eviction invalidate older commit rights; an absent→bound→evicted sequence must not let an old absent observer recreate the binding. A stale completion cannot overwrite or renew it. Version tracking must remain bounded by affinity capacity and in-flight work, without permanent per-conversation tombstones. Health fencing remains independent; do not serialize entire conversation requests.
- **Failure:** a failure never deletes or moves a binding. Cooldown decides whether selection waits for the bound target or moves; only the next success moves the binding.
- **Lifetime:** configurable sliding idle TTL (global, default 1 h) and configurable bounded capacity. Only an eligible successful completion renews the binding's TTL; request lookup, failures and read-only queries do not. Remove expired entries before evicting the least recently successful binding when capacity is needed. After expiry or eviction the conversation is new and fills from route order; a valid binding never proactively fails back. Provider cache lifetimes run from five minutes to about 24 hours, so a longer TTL rarely preserves a warm cache.
- **Replay:** moving a binding strips target-scoped private replay state under the accepted replay trust rules.

Do not base routing on inferred cache value, guessed cache lifetime or missing cache-usage fields.

### Optional restart continuity

Bounded affinity persistence remains a follow-up before hot reload, not part of the first rewrite. If later selected, store only private hashed conversation keys, target identity fingerprints and necessary expiry/success metadata, never prompts or credentials. Restore only entries whose configuration and credential identities still match, and bound file size, entry count and age.

Persistence is not selected for the first rewrite. Corrupt-file handling, stale configuration, privacy, shutdown behavior and whether persistence is opt-in must be decided before implementation. It does not make gateway-owned Responses history durable.

## OAuth maintenance

Keep one global maintenance policy and the existing account locks and lifecycle owner. Enumerate configured Active OAuth sessions independently of route references and routing enablement. Each rotation uses its own registered client and latest refresh material; expiry and principal facts come from the credential driver's verified contract.

Selected baseline: keep pure polling. The due check is local (account lock and store read, no network), so frequent scans are cheap. The selected defaults are a 15 s scan, 90 s timeout and 120 s refresh-ahead window. As operator guidance, allow at least the scan interval plus renewal timeout in the refresh-ahead window, and additional room for queueing or failed-renewal backoff; this is not a startup validation rule or a guarantee of timely renewal. A five-minute scan with a three-minute window can miss expiry: a scan seeing four minutes left skips renewal, and the next scan occurs one minute after expiry. A deadline-aware timer remains an alternative if polling delay becomes a concrete problem.

The maintenance loop must:

- Check on startup, serialize renewal for a session and recheck identity/expiry under the lock.
- Bound renewal time; apply the fixed global failed-renewal delay (default 60 s) only to failures for which the profile permits safely attempting renewal again. Do not add another exponential-backoff state machine.
- Persist replacement material safely and never reuse a potentially consumed refresh token after uncertain failure.
- Preserve account identity across normal rotation, and surface explicit reauthorization or persistence-recovery states.
- Treat unknown expiry as unknown; do not perform paid inference to test validity.
- Keep ordinary credential borrowing network-free. Any selected request-triggered 401 recovery calls the same maintenance owner instead of creating a second refresh implementation, and only where the profile's contract makes refresh the right recovery.

Timing and the safe failed-renewal delay are configurable. A request-triggered auth observation may notify the shared lifecycle owner where the profile permits recovery, but auth failure does not qualify for the network/server retry. Global timing does not remove Provider-specific token verification or rotation requirements.

Once replacement OAuth material for the same managed identity has been verified and durably published, only auth restrictions tied to the replaced material become eligible for an exclusive foreground probe. This does not declare inference healthy, clear quota/permission constraints, move affinity or bypass lifecycle vetoes. A late 401 for an older material revision cannot create or extend an auth penalty against the replacement. Refresh preserves identity; a changed principal/registration still follows the existing provenance and replay-isolation rules.

### Accepted lifecycle baseline

The user accepted this OAuth baseline on 2026-10-30, with request-boundary selection, no auth-triggered inference resend, routing-only enablement and subject-reservation clarifications on 2026-11-02. It records design decisions, not implementation authorization.

| Concern | Selected behavior |
|---|---|
| Registration and identity | Keep the issued client, verified subject/workspace, granted permissions and token set together. An alias is an operator label, not proof of identity. Ordinary token rotation preserves affinity; a changed principal or registration must not inherit private replay state |
| Concurrent renewal | Background maintenance, request-triggered recovery and CLI maintenance share session-level coordination, including across processes using the same store. Re-read the latest state after acquiring ownership; a late 401 for an old access token should not force another refresh if a replacement already exists |
| Temporary versus terminal failure | Temporary service failures do not erase the registration. Confirmed unusable refresh material requires reauthorization; invalid client or missing permission needs configuration/consent repair, not repeated refreshing. Classification belongs to each profile |
| Uncertain refresh outcome | A timeout after dispatch can mean the server rotated the token but its response was lost. Preserve explicit recovery/quarantine state; do not blindly resend potentially consumed material. Retrying refresh requires evidence that reuse is safe under the profile's contract |
| Durable replacement | New material must be verified and safely persisted before ordinary borrowing uses it. A crash or storage failure must not silently restore stale tokens or expose unverified replacements |
| Expiry and scheduling | Unknown expiry stays unknown. Account queueing and failed-renewal delays can exceed the nominal polling interval; frequent scans alone do not guarantee renewal before expiry. Existing code serializes account scans, so this matters as account count grows |
| Request-time recovery | Use material published by the shared lifecycle owner; an auth failure may request recovery for a later request but never triggers an inference resend now. Cancellation ends the request's participation; an already-started shared refresh remains lifecycle-owned and finishes safely within its own timeout. Cancellation must not restore possibly consumed material |
| Lifecycle versus routing health | Temporary cooldown, expired access, reauthorization-required and persistence-recovery states are different facts. A routing cooldown expiring cannot make invalid credentials usable |
| Verified replacement and auth probes | Durable verified replacement makes only old-material auth restrictions probe-eligible. Stale 401s do not penalize new material; quota/permission and lifecycle vetoes remain, and successful inference is still required to recover health |
| Account switching | Refreshing one registration is not switching accounts. A new request may select any eligible route candidate without separate opt-ins; refreshing does not restore exhausted subscription quota |
| Logout and inspection | Local removal and remote revocation have different outcomes. Surface incomplete revocation honestly; coordinate logout with renewal so a late refresh cannot reactivate a signed-out account. Status/explanation remains read-only and redacted, without exposing token values or credential locations |

The OpenAI [session contract](https://developers.openai.com/siwc/token-sharing-open-source/profiles-and-sessions) and [error guidance](https://developers.openai.com/siwc/token-sharing-open-source/errors-and-recovery), rechecked on 2026-10-30, require session-specific issued clients, serialized rotation with the latest material, and differentiated temporary, unusable-token and invalid-client handling. These are SIWC facts, not a universal refresh contract for every OAuth profile.

## Operational additions and exclusions

### Initial operational surfaces

| Addition | Value and scope |
|---|---|
| Offline configuration validation | Check references, Provider/credential-kind compatibility, override trust rules, duplicate names and OAuth subjects, and configuration types before activation; no policy-value range or relationship checks at this stage. Use the real loader; no login, refresh, paid probe or automatic repair |
| Read-only account and route explanation | Show normalized reasons, applicable wait/recovery/time bounds, the one-retry ceiling, eligible route candidates, cooldown/renewal state and affinity action. Queries must not change selection, extend TTL or trigger renewal |
| Explicit account diagnostics | Extend the existing upstream CLI to address a named account with bounded requests and no hidden retries; no automatic sweep of all accounts |

Reuse existing CLI and diagnostic owners. Ordinary reports remain body-free and redacted, with bounded attempt breadcrumbs; do not expose upstream origins or credential locators to clients. Usage reporting counts reported tokens per account and attempt, including failed attempts where usage is available; unknown usage stays unknown. Client response usage describes the delivered attempt, not a fabricated merged response. Compute no prices or balances. This design does not activate the deferred diagnostic aggregation/public-extension work in the [roadmap](roadmap.md#observability-follow-up).

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

## Review decisions

### Decided

| Decision | Date | Outcome |
|---|---|---|
| Sequencing | 2026-10-30 | Complete requirements first; rewrite account binding and execution policy afterwards |
| Design finalization | 2026-11-02 | Finalize this specification with the four final-review selections below. The subsequent breaking-rewrite scope and plan are recorded separately; design/planning does not execute code changes or deployment |
| Fully breaking rewrite and fresh setup | 2026-11-02 | User accepts removal of old configuration/credential formats, CLI/pool compatibility and stored-record reuse requirements. No migration/import/dual path; user manually provisions new keys and uses managed OAuth login where needed. Existing private data is not automatically modified or deleted |
| OpenAI service separation | 2026-11-02 | `openai` is API-key access; `chatgpt` is SIWC OAuth access, with independent model bindings and service contracts. Shared vendor/origin does not imply identical model availability; current SIWC `openai` references require explicit rebinding |
| Operational priority | 2026-11-02 | Cache affinity and simple code take precedence over transparent recovery of every request; normally no more than three accounts are expected. One bounded same-target retry absorbs brief network/server failures without restoring multi-target recovery |
| Route-wide eligibility | 2026-11-02 | Replaces the 2026-10-30 switching defaults and opt-in granularity: all enabled, validly bound accounts in the current model route may serve a new request, across credential kinds and Providers. Remove all three fallback switches and switch domains; route/account configuration authorizes destinations and billing paths, not target changes after dispatch. Usage counts tokens only |
| Bounded same-target retry | 2026-11-02 | Replaces the interim single-send choice and deferral: permit one additional serial inference send before content publication for explicitly retryable network/server faults. Provider, account, model and effective endpoint stay fixed. Short recovery and original request deadlines both apply; no OAuth auth resend, per-class counters, exponential retry sequence or within-request candidate switching |
| Attempt isolation and recovery ownership | 2026-11-02 | Release the failed upstream and discard unpublished attempt output before retrying; retain usage/failure observations separately. Keep probe ownership through recovery, and commit health/affinity only from eligible final outcomes with existing fencing |
| Endpoint override | 2026-10-30 | API-key accounts may override `base_url`; OAuth accounts may not |
| Account configuration | 2026-10-30 | Ordered named accounts per Provider reference the credential store by alias; explicit pools are removed; credentials stay under `$HOME/.local/share/morphiecore/credentials` and account/policy configuration stays outside the store |
| OAuth subject uniqueness | 2026-10-30 | Reject duplicate verified subjects under one Provider |
| Configuration transition | 2026-10-30, revised 2026-11-02 | Fresh current-schema setup, no migration or stored-record compatibility requirement. CLI output is human-readable by default with an optional JSON form; provisioning remains secret-safe |
| Execution policy | 2026-10-30, finalized 2026-11-02 | Retain cache-first selection, failure classes, cooldowns and success-only affinity. One bounded same-target retry replaces the original multi-target retry/fallback and interim single-send designs; the blanket downstream stop-retrying signal remains superseded |
| Conservative retry eligibility | 2026-11-02 | Retry selected HTTP 408/500/502/503/504, Provider-confirmed overload equivalents and positively identified transient connection/I/O interruption before publication. Exclude ambiguous post-dispatch timeout, unclassified transport errors, TLS/configuration/malformed faults and the other non-transient classes |
| Timeout after response head | 2026-10-30, finalized 2026-11-02 | Keep pre-content upstream timeout as transient health evidence, without automatically retrying ambiguous post-dispatch timeouts. Gateway/client retries can still duplicate upstream work; no billing or exactly-once promise |
| Recovery budget and defaults | 2026-11-02 | Recovery budget 30 s; no artificial delay, valid reset-hint waiting included without shortening the hint. Affinity capacity 4096, service/attempt 3600 s each, connection 10 s, safe failed-renewal delay fixed 60 s. Retain previously selected defaults for other settings |
| Mixed health observations | 2026-11-02 | Successful recovery clears pending penalties subject to fencing. Otherwise retain all observed upstream health constraints; neutral results do not erase them. Advance backoff once per scope per request and keep later deadlines; client-visible failure remains separate |
| OAuth material-aware recovery | 2026-11-02 | Verified durable replacement makes only old-material auth restrictions eligible for an exclusive probe; stale 401s cannot penalize new material. Do not clear quota/permission or bypass lifecycle vetoes |
| Client retry hints | 2026-11-02 | Emit no downstream `x-should-retry`; provide only credible positive route-level `Retry-After` within 60 s, never an account-wide delay when another target is ready. Preserve protocol error semantics and accept client-controlled retries even for long/unknown recovery |
| Request-boundary selection | 2026-11-02 | New conversations select the first ready route candidate; only valid bindings receive short cooldown waits. After dispatch, recovery stays on that target; only a later request can choose another. Recovered higher-priority targets do not displace a successful binding |
| Account guidance | 2026-10-30 | Drop pool-wide rate-limit cooling in the proposed rewrite; retain the draft's one-account-per-independent-organization/workspace guidance |
| Parameter ownership | 2026-10-30, revised 2026-11-02 | Retained cooldowns, cache lifetime and related bounds remain configurable at their listed owners; the short recovery-time budget belongs to Provider policy. One additional attempt is the structural ceiling, without per-class counters or a route send-limit setting |
| Implementation boundary | 2026-11-02 | Fully breaking account-system scope is accepted and execution order is documented in Roadmap. This turn plans the work; code execution requires the subsequent implementation request. Deployment and credential lifecycle operations retain their separate authorization boundary |
| Defaults and validation | 2026-10-30, revised 2026-11-02 | Keep defaults for retained settings; removed retry/send-limit settings do not survive as inactive fields. Defer policy-value range and relationship checks to operator judgment, preserving schema/type and security checks |
| Pre-dispatch candidate admission | 2026-11-02 | Replaces the 2026-10-30 Provider boundary: candidate-local representation failure permits checking later route candidates across Providers; invalid client input rejects, and trust, replay and explicit guarantees remain mandatory |
| Replacement scope | 2026-11-02 | Refines the 2026-10-30 scope: replace shared credential/account/target binding across existing entrances together; new recovery execution starts with Generation. Diagnostics and standalone tasks retain existing execution capabilities; no legacy pool path or frozen-feature activation |
| OAuth lifecycle baseline | 2026-10-30, revised 2026-11-02 | Keep polling, coordinated renewal, differentiated temporary/terminal/uncertain failures, verified durable replacement, lifecycle/health separation and read-only inspection. Auth does not qualify for the inference retry; request cancellation does not abandon an already-started shared refresh |
| Single OAuth login | 2026-10-30 | One managed session per account; no duplicate/concurrent login or duplicate subject aliases within a Provider. Refresh updates the session; explicit reauthorization or login after logout remains supported |
| Routing enablement and renewal | 2026-11-02 | Account enablement controls routing only; maintain configured Active OAuth sessions even when disabled or unreferenced. Logout ends the session; credential lifecycle vetoes remain independent |
| OAuth subject reservation | 2026-11-02 | Check uniqueness atomically at login publication and during validation. Disabled, signed-out and reauthorization-required accounts retain reservations until explicit account removal; reauthorization uses the same account |
| Concurrent affinity and idle TTL | 2026-11-02 | Successful completion conditionally commits against the observed binding version, independently of health fencing. Only an eligible successful completion renews TTL; lookup, failure and inspection do not |
| API-root composition | 2026-11-02 | An API-key override is the complete API root plus a catalog-declared operation-relative suffix; normalize the joining slash, never guess or strip version prefixes. Default operation targets stay intact without an override |
| Initial operations and deferred persistence | 2026-11-02 | Use existing provisioning, offline validation, read-only account/route inspection and bounded named-account diagnostics. No affinity persistence, hot reload or new operations framework in the first rewrite |

### Deferred decisions

These do not reopen the finalized account/execution contract or block its design acceptance.

| Decision | Boundary |
|---|---|
| Implementation start and deployment | The [execution plan](roadmap.md#account-rewrite-execution-order) covers the accepted breaking scope. Start implementation on the user's execution request. Real acceptance waits for fresh credentials and selected models; production activation remains separately approved |
| Optional restart continuity | Persistence remains excluded from the first rewrite. If later selected, decide opt-in, corruption handling, privacy, stale identities and shutdown behavior under [Optional restart continuity](#optional-restart-continuity) |

Keep this document as the detailed redesign specification, linked from Architecture's accepted decisions. Roadmap owns the accepted-scope execution plan and its gates; do not duplicate that sequence here. Update Usage when behavior is implemented.

## Design acceptance criteria

The design must explain deterministic request-boundary selection, at most one bounded same-target retry, configurable wait/recovery/time bounds, cooldown and affinity lifetimes, and honest behavior before and after publication. Identity, trusted targets, replay isolation, cancellation and credential lifecycle protections remain mandatory.

The final review has settled the policy choices. Roadmap now owns [rewrite sequencing and exit conditions](roadmap.md#account-rewrite-execution-order) within the shared-binding and Generation-first boundary. The project's existing [verification requirements](development.md#verification-gates) remain applicable; design and planning supply no runtime acceptance evidence.

## Reference observations

Reviewed **2026-10-30**. The revisions below are public main-branch observations, not promoted dependency pins, selected stable releases or executed compatibility evidence. Existing [source guides](references.md#source-guides) remain the broader reference entry point.

| Project and revision | Relevant observation | Proposed use and boundary |
|---|---|---|
| [BerriAI/litellm, `dbcacc7ab7880d5115376cd6c22d22898c50d08f`](https://github.com/BerriAI/litellm/blob/dbcacc7ab7880d5115376cd6c22d22898c50d08f/litellm/router.py) | Fallback groups wrap full retry loops with no total deadline; retries reselect deployments, so a pinned failing deployment absorbs them (issues #47482, #35303). A 429 `insufficient_quota` is treated as a transient rate limit (#32785). Session pins commit before the call. SDK retries are forced off | Borrow the single retry owner and `Retry-After`-driven cooldown. Avoid nested retry × fallback counts, pre-call commits and quota-as-rate-limit |
| [router-for-me/CLIProxyAPI, `90c447d3a8727d115aa7552ec93ce023736ed492`](https://github.com/router-for-me/CLIProxyAPI/blob/90c447d3a8727d115aa7552ec93ce023736ed492/sdk/cliproxy/auth/conductor_cooldown.go) | Provider-specific quota detection (Codex `usage_limit_reached`, Anthropic unified windows); extend-only deadlines that do not escalate inside an open window; fail fast past `max-retry-interval`. Pool rounds caused a retry storm (#5265); a stream defect cooled every key (#5283); one transient error rebinding sessions led to `rebind-on: quota` (#6537); multi-hour `Retry-After` stalls clients (#6646); encrypted reasoning crossed accounts (#6420) | Borrow typed scopes, extend-only deadlines, fail-fast waits and binding moves only on real account failure. Avoid rounds, shadow quota states, round-robin and derived sessions |
| [yetone/magpie, `eba7187368a8b05740326c90152db7b833cf0cc3`](https://github.com/yetone/magpie/blob/eba7187368a8b05740326c90152db7b833cf0cc3/internal/gateway/affinity.go) | Binding commits only after a complete error-free reply; a late stream error refreshing affinity was a bug (#733). Typed timed rests (rate, quota until reset, credit 30 min), ready-before-cooling with one probe per request (#1799), failover only before real content (#1418). A `strict` mode exists because mid-session account switches risk upstream bans (#1797). Earlier revision [`dca3e2d`](https://github.com/yetone/magpie/blob/dca3e2d716fe21bbb3e239f769a8e7861b135c01/internal/gateway/affinity.go) persists bindings across restarts | Borrow post-success commit, typed timed cooldowns, the single probe and bounded persistence. Avoid patience rounds, first-token hedging, usage spreading, cache-read heuristics and third-party Agent stores |
| [QuantumNous/new-api, `2d8e50bf36e94200b809dfb39e73624ec48b1e23`](https://github.com/QuantumNous/new-api/blob/2d8e50bf36e94200b809dfb39e73624ec48b1e23/middleware/distributor.go) (AGPL; ideas only) | Affinity records when the response status is below 400, so broken streams still refresh it; retries index weighted priority tiers (#6571, #6095); auto-disable is permanent and recovers only through scheduled tests (#7660) | Counterexamples only: time-bounded cooldowns and post-success commits instead |
| [earendil-works/pi, `4954d760504c11df9b2cbdc040346107b6a8153d`](https://github.com/earendil-works/pi/blob/4954d760504c11df9b2cbdc040346107b6a8153d/packages/coding-agent/src/core/settings-defaults.ts) | Agent retry enabled by default with three retries; [retryable failures](https://github.com/earendil-works/pi/blob/4954d760504c11df9b2cbdc040346107b6a8153d/packages/ai/src/utils/retry.ts) include 429 and 5xx | Input for [downstream signals](#downstream-signals); installed-Agent behavior still needs acceptance |
| [anomalyco/opencode, `e5a557efeee02c94486d391afc48f24a49ebc9a5`](https://github.com/anomalyco/opencode/blob/e5a557efeee02c94486d391afc48f24a49ebc9a5/packages/ai/src/provider-error.ts) | `isRetryable` honors `x-should-retry` before its own classification; [session retry](https://github.com/anomalyco/opencode/blob/e5a557efeee02c94486d391afc48f24a49ebc9a5/packages/core/src/session/runner/retry.ts) caps `retry-after` at 15 minutes | Input for [downstream signals](#downstream-signals) |

Primary protocol inputs:

- [OpenAI prompt caching](https://developers.openai.com/api/docs/guides/prompt-caching): caches are not shared across organizations; in-memory entries last about 5–10 idle minutes, extended retention up to 24 hours.
- [Anthropic prompt caching](https://platform.claude.com/docs/en/docs/build-with-claude/prompt-caching): caches are isolated by organization and, on the Claude API, by workspace; default lifetime 5 minutes, optional 1 hour, refreshed on use.
- [OpenAI accounts and sessions](https://developers.openai.com/siwc/token-sharing-open-source/profiles-and-sessions): apps manage several isolated registrations, each with its own issued client and credentials; email may be identical across registrations; the five-hour usage limit is shared across all apps of a ChatGPT user; refresh is serialized per session with the latest replacement. The page does not describe automatic account switching.
- [OpenAI errors and recovery](https://developers.openai.com/siwc/token-sharing-open-source/errors-and-recovery): `subscription_sharing_usage_limit_exceeded` (429) pauses new plan-based requests without inferring a reset time; `subscription_sharing_invalid_user` (401) leads to sign-in again after confirmed revocation or a terminal refresh error; OpenAI does not silently switch a request to another billing path.
- [OpenAI SDK retry documentation](https://github.com/openai/openai-python#retries): automatic SDK retries can multiply gateway attempts. Characterize actual client transmissions separately.

The SIWC and prompt-caching observations come from summarized page fetches on 2026-10-30, and the project observations from research passes over the cited revisions and issues. Recheck the verbatim official contract and relevant upstream revision before implementing a decision that depends on them. No research result here relaxes credential ownership, trust, publication or terminal-honesty boundaries.
