# MorphieCore Agent Instructions

## Project and sources of truth

- MorphieCore hides Provider differences behind one typed IR. pi and opencode are the primary clients. Routing is cache-affine failover, not load balancing.
- [README](README.md#documentation-map) is the document entry point. [Architecture](docs/architecture.md) owns accepted principles and decisions; [Roadmap](docs/roadmap.md) owns scope, execution dependencies and gates; [API expansion design](docs/api-expansion-design.md) owns unselected proposals; [Usage](docs/usage.md) and [Development](docs/development.md) own procedures. Code and tests describe current behavior, not authority over official protocols; read catalogs and startup code for current Providers, models and bindings.

## Working approach

- Follow the [standards reset and accepted breaking redesign](docs/roadmap.md#standards-reset-and-redesign) before further stage-4 implementation. Pin current stable dependencies and release sources first; integrate upstream-derived standard tests while rewriting, not after restoring old mappings. The [five-stage plan](docs/roadmap.md#planned-order), deferred counting and unscheduled stage 5 remain; union design does not activate new APIs.
- Investigate proactively: trace actual client workflows, primary protocol sources and existing implementations. Challenge weak premises, compare alternatives and recommend simpler designs with concrete trade-offs.
- Own implementation choices within the agreed scope. Explore and propose architectural improvements; when a semantic or structural decision is unresolved, show a minimal counterexample and a recommendation for user selection. Continue independent work meanwhile. Research does not reopen frozen implementation scope.
- Develop small, coherent slices. State consequential behavior goals and exit conditions in the roadmap, but choose the investigation, prototyping and verification sequence that fits the uncertainty. Do not turn every change into a test-first ritual or a new framework.

## Authorization

- Work on `main`; preserve unrelated changes. Commit and push completed slices to `origin/main` after the verification below. Deployment, other publication and credential lifecycle operations require explicit approval.
- Development-related real Provider calls are authorized without per-call confirmation or a spending cap. Fix the target, request count, output cap and timeout before each run; run serially without hidden retries, using the project loader and redacted reporting only.
- Approved phases allow large deletions and breaking rewrites without compatibility shims. Frozen implementation gets only necessary maintenance.

## Security

- Access credentials only through the project loader. Never inspect `.env`, private configuration, OAuth files or third-party auth caches as investigation/test input, or print environment values.
- Keep credentials, private config and business payloads out of source, public fixtures and tool output. The single deployer owns every downstream client and authorizes sensitive business headers/bodies in explicitly enabled, bounded [private exchange capture](docs/architecture.md#configurable-exchange-capture). Authentication is always redacted before capture; ordinary telemetry stays body-free. Private capture is not permission to publish payloads or use them as protocol fixtures.
- Preserve loopback-only ingress, fail-closed auth and trusted target selection. Business data cannot choose origins, credentials or auth/proxy headers; credential locators and trusted origins never reach clients. Bound resources and time, release upstreams on cancellation, and never replay after publication or fabricate successful termination.

## Engineering

- `src/semantic` is the official-protocol semantic union, with one authoritative owner per fact and no protocol, Provider or execution dependency. Keep targets, credentials, retries and adaptation-publication state outside it; codecs/lowering are pure. Protocol spelling and raw wire belong in codecs, not vendor bags or duplicate semantic containers.
- Keep known fields strict and ignore unknown ones. Apply [exact-first ordered fallback](docs/architecture.md#accurate-ir-and-declared-loss): lossless mapping, bounded approximation, meaningful simple estimates, otherwise rejection. Revisit prior conversions; report actual non-rejection losses without weakening identities, explicit guarantees, visibility or terminal honesty. Public APIs use official baselines, not Provider/MorphieCore dialects.
- Prefer existing solutions and direct code over speculative abstractions. Add checks for concrete failures and remove obsolete code. Update `Cargo.lock` for dependency changes and verify with `--locked`.
- Name code, fixtures and tests by protocol or behavior, not roadmap phases or package IDs. Keep scheduling identifiers in planning and acceptance documentation.
- Write all documentation, comments and commits in concise English. Do not inspect/edit build caches by hand or commit `testdata/{generated,reports,dist,runtime}/`.

## Tests

- Keep valuable assertions, not a test-count target. Prefer focused checks at the responsible layer and extend existing tests over duplicate cases or matrices. Follow the roadmap's phase-specific scope.
- The [standards-reset design is human-approved](docs/roadmap.md#generation-design-review-gate). Implement its integrated upstream-source test plan with each slice; unresolved concrete semantics and new integration still need their own selection. Historical mapping expectations are not the regression oracle, even if previously approved.
- Protocol mappings need independent wire→IR and IR→wire expectations from current official contracts and pinned upstream assets; a round trip alone is not evidence. Prefer thin fixture adapters and real SDK consumers over repeatedly rewritten local snapshots. Retain small local safety/IR assertions and check that negative cases reach their intended failure.
- Preserve credential/auth, lifecycle/cancellation, bounds, origin, strict-JSON, replay-isolation and terminal-honesty assertions. Remove obsolete tests/helpers with deleted code; use [coverage and pruning](docs/development.md#coverage-and-pruning) for deliberate pruning passes, not routine edits.

## Verification

- Use focused offline checks while iterating; reuse valid evidence instead of rerunning the suite after every edit. Before publishing an implementation slice, run the [full offline baseline](docs/development.md#offline-baseline) and applicable roadmap gates. Documentation-only changes need [documentation checks](docs/development.md#documentation-checks), not Rust/SDK runs.
- Keep SDK, real-client and Provider acceptance separate from offline regression. Report actual checks, failures and unverified contracts; compilation, coverage percentages and synthetic success do not prove full-chain behavior.

## Documentation

Keep the existing document set: `README.md`, `AGENTS.md`, `docs/{architecture,roadmap,usage,development,references}.md`, the [API expansion design](docs/api-expansion-design.md), `docs/openapi.json`, and sourced, dated protocol notes in `docs/refs/`. Do not add run reports or history pages. Remove finished roadmap slices and record meaningful decisions in the architecture decision table.

Organize each document by domain with shared rules above specialized branches. Keep one authoritative home per topic and link to it instead of maintaining duplicate plans or specifications. Separate proposed design, accepted decisions, implementation status and acceptance evidence; a structural edit grants no approval. Preserve source dates in research notes and use [References](docs/references.md#research-notes) as their index.
