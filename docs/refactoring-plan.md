# MorphieCore Refactoring Plan

This plan tracks the code-first refactor: enforce layer boundaries, move model policy out of protocol code, then simplify tests and documentation. The initial status audit covered source at `29781e2`; Phase 2 also tracks the subsequent implementation. A landed change is not a full acceptance pass. The [roadmap](roadmap/index.md) owns scope and release gates; [architecture](architecture/index.md) owns accepted contracts; [development](development.md) owns verification policy. This plan does not activate APIs or change [frozen scope](roadmap/backlog-and-frozen.md#frozen-scope).

## Status and next slice

| Phase | Status | Remaining work |
| --- | --- | --- |
| 0. Cleanup | Landed, with the scope corrections below | No blocker for Phase 2 |
| 1. Dependency boundaries | Landed | No blocker for Phase 2 |
| 2. Projection and catalog | Complete for the selected scope; offline baseline and stateless SDK loopbacks passed | Installed-client and real-Provider qualification remain separate |
| 3. Tests and observability | Test pruning landed; the observability gate remains open | Complete instrumentation and the evidence gaps below |
| 4. Documentation | No task complete; routes already match by inspection | Add independent gates and consolidate documentation after code boundaries settle |

## Phase 0 — Cleanup (landed)

| Task | Commit | Verified result |
| --- | --- | --- |
| 0.1 | `9c643ef` | Removed the direct production `ring` dependency; signature tests use `aws-lc-rs` and hashing uses `sha2`. The resolved production graph has no `ring` on the audit host; `Cargo.lock` still lists it for transitive dependencies. |
| 0.2 | `c422cb6` | Removed the dormant Responses storage implementation, eight ignored tests and the three `resources.history_*` options. The authenticated stateless boundary remains: reject valid stateful options and explicit stored-resource operations; emit `store: false` and `previous_response_id: null` on stateless Responses. Malformed known fields still fail validation. |
| 0.3 | `4cfba48` | Removed the Grok Imagine codec and adapter and the `opencode-go-step-chat` and `openrouter-ember-{chat,responses}` bindings. `openrouter-step` still serves `step-5-preview`; `muse-ember` is gone. The `GptImage` and standard Speech/Embeddings variants were **not** removed and still have test callers. Their removal is not a Phase 2 prerequisite. |
| 0.4 | `b0306c6` | Deleted the two bespoke Messages live scripts and their meta-tests. The [Rust bootstrap test](../tests/gateway/bootstrap.rs) uses a synthetic upstream returning HTTP 503, asserts the Messages overload response (529), and checks CLI `--validate --json` with test-owned configuration. |

Operators who explicitly set `resources.history_entries`, `resources.history_bytes` or `resources.history_ttl_ms` must remove those keys from their own configuration. Unknown keys are still rejected. `morphiecore --validate --json` checks a configured gateway's local assembly, not upstream connectivity. No compatibility aliases or migrations are planned.

## Phase 1 — Dependency boundaries (landed)

| Task | Commit | Verified result |
| --- | --- | --- |
| 1.1 | `4f193da` | Broke cross-layer dependencies: HTTP transport owns the upstream request type, execution classifies failures, credential code owns subscription renewal, and protocol owns codecs, adapters and shared SSE framing. |
| 1.2 | `b28c733` | Established the Workspace and extracted `semantic` and generic test support. |
| 1.3 | `20da4fb` | Split the remaining five production layer libraries. The transitional request projection and `TargetContract` still belong to `protocol`; Phase 2 moves only protocol-neutral policy into `lowering`. |
| 1.4 | `29781e2` | Moved conformance into [the gateway integration suite](../tests/gateway/conformance.rs) and split `tests/support/` into generic support and a gateway harness. The old cross-directory `#[path]` reuse is gone. |

[Cargo.toml](../Cargo.toml) defines **nine packages** and selects all nine by default: six production libraries, the root package, and two test packages. The normal production order is `semantic` ← `lowering` ← `protocol` ← `catalog` ← `runtime` ← `gateway` ← root; higher layers can also depend directly on lower ones. `semantic` has no async/network stack. The root package keeps the three auto-discovered binaries and their integration tests so `CARGO_BIN_EXE_*` remains available. `test-support` has no MorphieCore dependencies; `test-harness` uses it and the production layers. Root integration tests opt into `test-hooks` where needed. The normal/build graph rooted at the production package has neither test package nor `proptest`; a workspace-wide graph includes the test packages as roots and cannot establish this isolation.

## Phase 2 — Projection and embedded catalog

### Task 2.1 — Share request and delivery disposition rules

[Typed dispositions](../crates/morphiecore-lowering/src/disposition.rs) retain the same rule identities for projection and discovery. [Target capabilities](../crates/morphiecore-lowering/src/capabilities.rs) carry neutral limits, reasoning, output, tool and sampling policy. Provider restrictions and codec-specific validation remain separate from permitted lossy conversions. Tool schema and structured-output rejections retain distinct ownership.

### Task 2.2 — Move protocol-neutral request projection into `lowering`

[Semantic request projection](../crates/morphiecore-lowering/src/request.rs) owns configuration collapse, controls and output ceilings, ordered tools, cache markers and reasoning receipts. Its inputs are semantic requests, dispositions and typed policies, not envelopes or model labels. [Protocol preparation](../crates/morphiecore-protocol/src/lowering/request.rs) interleaves carrier admission, grouping, trusted replay transfer and context handling in the original order. Existing directional tests distinguish native rejection from candidate-local omission, and cover tool order, replay and named losses.

### Task 2.3 — Group wire rules and share SSE mechanics

[OpenAI wire rules](../crates/morphiecore-protocol/src/openai/dialect/adaptation.rs) group request, dialect hooks, reasoning, accounting, stream and metadata behavior. Typed chunk metadata policy retains distinct timestamp/fingerprint constraints. [Native Bailian model declarations](../crates/morphiecore-protocol/src/openai/dialect/bailian/models.rs) serve both the native codec and resolved target preparation, including configured upstream overrides. [Bounded SSE intake](../crates/morphiecore-protocol/src/stream.rs) shares framing, byte/frame accounting and strict EOF across all five Generation decoders. Each codec still validates its own terminal and trailer; fragmentation and exact-budget tests exercise all five.

### Task 2.4 — Replace hard-coded catalog declarations

[Embedded topology data](../crates/morphiecore-catalog/src/topology/catalog/topology.json) replaces Rust inventories for models, Generation bindings, binding effort mappings, service limits and standalone tasks. [Typed initialization](../crates/morphiecore-catalog/src/topology/catalog/data.rs) checks references, operation/auth consistency and strict field grammar before compilation. `--catalog` reads the compiled routes; transcription retains ordered bindings with explicit selection when ambiguous. The one-time compiled-topology and CLI comparison matched the Rust-declared baseline. A temporary standard model compiled and appeared in `--catalog` with JSON-only changes; it was removed after verification. Maintained tests cover invalid references rather than freezing production inventory.

### Task 2.5 — Tighten execution and delivery boundaries

[Standalone admission and exchange](../crates/morphiecore-gateway/src/tasks/lifecycle.rs) share request lifetime, permits, deadlines and bounded receive. Operation codecs still own multipart, binary and JSON transformations and their publication differences. [Response delivery](../crates/morphiecore-runtime/src/execution/delivery.rs) separates envelope metadata, projection and actual JSON/SSE modes; the gateway still owns handoff and cancellation. The [bounded cache successor](../crates/morphiecore-protocol/src/adapter/downstream.rs) remains unchanged, including its completion fence and exclusion of Messages. Direct IR concatenation has not established rendered-prefix equivalence; it is not required for this phase.

## Phase 3 — Test pruning and observability

### Task 3.1 — Keep independent test oracles

- The dialect sweep uses native client/target pairs; targeted cross-protocol lanes still exercise dispatch, replay, and carrier-specific behavior. Model-limit tests keep binding-local ceilings, same-model isolation, and synthetic no-default controls. Shared SSE helpers retain independent argument deltas and reject data after the Messages terminal. The SIWC quota case checks the public error code, pre-creation failure and redaction.

### Task 3.2 — Complete request and maintenance instrumentation

- **Current:** [Gateway diagnostics](../crates/morphiecore-gateway/src/diagnostics.rs) has request, operation and attempt spans but still resets flat attempt state. [HTTP transport](../crates/morphiecore-runtime/src/transport/http.rs) returns at headers without a client span. [Generation intake](../crates/morphiecore-gateway/src/generation/intake.rs) already tracks actual projected-content readiness for recovery, not telemetry. [Credential refresh storage](../crates/morphiecore-runtime/src/credential/manager/refresh.rs) already drains queued commits but does not carry tracing context into blocking work.
- **Remaining:** Group request identity, attempt and handoff state by lifetime. Parent sanitized client I/O to the active Generation or standalone attempt and keep body-consuming spans alive through actual completion or cancellation. Record the first projected content separately from headers, terminal, EOF and handoff; a controlled test needs separate content and terminal events. Carry renewal context into OAuth HTTP and blocking storage prepare/commit, including delayed commits after cancellation. Capture context before the work begins; a span created after prepare cannot account for a prepare timeout. Avoid empty maintenance traces and never emit credential, URL, path or payload fields.
- **Constraint:** Keep the pinned OTLP `reqwest-blocking-client` and existing bounded, nonblocking drop behavior. The standard batch workers do not run inside Tokio; switching exporter clients or processors is not part of this slice.
- **Exit:** Focused gateway/runtime tests exercise parentage, body lifetime, milestone ordering and blocking commit correlation.

### Task 3.3 — Verify the eight observability criteria

The [roadmap gate](roadmap/index.md#completion-criteria) remains open. Reuse the synthetic OTLP receiver, [bootstrap export tests](../tests/gateway/bootstrap.rs), and existing body, retry and renewal scenarios rather than introducing a second harness. Existing assertions are foundations, not closure:

| # | Existing coverage in source | Evidence still needed |
| --- | --- | --- |
| 1 | Synthetic export correlates a failure with its attempt | Navigate from exported log to attempt and request in the development query backend |
| 2 | Server-span body lifetime and existing milestones | First projected content and attempt-parented client spans through body consumption |
| 3 | Same-target sends and retained retry diagnosis tested separately | Correlate actual sends with exported attempt outcomes and distinguish planned from executed |
| 4 | Cancellation and late truncation behavior | Exactly-once observed endings without claiming downstream receipt |
| 5 | Queued-commit drain and independent persisted state | Timeout/cancel and durable commit observed in the same trace across blocking work |
| 6 | Stalled/unavailable exporter and nonblocking drop | Actual queue saturation, unchanged business output and a safe, non-recursive loss indication |
| 7 | Failure-path scan of exported logs and spans | Cross-path canaries for normal, failure, bridge, renewal and transport; include Resource attributes |
| 8 | Existing wire and lifecycle oracles | Paired diagnostics on/off for output, cancellation, publication fences and replay |

Use the existing Collector examples and verify the query path, not just JSONL output or an in-memory trace. Record uncovered criteria as open. Stage 4 stays gated on all eight.

## Phase 4 — Documentation and automated checks

### Task 4.1 — Reconcile the public contract

- **Current:** [OpenAPI](openapi.json) has 19 of 63 component schemas unreachable from `paths`: 16 future discovery schemas and three stored Responses schemas. Its 14 paths and 16 explicit operations already match [Axum registrations](../crates/morphiecore-gateway/src/ingress/http.rs) by inspection. The [Python schema suite](../tests/gateway/test_openapi_schema.py) checks reference validity but not reachability or exact route parity.
- **Remaining:** Delete unreachable schemas and update the [API reference](usage/api-reference.md); retain active discovery and explicit stateful denials. Add a transitive-reference check and two-way parity using an inventory tied to actual route registration; do not presume Axum can enumerate a built `Router`, or a probe of only documented routes can detect extras. Count explicit methods, not implicit `HEAD` or fallback behavior. Check the public example against the current schema and resource validation without resolving private credential state. An example can legitimately choose a non-default value.
- **Exit:** Run the OpenAPI Python checks and affected gateway route/config tests.

### Task 4.2 — Consolidate the document set

- Update architecture, usage, development and roadmap references to the implemented Workspace and catalog. Replace duplicated adapter prose with contract tables or links; keep structural decisions and their trade-offs, not per-model or per-PR changelogs.
- Give stateful rejection and observability one authoritative home each. Remove the completed observability roadmap slice only after Task 3.3 closes. Distinguish semantic maxima from gateway defaults, remove stale history options, and keep setup paths portable. The configuration guide already links the example templates and describes Logs and Traces; update what has drifted rather than rewriting those sections from scratch.
- Consolidate superseded research only after migrating useful pinned-source evidence and retargeting incoming links. The original reduction target is roughly half of the old document set; preserve authoritative constraints and evidence rather than meeting a line quota.
- **Exit:** Local links, heading anchors and whitespace pass; the docs describe current behavior and keep proposals distinct from accepted work.

### Task 4.3 — Add a lightweight documentation gate

- **Current:** [Manual documentation checks](development.md#documentation-checks) already apply; no tracked automated hygiene gate exists. Default Workspace membership already covers all packages.
- **Remaining:** Add an offline check for local Markdown links, heading anchors, template coverage, and the research index. Reuse an established checker if it meets the repository's offline requirements. Update `AGENTS.md` and `README.md` for final source locations and the check command. Do not require repeating routine model or dialect changes in architecture decisions.
- **Exit:** Check the full doc set with the new gate and include it in the documented baseline.

## Verification boundaries

- **Status audit:** Git history and current source establish what landed. Cargo metadata and locked offline dependency trees from the flake Rust shell verified membership, binaries and production dependency isolation. OpenAPI reachability was checked against the current JSON. No runtime, SDK, installed-client or Provider acceptance is claimed by this audit.
- **Implementation slices:** Run focused checks first, then the [offline baseline](development.md#offline-baseline) and applicable roadmap gates before publishing. Choose regression tests for the affected admission, projection, routing, wire, lifecycle or trust boundary.
- **Phase 2 and 3 milestones:** Use the [locked SDK loopback gate](development.md#sdk-loopback-gate). Check registered ignored cases with `cargo test --test sdk_loopback --locked --offline -- --ignored --list`, then run the affected stateless cases serially. Installed `pi`/OpenCode workflows and real Provider qualification remain separate evidence.
- **Documentation-only slices:** Check paths, anchors and whitespace. Add schema and route checks when the public contract changes; documentation-only edits do not require unrelated build or SDK runs.
