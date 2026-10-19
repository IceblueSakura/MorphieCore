# MorphieCore Agent Instructions

## Project and sources of truth

- MorphieCore hides Provider differences behind one typed IR. pi and opencode are the primary clients. Routing is cache-affine failover, not load balancing.
- [README](README.md#documentation-map) is the document entry point. [Architecture](docs/architecture.md) owns accepted principles and decisions; [Roadmap](docs/roadmap.md) owns scope, execution dependencies and gates; [API expansion design](docs/api-expansion-design.md) owns unselected proposals; [Usage](docs/usage.md) and [Development](docs/development.md) own procedures. Code and tests define behavior; read catalogs and startup code for current Providers, models and bindings.

## Working approach

- Follow the [five-stage plan](docs/roadmap.md#planned-order): same-protocol lossless IR codecs; basic agent tool loops; cross-protocol stability plus `count_tokens`; Embedding/audio/image/video APIs and Files/multipart Uploads; then unscheduled citations, tool discovery, hosted tools, server-side compaction, Batch and Realtime. Stage 5 is a backlog, not a scheduled delivery commitment.
- Investigate proactively: trace actual client workflows, primary protocol sources and existing implementations. Challenge weak premises, compare alternatives and recommend simpler designs with concrete trade-offs.
- Own implementation choices within the agreed scope. Explore and propose architectural improvements; when a semantic or structural decision is unresolved, show a minimal counterexample and a recommendation for user selection. Continue independent work meanwhile. Research does not reopen frozen implementation scope.
- Develop small, coherent slices. State consequential behavior goals and exit conditions in the roadmap, but choose the investigation, prototyping and verification sequence that fits the uncertainty. Do not turn every change into a test-first ritual or a new framework.

## Authorization

- Work on `main`; preserve unrelated changes. Commit and push completed slices to `origin/main` after the verification below. Deployment, other publication and credential lifecycle operations require explicit approval.
- Development-related real Provider calls are authorized without per-call confirmation or a spending cap. Fix the target, request count, output cap and timeout before each run; run serially without hidden retries, using the project loader and redacted reporting only.
- Approved phases allow large deletions and breaking rewrites without compatibility shims. Frozen implementation gets only necessary maintenance.

## Security

- Access credentials only through the project loader. Never inspect `.env`, private configuration, OAuth files or third-party auth caches as investigation/test input, or print environment values.
- Keep credentials, private config and business payloads out of source, fixtures, logs and tool output. Only explicitly enabled [private exchange capture](docs/architecture.md#configurable-exchange-capture) may retain business headers/bodies; authentication is always redacted before capture.
- Preserve loopback-only ingress, fail-closed auth and trusted target selection. Business data cannot choose origins, credentials or auth/proxy headers; credential locators and trusted origins never reach clients. Bound resources and time, release upstreams on cancellation, and never replay after publication or fabricate successful termination.

## Engineering

- `src/semantic` stays independent of protocols, Providers and execution. Keep runtime targets, credentials and retry state outside the IR; codecs and lowering are pure. Protocol-specific fields and raw wire handling belong in codecs, not IR request bags.
- Keep known upstream fields strict and ignore unknown ones. Reject unrepresentable critical semantics; use named, recorded rules for approved degradations. Follow [Adaptation](docs/architecture.md#adaptation).
- Prefer existing solutions and direct code over speculative abstractions. Add checks for concrete failures and remove obsolete code. Update `Cargo.lock` for dependency changes and verify with `--locked`.
- Use concise English for documentation, comments and commits. Do not inspect/edit build caches by hand or commit `testdata/{generated,reports,dist,runtime}/`.

## Tests

- Keep valuable assertions, not a test-count target. Prefer focused checks at the responsible layer and extend existing tests over duplicate cases or matrices. Follow the roadmap's phase-specific scope.
- Complete the selected Generation IR/codec design and obtain explicit human review approval before the ordinary comprehensive coverage stage. The [current roadmap](docs/roadmap.md#generation-design-review-gate) records explicit authorization to expand audit coverage before design approval; this does not approve unresolved designs or integration.
- Protocol mappings need independent wire→IR and IR→wire expectations; a round trip alone is not evidence. Check that a negative case reaches its intended failure, not an earlier rejection.
- Preserve credential/auth, lifecycle/cancellation, bounds, origin, strict-JSON, replay-isolation and terminal-honesty assertions. Remove obsolete tests/helpers with deleted code; use [coverage and pruning](docs/development.md#coverage-and-pruning) for deliberate pruning passes, not routine edits.

## Verification

- Use focused offline checks while iterating; reuse valid evidence instead of rerunning the suite after every edit. Before publishing an implementation slice, run the [full offline baseline](docs/development.md#offline-baseline) and applicable roadmap gates. Documentation-only changes need [documentation checks](docs/development.md#documentation-checks), not Rust/SDK runs.
- Keep SDK, real-client and Provider acceptance separate from offline regression. Report actual checks, failures and unverified contracts; compilation, coverage percentages and synthetic success do not prove full-chain behavior.

## Documentation

Keep the existing document set: `README.md`, `AGENTS.md`, `docs/{architecture,roadmap,usage,development,references}.md`, the [API expansion design](docs/api-expansion-design.md), `docs/openapi.json`, and sourced, dated protocol notes in `docs/refs/`. Do not add run reports or history pages. Remove finished roadmap slices and record meaningful decisions in the architecture decision table.

Organize each document by domain with shared rules above specialized branches. Keep one authoritative home per topic and link to it instead of maintaining duplicate plans or specifications. Separate proposed design, accepted decisions, implementation status and acceptance evidence; a structural edit grants no approval. Preserve source dates in research notes and use [References](docs/references.md#research-notes) as their index.
