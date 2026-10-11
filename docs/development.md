# Development

## Environment

[flake.nix](../flake.nix) provides the environment (`flake.lock` pins nixpkgs); rustup is not needed:

```sh
nix develop          # Rust toolchain, Python 3.13, uv
nix develop .#rust   # Rust only
nix flake check
```

direnv users can use the root `.envrc` (run `direnv allow` once). Update the toolchain separately with `nix flake update nixpkgs`, then run the full baseline.

## Baseline

```sh
cargo test --locked --offline
cargo clippy --locked --offline --all-targets -- -D warnings
cargo fmt -- --check
git diff --check
```

Run affected targets first, e.g. `cargo test --locked --offline --test semantic reasoning_replay::`, then the full baseline. Default checks use synthetic data only: no real Providers, no private configuration.

## Test layout

| Target | Scope |
|---|---|
| `src/**` unit tests | Module internals: credential manager/store/drivers, gateway entry points, affinity |
| `tests/semantic` | IR, codecs, lowering, replay, edit invalidation; one file per topic (`tool_calls`, `reasoning_replay`, `history`, `text`, `media`, …) |
| `tests/transport` | SSE framing, per-protocol stream decoding, candidate planning and native bindings, body lifecycle |
| `tests/credential` | `morphiecore-auth` black box (synthetic files and authority) |
| `tests/gateway` | Real Router → synthetic Provider smoke (including native Messages) and isolated binary startup |
| `tests/sdk_loopback` | Ignored by default: the pinned OpenAI Python SDK consuming the gateway across turns |

Protocol mapping tests hand-write wire→IR and IR→wire expectations independently, never generating the oracle with the encoder under test; round trips are supplementary. Shared synthetic builders live in `tests/support/`. Test selection follows the lean policy in [AGENTS.md](../AGENTS.md#tests).

## Coverage and pruning

`cargo-llvm-cov` needs LLVM tools matching rustc's LLVM major version (see `rustc -vV`). Use scoped Nix tools, not global installs:

```sh
nix shell --inputs-from . nixpkgs#cargo-llvm-cov nixpkgs#llvmPackages_21.llvm --command bash -c '
  export LLVM_COV=$(command -v llvm-cov) LLVM_PROFDATA=$(command -v llvm-profdata)
  cargo llvm-cov --locked --offline --lcov --output-path <out>.lcov'
```

When pruning, compare `src/` line coverage before and after and judge each newly uncovered line. To find per-test redundancy, build instrumented binaries once with `cargo llvm-cov show-env`, run each test alone with `--exact` and export its coverage. Delete the `default_*.profraw` files that binary startup tests leave in the repository root. Coverage shows execution, not assertion; safety-invariant tests are never deleted for redundancy.

## SDK loopback gate

`tests/sdk/.python-version` pins Python, `tests/sdk/uv.lock` locks dependencies, and the environment lives in the ignored `tests/sdk/.venv/`:

```sh
uv sync --project tests/sdk --locked
uv run --project tests/sdk --locked --offline cargo test --locked --offline --test sdk_loopback -- --ignored --test-threads=1
uv run --project tests/sdk --locked --offline python -m unittest discover -s tests/sdk -p 'test_*.py'
```

All listeners are temporary loopback ones with synthetic credentials; no environment proxies or real Providers.

## Real Provider checks

Authorization is defined in [AGENTS.md](../AGENTS.md#authorization). There is no standing probe tool: when needed, write a minimal entry point under `examples/` that gets credentials through the project loader, fixes request count, output cap and timeout, runs serially with no hidden retries, prints only redacted status and counts, and never saves bodies or opaque values. Real calls supplement, never replace, synthetic regression.

## Documentation

The doc set and its rules are in [AGENTS.md](../AGENTS.md#documentation). For Markdown-only changes, check links and run `git diff --check`.
