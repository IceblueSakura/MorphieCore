# Development

This page summarizes overall acceptance status and shared development conventions. [Architecture](architecture/index.md) owns accepted contracts, [Roadmap](roadmap/index.md) owns remaining work and gates, and [Usage](usage/index.md) owns setup and operation.

Acceptance notes are temporary development material. Keep the overall result and remaining boundaries here, not historical run classifications, execution dates, tested-version inventories, command recipes or per-run reports. Reusable checks belong in code and tests; protocol research and source pins belong in [References](references.md).

## Contents

- [Overall acceptance status](#overall-acceptance-status)
- [Environment](#environment)
- [Test layout](#test-layout)
- [Upstream test sources](#upstream-test-sources)
  - [Source links and updates](#source-links-and-updates)
- [Verification gates](#verification-gates)
  - [Offline baseline](#offline-baseline)
  - [SDK loopback gate](#sdk-loopback-gate)
  - [Installed-client acceptance](#installed-client-acceptance)
  - [Real Provider checks](#real-provider-checks)
  - [Reverse-proxy delivery gate](#reverse-proxy-delivery-gate)
  - [Documentation checks](#documentation-checks)
- [Test selection and pruning](#test-selection-and-pruning)

## Overall acceptance status

The Generation baseline has met the selected [partial-delivery exit](roadmap/index.md#partial-delivery-exit). The standard-only contract and simplified coordination pass offline regression, connected SDK checks and isolated current pi/OpenCode workflows. Representative real Provider evidence remains applicable only to unchanged contracts; standard-only real workflows and OAuth primary-path qualification stay in the [issue register](roadmap/unscheduled-issues.md#unscheduled-issues).

| Area | Overall status |
|---|---|
| Generation protocols | Standard Chat, Responses and Messages request, JSON/SSE, tools and native reasoning/history paths are supported, including stateless Responses across a fresh runtime. Google remains upstream-only; ordinary signatures without a standard stateless carrier reject explicitly |
| Agent workflows | Principal pi/OpenCode tool, coding/repair, recall, summary, session and cancellation/recovery workflows are accepted. Required-history Standard Chat tools need runtime-selected thinking off; active/default thinking rejects with an actionable error. Client-specific and Provider-specific qualifications remain in the roadmap |
| Protocol integration | Independent directional checks, pinned upstream fixtures, strict SDK consumers and complete continuation histories are integrated |
| Generation binding repairs | Production-constructed SIWC flat-tool carriage, GPT strict/image and reasoning constraints, native opaque replay continuation and growing discovery directories are covered offline. Connected SDK consumers and current installed pi/OpenCode tool/history loops remain accepted; real endpoint prerequisites stay in the [issue register](roadmap/unscheduled-issues.md#content-and-client-qualification) |
| Context discovery | Service-qualified total/input capacities reach current Pi through authenticated gateway discovery, native model storage, selected-session accounting and offline RPC restoration. Unknown multi-candidate capacities retain client fallbacks; explicit per-model planning caps remain effective |
| Runtime and credentials | Ordered routing with advisory cooldowns/preferences, bounded retry/publication, credential isolation, OAuth renewal and pending-verification recovery are covered; API-key updates use one transaction and Unix binaries handle SIGINT/SIGTERM |
| Diagnostics | Application-side failure Logs, bounded private context, redaction and non-blocking lifecycle are delivered. Production receiver, persistence, access and retention remain unselected |
| Embeddings | Independent text Embedding execution and selected float/Base64 and accounting conversions are accepted |
| Stage 4 | Static image/audio paths provide a starting point; per-operation contracts, resource lifecycles and affected acceptance remain to be completed |
| Real account failover | Replay isolation is accepted for selected profiles; independent compatible-account failover remains unscheduled |
| Production and Stage 5 | Deployment and production backend acceptance remain separate. Stage-5 APIs are unscheduled |

Accepted losses and service limitations live in [Architecture](architecture/index.md); unresolved qualifications have one home in the [unscheduled issue register](roadmap/unscheduled-issues.md#unscheduled-issues). Do not duplicate those registers here or turn deferred cases into new baseline prerequisites.

## Environment

[flake.nix](../flake.nix) provides the development shells and tool availability checks. `flake.lock` pins Nix inputs; `rust-toolchain.toml` selects Rust and its matching LLVM coverage tools. Use declared tooling rather than host-wide upgrades.

Cargo dependencies are pinned in `Cargo.toml` and `Cargo.lock`. The shared Python environment is owned by `tests/sdk/pyproject.toml`, `uv.lock` and `.python-version`; TypeScript SDK probes use `tests/sdk/pi/package-lock.json`. Restore the relevant locked environments before using their consumers. Generated environments and build caches are not hand-maintained sources.

Keep application dependencies, protocol/SDK reference pins and installed Agent versions independent. Check stable release contracts before promoting dependency or source pins, respect upstream constraints, and recharacterize affected client behavior when the installed Agent changes.

## Test layout

| Entry | Responsibility |
|---|---|
| `src/**` unit tests | Owner-local product invariants, credentials, routing, delivery and diagnostics |
| `src/bin/upstream/tests.rs` | Diagnostic target/wire contracts and synthetic credential refresh |
| `tests/semantic.rs` → `semantic/` | IR, codecs, lowering, replay and history editing |
| `tests/transport.rs` → `transport/` | SSE framing, preparation and Attempt/delivery lifecycle |
| `tests/credential.rs` → `credential/` | Credential CLI and synthetic authority/store checks |
| `tests/gateway.rs` → `gateway/` | Router, synthetic Providers and isolated binary startup |
| `tests/process.rs` → `process/` | Bounded child I/O, timeouts, cancellation and descendant cleanup |
| `tests/sdk_loopback.rs` → `sdk/` | Explicit, serial SDK consumer checks against loopback authorities |
| `tests/clients.rs` → `clients/` | Explicit installed-Agent checks with isolated state |
| `tests/{sdk,clients,gateway}/test_*.py` | SDK/client oracles and OpenAPI schemas |
| `tests/upstream/` | Source/asset provenance, acquisition and extraction checks |

Top-level Rust files are Cargo integration targets; nested files are registered modules. Shared fixtures live in `tests/support/`, imported directly by their consumers. Keep scenario/API data boundaries in `testdata/`; generated output, reports and runtime state stay in ignored `testdata/{generated,reports,dist,runtime}/`.

The synthetic [repair-and-recall corpus](../testdata/repair-and-recall/) supplies coherent multi-turn histories across the selected native APIs. Extend it only for distinct product invariants, using independent expectations rather than encoder dumps.

## Upstream test sources

[sources.lock.json](../tests/upstream/sources.lock.json) identifies immutable upstream sources; [assets.lock.json](../tests/upstream/assets.lock.json) identifies the reviewed, vendored subset. Ordinary regression uses local assets without network access. Missing files, digest drift, ambiguous extraction and source/asset disagreement fail visibly.

Preserve imported bytes, licenses, source paths and declared framing adaptations. `tests/support/upstream_assets.rs` owns the thin fixture adapters; the [asset boundary inventory](refs/2026-10-23-stable-protocol-contracts.md#imported-asset-boundaries) records exclusions. Do not regenerate expected output from a MorphieCore encoder or execute downloaded source during acquisition.

Use raw fixtures through MorphieCore codecs and actual SDK consumers receiving gateway output and constructing continuation independently. Keep wire-to-IR and IR-to-wire expectations independent. Distinguish official protocol contracts, Provider dialects, SDK conveniences and gateway policy.

### Source links and updates

- Resolve stable releases to immutable revisions, with package-specific tags for monorepos. Label unreleased snapshots explicitly.
- Preserve repository, revision, package, path, license, digest and extraction provenance in the source/asset locks. Research notes retain their source dates.
- Review changed types, request builders, reducers, migration notes and fixtures before promoting a baseline. Candidate discovery does not update pins or activate APIs.
- Inspect external test assumptions before reuse. Adapt differences without weakening gateway bounds, strict known-field validation, replay isolation or terminal honesty. Keep fetched inputs free of credentials and private payloads.

## Verification gates

Use focused checks while iterating and the required baseline before publishing an implementation slice. SDK, installed-client, real Provider and deployment checks keep their own boundaries. Select affected gates from the roadmap rather than repeating a full client/model/protocol cross-product.

### Offline baseline

The baseline includes Rust tests, strict all-target linting, formatting, production library/binary compilation, pinned-asset integrity, upstream extraction tests and whitespace checks. Use locked dependencies and offline execution in the project environment. Discover the SDK-oracle, client-oracle and OpenAPI Python suites separately when affected.

All baseline inputs and authorities are synthetic. Credential/auth isolation, trusted origins, strict JSON, resource bounds, cancellation/release, replay isolation and honest terminal reporting remain required assertions.

### SDK loopback gate

Use the existing SDK integration target and locked Python/TypeScript consumers serially against test-owned loopback authorities. Check actual decoded results, incremental callbacks, complete returned history and independent next-upstream expectations. Keep API-key and subscription startup construction at the production loader boundary, with synthetic credentials and explicit target contracts.

Binary bodies, multipart inputs and asynchronous job states belong to their operation's tests. Generation round trips do not replace those lifecycle checks.

### Installed-client acceptance

Use the current installed pi/OpenCode with safe unwrapped launchers, isolated HOME/XDG/session state and bounded child processes. Retain built-in model declarations for default-profile checks; select nondefault profiles explicitly. Do not patch serialized requests or returned history, downgrade clients to source pins, inspect personal configuration or retain historical compatibility branches.

The existing client drivers own request budgets, timeout/cancellation, scratch state and independent authority oracles. Check actual tool execution, final state, continuation and request counts. Recharacterize changed serializers and controls at their affected boundary, using source and test-owned state.

The `pi_gateway_context_discovery` gate additionally takes the current MorphieCore extension entry in `MORPHIECORE_PI_EXTENSION`. It connects real gateway discovery to native Pi model storage, selected-session context accounting and a separate offline RPC process. It checks metadata loading, not large-context inference quality or upstream token enforcement.

### Real Provider checks

Follow [authorization](../AGENTS.md#authorization) and use the project loader exclusively. Before each serial run, fix the target/profile, total request budget including auxiliary calls, output and byte caps, timeouts and permitted recovery. Disable hidden retries and reject unexpected requests rather than expanding the budget.

Use synthetic task content and model-issued call/result identities with untouched returned history. Check the intended client workflow and honest termination. Report only redacted status, counts and error classifications; keep private captures temporary. Remaining real qualification belongs in the roadmap.

### Reverse-proxy delivery gate

The selected [proxy contract](architecture/runtime-and-routing.md#reverse-proxy-delivery) is covered by isolated foreground Nginx and installed-client lifecycle checks. Test-owned configuration and processes exercise content visibility before terminal/EOF, coalescing, cancellation, upstream release and subsequent recovery. Production TLS, networking and deployment remain separately authorized work.

### Documentation checks

Check local paths, heading anchors and whitespace; update incoming links when moving sections. Preserve accepted contracts and approval boundaries, and retain source dates in research notes. OpenAPI changes also need schema checks against actual routes, authentication and error envelopes. Documentation-only edits do not require Rust or SDK runs.

## Test selection and pruning

Keep a test when it can falsify a product assumption, public protocol contract or concrete external failure risk. A useful counterexample names what a user could lose: authority, data, identity, an explicit guarantee, bounded resources or an honest terminal result.

Delete implementation-derived expectations, trivial accessors/assembly checks, catalog/example snapshots, third-party self-tests and redundant permutations, together with exclusive helpers, fixtures and dependencies. Do not relocate low-value assertions or invent replacements to preserve test counts.

Preserve independent wire-to-IR / IR-to-wire expectations and connected consumer workflows at their responsible boundaries. Use existing property-based machinery for meaningful fragmentation, malformed-input and lifecycle counterexamples; retain minimized reproducers rather than adding a parallel framework.

Run affected gates and reuse valid results. Judge completion by delivered behavior and remaining boundaries, not coverage percentages, test counts or an expanding acceptance archive.
