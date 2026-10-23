# Development

This guide owns tooling, test procedures and evidence reporting. Use offline, synthetic checks by default; SDK loopback, real-client and Provider checks are separate gates. [Roadmap](roadmap.md#verification-and-integration-boundary) owns which gates a slice requires, and [AGENTS.md](../AGENTS.md#authorization) owns authorization. Product setup belongs to [Usage](usage.md).

## Contents

- [Environment](#environment)
- [Test layout](#test-layout)
- [Upstream test sources](#upstream-test-sources)
- [Verification gates](#verification-gates)
  - [Offline baseline](#offline-baseline)
  - [Reusable Generation sessions](#reusable-generation-sessions)
  - [SDK loopback gate](#sdk-loopback-gate)
  - [Documentation checks](#documentation-checks)
- [Coverage and pruning](#coverage-and-pruning)
- [Online acceptance](#online-acceptance)
  - [Production-profile acceptance](#production-profile-acceptance)
  - [Installed-client synthetic recovery](#installed-client-synthetic-recovery)
  - [Installed-client synthetic summaries](#installed-client-synthetic-summaries)
  - [Real Provider checks](#real-provider-checks)
  - [pi acceptance](#pi-acceptance)
  - [opencode acceptance](#opencode-acceptance)

## Environment

[flake.nix](../flake.nix) provides the toolchain; `flake.lock` pins nixpkgs and rust-overlay. The overlay reads the exact stable version in `rust-toolchain.toml`; rustup and host-wide upgrades are not needed. The shell also pins Python 3.13 tooling and Node 24 LTS through nixpkgs.

```sh
nix develop          # Rust, Python 3.13 and uv
nix develop .#rust   # Rust only
nix flake check      # Tool availability, not the Rust test baseline
```

direnv users can allow the root `.envrc` once. Refresh project inputs with `nix flake update`, review their package versions, then run the applicable checks. Rust is selected independently of nixpkgs packaging lag. Do not claim every transitive/system package is the latest standalone release; dependency constraints and maintained runtime tracks remain explicit.

## Test layout

| Entry | Scope and execution |
|---|---|
| `src/**` unit tests | Owner-local contracts, including private state, credential lifecycle and synthetic Router regressions |
| `src/bin/upstream/tests.rs` | Manual diagnostic target/wire contracts and credential-manager refresh checks; no real upstream requests |
| `tests/semantic.rs` → `semantic/` | Offline IR, codecs, lowering, replay and history editing |
| `tests/transport.rs` → `transport/` | Offline SSE framing/decoding, candidate preparation and Attempt/delivery lifecycle |
| `tests/credential.rs` → `credential/` | Offline credential CLI checks with synthetic stores/authorities; Unix-gated |
| `tests/gateway.rs` → `gateway/` | Offline Router → synthetic Provider and isolated binary startup |
| `tests/process.rs` → `process/` | Offline child I/O limits, timeout, cancellation and descendant cleanup |
| `tests/sdk_loopback.rs` → `sdk/` | Pinned Python SDK loopback scenarios; ignored by default |
| `tests/clients.rs` → `clients/` | Ignored by default, Unix; explicitly selected installed clients against a synthetic CLI gateway |
| `tests/{sdk,clients,gateway}/test_*.py` | Separate Python unittest discovery for SDK oracles, installed-client oracles and OpenAPI schemas |
| `tests/upstream/{assets.py,test_assets.py}` | Offline provenance/extraction checks; explicit `--fetch` restores pinned public assets |
| Pinned pi probe | Separate ignored library test and Node scripts; see the [probe procedure](#pinned-pi-history-carrier-probe) |

Top-level Rust files are Cargo integration targets; nested files are registered modules, not additional targets. Keep roots small, but retain small target-local shared types there instead of adding forwarding modules. Do not flatten child modules into `tests/*.rs` merely to shorten paths. Test selection follows the lean policy in [AGENTS.md](../AGENTS.md#tests).

Shared fixture code lives in `tests/support/` and may serve multiple integration targets or source-local tests. Consumers import it directly, not through another scenario module. External static inputs live in `testdata/`: literal corpus data at `<scenario>/<api>/` and synthetic auth assets at `auth/`. Keep scenario/API boundaries, but avoid wrapper directories with no independent meaning. Generated output, reports and runtime state stay in ignored `testdata/{generated,reports,dist,runtime}/`, separate from committed inputs.

Semantic domain entries declare child modules; Rust module paths, not physical filenames or roadmap IDs, select tests. The OAuth CLI module remains `unix` in `tests/credential/oauth_accounts.rs`; the Router smoke and SDK text loops use `router_smoke::` and `text_loop::`. CLI startup checks live in `tests/gateway/bootstrap.rs`. Capture redaction, incomplete-source and retention checks live in `src/gateway/capture/tests.rs`, separate from body-free diagnostics. Cross-owner HTTP/cancellation assertions stay at their integration owner.

Bootstrap fixtures take explicit service/models JSON and synthetic credential seeds, then call the real private-file loader. Keep model selection independent of installed keys and negative cases at their intended production boundary. Raw malformed-file cases retain a byte writer; JSON values cannot preserve duplicate keys. SDK command helpers share only identical environment setup, not process ownership, protocol oracles or report assertions.

Protocol mappings need independent wire→IR and IR→wire expectations from official contracts and pinned upstream assets. Keep local assertions for IR ownership, safety or gaps in upstream coverage; do not maintain a duplicate protocol dataset or generate an oracle with the encoder under test. Round trips are supplementary.

Select checks from the relevant [roadmap package](roadmap.md#planned-order). Same-protocol fidelity needs a pinned revision/domain and independent request, response, JSON/SSE and returned-history expectations; a successful tool loop does not prove losslessness. Independent tasks and resources need their own lifecycle checks, not Generation round trips.

The [design-review gate](roadmap.md#generation-design-review-gate) owns approved scope, coverage authorization and outstanding approval requirements. Use counterexamples to support design review; do not encode unresolved defects as successful expectations. Passing checks, measured coverage and agent review are not human design approval.

## Upstream test sources

**Integrated into the accepted standards reset on 2026-10-23.** [sources.lock.json](../tests/upstream/sources.lock.json) fixes release commits and candidate paths. [assets.lock.json](../tests/upstream/assets.lock.json) identifies the reviewed, vendored subset in `testdata/upstream/`; thin adapters and selected real-SDK consumers are implemented. Broader conformance integration and automatic release updates remain separate. Reuse assets during the rewrite rather than afterward.

### Pinned asset acquisition and execution

Ordinary tests require no source checkout or network. Imported bytes retain their original content and applicable licenses; local aliases, immutable revision/path and SHA-256 are recorded per file. `.gitattributes` disables newline normalization for this directory. Do not format fixtures or regenerate expected output from a MorphieCore encoder.

```sh
python3 tests/upstream/assets.py
python3 -m unittest discover -s tests/upstream -p 'test_*.py'
# Explicit network operation: only restore missing/changed files from the selected revisions.
python3 tests/upstream/assets.py --fetch
cargo test --locked --offline --test transport upstream_fixtures
uv run --project tests/sdk --locked --offline cargo test --locked --offline \
  --test sdk_loopback upstream_fixtures_reach_real_sdks -- --ignored --test-threads=1
```

The acquirer checks source/asset revision agreement and expected digests, permits only immutable GitHub raw URLs, rejects redirects and caps each response at 2 MiB with a 30-second socket timeout and no retries. It does not install packages or execute downloaded code. Missing files, digest drift, ambiguous extractions and changed source pins fail rather than becoming skipped coverage. An upstream update requires deliberate source and asset review, not `--fetch` rewriting the locks.

`tests/support/upstream_assets.rs` performs only declared framing adaptations: append a missing dispatch delimiter to the selected Anthropic SDK fixtures, or frame Vercel JSON payload lines using their existing discriminator. The unmodified Anthropic fixtures still exercise strict EOF rejection. EventSource Parser extracts use only literal `onChunk` strings in three named functions; Python parses the literals without executing TypeScript. Generated sentinels/timers/random data are excluded and recorded in the [asset boundary inventory](refs/2026-10-23-stable-protocol-contracts.md#imported-asset-boundaries).

| Asset family | Boundary exercised |
|---|---|
| OpenAI Python tool/length SSE | Incremental intake, exact arguments/call identity, actual usage, incomplete terminal |
| Anthropic Python text/tool/sparse-usage SSE | Native IR and delivery, optional-count retention, strict EOF and invalid-order rejection |
| Vercel Responses custom call | Unchanged output-array replay with distinct item/call IDs; missing-sequence stream is a rejection case, not a repaired success |
| Vercel Google text | generateContent SSE content, signature-bearing owners and scoped source usage; static companion remains reference only |
| Vercel Interactions | Original stream reproduces late-service-tier rejection; static companion remains reference only |
| EventSource Parser | Literal multiline/CR/CRLF framing across every two-way byte split; no browser reconnection claim |

The connected SDK case makes eight serial synthetic requests: JSON and SSE tool responses for Chat and Messages, each followed by SDK-built history carrying a fixed result. The real client decoder re-admits that history. Chat's second fixture deliberately ends at `length`; Messages returns a final text answer. Retries/tool execution are disabled, request ceilings are 32/128 tokens, HTTP timeout is eight seconds and child timeout is 45 seconds. This uses actual Attempt/ResponseDelivery and strict pinned SDK consumers, not the Router, real Providers or installed agents. It does not establish streaming latency, backpressure or adaptation-extension retention.

### Pending standard requirements

Known differences have executable **desired-behavior** probes, not expected-failure success assertions. They are explicitly ignored in ordinary regression because the current stage inventories them without rewriting runtime semantics. Run them separately:

```sh
cargo test --locked --offline --test semantic standard_contracts -- --ignored
cargo test --locked --offline --test transport interactions_allows_first_service_tier_report_at_terminal -- --ignored
```

These commands currently fail at the recorded prediction, participant-name, omitted-strict, Messages sampling/direct-callers and Interactions late-report boundaries. A compilation/setup error is not a reproduced protocol gap. Repair each owner, remove its ignore marker and retain the independent assertion during the responsible redesign slice. Their failures block **new conformance acceptance**, not the provenance/fixture-integration preparation slice. The [inventory](refs/2026-10-23-stable-protocol-contracts.md#official-contract-difference-inventory) also records source-traced gaps not yet executable, explicit exclusions and SDK/schema disagreements.

### Source links and updates

- Keep upstream projects linked through test-only Git submodules or the equivalent source lock. Record repository, stable release/tag, resolved commit, package version, license and candidate fixture paths. Fetch/check out that exact revision outside production sources, with no install hooks or live fixture regeneration. Candidate paths are an inventory, not imported coverage.
- Prefer direct fixture loading. Where extraction is necessary, retain the source commit, case/path, license notices, content digest and documented transformations. Review selected material for credentials or private payloads before importing it. A public fixture is not automatically safe, authoritative or a real Provider capture.
- For this reset, current stable releases are mandatory; the former verified mapping baseline is not a fallback. Resolve package registries and official release tags together; monorepos require package-specific release tags. Sources without releases use explicitly labeled immutable snapshots. Pin all direct dependencies and refresh transitive locks within their upstream constraints; report incompatibilities rather than force unrelated major versions into the graph.
- After accepting the new baseline, resolve later stable releases as explicit candidates. Main-branch research is supplementary, not a substitute for stable release pins. Record drift and failures; offline tests never fetch floating sources or silently restore stale expected output.
- Review changes to relevant types, request builders, stream reducers, tests and fixtures. New, changed, removed and unsupported cases must remain visible; importer failure or an unrecognized case is not a passing skip. For a monorepo, record the selected package versions as well as its commit.
- Review release/API migration notes, request/response types, reducers, defaults and fixture changes before implementation. An old failing mapping assertion is classified against the new contract, not automatically repaired or skipped. Preserve valid assertions, replace obsolete ones using independent evidence, and remove tests only with obsolete behavior. A green old suite establishes maintenance evidence, not new conformance.

### Adapters and evidence

Use two complementary paths: raw upstream request/response/SSE fixtures through MorphieCore codecs, and actual SDK/Vercel consumers receiving gateway output and constructing the next request. Verify that the component under test actually ran; upstream tests passing against their own mocks or libraries do not establish gateway compatibility.

Retain independent wire-to-IR and IR-to-wire expectations for request controls, task results and reported facts, including exact argument carriage, call/result identity, ordered owners, usage and complete next-turn history for Generation. Reuse upstream expectations where their contract matches; keep the small IR-specific assertions local. Distinguish official protocol requirements, Provider dialects, SDK conveniences and gateway policy. Missing Provider-specific fixtures remain coverage gaps; generic compatible-API tests do not establish every dialect.

Run selected external code without private configuration or credentials, against synthetic loopback authorities, with retries and automatic tool execution disabled. Fetching sources does not authorize their install hooks, live snapshot regeneration or real Provider calls. No tool repair or model-capability scorer is part of this supplement.

For an operation-level extension, record which upstream cases exercise which local boundary and how to run them. Inspect mock-server assumptions and select raw fixtures or actual clients accordingly; SDK tests against their own generated mocks are not independent response oracles. Reuse source management and fixture readers, not a mandatory SSE representation: binary bodies, multipart inputs and job states stay in their responsible test layers.

Replace handwritten fixtures or duplicate test logic only after transferring their useful assertions to checks with equivalent boundary and gate coverage. A connected or ignored SDK test does not automatically replace a default offline check. Preserve small diagnostic, IR-specific and safety cases; follow [coverage and pruning](#coverage-and-pruning) for a deliberate removal pass.

### SSE layers

| Layer | Reused evidence | Local responsibility |
|---|---|---|
| Framing | WPT EventSource parsing cases and `eventsource-parser` regressions | Incremental bytes, UTF-8, line endings, fields, delimiters and limits; classify browser reconnection/control semantics and EOF-policy differences rather than claiming full browser conformance |
| Protocol events | Official SDK and Vercel static/stream fixtures | Interleaved calls, argument fragments, text/reasoning owners, trailing reports, errors, terminal outcomes and returned history |
| Transport lifecycle | Applicable SDK backpressure, interruption, early-exit and cleanup cases | Exercise the real gateway/transport boundary for bounded buffering, slow consumers, cancellation, upstream release, validated EOF and no replay after publication |

Apply existing `proptest` machinery and bounded fuzzing where useful to reused inputs: within declared budgets, byte repartitioning and legal comments must preserve declared meaning; truncation and malformed-event mutations must not invent success. Keep seeds and minimized reproducers. Timer/slow-reader checks need controlled scheduling, not assumptions about TCP packet boundaries. Test coalescing on/off against the same source events, not independent model generations.

Upstream implementation-specific assertions require adaptation, not blind adoption. For example, an SDK may accept events larger than the gateway budget or tolerate an unfinished stream; neither behavior permits weakening MorphieCore's limits or successful-terminal rules. Preserve current focused safety tests and the reusable session corpus rather than replacing them with an aggregate external-suite score.

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
python3 tests/upstream/assets.py
python3 -m unittest discover -s tests/upstream -p 'test_*.py'
git diff --check
```

These checks use synthetic data only. Do not use real Providers or private configuration as offline test input.

Focused examples:

```sh
cargo test --locked --offline --test semantic tool_calls::reported_tool_results::
cargo test --locked --offline --test semantic provider_observations::
cargo test --locked --offline --test gateway bootstrap::
cargo test --locked --offline --test process
```

### Python checks

The shared Python environment stays in `tests/sdk/`; test ownership does not determine environment location. Python and dependencies are pinned by `.python-version` and `uv.lock` there, including OpenAI, Anthropic and Google SDKs. `sdk_support.require_sdk` reads exact direct versions from `pyproject.toml`, avoiding duplicate version literals in scenarios. uv owns the ignored `.venv/`; do not edit it by hand.

Discover the suites separately so moving a test cannot silently drop it from a recursive search:

```sh
uv sync --project tests/sdk --locked
uv run --project tests/sdk --locked --offline python -m unittest discover -s tests/sdk -p 'test_*.py'
uv run --project tests/sdk --locked --offline python -m unittest discover -s tests/clients -p 'test_*.py'
uv run --project tests/sdk --locked --offline python -m unittest discover -s tests/gateway -p 'test_*.py'
```

These check SDK oracles, synthetic client-recovery authorities/drivers and public OpenAPI schemas, respectively. They neither launch installed clients nor replace SDK loopback acceptance. Client-recovery checks need only the standard library when run independently; the shared locked environment supports running all three suites.

For OpenAPI-only changes, select the schema checks:

```sh
uv run --project tests/sdk --locked --offline python -m unittest discover -s tests/gateway -p 'test_openapi_schema.py'
```

### Reusable Generation sessions

The committed [session corpus](../testdata/repair-and-recall/) is synthetic offline input, not a Provider capture or a serialization of the whole IR. It supplements focused codec tests with one coherent eight-generation repair/recall story, five native API variants and bounded failure forks. Run it without credentials, network services or fixture generation:

```sh
cargo test --locked --offline --test semantic history::sessions::
# One native API, including its complete returned history:
cargo test --locked --offline --test semantic history::sessions::interactions_repair_and_recall
```

The story covers two ordered `read` calls with reverse-order results, missing-file recovery via `locate`, and a `patch`/`verify` failure followed by successful repair. A second task recalls the history with no tools and higher effort. Its answer is projected into one final unsent user turn; there is no ninth generation or actual tool execution.

| Native variant | Client history carrier | Additional independent expectations |
|---|---|---|
| Chat | MorphieCore Chat | Raw argument whitespace/escapes and `9007199254740993`, interleaved indexed call fragments, null-content assistant shell, unreported progress |
| Responses | MorphieCore Responses | Item IDs distinct from call IDs, commentary/final phases, encrypted reasoning, explicit empty report arrays |
| Messages | Messages | Signed and redacted thinking, split signature deltas, structured arguments, explicit error flags, response-scoped effort and disjoint cache accounting |
| generateContent | MorphieCore Responses | Signed call owners, two identical `OK` texts with different signatures, original STOP ambiguity, split accounting |
| Interactions v1 | MorphieCore Responses | Separate thought/call signatures, argument fragments and full step snapshots, explicit `requires_action`, unknown accounting relationships |

Each variant has `session.json` and literal `.sse` wires with independent client/native requests, IR facts, feedback and history additions. `client_update`/`upstream_update` replace named current-request fields only; `request_rules` pins losses. Unknown manifest fields and unselected item kinds reject. These are finite fixtures, not a Session abstraction or action language.

The verifier checks source wire → IR, rebuilds client history through delivery/intake and compares **complete** projected native requests with the independent oracle. Assertions cover call/result order, error presence, effort scope, exact arguments, owner-bound replay, source immutability and accounting. Returned manifests remain unchanged. The three client APIs have static intake checks; all five variants use 13-byte and one-byte SSE fragmentation.

Failure forks reuse the six-turn prefix and preserve incomplete outcomes. EOF cuts remove the required closing frame without erasing preceding status/usage; exact closure errors and EOF-gated final publication are checked. Sparse Interactions counts remain unknown. Editing returned reasoning after a passing projection must reach the precise replay-dependency rejection.

Extend these cases with independent expectations, never encoder dumps or snapshot refreshes. Use synthetic labels/signatures, keep optional carriers in their variants and add another story only for a distinct invariant. Retain focused guards at their responsible layers.

This corpus exercises public codec/Attempt/delivery APIs with synthetic profiles and JSON client delivery. It does **not** establish configured binding admission, downstream incremental SSE timing, SDK/client execution, real tool behavior, billing, live Providers or all protocol pairs. Google stays upstream-SSE-only with no new native client/static intake; its [API expansion pause](roadmap.md#google-api-pause) is unchanged. Media/resources, hosted tools and other unselected semantics remain outside this corpus, not implicitly supported.

### SDK loopback gate

Use the shared locked environment from [Python checks](#python-checks). Run the ignored SDK scenarios explicitly and serially:

```sh
uv run --project tests/sdk --locked --offline cargo test --locked --offline --test sdk_loopback -- --ignored --test-threads=1
```

All listeners and credentials are synthetic and test-owned. The gate uses no real Providers or environment proxies.

The Router SDK gate validates sparse Responses envelopes and recursively required usage fields with the pinned response models, for JSON and SSE. It separately checks missing-accounting Chat usage tails, cross-protocol defaults and strict stored input-item resources. Native Messages tool loops split leading JSON whitespace before the first argument token and exercise the SDK accumulator. Source immutability, accounting precedence, lexical preservation and prefix bounds have focused Rust assertions; SDK acceptance alone does not establish those properties.

The Messages codec gate extends the same loopback suite: native manual-budget/identity controls, tool examples, document context, JSON tool input, SDK-returned call/result history, ordinary SSE, raw unfinished arguments at `max_tokens` and typed static/SSE refusal details. Its five requests use an empty child environment and scratch HOME. Raw-event acceptance does not establish replay of unfinished parameters; native JSON/history rejection remains required. Refusal details are output observations, not request-history controls.
This fixture covers codec/Attempt/delivery, not full gateway Router acceptance. Its buffered HTTP response does not measure incremental network publication; the transport suite independently checks incremental delivery and EOF withholding.

Production Router gates are organized by protocol and behavior, not roadmap package:

| Boundary | Wire-level Router tests | Pinned SDK gate |
|---|---|---|
| DashScope Chat tool/result/readable-reasoning/final history, explicit non-strict functions and caps | `tests/gateway/chat_tool_history.rs`; independent JSON/SSE history, tool identity and ordered roles | `tests/sdk/openai_tool_history.rs` → `chat_tool_history.py` |
| OpenRouter Responses phase/opaque history, stored/stateless continuation and retrieval | `tests/gateway/responses_history.rs`; effective stored/stateless history parity | `tests/sdk/openai_tool_history.rs` → `responses_history.py` |
| Native Go/Haiku Messages changed tools/system/effort, signed/redacted replay, tool-error feedback and owner-local 1h accounting | `tests/gateway/messages_tool_loop.rs`; a coherent five-request JSON/SSE loop | `tests/sdk/messages_tool_loop.rs` → `messages_tool_loop.py` |
| generateContent ordinary STOP, single-call error/correction and signed final history to Chat/Responses and selected Messages | `tests/gateway/google_tool_loop.rs`; four requests per delivery with independent native history | `tests/sdk/google_tool_history.rs` → `google_tool_history.py` |
| Interactions signed thoughts/calls, single-call correction, summary/argument deltas and supplied snapshots to Chat/Responses and selected Messages | Separate Interactions case in `tests/gateway/google_tool_loop.rs`; call-only replay uses the existing manifest | Separate Interactions case in `tests/sdk/google_tool_history.rs` → `google_tool_history.py` |
| Embedding float/Base64, scoped tokens and Google native reports | `tests/gateway/embeddings.rs`; independent native/numeric wire expectations, source/delivery receipts, budgets, malformed reports, errors and capture redaction | `tests/sdk/embeddings.rs` → `embeddings.py` and `embedding_inputs.py`; encoding defaults, tokens, Google report projection and strict complete-usage validation |

The Embedding scenarios share `tests/support/embedding_loop.rs` and explicit synthetic bindings, not production numeric encoders. Use the `embeddings` filter under the SDK command above. HTTP omission differs from SDK omission; `0.1` distinguishes exact float preservation, binary32 rounding and binary64-visible decoded JSON. Google input-only reports test derivation, while absent/detail-only Google usage and unreported OpenRouter token usage test complete request-based estimates. Strict SDK checks use explicit float: the pinned SDK still types vectors as `List[float]` and rejects Base64 before its normal decoding hook. Ordinary SDK encoding checks remain, and `embedding_inputs.py` validates actual float/Base64 response bodies against the complete OpenAPI schema. Semantic checks distinguish joint parts from independent samples, retain partial/zero reports and bound estimates without rewriting source facts. Owner-level checks retain EOF withholding, cancellation/release, failed-head handling and incomplete capture. This is shape/behavior acceptance, not estimator accuracy or billing certification; no new real Provider behavior is inferred.

`tests/support/openai_tool_loop.rs` owns fixture setup/teardown; its `chat.rs` and `responses.rs` children retain separate wire oracles. `messages_tool_loop.rs` owns the combined native tool/thinking/cache history fixture. Wire and SDK clients build continuation independently from delivered content; `messages_stream.rs` folds Messages SSE without production decoding or another test module.

`tests/support/google_tool_loop.rs` selects one API/candidate and one attempt, so the other API cannot mask failure. Its generateContent oracle covers call identity/arguments, distinct signatures, error/correction and final-history return; `google_tool_loop/interactions.rs` independently checks v1 thought/call signatures, summary/argument fragments and snapshots. Router and SDK clients build history independently, count transmissions and disable retries. Indexed Chat reasoning, trailing manifests and constant format metadata have explicit expectations. These are sequential single-call checks, not simultaneous ordering, all native deltas or installed-client/Provider acceptance.

Without content deltas, Chat content stays unreported/null, not `""`. Without a required manifest, that empty Chat history retains an assistant shell as an Interactions `model_output`; Responses needs none. Signed manifests restore recorded owners without inventing a shell or guessing segmentation. `empty_chat_carrier_is_not_an_empty_text_part` protects this distinction.

The Google Messages lane adds explicit error flags and independent complete accounting; the final Interactions turn omits accounting to verify mandatory downstream completion. Sparse source reports remain unchanged even when the delivery view estimates missing counters. The Anthropic SDK returns content unchanged across four JSON and four SSE requests per API. `src/execution/delivery/google_messages_tests.rs` separately covers signed dual-call order, source isolation, receipts, policy/context/accounting guards, EOF and cancellation; this is not real multi-call or incremental client-network acceptance.

Chat and native Messages recovery cases reuse those independent history oracles with a test-owned two-member managed credential pool. `tests/support/credential_recovery.rs` injects one unpublished `503` with `Retry-After: 0` on the primary's tool-result request. Another conversation then completes on the primary; the original conversation must keep the secondary. Run the focused Router cases with `cargo test --locked --offline --test gateway tool_feedback` and the SDK cases with the `sticks_after_primary_recovery` filter under the SDK command above. SDK HTTP hooks count client transmissions separately from observed upstream attempts; retries stay disabled. The gateway target also checks wire-created all-cooling responses, the remaining reported cooldown and no additional dispatch with the `cooling_` filter. Native upstream session identity stays stable through migration but differs for the recovery conversation. These checks do not establish real cross-account signature compatibility or installed-client recovery.

The DashScope Chat fixture's JSON tool turn carries `content: null`, while its client SSE folds initialize empty text as `content: ""`. Check the declared history facts in each delivery; do not infer byte-identical full histories from client accumulators or rewrite those delivered values to manufacture parity.

`tests/gateway/tool_publication.rs` independently checks Chat/Responses EOF withholding, cancellation, stored publication and no replay to an eligible secondary after a tool call is published. The separate native publication test in `tests/gateway/anthropic.rs` checks the same boundary for Messages, Chat and Responses delivery over a native upstream. Checks include finish/stop reasons as well as terminal markers. Configured Messages activation, all-native contract publication and rich-profile admission live in `src/gateway/startup/bootstrap/messages_tests.rs`; Router fixtures do not run that private-file loader. Shared bootstrap setup lives in `test_support.rs`, not another test module.

All authority remains synthetic and catalog contracts remain selected explicitly. Responses fixtures independently cover the selected [opening/terminal report relation](architecture.md#lifecycle-settings-and-service-reports), exact phase carriers and terminal-only stored retrieval. The SDK accumulator's null `parsed`/`parsed_arguments` attributes are SDK-only, not wire fields; remove only those checked null annotations before comparing retrieval or replaying input. Passing these regressions does not certify real Responses SSE. Live acceptance and its limitations remain in the roadmap.

#### Pinned pi history-carrier probe

The optional ignored test requires the pi release commit in [the source lock](../tests/upstream/sources.lock.json), the devShell's Node 24 LTS and `tests/sdk/pi/package-lock.json`. The probe checks the checkout revision. Keep it outside this repository and set `MORPHIECORE_PI_SOURCE`; install dependencies without private npm configuration or lifecycle scripts:

```sh
tmp=$(mktemp -d)
touch "$tmp/global.ini"
env -i PATH="$PATH" HOME="$tmp" npm --userconfig=/dev/null \
  --globalconfig="$tmp/global.ini" --prefix tests/sdk/pi ci \
  --ignore-scripts --no-audit --no-fund
MORPHIECORE_PI_SOURCE=/path/to/pinned/pi \
  cargo test --locked --offline --lib pinned_pi_owned_replay -- --ignored --nocapture
```

The existing scenario clears the child environment, uses a temporary HOME and replaces fetch with fixed synthetic SSE: 24 serial fetches, no retries, a 32-token cap and a five-second deadline. Its manifest-specific expectations are legacy compatibility characterization pending redesign, not official-protocol or new-release acceptance. Preserve useful equal-owner, exact-number, phase and next-request cases when replacing their carriers.

### Documentation checks

The fixed document set is defined in [AGENTS.md](../AGENTS.md#documentation).

For Markdown changes, check local paths and heading anchors, then run `git diff --check`. When moving sections, update incoming links across the fixed document set, verify that no contract or approval boundary was lost, and keep the original source dates. Do not add a second plan or specification at the old location. Recheck external sources when changing a protocol claim; a link check is not evidence of current API behavior.

For OpenAPI changes, also run the [schema checks](#python-checks) and compare paths, authentication and error envelopes with the gateway code. The OpenAPI file is an API summary, not a second codec implementation.

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

### Production-profile acceptance

For the selected [production closure](roadmap.md#generation-text-production-closure), begin with the actual intended client profile, not an assumption that the earlier reduced acceptance configuration is its default. Keep the existing scratch-state, unwrapped-binary, loader and redaction boundaries; reuse the existing test layout rather than another harness framework.

1. Record client version, API/model/compat flags, tool and strictness declarations, cache/session options, compaction mode/model, retries and auxiliary-request settings. Inspect only public source and test-owned configuration, never host credentials or private agent state.
2. Characterize request shapes and internal retries against a synthetic upstream first. The [source recheck](refs/2026-10-17-generation-contracts.md#production-client-profiles-2026-10-21) identifies pi's independent strict-tool downgrade and summary retry, and OpenCode's native-compaction choice, summary nudge and retry path. Count client transmissions separately from gateway attempts; a single command or disabled SDK retry does not establish a single request. A downgraded strict request does not pass the stricter profile.
3. Use ordinary Generation summarization for gateway long-session acceptance; do not select native compaction on a route without that operation. Bound all summary/continuation requests and verify the rebuilt history and next tool turn. Cache/session carriers may differ on summary calls; inspect the wire and trusted target defaults instead of inferring no caching from a client preference.
4. Reproduce the intended profile without request/history rewriting, then run the selected bounded live checks. Fix target, total request count including summaries, wire caps and timeouts before each run, with no hidden retries. Unexpected calls stop the gate rather than expanding its budget. Keep fallback/downgrade characterization synthetic unless a separately selected live contract explicitly includes it.
5. For incremental delivery, use a delayed synthetic upstream whose terminal/EOF follows actual client-visible content; separately record timing for a bounded no-tool live run. Do not remove the tool-validation guard to get a better timing result. After cancellation, verify release and no replay/success; connection closure does not prove that remote computation or billing stopped.

Source-derived expectations are not fresh acceptance. Record exclusions, including unverified account-compatible replay and process-local Responses history across restart. Preserve existing version-continuity results without treating them as acceptance of newly exercised paths.

#### Initial production-profile baseline

Use the existing `dashscope-flash-chat` and `opencode-go-haiku-messages` bindings for the three recovery profiles below. Production initially promises a single candidate per route; the synthetic two-key recovery pool is not real failover acceptance. The drivers' explicit compat/config values are authoritative; do not substitute client defaults.

| Profile | Fixed client declaration | Cache, session and auxiliary calls |
|---|---|---|
| pi Chat | `openai-completions`, `qwen3.8-flash`, low effort, 2048-token cap; developer role, strict mode and store disabled; reasoning effort and `max_completion_tokens` enabled | Short retention and OpenAI affinity headers; agent retries off, provider retries zero; warming and automatic compaction off |
| pi Messages | `anthropic-messages`, `claude-haiku-5-5`, requested low effort, 2048-token cap; adaptive thinking, strict tools and mid-conversation effort/system/tool changes enabled; temperature disabled | Short retention and native affinity; root effort remains high, requested effort is system-scoped; same retry/warming/compaction restrictions as pi Chat |
| OpenCode Chat | `@opencode/ai/providers/openai-compatible`, `qwen3.8-flash#low`, explicit 2048-token cap; only read permission | Explicit titles suppress title requests; warming and automatic compaction off; scratch persistent session resumed for continuation. Zero extra transmissions is checked, not an assertion that every client retry path is disabled |

Recovery allows exactly four client requests/five synthetic attempts per profile. It classifies initial tool declaration, tool feedback, an independent no-tool-invocation recovery conversation, and original-session continuation. It rejects duplicates, strict downgrades, summaries and other auxiliary transmissions rather than silently widening that budget. The 2026-10-21 rerun passed all three cases with local pi `1.3.0` and OpenCode `2.0.32`; this is synthetic recovery evidence only.

The final recovery verdict runs after authority sockets/workers drain, so a late auxiliary request or worker error cannot disappear behind an earlier successful snapshot. Its 2026-10-22 rerun passed all three cases with pi `1.4.0` and OpenCode `2.0.32`, at the same four/five bounds. This does not rerun the earlier versions' separate summary acceptance.

Later capability gates must fix their own schema/choice/thinking combinations; these read loops establish neither structured output nor required/named choice. The separately documented Responses profile remains distinct, including explicit non-strict tools. Long-session acceptance selects ordinary Generation summarization on the same binding/model, not native compaction or an implicit summary model. The [summary and strict-fallback gates](#installed-client-synthetic-summaries) are separate from recovery; their synthetic characterization does not accept retries, downgrade or auxiliary requests in a live capability gate.

Required sustained-operation diagnostics are narrowly scoped: observable dropped/failed logging, bounded restart-safe logging, body-free request/attempt correlation and consumer-side first-content timing against delayed terminal/EOF. The separate [sustained-operation checks](#sustained-operation-acceptance) exercise their composition; upstream first-byte or decoded-delta timing is not client visibility. Follow [Observability](architecture.md#observability), not another diagnostics framework.

### Installed-client synthetic recovery

This is a separate gate from SDK loopback and real Provider acceptance. `tests/clients.rs` creates a private synthetic credential store through `CredentialManager`; `tests/clients/recovery.py` starts the real gateway CLI with explicit scratch files and a loopback-only candidate. `drivers.py` runs pi RPC or OpenCode standalone with empty allowlisted environments, scratch HOME/XDG paths and no project resources. The command below runs recovery, summaries, strict-fallback characterization and lifecycle checks serially. Supply **unwrapped** binaries explicitly: a PATH wrapper can source host secrets even under `env -i`.

```sh
MORPHIECORE_PI_BIN=/absolute/path/to/unwrapped/pi \
MORPHIECORE_OPENCODE_BIN=/absolute/path/to/unwrapped/opencode \
  cargo test --locked --offline --test clients -- \
  --ignored --test-threads=1 --nocapture
```

Python 3 from the development environment needs only its standard library; `MORPHIECORE_CLIENT_PYTHON` optionally selects it. The harness requires the exact stable pi/OpenCode release selected in [the source lock](../tests/upstream/sources.lock.json), permitting only build metadata suffixes. This supersedes the former major-only policy. Do not downgrade/upgrade host installations: use explicitly supplied isolated release binaries and record their full versions. Legacy driver scenarios require release-specific revalidation; a version match alone is not acceptance.

Each of the three serial cases (pi Chat, pi native Messages, OpenCode Chat) fixes four client Generation requests and five upstream attempts: initial `read` on the primary, unpublished `503` on tool feedback, the same feedback completed by the secondary, an independent conversation recovering the primary, then original-session continuation on the secondary. Wire caps are 2048 tokens; candidate/route/exchange deadlines are 3/8/10 seconds, with a 120-second driver bound. The transparent admission forwarder counts client POSTs independently, rejects duplicates/excess requests and never rewrites payloads or session headers. pi retries are disabled; OpenCode's zero extra transmissions are observed, not inferred from an unsupported retry setting.

Independent wire expectations check ordered history, call/result IDs, readable reasoning and native signatures/redacted blocks. Client events and scratch OpenCode database state establish actual tool completion, final text and idle success; observed session carriers must match the clients' reported session IDs. Gateway diagnostics require honest completion, a real foreground recovery probe and zero unclassified ignored request fields. Native session carriers remain stable through migration and differ for the probe.

Owned HTTP sockets/workers and child process groups are bounded and released, including incomplete headers, timeout and cancellation. Focused authority-oracle checks live in `tests/clients/test_client_recovery.py`; descendant teardown checks live in `tests/process/child_process.rs`. These checks call no real Providers and do not prove account-compatible replay, Provider cache behavior, R1b Responses or every failure/concurrency scenario.

### Installed-client synthetic summaries

`tests/clients/summary.py` reuses the isolated drivers, bounded HTTP authority and CLI loader for pi Chat and native Messages. Each profile fixes **six client requests/six upstream attempts**: an initial read/tool-result loop, a no-tool recent user span, one manual ordinary Generation summary, then a new read/tool-result loop. The same bindings, models, compat flags, 2048-token cap and 3/8/10-second candidate/route/exchange deadlines apply. Automatic compaction, warming and client retries remain off; the candidate attempt cap is one.

The test sets `keepRecentTokens:512` and a bounded 4096-character recent user span to select a complete user boundary. It verifies the standalone summary's ordered serialized user/tool/result/answer exchange, omission of reasoning/signature prose and exclusion of the retained span. Merely setting the retained budget to zero does not mean retaining nothing: pi can select a split-turn cut, which is outside this case. The summary uses the same model, no declared tools and no implicit native-compaction endpoint.

The summary request has no caller cache markers or affinity headers. Native Messages still receives the trusted binding's five-minute automatic cache default and a distinct generated upstream session; ordinary requests retain their original client/native affinity. Native adaptive display/block-binding controls, root-high/system-low effort and the new signed tool history are checked separately. Client events establish fresh read execution and final text; projected history must retain the summary plus the recent span, without the summarized native tool exchange.

Two additional cases characterize auxiliary requests without widening the ordinary profiles:

- **Messages split summary:** `keepRecentTokens:0` cuts before the recent assistant, not before the whole turn. Exactly seven client requests/attempts include a history summary and a separate turn-prefix summary. Their serialized inputs are disjoint; rebuilt history retains the merged summary and recent assistant, not the summarized user span, then completes a fresh signed read loop.
- **Chat summary retry:** only this case enables one agent/summary retry (`maxRetries:1`, `baseDelayMs:1`); provider retries remain zero. The first summary receives an unpublished synthetic `503`, then the summary wrapper resends exactly the same native body. Seven requests/attempts produce one honest error and six completions, the scheduled/attempt/finished summary events and a successful rebuilt read loop. Ordinary/live profiles still disable this retry.

`tests/clients/strict_tools.py` separately characterizes preferred-strict behavior with transport/agent retries disabled. A direct synthetic Messages authority receives exactly two requests: strict rejection with the grammar-limit message, then a non-strict declaration with the deferred placeholder and non-tool request unchanged. The client reports its fallback diagnostic. Through the real gateway, the same upstream error produces exactly one client request/attempt, a sanitized error and no fallback or tool execution. Neither result establishes strict capability; do not expose upstream error prose to induce a downgrade.

`tests/clients/opencode_summary.py` selects ordinary manual compaction on the same Chat model/variant, with automatic compaction off and `keep:0`. A private loopback OpenCode server receives explicit-title CLI prompts and the client's `session.compact` control operation. Exactly seven gateway requests/attempts cover read/final, an unpublished summary `503`, its identical retry, one nudge after a heading-free reply, and rebuilt read/final. The nudge adds only its user instruction, not the unusable reply. The summary request preserves native call/result/reasoning history and the 2048-token ceiling; rebuilding replaces it with a checkpoint, not old tool owners. The authority rejects every extra transmission; this does not claim that OpenCode's default retry policy is disabled.

Installed OpenCode `2.0.32` lacks the pinned source's `session.wait` operation. The gate instead observes the test-owned SQLite compaction record read-only, bounded to 20 seconds with 100ms sampling, before submitting the next prompt. It requires a completed checkpoint with the exact synthetic summary, actual CLI tool/final events and final idle success. The private foreground server's requested SIGINT must exit 130 with drained pipes; the gateway must exit successfully. No host service/configuration is used.

Final verdicts run after expected client shutdown and joined HTTP workers, counting late or unsupported-method failures as failures. Diagnostics require one attempt per gateway request and zero unclassified ignored fields. Independent negative oracles live in `tests/clients/test_client_summary.py`, `test_strict_tools.py` and `test_opencode_summary.py`. The 2026-10-21 installed pi `1.3.0` runs passed the four summary cases and strict-fallback characterization; OpenCode `2.0.32` passed its separate retry/nudge/checkpoint case. This is synthetic evidence, not real Provider, native compaction, long-session capacity or client-first-content acceptance.

### Sustained-operation acceptance

`tests/clients/lifecycle.py` uses the preceding three declared profiles, the real CLI loader and direct client→gateway delivery, not the recovery gate's buffering admission forwarder. Each case fixes two Generation requests/two synthetic upstream attempts: held content → cancel, then same-session recovery. Gateway concurrency is one; candidate/route/exchange deadlines are 20/25/30 seconds, while client and observation deadlines remain bounded. A compatible two-key synthetic pool stays eligible so no extra attempt can hide behind a one-attempt configuration.

The authority flushes a nonempty text delta but withholds every block/turn terminal and HTTP EOF. Require actual consumer receipt while that stream remains open:

| Consumer | Observed boundary | Cancellation |
|---|---|---|
| pi Chat and Messages RPC | `message_update` with `assistantMessageEvent.type:text_delta` and nonempty `delta`, not lifecycle or a completed message | Correlated `abort`, aborted assistant/settlement, then idle recovery in the same process/session |
| OpenCode Chat core | Nonempty `session.text.delta` on the installed server's independently opened `event.subscribe` SSE; introspect `/openapi.json` before accepting that surface | First CLI SIGINT exits 130; matching execution reports user interruption, then the same server/session completes recovery |

OpenCode `run --format json` emits completed text blocks, not first text deltas. Its core-SSE observation proves installed agent receipt, not early CLI stdout/TUI rendering. Never relabel `step_start`, reasoning, a completed block or SDK cumulative snapshots as first text.

Before fixture teardown, passively require upstream peer EOF/reset; a cleanup-induced close or a later failed write is not the oracle. Require no cancelled success, no extra request/attempt, no post-publication advance, zero ignored fields and an honest final answer through the recovered single permit. The 2026-10-22 pi `1.3.0` and OpenCode `2.0.32` cases passed independently with two requests/attempts each. Run `cargo test --locked --offline --test clients incremental_lifecycle -- --ignored --test-threads=1 --nocapture` with the explicit binary variables above. Focused authority oracles live in `test_client_lifecycle.py`.

After observing consumer content, allow a bounded wait for the authority's post-flush acknowledgement; scheduling between flush and acknowledgement is not a delivery failure. Unsupported HTTP methods count as authority errors. The repaired cases passed independently with pi `1.4.0` and OpenCode `2.0.32` on the same date and bounds; passive pre-teardown cancellation remains required.

Service composition remains at existing owners:

- `tests/gateway/tool_publication.rs` stops reading after actual Responses content, keeps terminal/EOF unavailable, checks `429/gateway_busy` without dispatch and authentication before history lookup, then observes upstream drop and recovers within eight explicit admission probes. Exactly two upstream requests dispatch. This does not claim saturated TCP buffers, an RSS bound or measured read-ahead.
- `tests/gateway/responses_history.rs` runs real loopback services with one entry/64 KiB retention: a 96 KiB input and a full entry reject, DELETE releases capacity, GET preserves the delivered terminal JSON, and restart loses old lookup/continuation without dispatch. Four requests dispatch before restart and one independent generation after it; expiry/non-refreshing TTL stays with the explicit-clock owner tests.
- `tests/gateway/bootstrap.rs` runs the actual CLI through 1 MiB record-aligned rotation, nine-byte recognized-tail recovery, new run correlation and successful shutdown. Both files stay bounded; complete records survive, loss/failure/flush counters remain zero, and business/auth markers remain absent. Unrelated diagnostics and existing enabled capture files refuse startup before readiness or upstream calls, preserving sentinels. Fixtures use the existing private-directory owner, not relaxed permissions.
- The same bootstrap owner checks 1 KiB capture retention saturation across eight authenticated model GETs: all remain HTTP 200, no upstream calls, finite retained JSONL, exact retention-drop accounting, zero queue/write failure and redacted auth. `src/gateway/diagnostics.rs` separately proves full/disconnected metadata queues cannot change real HTTP admission errors; writer-local queue/failed-write/health oracles remain required.

These bounded checks and the ordinary summary/rebuilt-history cases establish selected long-session/resource behavior, not an unbounded endurance benchmark, persisted Responses history, remote cancellation or real multi-Provider failover.

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

#### Native Messages input acceptance

`tests/sdk/messages_inputs_live.py` is an explicit live companion, not an offline test. It starts the CLI with scratch service/model configuration, resolves only existing Go/Haiku credentials through the project loader, and uses one candidate with one attempt per request. Each case fixes a 2048-token output ceiling, 120-second attempt/route/exchange deadlines, 256 KiB client requests and 8 MiB responses. No SDK/client retry, model fallback, URL fetch, Files ID or real tool execution is involved.

Run cases separately and serially after building the selected CLI:

```sh
python3 tests/sdk/messages_inputs_live.py --gateway-bin /absolute/path/to/morphiecore examples
python3 tests/sdk/messages_inputs_live.py --gateway-bin /absolute/path/to/morphiecore context
python3 tests/sdk/messages_inputs_live.py --gateway-bin /absolute/path/to/morphiecore cache
```

The examples case fixes three requests: root examples → model-issued result with a by-value example definition → second real-call result/final answer. The context case fixes three: distinct contexts for two uses of one plain-text source, a real fetch call, then a contextual document in its result. The cache case fixes four: unique synthetic prefixes of 2,000 words, immediately repeated under explicit 5m and 1h automatic policies. Require positive reported cache writes/reads and the requested TTL write class; absent counters are not zero. Prepared 5m may use the documented omitted-default representation; 1h must remain explicit.

Every case requires honest completion, exact request/attempt counts and zero ignored request fields. Opt-in private upstream-request capture checks examples/schema/order, per-use contexts or automatic TTL without using capture as replay input; temporary captures/configuration are deleted before exit. Only fixed status/error codes and numeric reports are printed. Offline runner oracles live in `test_messages_inputs_live.py`; separate directional/native SDK gates remain required.

The 2026-10-22 Go/Haiku cases passed 3/3/4 requests respectively. The accepted cache run reported 14,056 written/read tokens for 5m and 14,055 for 1h, with matching write classes. An earlier four-request cache run completed but its final harness assertion incorrectly required an explicit 5m field; the corrected offline oracle and separate bounded rerun resolved that harness failure. An earlier runner startup type error made no HTTP request. These are bounded binding checks, not complete protocol/model conformance, measured retention, invoice accuracy, signed-thinking reuse or deployment authorization.

#### Constrained Messages profile acceptance

Run `messages_capabilities_live.py` explicitly with the [locked SDK environment](#python-checks):

```sh
uv run --project tests/sdk --locked --offline python tests/sdk/messages_capabilities_live.py \
  --gateway-bin /absolute/path/to/morphiecore
```

The existing Go/Haiku binding, loader, single-attempt routing, temporary private capture and deadlines follow the preceding native input gate. This profile fixes seven serial requests: structured JSON, structured SSE using returned history, an SSE `max_tokens:1` truncation, then required JSON and named SSE tool/result pairs. Other requests use 2048 tokens. Keep adaptive thinking with summarized display and `drop_block`, root-high/system-low effort, explicit strict tools, numeric `record` enum `[71,82]`, and single-call `disable_parallel_tool_use:true` unchanged. JSON/SSE output and tool prompts request out-of-enum `93`/`94` without supplying an allowed fallback; require an actual integer from the unchanged enum. Tool feedback follows the actual accepted argument, not a preselected answer. The named control selects `inspect` despite the prompt requesting `lookup`; result turns explicitly select `none`. Use only model-issued call IDs and untouched returned content.

Require exact JSON keys/types, source-aligned SSE termination, `max_tokens` rather than a successful schema claim on truncation, honest final answers and seven request/attempt records with zero ignored fields. Prepared captures retain schema/strictness/choice/effort and the actual conflicting initial prompts. Compare complete continuation messages against independent copies of returned assistant content and model-issued call/result identities. A test-only `mc1.` decoder derives expected native thinking signatures without modifying outgoing history; report the number actually observed. Capture units must be complete, distinct exchanges and unredacted for exact equality. JSON create may omit `stream` (native false); null or numeric substitutes fail. Offline negative oracles cover changed/dropped/reordered history, signatures, call/result pairs, initial prompts, controls, redaction and capture identity, alongside actual pinned-SDK serialization and JSON Boolean identity. Native preparation separately checks the full wire and unsupported-string-enum rejection. Do not simplify schemas or repair output.

This concrete profile passed all seven requests/attempts on 2026-10-22. A later independent three-request structured/history/truncation prefix recheck passed with explicit JSON Boolean identity. An earlier proposed string-enum tool profile stopped locally before its fourth upstream attempt; it remains rejected, not converted into this numeric profile. The investigation also repaired missing JSON-output subset validation. A separate seven-request numeric sample completed but its final capture assertion failed because the synthetic property `key` was correctly redacted as authentication-shaped; `record` avoids that fixture collision without changing the redaction policy. Capture is a redacted observation, never a raw-wire or replay authority. Failed gates report body-free counts and diagnostic classifications before scratch cleanup.

The audit-strengthened seven-request profile also passed on 2026-10-22: numeric conflicts, exact prepared initial/returned history and distinct unredacted capture all passed with seven attempts and zero ignored fields. No signed thinking block was returned in that run; signature mutation/reordering checks remain independent offline evidence, not an observed live signed-history claim. Its first seven-request run completed generation but failed the final harness check on omitted JSON `stream`; the native-default correction and negative oracle preceded a separate bounded rerun. Neither run retried a request internally.

This accepts only the declared schema/control combination. Conflict cases establish observed schema-compatible behavior despite contrary prompts, not the Provider's internal decoding mechanism. It does not certify arbitrary schemas, parallel calls, a real refusal event, other Providers or whole-client strict fallback. Existing independent SDK refusal/error oracles remain separate; no live safety refusal is induced.

#### No-tool text timing

`tests/sdk/text_timing_live.py` is a separately enabled one-request Go/Haiku check:

```sh
uv run --project tests/sdk --locked --offline python tests/sdk/text_timing_live.py \
  --gateway-bin /absolute/path/to/morphiecore
```

Use the same loader, native adaptive/root-high/system-low profile, private scratch cleanup and 120-second bounds as the preceding companion, with 2048 output tokens, no declared tools, explicit `tool_choice:none`, one candidate/attempt and zero SDK retries. Observe only source text deltas, not reasoning or cumulative SDK snapshots; reject unexpected calls and incomplete completion without executing or repairing them. Offline ordering oracles live in `test_text_timing_live.py`.

The single 2026-10-22 pinned-SDK sample passed one request/attempt: client first text at 1730.680 ms, terminal at 4568.093 ms; source first delta/elapsed were 1709/4564 ms. Client and source clocks begin at different boundaries and are reported separately. These are one-run observations, not a benchmark, latency SLO, evidence of first content before **upstream** EOF, actual installed-CLI timing, or real remote cancellation. The controlled consumer/authority gate above supplies the terminal/EOF ordering evidence.

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

For the selected Chat profile, follow the [client compatibility evidence](refs/2026-10-17-generation-contracts.md#responses-lifecycle-reports-2026-10-19). Structured `reasoning_details` is a valid returned-history carrier; do not require redundant `reasoning_content`.

For the selected Responses gate, use a test-owned `read` from `createReadToolDefinition()` with `constrainedSampling: false`; disable built-in tools and load only that extension. Keep `supportsStrictMode: true` so the builder emits `strict: false`; false omits the field and selects an unadmitted default. This is a client declaration, not a payload patch. Use low effort, short cache retention, a 2048-token wire cap and a three-request read/final/next-turn plan, with no warming, compaction or retries. A rejecting loopback guard checks caps, explicit non-strict tools, transmission counts and returned history without rewriting bodies. A run with no opaque item does not establish opaque replay.

For the R2 changed-tool gate, use RPC with the installed Haiku compatibility flags, not pi's less capable built-in `opencode-go` entry. Launch with `--tools read,ls`; a test-owned extension initially selects only `read`, then switches to `ls` using `setActiveTools`. Change structured prompt sections through `before_agent_start` and effort through RPC. Do not patch serialized payloads or returned history. Correlate command IDs and wait for `agent_settled`, not prompt acknowledgement or `agent_end`. Fix five upstream requests: read/final, switch tools and high→low effort for ls/final, then recall both results without tools.

Set the scratch model's `maxTokens` and verify the wire ceiling. Disable compaction, warming and both agent/provider retries. pi can independently retry a preferred-strict grammar-limit error without strictness; reject downgraded, duplicate or excess requests before forwarding, count client attempts separately and fail the gate on any fallback. Keep session affinity stable. The managed-effort profile retains root `output_config.effort: high`; requested effort changes belong in system messages, not root replacements.

Select retention explicitly (`PI_CACHE_RETENTION=long` for 1h). Observe only presence flags and numeric cache/TTL reports through a read-only `provider_stream_event` hook; pi's normalized zero counters cannot distinguish absent reports. A successful `drop_block` continuation alone does not prove thinking reuse. A separately bounded native JSON companion may use the already supported `"error"` binding behavior to check exact signature restoration and absence of reported transformations through opt-in private capture. Keep that companion distinct from the unmodified pi profile and remove captures afterward. Cache write classes/read counts do not measure expiry, invoices or cross-account reuse.

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

For R1a Chat, use `low`, set `body.max_completion_tokens` explicitly and check the selected `#low` variant's actual `reasoning_effort`. Verify next-turn call/result, final-answer and reasoning history, not just the first tool round.

OpenCode Responses continuation uses the selected [input-only annotations shorthand](architecture.md#client-responses-history-shorthand); check its named insertion receipt separately from ignored fields.
