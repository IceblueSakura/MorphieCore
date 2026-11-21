# Runtime, Routing, and Trust

This document defines failover state transitions, conversation/cache affinity, streaming normalization, reverse-proxy delivery, loopback credentials, cluster diagnostics, and private exchange capture.

See [Architecture Overview](index.md) for system boundaries and core IR semantics; [Protocol adaptation](adaptation-and-dialects.md) for wire mappings; and [Settled decisions](decisions.md) for accepted trade-offs.

## Contents

- [Runtime and trust](#runtime-and-trust)
  - [Failover](#failover)
    - [Legal candidates](#legal-candidates)
    - [Request conversion reuse](#request-conversion-reuse)
    - [Advancement](#advancement)
      - [Bounded same-target retry](#bounded-same-target-retry)
    - [Health and recovery](#health-and-recovery)
      - [Downstream signals](#downstream-signals)
    - [Routing unit](#routing-unit)
  - [Conversation and cache affinity](#conversation-and-cache-affinity)
    - [Upstream session projection](#upstream-session-projection)
  - [Streaming and delivery](#streaming-and-delivery)
    - [Semantic stream normalization](#semantic-stream-normalization)
    - [Reverse-proxy delivery](#reverse-proxy-delivery)
    - [Optional delta coalescing](#optional-delta-coalescing)
  - [Credentials and security](#credentials-and-security)
    - [Trusted account targets](#trusted-account-targets)
    - [OAuth maintenance](#oauth-maintenance)
  - [Agent identity and cluster diagnostics](#agent-identity-and-cluster-diagnostics)
    - [Identity and authentication](#identity-and-authentication)
    - [Request and usage attribution](#request-and-usage-attribution)
  - [Observability](#observability)
  - [Configurable exchange capture](#configurable-exchange-capture)

## Runtime and trust

### Failover

Shared named-account binding, scoped cooldowns, old-material auth cooldown release, advisory affinity and bounded same-target recovery implement the contracts below. [Roadmap](../roadmap/unscheduled-issues.md#unscheduled-issues) tracks remaining profile qualification and prerequisites. Production deployment requires separate approval.

Failover operates on precompiled candidate lists under a cache-affine routing policy.

#### Legal candidates

Routes compile at startup (up to 64 candidates per model). Ingress requests cannot specify arbitrary candidate targets.

A target is one account serving a route candidate: Provider, credential/security identity, upstream model, operation/protocol and effective origin/path. Matching hostnames alone do not establish target identity. Candidate order precedes account declaration order; credential kind adds no priority. Every enabled, validly bound account in the declared route may serve a new request, including another Provider's account. Stored credentials alone authorize no destination or billing path. Standalone tasks retain fixed-account execution rather than Generation recovery.

Selection prefers a fresh conversation binding when ready. Only that binding earns one cancellable cooldown wait, and only when its blocking deadline falls within the configured `max_wait`; otherwise select another ready legal candidate immediately or fail fast. Any permitted wait remains bounded by the original request deadline. Recheck lifecycle and cooldowns afterward. New or unbound requests never wait for a higher-priority cooling target. Do not loop through additional waits or balance traffic by weights, price, latency, usage or inferred cache value.

Pre-dispatch representation checks may consider later candidates without weakening ingress, replay or explicit-guarantee validation. Once a send begins, upstream request/malformed failures never restart candidate search.

#### Request conversion reuse

Gateway preflight retains credential-free final bytes, protocol-header projections, bounded prefix digests and adaptation evidence together with the actual bound candidate. Executor ordering moves the whole record; preparation consumes it without reconstructing JSON or repeating semantic conversion. Public planning results remain metadata-only, and discovery uses the checked projection directly.

The checked projection owns an immutable wire value, its bounded serialized-size measurement and evidence. Retention admission, exact body allocation and eligible prefix hashing reuse that measurement instead of serializing again to count. Gateway materialization shares lookup eligibility with affinity selection and derives prefixes only for eligible initial lookups; direct preparation, disabled/explicit/unmapped lookup paths and already-hinted requests do not derive them.

The request-local retention budget is 1 MiB, charging body/header/digest allocation capacity and evidence including spare vector slots. Overflow leaves candidate admission and ordering unchanged: an attempted cache miss recomputes the existing pure conversion. Remaining records belong to the exchange future and are released on completion or cancellation, not retained in shared gateway state.

Preparation still validates the trusted binding, streaming and final body bounds before attaching credentials and account headers.

Codecs validate header carriers during request encoding; materialization stores the projected headers as a plain vector rather than retaining a deferred error state.

An affinity hint changes a typed candidate-local request and carries its new checked conversion; a hint that cannot fit leaves the original accepted body intact.

The test-only forced-stream override likewise invalidates its body.

No path patches encoded JSON or reparses it through generic `Value` deserialization, which could reinterpret ordinary reserved-key schema objects.

#### Advancement

The production model loader fixes one account target before dispatch. Configuration exposes no fallback switch, credential-member budget or route send ceiling. One selected candidate and one prepared request own the original send and at most one serial retry. The retry reuses the exact effective endpoint, credential material, headers and body, with a network-free lifecycle/material veto before sending.

Publication remains irreversible: no later attempt may replay committed output. Provider-owned finite classification separates health from the narrow resend allowlist. Typed transport causes distinguish positive interruption/connection timeout from TLS, local faults, unknown causes and ambiguous post-dispatch timeout; no private URL or third-party message enters policy. Unknown structured codes retain actual-status fallback and unknown in-band errors gain no synthetic-500 resend permission.

The short recovery clock covers cleanup, hints, preparation and the retry until projected deliverable content or a validated successful no-content terminal. Headers, usage and lifecycle frames cannot remove it. Buffered JSON remains private and terminal publication still requires validated EOF; readiness removes neither the original request deadline nor the attempt deadline.

##### Bounded same-target retry

- **Eligible failures:** HTTP 408/500/502/503/504 and Provider-confirmed equivalent overload, unless structured evidence reclassifies them as an excluded class. In-band failures need the same positive classification; HTTP 2xx alone supplies none.
- **Network evidence:** positive transient connection/I/O interruption or known connection-phase timeout. Ambiguous post-dispatch timeouts, unclassified send/connect errors and missing protocol terminals alone do not qualify.
- **Exclusions:** rate limits, quota, auth, permission, invalid requests, malformed output, TLS/trust, local configuration faults and cancellation. Other 5xx statuses do not qualify merely by family.
- **Isolation:** release the old upstream before the retry; discard unpublished frames, parser state, response IDs and output metadata. Preserve reported usage and attempt diagnostics independently. Sends never overlap; SDK and transport retries remain disabled.
- **Timing:** one recovery deadline starts at the first eligible failure and is clipped to the original request deadline. Retry immediately after cleanup unless a valid upstream hint requires waiting. Never shorten that hint to fit the budget; return the original failure if no recovery time remains. No third send or new full timeout is granted.

The request clock starts before body admission. Route resolution can tighten it relative to that start, never restart it. A network retry can repeat work already performed upstream; it does not promise exactly-once execution. Current installed-client acceptance must count client retries separately.

#### Health and recovery

One exchange-local failure ledger retains the original HTTP observation, later interruption, material revision and previous-attempt evidence. Terminal cleanup records advisory cooldowns before asynchronous downstream normalization.

- **Scoping:** Auth/quota constrain an account; transient, rate-limit and permission failures constrain its exact model/operation/origin/path target. Independent constraints retain their own deadlines.
- **Concurrency:** expired cooldowns admit ordinary concurrent requests. There are no exclusive probes, routing generations or affinity observation leases. Credential lifecycle coordination remains independent.
- **Recovery:** Validated successful work clears expired constraints and resets exhausted backoff; live cooldowns survive concurrent successes.

  Reported failures and independently identified transport interruptions retain observation times and material revisions through cleanup or cancellation.

  Clean HTTP EOF missing a protocol terminal is malformed, not positive transport-interruption evidence.

  A completed retry discards its first failure's pending penalty; otherwise merged account/target constraints advance each affected scope at most once.

  Attempt-local output resets while usage and diagnostics remain separately attributable.
- **Downstream hints:** Upstream reset evidence remains available to health/recovery. Returned `Retry-After` describes only a credible short route-wide block and is suppressed when another usable target is ready; `x-should-retry` is never emitted.

Provider-owned finite classifiers use status, structured codes, declared reset fields and in-band outcomes. Unknown codes retain actual-status fallback; operators do not supply error rules. Configuration defaults and parameter ownership live in [Usage](../usage/credentials-and-diagnostics.md#recovery-defaults).

| Class | Scope and disposition |
|---|---|
| `request` / `malformed` | No cooldown: invalid/unsupported input or protocol-invalid output cannot rotate through accounts |
| `transient` | Exact target; health includes failures such as timeout that may still be ineligible for retry |
| `rate_limit` | Exact target unless classified as account quota |
| `quota` / `auth` | Whole account; credential lifecycle vetoes remain independent of cooldown expiry |
| `permission` | Exact model/operation/origin/path target |

Failures extend deadlines; another failure inside an open window does not escalate backoff. Successful completion clears only expired constraints; a configured failure-free interval can reset backoff. Valid `Retry-After` delta/date and Provider reset fields supply deadlines; malformed, missing and expired hints do not. Health hint floors/caps never authorize retrying earlier than a valid hint. Several keys in one upstream organization may share quota and caches; aliases alone provide no independent failover domain.

Delivery, outcome and health are separate facts. A completed tool/refusal turn or output-limit incomplete completion can settle success, while HTTP success, raw EOF or delivering a failure cannot. A cancelled or undeliverable response never renews affinity. Preserve independently observed upstream failures through later malformed output, local timeout or cancellation; do not infer upstream failure from client disconnect. For example, `503 → quota` retains both target-transient and account-quota constraints, while `503 → malformed` retains the original transient observation only.

The final attempted failure supplies the client error. A recovery deadline expiring during the second attempt is a timeout, not a fabricated upstream error. Never concatenate failure bodies or retract published content.

##### Downstream signals

If all otherwise-legal targets are blocked, return 429 for rate/quota-only blocking and 503 otherwise. For each lifecycle-usable target, availability is the latest deadline of its blocking scopes; the route hint is the earliest such availability. Emit `Retry-After`, rounded up to seconds, only for a known positive delay within the configured horizon. Quarantine, renewal and reauthorization do not acquire invented reset times.

Dispatched native failures keep their protocol meaning; upstream credential failure is not downstream authentication failure. Do not forward `x-should-retry` or rewrite native failed outcomes merely to trigger client retries. Headers are fixed before publication; published streams use their protocol error mechanism rather than revised status/headers.

#### Routing unit

An explicit conversation/session carrier is the routing unit, scoped by authenticated Agent and public model. Only a valid binding earns one bounded cooldown wait; after waiting, lifecycle and cooldowns are rechecked before dispatch. Successful final delivery updates a bounded TTL preference; the last eligible completion wins. Without a preference, configured order selects the first available exit.

### Conversation and cache affinity

Affinity keys and advisory cache policies are defined in [`protocol::cache`](../../crates/morphiecore-protocol/src/cache.rs):
- `X-MorphieCore-Conversation-Id` supplies declared conversation identity for multi-turn route affinity and upstream cache/session projection.
- Prefix-cache indexing supplies advisory request hints, never an implicit conversation or account-routing key.
- Target cache capabilities are enforced via [`CacheTargetPolicy`](../../crates/morphiecore-protocol/src/cache.rs), omitting unsupported advisory hints under declared loss rules.

#### Upstream session projection

Session grouping is pure candidate-local transport policy, not stored history, authorization or replay authority. Ingress accepts the neutral conversation header before generic Agent carriers, then native `x-opencode-session` affinity before `x-opencode-session-id` identity. The native values can differ for child or forked OpenCode sessions. [Carrier precedence](../usage/client-requests.md#conversation-and-cache-affinity) owns the complete ingress list and validation.

Generated IDs use XXH3-128 with the default zero seed: NUL-terminated Provider-specific versioned domain, one source-kind byte, the source byte length as little-endian `u32`, then the unchanged source bytes (UTF-8 for text sources).

The unsigned 128-bit result is formatted as 32 lowercase ASCII hex characters, most-significant digit first and zero-padded.

Version-2 domains identify this encoding; generated prompt-cache keys use `MorphieCore.prompt-cache.v2`.

These IDs are grouping hints, not authentication or integrity checks.

Source tags distinguish independent requests (`0`), declared conversations (`1`), inferred prefix groups (`2`) and cache keys (`3`). Domain and source-kind framing separate namespaces without ambiguous concatenation.

No clock, random seed, mutable map, process salt, credential revision, model name or prompt body participates in derivation. The same source and namespace therefore survive restart, ordinary credential renewal and model changes. Routing and replay retain their independent authenticated scope below.

Inferred prefix groups themselves remain ephemeral: restarting or expiring the prefix index can select a new source group, so equal prompt text alone does not promise a stable session across those boundaries.

| Target | Wire carrier | Grouping source and precedence |
|---|---|---|
| SIWC Responses | `session-id` header | Hash the final projected `prompt_cache_key` under `MorphieCore.siwc-session.v2`. Its existing explicit-key → declared-conversation → inferred-prefix derivation stays authoritative. Without a key, omit the header; do not invent cross-request affinity |
| OpenCode Go Chat, Responses and Messages | `x-opencode-session` header | Preserve an explicitly admitted dialect session; otherwise hash declared conversation, inferred prefix group, or independent request in that order under `MorphieCore.go-session.v2`. Cache keys remain independent and never become Go body sessions |
| OpenRouter Chat and Responses | body `session_id` | Preserve an explicitly admitted dialect session; otherwise hash only a declared conversation under `MorphieCore.openrouter-session.v2`. Key-only, inferred-prefix and independent-request traffic stays without a synthesized session |

Explicit dialect sessions retain their existing validation and spelling; they are not a public extension to the standard downstream APIs. No arbitrary inbound header map is forwarded. New Providers must select a documented carrier, source semantics and distinct versioned domain before joining this projection.

OpenRouter also accepts `x-session-id`, but its body carrier avoids redundant precedence. Explicit sessions establish upstream stickiness after the first successful request; key-only fallback waits for observed cache usage. All three policies remain advisory: they do not guarantee a cache hit or pin a backend worker. [Session carrier sources](../refs/2026-11-12-generation-binding-contracts.md#session-carriers--2026-11-13) separate official contracts from observed SIWC behavior.

Route/cache identity includes configuration namespace, credential provenance and the exact public-model/target/operation identity. Native OpenAI reasoning uses a separate model-independent service-source identity; compaction and other native signatures remain strict. [History and replay](history-and-replay.md#reasoning-replay) owns the scope rules. API-key replacement invalidates old private replay; normal OAuth material renewal preserves provenance.

Affinity is an advisory, success-updated preference scoped by authenticated Agent, public model and declared conversation. Failure never deletes or moves it. Only validated eligible success stores or renews its sliding idle TTL; lookup and failure do not. The last eligible completion wins and may recreate an expired/evicted entry, without leases or version arbitration. Insertion drops expired entries and evicts the least recently successful entry at capacity. Zero TTL or capacity retains nothing. Restart persistence remains [unselected](../roadmap/unscheduled-issues.md#routing-qualification).

### Streaming and delivery

Delivery lifecycle and event streaming are coordinated by [`ResponseDelivery`](../../crates/morphiecore-runtime/src/execution/delivery.rs) under [`SseLimits`](../../crates/morphiecore-protocol/src/stream.rs):
- **Pre-publication holding:** Bounded lifecycle-only frames wait for content while the one same-target retry remains possible.
- **Publication freeze:** Flushing frames irreversibly prohibits another send. Every later failure, including transport, malformed, byte-limit, attempt or exchange deadline and shutdown, drains validated pending content and then ends the stream with exactly one protocol error frame followed by a clean body end, which is never a success terminal. Deadline and shutdown framing is bounded to 2 s; when the frame cannot be encoded or handed off, the transport is aborted instead.
- **Honest completion:** Validated upstream EOF gates the successful terminal, not earlier content frames.
- **Final handoff:** Final-frame acceptance and cancellation share one fence. Before acceptance, cancellation revokes delivery; afterward only synchronous completion checks, health/affinity settlement and diagnostics remain. Acceptance alone never makes a failed or neutral operation successful.

#### Semantic stream normalization

Shared [SSE framing](../../crates/morphiecore-protocol/src/sse.rs) follows WHATWG dispatch: comment and field-only blocks without data lines (`retry:`, `id:`, `event:` keep-alives) are not events and count only toward wire bytes; an empty `data:` line is an event with empty data.

Codecs validate and reduce upstream events in source order, emitting legal target streams:
- Preserves delta ordering, call identities, and signature attachments.
- Emits block openers before deltas, and closes blocks before terminal events.
- Interleaving tool calls and text parts are serialized to conform to target grammar.

Responses opens with exactly one `response.created`, including when its actual status is `queued`.

Queued does not establish semantic Started; only an observed `response.in_progress` starts a queued response.

Encoding then emits progress without a second created event.

The pinned [OpenAI SDK created-event contract](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/responses/response_created_event.py) admits a Response with queued status, and its [accumulator](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/lib/streaming/responses/_responses.py#L515-L524) requires created first.

Queued EOF, cancellation and errors cannot fabricate progress or successful completion.

Messages downstream delivery uses one active wire block in semantic owner/part declaration order, independently of numeric IDs. The head streams incrementally; successors wait for its actual owner finish, including a delayed thinking signature. Thinking emits one signature immediately before block stop; redacted blocks keep their native shape. Reports retain their existing independent timing and authority.

The codec-local [scheduler](../../crates/morphiecore-protocol/src/anthropic/messages/client/sse/schedule.rs) bounds retained payloads and container overhead, releases drained capacity, and rejects known unsupported content before queueing.

[Frame construction](../../crates/morphiecore-protocol/src/anthropic/messages/client/sse.rs) checks serialized size and cumulative wire/event budgets during drainage.

Cancellation or rejection discards pending content.

EOF-authorized incomplete termination may close unfinished text/tool wire blocks without inventing semantic completion; source reduction, argument validation and publication rules remain outside the scheduler.

Native Messages intake is unchanged.

An observed incomplete text-item finish also authorizes closure of that owner's remaining wire blocks after their queued content drains. It preserves the source item status and unfinished part facts, matching existing static rendering and terminal-only closure. A completed item still requires actual part completion; block closure does not authorize a successful operation terminal.

#### Reverse-proxy delivery

MorphieCore operates behind a loopback-bound reverse proxy:
- SSE responses set `X-Accel-Buffering: no` and `Cache-Control: no-store`.
- The proxy terminates TLS and provides client compression; MorphieCore handles authentication and streaming logic.
- The [HTTP/1 listener](../../crates/morphiecore-gateway/src/ingress/http.rs) applies fixed bounds before request admission: a 90 s header-read timeout that also closes idle keep-alive connections, at most 512 open connections (excess peers wait in the backlog), and a 5 s connection drain after the shutdown signal before remaining connections are dropped and request and storage drains run. Proxy upstream keep-alive timeouts should stay below 90 s.

#### Optional delta coalescing

Configured via [`DeltaCoalescing`](../../crates/morphiecore-runtime/src/execution/delivery/coalescing.rs) (default 50 ms / 16 KiB bounds):
- Merges consecutive adjacent `PartKind::Text` fragments into larger downstream frames.
- Flushes immediately upon non-text events, usage reports, or timer expiration.

### Credentials and security

- **Store:** Private files and redacted, zeroizing in-memory secret types; loaders reject unsafe paths. Provider documents hold typed OAuth/key maps, rejecting duplicate aliases and malformed known fields across both partitions. Publication moves and returns records without secret-cloning APIs, preserving shared locks, bounds, quarantine and atomic-write ordering. Only the current format is maintained: no historical filename detection or migration; generic invalid-directory rejection and reserved namespace protection remain.
- **Driver ownership:** Managers share an immutable driver registry. Each driver has one registration-aware login entrance and one renewal entrance; pending renewal still requires its separate identity verification and durable publication steps.
- **Browser login input:** Every browser login accepts HTTP callback and a full callback URL on stdin together; the first valid result wins at the shared listener.

  Stdin uses ordinary echoed lines, with no mode switch, hidden-input parser or partial-line coordination.

  Pasted URLs relay only to the prepared loopback target and share state/field validation, PKCE, issued-client binding, nonce, identity proof and the original deadline.

  No token import or second exchange is introduced.

  See [operation](../usage/credentials-and-diagnostics.md#oauth) and [implementation references](../refs/2026-11-02-account-recovery-sources.md#headless-browser-callback-input).
- **Automatic OAuth renewal:** Startup owns the deduplicated maintenance registry, including configured disabled or route-unreferenced accounts. Listener-owned renewal consumes that registry and rotates credentials serially under account locks. [OAuth maintenance](#oauth-maintenance) owns cancellation and replacement-verification checkpoints.
- **API-key access:** Startup binds a reference; borrowing enforces enabled state, quarantine, record/epoch/generation identity and material digest. No separate strict-binding policy is maintained.
- **Ingress Auth:** Binds exactly one configured Agent key via Bearer token or `x-api-key`.

#### Trusted account targets

Validate target bindings before borrowing secrets. API-key `base_url` overrides are complete HTTPS API roots (literal loopback HTTP is also permitted) without userinfo, query or fragment. Append the catalog's explicit operation suffix after normalizing the joining slash; never guess or remove version prefixes. OAuth accounts reject overrides and send tokens only to their profile's resource origin. Business data selects neither origins, credentials, auth headers nor proxies.

Account enablement controls routing only.

OAuth registrations with the same verified subject under one Provider cannot become separate managed aliases, even with different issued clients.

Login publication reserves that subject atomically across processes.

Disabled, signed-out and reauthorization-required records retain the reservation; explicit removal releases it.

Active sessions and pending login reject duplicate login before authority I/O; reauthorization replaces the existing unusable registration.

Ordinary renewal preserves provenance, while a changed registration or API key cannot inherit private replay identity.

#### OAuth maintenance

Inference borrowing and eligibility checks are network-free and never start login or refresh. Listener-owned polling and explicit CLI maintenance share the account lifecycle owner, re-read latest material under the lock and use each registration's issued client. API-key mutations need only their short store transaction. Each runtime has one maintenance listener; no multi-listener arbitration is added.

- **Scheduling:** check on startup, then poll with configured scan, refresh-ahead, timeout and safe failed-renewal delay. Successful short-grant renewals also respect minimum spacing. Defaults live in [Usage](../usage/gateway-configuration.md#configure-the-gateway). Unknown expiry stays unknown; no paid inference tests validity. Serial account queueing and failure delays can exceed a scan interval, so polling does not guarantee renewal before expiry.
- **Cancellation:** filesystem phases run off async workers while retaining account ownership. Cancellation drops network work and prevents queued preparation. Once replacement/pending material is returned, revision-checked publication completes even after cancellation: old refresh material may already be consumed. Graceful listener shutdown drains its queued and running storage work before returning and releasing account ownership. Aborting the listener future still preserves queued publication but does not await it. SIGINT and SIGTERM use graceful shutdown; filesystem completion is not a hard wall-clock guarantee.
- **Verification:** persist complete replacement material before secondary identity verification. Grok UserInfo and SIWC failures leave pending, unusable material; explicit refresh resumes verification without another exchange. Network/timeout verification failures may resume automatically after the safe delay; denial, identity mismatch, protocol and validation failures wait for explicit refresh. Uncertain exchanges retain recovery/quarantine state rather than restoring or blindly resending old tokens.
- **Auth availability:** verified durable replacement releases only old-material auth cooldowns. Stale 401 observations cannot penalize replacement material; quota, permission, reauthorization and lifecycle vetoes remain. Refresh moves neither affinity nor billing accounts.
- **Logout and inspection:** coordinate logout with renewal so late publication cannot reactivate a signed-out account. Local removal and remote revocation have separate outcomes; incomplete revocation surfaces honestly. Status/explanation stays read-only and redacted.

Profile-specific verification, rotation and terminal-error semantics follow the [account source guide](../refs/2026-11-02-account-recovery-sources.md); global timing cannot weaken them.

### Agent identity and cluster diagnostics

#### Identity and authentication

- `agent_id`: Stable configured agent identity across key rotations.
- Authentication table maps keys to `agent_id` with constant-time verification.

#### Request and usage attribution

- Every request is tagged with an immutable Agent identity and runtime nonce.
- Usage is attributed to the executing Agent rather than the conversation owner.

### Observability

The gateway uses isolated `tracing` Logs/Traces bridges, bounded OpenTelemetry SDK batch processors and OTLP/HTTP JSON exporters. The library installs no process-global subscriber. INFO is the default log level and does not control traces. Instrument I/O and orchestration owners, not pure semantic, lowering or protocol code. [References](../references.md#telemetry-dependency-selection) owns the pins.

| Boundary | Observation and limits |
|---|---|
| Ingress and request | A local server span covers auth/admission and response body completion, error or drop. Context follows spawned producers and body polls. Rejected authentication is body-free. Configuration errors report a safe domain and location without rejected values or private map keys. |
| Operation and attempt | An operation owns candidate planning and local outcomes; each attempt covers preparation through its local result or delivery. A slot may fail before any send. A failed attempt retains its own diagnosis even if recovery succeeds. SDK spans and timestamps replace manual timers/counters; telemetry cannot prove upstream execution counts. |
| Upstream I/O and delivery | An HTTP client child span runs from send through body EOF, transport failure or undrained exit. Headers, first projected content, terminal validation, EOF, publication and handoffs are bounded one-shot observations, not per-chunk logs. First content is not first frame or recovery readiness. Local handoff is neither remote receipt nor inference success. An undrained exit alone does not tell why consumption stopped. |
| Failures | Recovered attempts and client/upstream 4xx rejection are WARN; unrecovered service, transport, decode and terminal failures are ERROR. HTTP-200 failures use their source classification. Ordinary cancellation/shutdown, length stops, refusal and accepted adaptations are not failures. Closed categories and source boundaries survive cancellation. Responses retains the last complete source event and native sequence on decoder rejection. |
| OAuth renewal | The scheduled renewal span owns correlated failure logs and HTTP/prepare/commit children, including blocking storage work. A queued commit may publish after the renewal caller cancels. Caller end and durable publication are separate; token replies, grants, locators and raw third-party errors never become diagnostic bodies. |
| Export and shutdown | Requests enqueue without waiting. SDK queues, all-on sampling, spans, batches and HTTP timeouts are bounded; export does not retry. Queue/export/flush loss emits one body-free local warning per owner without affecting execution or recursively exporting it. Ingress and producers stop and required credential commits drain before bounded provider shutdown off async workers. Embedded owners stop producers before explicit shutdown; last-owner destruction never waits for export. |

Allowlist exit fields and Resources: closed reason, operation, trusted Agent/request identity, selected Provider/account/model, attempt and safe transport/delivery observations. Ordinary telemetry excludes credentials, provenance, raw URLs, headers, private paths, session IDs, business content and raw third-party errors. No ambient auth/Resource enrichment, automatic raw exception attributes, incoming baggage, upstream trace injection or required client echo. Private capture remains separate. Delayed client-response capture links to the request, not a stale attempt or ambient span; public adaptation notices and delivery fences are independent of export.

The receiver owns retention and durability; no application database, success-wire archive or usage ledger. Use the [loopback file receiver](../usage/gateway-configuration.md#loopback-file-receiver) for bounded JSONL or the [development query](../usage/gateway-configuration.md#indexed-development-query) for indexed synthetic evidence. Business metrics, tail sampling and production deployment are separate.

### Configurable exchange capture

Explicit `diagnostics.private_bodies` adds bounded sanitized business context to eligible WARN/ERROR logs, not a complete exchange archive. [Usage](../usage/gateway-configuration.md#private-failure-context) owns configuration and limits.

Four passive points observe client input, prepared upstream input, upstream output and polled client output. Each receives one quarter of the retained-text allowance and at most four recent safe units. Complete bounded JSON is sanitized before text extraction; SSE retains recent complete sanitized events. Truncation, incompleteness and omissions remain explicit. No parse-failed-to-raw fallback or argument reconstruction exists. Credential body fields and credential URL values are redacted before retention/export; headers and OAuth/token endpoints are excluded.

Upstream context resets per attempt; late observers cannot pollute another send. A correlated `gateway.failure.response` can add client-output context after the main request diagnostic sequence ends without a completion barrier, prefetch or terminal wait. Disabled mode retains no diagnostic bodies. Standalone tasks leave failed-head bodies unread and provide metadata-only coverage; a finite error-body read policy remains unselected. Raw malformed/HTML/plain-text capture also remains unselected.

Current diagnostics do not provide complete success-wire archives or exact live-attempt attestation. Offline directional fixtures, SDK consumers and test-owned upstream hooks retain independent protocol/count assertions; live scripts report observed client outcomes/counts only. Never publish, commit, print, replay or promote real private context into protocol fixtures.

