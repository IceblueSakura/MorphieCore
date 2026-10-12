# MorphieCore Agent Instructions

## Project and sources of truth

- A central gateway built on one unified IR that hides Provider differences from a few agents' loops: pi and opencode first, Codex and Claude Code second. Downstream: OpenAI Chat Completions and Responses, Anthropic Messages. Upstream: OpenAI Responses/Chat, Anthropic Messages, Google generateContent and Interactions. Routing is cache-affine failover across a model's Providers and keys, never load balancing.
- Design principles and settled decisions: [docs/architecture.md](docs/architecture.md). Direction, frozen scope and the current slice: [docs/roadmap.md](docs/roadmap.md).
- Code and tests define behavior; docs record only concepts, decisions and direction. For available Providers, models and bindings read `src/provider/catalog.rs`, `src/topology/catalog*` and `src/gateway/startup/bootstrap.rs`; never keep lists in docs or memory.

## Authorization

- Before editing, check the branch, `git status` and the target diff. Preserve others' changes; stop if they overlap yours.
- Work directly on `main`. Commit and push every slice that passes the offline baseline to `origin/main` (standing authorization). Without explicit approval: no deployment, no publishing elsewhere, no credential lifecycle operations (login, refresh, revoke, migrate).
- Real Provider calls needed for current development are authorized without per-call confirmation or a spending cap. Before each run, fix the target, request count, output cap and timeout; run serially with no hidden retries; obtain credentials only through the project loader; print only redacted status and counts.
- Within a roadmap-approved phase, large deletions and breaking rewrites are allowed; keep no compatibility shims. Frozen scope gets only the maintenance a refactor requires.

## Security

- Access credentials only through the project loader (default store: `src/credential/storage/directory.rs`). Never use `.env`, private `config/`, OAuth files or third-party auth caches as investigation or test input.
- Real secrets, private config, sensitive bodies and opaque values never appear in source, fixtures, logs or tool output. Never print environment variable values.
- Ingress listens on loopback only and authentication fails closed. Business data cannot select an upstream origin, credential, or auth/proxy header. Credential locators and trusted origins never reach clients.
- Memory, frames, events and time are all bounded. Cancellation releases the upstream. After commit: no replay, no fabricated success terminal.

## Engineering

- **One IR**: `src/semantic` does not depend on protocols, Providers or execution. Codecs and lowering are pure: no network, credentials or registry. Runtime targets, credentials and retry state stay out of the IR.
- **Protocol differences stay in protocol modules**: no Provider-specific IR fields, no raw JSON request bags. When the IR cannot express a concept, first show the user a minimal counterexample and structural options, then implement.
- **Parsing**: upstream responses are strict on known fields and ignore unknown ones. Downstream requests reject critical semantics they cannot express; degradable fields are removed by named rules and recorded (architecture §4).
- **Keep it simple**: no prebuilt generic frameworks, extra defensive layers or speculative abstractions. Delete code that is no longer needed instead of keeping old paths. Every new check or limit needs a concrete failure scenario.
- **Process**: for behavior changes, first state the goal and exit condition in the roadmap's current focus, then write tests first. Dependency changes update `Cargo.lock` and are verified with `--locked`.
- Docs, code comments, docstrings and commit messages use concise English. Comments explain only non-obvious reasons, with source URLs when useful. Non-ASCII test inputs are data, not prose.
- Never inspect or edit `target/`, `.venv/` or `__pycache__/` by hand; never commit `testdata/{generated,reports,dist,runtime}/`.

## Tests

The suite is deliberately lean. Keep it that way; do not regrow it.

- One positive and one negative case per contract, at the lowest responsible layer. No variant matrices, exhaustive presence combinations, or per-Provider/per-model duplicates.
- Before adding a test, check that no same-layer test already asserts it; extend or replace an existing test rather than adding a near-copy.
- Protocol mappings need independently hand-written wire→IR and IR→wire expectations; a round trip alone is not evidence.
- Frozen tasks keep contract smoke only: one wire oracle, binding preparation and cancellation per task.
- Always keep safety-invariant tests, whose value is in their assertions rather than line coverage: credentials, ingress auth, body lifecycle and cancellation, transport bounds, origin checks, strict JSON, replay isolation, terminal honesty.
- When code is deleted, delete its tests and any helpers left unused. To prune, use per-test coverage (procedure in [docs/development.md](docs/development.md)) and report the coverage change.

## Verification

- Default checks are offline and synthetic-only (commands in [docs/development.md](docs/development.md)): run the affected targets first, then the full baseline: `cargo test`, `cargo clippy --all-targets -D warnings`, `cargo fmt --check`, `git diff --check`.
- The SDK loopback gate and real Provider calls are separate explicit steps; they supplement, never replace, offline regression.
- Report the checks actually run, failures and uncovered layers. Passing compilation or synthetic tests does not prove real Provider, SDK or production behavior.

## Documentation

- The doc set is fixed: `README.md`, `AGENTS.md`, `docs/architecture.md`, `docs/roadmap.md`, `docs/usage.md`, `docs/development.md`, `docs/references.md`, `docs/openapi.json`. No analysis pages, run reports or history (use Git).
- Remove finished slices from the roadmap; record new decisions in the architecture decision table.
- For Markdown-only changes, check links and run `git diff --check`.
