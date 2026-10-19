# Development

This guide owns tooling, test procedures and evidence reporting. Use offline, synthetic checks by default; SDK loopback, real-client and Provider checks are separate gates. [Roadmap](roadmap.md#verification-and-integration-boundary) owns which gates a slice requires, and [AGENTS.md](../AGENTS.md#authorization) owns authorization. Product setup belongs to [Usage](usage.md).

## Contents

- [Environment](#environment)
- [Test layout](#test-layout)
- [Verification gates](#verification-gates)
  - [Offline baseline](#offline-baseline)
  - [SDK loopback gate](#sdk-loopback-gate)
  - [Documentation checks](#documentation-checks)
- [Coverage and pruning](#coverage-and-pruning)
- [Online acceptance](#online-acceptance)
  - [Real Provider checks](#real-provider-checks)
  - [pi acceptance](#pi-acceptance)
  - [opencode acceptance](#opencode-acceptance)

## Environment

[flake.nix](../flake.nix) provides the toolchain; `flake.lock` pins nixpkgs. rustup is not needed.

```sh
nix develop          # Rust, Python 3.13 and uv
nix develop .#rust   # Rust only
nix flake check      # Tool availability, not the Rust test baseline
```

direnv users can allow the root `.envrc` once. Update nixpkgs separately with `nix flake update nixpkgs`, then run the full baseline.

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

Select checks from the relevant [roadmap package](roadmap.md#planned-order). Same-protocol fidelity needs a pinned revision/domain and independent request, response, JSON/SSE and returned-history expectations; a successful tool loop does not prove losslessness. Independent tasks and resources need their own lifecycle checks, not Generation round trips.

The [design-review gate](roadmap.md#generation-design-review-gate) owns coverage authorization and pending approval. Use counterexamples to support that review; do not encode unresolved defects as successful expectations. Passing checks, measured coverage and agent review are not human design approval.

## Verification gates

| Change or claim | Procedure |
|---|---|
| Markdown-only change | Local paths/anchors and whitespace under [documentation checks](#documentation-checks); no Rust/SDK run |
| Implementation slice | Focused checks while iterating, then the [offline baseline](#offline-baseline) before publication |
| Connected protocol boundary | Separate [SDK loopback gate](#sdk-loopback-gate); mark disconnected boundaries blocked, not passed |
| Actual client or Provider behavior | Separately scoped [online acceptance](#online-acceptance); no inference from offline success |

### Offline baseline

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

### SDK loopback gate

Python and dependencies are pinned by `tests/sdk/.python-version` and `tests/sdk/uv.lock`, including the selected OpenAI and Anthropic releases. uv owns the ignored `tests/sdk/.venv/`; do not edit it by hand.

```sh
uv sync --project tests/sdk --locked
uv run --project tests/sdk --locked --offline cargo test --locked --offline --test sdk_loopback -- --ignored --test-threads=1
uv run --project tests/sdk --locked --offline python -m unittest discover -s tests/sdk -p 'test_*.py'
```

All listeners and credentials are synthetic and test-owned. The gate uses no real Providers or environment proxies.

The Messages gate extends the same loopback suite: native manual-budget/identity request controls, JSON tool input, SDK-returned call/result history, ordinary SSE and raw unfinished argument deltas at `max_tokens`. It uses an empty child environment and scratch HOME. Raw-event acceptance is not a claim that an SDK's accumulated-message helper or tool runner can replay unfinished parameters; native JSON/history rejection remains required.
This fixture covers codec/Attempt/delivery, not full gateway Router acceptance. Its buffered HTTP response does not measure incremental network publication; the transport suite independently checks incremental delivery and EOF withholding.

The selected R1 gate in `tests/sdk/r1.rs` uses the production Router and the catalog's DashScope Chat and OpenRouter Responses contracts, with synthetic origins and credentials. It checks independent three-turn JSON/SSE tool/result/final/next-history expectations, explicit non-strict tools and caps, and Responses stored/stateless continuations. `tests/gateway/r1.rs` separately checks effective-history parity, tool-error text feedback, EOF withholding, cancellation and no replay to an eligible secondary after tool publication. Synthetic reports remain internally consistent; these tests do not resolve the [unselected lifecycle-report design](api-expansion-design.md#lifecycle-settings-and-service-reports-unselected) or certify real R1b SSE.

#### Pinned pi history-carrier probe

The optional ignored test uses the [pi client baseline](references.md#pinned-sources), Node 24.21.0 and the dependency lock in `tests/sdk/pi/`. Keep the pi checkout outside this repository; set `MORPHIECORE_PI_SOURCE` to it. Install the public test dependencies without private npm configuration or lifecycle scripts:

```sh
tmp=$(mktemp -d)
touch "$tmp/global.ini"
env -i PATH="$PATH" HOME="$tmp" npm --userconfig=/dev/null \
  --globalconfig="$tmp/global.ini" --prefix tests/sdk/pi ci \
  --ignore-scripts --no-audit --no-fund
MORPHIECORE_PI_SOURCE=/path/to/pinned/pi \
  cargo test --locked --offline --lib pinned_pi_owned_replay -- --ignored --nocapture
```

The test clears the child environment, uses a temporary HOME, and replaces fetch with fixed synthetic SSE. It performs 24 serial fetches, two per case, with no retries, a 32-token cap and a five-second deadline per stream. Cases cover three protocols, equal-text owners, empty signed text, an exact-number mutation that drops its carrier, and phase-only history with unlabelled prefixes. The pinned Chat builder drops the opaque-only empty turn; that case asserts the [accepted limitation](refs/2026-10-17-generation-contracts.md#signed-google-parts) instead of restoration. Returned history is checked separately from request-control/tool admission. This is not a live-client, opencode, Provider or routed full-chain acceptance gate.

For OpenAPI-only changes, run the affected schema checks:

```sh
uv run --project tests/sdk --locked --offline python -m unittest discover -s tests/sdk -p 'test_openapi_schema.py'
```

### Documentation checks

The fixed document set is defined in [AGENTS.md](../AGENTS.md#documentation).

For Markdown changes, check local paths and heading anchors, then run `git diff --check`. When moving sections, update incoming links across the fixed document set, verify that no contract or approval boundary was lost, and keep the original source dates. Do not add a second plan or specification at the old location. Recheck external sources when changing a protocol claim; a link check is not evidence of current API behavior.

For OpenAPI changes, also run the schema checks above and compare paths, authentication and error envelopes with the gateway code. The OpenAPI file is an API summary, not a second codec implementation.

## Coverage and pruning

`cargo-llvm-cov` needs LLVM tools matching rustc's LLVM major version. Check `rustc -vV` and use scoped Nix tooling, not a global install:

```sh
nix shell --inputs-from . nixpkgs#cargo-llvm-cov nixpkgs#llvmPackages_21.llvm --command bash -c '
  export LLVM_COV=$(command -v llvm-cov) LLVM_PROFDATA=$(command -v llvm-profdata)
  cargo llvm-cov --locked --offline --lcov --output-path <out>.lcov'
```

When measuring coverage changes, use the same test selection before and after, and keep reports under ignored `testdata/reports/`. A run leaves startup-test `default_*.profraw` files in the repository root; remove them afterwards. `--json --output-path testdata/reports/coverage.json` exports machine-readable file summaries. Report the scope precisely: an aggregate of `src/` also includes unit tests embedded there; focused production-only modules are a clearer measure for local changes. Inspect uncovered paths for missing invariants before adding tests. Do not force defensive unreachable branches or count generated cases as separate contracts.

For a pruning pass:

1. Record `src/` line coverage before changes.
2. Build instrumented binaries using `cargo llvm-cov show-env`.
3. Run each selected test with `--exact` and export its coverage.
4. Compare coverage after pruning and inspect every newly uncovered line.

Coverage proves execution, not assertions. Never remove safety-invariant tests just because their lines are covered elsewhere.

## Online acceptance

Record the tested binary version separately from the pinned source revision. An installed-client gate does not establish behavior for a different pinned revision or every protocol route.

### Real Provider checks

Follow [authorization](../AGENTS.md#authorization) and [Usage](usage.md). This is a separate online gate, not part of the baseline.

Before each run:

1. Fix the target model/Provider, request count, output cap and timeout.
2. Use the project loader and application-owned store. Do not source private environment files or third-party auth caches.
3. Use explicit model/candidate entries in `models.json` so unrelated pools are not activated.
4. Run serially with no hidden retries. For a single-attempt check, set the selected model's attempt ceiling to one.
5. Report only redacted status, counts and error codes. Never print credentials or request/response bodies.

The gateway CLI loads `gateway.json`, its selected `models.json` and pools through the project loader. A test harness must do the same; do not hand-parse credential files.

Local byte, event and deadline limits do not prove bounded upstream charges when a target has no output-token control. Real checks supplement synthetic regression; they do not replace it.

For tool-result image checks, first obtain a real tool call from the selected model, then return a synthetic valid image with the complete reported history. Preserve thinking and opaque replay carriers unchanged. A hand-written call without model-issued continuation values is not a valid replay oracle for thinking models.

Distinguish HTTP success, honest completion and visual recognition. Check a known feature of the synthetic image without logging response text. A successful public-URL sample does not prove URL availability or justify automatic retries after an earlier failure.

### pi acceptance

Use pi's real binary against the CLI gateway, not a wrapper that sources private environment.

1. Create a scratch `HOME` and a `0700` `PI_CODING_AGENT_DIR`.
2. Run with `env -i` and only the required tool paths and ingress credential.
3. Write scratch `models.json` with one provider for each API under test:
   - `anthropic-messages`: bare gateway origin.
   - `openai-completions` and `openai-responses`: origin with `/v1`.
   - Use `apiKey: "$NAME"` and `reasoning: true` on the selected model entry.
   - For the R2 Messages profile, declare the model with `api: "anthropic-messages"`, `reasoning: true` and the built-in Haiku 5.5 compat (`supportsMidConvoEffort`, `supportsMidConvoSystemMessages`, `supportsMidConvoToolChanges`, `forceAdaptiveThinking`, `supportsStrictTools`, `supportsTemperature: false`), as the pi extension does. Overriding only the built-in `anthropic` provider's `baseUrl` sends the same shape.
   - For session-affinity checks on Chat/Messages, set `compat.sendSessionAffinityHeaders: true` and keep caching enabled (`cacheRetention` other than `none`). Verify the same neutral conversation across the tool round; Responses normally uses `session_id`, Messages uses `x-session-affinity`. Transport-header checks alone are not full-client acceptance.
4. Set `retry.enabled: false` and `cacheWarming: "off"` in scratch `settings.json`.
5. Ask pi to read a synthetic file, then answer. Use these flags:

```sh
pi -p --mode json --offline --no-session --no-extensions --no-skills \
  --no-context-files --no-mcp --tools read --thinking medium '<synthetic prompt>'
```

6. Check `message_end` events for a successful tool turn and final answer. Report counts and status, not bodies.
7. Remove the test-owned scratch directory after the run.

pi's `--offline` flag does not make an upstream gateway request synthetic. This remains a real-client gate when the gateway calls a real Provider.

For the image gate, use a synthetic PNG and declare image input in the scratch model. Set a fixed output cap, disable compaction, and verify a completed `read`, an image in the next tool-result request and an honest final answer. Bound actual upstream requests; do not rely only on client retry settings.

### opencode acceptance

Use opencode's real binary against the CLI gateway, not a wrapper that sources private environment. The installed binary can lag the source pinned in [References](references.md); check `--version` before comparing behavior with source.

1. Create a `0700` scratch directory with `HOME`, a work directory and a synthetic file.
2. Run with `env -i`, scratch `HOME` and `XDG_*` directories, and only the required tool paths and ingress credential.
3. Set `OPENCODE_CONFIG` to a scratch config and set `OPENCODE_DISABLE_AUTOUPDATE`, `OPENCODE_DISABLE_MODELS_FETCH` and `OPENCODE_DISABLE_PROJECT_CONFIG` to `1`.
4. Under `providers`, add one entry for each API under test. Each sets `package`, `env` naming the ingress-credential variable, `settings.baseURL` set to the origin with `/v1`, and the model entry:

| API | `package` |
|---|---|
| Chat Completions | `@opencode/ai/providers/openai-compatible` |
| Responses | `@opencode/ai/providers/openai` |
| Messages | `@opencode/ai/providers/anthropic` |

5. From the work directory, ask opencode to read the synthetic file, then answer:

```sh
opencode run --standalone --format json --title '<title>' \
  -m '<provider>/<model>#medium' '<synthetic prompt>'
```

6. Check for a completed `read` tool event and a final answer. The JSON stream can omit the last `step_finish`. Read each assistant `finish` (`tool-calls`, then `stop`) from `session_message` and the idle outcome from `session_v2` in the scratch `opencode.db`. Report counts and status, not bodies.
7. Remove the test-owned scratch directory after the run.

`--standalone` keeps the server inside the run instead of a background service. A title avoids an extra title-generation request.

For the image gate, set the scratch model's `limit.output` and image-input capability. Verify the completed `read`, the tool-result image on the wire and the final assistant stop in the scratch database (`session_message.type` is the role; `data.finish` is the finish reason). Also check `session_v2.idle_outcome` is `succeeded`.

Some installed clients omit the wire cap despite `limit.output`. Set the fixed cap through `providers.<id>.body.max_output_tokens` for a Responses gate and check the actual request. A loopback admission guard may reject absent or over-cap, duplicate or excess requests without rewriting their bodies.

For R1a Chat, use `low`, set `body.max_completion_tokens` explicitly and check the selected `#low` variant's actual `reasoning_effort`. Verify next-turn call/result, final-answer and reasoning history, not just the first tool round. For pi, the selected compatibility differences are recorded in the [client profile evidence](refs/2026-10-17-generation-contracts.md#responses-lifecycle-reports-2026-10-19); structured `reasoning_details` is a valid returned-history carrier, so do not require redundant `reasoning_content`.
