# Tests and Verification Guide (`tests/`, test harnesses)

Applies to `tests/`, `crates/morphiecore-test-harness/`, and `crates/morphiecore-test-support/` (inherits [AGENTS.md](../AGENTS.md)).

## Authoritative policy and high-signal pruning

- Follow [Development](../docs/development.md#verification-gates) and [test selection and pruning](../docs/development.md#test-selection-and-pruning).
- Keep tests that independently falsify a product assumption, public contract, or concrete external failure risk; preserve credential/auth, lifecycle/cancellation, bounds, origin, strict-JSON, replay-isolation, and terminal-honesty assertions.
- Aggressively delete implementation restatements, static catalog/config snapshots, trivial accessor/assembly checks, third-party self-tests, and redundant permutations together with their exclusive helpers.
- Protocol mappings need independent wire→IR and IR→wire expectations (a round trip alone is not evidence); negative cases must reach their intended failure.

## Client baselines and live checks

- **Host clients:** Adapt production client boundaries and Agent acceptance only to the host's current latest `pi`, OpenCode, and other Agents, using isolated test-owned state and safe unwrapped launchers (no old-version compatibility branches, downgrades, or historical version matrices). Historical mapping expectations are not the regression oracle.
- **Real-service functional checks:** Prefer direct functional checks over bespoke harnesses. Do not impose fixed token, elapsed-time, or request-count budgets unless the user requests them or the check specifically tests those controls; keep ordinary application/Provider safety limits and cancellation intact.

## Verification gates and honest reporting

- Iterate with focused offline checks and reuse valid results. Before pushing code changes, run the [offline baseline](../docs/development.md#offline-baseline) in the [flake](../flake.nix) environment and applicable roadmap gates.
- When affected, also run:
  ```sh
  tests/sdk/.venv/bin/python -m unittest discover -s tests/gateway
  tests/sdk/.venv/bin/python -m unittest discover -s tests/sdk
  python3 -m unittest discover -s tests/clients
  ```
- Keep offline, SDK (`tests/sdk_loopback.rs`, `#[ignore]`), installed-client (`tests/clients.rs`), and real-Provider results separate, reporting exact checks, failures, counts/retries, and unverified contracts.
