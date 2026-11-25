# Rust Workspace Guide (`crates/`, `src/`)

Applies to all Rust crates and binaries under `crates/` and `src/` (inherits [AGENTS.md](../AGENTS.md)).

- Also read [crates/morphiecore-protocol/AGENTS.md](morphiecore-protocol/AGENTS.md) before editing `morphiecore-protocol/` or `morphiecore-lowering/`.
- Also read [crates/morphiecore-gateway/AGENTS.md](morphiecore-gateway/AGENTS.md) before editing `morphiecore-gateway/`, `morphiecore-runtime/`, or `src/bin/`.

## Layer purity

Follow the [Workspace layer map](../docs/architecture/index.md#layers-and-data-flow):
- `morphiecore-semantic` owns the official-protocol IR union with no protocol, Provider, target, credential, retry, or I/O dependencies.
- `morphiecore-lowering` and `morphiecore-protocol` (including adapters) are pure in-memory transformations.
- `morphiecore-catalog` owns static trusted Provider, model, and topology metadata.

## Dependencies and upstream pins

- When introducing or upgrading protocols, dialects, or dependencies, pin current stable dependencies and release sources first, and integrate upstream-derived standard tests alongside the rewrite.
- Union IR design does not implicitly activate new public APIs.
- Update `Cargo.lock` for dependency changes and verify with `--locked`.

## Direct code, current schema, and naming

- Prefer existing solutions and direct code over speculative abstractions.
- Maintain only the current application configuration schema and paths — add no historical aliases, migrations, obsolete-field detection, or old-schema rejection tests, while keeping ordinary strict parsing and safety checks.
- Name code, fixtures, and tests by protocol or behavior, never roadmap phases or package IDs.
- Never hand-edit build caches; write code comments in concise English explaining non-obvious invariants.
