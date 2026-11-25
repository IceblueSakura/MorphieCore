# MorphieCore Agent Instructions

## Project and sources of truth

- MorphieCore hides Provider differences behind one typed IR (`crates/morphiecore-semantic`). `pi` and OpenCode are the primary clients. Routing is request-boundary cache-affine selection with at most one pre-publication same-target retry (no post-dispatch target switching or load balancing).
- [README](README.md#documentation-map) is the single documentation map. Code and tests describe current behavior, not authority over official protocols; see [trusted catalog inputs](docs/architecture/index.md#shared-provider-metadata) for current Providers, models, and bindings.

## Subtree guides (read before editing matching paths)

Because some Agent harnesses only auto-load this root file, explicitly read the matching subtree guide(s) below before inspecting or editing inside that scope:

| Scope | Guide | When to read |
|---|---|---|
| Rust workspace | [crates/AGENTS.md](crates/AGENTS.md) | Any edit under `crates/` or `src/` |
| Codecs & lowering | [crates/morphiecore-protocol/AGENTS.md](crates/morphiecore-protocol/AGENTS.md) | Codecs, dialects, field rules, conversion ladder, or `crates/morphiecore-lowering/` |
| Gateway & runtime | [crates/morphiecore-gateway/AGENTS.md](crates/morphiecore-gateway/AGENTS.md) | Ingress, routing, failover, delivery, `crates/morphiecore-runtime/`, or `src/bin/` |
| Tests & harnesses | [tests/AGENTS.md](tests/AGENTS.md) | Adding/pruning tests, test harnesses, SDK/client suites, or live Provider checks |
| Documentation | [docs/AGENTS.md](docs/AGENTS.md) | Any edit under `docs/` or `README.md` |

## Security

- **Credential isolation:** Access credentials only through the project loader. Never inspect `.env`, private `config/*` files (outside `config/*.example.*`), OAuth files, local credential stores, or third-party auth caches as investigation/test input, and never print environment values.
- **Payload & fixture hygiene:** Keep credentials, private config, and business payloads out of source, public fixtures, and tool output. [Private failure context](docs/architecture/runtime-and-routing.md#configurable-exchange-capture) is opt-in, bounded, and always auth-redacted before capture; ordinary telemetry stays body-free. Private capture is never permission to publish payloads or use them as fixtures.

## Authorization

- **Git workflow:** Work on `main`; preserve unrelated changes. Stage only files belonging to your slice (`git add <paths>`, never `git add -A` over unrelated work). Commit each completed implementation or documentation slice after the applicable verification passes. Pushes to `origin/main` may batch multiple commits.
- **Phase boundaries:** Approved phases allow large deletions and breaking rewrites without compatibility shims. Non-goals live in [Not planned](docs/roadmap/index.md#not-planned); keep routing within [accepted failover boundaries](docs/architecture/runtime-and-routing.md#failover).

| Action | Policy |
|---|---|
| Provider requests for the task (configured API-key or OAuth accounts via project loader) | **Pre-authorized** — choose targets, calls, and recovery as needed without spending caps or fixed token, elapsed-time, or request-count budgets; report counts and retries visibly |
| Normal OAuth maintenance (auto renewal, manual refresh, pending-renewal verification, rotated persistence via project manager) | **Pre-authorized** |
| Initial login, reauthorization, permission expansion, account/API-key replacement or removal, logout/revocation | **Requires explicit approval** |
| Deployment or external publication beyond `origin/main` | **Requires explicit approval** |

## Working approach and global boundaries

- **Roadmap alignment:** Check [Roadmap current status](docs/roadmap/index.md#current-status), [Planned order](docs/roadmap/index.md#planned-order), and [Not planned](docs/roadmap/index.md#not-planned) before starting feature or refactor work. Follow the [partial-delivery exit](docs/roadmap/index.md#partial-delivery-exit) and record uncertain cells in [Unscheduled issues](docs/roadmap/unscheduled-issues.md#unscheduled-issues).
- **Stateful API freeze:** Upstream and downstream stateful APIs are experimental test features until the user explicitly enables them; exclude their acceptance from model adaptation and issue repairs. Reject downstream storage, reference-based continuation, and stored-resource operations while keeping omitted/null/false `store` stateless and preserving full-history replay. [Stateful boundary](docs/architecture/history-and-replay.md#stateful-history) owns the contract.
- **Proactive design ownership & coherent slices:** Trace actual client workflows, primary protocol sources, and existing implementations; challenge weak premises and own choices within agreed scope (show a minimal counterexample and recommendation for unresolved semantic/structural decisions while continuing independent work; research never reopens [Not planned](docs/roadmap/index.md#not-planned) scope). Develop small, coherent slices and state consequential goals/exit conditions in the roadmap without turning every change into a test-first ritual.
- **Repository hygiene & language:** Write repository documentation, code comments, and commit messages in concise English. Never hand-edit build caches or commit private `config/*` files, `.venv`/`node_modules`, or `testdata/{generated,reports,dist,runtime}/`.

## Offline baseline commands

Doc-only edits need [documentation checks](docs/development.md#documentation-checks) only (`docs/AGENTS.md`); conditional SDK/client test commands live in `tests/AGENTS.md`. Before pushing code changes, run:

```sh
cargo fmt --check
cargo clippy --all-targets --locked --offline -- -D warnings
cargo check --lib --bins --locked --offline
cargo test --locked --offline
python3 tests/upstream/assets.py
python3 -m unittest discover -s tests/upstream
nix develop .#docs --offline --command python3 tests/check_docs.py
git diff --check
```
