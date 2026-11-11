# Runtime, Routing, and Trust

This document defines failover state transitions, conversation/cache affinity, streaming normalization, reverse-proxy delivery, loopback credentials, cluster diagnostics, and private exchange capture.

See [Architecture Overview](index.md) for system boundaries and core IR semantics; [Protocol adaptation](adaptation-and-dialects.md) for wire mappings; and [Settled decisions](decisions.md) for accepted trade-offs.

## Contents

- [Runtime and trust](#runtime-and-trust)
  - [Failover](#failover)
    - [Legal candidates](#legal-candidates)
    - [Request conversion reuse](#request-conversion-reuse)
    - [Advancement](#advancement)
    - [Health and recovery](#health-and-recovery)
    - [Routing unit](#routing-unit)
  - [Conversation and cache affinity](#conversation-and-cache-affinity)
  - [Streaming and delivery](#streaming-and-delivery)
    - [Semantic stream normalization](#semantic-stream-normalization)
    - [Reverse-proxy delivery](#reverse-proxy-delivery)
    - [Optional delta coalescing](#optional-delta-coalescing)
  - [Credentials and security](#credentials-and-security)
  - [Agent identity and cluster diagnostics](#agent-identity-and-cluster-diagnostics)
    - [Identity and authentication](#identity-and-authentication)
    - [Shared history and usage attribution](#shared-history-and-usage-attribution)
  - [Observability](#observability)
  - [Configurable exchange capture](#configurable-exchange-capture)

## Runtime and trust

### Failover

Shared named-account binding, scoped health, material-aware auth probes, success-owned affinity and bounded same-target recovery implement the accepted [execution policy](../provider-account-redesign-plan.md#execution-policy). [Roadmap](../roadmap/unscheduled-issues.md#unscheduled-issues) tracks remaining profile qualification and prerequisites. Production deployment requires separate approval.

Failover operates on precompiled candidate lists under a cache-affine routing policy.

#### Legal candidates

Routes compile at startup (up to 64 candidates per model). Ingress requests cannot specify arbitrary candidate targets.

#### Request conversion reuse

Gateway preflight retains credential-free final bytes, protocol-header projections, bounded prefix digests and adaptation evidence together with the actual bound candidate. Executor ordering moves the whole record; preparation consumes it without reconstructing JSON or repeating semantic conversion. Public planning results remain metadata-only, and discovery uses the checked projection directly.

The checked projection owns an immutable wire value, its bounded serialized-size measurement and evidence. Retention admission, exact body allocation and eligible prefix hashing reuse that measurement instead of serializing again to count. Gateway materialization shares lookup eligibility with affinity selection and derives prefixes only for eligible initial lookups; direct preparation, disabled/explicit/unmapped lookup paths and already-hinted requests do not derive them.

The request-local retention budget is 1 MiB, charging body/header/digest allocation capacity and evidence including spare vector slots. Overflow leaves candidate admission and ordering unchanged: an attempted cache miss recomputes the existing pure conversion. Remaining records belong to the exchange future and are released on completion or cancellation, not retained in shared gateway state.

Preparation still validates the trusted binding, streaming and final body bounds before attaching credentials and account headers. Codecs validate header carriers during request encoding; materialization stores the projected headers as a plain vector rather than retaining a deferred error state. An affinity hint changes a typed candidate-local request and carries its new checked conversion; a hint that cannot fit leaves the original accepted body intact. The test-only forced-stream override likewise invalidates its body. No path patches encoded JSON or reparses it through generic `Value` deserialization, which could reinterpret ordinary reserved-key schema objects.

#### Advancement

The production model loader fixes one account target before dispatch. Configuration exposes no fallback switch, credential-member budget or route send ceiling; the old chain/transition internals are removed. One lease and one prepared request own the original send and at most one serial retry. The retry reuses the exact effective endpoint, credential material, headers and body, with a network-free lifecycle/material veto before sending.

Publication remains irreversible: no later attempt may replay committed output. Provider-owned finite classification separates health from the narrow resend allowlist. Typed transport causes distinguish positive interruption/connection timeout from TLS, local faults, unknown causes and ambiguous post-dispatch timeout; no private URL or third-party message enters policy. Unknown structured codes retain actual-status fallback and unknown in-band errors gain no synthetic-500 resend permission.

The short recovery clock covers cleanup, hints, preparation and the retry until projected deliverable content or a validated successful no-content terminal. Headers, usage and lifecycle frames cannot remove it. Buffered JSON remains private and terminal publication still requires validated EOF; readiness removes neither the original request deadline nor the attempt deadline. The [execution contract](../provider-account-redesign-plan.md#bounded-same-target-retry) owns budgets and eligibility.

#### Health and recovery

One exchange-local failure ledger retains the original HTTP observation, later interruption, material revision and previous-attempt evidence. Terminal cleanup resolves health and releases its lease before asynchronous downstream normalization.

- **Scoping:** Auth/quota constrain an account; transient, rate-limit and permission failures constrain its exact model/operation/origin/path target. Independent constraints retain their own deadlines.
- **Probe ownership:** Acquisition is atomic across account and target scopes. Account probes exclude sibling-model probes in either acquisition order; target probes alone do not exclude healthy sibling traffic. Cancellation releases ownership without declaring recovery.
- **Recovery:** Only validated successful work can clear health, subject to generation fencing. Reported failures and independently identified transport interruptions retain observation times and material revisions through cleanup or cancellation. Clean HTTP EOF missing a protocol terminal is malformed, not positive transport-interruption evidence. A completed retry discards its first failure's pending penalty; otherwise merged account/target constraints advance each affected scope at most once. Attempt-local output resets while usage and diagnostics remain separately attributable. Provider-owned timing and narrowly scoped material replacement follow the [selected contract](../provider-account-redesign-plan.md#cooldown-and-recovery).
- **Downstream hints:** Upstream reset evidence remains available to health/recovery. Returned `Retry-After` describes only a credible short route-wide block and is suppressed when another usable target is ready; `x-should-retry` is never emitted.

#### Routing unit

An explicit conversation/session carrier is the routing unit, scoped by authenticated Agent and public model. Only a valid binding earns one bounded cooldown wait; after waiting, lifecycle and probe ownership are rechecked before dispatch. Successful final delivery conditionally establishes affinity to the serving target.

### Conversation and cache affinity

Affinity keys and advisory cache policies are defined in [`protocol::cache`](../../src/protocol/cache.rs):
- `X-MorphieCore-Conversation-Id` establishes route affinity across multi-turn exchanges via hashed SHA-256 grouping keys.
- Prefix-cache indexing supplies advisory request hints, never an implicit conversation or account-routing key.
- Target cache capabilities are enforced via [`CacheTargetPolicy`](../../src/protocol/cache.rs#L48), omitting unsupported advisory hints under declared loss rules.

Bound replay issuers include configuration namespace, credential provenance and the exact public-model/target/operation identity. API-key replacement invalidates old private replay; normal OAuth material renewal preserves provenance. Codecs use the bound issuer for OpenAI, Messages and Google replay, not only prefix-cache keys.

### Streaming and delivery

Delivery lifecycle and event streaming are coordinated by [`ResponseDelivery`](../../src/execution/delivery.rs#L100) under [`SseLimits`](../../src/protocol/stream.rs#L8):
- **Pre-publication holding:** Bounded lifecycle-only frames wait for content while the one same-target retry remains possible.
- **Publication freeze:** Flushing frames irreversibly prohibits another send; subsequent upstream failures abort the stream with terminal error events.
- **Honest completion:** Streams require validated upstream EOF before downstream release.
- **Final handoff:** Final-frame acceptance and cancellation share one fence. Before acceptance, cancellation revokes delivery; afterward only synchronous completion checks, history/health/affinity settlement and diagnostics remain. Acceptance alone never makes a failed or neutral operation successful. Local Responses snapshot reads/deletes release their concurrency permit before returning immutable body bytes.

#### Semantic stream normalization

Codecs validate and reduce upstream events in source order, emitting legal target streams:
- Preserves delta ordering, call identities, and signature attachments.
- Emits block openers before deltas, and closes blocks before terminal events.
- Interleaving tool calls and text parts are serialized to conform to target grammar.

Responses opens with exactly one `response.created`, including when its actual status is `queued`. Queued does not establish semantic Started; only an observed `response.in_progress` starts a queued response. Encoding then emits progress without a second created event. The pinned [OpenAI SDK created-event contract](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/types/responses/response_created_event.py) admits a Response with queued status, and its [accumulator](https://github.com/openai/openai-python/blob/e8a0fd28bbdc1699c97ff0f602b1af3dd378382d/src/openai/lib/streaming/responses/_responses.py#L515-L524) requires created first. Queued EOF, cancellation and errors cannot fabricate progress or successful completion.

Messages downstream delivery uses one active wire block in semantic owner/part declaration order, independently of numeric IDs. The head streams incrementally; successors wait for its actual owner finish, including a delayed thinking signature. Thinking emits one signature immediately before block stop; redacted blocks keep their native shape. Reports retain their existing independent timing and authority.

The codec-local [scheduler](../../src/protocol/anthropic/messages/client/sse/schedule.rs) bounds retained payloads and container overhead, releases drained capacity, and rejects known unsupported content before queueing. [Frame construction](../../src/protocol/anthropic/messages/client/sse.rs) checks serialized size and cumulative wire/event budgets during drainage. Cancellation or rejection discards pending content. EOF-authorized incomplete termination may close unfinished text/tool wire blocks without inventing semantic completion; source reduction, argument validation and publication rules remain outside the scheduler. Native Messages intake is unchanged.

An observed incomplete text-item finish also authorizes closure of that owner's remaining wire blocks after their queued content drains. It preserves the source item status and unfinished part facts, matching existing static rendering and terminal-only closure. A completed item still requires actual part completion; block closure does not authorize a successful operation terminal.

#### Reverse-proxy delivery

MorphieCore operates behind a loopback-bound reverse proxy:
- SSE responses set `X-Accel-Buffering: no` and `Cache-Control: no-store`.
- The proxy terminates TLS and provides client compression; MorphieCore handles authentication and streaming logic.

#### Optional delta coalescing

Configured via [`DeltaCoalescing`](../../src/execution/delivery/coalescing.rs#L17) (default 50 ms / 16 KiB bounds):
- Merges consecutive adjacent `PartKind::Text` fragments into larger downstream frames.
- Flushes immediately upon non-text events, usage reports, or timer expiration.

### Credentials and security

- **Store:** Private files and redacted, zeroizing in-memory secret types; loaders reject unsafe paths. Provider documents hold typed OAuth/key maps, rejecting duplicate aliases and malformed known fields across both partitions. Publication moves and returns records without secret-cloning APIs, preserving shared locks, bounds, quarantine and atomic-write ordering. Only the current format is maintained: no historical filename detection or migration; generic invalid-directory rejection and reserved namespace protection remain.
- **Driver ownership:** Managers share an immutable driver registry. Each driver has one registration-aware login entrance and one renewal entrance; pending renewal still requires its separate identity verification and durable publication steps.
- **Browser login input:** Every browser login accepts HTTP callback and a full callback URL on stdin together; the first valid result wins at the shared listener. Stdin uses ordinary echoed lines, with no mode switch, hidden-input parser or partial-line coordination. Pasted URLs relay only to the prepared loopback target and share state/field validation, PKCE, issued-client binding, nonce, identity proof and the original deadline. No token import or second exchange is introduced. See [operation](../usage/credentials-and-diagnostics.md#oauth) and [implementation references](../refs/2026-11-02-account-recovery-sources.md#headless-browser-callback-input).
- **Automatic OAuth renewal:** Startup owns the deduplicated maintenance registry, including configured disabled accounts. Listener-owned renewal consumes that registry and scans tokens expiring within 120s, rotating credentials serially under account locks. [OAuth maintenance](../provider-account-redesign-plan.md#oauth-maintenance) owns filesystem-phase cancellation and replacement-verification checkpoints.
- **API-key access:** Startup binds a reference; borrowing enforces enabled state, quarantine, record/epoch/generation identity and material digest. No separate strict-binding policy is maintained.
- **Ingress Auth:** Binds exactly one configured Agent key via Bearer token or `x-api-key`.

### Agent identity and cluster diagnostics

#### Identity and authentication

- `agent_id`: Stable configured agent identity across key rotations.
- Authentication table maps keys to `agent_id` with constant-time verification.

#### Shared history and usage attribution

- Every request is tagged with an immutable Agent identity and runtime nonce.
- Usage is attributed to the executing Agent rather than the conversation owner.

### Observability

Configuration intake reports a domain and fixed safe reason, with a root location for schema/type errors. It does not traverse Serde paths or emit rejected values or private map keys. Semantic validation may retain its schema-owned field/index attribution.

Failure diagnostics use one runtime-owned native OpenTelemetry Logs provider, the standard bounded batch processor and official OTLP/HTTP JSON exporter. The receiver owns retention, indexing and optional durability. The application owns no diagnostic files, trace tree, successful-request archive or diagnostic usage ledger. Execution facts remain independent of optional export.

- **Population and severity:** recovered attempt failures and client/upstream 4xx rejection are WARN; unrecovered service, transport, decode and terminal failures are ERROR. HTTP-200 failed tasks use their source class. Ordinary cancellation/shutdown is not a default error; independently observed upstream failures survive it. Length stops, refusal, accepted adaptation and ignored fields do not generate warnings.
- **Attribution:** closed reason codes, original source/conversion boundary, trusted Agent/request identity, operation, selected Provider/account, public/upstream model and attempt. HTTP status alone does not assign blame. `http.response.status_code` is the observed upstream head; `gateway.error.status_code` is the selected local error presentation. `client.handoff_complete:true` records acknowledged local delivery, not remote receipt or inference success.
- **Lifecycle:** requests enqueue without waiting for export. Bounded SDK queue/batches, finite HTTP timeout and no export retries. Queue/export/flush problems produce fixed body-free stderr warnings without changing inference, routing, replay or credential state. Explicit flush runs off the async worker; last-owner destruction signals shutdown with no wait. Delivery and storage are best effort.
- **Maintenance:** OAuth renewal uses the same logger with trusted account/profile and a closed category. Token replies, grants, credential locators and raw third-party errors are never diagnostic bodies.

The public adaptation notice retains bounded semantic evidence across request intake and current-attempt/source/output views. Historical reports, diagnostic usage collection and per-frame/per-effect delivered counters are removed. Real oneshot acknowledgment, delivery commit, publication fences and history reservations remain execution responsibilities. Logs cannot prove execution counts; synthetic authorities and client outcomes are the acceptance oracle.

### Configurable exchange capture

Explicit `diagnostics.private_bodies` adds bounded sanitized business context to eligible WARN/ERROR logs, not a complete exchange archive. [Usage](../usage/gateway-configuration.md#private-failure-context) owns configuration and limits.

Four passive points observe client input, prepared upstream input, upstream output and polled client output. Each receives one quarter of the retained-text allowance and at most four recent safe units. Complete bounded JSON is sanitized before text extraction; SSE retains recent complete sanitized events. Truncation, incompleteness and omissions remain explicit. No parse-failed-to-raw fallback or argument reconstruction exists. Credential body fields and credential URL values are redacted before retention/export; headers and OAuth/token endpoints are excluded.

Upstream context resets per attempt; late observers cannot pollute another send. A correlated `gateway.failure.response` can add client-output context after the trace ends without a completion barrier, prefetch or terminal wait. Disabled mode retains no diagnostic bodies. Standalone tasks leave failed-head bodies unread and provide metadata-only coverage; a finite error-body read policy remains unselected. Raw malformed/HTML/plain-text capture also remains unselected.

Complete success-wire and exact live-attempt attestation are withdrawn. Offline directional fixtures, SDK consumers and test-owned upstream hooks retain independent protocol/count assertions; live scripts report observed client outcomes/counts only. Never publish, commit, print, replay or promote real private context into protocol fixtures.

