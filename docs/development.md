# Development

This guide owns tooling, test procedures and evidence reporting. Use offline, synthetic checks by default; SDK loopback, real-client and Provider checks are separate gates. [Roadmap](roadmap.md#verification-and-integration-boundary) owns which gates a slice requires, and [AGENTS.md](../AGENTS.md#authorization) owns authorization. Product setup belongs to [Usage](usage.md).

## Contents

- [Environment](#environment)
- [Test layout](#test-layout)
- [Upstream test sources](#upstream-test-sources)
  - [Offline and synthetic qualification evidence](#offline-and-synthetic-qualification-evidence)
- [Verification gates](#verification-gates)
  - [Offline baseline](#offline-baseline)
  - [Diagnostics behavior checks](#diagnostics-behavior-checks)
  - [Reusable Generation sessions](#reusable-generation-sessions)
  - [SDK loopback gate](#sdk-loopback-gate)
  - [Reverse-proxy delivery gate](#reverse-proxy-delivery-gate)
  - [Documentation checks](#documentation-checks)
- [Test selection and pruning](#test-selection-and-pruning)
- [Online acceptance](#online-acceptance)
  - [Production-profile acceptance](#production-profile-acceptance)
    - [Real-Agent scenario selection](#real-agent-scenario-selection)
  - [Provider qualification evidence](#provider-qualification-evidence)
    - [Tool guarantee evidence](#tool-guarantee-evidence)
    - [Protected replay evidence](#protected-replay-evidence)
    - [Agent workflow evidence](#agent-workflow-evidence)
    - [Content fidelity evidence](#content-fidelity-evidence)
    - [Replay isolation evidence](#replay-isolation-evidence)
  - [Installed-client default characterization](#installed-client-default-characterization)
  - [Installed-client synthetic recovery](#installed-client-synthetic-recovery)
  - [Installed-client synthetic summaries](#installed-client-synthetic-summaries)
  - [Real Provider checks](#real-provider-checks)
  - [pi acceptance](#pi-acceptance)
  - [opencode acceptance](#opencode-acceptance)

## Environment

[flake.nix](../flake.nix) provides the toolchain; `flake.lock` pins nixpkgs and rust-overlay. The overlay reads the exact stable version and components in `rust-toolchain.toml`; rustup and host-wide upgrades are not needed. Both shells include cargo-llvm-cov and LLVM coverage tools from that same Rust distribution. The default shell also pins Python 3.13 tooling and Node 24 LTS through nixpkgs.

```sh
nix develop          # Rust, Python 3.13 and uv
nix develop .#rust   # Rust and coverage tools
nix flake check      # Tool availability and an instrumented Cargo smoke test
```

direnv users can allow the root `.envrc` once. Dependencies and toolchain versions are pinned via `flake.lock` and `rust-toolchain.toml`. Update all Nix inputs with `nix flake update`, check the [official stable Rust manifest](https://static.rust-lang.org/dist/channel-rust-stable.toml) before changing the exact toolchain version, and run `nix flake check` before updating Cargo dependencies. The smoke test compiles and runs an instrumented crate, merges its profile and verifies an executed LCOV line; it does not replace the [offline baseline](#offline-baseline). Keep protocol/SDK reference pins independent from application dependency updates.

For Cargo updates, check current non-yanked stable releases before editing exact direct-dependency pins, then run `cargo update` and `cargo fetch --locked` inside the devShell. `cargo update --dry-run --verbose` identifies remaining older transitive versions; use `cargo tree --invert <name>@<version>` to trace their owners. Respect upstream exact constraints rather than adding root pins or patches solely to force newer versions. Validate the resulting lock with the offline baseline and affected SDK/client gates.

**Environment acceptance, 2026-10-31:** the refreshed Nix inputs, distribution-matched coverage tools and updated Cargo resolution pass `nix flake check`, the full offline baseline, all 18 SDK scenarios and all 12 isolated current-client scenarios inside the devShell on `x86_64-linux`. The other declared systems have evaluation-only evidence. The official stable Rust and direct-crate release checks require no version-pin changes; Cargo refreshes 15 transitive packages and adds `utf_types`. Remaining older `generic-array` and `matchit` releases follow exact upstream constraints from crypto-common and Axum.

## Test layout

| Entry | Scope and execution |
|---|---|
| `src/**` unit tests | Product invariants at their responsible owner, credential lifecycle and synthetic Router regressions |
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

Top-level Rust files are Cargo integration targets; nested files are registered modules, not additional targets. Keep roots small, but retain small target-local shared types there instead of adding forwarding modules. Do not flatten child modules into `tests/*.rs` merely to shorten paths. Test selection follows the lean policy in [AGENTS.md](../AGENTS.md#tests).

Shared fixture code lives in `tests/support/` and may serve multiple integration targets or source-local tests. Consumers import it directly, not through another scenario module. External static inputs live in `testdata/`: literal corpus data at `<scenario>/<api>/` and synthetic auth assets at `auth/`. Keep scenario/API boundaries, but avoid wrapper directories with no independent meaning. Generated output, reports and runtime state stay in ignored `testdata/{generated,reports,dist,runtime}/`, separate from committed inputs.

Semantic domain entries declare child modules; Rust module paths, not physical filenames or roadmap IDs, select tests. The OAuth CLI module remains `unix` in `tests/credential/oauth_accounts.rs`; the Router smoke and SDK text loops use `router_smoke::` and `text_loop::`. CLI startup checks live in `tests/gateway/bootstrap.rs`. Capture redaction, incomplete-source and retention checks live in `src/gateway/capture/tests.rs`, separate from body-free diagnostics. Cross-owner HTTP/cancellation assertions stay at their integration owner.

Bootstrap fixtures take explicit service/models JSON and synthetic credential seeds, then call the real private-file loader. Keep model selection independent of installed keys and negative cases at their intended production boundary. Raw malformed-file cases retain a byte writer; JSON values cannot preserve duplicate keys. SDK command helpers share only identical environment setup, not process ownership, protocol oracles or report assertions.

Protocol mappings need independent wire→IR and IR→wire expectations from official contracts and pinned upstream assets. Keep local assertions for IR ownership, safety or gaps in upstream coverage; do not maintain a duplicate protocol dataset or generate an oracle with the encoder under test. Round trips are supplementary.

Select checks from the relevant [roadmap package](roadmap.md#planned-order). Same-protocol fidelity needs a pinned revision/domain and independent request, response, JSON/SSE and returned-history expectations; a successful tool loop does not prove losslessness. Independent tasks and resources need their own lifecycle checks, not Generation round trips.

[Architecture](architecture.md#scope-and-activation-decisions) records approved design; [Roadmap](roadmap.md#planned-order) owns remaining scope and exit gates. Unresolved concrete semantics and new integration still need selection. Use counterexamples to support design review; do not encode unresolved defects as successful expectations. Passing checks, measured coverage and agent review are not human design approval.

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

Keep handwritten fixtures and local IR checks only when they challenge a concrete product or protocol assumption. Delete implementation restatements and their exclusive scaffolding rather than moving them into another suite. A connected or ignored SDK test does not automatically replace an independently valuable offline boundary check. Follow [test selection and pruning](#test-selection-and-pruning).

### SSE layers

| Layer | Reused evidence | Local responsibility |
|---|---|---|
| Framing | WPT EventSource parsing cases and `eventsource-parser` regressions | Incremental bytes, UTF-8, line endings, fields, delimiters and limits; classify browser reconnection/control semantics and EOF-policy differences rather than claiming full browser conformance |
| Protocol events | Official SDK and Vercel static/stream fixtures | Interleaved calls, argument fragments, text/reasoning owners, trailing reports, errors, terminal outcomes and returned history |
| Transport lifecycle | Applicable SDK backpressure, interruption, early-exit and cleanup cases | Exercise the real gateway/transport boundary for bounded buffering, slow consumers, cancellation, upstream release, validated EOF and no replay after publication |

Apply existing `proptest` machinery and bounded fuzzing where useful to reused inputs: within declared budgets, byte repartitioning and legal comments must preserve declared meaning; truncation and malformed-event mutations must not invent success. Keep seeds and minimized reproducers. Timer/slow-reader checks need controlled scheduling, not assumptions about TCP packet boundaries. Test coalescing on/off against the same source events, not independent model generations.

Upstream implementation-specific assertions require adaptation, not blind adoption. For example, an SDK may accept events larger than the gateway budget or tolerate an unfinished stream; neither behavior permits weakening MorphieCore's limits or successful-terminal rules. Preserve current focused safety tests and the reusable session corpus rather than replacing them with an aggregate external-suite score.

### Text and SSE integration baseline

Verified on **2026-10-24**, using the source/asset locks and Python SDK lock (OpenAI 3.37.0, Anthropic 2.1.0, Google Gen AI 2.33.0). This closes the finite text/SSE integration gate, not all official features or latest-candidate acceptance.

| Boundary | Executable evidence |
|---|---|
| Provenance and updates | `tests/upstream/{assets,candidates}.py` and offline `test_*.py`; immutable source/asset digest checks in `tests/transport/upstream_assets.rs`. Candidate drift remains visible in the [source review](refs/2026-10-23-stable-protocol-contracts.md#latest-candidate-review-2026-10-24) |
| Official contracts and external consumers | Existing `standard_contracts`, pinned SDK/Vercel asset adapters and `upstream_fixture_sdk`; official Chat/Responses/Messages strict SDK consumers exercise actual Attempt/delivery and Router paths. Vercel supplies byte-locked fixtures, not a claim that its whole consumer/test suite ran |
| Complex tools and full continuation | `openai_tool_loop/{chat,responses}.rs` announces two calls and interleaves their argument fragments, finishing the second first. Existing Router and SDK history cases return results in reverse order and verify complete independent next-request arrays, including final-answer replay. Responses covers stored/stateless JSON/SSE and retrieval; Messages adds two ordered tool blocks, reverse results and exact five-turn native history with signatures/tool/effort changes. All three SDK consumers use real accumulators and strict response validation |
| Framing, mutations and ingress bounds | Existing exhaustive/property-based UTF-8, CR/LF, split and malformed-event checks remain. Imported Chat and both Google streams additionally check exact event/wire/count budgets and one-under rejection for whole-buffer/single-byte intake, then absorbing failure. Google error envelopes before/after content retain classified failures; Chat padding checks aggregate decoded UTF-8 bytes independently |
| Transport lifecycle and publication | Existing body acknowledgement/read-count assertions prove backpressure, with coalescing on/off, slow-consumer capacity, cancellation and upstream release checks. `tool_publication` adds complete protocol terminal followed by HTTP read failure: no success, stored publication or eligible-secondary replay. Native Messages retains its own publication/EOF cases |

The full offline baseline, all connected SDK scenarios, pinned imported assets, and upstream/SDK/client-oracle/OpenAPI Python checks must pass. Key focused selectors include `--test gateway tool_history`, `--test gateway messages_tool_loop`, `--test gateway tool_publication`, `--test transport upstream_fixtures`, and `--test transport decoded_padding`.

### Offline and synthetic qualification evidence

These are dated results against synthetic authorities, separate from [Provider qualification](#provider-qualification-evidence) and deployment. Original client versions describe the tested environment, not compatibility targets. The domain rows retain distinct regressions and scope; Git preserves intermediate delivery logs.

| Domain / date | Meaningful result | Verification and limits |
|---|---|---|
| Protocol fidelity, 2026-10-27 | `c8d0c96` / `14bddfe`: parsing/stream corrections, single Messages signature validation, narrow text/token-cap projection and original-request Responses metadata echo | Offline baseline, 17 SDK and 12 curated client scenarios; pi 1.4.0 / OpenCode 2.0.32. No live Provider/deployment acceptance |
| Production intake and request reuse, 2026-10-30 | Corrected production-construction corpus reproduces from installed public pi-ai; strictness and request-retention/preparation checks pass. The 47-endpoint intake experiment retains its [original source findings](refs/2026-10-27-client-ir-boundaries.md#production-corpus-and-intake-experiment-2026-10-30) | Offline baseline, assets/Python, 18 SDK and 12 curated client scenarios; pi 1.4.0 / OpenCode 2.0.32. Later admission changes require their own qualification |
| Request projection, 2026-10-31 | Regressions reproduced a supposedly omitted parallel flag reaching wire, mismatched reasoning-presentation admission/loss, inferred strictness from unknown schema keywords, and lost truncation counts with 65 tool-result owners. Checks retain serial guarantees, exact source/schema effects, native Responses phase, interrupted-summary JSON/SSE/retained agreement and saturated evidence | Offline baseline, assets/Python, 18 SDK and 12 curated client scenarios; pi 1.4.0 / OpenCode 2.0.32. [Source distinctions](refs/2026-10-27-client-ir-boundaries.md#conditional-strictness-migration-2026-10-30); no default-profile or live Provider claim |
| Native partial arguments, 2026-10-31 | Incomplete numeric/Unicode-escape prefixes survive both OpenAI clients' JSON/SSE and retained snapshots. Router checks actual native SSE/Accept selection, exact deltas, honest incomplete terminals and stored retrieval across four sends; native object-history rejection stays strict | Same offline/SDK/curated-client gates as request projection. Pinned Messages streaming request types consumed; buffered fixtures do not measure network timing |
| Delivery and replay, 2026-11-01 | Common owner aligns JSON/SSE/retained arguments, text/refusal/usage, omissions, terminals and execution IDs. Late structured-Chat issuer IDs no longer change retained Responses identity after SSE publication; source issuer stays unchanged | Offline baseline, assets/Python, 18 SDK and 12 curated client scenarios; pi 1.4.0 / OpenCode 2.0.32. Non-Chat execution intake is SSE-only, with the selected Chat JSON exception |
| Replay fingerprint work, 2026-11-01 | A completed 32-KiB signed owner followed by 64 one-character text deltas formerly added 2.25 MiB of reasoning hashing. Item-boundary synchronization adds zero during that stage in JSON and SSE, retaining terminal synchronization, edited-owner rejection and source immutability | Same offline/SDK/curated-client gates as delivery. This measures hashing work, not latency |
| Native intake and discovery, 2026-11-02 | Raw sampling rounded into range or underflowed to zero rejects; mathematically integral `top_k` and extreme exponents retain exact semantics. Oracles catch lost tool/path declarations and invented effort/display/strict/eager values. History selection preserves the next retained owner; thinking-off retains explicit constraints, ingress 413 cannot publish positive default discovery, duplicate OpenAPI keys reject | Offline baseline, assets/Python, 18 SDK and 14 isolated client scenarios; pi 1.4.0 / OpenCode 2.0.32. Current default-Agent acceptance has its [own gate](#installed-client-synthetic-recovery) |
| Tool projection and strict binding, 2026-11-05 | Active-set collapse, retained additions and inventory filtering preserve source bytes, raw historical arguments, selection and legal positions/capacity. Regressions cover cardinality before admission, zero-dispatch Bailian rejection, beta-root suffix composition and collapse before budgeting. Pi's strict DeepSeek read/history loop completes in three requests/attempts, preserving `strict:true` and high effort | Offline baseline and 18 SDK scenarios; pi 1.5.0. Other default-Agent lanes were not qualified by this intermediate slice; later closure is below |
| Nine-direction function loops, 2026-11-05 | Chat/Responses/Messages target matrix passes JSON/SSE and Responses stored/stateless call/result/answer/recall. Independent oracles retain raw strings, reversed results, distinct item/call IDs, empty envelopes and complete retained input/output | Offline baseline, assets/Python and 19 SDK scenarios. Each target receives 24 Generation sends and 36 SDK HTTP requests including retrieval/listing; no live Provider claim |
| Ordered effort and Client annotations, 2026-11-06 | Native Messages five-turn JSON/SSE history retains effort/system boundaries, signed/redacted thinking and bounded failure continuation. Pi Messages and OpenCode Responses default read/result/answer/recall complete in three requests/attempts without rewriting input; selected empty-annotations L2 fixes the prior OpenCode negative. Malformed explicit values, nonempty citations and strict upstream snapshots remain guarded | Offline baseline, assets/Python, 19 SDK and 21 isolated client scenarios; pi 1.5.0 / OpenCode 2.0.32. Current versions/results live in [installed-client acceptance](#installed-client-synthetic-recovery) |
| History projection, 2026-11-06 | Reproduced premature effort activation after reasoning-only deletion, stale mixed-system membership after tool conversion and invalid pending off/effort composition. Contrasts retain cleared root effort, immediate/pending timing, native encoding and rejection of prefix changes before signed thinking even with `drop_block` | Offline baseline, assets/Python, 19 SDK and 21 isolated client scenarios; pi 1.5.0 / OpenCode 2.0.32. [Ordered effort contract](architecture.md#ordered-effort-repair-direction); no fabricated assistant boundary or relaxed replay validation |
| Convergence cleanup, 2026-11-10 | First-body state retains only a validated chunk; Generation passes its existing delivery lane; plain preparation no longer builds discarded rule summaries. Authentication preserves duplicate/mixed-header rejection and the full constant-time scan without temporary vectors. SDK launch/client setup shares existing isolation owners; unused Node source-loader and dependencies are removed | Full offline Rust baseline, all-target Clippy under `-D warnings`, formatting, production compilation, 20 pinned assets, all four Python suites and 20 serial SDK loopbacks pass. Locked Node restore needed public package download because the local npm cache was incomplete. No installed-Agent, live Provider or deployment reacceptance was run |

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

### Offline baseline

Run affected targets first, then the full baseline:

```sh
cargo test --locked --offline
cargo clippy --locked --offline --all-targets -- -D warnings
cargo fmt -- --check
cargo check --locked --offline --lib --bins
python3 tests/upstream/assets.py
python3 -m unittest discover -s tests/upstream -p 'test_*.py'
git diff --check
```

These checks use synthetic data only. Do not use real Providers or private configuration as offline test input.

Account-recovery owner checks use `cargo test --locked --offline --lib gateway::body::tests::recovery`, `gateway::generation::intake::tests`, `provider::errors::tests` and `transport::http::tests::wire_failures_distinguish` as separate selectors. They exercise real loopback HTTP with controlled response bodies/clocks, strict finite failure evidence, native failed-envelope accounting and positive interruption versus TLS/protocol/timeout facts. Source content must survive the client projection before it ends the short recovery clock; valid no-content terminals end that clock without authorizing publication before EOF. Production-loader account checks separately cover a long primary hint with a ready backup. SDK consumers and installed-Agent recovery remain separate gates.

Configuration checks exercise private-file admission and real startup failures rather than snapshotting committed examples or activated catalog sizes. Validate the Nginx template with `nginx -t` after replacing its listen address with a test-owned unprivileged loopback port and using temporary certificate/log/body paths. Configuration validation may open files or bind sockets; never start or reload a host service as part of this check.

Generation Text discovery has focused wire, source-domain projection, common-subset, independent context/input/output, unknown-limit, image placement/detail, bounds, authentication and privacy checks in `cargo test --locked --offline --lib gateway::tests::models::`. `protocol::model_limits` and `configured_accounts_preserve_order_targets_limits_and_replay_isolation` cover narrowing and invalidation after model/origin/path changes. Synthetic CLI startup checks confirm Generation-only publication without upstream calls; `tests/gateway/test_openapi_schema.py` validates public schema/reference integrity and protocol constraints without a second hypothetical discovery corpus.

**Capability publication budget, 2026-11-09:** the complete-catalog regression fails with the former ceiling and passes with the selected 4 MiB ceiling. All 64 synthetic model labels and their activated client interfaces survive publication without truncation; a configured response budget one byte below the encoded snapshot still rejects. The focused discovery checks, full offline baseline and OpenAPI schema checks pass. Generation Text discovery retains its independent bound.

Pi integration is maintained by the workstation's provider extension. Use disposable, test-owned state to load both its source and generated instrumented entry through the installed Pi loader, refresh with synthetic discovery data, and intercept native Responses requests. Check discovered-versus-fallback capacities, offline/failed-refresh budget narrowing, excluded cached protocols/operations/origins, the native minimum output cap, user/tool-result images with explicit auto detail, task exclusions and cancellation. Do not patch Pi, inspect personal auth/model caches or call real Providers for this offline gate. Existing SDK Models consumption remains a standard-API regression gate; neither metadata publication nor synthetic Pi payload checks certify real Provider effectiveness.

**Native effort/capacity discovery, 2026-11-10:** the discovery regression first failed with six advertised choices for a high-only native declaration. Route intersections now expose only unchanged native names and keep accepted aliases separate, including disjoint native domains, Qwen, GLM, Google scope conversion and native/Bailian DeepSeek. Native DeepSeek Chat/Responses use its documented low/high/max and minimal/medium/xhigh mappings; the request oracle checks actual bound encoding, not only metadata.

The installed pi **1.5.1** loader/provider experiment passes discovery → native picker → persisted model store → real Responses serializer checks, with exact low/high/max choices, advertised context/output over global fallbacks, input-only planning fallback, explicit caps, offline/failed refresh and idle active/scoped-selection reconciliation. The replay experiments remain passing. The full offline baseline, 20 serial SDK scenarios, OpenAPI checks and installed pi's strict DeepSeek read/history loop pass; that Agent loop has three requests/attempts and no extras. Authorities and credentials are synthetic; this does not qualify real Provider behavior or activate the workstation.

Request-conversion reuse has owner-local checks in `cargo test --locked --offline --lib gateway::runtime::tests` and `cargo test --locked --offline --lib gateway::routing::prefix_cache::tests`. They cover retained versus recomputed bytes/evidence, bound candidate and credential pairing after reordering, unchanged eligibility when the retention budget exhausts, spare evidence capacity, and ordinary reserved-key schema objects through actual hit/miss paths. Prefix checks retain account/target isolation and unchanged forwarded bodies rather than internal hash-placement assertions. Existing affinity, credential-recovery, cancellation and production-construction checks exercise the surrounding exchange path, including invalidation by the test-only streaming override. [Architecture](architecture.md#request-conversion-reuse) owns the retention contract; [dated findings](refs/2026-10-27-client-ir-boundaries.md#request-conversion-reuse-2026-10-30) record the reconstruction counterexamples and measured-size/lazy-prefix simplification.

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

**Headless browser login, 2026-11-09:** `cargo test --locked --offline --bin morphiecore-auth` and the `siwc_pasted_callback` credential regression pass. The subprocess check covers pasted/direct state-bound denial, open stdin at exit, and a direct code callback followed by a simultaneous paste while a test-owned proxy holds token exchange. A disposable five-case PTY check verifies hidden input, terminal restoration and cleared login state after pasted/direct denial, keyboard Ctrl-C, unfinished-paste cancellation and oversized unfinished input. The overlap and terminal-bound counterexamples fail before their fixes. The full offline baseline and `nix flake check` pass on `x86_64-linux`; no real OAuth login, other-platform runtime or production deployment is claimed.

### Diagnostics behavior checks

Run `cargo test --locked --offline --lib gateway::diagnostics` against actual production sinks and traces. Dependency-only tracing, metric and OTLP exporter feasibility tests are removed; future exporter integration needs its own product boundary.

These checks inspect actual JSONL records from scoped SDK exporters: one structured report, request/attempt trace IDs, recovered statuses without phantom interruption, independent runtimes, body-abort/provider ownership, allowlisted events and exact wide-counter encoding. An ordinary wrapper runs its ignored configuration child through the owned-child helper with a cleared environment, synthetic hostile OTEL settings, a 25-second deadline and 32-KiB output caps; do not run that child manually with inherited environment. Existing private-file, queue/disconnection and write/flush-failure tests remain authoritative. OTLP/backend integration and process durability remain separate acceptance boundaries.

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

The production-construction companion runs with `cargo test --locked --offline --lib gateway::startup::bootstrap::conformance_tests`. Its text/tool gate enumerates every API-key Generation endpoint, with an exact reviewed-membership assertion. All four subscription endpoints use test-owned OAuth accounts, real builtin validation, shared account binding and production startup; upstream auth, forced SIWC streaming and stateless storage are checked at the loopback receiver. OAuth configuration still rejects endpoint overrides; a test-only loopback transport authority replaces network delivery after catalog target validation. Captured SIWC text defaults now complete after candidate-local ceiling omission; flat tools retain their endpoint-qualified rejection, and a separately labelled uncapped Responses control remains. Reviewed API-key disposition groups describe basic admission/delivery, not interchangeable reasoning contracts. Common subscription protocol controls likewise make no claim about actual emitted Provider shapes.

Distinct controls cover Bailian summary-only history, explicitly readable DeepSeek Chat, adaptive signed/redacted Messages and Grok opaque-only continuation on an otherwise nonreasoning request. Each checks actual second-upstream history; Responses controls also compare retrieval with delivered output. API-key/subscription catalog cases and these controls compare content, usage, terminal meaning and report fields across JSON/SSE, normalizing isolated request-owned IDs, sealed carrier bytes and the Chat accumulator's empty/null shell. Non-null Responses metadata now takes the shared owner's named Chat SSE omission while JSON retains its carrier; null still suppresses request echo. The independent Responses fold requires text, summary and tool-argument deltas to agree with value completion, completed items and the terminal array; done snapshots cannot repair missing content. Native Messages and existing incomplete-reasoning controls retain their own folds and replay checks. [Source distinctions](refs/2026-10-27-client-ir-boundaries.md#binding-history-source-review-2026-10-28) leave unestablished history facts open for S11.

The committed `agent-defaults.json` retains nine bare synthetic-model HTTP bodies from installed pi-ai 1.5.0 (text, tools and medium reasoning per API), beside three bodies from the actual `getBuiltinModel` Go/Haiku record. The latter retains complete public model metadata, source digests and raw headers, changes only the origin, and uses intercepted fetch with synthetic options. The 2026-11-06 refresh leaves the nine bare bodies unchanged and updates the built-in artifact digests, `supportsMidConvoEffort`, beta headers and root-low/message-local-medium reasoning body. Regenerate for review with `node tests/clients/pi_default_shapes.mjs /absolute/path/to/installed/pi-ai`; no network request or personal configuration is used. The gate selects resolved Haiku bodies by default, separately checks bare manual-budget conversion to adaptive thinking, and checks raw resolved/version/beta headers for two-turn acceptance. Actual installed-Agent execution remains a separate gate.

The companion pins per-lane diagnostic failure codes and dispatch counts, checks client-visible failure/terminal honesty, consumes JSON/SSE outputs and checks actual second-upstream history. Current blockers report an acceptance target and their migration owner; an asserted rejection is not closure. Responses storage controls compare retrieval with delivered output, including finalized incomplete reasoning. JSON, `store:true` and cache-free Messages requests are explicitly labelled controls, not pi defaults. Messages controls cover both accepted version headers and ignored beta headers without enabling unselected body semantics. Its Rust history adapter models pi reconstruction; [actual current-client characterization](#installed-client-default-characterization) checks that assumption separately, including phase and grouped results. Native delivery still checks exact original arguments despite the corpus's JavaScript unsafe-integer rounding.

The 2026-10-30 intake experiment reran every JSON-client observation with an upstream-streaming override and compared complete JSON envelopes plus semantic diagnostics. Its 47-endpoint coverage included 45 JSON-capable source codecs and two already-streaming-only Google bindings. Chat controls exposed the metadata/annotation carrier gap on 24 admitted bindings; two TokenPlan Chat-client paths were unreachable before dispatch. The qualified non-Chat paths now use streaming execution intake permanently. [Dated evidence](refs/2026-10-27-client-ir-boundaries.md#production-corpus-and-intake-experiment-2026-10-30) owns the original experiment; paths reopened by later admission changes still need qualification.

The existing summary control sends auto/encrypted-include requests through both turns and checks effort-only upstream requests, summary IDs, JSON/SSE agreement and retention. Conditional-strictness regressions check closed/reference carriers, open/unknown fallback, parameterless tools, strict-disabled targets and unchanged source schemas. Messages adds root/inline definitions, all-conditional overflow fallback, carrier collapse before budgeting, and retained explicit-tool/output rejection. These extend the existing schema and native-preparation tests rather than add another fixture harness. The production receiver checks original captured parameter schemas and exact dialect-specific strict fields on every dispatched turn: standard Chat and Messages receive false for these open schemas, while Bailian and native Responses omit the flag. Captured Responses tools complete both synthetic turns on reviewed API-key bindings. Messages reasoning completes on the reviewed native, GLM, DeepSeek, OpenRouter Responses and Bailian Responses lanes; GLM Chat/Responses-client reasoning also completes. Bailian Responses continuation applies phase omission and qualified visible-history transfer. The stored native tool lane preserves actual SSE argument bytes in retrieval and typed retained history without changing structured source observations.

The grouped-history control uses the production loader and a phase-free Bailian Responses source, two distinct function calls/results and literal incremental argument events. JSON and SSE each make exactly three requests: stored creation, actual `previous_response_id` continuation and an equivalent stateless branch. Both continuation requests reach the authority with identical complete ordered bodies and no upstream conversation reference. Directional native/standard intake and direct-writer checks retain orphan/forward-result, duplicate-call/result and protected-replay negatives. Removing adjacency also completes the captured Chat tool lanes on Bailian Responses bindings. Separate native Responses Router/SDK controls preserve commentary/final phases through stored and stateless continuation, guarding against accidental omission on targets with native carriers. This is synthetic gateway evidence, not new Provider or installed-pi Responses default acceptance.

Use the shared locked environment from [Python checks](#python-checks). Run the ignored SDK scenarios explicitly and serially:

```sh
uv run --project tests/sdk --locked --offline cargo test --locked --offline --test sdk_loopback -- --ignored --test-threads=1
```

All listeners and credentials are synthetic and test-owned. The gate uses no real Providers or environment proxies.

The Responses history SDK scenario's ordinary three-turn lanes use a two-function `additional_tools` input batch and a returned developer batch. Independent upstream arrays check both IDs/positions, unchanged raw calls, reverse results and final-answer recall under JSON/SSE and stored/stateless history. The returned batch opens empty and receives definitions at `done`; retrieval and input-item listing consume the actual SDK types. Semantic tool cases separately check namespace/custom members, malformed/bounded input, candidate-only mutation, non-developer replay rejection, custom capability gates at both snapshots, and missing batch closure under an incomplete terminal.

The same scenario adds four requests for incomplete reasoning: JSON and SSE each retrieve and continue a stored max-token response with root definitions. It checks the genuine `response.incomplete` event, preserved incomplete owner status and opened same-issuer token on the next upstream request. The pinned Python accumulator's `get_final_response()` requires `response.completed`; incomplete cases use its typed terminal event rather than fabricate completion or claim accumulator finalization.

The existing Chat custom-tool JSON SDK turns exercise multipart plain-text grouping through `ClientAdapter::snapshot`, then return the combined text and exact custom call as history. Independent Responses wire and direct native-progress regressions cover the grouping owner; nonempty probability metadata and text after calls remain rejected. This does not activate custom-tool streaming.

The Messages completion SDK scenario makes eight requests through the production Router: Chat and Responses sources, JSON and SSE delivery, and a second turn using the actual returned Messages content. The pinned Anthropic consumer uses strict response validation, its `httpx2` transport and documented version header. Owner-level checks preserve original/effective stop restrictions, source response/terminal immutability, EOF withholding, cancellation, late report omission and the separately selected Google tool-stop policy. These are synthetic no-cache controls, not installed-Agent default acceptance.

Report-bearing Messages production controls cover normal and refusal outcomes through all three clients, JSON/SSE, actual second-upstream history and stored Responses retrieval. Native capacity classes/categories remain present; OpenAI omits only their unsupported reports while retaining content and honest terminals. Focused tests preserve supported classes, original single/lifecycle drift rejection and strict-complete rejection with an otherwise complete fixture. The independent Messages SSE fold applies all known terminal delta fields, including `stop_details` and explicit null clears.

The Router SDK gate validates sparse Responses envelopes and recursively required usage fields with the pinned response models, for JSON and SSE. It separately checks missing-accounting Chat usage tails, cross-protocol defaults and strict stored input-item resources. Native Messages tool loops split leading JSON whitespace before the first argument token and exercise the SDK accumulator. Source immutability, accounting precedence, lexical preservation and prefix bounds have focused Rust assertions; SDK acceptance alone does not establish those properties.

The same Router consumer checks two synthetic pre-frame context/quota failures as strict pinned `ResponseErrorEvent` objects. Each stream contains only the standard error event at sequence zero, with the exact public classification and no private upstream message or fabricated failed Response. Quota runs last because its evidence legitimately changes executor health. Owner/Router tests separately protect post-prefix ordering, bounds and no replay; native upstream `response.failed` parsing is not replaced by this gateway error carrier.

The Responses text codec loop also sends pinned typed input-text and tool-result cache boundaries through JSON/SSE continuation, retaining request lifetime options and checking owner-local semantics. `chat::files` supplies independent request-direction expectations and edited-owner checks; `tool_calls::reported_tool_results` covers static/SSE boundary addition/removal, repeated snapshots and rejection of native TTL reports before publication. Imported upstream streams do not contain those cases; these source-backed checks fill that bounded gap, without cache-hit or retention claims.

The Chat text codec loop sends typed named system/developer/user inputs and explicitly caller-labeled assistant history, including a call-only message, through the pinned parser's JSON/SSE continuation. Generated messages do not acquire names from request history. Source-backed `standard_contracts` assertions independently check IR ownership, exact encoding and missing-carrier rejection; context/attachment tests cover protected labels, byte bounds, selection and manifest-return edits. The Messages codec loop checks the unmodified SDK's singleton current-version header and sends ordinary `type: custom`, direct-only callers and nullable eager/cache defaults, checking equivalent default owners and unchanged tool examples. The production Router SDK loop also sends by-value tool-cache null; root/by-value directional and negative cases preserve eager false/true, real TTLs, strict known fields and hosted-execution rejection.

The Messages codec gate extends the same loopback suite: native manual-budget/identity controls, tool examples, document context, typed image-size enforcement on user/tool-result uses, JSON tool input, SDK-returned call/result history, ordinary SSE, raw unfinished arguments at `max_tokens` and typed static/SSE refusal details. Five native requests retain that scope. Six additional requests check unsigned Chat reasoning omission in Messages JSON/SSE, return the actual SDK tool content with matching feedback to the next upstream call, and reject malformed returned thinking signatures. They do not reinterpret Chat's ambiguous `stop` as Messages `end_turn`. These eleven requests use an empty child environment and scratch HOME. Image URLs are inert synthetic inputs; this is not live resizing acceptance. Raw-event acceptance does not establish replay of unfinished parameters; native JSON/history rejection remains required. Refusal details are output observations, not request-history controls.
This fixture covers codec/Attempt/delivery, not full gateway Router acceptance. Its buffered HTTP response does not measure incremental network publication; the transport suite independently checks incremental delivery and EOF withholding.

The same gate adds twelve scheduling/history requests: six each through pinned Python and TypeScript consumers, using the existing `tests/sdk/pi` lock and bounded child runner. Nonmonotonic source IDs, delayed signatures, interleaved text parts and reverse-finishing calls exercise the actual Messages encoder. Both SDKs assert exclusive block lifetimes, signature-before-stop, their derived text/thinking/signature/input/stop callbacks, final objects and untouched returned history with reverse tool results. They separately consume explicitly incomplete text owners with unfinished parts, check ordered text/block callbacks and `max_tokens`, then submit the returned content unchanged for continuation. A third pair per consumer bridges Responses `metadata:null` through fragmented Attempt/ResponseDelivery, retaining the source observation and withholding the terminal until EOF. Native re-encoding independently checks complete history and original signatures. This brings the scenario to twenty-three requests without changing dependency pins. Offline semantic tests independently check interrupted-summary events; the request-bound delivery test checks whole-owner omission across JSON, SSE and retained views without invented completion. Queue capacity, 200,000 one-byte uncarriable reasoning deltas, empty fragments, early rejection, redacted blocks, incomplete terminal drainage, error cleanup and escaped/cumulative frame bounds have focused owner tests.

`execution::delivery::tests::messages_report_omissions_match_json_stream_and_retained_views` supplies independent Responses JSON and literal SSE expectations, including explicitly empty static and event probability arrays. It contrasts absent/null/empty/nonempty metadata and retained `background:false`, both at setup and after committed text output. Null metadata and empty probabilities deliver without loss receipts; carrier-less non-null reports and execution hints finish through named omission, including after text publication. Malformed known values still fail closed, and source reduction still rejects report retractions. The raw Responses `store` field belongs to its storage envelope, so it is not used as an execution-hint rejection oracle.

The [semantic-equivalence implementation](architecture.md#semantic-equivalence-compatibility) preserves source observations and same-protocol presence while removing both null-metadata receipt paths. Focused regressions reproduced the old receipt and empty-probability rejections before the fix. Messages request requirements and native history, JSON and all three probability event forms now agree on empty arrays; nonempty/null probabilities and annotations retain their guards. Source reduction still rejects empty snapshots/final metadata that retract earlier nonempty reports. The production Messages controls now reach the independently scheduled terminal restriction instead of the empty-probability rejection.

**Semantic-equivalence verification, 2026-10-28:** full offline baseline, production-only compilation, all four Python suites, 17 SDK scenarios and 12 isolated installed-client scenarios pass (pi 1.4.0, OpenCode 2.0.32). These installed-client scenarios retain their existing curated profiles; they do not close default-profile acceptance or establish live Provider behavior.

**Verified on 2026-10-28:** Messages scheduling, incomplete text-owner closure and null-only metadata projection pass the full offline baseline, Clippy, formatting, production-only compilation, all four Python suites and 20 pinned assets. All 17 SDK scenarios and 12 isolated installed-client scenarios pass with pi 1.4.0 and OpenCode 2.0.32. The delayed-owner regression failed before scheduling; a separate regression exposed minimum deque-slot allocation exceeding its original charge, now covered by boxed pending values, conservative container charges and released drained capacity. The subsequent incomplete-owner regression failed at the SSE encoder despite valid source reduction and static delivery; it now checks both closure forms, retained lifecycle, delayed-owner ordering, missing-terminal rejection and the completed-item negative case. The null-metadata regression likewise reproduced the old delivery rejection before the selected rule was implemented. The full baseline also rechecks native-effort preference over a conflicting configured preset. No live Provider or deployment acceptance was performed. Remaining report and budget restrictions are recorded in [Roadmap](roadmap.md#protocol-boundaries).

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

`tests/support/google_tool_loop.rs` selects one API/candidate and one attempt, so another API cannot mask failure. generateContent and Interactions keep independent signature/history oracles. Positive Router correction loops cover managed Responses and explicit stateless owner replay; SDK correction loops use managed continuation. Unselected stateless Responses and signed Chat/Messages verify standard errors or aborts without successful termination. Transmissions are counted and retries disabled. These are sequential single-call checks, not installed-client/Provider acceptance.

**Stateless owner replay, 2026-11-10:** independent native history checks pass for generateContent/Interactions JSON and SSE, including tool-error correction and signed final text. Delivery checks compare owner-local tokens across item-done, terminal JSON/SSE and next native input; repeated text, exact arguments, edited-owner rejection and baseline-client isolation are covered. Selected malformed/stateful requests reject before dispatch. The optional/late Google `responseId` regression retains stable downstream identities and rejects malformed/changed reports; the corrected `in_memory` negative reaches `admission.cache_retention` without an attempt.

The installed pi **1.5.1** companion completed a real `read` tool loop, process exit/session resume and exact signed native-history replay with three client requests/three upstream attempts and no extras, with Google response IDs absent throughout. Its separate native-loader/SessionManager experiment preserves multiple identical Unicode text parts through pi's flattened text and durable signature slots, rejects edited text/arguments and changed gateway scope, strips foreign-model replay and releases the SDK reader after malformed attachment rejection. The full offline baseline passes. All authorities and credentials are synthetic; no real Provider, private configuration or host activation was used.

```sh
MORPHIECORE_PI_BIN=/absolute/path/to/current/unwrapped/pi \
MORPHIECORE_PI_REPLAY_SOURCE=/absolute/path/to/morphiecore/replay.ts \
  cargo test --locked --offline --test clients pi_responses_google_owner_replay \
  -- --ignored --nocapture
```

The Google SDK managed loops use root-definition reference schemas for JSON and equivalent inline schemas for SSE, without expanding either document. Their final continuation keeps an otherwise-unqualified length constraint under explicit none plus serial intent; independent native wire authorities require NONE/none, the unchanged complete schema and the original signed history. Both APIs retain eight transmissions. Directional semantic and preparation tests separately cover referenced constraints, cycles/scopes, pointer escapes and cached path-depth bounds; source IR remains immutable.

Without content deltas, Chat content stays unreported/null, not `""`. An empty Chat history retains an assistant shell as an Interactions `model_output`; Responses needs none. `empty_chat_carrier_is_not_an_empty_text_part` protects this distinction. No manifest reconstructs hidden owners or segmentation.

`src/execution/delivery/google_messages_tests.rs` separately checks unsigned dual-call order, source isolation, receipts, policy/context/accounting guards, EOF and cancellation. Signed text/call rejection covers both stateless Chat and Messages. Sparse source reports stay unchanged when delivery estimates required counters.

Chat and native Messages recovery cases use the shared account loader with two test-owned named accounts. `tests/support/credential_recovery.rs` returns an unpublished `503` with `Retry-After: 60` during primary tool feedback, beyond the selected short recovery budget. The client observes that failure and explicitly submits one new HTTP request with unchanged history; only that new request may select the secondary. Later turns retain the successful target. Run Router cases with `cargo test --locked --offline --test gateway tool_feedback` and SDK cases with the `tool_feedback` filter under the SDK command above. SDK retries stay disabled, and HTTP hooks separately count client requests and upstream attempts. The `cooling_` Router cases cool each account with its own request, then prove that an all-cooling request makes no upstream call. Native upstream session identity stays unchanged throughout. Health-owner tests separately cover preference after a higher-priority target recovers; these wire checks establish no live cross-account signature compatibility or installed-client recovery.

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

## Test selection and pruning

Keep a test when it can falsify a product assumption, public protocol contract or concrete external failure risk. Expectations must be independent of the implementation being checked. A useful counterexample names what a user could lose: authority, data, identity, an explicit guarantee, bounded resources or an honest terminal result.

Aggressively delete implementation-derived expectations, getter/setter and assembly checks, catalog/example snapshots, third-party self-tests and redundant permutations. Remove their exclusive helpers, fixtures, features and dependencies too. Do not relocate low-value assertions or invent replacements to preserve test counts. Clear cases should be deleted without a mandatory inventory, coverage experiment or mutation framework.

Preserve direct evidence for credential/auth isolation, trusted origins, strict JSON and resource bounds, cancellation/release, replay identity and terminal honesty. Keep independent wire→IR / IR→wire checks and connected SDK/Agent workflows at their responsible boundaries. A lower-level implementation test earns no exemption merely because it is called a regression.

For current-Agent qualification, characterize the installed serializer and independently compare the complete observed tool inventory/schema at ingress and preparation. Do not snapshot auxiliary tool names or preserve an obsolete serializer's omission as a requirement. Keep negative controls for lost declarations/parameters and meaningful history/replay changes; remove literal-default mutations, fixture-assembly echoes and helper tests already covered through their real consumer path. The real-run cap, request/attempt accounting and terminal checks remain in the drivers even when their trivial mirror tests are deleted.

Run the affected behavior gates and the offline baseline; check SDK/Agent boundaries when their drivers change. Judge completion by the removed maintenance burden and surviving product hypotheses, not line coverage, percentages or test counts. Coverage tooling remains available for optional diagnosis, but coverage measurements/comparisons are not required for pruning or used as execution status.

**Verified 2026-11-04:** the Generation/credential pruning and ownership cleanup pass the offline baseline, pinned-asset verification, all retained upstream/SDK/client/OpenAPI Python checks, connected SDK loopback and installed-Agent synthetic workflows with pi 1.5.0 / OpenCode 2.0.32. Focused credential lifecycle, renewal, source binding, CLI, delivery, cancellation and recovery checks also pass. No coverage was collected and no real Provider or private configuration was used. Later native-default closure is recorded in the [lane table](#installed-client-synthetic-recovery).

## Online acceptance

Record the tested binary version separately from the pinned source revision. Maintain only the current latest local Agent at test time: update characterization and drivers when it changes, without old-version branches or a multi-version matrix. Historical results retain their dates and tested versions. An installed-client gate does not establish behavior for a different revision or every protocol route.

### Production-profile acceptance

For [remaining production acceptance](roadmap.md#unscheduled-issues), begin with the actual intended client profile, not an assumption that the earlier reduced acceptance configuration is its default. Keep the existing scratch-state, unwrapped-binary, loader and redaction boundaries; reuse the existing test layout rather than another harness framework.

1. Record client version, API/model/compat flags, tool and strictness declarations, cache/session options, compaction mode/model, retries and auxiliary-request settings. Inspect only public source and test-owned configuration, never host credentials or private agent state.
2. Characterize request shapes and internal retries against a synthetic upstream first. The [client source guide](refs/2026-10-27-client-and-proxy-sources.md#pi-actual-consumer-and-history-behavior) identifies pi's independent strict-tool downgrade and compaction behavior; its dated OpenCode observations distinguish native compaction and summary nudges. Count client transmissions separately from gateway attempts; a single command or disabled SDK retry does not establish a single request. A downgraded strict request does not pass the stricter profile.
3. Use ordinary Generation summarization for gateway long-session acceptance; do not select native compaction on a route without that operation. Bound all summary/continuation requests and verify the rebuilt history and next tool turn. Cache/session carriers may differ on summary calls; inspect the wire and trusted target defaults instead of inferring no caching from a client preference.
4. Reproduce the intended profile without request/history rewriting, then run the selected bounded live checks. Fix target, total request count including summaries, wire caps and timeouts before each run, with no hidden retries. Unexpected calls stop the gate rather than expanding its budget. Keep fallback/downgrade characterization synthetic unless a separately selected live contract explicitly includes it.
5. For incremental delivery, use a delayed synthetic upstream whose terminal/EOF follows actual client-visible content; separately record timing for a bounded no-tool live run. Do not remove the tool-validation guard to get a better timing result. After cancellation, verify release and no replay/success; connection closure does not prove that remote computation or billing stopped.

Source-derived expectations are not fresh acceptance. Record exclusions, including unverified account-compatible replay and process-local Responses history across restart. Preserve existing version-continuity results without treating them as acceptance of newly exercised paths.

#### Real-Agent scenario selection

[Qualification evidence](#provider-qualification-evidence) owns dated coverage; [Roadmap](roadmap.md#unscheduled-issues) owns remaining qualification and deferrals. Reuse `tests/clients/drivers.py` for isolated processes/RPC, `protocol_defaults.py` for built-in-model characterization, and the existing live, summary and lifecycle runners for their specific boundaries. The common pi helper defaults to `read` and disables persisted sessions; the Responses cancellation gate explicitly selects `write`. Coding and restart cases must select their own tools and test-owned session storage. Use supported client operations, not serialized-body or returned-history patches.

Before each run, name the scenario, exact Agent/API/model/binding, changed controls, client-request and upstream-attempt bounds, auxiliary-call allowance, output/request/response caps, exchange/route deadlines and overall process bound. Keep the existing 2048-token/120-second live baseline unless the scenario needs a separately declared profile; check thinking-budget/output compatibility before dispatch. Do not expand a budget after observing excess requests. Read-only oracles may reject an invalid test request before forwarding.

For thinking/summary cases, inspect current public client source and scratch RPC/configuration first. Record the selected UI level, emitted client fields and prepared upstream fields separately, including defaults and omission. Use a short tool/result/follow-up story to check history after level changes. Distinguish reasoning-summary verbosity from conversation compaction. An SDK-emitted setting is SDK evidence; an unavailable client control is an explicit coverage boundary.

Use synthetic files, a small local coding task and bounded test-owned tool commands. Check actual file changes, exit codes and model-issued call/result associations without prescribing incidental prose or exact tool counts the task does not require. Count all actual transmissions and require honest termination. For cancellation, synchronize on the observed phase; if it never occurs within the fixed run, record it as unexercised rather than cancel at a different phase and claim coverage.

Keep a concise dated coverage inventory with the evidence in [Qualification evidence](#provider-qualification-evidence): passed, observed failure with its boundary, or unexercised with its prerequisite. Reuse exact-profile evidence when applicable. Keep private captures temporary and redacted reporting body-free; use committed runners/oracles for reusable mechanics rather than turning private exchanges into protocol fixtures. During the collection phase, record product failures and continue independent cases; repair selection follows joint review.

For repair acceptance, separate content retention, formatting, tool identity/arguments, actual execution and terminal assertions. Exercise selected production conversions with unchanged client requests and compare prepared controls and loss evidence against their disposition, rather than requiring wire equality. With a reply-only or omitted total ceiling, declare the effective native cap, if any, and retain independent byte/time/request bounds before each live run. Test a structural no-tool recovery guarantee with actual tool withdrawal or `tool_choice:none`; prompt-only no-tool instructions belong to content-following checks. Preserve original-profile failures separately from a newly selected profile's acceptance.

For the explicit readable Chat real-Agent gate, use the current installed unwrapped binary:

```sh
python3 tests/clients/reasoning_live.py pi-chat \
  --gateway /absolute/path/to/morphiecore \
  --client-binary /absolute/path/to/current/unwrapped/pi \
  --accounts "$ROOT/accounts.json" --store "$ROOT/credentials"
# Run separately and serially with opencode-chat and the current OpenCode binary.
```

This runner fixes `aliyun-tokenplan-cn-chat` / `qwen3.8-flash`, three Generation requests, 2048 output tokens each and 120-second route/exchange deadlines. Supply an absolute current-schema account file with only the selected account and Provider `recovery.recovery_timeout_ms:0`; this bounds each request to one send without a removed model attempt ceiling. The store path is also explicit; only the production loader opens these inputs. It admits an actual read/tool-result/final-answer loop and same-session recall, rejecting extra transmissions before forwarding. It reuses the ordinary CLI credential loader and isolated Agent drivers; no host Agent configuration or credentials are read by the runner. The test-owned loopback forwarder observes unchanged client deliveries in memory, capped at 2 MiB per response and 1 MiB per event, and verifies complete EOF/DONE/terminal structure. It compares delivered reasoning, text and call identities/argument values against client replay and complete prepared next-request history, including role order and tool results. Prepared-request capture must be complete and unredacted; response capture may intentionally exclude unsafe tool-argument fragments. Production redaction is unchanged, raw response observations are never persisted, temporary captures are deleted, and reports expose only counts/fixed classifications. This observes delivery→client replay→preparation, not an independent raw Provider→delivery comparison, real failover, summarization or timing. Use an outer process bound of 420 seconds.

The first client command permits 250 seconds for its two sequential Generation requests; recall permits 130 seconds for one. Pi's RPC reader honors the command deadline without a shorter idle limit and wakes on EOF or reader failure. Both live clients accept surrounding final-answer whitespace without modifying delivery or replay bytes; synthetic final-answer checks remain exact. After OpenCode recall, the runner also reads only its scratch database, requiring assistant finishes `tool-calls`, `stop`, `stop` and a successful idle outcome. Missing or extra assistant terminals and failed idle state reject even when the displayed answer is correct.

This remains a curated readable-Chat profile, separate from the fresh-account default/cross-protocol evidence below. Extending this runner by changing only the API name would not establish those contracts.

### Provider qualification evidence

These are bounded, profile-specific results, not a full model/client matrix or an active repair queue. [Roadmap](roadmap.md#unscheduled-issues) owns unresolved issues and re-entry; [Architecture](architecture.md) owns accepted semantics. Preserve failed content assertions, incomplete-capture limits and observer/setup failures separately from passing delivery. Keep body-free evidence logs under ignored `testdata/reports/`; private capture follows its existing opt-in/bounds policy and superseded debugging narratives remain in Git.

**Scope:** real samples date from **2026-11-06–09**, using pi **1.5.0** and OpenCode **2.0.32** unless a newer version is explicit. Current affected-client qualification uses OpenCode **2.2.5**; historical versions are evidence labels, not compatibility targets. OpenAI Python is **3.37.0**; Google source releases are recorded in the [SDK note](refs/2026-10-27-official-sdk-sources.md#google-function-argument-carriers-2026-11-09). Real runs use the project loader, isolated state, serial single-attempt requests, 2048 output tokens, 120-second route/exchange deadlines, 256 KiB requests / 2 MiB responses and concurrency 1. Each story fixes its total budget first; selected reply-only ceilings retain independent byte/time bounds. No native Google client API, deployment or new binding is qualified here.

#### Tool guarantee evidence

| Contract / date | Evidence | Limit |
|---|---|---|
| Google strict managed continuation, 2026-11-09 | Each API: JSON **3/3**, SSE **3/3**, automatic-validated nested-schema JSON **3/3**; required-nullable native **1/1** and repaired managed **3/3** | Complete original schema, call identity/feedback and exact marker retained; no full Agent strict-profile claim |
| Google references and no-call, 2026-11-09 | Each API: matched root-reference/inline native **1/1 per shape**; later native none **1/1** and root+nested-reference call → feedback/none → none continuation **3/3** | Zero calls under none, correct enum arguments and marker recall; schemas remain unchanged, including otherwise-unqualified length constraints under none |
| Google independent regression/SDK boundary, 2026-11-09 | Both writers and shared preparation check root/nested/nullable refs, mixed inventories, pointer escapes, cycles/scopes and cached path bounds. SDK JSON uses references, SSE inline, with explicit-none final continuation and eight transmissions/API | No reference expansion, remote fetch, general composition or enabled-call guarantee relaxation; source IR stays immutable |
| Google native counterexamples, 2026-11-08–09 | Length constraints can be ignored; one declaration can produce two calls; boolean enums can be violated; integer enum `9007199254740993` rejects. Fractional `1.25` and ordinary closed nested structures pass | Function-input validation does not inherit the text-output subset |
| Bailian forcing/serial controls, 2026-11-08 | Non-strict Qwen/GLM Responses each **3/3** SDK call/feedback/recall with serial auto/none; thinking-off Qwen Chat forced loop **3/3**. Six native controls distinguish successful serial-auto from HTTP-400 required/required-subset in default/enabled thinking | Serial permission does not supply forcing. Thinking-off Qwen Responses later passes JSON **3/3**, SSE **3/3** with exact schema/history and selected subset |
| Bailian strict counterexamples and selected relaxation, 2026-11-09 | Six native controls include out-of-enum GLM Chat strict-true and thinking-off Qwen Responses required-subset calls. Selected best-effort policy passes pi/GLM Chat **3/3** and Qwen Responses SDK JSON **3/3**, SSE **3/3** | Schemas, actual read/result, forcing and history remain; no strict-enforcement exception or separate TokenPlan live qualification |
| Bailian ordinary SDK/binding paths, 2026-11-06–09 | Qwen/GLM/DeepSeek × Chat/Responses JSON/SSE smoke **1/1** and non-strict tool loops **3/3 per cell**; DeepSeek Chat **4/4**, three DeepSeek Responses **4/4** stories, two GLM Responses **4/4** stories | A separate GLM timeout retains its failed outcome, without replacement send or assigned protocol cause; content variability is not new repair scope |

Google evidence: `ar5-selected-*-20261109.job`, `ar5-convergence-google-*-20261109.job`, `google-no-call-references-*-20261109.job`. The first three native Interactions rechecks were misread using `outputs` rather than `steps` and establish HTTP delivery only; corrected explicit repeats supply argument verdicts. The later managed fixture first failed startup without model metadata, then one direct-connect attempt returned 504 until explicit trusted proxy selection fixed that fixture. The initial bare-Python SDK invocation lacked pinned modules; the locked interpreter passes. None is a product fallback or hidden retry.

Bailian evidence: `ar5-tool-{serial-native,required-native,serial-gateway}-20261108.log` and `bailian-strict-relaxation-*-20261109.job`. A pi observer initially required nonexistent readable reasoning and stopped after one complete attempt; the target-appropriate rerun passes, retaining all ten relaxation-trial attempts as separate outcomes. No request, schema, forwarding or returned history was rewritten to make an observer pass.

#### Protected replay evidence

| Profile / date | Passing evidence | Qualification boundary |
|---|---|---|
| Current-Agent native Messages → Readable Chat, rechecked 2026-11-09 | pi 1.5.0 and OpenCode 2.2.5 each **3/3**, independent signed/redacted owners, exact envelopes, actual read/result and recall; pinned SDK JSON/SSE **6 requests** | Foreign origin, malformed envelopes, dependencies, incomplete owners and replay isolation retain independent checks |
| Real OpenCode 2.0.32 → Grok / Interactions, 2026-11-08 | Each **3/3**; Grok protected owners and Interactions thought anchors/signatures restore original native values | Interactions thought replay supplies no independent signed-call verdict |
| Real OpenCode 2.2.5 → Haiku, 2026-11-09 | Original Chat story **3/3** and explicit adaptive/medium discriminator **1/1**, both without protected owners | These complete workflows do not exercise real protected replay; existing synthetic carrier acceptance remains |

Late signature metadata neither repeats visible text nor requires whole-response buffering. The [dated carrier research](refs/2026-11-08-chat-reasoning-carriers.md) records source/consumer distinctions. Ordinary Google text/call attachments remain on their real owners, not reasoning. Existing managed Responses JSON/SSE evidence retains signed calls/text; stateless SDK/Agent samples return `codec.unsupported.replay` when no carrier exists. Current OpenCode 2.2.5 Responses still sends `store:false` and full history without `previous_response_id`; the stateless-carrier issue remains paused in the roadmap.

#### Agent workflow evidence

Current synthetic defaults/recovery are listed [separately](#installed-client-synthetic-recovery). Native real profiles below use the [production-profile procedure](#production-profile-acceptance), fresh accounts through the CLI loader and unchanged forwarding. Successful Agent rows require actual read/result/final-answer/same-session recall, honest terminals, preserved call/reasoning history and no extra transmissions.

| Native profile / date | Passing evidence | Scope |
|---|---|---|
| Readable Qwen Chat, 2026-10-24/25 | pi 1.4.0 and OpenCode 2.0.32 each **3/3** on `aliyun-tokenplan-cn-chat` / `qwen3.8-flash` | 2048 tokens/request, 120-second deadlines; historical observation, not fresh acceptance after later codec changes or real failover/summarization |
| Haiku / Grok / DeepSeek beta / Luna smoke, 2026-11-06 | Five new local account references validate; each selected profile **1/1** | Fresh API keys and managed Grok OAuth; no host/deployed configuration activation |
| Native Messages, 2026-11-06 | pi / OpenCode Go/Haiku each **3/3** | Built-in adaptive thinking, strict omitted and original signed replay |
| DeepSeek beta Chat, 2026-11-06 | pi / OpenCode each **3/3**; separate public-label smoke **1/1** | Pi's cloned built-in `deepseek-v4.1-flash-1017` uses medium default/strict read; OpenCode built-in `deepseek-flash` uses strict false. Explicit readable-Chat profile |
| Grok OAuth Responses, 2026-11-06 | pi / OpenCode built-in `xai/grok-4.7` each **3/3** | Pi medium/auto reasoning, strict omitted; OpenCode HTTP, strict false |
| Luna Responses, 2026-11-06 | OpenCode built-in `gpt-6-luna` **3/3** after bounded smoke | HTTP, strict false and encrypted history; native auto→default service resolution and distinct item-done/terminal ciphertext retain the selected single-binding authority |
| Google managed SDK continuation, 2026-11-06 | Replacement key validates with the other four references; smoke **1/1**, OpenAI SDK JSON **3/3** and SSE **3/3** | Final complete native SSE capture verifies exact signed calls/text; first optional client SSE capture is partial. No pi/OpenCode default-stateless qualification |

The 2026-11-06 Agent rows use pi **1.5.0** / OpenCode **2.0.32**, 2048-token caps, 120-second deadlines and exactly three requests/attempts per successful story. Direct synthetic authorities characterize their declarations/history first. The initial pi DeepSeek observer rejected legal declared parameters after one request; the corrected observer passes without changing delivery. Only the final pi DeepSeek/Grok reruns include disposable fourth-request/excessive-cap negative controls.

Two preliminary Gemini SSE observers expected an absent `service_tier`, while real Google reported `default`; removing that fixture-only assumption yields the final passing loop without gateway/forwarding changes. Those interrupted observations remain separate outcomes. Complete native boundaries, not a partial client capture, support the replay verdict.

The public-label, service-tier and replay repairs pass the offline baseline, assets/Python, 19 SDK scenarios and six native default-Agent gates. Regressions retain next-request use of the item-done token and exact replay-addition, incomplete-change, identity, summary and argument conflicts. These synthetic checks are separate from the real rows above; private captures and disposable live runners are not public fixtures.

The additional **2026-11-06–08** workflow samples below use the dated clients above, explicit serializers, untouched history and actual tool execution.

| Domain | Passing evidence | Scope |
|---|---|---|
| Cross-protocol history | OpenCode Responses→Haiku/Interactions, pi Messages→Grok, and final pi Luna Chat/Messages/Chat stories **3/3 each**; Interactions smoke/loops **7/7** | No default-stateless Google or full signed-call profile extrapolation |
| Coding/repair | pi Haiku **6/6**, DeepSeek **8/8**, Grok **7/7**, Qwen Responses **7/7**; OpenCode Haiku **6/6** | Failure/read/edit/verification, unchanged verifier, independent final code checks and Unicode recall |
| Dynamic controls and sessions | pi Haiku/DeepSeek/Luna/Grok thinking selections; Luna/Grok typed summary controls. Haiku/DeepSeek restart/fork/edit **8/8 each**; Haiku interleaved sessions **6/6**, multiple reads **3/3**, model switch **4/4** | Emitted/prepared controls, original/fork identities, independent labels and call/result links; not reasoning quality or summary granularity |
| Summaries | pi ordinary **6/6**, OpenCode **5/5**, five consecutive repeated-compaction stories **9/9 each**, automatic **4/4** | Ordinary Generation summaries, exact recall and rebuilt read; no server-side compaction API |
| Cancellation/recovery | First-text cancellation **3/3 per Agent**; pi thinking/local bash, tool arguments and compaction single-permit stories | Honest interruption, released resources, no partial execution/file, subsequent recovery; no remote billing/compute claim |
| Inputs/accounting | Haiku image **3/3**, 420-line tool result **3/3**, usage/cache **2/2**, native output limit **1/1** | Actual PNG read/recall, Unicode facts, native/client counters and observed length terminal |
| Go native DeepSeek companion | Authenticated models GET and OpenCode native loop **3/3** | Exact Go identity/root/session headers; bypasses gateway codecs and activates no binding |

Earlier OpenCode coding failures used `bash` permissions rather than its actual `shell`; corrected permission evidence remains distinct. Repeated-compaction failures are not erased by passing repeats and do not imply deterministic model recall. Evidence retains original dates/profiles in the ignored reports; private captures and disposable live runners are not public fixtures.

#### Content fidelity evidence

| Profile / date | Verified attribution | Verdict retained |
|---|---|---|
| pi Messages literal-label recall (R13), 2026-11-08–09 | Earlier collection: seven stories / 35 attempts, two passes, four recall failures, one final-request HTTP 400; three failures have complete native/delivered/client equality, another has incomplete native capture. The unchanged **5/5** recheck verifies cumulative roles/prefixes, instructions, `[read],[read],[ls],[ls],[]`, ordered effort and call/results; only selected `thinking.block_binding` omission differs in preparation | Native final text already lacks `cedar-71`, retains the filename and equals delivered/client text. Project-owned request/delivery divergence is not observed; the exact-label assertion remains failed, not waived |
| pi/Grok write-feedback (R21), 2026-11-08 | Two **3-request/3-attempt** stories, interrupted → complete → complete: no file before recovery, distinct cancelled/new IDs, one exact new write/feedback, no cancelled identity in history. The second complete final native comparison equals delivery/client; prepared user text/feedback match ingress | Both fail exact-marker and prompt-only no-tool expectations. Partial tool captures do not qualify complete snapshots; four native declaration-on/off × omitted/none controls return HTTP 200 without repairing marker omission. This is scoped content behavior, not cancellation causality |

Keep immutable request text/order and explicit tool policy; prose does not synthesize a structural no-tool guarantee. Scoped [Grok service policy](architecture.md#subscription-responses) and the roadmap dispositions remain authoritative. Evidence: `ar5-convergence-r13-*-20261109.job`, `ar5-r21-{write-baseline,live-slot1,renewal,live-slot2,live-slot3}-20261108.log`, `ar5-grok-shape-native-20261108.log`.

#### Replay isolation evidence

**2026-11-08:** real stateless Responses SDK JSON and SSE each pass **4/4**: Grok protected call → same-target feedback → Luna visible-history transfer → Grok again. Original native values restore on the same target; foreign reasoning takes the selected omission while visible text and call/result links survive. Evidence: `ar5-r09-replay-{json,sse,offline}-20261108.log`.

This qualifies replay isolation for those profiles, not independent-account failover. Earlier loader evidence reports one enabled declared account per Provider; no independent compatible same-model pair is qualified and no fresh inventory/provisioning is implied. R09 retains its separate [selected pre-Stage-4 gate](roadmap.md#cache-affinity-qualification).

### Installed-client default characterization

Characterize the current producer/consumer before final gateway acceptance. Reuse `tests/clients/drivers.py` bounded RPC/child handling and `recovery.py`'s owned loopback authority, not personal settings or a new harness. For Responses, register a test-owned model with `api:openai-responses` and no body `compat` object; do not reuse the curated Chat/Messages configuration. Omit the thinking override and verify `get_state` plus the first request. Retries, warming and automatic compaction may be disabled as declared observation bounds, not semantic compatibility changes.

Drive a successful tool reply, actual scratch tool handling, a final answer and another user turn. Inspect actual outgoing history for reasoning IDs/summary/opaque state, phase, call/result association and optional status. A two-call reply distinguishes grouped results from single-call adjacency. A length-stopped call with both unfinished and valid arguments distinguishes no-execution protection from mere parse failure; error-result events do not prove execution. Keep observed client transformations separate from gateway source-byte preservation and protected replay.

**Characterized on 2026-10-29:** current pi 1.4.0, medium default, five isolated synthetic cases, three requests each; model-declared 2048-token ceiling, 30-second prompt bounds, 256 KiB requests and sub-32 KiB scripted responses. Summary and opaque carriers returned, completed-message phase survived, and two actual reads produced grouped result history. Length-stopped calls produced no-execution error results even with valid arguments. [Dated evidence](refs/2026-10-27-client-ir-boundaries.md#pre-stage-4-contract-qualification-2026-10-29) records installed-source hashes and scope. This direct-authority observation bypasses gateway admission/sealing and every Provider; it adds no passing gateway default-profile claim to the 12 curated regressions.

Final acceptance must use the same current client through the production gateway and the selected success domain. Keep retained rejections and explicitly nondefault adaptive/uncapped profiles separately labelled. Discovery publication and client consumption are separate checks. Record current versions and recompare relevant initial/replay behavior rather than installing versions to match source pins.

For built-in models, override the test origin/key and permitted headers without replacing the model record. The 2026-10-29 direct-authority Go/Haiku check preserved built-in adaptive metadata, used the observed medium default and completed one actual read plus signed replay across three requests. This was a direct-authority observation, separate from the curated `forceAdaptiveThinking` override. Subsequent production-gateway default loops are recorded in [installed-client acceptance](#installed-client-synthetic-recovery); a bare-model manual rejection cannot replace them.

### Installed-client synthetic recovery

**Current-client convergence, 2026-11-09:** the full offline baseline, production compilation, all-target Clippy under `-D warnings`, formatting, 20 pinned assets and Python suites (8 upstream / 16 SDK-oracle / 45 client-oracle / 17 OpenAPI) pass. All **20 SDK loopbacks** and **25 installed-client scenarios** pass on safe unwrapped pi **1.5.0** and OpenCode **2.2.5**. Default tool checks require the real read contract and preserve the complete observed declaration/schema inventory through preparation, without snapshotting auxiliary names such as OpenCode's new `skill`. Responses accepts official empty annotations as well as the selected Client shorthand. Redundant oracle self-tests and obsolete Google `call`/`tail` surrogate-signature compatibility are removed; actual driver bounds and replay/terminal checks remain. Evidence: `ar5-convergence-*-20261109.job`, `current-agent-{inventory-clients,pruning-offline}-20261109.job` and `google-no-call-references-*-20261109.job`. Historical version labels below retain their original dates, not compatibility targets.

`protocol_defaults.py` characterizes native Chat, Responses and Messages defaults using the existing isolated drivers, transparent forwarder and synthetic upstream authority. It retains built-in models and their tool/strictness/reasoning controls rather than reusing curated compatibility profiles. Origin/key and the 2048-token output cap are test-owned; warming and compaction are disabled. Pi retries are disabled; exact transmission counts detect any OpenCode retries. OpenCode Responses explicitly selects HTTP because its built-in OpenAI transport defaults to WebSocket; no body/history rewrite is applied. Provider and model endpoint overrides are both explicit because built-in model settings take precedence.

| Client / protocol | Built-in client model → synthetic gateway binding | Client requests / upstream attempts | Current characterized result |
|---|---|---|---|
| pi / Chat | `deepseek/deepseek-flash` → `deepseek-beta-chat` | 3 / 3 | Reverified with pi 1.5.0 on 2026-11-06: strict read, tool result, final answer and same-session recall complete through an explicit `/beta` account root |
| OpenCode / Chat | `deepseek/deepseek-flash` → `deepseek-chat` | 3 / 3 | Read/tool-result/final answer and same-session recall complete |
| pi / Responses | `opencode/gpt-6.1-sol` → `openai-sol-responses` | 3 / 3 | Built-in developer `additional_tools` remains at the transcript head; direct call/result, final answer and same-session recall complete |
| OpenCode / Responses | `openai/gpt-6.1-sol` → `openai-sol-responses` | 3 / 3 | Read/tool-result/final answer and same-session recall complete; prepared upstream text history requires the official annotations array, including when current Client replay uses the selected empty-collection shorthand |
| pi / Messages | `opencode-go/claude-haiku-5-5` → `opencode-go-haiku-messages` | 3 / 3 | Root-low effort and message-local medium markers retain their positions; default read/result/answer/recall and original signed/redacted history complete |
| OpenCode / Messages | `opencode-go/claude-haiku-5-5` → `opencode-go-haiku-messages` | 3 / 3 | Default adaptive thinking, native signed/redacted history and same-session recall complete |

The 2026-11-06 recheck uses pi 1.5.0 and OpenCode 2.0.32. All six native default tool/history loops complete, alongside recovery, summaries and lifecycle checks. OpenCode Responses raw ingress and prepared upstream history have independent oracles: omission at ingress must become `annotations:[]` upstream through the selected [Client-history L2 rule](architecture.md#semantic-equivalence-compatibility), never a forwarder/fixture rewrite. Successful loops require exact final text, reused sessions, three honest completions, three upstream attempts with no extras, and successful database/RPC terminal state. Wire history checks preserve reasoning, ordered calls/results, distinct Responses item/call IDs and `final_answer` phase; no opaque Responses replay is asserted by the summary-only fixture. Nonempty annotations retain their independent codec preservation check; the installed empty-collection fixture does not qualify citation recovery. Recharacterize when the installed Agent changes; do not keep an old-version branch.

The native Messages pair preserves the built-in Go/Haiku model and default thinking, overriding only origin/key and the output bound. No header, compatibility or reasoning-level override is supplied. Pi's RPC state checks the resolved model and medium default; its serializer now expresses that level through ordered message-local markers rather than root effort. The gate verifies original signed/redacted thinking, call/result identity and final-answer replay; authority teardown precedes the final verdict. These default gates remain separate from the curated recovery/summary/lifecycle profiles below.

The default wire oracle checks ingress and prepared upstream requests independently: exactly one `read` declaration with a required string `path`, plus client-specific strictness, eager-input, display and effort presence. Current pi Messages defaults omit strict, enable eager input, select summarized display and pair root-low effort with message-local medium markers and `block_binding:drop_block`; current OpenCode defaults omit strict, eager-input, effort and display. Missing declarations/schema/effort and invented controls must fail before the scripted authority returns a tool call. The curated low-effort/strict profile is not reused for these defaults.

**Verified on 2026-11-02:** pi 1.4.0 and OpenCode 2.0.32 each complete all three requests through the production CLI loader and synthetic native authority, execute the scratch read successfully and retain signed history for same-session recall, with zero extra transmissions. Both send `2023-06-01`; OpenCode also sends a nonempty beta header and `block_binding:drop_block`, while this resolved pi model sends neither. No client header or compatibility opt-out is used. This accepts the selected native Messages default lane, not live Provider behavior or the remaining Chat/Responses/cross-protocol exit.

Synthetic recovery profiles configure single-candidate routes with explicit client parameters:

| Profile | Fixed client declaration | Cache, session and auxiliary calls |
|---|---|---|
| pi Chat | `openai-completions`, `qwen3.8-flash`, low effort, 2048-token cap; developer role, strict mode and store disabled; reasoning effort and `max_completion_tokens` enabled | Short retention and OpenAI affinity headers; agent retries off, provider retries zero; warming and automatic compaction off |
| pi Messages | `anthropic-messages`, `claude-haiku-5-5`, low effort, 2048-token cap; adaptive thinking, strict/eager read tools and mid-conversation system messages enabled; mid-conversation effort/tool changes and temperature disabled | Short retention and native affinity; stable root-low effort without message-local effort; same retry/warming/compaction restrictions as pi Chat |
| OpenCode Chat | `@opencode/ai/providers/openai-compatible`, `qwen3.8-flash#low`, explicit 2048-token cap; only read permission | Explicit titles suppress title requests; warming and automatic compaction off; scratch persistent session resumed for continuation. Zero extra transmissions is checked, not an assertion that every client retry path is disabled |

Recovery allows exactly four client requests/five synthetic attempts per profile. It classifies initial tool declaration, tool feedback, an independent no-tool-invocation recovery conversation, and original-session continuation. It rejects duplicates, strict downgrades, summaries and other auxiliary transmissions rather than silently widening that budget. The 2026-10-21 rerun passed all three cases with local pi `1.3.0` and OpenCode `2.0.32`; this is synthetic recovery evidence only.

The final recovery verdict runs after authority sockets/workers drain, so a late auxiliary request or worker error cannot disappear behind an earlier successful snapshot. Its 2026-10-22 rerun passed all three cases with pi `1.4.0` and OpenCode `2.0.32`, at the same four/five bounds. This does not rerun the earlier versions' separate summary acceptance.

Later capability gates must fix their own schema/choice/thinking combinations; these read loops establish neither structured output nor required/named choice. The separately documented Responses profile remains distinct, including explicit non-strict tools. Long-session acceptance selects ordinary Generation summarization on the same binding/model, not native compaction or an implicit summary model. The [summary and strict-fallback gates](#installed-client-synthetic-summaries) are separate from recovery; their synthetic characterization does not accept retries, downgrade or auxiliary requests in a live capability gate.

Required sustained-operation diagnostics are narrowly scoped: observable dropped/failed logging, bounded restart-safe logging, body-free request/attempt correlation and consumer-side first-content timing against delayed terminal/EOF. The separate [sustained-operation checks](#sustained-operation-acceptance) exercise their composition; upstream first-byte or decoded-delta timing is not client visibility. Follow [Observability](architecture.md#observability), not another diagnostics framework.

This is a separate gate from SDK loopback and real Provider acceptance. `tests/clients.rs` creates a private synthetic credential store through `CredentialManager`; `tests/clients/recovery.py` starts the real gateway CLI with explicit scratch files and a loopback-only candidate. `drivers.py` runs pi RPC or OpenCode standalone with empty allowlisted environments, scratch HOME/XDG paths and no project resources. The command below runs recovery, summaries, strict-fallback characterization and lifecycle checks serially. Supply **unwrapped** binaries explicitly: a PATH wrapper can source host secrets even under `env -i`.

```sh
MORPHIECORE_PI_BIN=/absolute/path/to/unwrapped/pi \
MORPHIECORE_OPENCODE_BIN=/absolute/path/to/unwrapped/opencode \
  cargo test --locked --offline --test clients -- \
  --ignored --test-threads=1 --nocapture
```

Python 3 from the development environment needs only its standard library; `MORPHIECORE_CLIENT_PYTHON` optionally selects it. Always use the host's current installed pi, OpenCode and other Agents, not binaries installed to match protocol source pins. The harness validates and reports actual version strings without requiring source-lock equality. Use safe unwrapped launchers with isolated test state; do not downgrade, replace or reconfigure host installations. Record full actual versions and revalidate changed client behavior; a version string alone is not acceptance.

Each of the three serial recovery cases (pi Chat, pi native Messages, OpenCode Chat) fixes four client Generation requests and five upstream sends: initial `read` on the primary, unpublished `503` on tool feedback, an identical same-target retry after `Retry-After:1`, an independent primary conversation, then original-session continuation on the primary. A configured secondary must receive no sends. Recovery has a 2.5-second budget; route/exchange deadlines are 8/10 seconds, with 2048 output tokens and a 120-second driver bound. The transparent admission forwarder counts client POSTs independently, rejects duplicates/excess requests and never rewrites payloads or session headers. The authority compares retry bytes independently of diagnostics and brackets header publication with monotonic timestamps. It rejects retries earlier than the earliest possible deadline and reports the possible elapsed interval as `retry_hint_wait_seconds`; an interval crossing the deadline remains uncertain, not proof of early retry or exact waiting. The [owner-local recovery check](#offline-baseline) independently requires a full hint delay from the gateway's retained failure-observation timestamp, excluding request startup time. pi retries are disabled; OpenCode's zero extra transmissions are observed, not inferred from an unsupported retry setting.

`recovery_boundary.py` adds three current-pi Chat cases, each with three gateway requests. A 60-second hint prevents a second send; only the next client request selects the backup, which remains sticky (three sends). Two short-hint failures stop at two identical primary sends, then a new client request selects the backup (four sends). Cancellation runs directly against the gateway and synchronizes on its failed-attempt telemetry receipt before the two-second hint expires. After pi settles, it waits for the correlated gateway terminal receipt before the next prompt, proving release without a delayed retry or false success; already observed failure health lets the next request select the backup (three sends). Request diagnostics independently check ingress and attempt counts for this direct cancellation path. Existing owner-local controlled-clock tests retain deadline and concurrent health authority.

`pi_responses_tool_argument_cancellation` extends `lifecycle.py` with exactly three requests through the built-in Responses profile. Cancel only after an unfinished argument delta, then require an aborted client terminal, no tool execution, upstream socket release and a non-complete gateway receipt. Recovery deliberately returns a distinct complete read call; its execution and legitimate third-request feedback must retain the new identity and finish normally. The authority holds the first stream without a terminal or EOF; concurrency is one and an eligible secondary cannot hide replay. This gate distinguishes new execution from a cancelled call without inferring tool withdrawal from prose. It does not qualify a real model's recovery instructions or remote compute termination.

Independent wire expectations check ordered history, call/result IDs, readable reasoning and native signatures/redacted blocks. Client events and scratch OpenCode database state establish actual tool completion, final text and idle success; observed session carriers must match the clients' reported session IDs. Gateway diagnostics require honest completion, no cooldown/probe after successful same-target recovery and zero unclassified ignored request fields. Native session carriers remain stable through recovery and differ for the independent conversation. Summary retry gates explicitly disable gateway recovery and transient cooldown so they measure the Agent's new HTTP request, not an absorbed gateway retry.

Owned HTTP sockets/workers and child process groups are bounded and released, including incomplete headers, timeout and cancellation. Focused authority-oracle checks live in `tests/clients/test_client_recovery.py`; descendant teardown checks live in `tests/process/child_process.rs`. These checks call no real Providers and do not prove account-compatible replay, Provider cache behavior, R1b Responses or every failure/concurrency scenario.

The ordinary offline gate `cargo test --locked --offline --test gateway fresh_account_setup` exercises [fresh setup](usage.md#prepare-credentials) with synthetic private files: store-free explanation, honest unprovisioned validation, actual key provisioning/status, local validation without sink activation, the shared live-runner constructor and one explicitly selected diagnostic send. `cargo test --locked --offline --lib configured_task_sends_once` checks fixed-account Embedding dispatch with Generation recovery enabled and two available accounts; its failure sends once to the selected account. Neither gate requires installed Agents or real credentials.

### Installed-client synthetic summaries

`tests/clients/summary.py` reuses the isolated drivers, bounded HTTP authority and CLI loader for pi Chat and native Messages. Each profile fixes **six client requests/six upstream attempts**: an initial read/tool-result loop, a no-tool recent user span, one manual ordinary Generation summary, then a new read/tool-result loop. The same bindings, models, compat flags, 2048-token cap and 3/8/10-second candidate/route/exchange deadlines apply. Automatic compaction, warming and client retries remain off; Provider `recovery.recovery_timeout_ms:0` bounds each gateway request to one send.

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

`tests/clients/lifecycle.py` uses the preceding three declared profiles, the real CLI loader and direct client→gateway delivery, not the recovery gate's buffering admission forwarder. Each case fixes two Generation requests/two synthetic upstream attempts: held content → cancel, then same-session recovery. Gateway concurrency is one; candidate/route/exchange deadlines are 20/25/30 seconds, while client and observation deadlines remain bounded. Two compatible synthetic named accounts stay eligible so no extra attempt can hide behind a one-attempt configuration.

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
3. Use explicit model/candidate entries in `models.json` so unrelated accounts are not selected.
4. Run serially with no hidden retries. For a single-send Generation check, use only the selected configured account and set its Provider `recovery.recovery_timeout_ms` to zero. Model attempt ceilings are not supported.
5. Report only redacted status, counts and error codes. Never print credentials or request/response bodies.

The gateway CLI loads `gateway.json`, its selected Agent-key `auth.json`, `accounts.json`, `models.json` and managed credential references through the project loader. A test harness must do the same; do not hand-parse credential files.

Local byte, event and deadline limits do not prove bounded upstream charges when a target has no output-token control. Real checks supplement synthetic regression; they do not replace it.

For tool-result image checks, first obtain a real tool call from the selected model, then return a synthetic valid image with the complete reported history. Preserve thinking and opaque replay carriers unchanged. A hand-written call without model-issued continuation values is not a valid replay oracle for thinking models.

Distinguish HTTP success, honest completion and visual recognition. Check a known feature of the synthetic image without logging response text. A successful public-URL sample does not prove URL availability or justify automatic retries after an earlier failure.

#### Native Messages input acceptance

`tests/sdk/messages_inputs_live.py` is an explicit live companion, not an offline test. It starts the CLI with scratch service/model configuration and explicit absolute `--accounts`/`--store` paths, resolved only through the project loader. Prepare the account file with only the selected Go account and `recovery.recovery_timeout_ms:0` for one send per request. Each case fixes a 2048-token output ceiling, 120-second route/exchange deadlines, 256 KiB client requests and 8 MiB responses. No SDK/client retry, model fallback, URL fetch, Files ID or real tool execution is involved.

Run cases separately and serially after building the selected CLI:

```sh
python3 tests/sdk/messages_inputs_live.py --gateway-bin /absolute/path/to/morphiecore \
  --accounts "$ROOT/accounts.json" --store "$ROOT/credentials" examples
python3 tests/sdk/messages_inputs_live.py --gateway-bin /absolute/path/to/morphiecore \
  --accounts "$ROOT/accounts.json" --store "$ROOT/credentials" context
python3 tests/sdk/messages_inputs_live.py --gateway-bin /absolute/path/to/morphiecore \
  --accounts "$ROOT/accounts.json" --store "$ROOT/credentials" cache
```

The examples case fixes three requests: root examples → model-issued result with a by-value example definition → second real-call result/final answer. The context case fixes three: distinct contexts for two uses of one plain-text source, a real fetch call, then a contextual document in its result. The cache case fixes four: unique synthetic prefixes of 2,000 words, immediately repeated under explicit 5m and 1h automatic policies. Require positive reported cache writes/reads and the requested TTL write class; absent counters are not zero. Prepared 5m may use the documented omitted-default representation; 1h must remain explicit.

Every case requires honest completion, exact request/attempt counts and zero ignored request fields. Opt-in private upstream-request capture checks examples/schema/order, per-use contexts or automatic TTL without using capture as replay input; temporary captures/configuration are deleted before exit. Only fixed status/error codes and numeric reports are printed. Offline runner oracles live in `test_messages_inputs_live.py`; separate directional/native SDK gates remain required.

The 2026-10-22 Go/Haiku cases passed 3/3/4 requests respectively. The accepted cache run reported 14,056 written/read tokens for 5m and 14,055 for 1h, with matching write classes. An earlier four-request cache run completed but its final harness assertion incorrectly required an explicit 5m field; the corrected offline oracle and separate bounded rerun resolved that harness failure. An earlier runner startup type error made no HTTP request. These are bounded binding checks, not complete protocol/model conformance, measured retention, invoice accuracy, signed-thinking reuse or deployment authorization.

#### Constrained Messages profile acceptance

Run `messages_capabilities_live.py` explicitly with the [locked SDK environment](#python-checks):

```sh
uv run --project tests/sdk --locked --offline python tests/sdk/messages_capabilities_live.py \
  --gateway-bin /absolute/path/to/morphiecore \
  --accounts "$ROOT/accounts.json" --store "$ROOT/credentials"
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
  --gateway-bin /absolute/path/to/morphiecore \
  --accounts "$ROOT/accounts.json" --store "$ROOT/credentials"
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

For the selected Chat profile, follow [Chat reasoning extensions](architecture.md#chat-reasoning-extensions). The existing `readable_reasoning` opt-in now includes typed protected replay. `sdk_chat_signed_reasoning_tool_history` exercises JSON/SSE and actual SDK-returned history; the two installed-client `chat_signed_reasoning_tool_history` gates check native thinking/redacted payloads and unchanged delivered/returned envelope bytes. Keep the unsigned SDK gate's no-details assertion and the foreign-origin, malformed-envelope and partial-replay oracles. Shared client operations live in `tests/clients/drivers.py`; scenario modules do not import another scenario's runner. Do not infer Standard Chat support from these extension gates.

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

OpenCode Responses continuation uses the selected [input-only annotations shorthand](architecture.md#client-responses-history-shorthand) when the current serializer omits an empty array. Accept the official array at ingress too, and independently require prepared upstream `annotations:[]`; do not turn an old serializer's omission into a product requirement. This L2 equivalence creates no loss/insertion receipt or ignored field.
