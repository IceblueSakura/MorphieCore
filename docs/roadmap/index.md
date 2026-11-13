# Development Roadmap

This page owns current scope, planned execution sequence, stage priorities, and release/integration gates. Known limitations and qualification gaps belong to [Unscheduled issues](unscheduled-issues.md); paused and deferred work belongs to [Backlog and frozen scope](backlog-and-frozen.md).

[Architecture](../architecture/index.md) owns accepted design, [API expansion design](../proposals/index.md) owns unselected proposals, and [Development](../development.md) owns shared development conventions and overall acceptance status. Code and tests define behavior. Completed slices are removed; Git preserves history.

## Contents

- [Current status](#current-status)
- [Planned order](#planned-order)
  - [Execution sequence and work-package rules](#execution-sequence-and-work-package-rules)
  - [Stage 4 scope](#stage-4-scope)
    - [Decision API closure](#decision-api-closure)
    - [Stage-4 operation readiness](#stage-4-operation-readiness)
    - [Grok Imagine adaptation](#grok-imagine-adaptation)
  - [Stage 5 backlog](#stage-5-backlog)
- [Test supplementation](#test-supplementation)
  - [Stage-4 reuse and acceptance](#stage-4-reuse-and-acceptance)
- [Phase rules](#phase-rules)
  - [Observability follow-up](#observability-follow-up)
  - [Agent authorization and diagnostics follow-up](#agent-authorization-and-diagnostics-follow-up)
  - [Temporary routing freeze](#temporary-routing-freeze)
  - [IR and codec migration](#ir-and-codec-migration)
  - [Verification and integration boundary](#verification-and-integration-boundary)
- [Integration handoff](#integration-handoff)
  - [Partial-delivery exit](#partial-delivery-exit)
  - [Public route activation review](#public-route-activation-review)

## Current status

The Generation review baseline has converged under the [partial-delivery exit](#partial-delivery-exit). Configuration/diagnostic cleanup, maintenance simplification, and lifecycle convergence are delivered under [Observability](../architecture/runtime-and-routing.md#observability), while remaining production observability, reliability, and privacy gates are deferred in the [cleanup plan](../convergence-cleanup-plan.md). Accepted contracts live in [Architecture](../architecture/index.md); overall acceptance status is summarized in [Development](../development.md#overall-acceptance-status). Residual limitations and prerequisites share the [issue register](unscheduled-issues.md#unscheduled-issues), without a separate review repair queue. Current work prioritizes Provider/model catalog review and model-list expansion. The selected OpenRouter Luna Responses SSE intake issue must close before Stage 4 resumes. R09 real-account failover and Agent session storage remain outside that entry gate. No deployment or new API activation is implied.

| Priority | Remaining work | Exit |
|---|---|---|
| Current focus | Review the compiled Provider/model catalog and expand selected Generation model bindings | Exact upstream IDs and source-backed target contracts; offline baseline and affected qualification |
| Before Stage 4 | Resolve [OpenRouter Luna Responses SSE intake](unscheduled-issues.md#content-and-client-qualification) | First failing event/error localized, independent regression and corrected native-to-IR-to-downstream stream acceptance |
| Conditional tool follow-ups | [S3-T5–S3-T7](unscheduled-issues.md#conditional-stage-3-follow-ups): namespace aliases, custom-text wrappers and examples projection | Separate policy selection and full affected-loop acceptance; not default handoff prerequisites |
| API expansion | Select and implement Stage-4 operation contracts | Per-operation lifecycle, protocol and SDK acceptance |
| Downstream integration | Opt-in [per-Agent JSONL session storage](../architecture/history-and-replay.md#agent-session-files); unscheduled and not a Stage-4 prerequisite | Select isolation and lifetime bounds before implementation |
| Production preparation | Select real diagnostic receiver/storage and deferred cluster observability scope | Explicit deployer selection; cleanup implementation does not deploy a Collector or backend |

## Planned order

The Generation review baseline is delivered under the [partial-delivery contract](#partial-delivery-exit). First review the Provider/model catalog and expand selected model bindings. Before resuming Stage 4, close the explicitly selected [OpenRouter Luna Responses SSE intake issue](unscheduled-issues.md#content-and-client-qualification). Other [unscheduled issues](unscheduled-issues.md#unscheduled-issues), including R09, do not become implicit prerequisites. Once this entry gate closes, Stage-4 families retain equal priority and advance as contracts are selected. Conditional tool packages keep their own policy gates; Stage 5 remains unscheduled.

| Stage | Goal | Exit evidence |
|---|---|---|
| **3 — Conditional compatibility follow-ups** | [S3-T5–S3-T7](unscheduled-issues.md#conditional-stage-3-follow-ups): namespace aliases, plain custom wrappers and examples projection | Separate policy selection and affected full-loop verification; not default blockers for Stage 4 or optional Agent qualification |
| **4 — Modalities, Decision & file resources** | Decision, Embedding extensions, Audio (Speech/Transcription/Translation), Images, Files/Uploads, and Video at equal priority | Verified request/result contracts, media bounds, resource isolation, cancellation, and SDK loops |
| **5 — Unscheduled follow-ups** | Citations, tool discovery, hosted tools, server-side compaction, Batch, and Realtime | Requires explicit per-slice design selection and scheduling prior to implementation |

### Execution sequence and work-package rules

Prefer small, independently decidable repairs before complex integration; diagnostic global → protocol → Provider/model layers are not a mandatory work queue or cross-product sweep. Reuse existing schema, lowering, execution, typed history and isolated current-client drivers. Every selected work package follows this lifecycle:
1. **Entry:** Pin source revisions, dialect rules, target profiles, and unresolved decisions. Document protocol findings in `docs/refs/`.
2. **Delivery:** Implement one coherent request/result/stream/history slice. Resolve unselected structures with a minimal counterexample and options before proceeding.
3. **Invariants:** Maintain immutable source IR and fresh per-attempt decoder state. Native/equivalent history projections preserve earlier turns; selected lossy projections record rendered-prefix/cache changes under the [candidate-local contract](../architecture/adaptation-and-dialects.md#candidate-local-projection).
4. **Evidence:** Provide independent directional tests and focused failure assertions at the responsible layer. Classify each field's ladder level and give rejections their L5 reason.
5. **Exit:** Run the offline baseline and relevant SDK gates before commit and push. Remove completed roadmap packages and record decisions in [Architecture](../architecture/index.md).

### Stage 4 scope

Stage 4 API families have equal priority and advance as their contracts are selected:

| Package | Scope | Exit criteria |
|---|---|---|
| **S4-0 Per-operation design** | Pin official contracts, select targets, define resource lifecycles, and apply the conversion ladder to each family's fields | Explicit contract decisions classifying fields by ladder level, without speculative shared abstractions |
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
| Images Generate | Grok static codec implemented; public contracts under review | Select public default encoding (PNG vs Base64) and response metadata |
| Images Edit & Stream | Research complete; mask boundaries and stream formats unselected | Await static Generate closure |
| Audio (Speech/ASR) | Static paths exist; streaming and multipart unselected | Select bounded single-operation profile |
| Files & Uploads | Safety requirements defined; storage authority unselected | Select storage authority and ID authorization |
| Video | Upstream API research available; job lifecycles unselected | Select job status and retrieval contract |

#### Grok Imagine adaptation

Before public Generate activation, resolve dimensions, quality tiers, creation timestamps, default encoding and response metadata using the existing static codec. OAuth must use the shared account source without inline login. Video integration requires a native asynchronous job contract rather than a synchronous facade.

### Stage 5 backlog

All Stage 5 tasks are currently unscheduled:
- **Citations & source attribution:** Unified citation models ([1C proposal](../proposals/stage5-backlog.md#1c-design-messages-citations)).
- **Tool discovery:** Dynamic tool discovery and native `web_search`.
- **Hosted tool execution:** Separate contracts for Provider-executed tools and observations; ordinary declarations, aliases and custom-text adaptation remain in the Stage-3 tool work.
- **Server-side compaction:** Provider-executed `/responses/compact` operations.
- **Batch:** Asynchronous job processing and reconciliation.
- **Realtime:** Bi-directional streaming sessions, interruption, and media transport.

## Test supplementation

The test strategy uses pinned upstream assets and standard SDK consumers rather than hand-maintained mock matrices.

### Stage-4 reuse and acceptance

1. Identify applicable official SDK fixtures for each selected operation.
2. Verify MorphieCore request preparation and response decoding against upstream test cases.
3. Preserve the full offline baseline while adding operation-specific lifecycle and safety checks.

## Phase rules

### Observability follow-up

Application-side [failure diagnostics](../architecture/runtime-and-routing.md#observability), bounded private body context and custom-file removal are delivered. Real receiver/backend activation, centralized cluster topology, public diagnostic schema stabilization and metrics aggregation still await production packaging.

Safe-to-ignore diagnostic extensions may be added without dedicated extension tests. That exception does not waive the selected logger/capture replacement's behavior, redaction, bounds, cancellation and exporter checks. Further aggregation remains deferred; existing standard Agent-loop and authoritative evidence checks remain required.

### Agent authorization and diagnostics follow-up

- **Session design:** Opt-in [per-Agent JSONL session storage](../architecture/history-and-replay.md#agent-session-files) is unscheduled; select boundary isolation and lifetime bounds before implementation.
- **Deferred scope:** Dynamic OAuth authorization servers, cross-agent token exchange, and cluster IAM remain unscheduled.

### Temporary routing freeze

The [accepted account contract](../provider-account-redesign-plan.md) and [routing decisions](../architecture/decisions.md#routing-and-security-decisions) constrain future work. Further implementation does not authorize changing deployed credentials or activation.

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

