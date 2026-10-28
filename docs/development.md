# Development

This guide owns tooling, test procedures and evidence reporting. Use offline, synthetic checks by default; SDK loopback, real-client and Provider checks are separate gates. [Roadmap](roadmap.md#verification-and-integration-boundary) owns which gates a slice requires, and [AGENTS.md](../AGENTS.md#authorization) owns authorization. Product setup belongs to [Usage](usage.md).

## Contents

- [Environment](#environment)
- [Test layout](#test-layout)
- [Upstream test sources](#upstream-test-sources)
- [Verification gates](#verification-gates)
  - [Offline baseline](#offline-baseline)
  - [Telemetry ecosystem checks](#telemetry-ecosystem-checks)
  - [Reusable Generation sessions](#reusable-generation-sessions)
  - [SDK loopback gate](#sdk-loopback-gate)
  - [Reverse-proxy delivery gate](#reverse-proxy-delivery-gate)
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

direnv users can allow the root `.envrc` once. Dependencies and toolchain versions are pinned via `flake.lock` and `rust-toolchain.toml`.

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
| `tests/telemetry.rs` → `telemetry/` | Test-only tracing/OTel correlation, sampling, aggregation, cancellation, bounded batch/export failure and loopback OTLP; no production instrumentation |
| `tests/sdk_loopback.rs` → `sdk/` | Pinned Python SDK loopback scenarios; ignored by default |
| `tests/clients.rs` → `clients/` | Ignored by default, Unix; explicitly selected installed clients against a synthetic CLI gateway |
| `tests/{sdk,clients,gateway}/test_*.py` | Separate Python unittest discovery for SDK oracles, installed-client oracles and OpenAPI schemas |
| `tests/upstream/{assets.py,test_assets.py}` | Offline provenance/extraction checks; explicit `--fetch` restores pinned public assets |

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
| Vercel Interactions | Unchanged stream passes terminal-first service report and terminal-without-DONE intake across fragmented chunks; static companion remains reference only |
| EventSource Parser | Literal multiline/CR/CRLF framing across every two-way byte split; no browser reconnection claim |

The connected SDK case makes eight serial synthetic requests: JSON and SSE tool responses for Chat and Messages, each followed by SDK-built history carrying a fixed result. The real client decoder re-admits that history. Chat's second fixture deliberately ends at `length`; Messages returns a final text answer. Retries/tool execution are disabled, request ceilings are 32/128 tokens, HTTP timeout is eight seconds and child timeout is 45 seconds. This uses actual Attempt/ResponseDelivery and strict pinned SDK consumers, not the Router, real Providers or installed agents. It does not establish streaming latency, backpressure or adaptation-extension retention.

### Pending standard requirements

Known differences use executable **desired-behavior** probes, not expected-failure success assertions. All currently registered probes now run in ordinary regression, including the original Interactions late-report asset:

```sh
cargo test --locked --offline --test semantic standard_contracts
cargo test --locked --offline --test transport interactions_allows_first_service_tier_report_at_terminal
```

The selected mapping package also retains independent direction, strict-null, prefix/grouping, exact structured-result, binding-policy, report-phase and signature checks at their responsible owners. Existing pinned SDK loops exercise nullable cache defaults, ordered message effort, thought placeholders and terminal-only service reports without adding a parallel harness. A compilation/setup error is not a reproduced protocol gap. Future executable unmet requirements must remain visibly separate until repaired; green current probes are not comprehensive conformance. The [Client/IR review](refs/2026-10-27-client-ir-boundaries.md) records source distinctions, explicit exclusions and unresolved SDK/schema disagreements.

### Source links and updates

- Keep upstream projects linked through test-only Git submodules or the equivalent source lock. Record repository, stable release/tag, resolved commit, package version, license and candidate fixture paths. Fetch/check out that exact revision outside production sources, with no install hooks or live fixture regeneration. Candidate paths are an inventory, not imported coverage.
- Prefer direct fixture loading. Where extraction is necessary, retain the source commit, case/path, license notices, content digest and documented transformations. Review selected material for credentials or private payloads before importing it. A public fixture is not automatically safe, authoritative or a real Provider capture.
- For this reset, current stable releases are mandatory; the former verified mapping baseline is not a fallback. Resolve package registries and official release tags together; monorepos require package-specific release tags. Sources without releases use explicitly labeled immutable snapshots. Pin all direct dependencies and refresh transitive locks within their upstream constraints; report incompatibilities rather than force unrelated major versions into the graph.
- After accepting the new baseline, resolve later stable releases as explicit candidates. Main-branch research is supplementary, not a substitute for stable release pins. Record drift and failures; offline tests never fetch floating sources or silently restore stale expected output.
- `python3 tests/upstream/candidates.py --check-latest` explicitly checks public registry/release metadata and resolves matching tag commits, including annotated and package-specific monorepo tags. It is read-only, serial, credential-free, has no retries/redirects and uses a 15-second socket timeout with a 2-MiB response cap (at most three requests per release source). Exit 1 exposes candidate drift or lookup failure; snapshots are labeled, not claimed latest. It never promotes pins/assets or installs packages. Offline unit checks inject metadata; ordinary regression has no network dependency.
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

### Text and SSE integration baseline

Verified on **2026-10-24**, using the source/asset locks and Python SDK lock (OpenAI 3.37.0, Anthropic 2.1.0, Google Gen AI 2.33.0). This closes the finite [text/SSE gate](roadmap.md#current-text-and-sse-gate), not all official features or latest-candidate acceptance.

| Boundary | Executable evidence |
|---|---|
| Provenance and updates | `tests/upstream/{assets,candidates}.py` and offline `test_*.py`; immutable source/asset digest checks in `tests/transport/upstream_assets.rs`. Candidate drift remains visible in the [source review](refs/2026-10-23-stable-protocol-contracts.md#latest-candidate-review-2026-10-24) |
| Official contracts and external consumers | Existing `standard_contracts`, pinned SDK/Vercel asset adapters and `upstream_fixture_sdk`; official Chat/Responses/Messages strict SDK consumers exercise actual Attempt/delivery and Router paths. Vercel supplies byte-locked fixtures, not a claim that its whole consumer/test suite ran |
| Complex tools and full continuation | `openai_tool_loop/{chat,responses}.rs` announces two calls and interleaves their argument fragments, finishing the second first. Existing Router and SDK history cases return results in reverse order and verify complete independent next-request arrays, including final-answer replay. Responses covers stored/stateless JSON/SSE and retrieval; Messages adds two ordered tool blocks, reverse results and exact five-turn native history with signatures/tool/effort changes. All three SDK consumers use real accumulators and strict response validation |
| Framing, mutations and ingress bounds | Existing exhaustive/property-based UTF-8, CR/LF, split and malformed-event checks remain. Imported Chat and both Google streams additionally check exact event/wire/count budgets and one-under rejection for whole-buffer/single-byte intake, then absorbing failure. Google error envelopes before/after content retain classified failures; Chat padding checks aggregate decoded UTF-8 bytes independently |
| Transport lifecycle and publication | Existing body acknowledgement/read-count assertions prove backpressure, with coalescing on/off, slow-consumer capacity, cancellation and upstream release checks. `tool_publication` adds complete protocol terminal followed by HTTP read failure: no success, stored publication or eligible-secondary replay. Native Messages retains its own publication/EOF cases |

The full offline baseline, 17 connected SDK scenarios, 20 imported assets, and upstream/SDK/client-oracle/OpenAPI Python checks must pass. Key focused selectors include `--test gateway tool_history`, `--test gateway messages_tool_loop`, `--test gateway tool_publication`, `--test transport upstream_fixtures`, and `--test transport decoded_padding`.

## Verification gates

Verification is organized into progressive, focused gates:

| Change type | Required verification procedure |
|---|---|
| Documentation-only | Local paths, anchors, and whitespace under [Documentation checks](#documentation-checks); no Rust/SDK run |
| OpenAPI schema changes | [Python schema checks](#python-checks) against gateway route contracts |
| Implementation slice | Focused tests while iterating, followed by the [Offline baseline](#offline-baseline) before publication |
| Protocol / SDK boundary | [SDK loopback gate](#sdk-loopback-gate) for connected protocol surfaces |
| Sustained delivery & proxy | [Reverse-proxy delivery gate](#reverse-proxy-delivery-gate) |
| Synthetic agent recovery | [Installed-client synthetic recovery](#installed-client-synthetic-recovery) (pi and OpenCode) |
| Live Provider behavior | [Online acceptance](#online-acceptance) with explicit target, request budget, and serial execution |

For diagnostic extensions, safe-to-ignore fields do not require dedicated tests during implementation slices; test coverage focuses on standard semantic contracts, bounds, cancellation, and terminal honesty.

**Delivered fidelity baseline, 2026-10-27:** `c8d0c96` and `14bddfe` contain the parsing/stream corrections, single Messages signature validation, narrow text/token-cap projection and original-request Responses metadata echo. The latter passed the full offline baseline, 17 SDK scenarios and 12 installed-client synthetic scenarios (pi 1.4.0, OpenCode 2.0.32). These are historical checks, not fresh Provider or deployment acceptance.

**Bounded design check, 2026-10-28:** three disposable public-API probes at `1e3fdca` confirmed an overlapping Messages block emitted from a valid IR prefix but rejected by native intake, rejection of an earlier owner's delayed first part, and rejection of a complete nonempty JSON fragment after a nonempty tool opener. The probes asserted current limitations, not repaired behavior, and were removed. Existing `--test semantic requests_reject_unselected_controls_while_incidental_reports_project` (one test) and `--test semantic native_controls` (seven tests) passed with `--locked --offline`, confirming preset conflicts, default ordering and explicit-cap/model-limit guards. No full baseline, connected SDK/client or live Provider gate was rerun for that documentation slice. Its source-derived SDK callback concerns are subsequently exercised under the [SDK loopback gate](#sdk-loopback-gate).

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

The committed configuration templates are exercised by `cargo test --locked --offline --lib committed_configuration_templates`: copy them into private test storage, prove unchanged Agent-key placeholders reject, replace only those keys with synthetic values, then load each model variant through the real startup path with test-owned credential pools. Capture and diagnostics stay disabled. This is configuration/catalog validation, not Provider access. Validate the Nginx template with `nginx -t` after replacing its listen address with a test-owned unprivileged loopback port and using temporary certificate/log/body paths. Do not assume configuration testing cannot open files or attempt socket binds; never start or reload a host service as part of this check.

Generation Text discovery has focused wire, source-domain projection, common-subset, independent context/input/output, unknown-limit, image placement/detail, bounds, authentication and privacy checks in `cargo test --locked --offline --lib gateway::tests::models::`. `protocol::model_limits` and `configured_failover_orders_and_limits_a_models_providers` cover narrowing and invalidation after model/origin/path changes. Synthetic CLI startup checks confirm Generation-only publication without upstream calls; `tests/gateway/test_openapi_schema.py` validates active shapes and schema-only task branches, including rejection of those branches by the active list/detail schema.

Pi integration is maintained by the workstation's provider extension. Use disposable, test-owned state to load both its source and generated instrumented entry through the installed Pi loader, refresh with synthetic discovery data, and intercept native Responses requests. Check discovered-versus-fallback capacities, offline/failed-refresh budget narrowing, excluded cached protocols/operations/origins, the native minimum output cap, user/tool-result images with explicit auto detail, task exclusions and cancellation. Do not patch Pi, inspect personal auth/model caches or call real Providers for this offline gate. Existing SDK Models consumption remains a standard-API regression gate; neither metadata publication nor synthetic Pi payload checks certify real Provider effectiveness.

Focused examples:

```sh
cargo test --locked --offline --lib automatic_refresh
cargo test --locked --offline --lib gateway::renewal
cargo test --locked --offline --test semantic tool_calls::reported_tool_results::
cargo test --locked --offline --test semantic provider_observations::
cargo test --locked --offline --test gateway bootstrap::
cargo test --locked --offline --test process
```

Automatic OAuth renewal is verified via unit tests for eligibility, identity pinning, exclusivity, deduplication, cooldown, deadlines, and listener cancellation.

### Telemetry ecosystem checks

MorphieCore's telemetry stack (`tracing` and OpenTelemetry) is tested offline via synthetic tracing and log events:

```sh
cargo test --locked --offline --test telemetry
cargo tree --locked --offline -d
```

Run the target normally, not directly with `--ignored`. Its ordinary wrapper reuses the owned-child helper to run four environment-cleared checks serially, with a 45-second deadline and 64-KiB output caps. The ignored children are executed and their successful completion checked by that wrapper, not omitted from the offline baseline. Only synthetic OTEL settings enter the child; no inherited credential, proxy, resource or exporter environment is used.

The in-memory SDK checks assert exact request/attempt/projection log-span attribution, one failed attempt beside one successful request, outcome-specific counters and a duration histogram. Events use the log bridge only, avoiding duplicate trace events. `skip_all` and target filtering exclude synthetic payload/error sentinels; this is not automatic redaction of arbitrary approved fields. The default log bridge does not copy request fields from tracing spans, so independently queried log fields need explicit recording. Scoped dispatch must remain active during span destruction and registry parent cleanup; the test retains the attempt owner handle through task join and releases it under that dispatch.

The abort check runs with sampling on and off: logs remain available, an unfinished worker closes its span without a fabricated terminal event, and span closure alone classifies neither success nor cancellation. This is a synthetic instrumented worker, not a gateway/SSE-body acceptance claim. Existing gateway publication, EOF, body-drop and cancellation tests retain that authority.

The loopback receiver observes actual protobuf HTTP export, identical retry bodies after one `503`, non-retryable `400`, timeout and pre-HTTP oversize rejection. Endpoint builder precedence is exercised against a synthetic environment endpoint; headers reveal a limitation: the reviewed SDK merges a synthetic environment-only header even with explicit builder headers. Content-type/bytes and response handling are checked, not backend ingestion, indexing, TLS, full protobuf field decoding or retained queries. No listener or Provider exists outside test-owned loopback.

A gated test exporter deterministically holds one batch: two queued records remain, six overflow records drop, and the separate local shutdown summary exposes that count without re-exporting SDK health. The first warning inside the tracing bridge may be suppressed by reentrancy protection; it is not a reliable standalone overflow alarm. Shutdown timeout ends the caller's wait, not the exporter; explicit release reaches exporter shutdown afterward. Stable runtime drop/failure health and configuration isolation still need production integration. A later successful empty flush is not recovery of a failed batch, and neither flush nor shutdown is a durable-delivery receipt.

Domain-evidence checks remain separate from this ecosystem prototype. `execution::report::tests` covers complete/partial/abandoned batches, empty-effect progress, association saturation, failure preservation and successful recovery. Existing delivery/accounting tests check individual exact/estimated counters and source immutability; protocol tests compare closed shape/carrier/resource causes rather than deleted free-form labels. Gateway checks retain decoder-initialization failures, source/public-status separation, cancellation and validated EOF. Run the normal offline baseline and all 16 connected SDK scenarios after this migration. Standard response/retrieval comparisons ignore the optional `morphiecore` field; they do not establish diagnostic parity or SDK retention.

Production local wiring has separate owner-local checks: `cargo test --locked --offline --lib gateway::diagnostics`. They inspect actual JSONL records from scoped SDK exporters: one structured report, request/attempt trace IDs, recovered statuses without phantom interruption, independent runtimes, body-abort/provider ownership, allowlisted events and exact wide-counter encoding. An ordinary wrapper runs its ignored configuration child through the owned-child helper with a cleared environment, synthetic hostile OTEL settings, a 25-second deadline and 32-KiB output caps; do not run that child manually with inherited environment. Existing private-file, queue/disconnection and write/flush-failure tests remain authoritative. Local file checks do not prove OTLP ingestion, Collector query retention, full-process power-loss durability or a general detached-worker shutdown barrier.

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
| Messages | Messages | Signed and redacted thinking, one opaque signature delta per thinking block, structured arguments, explicit error flags, response-scoped effort and disjoint cache accounting |
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

The same Router consumer checks two synthetic pre-frame context/quota failures as strict pinned `ResponseErrorEvent` objects. Each stream contains only the standard error event at sequence zero, with the exact public classification and no private upstream message or fabricated failed Response. Quota runs last because its evidence legitimately changes executor health. Owner/Router tests separately protect post-prefix ordering, bounds and no replay; native upstream `response.failed` parsing is not replaced by this gateway error carrier.

The Responses text codec loop also sends pinned typed input-text and tool-result cache boundaries through JSON/SSE continuation, retaining request lifetime options and checking owner-local semantics. `chat::files` supplies independent request-direction expectations and edited-owner checks; `tool_calls::reported_tool_results` covers static/SSE boundary addition/removal, repeated snapshots and rejection of native TTL reports before publication. Imported upstream streams do not contain those cases; these source-backed checks fill that bounded gap, without cache-hit or retention claims.

The Chat text codec loop sends typed named system/developer/user inputs and explicitly caller-labeled assistant history, including a call-only message, through the pinned parser's JSON/SSE continuation. Generated messages do not acquire names from request history. Source-backed `standard_contracts` assertions independently check IR ownership, exact encoding and missing-carrier rejection; context/attachment tests cover protected labels, byte bounds, selection and manifest-return edits. The Messages codec loop checks the unmodified SDK's singleton current-version header and sends ordinary `type: custom`, direct-only callers and nullable eager/cache defaults, checking equivalent default owners and unchanged tool examples. The production Router SDK loop also sends by-value tool-cache null; root/by-value directional and negative cases preserve eager false/true, real TTLs, strict known fields and hosted-execution rejection.

The Messages codec gate extends the same loopback suite: native manual-budget/identity controls, tool examples, document context, typed image-size enforcement on user/tool-result uses, JSON tool input, SDK-returned call/result history, ordinary SSE, raw unfinished arguments at `max_tokens` and typed static/SSE refusal details. Five native requests retain that scope. Four additional requests verify rejection of unsigned Chat reasoning in Messages JSON/SSE and of unsigned returned thinking history. They do not reinterpret Chat's ambiguous `stop` as Messages `end_turn`. These nine requests use an empty child environment and scratch HOME. Image URLs are inert synthetic inputs; this is not live resizing acceptance. Raw-event acceptance does not establish replay of unfinished parameters; native JSON/history rejection remains required. Refusal details are output observations, not request-history controls.
This fixture covers codec/Attempt/delivery, not full gateway Router acceptance. Its buffered HTTP response does not measure incremental network publication; the transport suite independently checks incremental delivery and EOF withholding.

The same gate adds twelve scheduling/history requests: six each through pinned Python and TypeScript consumers, using the existing `tests/sdk/pi` lock and bounded child runner. Nonmonotonic source IDs, delayed signatures, interleaved text parts and reverse-finishing calls exercise the actual Messages encoder. Both SDKs assert exclusive block lifetimes, signature-before-stop, their derived text/thinking/signature/input/stop callbacks, final objects and untouched returned history with reverse tool results. They separately consume explicitly incomplete text owners with unfinished parts, check ordered text/block callbacks and `max_tokens`, then submit the returned content unchanged for continuation. A third pair per consumer bridges Responses `metadata:null` through fragmented Attempt/ResponseDelivery, retaining the source observation and withholding the terminal until EOF. Native re-encoding independently checks complete history and original signatures. This brings the scenario to twenty-one requests without changing dependency pins. Offline semantic tests separately compare explicit JSON/SSE expectations and run fragmented Chat input through actual Attempt/ResponseDelivery with EOF withholding. Queue capacity, empty fragments, early rejection, redacted blocks, incomplete terminal drainage, error cleanup and escaped/cumulative frame bounds have focused owner tests.

`execution::delivery::tests::messages_null_metadata_matches_json_stream_and_retained_views` supplies independent Responses JSON and literal SSE expectations, including required empty event probability arrays rather than an optional static probability report. It contrasts absent/null/empty/nonempty metadata and retained `background:false`, both at setup and after committed text output. JSON/SSE and retained snapshots have the selected receipt only for null; raw rendering and constructor/update checks cannot bypass rejection. Unsupported late reports cannot finalize or recover into success. The raw Responses `store` field belongs to its storage envelope, so it is not used as an execution-hint rejection oracle.

**Verified on 2026-10-28:** Messages scheduling, incomplete text-owner closure and null-only metadata projection pass the full offline baseline, Clippy, formatting, production-only compilation, all four Python suites and 20 pinned assets. All 17 SDK scenarios and 12 isolated installed-client scenarios pass with pi 1.4.0 and OpenCode 2.0.32. The delayed-owner regression failed before scheduling; a separate regression exposed minimum deque-slot allocation exceeding its original charge, now covered by boxed pending values, conservative container charges and released drained capacity. The subsequent incomplete-owner regression failed at the SSE encoder despite valid source reduction and static delivery; it now checks both closure forms, retained lifecycle, delayed-owner ordering, missing-terminal rejection and the completed-item negative case. The null-metadata regression likewise reproduced the old delivery rejection before the selected rule was implemented. The full baseline also rechecks native-effort preference over a conflicting configured preset. No live Provider or deployment acceptance was performed. Remaining report and budget restrictions are recorded in [Roadmap](roadmap.md#remaining-issue-dispositions).

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

`tests/support/google_tool_loop.rs` selects one API/candidate and one attempt, so another API cannot mask failure. generateContent and Interactions keep independent signature/history oracles. Positive Router/SDK correction loops use managed Responses continuation; signed stateless Chat/Messages lanes verify standard errors or aborts without successful termination. Transmissions are counted and retries disabled. These are sequential single-call checks, not installed-client/Provider acceptance.

Without content deltas, Chat content stays unreported/null, not `""`. An empty Chat history retains an assistant shell as an Interactions `model_output`; Responses needs none. `empty_chat_carrier_is_not_an_empty_text_part` protects this distinction. No manifest reconstructs hidden owners or segmentation.

`src/execution/delivery/google_messages_tests.rs` separately checks unsigned dual-call order, source isolation, receipts, policy/context/accounting guards, EOF and cancellation. Signed text/call rejection covers both stateless Chat and Messages. Sparse source reports stay unchanged when delivery estimates required counters.

Chat and native Messages recovery cases reuse those independent history oracles with a test-owned two-member managed credential pool. `tests/support/credential_recovery.rs` injects one unpublished `503` with `Retry-After: 0` on the primary's tool-result request. Another conversation then completes on the primary; the original conversation must keep the secondary. Run the focused Router cases with `cargo test --locked --offline --test gateway tool_feedback` and the SDK cases with the `sticks_after_primary_recovery` filter under the SDK command above. SDK HTTP hooks count client transmissions separately from observed upstream attempts; retries stay disabled. The gateway target also checks wire-created all-cooling responses, the remaining reported cooldown and no additional dispatch with the `cooling_` filter. Native upstream session identity stays stable through migration but differs for the recovery conversation. These checks do not establish real cross-account signature compatibility or installed-client recovery.

The DashScope Chat fixture's JSON tool turn carries `content: null`, while its client SSE folds initialize empty text as `content: ""`. Check the declared history facts in each delivery; do not infer byte-identical full histories from client accumulators or rewrite those delivered values to manufacture parity.

`tests/gateway/tool_publication.rs` independently checks Chat/Responses EOF withholding, cancellation, stored publication and no replay to an eligible secondary after a tool call is published. The separate native publication test in `tests/gateway/anthropic.rs` checks the same boundary for Messages, Chat and Responses delivery over a native upstream. Checks include finish/stop reasons as well as terminal markers. Configured Messages activation, all-native contract publication and rich-profile admission live in `src/gateway/startup/bootstrap/messages_tests.rs`; Router fixtures do not run that private-file loader. Shared bootstrap setup lives in `test_support.rs`, not another test module.

All authority remains synthetic and catalog contracts remain selected explicitly. Responses fixtures independently cover the selected [opening/terminal report relation](architecture.md#lifecycle-settings-and-service-reports), exact phase carriers and terminal-only stored retrieval. The SDK accumulator's null `parsed`/`parsed_arguments` attributes are SDK-only, not wire fields; remove only those checked null annotations before comparing retrieval or replaying input. Passing these regressions does not certify real Responses SSE. Live acceptance and its limitations remain in the roadmap.

Managed Responses continuation composes typed history rather than serializing retained items into temporary wire. Native Responses Router/SDK stored-versus-stateless cases remain independent request oracles. Google Router/SDK correction loops now use `previous_response_id`, with independent upstream signature/history expectations and no public manifest reasoning item. Execution checks distinguish execution-owned storage from metadata, static/SSE delivery, stateless rejection, typed signed equal-text/call owners, exact large integers and public input listing. Owner-local cases retain resource/citation identity collisions, empty-envelope append boundaries, sealed compaction, stale retained-prefix receipts and empty-part storage charges. The bounded-history Router case verifies that a large empty-part vector cannot bypass capacity with a tiny response. Messages keeps native thinking/redacted-thinking carriers, omits phase without a fabricated owner and rejects hidden text/call signatures. Former Google-to-Messages signed-loop success cases are replaced by Router and strict SDK failure checks; ordinary unsigned terminal-projection cases and native Messages success loops remain. Standard Chat text/tool, recovery and publication loops use plain source content; a separate real Bailian `reasoning_content` fixture checks generated-reasoning rejection. Official reasoning controls/usage counters remain covered. Real-client reacceptance remains separate.

### Reverse-proxy delivery gate

The [proxy contract](architecture.md#reverse-proxy-delivery) and [deployment procedure](usage.md#reverse-proxy-deployment) have a separate synthetic gate. Reuse the installed-client lifecycle scenarios with an explicit Nginx binary:

```sh
MORPHIECORE_PI_BIN=/absolute/path/to/current/unwrapped/pi \
MORPHIECORE_OPENCODE_BIN=/absolute/path/to/current/unwrapped/opencode \
MORPHIECORE_NGINX_BIN=/absolute/path/to/nginx \
  cargo test --locked --offline --test clients incremental_lifecycle \
  -- --ignored --test-threads=1
```

Nix users can obtain scoped tooling with `nix build --no-link --print-out-paths --inputs-from . nixpkgs#nginx`; use the returned `bin/nginx`, not a host daemon. The harness runs Nginx in the foreground on loopback with test-owned configuration, uploads, logs and processes. Only response encoding is logged. Omitting `MORPHIECORE_NGINX_BIN` retains the direct-client gate; it is not an implicit skip of requested proxy acceptance.

Each case makes two synthetic requests with the CLI's default coalescing: after immediate first content, merge two further fragments and observe their complete text in one client delta while the source is silent and terminal/EOF remain withheld; cancel and require upstream socket release, then complete a new request with the single gateway permit. An eligible second credential cannot hide a post-publication retry. Nginx's two access rows must confirm gzip. This covers pi Chat/Messages and OpenCode Chat without private configuration or real Providers; it does not certify TLS, HTTP/2/3, gzip corruption/truncation handling, radio loss, compression ratios or a production deployment.

The proxy case establishes content visibility before terminal/EOF, not a first-prefix latency bound. The separate owner-level checks establish that coalescing does not intentionally hold the first content batch.

**Verified on 2026-10-26:** all three proxy lifecycle cases pass with pi 1.4.0, OpenCode 2.0.32 and Nginx 1.30.6. The default-enabled slice also passes the full offline baseline, production-only compilation, Python checks, all 16 SDK cases and all 12 direct installed-client scenarios. These are separate synthetic gates; no real Provider, TLS deployment or weak-network acceptance was run.

The deterministic window check uses actual intake/projection/encoding with a simulated clock, 101 four-byte text fragments, 5/20 ms arrival gaps and 10/50/100 ms windows:

```sh
cargo test --locked --offline --lib paced_text_windows -- --nocapture
```

It independently checks exact content, immediate first content, fixed-window delay bounds and batch counts; it reports pre-compression opening/text-frame bytes (no terminal frames) and intentional delay. The 20 ms cadence produced:

| Window | Nonempty text events | Encoded opening/text bytes | Mean intentional delay |
|---|---:|---:|---:|
| 10 ms | 101 | 17,405 | 9.90 ms |
| 50 ms | 35 | 6,515 | 29.90 ms |
| 100 ms | 21 | 4,205 | 59.41 ms |

The gateway body test separately drives the real timer during source silence and preserves bounded handoff/backpressure. These checks explain window trade-offs, not real throughput: no proxy, gzip, network queue, packet loss or scheduler delay is present in the simulated-clock measurement. Compare identical source streams when extending it; never compare independent model generations as a compression benchmark.

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

Do not remove safety invariant tests solely because their code paths are exercised by other tests.

## Online acceptance

Record the tested binary version separately from the pinned source revision. An installed-client gate does not establish behavior for a different pinned revision or every protocol route.

### Production-profile acceptance

For the selected [production closure](roadmap.md#generation-text-production-closure), begin with the actual intended client profile, not an assumption that the earlier reduced acceptance configuration is its default. Keep the existing scratch-state, unwrapped-binary, loader and redaction boundaries; reuse the existing test layout rather than another harness framework.

1. Record client version, API/model/compat flags, tool and strictness declarations, cache/session options, compaction mode/model, retries and auxiliary-request settings. Inspect only public source and test-owned configuration, never host credentials or private agent state.
2. Characterize request shapes and internal retries against a synthetic upstream first. The [client source guide](refs/2026-10-27-client-and-proxy-sources.md#pi-actual-consumer-and-history-behavior) identifies pi's independent strict-tool downgrade and compaction behavior; its dated OpenCode observations distinguish native compaction and summary nudges. Count client transmissions separately from gateway attempts; a single command or disabled SDK retry does not establish a single request. A downgraded strict request does not pass the stricter profile.
3. Use ordinary Generation summarization for gateway long-session acceptance; do not select native compaction on a route without that operation. Bound all summary/continuation requests and verify the rebuilt history and next tool turn. Cache/session carriers may differ on summary calls; inspect the wire and trusted target defaults instead of inferring no caching from a client preference.
4. Reproduce the intended profile without request/history rewriting, then run the selected bounded live checks. Fix target, total request count including summaries, wire caps and timeouts before each run, with no hidden retries. Unexpected calls stop the gate rather than expanding its budget. Keep fallback/downgrade characterization synthetic unless a separately selected live contract explicitly includes it.
5. For incremental delivery, use a delayed synthetic upstream whose terminal/EOF follows actual client-visible content; separately record timing for a bounded no-tool live run. Do not remove the tool-validation guard to get a better timing result. After cancellation, verify release and no replay/success; connection closure does not prove that remote computation or billing stopped.

Source-derived expectations are not fresh acceptance. Record exclusions, including unverified account-compatible replay and process-local Responses history across restart. Preserve existing version-continuity results without treating them as acceptance of newly exercised paths.

For the explicit readable Chat real-Agent gate, use the current installed unwrapped binary:

```sh
python3 tests/clients/reasoning_live.py pi-chat \
  --gateway /absolute/path/to/morphiecore \
  --client-binary /absolute/path/to/current/unwrapped/pi
# Run separately and serially with opencode-chat and the current OpenCode binary.
```

This runner fixes `aliyun-tokenplan-cn-chat` / `qwen3.8-flash`, three Generation requests, 2048 output tokens each, one candidate/attempt and 120-second attempt/exchange deadlines. It admits an actual read/tool-result/final-answer loop and same-session recall, rejecting extra transmissions before forwarding. It reuses the ordinary CLI credential loader and isolated Agent drivers; no host Agent configuration or credentials are read by the runner. The test-owned loopback forwarder observes unchanged client deliveries in memory, capped at 2 MiB per response and 1 MiB per event, and verifies complete EOF/DONE/terminal structure. It compares delivered reasoning, text and call identities/argument values against client replay and complete prepared next-request history, including role order and tool results. Prepared-request capture must be complete and unredacted; response capture may intentionally exclude unsafe tool-argument fragments. Production redaction is unchanged, raw response observations are never persisted, temporary captures are deleted, and reports expose only counts/fixed classifications. This observes delivery→client replay→preparation, not an independent raw Provider→delivery comparison, real failover, summarization or timing. Use an outer process bound of 420 seconds.

#### Initial production-profile baseline

**Bounded live evidence, 2026-10-24/25:** host-installed pi 1.4.0 and OpenCode 2.0.32 completed the real `aliyun-tokenplan-cn-chat` / `qwen3.8-flash` reasoning/tool/history gate, each with three client requests and three attempts, 2048 output tokens per request and 120-second deadlines. This retains the earlier observation, not fresh acceptance of subsequent codec changes or a real-failover/summarization claim.

The verified baseline uses host-installed `pi` and `opencode` with isolated test configurations:
- **pi Chat:** readable reasoning on `aliyun-tokenplan-cn-chat` (`qwen3.8-flash`)
- **pi Messages:** native Messages on `opencode-go-haiku-messages` (`claude-haiku-5-5`, `anthropic-version: 2026-10-21`)
- **OpenCode Chat:** OpenAI-compatible on `aliyun-tokenplan-cn-chat`

Synthetic recovery profiles configure single-candidate routes with explicit client parameters:

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

Python 3 from the development environment needs only its standard library; `MORPHIECORE_CLIENT_PYTHON` optionally selects it. Always use the host's current installed pi, OpenCode and other Agents, not binaries installed to match protocol source pins. The harness validates and reports actual version strings without requiring source-lock equality. Use safe unwrapped launchers with isolated test state; do not downgrade, replace or reconfigure host installations. Record full actual versions and revalidate changed client behavior; a version string alone is not acceptance.

Each of the three serial cases (pi Chat, pi native Messages, OpenCode Chat) fixes four client Generation requests and five upstream attempts: initial `read` on the primary, unpublished `503` on tool feedback, the same feedback completed by the secondary, an independent conversation recovering the primary, then original-session continuation on the secondary. Wire caps are 2048 tokens; candidate/route/exchange deadlines are 3/8/10 seconds, with a 120-second driver bound. The transparent admission forwarder counts client POSTs independently, rejects duplicates/excess requests and never rewrites payloads or session headers. pi retries are disabled; OpenCode's zero extra transmissions are observed, not inferred from an unsupported retry setting.

Independent wire expectations check ordered history, call/result IDs, readable reasoning and native signatures/redacted blocks. Client events and scratch OpenCode database state establish actual tool completion, final text and idle success; observed session carriers must match the clients' reported session IDs. Gateway diagnostics require honest completion, a real foreground recovery probe and zero unclassified ignored request fields. Native session carriers remain stable through migration and differ for the probe.

Owned HTTP sockets/workers and child process groups are bounded and released, including incomplete headers, timeout and cancellation. Focused authority-oracle checks live in `tests/clients/test_client_recovery.py`; descendant teardown checks live in `tests/process/child_process.rs`. These checks call no real Providers and do not prove account-compatible replay, Provider cache behavior, R1b Responses or every failure/concurrency scenario.

### Installed-client synthetic summaries

`tests/clients/summary.py` reuses the isolated drivers, bounded HTTP authority and CLI loader for pi Chat and native Messages. Each profile fixes **six client requests/six upstream attempts**: an initial read/tool-result loop, a no-tool recent user span, one manual ordinary Generation summary, then a new read/tool-result loop. The same bindings, models, compat flags, 2048-token cap and 3/8/10-second candidate/route/exchange deadlines apply. Automatic compaction, warming and client retries remain off; the candidate attempt cap is one.

The test sets `keepRecentTokens:512` and a bounded 4096-character recent user span to select a complete user boundary. It verifies the standalone summary's ordered serialized user/tool/result/answer exchange, omission of reasoning/signature prose and exclusion of the retained span. Merely setting the retained budget to zero does not mean retaining nothing: pi can select a split-turn cut, which is outside this case. The summary uses the same model, no declared tools and no implicit native-compaction endpoint.

The summary request has no caller cache markers or affinity headers. Native Messages still receives the trusted binding's five-minute automatic cache default and a distinct generated upstream session; ordinary requests retain their original client/native affinity. Native adaptive display/block-binding controls, current-host root-low/system-low effort and the new signed tool history are checked separately. Earlier root-high evidence retains its dated profile; the 2026-10-25 pi 1.4.0 synthetic recheck observed root-low/system-low independently of the unchanged version label. Client events establish fresh read execution and final text; projected history must retain the summary plus the recent span, without the summarized native tool exchange.

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

The gateway CLI loads `gateway.json`, its selected Agent-key `auth.json`, `models.json` and pools through the project loader. A test harness must do the same; do not hand-parse credential files.

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

For the selected Chat profile, follow [Chat reasoning extensions](architecture.md#chat-reasoning-extensions). The explicit private profile carries unsigned `reasoning_content`; do not infer standard Chat support or accept structured `reasoning_details` from an older client compatibility note.

For the selected Responses gate, use a test-owned `read` from `createReadToolDefinition()` with `constrainedSampling: false`; disable built-in tools and load only that extension. Keep `supportsStrictMode: true` so the builder emits `strict: false`; false omits the field and selects an unadmitted default. This is a client declaration, not a payload patch. Use low effort, short cache retention, a 2048-token wire cap and a three-request read/final/next-turn plan, with no warming, compaction or retries. A rejecting loopback guard checks caps, explicit non-strict tools, transmission counts and returned history without rewriting bodies. A run with no opaque item does not establish opaque replay.

For the R2 changed-tool gate, use RPC with the installed Haiku compatibility flags, not pi's less capable built-in `opencode-go` entry. Launch with `--tools read,ls`; a test-owned extension initially selects only `read`, then switches to `ls` using `setActiveTools`. Change structured prompt sections through `before_agent_start` and effort through RPC. Do not patch serialized payloads or returned history. Correlate command IDs and wait for `agent_settled`, not prompt acknowledgement or `agent_end`. Fix five upstream requests: read/final, switch tools and high→low effort for ls/final, then recall both results without tools.

Set the scratch model's `maxTokens` and verify the wire ceiling. Disable compaction, warming and both agent/provider retries. pi can independently retry a preferred-strict grammar-limit error without strictness; reject downgraded, duplicate or excess requests before forwarding, count client attempts separately and fail the gate on any fallback. Keep session affinity stable. The current installed managed-effort profile uses root `output_config.effort: low`; requested effort changes belong in system messages, not root replacements. Do not substitute an old build merely to reproduce the historical root-high profile.

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
