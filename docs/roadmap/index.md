# Development Roadmap

This page owns current scope, planned execution sequence, stage priorities, and release/integration gates. Known limitations and qualification gaps belong to [Unscheduled issues](unscheduled-issues.md); paused and deferred work belongs to [Backlog and frozen scope](backlog-and-frozen.md).

[Architecture](../architecture/index.md) owns accepted design, [API expansion design](../proposals/index.md) owns unselected proposals, and [Development](../development.md) owns shared development conventions and overall acceptance status. Code and tests define behavior. Completed slices are removed; Git preserves history.

## Contents

- [Current status](#current-status)
- [Planned order](#planned-order)
  - [Execution sequence and work-package rules](#execution-sequence-and-work-package-rules)
  - [Observability prerequisite](#observability-prerequisite)
    - [Delivery sequence](#delivery-sequence)
    - [Completion criteria](#completion-criteria)
  - [Stage 4 scope](#stage-4-scope)
    - [Decision API closure](#decision-api-closure)
    - [Stage-4 operation readiness](#stage-4-operation-readiness)
    - [Grok Imagine adaptation](#grok-imagine-adaptation)
  - [Stage 5 backlog](#stage-5-backlog)
- [Test supplementation](#test-supplementation)
  - [Stage-4 reuse and acceptance](#stage-4-reuse-and-acceptance)
- [Phase rules](#phase-rules)
  - [Agent authorization and diagnostics follow-up](#agent-authorization-and-diagnostics-follow-up)
  - [Temporary routing freeze](#temporary-routing-freeze)
  - [IR and codec migration](#ir-and-codec-migration)
  - [Verification and integration boundary](#verification-and-integration-boundary)
- [Integration handoff](#integration-handoff)
  - [Partial-delivery exit](#partial-delivery-exit)
  - [Public route activation review](#public-route-activation-review)

## Current status

Stateful upstream/downstream APIs remain [experimental and disabled](../architecture/history-and-replay.md#stateful-history). Their acceptance is excluded from current model adaptation and repairs until explicit user activation; no other work package enables them implicitly.

The selected [observability prerequisite](#observability-prerequisite) precedes
further Stage-4 work. Its SDK/request foundation is implemented; the full
observable acceptance gate remains open. Existing static carriers
do not activate new public APIs.

Other limitations and prerequisites share the [issue register](unscheduled-issues.md#unscheduled-issues); R09 real-account failover, remaining model-service qualification and Agent session storage retain separate scheduling decisions and are not implicit Stage-4 prerequisites.

[Development](../development.md#overall-acceptance-status) owns overall acceptance.

No deployment or new API activation is implied.

| Priority | Remaining work | Exit |
|---|---|---|
| Before Stage 4 | Deliver the selected [logs-and-traces slice](#observability-prerequisite) | All eight [completion criteria](#completion-criteria) observed and verified |
| Conditional tool follow-ups | [Conditional Stage-3 follow-ups](unscheduled-issues.md#conditional-stage-3-follow-ups) | Separate policy selection and affected-loop acceptance; not default Stage-4 prerequisites |
| API expansion | Select and implement Stage-4 operation contracts | Per-operation lifecycle, protocol and SDK acceptance |
| Downstream integration | Opt-in [per-Agent JSONL session storage](../architecture/history-and-replay.md#agent-session-files); unscheduled and not a Stage-4 prerequisite | Select isolation and lifetime bounds before implementation |

## Planned order

Complete the [observability prerequisite](#observability-prerequisite), then
resume Stage-4 contract selection and implementation. Stage-4 families retain
equal priority.
Other [unscheduled issues](unscheduled-issues.md#unscheduled-issues), including R09,
do not become implicit prerequisites. Conditional tool packages keep their own
policy gates; Stage 5 remains unscheduled.

| Stage | Goal | Exit evidence |
|---|---|---|
| **Before Stage 4 — Observability** | Correlated structured logs and traces across existing runtime boundaries, with a development query path | The eight [observable completion criteria](#completion-criteria); no production deployment or new API activation |
| **3 — Conditional compatibility follow-ups** | [Conditional packages](unscheduled-issues.md#conditional-stage-3-follow-ups) | Separate policy selection and affected-loop verification; not default Stage-4 or optional Agent blockers |
| **4 — Modalities, Decision & file resources** | Decision, Embedding extensions, Audio (Speech/Transcription/Translation), Images, Files/Uploads, and Video at equal priority | Verified request/result contracts, media bounds, resource isolation, cancellation, and SDK loops |
| **5 — Unscheduled follow-ups** | Citations, tool discovery, hosted tools, server-side compaction, Batch, and Realtime | Requires explicit per-slice design selection and scheduling prior to implementation |

### Execution sequence and work-package rules

Prefer small, independently decidable repairs before complex integration; diagnostic global → protocol → Provider/model layers are not a mandatory work queue or cross-product sweep. Reuse existing schema, lowering, execution, typed history and isolated current-client drivers. Every selected work package follows this lifecycle:
1. **Entry:** Pin source revisions, dialect rules, target profiles, and unresolved decisions. Document protocol findings in `docs/refs/`.
2. **Delivery:** Implement one coherent request/result/stream/history slice. Resolve unselected structures with a minimal counterexample and options before proceeding.
3. **Invariants:** Maintain immutable source IR and fresh per-attempt decoder state. Native/equivalent history projections preserve earlier turns; selected lossy projections record rendered-prefix/cache changes under the [candidate-local contract](../architecture/adaptation-and-dialects.md#candidate-local-projection).
4. **Evidence:** Provide independent directional tests and focused failure assertions at the responsible layer. Classify each field's ladder level and give rejections their L5 reason.
5. **Exit:** Run the offline baseline and relevant SDK gates before commit and push. Remove completed roadmap packages and record decisions in [Architecture](../architecture/index.md).

### Observability prerequisite

**Status:** in progress; the SDK/request foundation is implemented, not the full
prerequisite. Correlated server spans and Logs, body lifetime, isolated
subscribers and producer/provider shutdown are covered by focused synthetic
checks. Attempt/client spans, maintenance tracing, manual-timeline replacement
and indexed development-backend queries remain pending. None of the eight
criteria is closed solely by this foundation.

This section owns this slice's
scope, execution order and completion gate; [Architecture](../architecture/runtime-and-routing.md#selected-logs-and-traces-design)
owns the accepted design. Current temporary diagnostics remain described in
[Usage](../usage/gateway-configuration.md#failure-diagnostics).

Deliver correlated structured logs and traces for ingress/admission, Generation
planning and same-target attempts, upstream response consumption, downstream body
delivery, OAuth renewal/storage, startup/shutdown and existing standalone-task
execution. Keep protocol/semantic transformations pure. Reuse official
OpenTelemetry SDKs, bridges, processors and Collector facilities rather than a
custom tracer, event bus, buffering pipeline or diagnostic store.

The slice includes a test-owned development query path, not a production
observability platform. Business metrics, tail sampling, success-payload archives,
new client correlation requirements and external trace propagation are outside
this slice. Existing routing, credential, stateful-API and modality freezes remain
unchanged. Persistent service deployment and new external data recipients retain
their separate authorization requirements.

#### Delivery sequence

Each row is an independently verifiable implementation slice, not a new framework
or a required client/model/protocol cross-product.

| Slice | Main owners | Deliverable and local exit |
|---|---|---|
| Foundation vertical slice | Cargo pins, telemetry composition root, `src/gateway/startup/`, `src/bin/morphiecore.rs`, diagnostics SDK wiring | Pin compatible released SDK/bridge sources; one Responses request produces correlated logs and spans visible in a development backend; disabled export creates no exporter; embedded users/tests do not compete for global subscribers |
| Request and stream lifecycle | `src/gateway/ingress/`, `generation/{exchange,intake}.rs`, `body.rs`, `src/transport/http.rs` and response consumers | Cover admission, selection, actual attempts, body consumption and handoff; normal, decode-failure, retry and cancellation paths end and attribute correctly |
| Background and existing task coverage | `src/gateway/renewal.rs`, `src/credential/manager/refresh.rs`, shared `src/gateway/tasks/` exchange and CLI entry points | Correlate renewal, verification and durable commits; cover existing task I/O without changing modality contracts; preserve CLI JSON stdout |
| Migration and simplification | `src/gateway/diagnostics/`, runtime configuration and affected Usage/Architecture | Replace redundant manual clocks, sequence and timeline machinery with standard spans/events; retain adaptation-notice facts and private-capture boundaries; leave one logging/export path rather than permanent dual writes |
| Operational acceptance | Collector examples, development queries and focused lifecycle/privacy/exporter checks | Demonstrate all eight completion criteria below; distinguish synthetic, SDK, live-service and deployment evidence |

Request-lifecycle and background/task slices may proceed independently after the
foundation. Remove old diagnostic machinery only when its relevant failure and
cancellation coverage is preserved. Update only the current configuration schema
and examples; do not introduce historical aliases or migrations.

#### Completion criteria

All eight results are mandatory for this prerequisite. Approval of this plan,
successful compilation, or screenshots alone do not establish completion. Reuse
existing body, renewal, diagnostics and capture scenarios; add maintained tests
only for actual correlation, lifetime, safety or exporter gaps.

| # | Observable result | Verification boundary |
|---|---|---|
| 1 | A failure log identifies its logical request, responsible attempt when one exists, and failure boundary in the query backend | SDK in-memory assertions verify trace/span correlation and closed error attribution; a development-backend query verifies navigation from the exported log to the trace |
| 2 | Upstream headers, first projected content, terminal validation, EOF validation and downstream handoff have distinct timing/meaning | Controlled streaming scenarios verify their order and request/attempt attribution; response headers or handler return do not end body-consuming spans; first frame is not mislabeled TTFT |
| 3 | A recovered same-target retry preserves the initial failure while recording the correct final request result | Existing synthetic upstream authority verifies actual sends and target identity; exported attempt outcomes distinguish retry planned from executed without treating send invocations as exact upstream executions |
| 4 | Consumer cancellation and late stream errors cannot fabricate successful completion | Body-drop, late-truncation and terminal/EOF scenarios verify exactly-once observed endings consistent with existing delivery arbitration; local handoff never claims remote receipt |
| 5 | Renewal timeout/cancellation and a subsequent durable credential commit remain separately observable and correlated | Existing queued-commit/rotation scenarios verify context across async/blocking tasks and commit lifetime through storage drain; observations agree with independent persisted-state checks |
| 6 | Collector outage or queue saturation does not change business behavior, and possible telemetry loss has a safe local indication | Test-owned unavailable/slow receivers and SDK limits exercise nonblocking bounded export and shutdown; exporter diagnostics are body-free and cannot recursively enter their own export pipeline |
| 7 | Final exported logs/spans contain no forbidden sensitive fields | Synthetic secret canaries exercise normal, failure, bridge and background paths; inspect emitted records and resources, not only call-site fields; private capture remains separately opted in and credential-redacted |
| 8 | Enabling or disabling instrumentation preserves wire behavior, cancellation, publication fences and history replay | Reuse affected gateway/SDK and lifecycle oracles with instrumentation on/off; independent outputs and state, not telemetry itself, establish behavioral equivalence |

Run the [offline baseline and affected gates](../development.md#verification-gates)
before publishing implementation. Keep verification proportional to changed
contracts; this gate requires no broad Provider sweep or telemetry-text snapshots.
Record remaining uncertainty in the existing issue register without marking an
unverified criterion passed. Resume Stage 4 only after all eight criteria are
verified; later metrics, backend expansion and production deployment are not
additional implicit gates. Remove this completed slice from the roadmap while
retaining its architectural decisions.

### Stage 4 scope

After the [observability prerequisite](#observability-prerequisite), Stage 4 API
families have equal priority and advance as their contracts are selected:

| Package | Scope | Exit criteria |
|---|---|---|
| **S4-0 Per-operation design** | Pin official contracts, select targets, define resource lifecycles, and apply the conversion ladder to each family's fields | Explicit contract decisions classifying fields by ladder level, without speculative shared abstractions |
| **S4-1 Embedding extensions** | Select remaining extensions to the [independent Embedding contract](../proposals/tasks-and-counting.md#embeddings); existing implemented behavior is not pending | Independently verified selected input/result contracts, bounds and applicable SDK acceptance |
| **S4-6 Decision closure** | Complete dual-SDK design, typed answers, and synchronous execution | Verified independent oracles, both SDK loops (TypeSafe and OpenAI), and bounded Provider evidence |
| **S4-2a Speech** | Text-to-speech with static binary and optional streaming delivery | Bounded buffering, cancellation safety, and verified audio format carriage |
| **S4-2b Transcription** | Audio-to-text recognition with multipart upstream preparation | Accurate text/timing carriage, format rejection, and bounded resource handling |
| **S4-2c Translation** | Audio-to-text translation as a distinct native operation | Preserved language intent, output format validation, and direct execution |
| **S4-3 Images** | Generate, Edit, and preview/final streaming (internal sequence: Generate first) | Selected defaults, format encoding, mask boundaries, and artifact identity |
| **S4-4a Files** | File storage, public ID authorization, and content access | Isolated namespaces, bounded spooling, deletion lifecycle, and durable mappings |
| **S4-4b Multipart Uploads** | Chunked upload sessions, part ordering, and cleanup | Ordered part assembly, cancellation cleanup, and bounded memory use |
| **S4-4c File-reference integration** | Connect resolved file resources to dependent task inputs | Target-scoped references, stale reference rejection, and direct-input fallback |
| **S4-5 Video** | Async video generation, job status tracking, and artifact access | Bounded polling, status reconciliation, cancellation, and artifact delivery |

#### Decision API closure

Selected direction: Dual-SDK support (TypeSafe and OpenAI) backed by a unified Decision IR.
1. Pin SDK schemas and resolve differences in discovery, error envelopes, and URL handling.
2. Complete capability union review: evidence representation, rubrics, confidence metrics, and refusal behavior.
3. Define bounded synchronous execution profile and input validation.
4. Validate both official SDK clients against gateway endpoints.

#### Stage-4 operation readiness

| Family | Readiness | Next step |
|---|---|---|
| Decision | Dual-SDK direction selected; IR drafts under review | Finalize contract selection and implement codecs |
| Embedding extensions | Existing independent path implemented; additional extensions unselected | Select a concrete extension contract |
| Images Generate | Grok static codec implemented; public contracts under review | Select standard versus explicit native-output delivery, encoding defaults separately from Base64/URL carriage, and required result metadata |
| Images Edit & Stream | Research complete; mask boundaries and stream formats unselected | Await static Generate closure |
| Audio (Speech/ASR) | Static speech/transcription and bounded client multipart intake exist; Aliyun speech consumes upstream SSE. Native multipart upstream preparation and downstream SSE profiles remain unselected | Select bounded single-operation profile |
| Files & Uploads | Safety requirements defined; storage authority unselected | Select storage authority and ID authorization |
| Video | Upstream API research available; job lifecycles unselected | Select job status and retrieval contract |

#### Grok Imagine adaptation

Before Grok public Generate activation, select public delivery, encoding defaults and metadata requirements under the [static Generate review](../proposals/multimodal.md#static-generate-review-proposal). Native ratio/resolution controls remain independently selectable. OAuth must use the shared account source without inline login. Video integration requires a native asynchronous job contract rather than a synchronous facade.

### Stage 5 backlog

All Stage-5 work remains unscheduled. [Detailed proposals](../proposals/stage5-backlog.md) own citations, tool discovery/web search, hosted execution, compaction, Batch and Realtime designs; inclusion is not implementation or activation approval.

## Test supplementation

The test strategy uses pinned upstream assets and standard SDK consumers rather than hand-maintained mock matrices.

### Stage-4 reuse and acceptance

1. Identify applicable official SDK fixtures for each selected operation.
2. Verify MorphieCore request preparation and response decoding against upstream test cases.
3. Preserve the full offline baseline while adding operation-specific lifecycle and safety checks.

## Phase rules

### Agent authorization and diagnostics follow-up

- **Session design:** Opt-in [per-Agent JSONL session storage](../architecture/history-and-replay.md#agent-session-files) is unscheduled; select boundary isolation and lifetime bounds before implementation.
- **Deferred scope:** Dynamic OAuth authorization servers, cross-agent token exchange, and cluster IAM remain unscheduled.

### Temporary routing freeze

The [accepted account contract](../architecture/runtime-and-routing.md#failover) and [routing decisions](../architecture/decisions.md#routing-and-security-decisions) constrain future work. Provider access and normal OAuth renewal follow [project authorization](../../AGENTS.md#authorization); further implementation grants no additional credential or deployment permissions.

The [stable routing baseline](../architecture/runtime-and-routing.md#failover) remains frozen unless its own change is explicitly selected. Unrelated codec and protocol work must not alter:
- Account membership, health probes, or target ordering.
- Conversation cache affinity or cooldown thresholds.
- Route selection based on projected loss or codec capability.

### IR and codec migration

- Preserve immutable source IR across retries and failover attempts.
- Enforce strict JSON parsing and bounded resource limits on all decoders.
- Require independent directional expectations for all new codec mappings.
- Maintain honest terminal reporting: errors and cancellations must never fabricate success.

### Verification and integration boundary

| Milestone | Requirements | Permitted actions |
|---|---|---|
| Development slice | Focused owner/codec tests pass; regressions isolated | Continue authorized implementation within its scope; unrelated work stays under the routing freeze |
| Commit and push | Full [offline baseline](../development.md#offline-baseline) passes for implementation; [documentation checks](../development.md#documentation-checks) for docs only | Commit completed slices; publication follows project authorization |
| Integration acceptance | Connected SDK scenarios and client loops pass | Declare client/protocol compatibility |

## Integration handoff

Carrier availability in codecs does not imply public route admission. Route admission is controlled by explicit target binding policy in [topology compilation](../../src/topology/compile.rs).

### Partial-delivery exit

Under the partial-delivery exit: principal workflows are usable, selected low-loss conversions record their losses, affected paths are verified, and remaining difficulties have explicit scope for next-stage analysis.

1. Preserve representative standard tool/history paths across Haiku Messages, DeepSeek Chat and Grok Responses, plus the existing Qwen Responses coding path. DeepSeek Chat follows the [runtime-selected thinking-off or pre-dispatch rejection contract](../architecture/history-and-replay.md#binding-reasoning-history-contract), not a private readable profile. Use installed pi/OpenCode for affected client boundaries; reuse still-applicable evidence rather than requiring every client × model × protocol combination.
2. Verify each selected repair before claiming it complete. A new ambiguity may defer its dependent subcase without delaying unrelated slices.
3. Apply required offline and applicable SDK gates before publishing implementation. Recheck changed real workflows, including honest incomplete/cancelled delivery and continuation where affected. No broad re-sweep is required merely to preserve test counts.
4. Classify each remaining observation as resolved, a scoped limitation, next-stage analysis, or an unavailable prerequisite. A recorded deferral is sufficient disposition; closing every R identifier is not an exit condition.
5. Unscheduled issues may be selected alongside relevant Stage-4 work; it does not make Stage 5 features or production activation implicit.

### Public route activation review

Prerequisites for activating newly implemented codec carriers on public routes:
1. Classify each field on the conversion ladder: exact, equivalent, approximate, omitted or rejected.
2. Verify preflight/preparation agreement on candidate bindings.
3. Validate routing stability under failure and reconnection.
4. Complete offline baseline and connected SDK acceptance before client validation.

