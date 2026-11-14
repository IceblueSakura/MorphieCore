# MorphieCore Agent Instructions

## Project and sources of truth

- MorphieCore hides Provider differences behind one typed IR (`src/semantic`). `pi` and OpenCode are the primary clients. Routing is request-boundary cache-affine selection with at most one pre-publication same-target retry (no post-dispatch target switching or load balancing).
- [README](README.md#documentation-map) is the single documentation map. Code and tests describe current behavior, not authority over official protocols; read [providers.json](src/provider/providers.json), [topology catalog](src/topology/catalog.rs), and [startup bootstrap](src/gateway/startup/bootstrap.rs) for current Providers, models, and bindings.

## Security

- **Credential isolation:** Access credentials only through the project loader. Never inspect `.env`, private `config/*` files (outside `config/*.example.*`), OAuth files, local credential stores, or third-party auth caches as investigation/test input, and never print environment values.
- **Payload & fixture hygiene:** Keep credentials, private config, and business payloads out of source, public fixtures, and tool output. [Private failure context](docs/architecture/runtime-and-routing.md#configurable-exchange-capture) is opt-in, bounded, and always auth-redacted before capture; ordinary telemetry stays body-free. Private capture is never permission to publish payloads or use them as fixtures.
- **Runtime trust invariants:** Preserve loopback-only ingress, fail-closed auth, and trusted target selection (business data never selects origins, credentials, or auth/proxy headers; credential locators and trusted origins never reach clients). Enforce resource and time bounds, release upstreams on cancellation, never replay after publication, and never fabricate successful termination.

## Authorization

- **Git workflow:** Work on `main`; preserve unrelated changes. Stage only files belonging to your slice (`git add <paths>`, never `git add -A` over unrelated work). Commit and push completed implementation or documentation slices to `origin/main` after the verification below passes.
- **Phase boundaries:** Approved phases allow large deletions and breaking rewrites without compatibility shims. [Frozen scope](docs/roadmap/backlog-and-frozen.md#frozen-scope) and the [temporary routing freeze](docs/roadmap/index.md#temporary-routing-freeze) permit necessary maintenance only.

| Action | Policy |
|---|---|
| Provider requests for the task (configured API-key or OAuth accounts via project loader) | **Pre-authorized** — choose targets, calls, and recovery as needed without spending caps or fixed token, elapsed-time, or request-count budgets; report counts and retries visibly |
| Normal OAuth maintenance (auto renewal, manual refresh, pending-renewal verification, rotated persistence via project manager) | **Pre-authorized** |
| Initial login, reauthorization, permission expansion, account/API-key replacement or removal, logout/revocation | **Requires explicit approval** |
| Deployment or external publication beyond `origin/main` | **Requires explicit approval** |

## Working approach

- **Roadmap alignment:** Check [Roadmap current status](docs/roadmap/index.md#current-status), [Planned order](docs/roadmap/index.md#planned-order), and [Frozen scope](docs/roadmap/backlog-and-frozen.md#frozen-scope) before starting feature or refactor work. Follow the [partial-delivery exit](docs/roadmap/index.md#partial-delivery-exit) and record uncertain cells in [Unscheduled issues](docs/roadmap/unscheduled-issues.md#unscheduled-issues).
- **Dependencies & upstream pins:** When introducing or upgrading protocols, dialects, or dependencies, pin current stable dependencies and release sources first and integrate upstream-derived standard tests alongside the rewrite. Union IR design does not implicitly activate new public APIs.
- **Proactive design ownership:** Trace actual client workflows, primary protocol sources, and existing implementations; challenge weak premises and recommend simpler designs with concrete trade-offs. Own choices within agreed scope; for unresolved semantic or structural decisions, show a minimal counterexample and a recommendation while continuing independent work. Research does not reopen frozen scope.
- **Coherent slices:** Develop small, coherent slices. State consequential behavior goals and exit conditions in the roadmap, matching investigation/prototyping/verification sequence to uncertainty instead of turning every change into a test-first ritual or a new framework.

## Engineering

- **Gateway/runtime boundary:** Apply [system boundaries](docs/architecture/index.md#system-boundaries) — reject target limitations requiring execution-strategy changes rather than silently disabling thinking or repairing a session.
- **Layer purity:** `src/semantic` is the official-protocol semantic union (one owner per fact) with zero protocol, Provider, target, credential, retry, or I/O dependencies. `src/protocol`, `src/lowering`, and `src/adapter` are pure in-memory transformations (no I/O, credentials, or adaptation-publication state); protocol spelling and raw wire stay in codecs, never vendor bags or duplicate semantic containers.
- **Field handling & conversion ladder:** Keep known fields strict and discard unknown client extension fields without IR retention or Provider passthrough ([field handling](docs/architecture/adaptation-and-dialects.md#field-handling)).

  Follow the [conversion ladder](docs/architecture/adaptation-and-dialects.md#conversion-ladder): admit by default without requiring prior equivalence evidence, delegate unknown upstream behavior to upstream validation instead of pre-rejecting, and reject only when omitting an inexpressible field would let the client mistake the response for satisfying its request.

  Never weaken trust, bounds, malformed-known-value checks, identities, or terminal honesty.

  Lowering a content field or explicit guarantee below rejection requires explicit user selection; same-semantic and lossy rules need a named rule and disposition-table entry (not per-rule confirmation).
- **Public baselines & diagnostics:** Public APIs default to official baselines (no public replay profiles or client-specific semantic retention extensions); Provider dialects remain upstream-only. Diagnostic output is optional, uses protocol-permitted extension positions, and never requires client echo.
- **Stateful API freeze:** Upstream and downstream stateful APIs are experimental test features until the user explicitly enables them. Exclude their acceptance from model adaptation and issue repairs. Reject downstream storage, reference-based continuation and stored-resource operations; omitted/null/false `store` remains stateless. Preserve full-history replay and advisory cache affinity. [Stateful boundary](docs/architecture/history-and-replay.md#stateful-history) owns the contract.
- **Direct code & current schema:** Prefer existing solutions and direct code over speculative abstractions (keep routing within [failover boundaries](docs/architecture/runtime-and-routing.md#failover)). Maintain only the current application configuration schema and paths — add no historical aliases, migrations, obsolete-field detection, or old-schema rejection tests, while keeping ordinary strict parsing and safety checks. Update `Cargo.lock` for dependency changes and verify with `--locked`.
- **Naming & repository hygiene:** Name code, fixtures, and tests by protocol or behavior, not roadmap phases or package IDs. Write repository documentation, code comments, and commit messages in concise English. Never hand-edit build caches or commit private `config/*` files, `.venv`/`node_modules`, or `testdata/{generated,reports,dist,runtime}/`.

## Verification and tests

- **Authoritative test policy:** Follow [Development](docs/development.md#verification-gates) and [test selection and pruning](docs/development.md#test-selection-and-pruning).
- **Real-service functional checks:** Prefer direct functional checks over bespoke harnesses. Do not impose fixed token, elapsed-time, or request-count budgets unless the user requests them or the check specifically tests those controls; keep ordinary application/Provider safety limits and cancellation intact.
- **Host client & baseline rules:** Adapt production client boundaries and Agent acceptance only to the host's current latest `pi`, OpenCode, and other Agents, using isolated test-owned state and safe unwrapped launchers (no old-version compatibility branches, downgrades, or historical version matrices). Historical mapping expectations are not the regression oracle.
- **High-signal tests only:** Keep tests that independently falsify a product assumption, public contract, or concrete external failure risk (preserve credential/auth, lifecycle/cancellation, bounds, origin, strict-JSON, replay-isolation, and terminal-honesty assertions).

  Aggressively delete implementation restatements, static catalog/config snapshots, trivial accessor/assembly checks, third-party self-tests, and redundant permutations together with exclusive helpers.

  Protocol mappings need independent wire→IR and IR→wire expectations — a round trip alone is not evidence; negative cases must reach their intended failure.
- **Gates & honest reporting:** Iterate with focused offline checks and reuse valid results. Before pushing code changes, run the [offline baseline](docs/development.md#offline-baseline) (in the [flake](flake.nix) environment) and applicable roadmap gates; doc-only edits need [documentation checks](docs/development.md#documentation-checks) only. Keep offline, SDK (`tests/sdk_loopback.rs`, `#[ignore]`), installed-client (`tests/clients.rs`), and real-Provider results separate, reporting exact checks, failures, and unverified contracts.
- **Offline baseline commands:**
  ```sh
  cargo fmt --check
  cargo clippy --all-targets --locked --offline -- -D warnings
  cargo check --lib --bins --locked --offline
  cargo test --locked --offline
  python3 tests/upstream/assets.py
  python3 -m unittest discover -s tests/upstream
  git diff --check
  ```
  *(When affected, also run `tests/sdk/.venv/bin/python -m unittest discover -s tests/gateway`, `tests/sdk/.venv/bin/python -m unittest discover -s tests/sdk`, and `python3 -m unittest discover -s tests/clients`.)*

## Documentation

- **Single authoritative home:** Follow [README](README.md#documentation-map) and keep one authoritative owner per topic; link instead of duplicating specs or maintaining parallel plans. Separate proposed design, accepted decisions, implementation status, and acceptance evidence (a structural edit grants no approval).
- **No ephemeral archives:** Keep [Development](docs/development.md) as a concise overall acceptance summary without run archives, historical run classifications, execution dates, tested-version inventories, acceptance commands, or per-run evidence catalogs. Do not retain phase-specific review pages or add run reports/history pages.
- **Lifecycle & domain organization:** Organize each document by domain with shared rules above specialized branches. Keep unresolved issues in [Unscheduled issues](docs/roadmap/unscheduled-issues.md#unscheduled-issues), accepted adaptations in [Architecture](docs/architecture/index.md), and dated research notes indexed in [References](docs/references.md#research-notes). Remove finished slices from [Roadmap](docs/roadmap/index.md) and record meaningful decisions in [Settled decisions](docs/architecture/decisions.md).
