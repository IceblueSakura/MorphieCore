# Provider account redesign plan

**Status: draft direction accepted with configurable policy parameters, 2026-10-30. Final design confirmation and rewrite planning come later. No implementation design or schedule is approved.**

This document records the user's requirements, decisions and recommendations still under review. The user accepted the final draft from Claude Code session `ea0a9e90-bbc6-42f3-944d-5e03a3a83eaf`, with retries, cooldowns, cache lifetimes and related policy parameters configurable, and subsequently selected its stated numeric defaults. This accepts the design direction, not a finalized specification or rewrite authorization. Do not plan implementation slices, implement the design, change credentials or activate bindings until the user confirms the design and selects the later rewrite scope.

[Architecture](architecture.md) remains the authority for accepted contracts. [Roadmap](roadmap.md#temporary-routing-freeze) retains the current routing freeze and owns scheduling. This separately requested plan owns the account-redesign requirements and proposal; it does not reopen routing through the active IR/codec migration. [Usage](usage.md#prepare-credentials) describes the currently implemented configuration, not the shapes discussed here.

## Contents

- [User requirements](#user-requirements)
- [Current implementation and rewrite boundary](#current-implementation-and-rewrite-boundary)
- [Proposed account and configuration model](#proposed-account-and-configuration-model)
- [Proposed execution policy](#proposed-execution-policy)
- [Cache affinity](#cache-affinity)
- [OAuth maintenance](#oauth-maintenance)
- [Operational additions and exclusions](#operational-additions-and-exclusions)
- [Review decisions](#review-decisions)
- [Design acceptance criteria](#design-acceptance-criteria)
- [Reference observations](#reference-observations)

## User requirements

1. **Several named accounts per Provider.** A Provider such as OpenAI must support multiple named API keys and multiple named OAuth2 registrations together. Account fallback can distinguish exhausted credit, rate limits and other failures.
2. **Provider-owned recovery configuration.** Configure retries before fallback by failure class, together with retry backoff, cooldown durations and caps. The accepted draft uses Provider classifiers rather than operator-defined per-status rules; the earlier five retries for 5xx, two for 419 and one for 401 were examples, not selected defaults or universal classifications.
3. **API-key endpoint override.** API-key accounts inherit the Provider's default `base_url` unless they supply their own, including a deployment-specific API prefix. OAuth accounts never override it (user decision, 2026-10-30).
4. **Global OAuth maintenance settings.** Configure the check interval and refresh-ahead window globally, without a separate timing configuration for every Provider.
5. **Cache-first execution.** Keep the serving account through recoverable failures where bounded retries can help. Switch promptly for failures that cannot recover within the request, such as confirmed exhausted credit. Repeated temporary errors may eventually justify switching.
6. **Incremental delivery.** Runtime key addition is optional; restart-based activation is acceptable. Assume accounts of a Provider have the same model capabilities; do not add capability-based routing.
7. **Maintainability and simplicity.** Prefer one authoritative owner per fact, deterministic behavior and a small configuration surface. Reference projects supply mechanisms and counterexamples, not a feature checklist.
8. **No implicit switches beyond API keys.** By default, automatic switching between OAuth registrations of any profile (SIWC, Grok and others), fallback between OAuth and API-key accounts, and fallback across Providers are disabled; the operator can enable each explicitly. Only switching between API-key accounts of one Provider follows recovery policy without an opt-in. MorphieCore does not manage billing, and its usage accounting counts tokens only (user decisions, 2026-10-30).
9. **Requirements before rewrite.** The current implementation deviates substantially from these requirements. Complete the requirements first, then rewrite account binding and execution policy instead of incrementally reshaping the current path (user decision, 2026-10-30).
10. **Operator-readable CLI.** Provisioning, validation and explanation commands print human-readable output by default and offer structured JSON as an option (user decision, 2026-10-30).
11. **Configurable policy parameters.** Retry counts, wait and timeout budgets, retry/cooldown backoff, reset-hint bounds, affinity cache lifetime and capacity, downstream retry-hint horizon and OAuth maintenance timing must be configurable at their authoritative owner. Configuration cannot relax identity, publication, terminal-honesty or credential-lifecycle safety rules (user decision, 2026-10-30).
12. **Defaults and operator responsibility.** Use the draft's stated defaults. Do not add policy-value range or cross-parameter relationship validation at this stage; the operator selects suitable values. Retain schema/type parsing and credential, identity and target security checks (user decision, 2026-10-30).
13. **System-wide destination, Generation first.** The new credential/account routing system ultimately replaces the existing system throughout MorphieCore. Initial integration may cover Generation only because it is currently the mature path; other paths join later, without a parallel long-term routing design (user decision, 2026-10-30).

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

Current Generation, manual upstream diagnostics and standalone tasks have different pool rules. The selected initial integration scope is Generation; the final destination is one credential/account routing system across the whole application. Other paths are deferred, not excluded from replacement. This scope decision does not activate frozen standalone tasks or schedule their migration.

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

- Keep secrets in the existing managed credential store under `$HOME/.local/share/morphiecore/credentials`. Account configuration references them by alias; it does not duplicate tokens, expiry or verified principal metadata.
- Keep account lists and Provider/route policy with the other private configuration under `$HOME/.local/share/morphiecore`, outside the credential store. Each Provider lists its ordered named accounts; explicit pools are removed.
- Use stable configured account names, not file ordering, startup vector indexes or token values, as operator identities. Credential provenance still owns the security identity.
- Let declaration order select among new, unbound conversations. Do not add separate priority, weight and ordering fields.
- By default, the eligible set is the Provider's enabled accounts, narrowed by the [switch domains](#terms). Defer named sub-pools and account subsets until a concrete isolation requirement exists.
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

Several OAuth registrations of one profile may belong to the same upstream user; SIWC documents that email may be identical across registrations and that usage limits are per ChatGPT user across apps. Provenance currently includes the issued client, so two registrations of one user look like independent accounts. Once OAuth switching is enabled, switching between them cannot escape a shared limit. Configuration validation therefore rejects two registrations with the same verified subject under one Provider.

Each configured OAuth account has one managed login session. Do not support duplicate or concurrent logins for an already active account, multiple token sets under one account, or another alias for the same verified subject within the Provider. Normal refresh updates that session. Explicit reauthorization after invalidation or login after logout remains supported; it replaces the unusable or ended session rather than adding another one. These are MorphieCore-managed account rules, not a restriction on the user's sessions in unrelated applications.

Exact file and field names are chosen with the rewrite. Replacing the pool schema follows the project's current-schema-only rule: no silent migration, historical aliases or parallel legacy execution path. Reuse existing credential records where compatible.

### Configuration transition and CLI output

The rewrite is a breaking configuration change. Operators re-provision accounts through the CLI following a documented procedure; startup rejects the old pool schema with an error that names the procedure.

Provisioning, validation and explanation commands print concise human-readable output by default: what was checked or changed, each problem with its account or Provider name, and the next step. An explicit JSON option prints one structured document with stable field names and the same redaction. Neither form prints secret values or credential locators, and errors keep a non-zero exit status in both. The current `morphiecore-auth` prints JSON by default; commands the rewrite touches follow the new rule.

## Proposed execution policy

**Draft direction and stated defaults accepted, 2026-10-30; final design still pending.** This section synthesizes the [reference observations](#reference-observations). Numeric defaults are configurable, not fixed policy. The [configuration surface](#configuration-surface) owns parameter scope; exact schema and implementation are deferred.

### Principles

1. **Cache affinity first.** A bound conversation stays on its target through recoverable failures within the configured wait and retry budgets. A move outside the upstream cache scope loses cache reuse.
2. **Fallback second.** Move only when the target cannot serve this request within the wait budget: exhausted quota or credit, unusable authentication, repeated transient failure, or a cooldown longer than the budget.
3. **One ordered list, no balancing.** Selection is deterministic route order. No weights, tiers, round-robin, usage spreading, hedging or cache-value estimates.
4. **One owner, one budget.** The execution loop owns every send, wait, refresh and move. SDK and transport retries stay off; one request deadline and one send ceiling bound everything.
5. **Typed, time-bounded health.** Cooldowns are absolute deadlines at a known scope. No error disables a target permanently, and request-shaped or malformed-output errors never cool or rotate anything.

### Terms

- **Target:** one account serving one route candidate (Provider, account, upstream model, effective origin). It is the unit of sending, affinity and target-scoped cooldown.
- **Switch domain:** the targets a request may move between. By the [decided defaults](#decided), all API-key accounts of one Provider form one domain and each OAuth account is its own domain. The two Provider flags merge OAuth registrations, or OAuth with API-key accounts; the route flag merges Providers.
- **Binding:** a conversation's current target, keyed by explicit conversation carriers (see [Cache affinity](#cache-affinity)).
- **Cooldown:** an absolute deadline on a target, or on an account across all its targets.

### Selection

```text
anchor = the fresh binding's target, else the route's first target
domain = targets in the anchor's switch domain, route order, anchor first
ready  = domain targets not cooling; an expired cooldown admits exactly one probe

if the anchor is cooling and its deadline is within max_wait: wait, then send to the anchor
else send to the first ready target
no ready target: fail fast (see Downstream signals)
```

New conversations therefore fill the first ready target of the first domain. Without a conversation carrier every request is new, and fill-first still keeps traffic on one target while it is healthy.

When cross-Provider fallback is off, an inability to represent the request before dispatch does not permit selecting a later Provider. Stay within the selected Provider and the permitted switch domain; if no permitted candidate can represent the request, return the appropriate failure without crossing the boundary.

### Failure classes

Provider classifiers map HTTP status, structured error codes, reset headers and in-band failed outcomes before publication into exactly one class with an optional reset deadline. Codes a Provider classifier does not know fall back to the status column. There is no operator rule language.

| Class | Typical evidence | Same-target retry | Cooldown | Move within domain |
|---|---|---|---|---|
| `request` | 400, 404, 413, 422; context overflow; refused or unsupported input | None | None | No; return the failure |
| `malformed` | Upstream output that fails strict parsing or protocol validation | None | None | No; return an honest upstream failure |
| `transient` | Connect failure, 408, 5xx, 529, EOF or attempt timeout before content | Configured retries (default 1), configurable backoff (default 1 s, doubling) | Target: 1 s doubling to 60 s, set only when retries are exhausted | Yes |
| `rate_limit` | 429 without quota evidence; Provider capacity codes | Configured retries (default 1), only when the reset hint is within `max_wait` | Target: hint capped at 1 h, else 60 s doubling to 30 min | Yes |
| `quota` | 402; `insufficient_quota`; SIWC `subscription_sharing_usage_limit_exceeded`; Codex `usage_limit_reached`; Anthropic unified-window rejection; OpenCode Go usage limits | None | Account: reset hint capped at 7 days, else 30 min | Yes |
| `auth` | 401; terminal OAuth refresh errors | OAuth: configured recovery allowance (default 1); each resend requires coordinated refresh producing new material. API key: none | Account: 30 min; an OAuth reauthorization state lasts until login | Yes |
| `permission` | 403 that the Provider classifier does not map elsewhere | None | Target: 30 min | Yes |

- Provider-specific statuses such as 419 map into one of these classes in that Provider's classifier.
- All numeric retry, backoff and cooldown values in this table are selected, configurable defaults. Classes, scopes and retry eligibility remain semantic rules; zero retries disables that recovery, not its safety checks.
- Quota and auth cool the whole account because every model on it fails. Transient and rate-limit failures cool only that account and model.
- `malformed` exists because a deterministic Provider protocol defect would otherwise cool every account in turn.
- A timeout after the response head but before content is `transient`. The upstream may bill both sends; MorphieCore manages no billing and accepts this within the send ceiling.

### Retries, waits and bounds

- `N` retries means up to `N` additional sends to the same target.
- Retries are counted per request for each target and class, cumulatively; a class change resets nothing.
- `max_sends` (route, default 4) counts every send, including retries and probes. The request deadline covers sends, waits and refresh work.
- `max_wait` (global, default 10 s) caps any single in-request wait, whether for a `Retry-After` hint or for a cooling bound target. A longer wait means moving within the domain or failing fast.
- Waits are cancellable and count against the deadline. No SDK or transport retries.

### Cooldown and recovery

- Deadlines only extend. A failure inside an open cooldown window does not escalate the backoff.
- The backoff level resets after a success or a configurable failure-free interval (default 30 min).
- Hints come from `Retry-After` and Provider reset fields, clamped to a configurable floor (default 1 s) and class cap.
- An expired cooldown admits one foreground probe; a successful send clears every scope of its target. There are no background probes, permanent disables or health persistence; a restart clears health.
- Generation fencing keeps stale completions from overwriting newer health or bindings.
- The current pool-wide rate-limit scope is dropped. Caches and rate limits follow upstream organizations or workspaces, so operators configure one account per independent organization or workspace; several keys of one organization add no failover value.

### Publication

Retries and moves happen only before the first content is released; lifecycle frames stay held as today. After publication a failure ends with an honest terminal error, never a resend or splice. It still updates health; the binding stays unchanged.

### Downstream signals

Clients retry on their own: pi's agent loop retries 429 and 5xx three times by default, and OpenCode retries rate limits and provider failures unless the response carries `x-should-retry: false`. Gateway retries multiply with these.

- When the gateway stops recovering a failure, it answers with `x-should-retry: false`.
- When no ready target remains and the nearest cooldown ends within the configured downstream retry-hint horizon (default 60 s), it answers 429 (rate limit or quota) or 503 (other) with `Retry-After` set to that deadline, and leaves client retry allowed.
- Longer cooldowns answer with `x-should-retry: false` and the reset time in the error message, not a multi-hour `Retry-After`.

Both headers are safe to ignore. Verify their effect on currently installed pi and OpenCode during acceptance.

### Configuration surface

These are conceptual settings, not final field names or a configuration schema. Each parameter has one owner; no per-account/model/request override stack is introduced.

| Owner | Setting | Selected default or unresolved value |
|---|---|---|
| Global | `max_wait` | 10 s |
| Global | Affinity idle TTL | 1 h |
| Global | Affinity cache capacity | Bounded; value to confirm |
| Global | Downstream retry-hint horizon | 60 s |
| Global | Service request ceiling and attempt timeouts | Bounded; values to confirm |
| Global | OAuth scan interval / refresh-ahead / renewal timeout | Current values as starting points: 15 s / 120 s / 90 s |
| Global | Failed-renewal backoff: initial delay, multiplier and cap | Bounded; values to confirm |
| Provider | `transient` retries | 1 |
| Provider | `rate_limit` retries | 1 |
| Provider | OAuth `auth` recovery allowance | 1 |
| Provider | Transient retry backoff: initial delay, multiplier and cap | 1 s / 2 / cap to confirm |
| Provider | Transient cooldown: initial delay, multiplier and cap | 1 s / 2 / 60 s |
| Provider | Rate-limit fallback cooldown: initial delay, multiplier and cap | 60 s / 2 / 30 min |
| Provider | Reset-hint floor and class caps | Floor 1 s; rate limit 1 h; quota 7 days |
| Provider | Quota fallback / auth / permission cooldown | 30 min each |
| Provider | Failure-free interval resetting backoff | 30 min |
| Provider | OAuth registration switching | Off |
| Provider | OAuth/API-key fallback | Off |
| Route | Cross-Provider fallback | Off |
| Route | `max_sends` | 4 |
| Route | Request deadline | Service ceiling |

Provider classifiers, failure scopes and retry eligibility change with Provider contracts, not operator rules. Counts and timing are configurable within those rules: `request`, `malformed`, `quota`, `permission` and API-key `auth` do not gain same-target retries. Terminal OAuth errors still require reauthorization; no retry count permits reuse of uncertain refresh material.

Do not add policy-value range checks or cross-parameter relationship checks at this stage; operators are responsible for suitable values. Schema/type parsing and security validation remain in place. Zero retry counts and zero in-request wait disable those recovery actions; zero does not mean unlimited execution. Configured send, wait and timeout limits still govern execution. The stated numeric defaults are selected; entries without a number remain unspecified rather than acquiring an invented default.

Cache lifetime here means gateway-owned affinity state. It does not change an upstream prompt-cache lifetime, promise a cache hit or change gateway-owned Responses history retention. Any protocol-supported upstream cache-retention choice remains governed by that protocol's contract.

### Worked examples

1. **Transient failure.** A conversation is bound to API key A, which returns 503. A is retried after 1 s and answers. The binding stays on A and no cooldown is set.
2. **Short and long rate limits.** A returns 429 with `Retry-After: 5`: wait 5 s and resend to A. With `Retry-After: 120`, A's model cools for 120 s and the request moves to key B. When B answers, the binding moves to B and later turns stay there after A recovers; new conversations return to A after 120 s.
3. **SIWC usage limit.** A conversation on SIWC registration A hits `subscription_sharing_usage_limit_exceeded`. Account A cools for 30 min. With OAuth switching off, the domain is {A}, so the gateway answers 429 with `x-should-retry: false`. With OAuth switching on, the request moves to registration B and the binding follows on success.
4. **Mixed failures.** Configured with one transient and one rate-limit retry and `max_sends` 4:

   | Send | Target | Result | Next step |
   |---|---|---|---|
   | 1 | A | 5xx | Retry A (transient retry 1/1) |
   | 2 | A | 5xx | Cool A's model; move to B |
   | 3 | B | 429, `Retry-After: 3` | Wait 3 s; retry B (rate-limit retry 1/1) |
   | 4 | B | 5xx | `max_sends` reached; fail with `x-should-retry: false` |

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

The [current executor](../src/gateway/routing/executors.rs) already keeps conversation affinity, prefers the bound target over recovered higher-priority targets, commits only after completion, and fences stale completions. The rewrite keeps these contracts with the following rules.

- **Key:** Agent identity, public model and conversation carrier. Carriers come from [ingress admission](../src/gateway/ingress/admission.rs): `X-MorphieCore-Conversation-Id`, then OpenCode `X-Session-Id`, pi `session_id`, Codex `session-id` and pi `x-session-affinity`. Do not derive sessions from content, prefixes or first messages, and do not let one caller's label select another caller's binding.
- **Identity:** a binding names the effective target: credential provenance, account endpoint and model/protocol scope. Normal OAuth token rotation preserves it; replacement API keys or changed targets cannot inherit it.
- **Commit:** only after successful completion. That is a valid terminal with a completed outcome, or an outcome that is incomplete only because of the output limit; tool-call turns count. Failed, cancelled, errored or published-then-failed attempts never commit.
- **Failure:** a failure never deletes or moves a binding. Cooldown decides whether selection waits for the bound target or moves; only the next success moves the binding.
- **Lifetime:** configurable sliding idle TTL (global, default 1 h) and configurable bounded capacity. After expiry the conversation is new and fills from route order, which is the only failback. Provider cache lifetimes run from five minutes to about 24 hours, so a longer TTL rarely preserves a warm cache.
- **Replay:** moving a binding strips target-scoped private replay state under the accepted replay trust rules.

Do not base routing on inferred cache value, guessed cache lifetime or missing cache-usage fields.

### Optional restart continuity

Bounded affinity persistence is the preferred follow-up before hot reload. Store only private hashed conversation keys, target identity fingerprints and necessary expiry/success metadata, never prompts or credentials. Restore only entries whose configuration and credential identities still match, and bound file size, entry count and age.

Persistence is not selected for the first rewrite. Corrupt-file handling, stale configuration, privacy, shutdown behavior and whether persistence is opt-in must be decided before implementation. It does not make gateway-owned Responses history durable.

## OAuth maintenance

Keep one global maintenance policy and the existing account locks and lifecycle owner. Each rotation uses its own registered client and latest refresh material; expiry and principal facts come from the credential driver's verified contract.

Selected baseline: keep pure polling. The due check is local (account lock and store read, no network), so frequent scans are cheap. The selected defaults are a 15 s scan, 90 s timeout and 120 s refresh-ahead window. As operator guidance, allow at least the scan interval plus renewal timeout in the refresh-ahead window, and additional room for queueing or failed-renewal backoff; this is not a startup validation rule or a guarantee of timely renewal. A five-minute scan with a three-minute window can miss expiry: a scan seeing four minutes left skips renewal, and the next scan occurs one minute after expiry. A deadline-aware timer remains an alternative if polling delay becomes a concrete problem.

The maintenance loop must:

- Check on startup, serialize renewal for a session and recheck identity/expiry under the lock.
- Bound renewal time and account for failed-renewal backoff.
- Persist replacement material safely and never reuse a potentially consumed refresh token after uncertain failure.
- Preserve account identity across normal rotation, and surface explicit reauthorization or persistence-recovery states.
- Treat unknown expiry as unknown; do not perform paid inference to test validity.
- Keep ordinary credential borrowing network-free. Any selected request-triggered 401 recovery calls the same maintenance owner instead of creating a second refresh implementation, and only where the profile's contract makes refresh the right recovery.

Stated numeric defaults are selected; timing and failed-renewal backoff are configurable. The draft did not specify a failed-renewal backoff curve. Request-triggered renewal follows the accepted execution draft's bounded OAuth recovery. Global timing does not remove Provider-specific token verification or rotation requirements.

### Accepted lifecycle baseline

The user accepted this OAuth baseline on 2026-10-30. It records design decisions, not implementation authorization.

| Concern | Selected behavior |
|---|---|
| Registration and identity | Keep the issued client, verified subject/workspace, granted permissions and token set together. An alias is an operator label, not proof of identity. Ordinary token rotation preserves affinity; a changed principal or registration must not inherit private replay state |
| Concurrent renewal | Background maintenance, request-triggered recovery and CLI maintenance share session-level coordination, including across processes using the same store. Re-read the latest state after acquiring ownership; a late 401 for an old access token should not force another refresh if a replacement already exists |
| Temporary versus terminal failure | Temporary service failures do not erase the registration. Confirmed unusable refresh material requires reauthorization; invalid client or missing permission needs configuration/consent repair, not repeated refreshing. Classification belongs to each profile |
| Uncertain refresh outcome | A timeout after dispatch can mean the server rotated the token but its response was lost. Preserve explicit recovery/quarantine state; do not blindly resend potentially consumed material. Retrying refresh requires evidence that reuse is safe under the profile's contract |
| Durable replacement | New material must be verified and safely persisted before ordinary borrowing uses it. A crash or storage failure must not silently restore stale tokens or expose unverified replacements |
| Expiry and scheduling | Unknown expiry stays unknown. Account queueing and failed-renewal delays can exceed the nominal polling interval; frequent scans alone do not guarantee renewal before expiry. Existing code serializes account scans, so this matters as account count grows |
| Request-time recovery | Use fresh material from the shared owner, then resend only within the configured auth allowance, send ceiling and request deadline, before publication. Request cancellation ends its wait and prevents its resend; an already-started shared refresh remains owned by the credential lifecycle and finishes safely within its own timeout. Cancellation must not restore possibly consumed material |
| Lifecycle versus routing health | Temporary cooldown, expired access, reauthorization-required and persistence-recovery states are different facts. A routing cooldown expiring cannot make invalid credentials usable |
| Account switching | Refreshing one registration is not switching accounts. OAuth-to-OAuth, OAuth-to-key and cross-Provider moves retain their separate opt-ins; refreshing does not restore exhausted subscription quota |
| Logout and inspection | Local removal and remote revocation have different outcomes. Surface incomplete revocation honestly; coordinate logout with renewal so a late refresh cannot reactivate a signed-out account. Status/explanation remains read-only and redacted, without exposing token values or credential locations |

The OpenAI [session contract](https://developers.openai.com/siwc/token-sharing-open-source/profiles-and-sessions) and [error guidance](https://developers.openai.com/siwc/token-sharing-open-source/errors-and-recovery), rechecked on 2026-10-30, require session-specific issued clients, serialized rotation with the latest material, and differentiated temporary, unusable-token and invalid-client handling. These are SIWC facts, not a universal refresh contract for every OAuth profile.

## Operational additions and exclusions

### Recommended additions

| Addition | Value and scope |
|---|---|
| Offline configuration validation | Check references, Provider/credential-kind compatibility, override trust rules, duplicate names and OAuth subjects, and configuration types before activation; no policy-value range or relationship checks at this stage. Use the real loader; no login, refresh, paid probe or automatic repair |
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

## Review decisions

### Decided

| Decision | Date | Outcome |
|---|---|---|
| Sequencing | 2026-10-30 | Complete requirements first; rewrite account binding and execution policy afterwards |
| Switching defaults | 2026-10-30 | OAuth registration switching for every profile, OAuth/API-key fallback and cross-Provider fallback are disabled by default and enabled explicitly; API-key switching within a Provider follows recovery policy; MorphieCore manages no billing and counts tokens only |
| Endpoint override | 2026-10-30 | API-key accounts may override `base_url`; OAuth accounts may not |
| Account configuration | 2026-10-30 | Ordered named accounts per Provider reference the credential store by alias; explicit pools are removed; credentials stay under `$HOME/.local/share/morphiecore/credentials` and account/policy configuration stays outside the store |
| Opt-in granularity | 2026-10-30 | Two Provider flags (OAuth registration switching, OAuth/API-key fallback) and one route flag (cross-Provider fallback) |
| OAuth subject uniqueness | 2026-10-30 | Reject duplicate verified subjects under one Provider |
| Configuration transition | 2026-10-30 | Breaking change with documented CLI re-provisioning and no automatic migration; CLI output is human-readable by default with an optional JSON form |
| Execution draft direction | 2026-10-30 | Accept cache-first selection, failure classes, bounded recovery, success-only affinity commits and downstream signals, with configurable policy parameters; final design confirmation remains pending |
| Timeout after response head | 2026-10-30 | Keep the draft's pre-content timeout classification as `transient`, accepting possible duplicate upstream billing within configured bounds |
| Unbound placement | 2026-10-30 | Switch domains also restrict new conversations, as in the accepted draft |
| Account guidance | 2026-10-30 | Drop pool-wide rate-limit cooling in the proposed rewrite; retain the draft's one-account-per-independent-organization/workspace guidance |
| Parameter ownership | 2026-10-30 | Retry counts, cooldowns, cache lifetime and related bounds are configurable as listed in the configuration surface; retain failure-class policy rather than introduce a per-status rule language |
| Design-stage boundary | 2026-10-30 | Continue refining the design only; remove candidate implementation slices and defer rewrite planning until the user confirms the design |
| Defaults and validation | 2026-10-30 | Adopt the draft's stated defaults; defer policy-value range and cross-parameter relationship checks to operator judgment, preserving schema/type and security checks |
| Pre-dispatch Provider boundary | 2026-10-30 | Cross-Provider fallback off also forbids moving to another Provider when the selected candidate cannot represent the request before dispatch |
| Replacement scope | 2026-10-30 | Ultimately replace credential/account routing throughout the system; initially integrate Generation only, with other paths deferred rather than permanently separate |
| OAuth lifecycle baseline | 2026-10-30 | Accept polling, coordinated renewal, differentiated temporary/terminal/uncertain failures, verified durable replacement, lifecycle/health separation and read-only inspection; request cancellation stops waiting and resending without abandoning an already-started shared refresh |
| Single OAuth login | 2026-10-30 | One managed session per account; no duplicate/concurrent login or duplicate subject aliases within a Provider. Refresh updates the session; explicit reauthorization or login after logout remains supported |

### Open

A recommendation below is not a default configuration or an accepted architecture decision.

| Decision | Recommendation or alternatives to resolve |
|---|---|
| Final design confirmation | Review the complete draft with configurable policy parameters before planning the rewrite |
| Unspecified parameter values | Only settings for which the draft supplied no numeric value remain unspecified; existing stated defaults are selected |
| Persistence and operations | Select whether/when to persist affinity, its failure policy, and the minimum CLI/diagnostic surfaces |
| Later rewrite scope | Deferred until design confirmation; runtime mutation remains optional. No slices or rollout schedule are planned now |

Keep this draft as the authoritative home for the redesign while it is under refinement. After final design confirmation, record settled contracts in Architecture; only the separately selected rewrite scope adds work and gates to Roadmap. Update Usage when behavior is implemented.

## Design acceptance criteria

The design must explain deterministic selection and switching boundaries, configurable recovery budgets, cooldown and affinity lifetimes, and honest behavior before and after publication. Identity, trusted targets, replay isolation, cancellation and credential lifecycle protections remain mandatory.

Final review must settle the remaining policy questions without prescribing modules, implementation slices or a test matrix. Detailed rewrite sequencing and concrete verification plans will be selected only after design confirmation, within the selected system-wide replacement and Generation-first boundary. The project's existing [verification requirements](development.md#verification-gates) remain applicable to that later work.

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
