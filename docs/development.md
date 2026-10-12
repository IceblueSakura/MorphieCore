# Development

Use offline, synthetic checks by default. SDK loopback and real-client checks are separate gates.

## Contents

- [Environment](#environment)
- [Offline baseline](#offline-baseline)
- [Test layout](#test-layout)
- [Coverage and pruning](#coverage-and-pruning)
- [SDK loopback gate](#sdk-loopback-gate)
- [Real Provider checks](#real-provider-checks)
- [pi acceptance](#pi-acceptance)
- [Documentation checks](#documentation-checks)

## Environment

[flake.nix](../flake.nix) provides the toolchain; `flake.lock` pins nixpkgs. rustup is not needed.

```sh
nix develop          # Rust, Python 3.13 and uv
nix develop .#rust   # Rust only
nix flake check      # Tool availability, not the Rust test baseline
```

direnv users can allow the root `.envrc` once. Update nixpkgs separately with `nix flake update nixpkgs`, then run the full baseline.

## Offline baseline

Run affected targets first, then the full baseline:

```sh
cargo test --locked --offline
cargo clippy --locked --offline --all-targets -- -D warnings
cargo fmt -- --check
git diff --check
```

Example focused check:

```sh
cargo test --locked --offline --test semantic reasoning_replay::
```

These checks use synthetic data only. Do not use real Providers or private configuration as offline test input.

## Test layout

| Target | Scope |
|---|---|
| `src/**` unit tests | Module internals, credential ownership, ingress and affinity |
| `tests/semantic` | IR, codecs, lowering, replay and history editing |
| `tests/transport` | SSE framing/decoding, candidate planning, bindings and body lifecycle |
| `tests/credential` | Credential CLI with synthetic stores and authorities |
| `tests/gateway` | Router → synthetic Provider and isolated binary startup |
| `tests/sdk_loopback` | Ignored by default; pinned Python SDK over loopback |

Shared builders live in `tests/support/`. Test selection follows the lean policy in [AGENTS.md](../AGENTS.md#tests).

Protocol mappings need independently hand-written wire→IR and IR→wire expectations. Do not generate an oracle with the encoder under test. Round trips are supplementary.

## Coverage and pruning

`cargo-llvm-cov` needs LLVM tools matching rustc's LLVM major version. Check `rustc -vV` and use scoped Nix tooling, not a global install:

```sh
nix shell --inputs-from . nixpkgs#cargo-llvm-cov nixpkgs#llvmPackages_21.llvm --command bash -c '
  export LLVM_COV=$(command -v llvm-cov) LLVM_PROFDATA=$(command -v llvm-profdata)
  cargo llvm-cov --locked --offline --lcov --output-path <out>.lcov'
```

For a pruning pass:

1. Record `src/` line coverage before changes.
2. Build instrumented binaries using `cargo llvm-cov show-env`.
3. Run each selected test with `--exact` and export its coverage.
4. Compare coverage after pruning and inspect every newly uncovered line.
5. Remove startup-test `default_*.profraw` artifacts from the repository root.

Coverage proves execution, not assertions. Never remove safety-invariant tests just because their lines are covered elsewhere.

## SDK loopback gate

Python and dependencies are pinned by `tests/sdk/.python-version` and `tests/sdk/uv.lock`. uv owns the ignored `tests/sdk/.venv/`; do not edit it by hand.

```sh
uv sync --project tests/sdk --locked
uv run --project tests/sdk --locked --offline cargo test --locked --offline --test sdk_loopback -- --ignored --test-threads=1
uv run --project tests/sdk --locked --offline python -m unittest discover -s tests/sdk -p 'test_*.py'
```

All listeners and credentials are synthetic and test-owned. The gate uses no real Providers or environment proxies.

For OpenAPI-only changes, run the affected schema checks:

```sh
uv run --project tests/sdk --locked --offline python -m unittest discover -s tests/sdk -p 'test_openapi_schema.py'
```

## Real Provider checks

Follow [authorization](../AGENTS.md#authorization) and [Usage](usage.md). This is a separate online gate, not part of the baseline.

Before each run:

1. Fix the target model/Provider, request count, output cap and timeout.
2. Use the project loader and application-owned store. Do not source private environment files or third-party auth caches.
3. Use an explicitly selected `models` list so unrelated pools are not activated.
4. Run serially with no hidden retries. For a single-attempt check, set the gateway attempt ceiling to one.
5. Report only redacted status, counts and error codes. Never print credentials or request/response bodies.

The gateway CLI loads `gateway.json` and pools through the project loader. A test harness must do the same; do not hand-parse credential files.

Local byte, event and deadline limits do not prove bounded upstream charges when a target has no output-token control. Real checks supplement synthetic regression; they do not replace it.

## pi acceptance

Use pi's real binary against the CLI gateway, not a wrapper that sources private environment.

1. Create a scratch `HOME` and a `0700` `PI_CODING_AGENT_DIR`.
2. Run with `env -i` and only the required tool paths and ingress credential.
3. Write scratch `models.json` with one provider for each API under test:
   - `anthropic-messages`: bare gateway origin.
   - `openai-completions` and `openai-responses`: origin with `/v1`.
   - Use `apiKey: "$NAME"` and `reasoning: true` on the selected model entry.
4. Set `retry.enabled: false` and `cacheWarming: "off"` in scratch `settings.json`.
5. Ask pi to read a synthetic file, then answer. Use these flags:

```sh
pi -p --mode json --offline --no-session --no-extensions --no-skills \
  --no-context-files --no-mcp --tools read --thinking medium '<synthetic prompt>'
```

6. Check `message_end` events for a successful tool turn and final answer. Report counts and status, not bodies.
7. Remove the test-owned scratch directory after the run.

pi's `--offline` flag does not make an upstream gateway request synthetic. This remains a real-client gate when the gateway calls a real Provider.

## Documentation checks

The fixed document set is defined in [AGENTS.md](../AGENTS.md#documentation).

For Markdown changes, check local paths and heading anchors, then run `git diff --check`. Recheck external sources when changing a protocol claim; a link check is not evidence of current API behavior.

For OpenAPI changes, also run the schema checks above and compare paths, authentication and error envelopes with the gateway code. The OpenAPI file is an API summary, not a second codec implementation.
