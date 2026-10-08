# MorphieCore Agent Instructions

These rules apply repository-wide. Authorization is defined here and by explicit user instructions; platform approvals remain binding. Operational guides own execution details and may strengthen safeguards, not grant additional permissions.

## Scope and Authorization

- Work against current goals, contracts and source. The current architecture is the sole design authority; mainline status does not prove production readiness.
- Before editing, inspect branch, Git status and target diff. Preserve existing work; stop on overlapping external edits. Do not discard, stage, commit or push without scoped authorization; committing does not authorize pushing.
- Treat review and planning as read-only work. Implement requested changes through disposable verification; stop for material unresolved choices, not arbitrary phase gates.
- Deployment/service changes, publication and credential lifecycle operations (including login, refresh, revocation and migration) require explicit target/effect authorization. Live calls outside the standing grant below require separate authorization. Plans, credentials and prior successful calls do not grant permission.

### Standing authorization for live Provider verification

- Real Provider calls and necessary credential-bearing discovery for the current development task are pre-authorized through the project's controlled probe entry points. No per-run/per-call confirmation or monetary spending cap applies.
- Prioritize requirement coverage, effective tests and sufficient diagnostic evidence over minimizing cost. Avoid unrelated exhaustive matrices and repeated calls without diagnostic value.
- Define each run's task-related targets, finite request matrix, request limits, task-appropriate output/resource limits, deadlines, cancellation/cleanup and sanitized output limits. These are execution safeguards, not spending approval thresholds. Follow [probe operations](docs/probes.md) and retain existing guards; authorization does not change probe capabilities.

## Read the Relevant Context

Read [README](README.md) and [docs index](docs/README.md) before non-trivial work, then follow the owning sources rather than reading every guide:

| Concern | Owner |
|---|---|
| Purpose, semantic concepts, standard APIs and Agent reuse | [Semantic Model](docs/architecture/semantic-ir.md) and relevant [ADRs](docs/architecture/README.md#架构决策) |
| Priorities / approved behavior slice | [next-goal](docs/implementation-plans/next-goal.md) / [current-focus](docs/implementation-plans/current-focus.md) |
| Mapping, loss and fidelity | [Protocol/lowering](docs/architecture/protocol-and-lowering.md), affected profiles, codecs and callers |
| Ownership, I/O and failure boundaries | [Architecture](docs/architecture.md), [execution](docs/architecture/execution-model.md) |
| Gaps and verification | [Implementation gaps](docs/implementation-status/generation.md), [development](docs/development.md), independent tests |
| Protocol evidence | [Fixed sources](docs/references/upstream-sync.md), [references](docs/references/README.md) |

Design contracts, source implementation and executed observations are different evidence. Resolve disagreements explicitly; do not rewrite a contract to excuse a bug. Realtime and other deferred designs are not prerequisites for the current request-based scope.

## Current Provider, Model and Compatibility Information

Query current facts; do not maintain Provider/model inventories, support matrices, account status or test results in docs, comments or memory.

1. **Registration:** [provider](src/provider/catalog.rs) and [topology](src/topology/catalog.rs) catalogs; follow Public Model → Route → Endpoint, upstream ID, representation and execution contracts. A Provider or Dialect declaration is not a usable binding.
2. **Activation:** [bootstrap](src/gateway/bootstrap.rs) and [configuration](src/gateway/config.rs); check explicit entries, protocol and credential activation. Chat admission does not imply Responses admission.
3. **Wire:** [adapters](src/adapter/mod.rs), [named rules](src/protocol/adaptation.rs), owning codecs/lowering and independent expectations. Test names/counts do not establish capability.
4. **Probes:** [catalog](examples/probe_support/catalog.py), selected entry points and [operations](docs/probes.md). Probe choices/defaults are not the product catalog.
5. **Instance:** verify binary revision and operator-approved non-secret activation metadata. Read [router](src/gateway/http.rs) before assuming discovery endpoints. Unavailable metadata means unknown activation. [Official sources](docs/references/providers/README.md) do not prove local admission or inference.

Report revision, scope and unverified layers in the conversation.

## Implementation Discipline

### Semantic boundaries

- Maintain one independent Semantic Model for Gateway and future Agent use. IR is its typed representation, not a Provider DTO, protocol field union, common denominator or universal request. Responses is the primary reference; Generation and Embedding remain distinct tasks within the same semantic authority.
- Follow [next-goal](docs/implementation-plans/next-goal.md) for priorities and deferred scope; do not infer a new behavior slice from the broader design. Standard API targets belong to the [Semantic Model](docs/architecture/semantic-ir.md#3-客户端-api-目标与扩展边界). Chat Completions is only a compatibility projection under the [loss contract](docs/architecture/protocol-and-lowering.md#semantic-loss), not permission for arbitrary field dropping, missing IR owners, fabricated facts or unsafe replay. Existing strict behavior remains binding until its implementation slice changes it.
- Distinguish an IR gap, a target-wire gap and missing wiring. Report an IR gap with structural options before choosing an implementation; do not hide it in adapters, fidelity or custom API fields. Do not accept, emit or rebuild an independent `_openbridge` attachment; any future replacement is decided after migration. New client extensions require explicit approval. Do not prebuild deferred protocols or generic frameworks.
- Stabilize concepts, owners and invariants before Rust shapes. Keep task, modality, resource and wire distinct. Pure codecs/lowering cannot access registry, credentials or network; runtime targets/retry/commit state never enter semantic data.
- Final typed values govern encoding. Preserve field-specific presence, order, precision, identity and resource meaning. Edits invalidate dependent replay/annotations; deleted values cannot return through fidelity. Revalidate transformed/projected values and derive requirements anew; each fixed candidate starts from the same immutable input, without capability unions or route reordering.
- Keep response closure, artifact completeness, logical-turn progress and continuation requirements distinct. Representation does not authorize tools, orchestration or automatic continuation. Static/Event and failure behavior must agree.

### Change workflow

- Before behavior changes, record observable result, requirement, failing case, non-goals and verification boundary in current-focus, then use TDD. Clear completed focus without erasing unfinished direction. Documentation-only work does not create a behavior slice.
- Split by responsibility, preserve intended public paths through explicit re-exports, and avoid unrelated cleanup. Large breaking rewrites are allowed within the approved migration scope, without legacy shims or empty schema bumps; update serialization, OpenAPI, examples, fixtures and contracts together. Dependency changes update manifests and locks with locked validation.

## Security and Resources

- **Credential access:** use the user-specified STORE; otherwise use the program's current default from the [CLI path resolver](src/credential/directory.rs), as described in the [credential guide](docs/credentials.md#自有文件目录). Access live credentials only through the project's existing loader. Do not inspect private files or discover third-party auth caches; `.env`, private `config/` and OAuth files are not investigation or offline-test inputs.
- **Data handling:** send only synthetic or explicitly authorized request data. Never expose real secrets, private configuration, sensitive bodies or auth caches in files, tool output, logs or fixtures. Do not display opaque values or dump environment values.
- Preserve loopback ingress, trusted egress and fail-closed auth. Business data cannot select upstream origins, credentials, auth/proxy headers or scripts. Credential locators and trusted origins do not go downstream.
- Bound allocation, capture, framing, buffering, events and time. Preserve cancellation/cleanup, real terminals and retry/fallback/publication/commit boundaries; no post-commit replay or fabricated success.
- Content capture requires explicit configuration/authorization, begins after authentication, observes the final downstream boundary and redacts sensitive headers. Bounded snapshots belong only in the dedicated local JSONL sink, not stdout or reviewed OTLP traces. Sink failure cannot change responses; a development logging profile is not sensitive-traffic authorization.
- Do not inspect or manually edit `target/`, `tools/corpus/.venv/`, `.pytest_cache/` or `__pycache__/` without a targeted need. Normal tools may populate caches. Do not commit derived `testdata/{generated,reports,dist,runtime}/`; canonical fixtures are not cleanup targets.

## Verification

| Mode | Boundary |
|---|---|
| Default/offline checks | Synthetic inputs; no real credentials, real Provider I/O, implicit services or added retry/fallback. |
| Bootstrap tests | Explicit isolated disposable loopback binary with synthetic keys and rejecting loopback egress. |
| Live Provider probes | Explicit runs under the [standing authorization](#standing-authorization-for-live-provider-verification); supplement, not replace, independent offline regressions. |
| SDK loopback and other ignored-network/external-dependency gates | Separate explicit gates requiring scoped approval, outside the default baseline. |

- Test distinct semantic, wire and security/resource boundaries at their lowest owner. Pure codec tests do not depend on product catalogs; credential-policy tests do not construct a Gateway. Keep binding/admission integration separate; use at most one production-Router smoke with independent value.
- Deduplicate Provider tests by risk and contract boundary, not Provider/model counts. Reuse shared codec/lowering coverage and representative bindings; prefer small independent fixtures over incidental response snapshots. Preserve Provider-specific mappings, rejection, stream termination and security/resource coverage. Remove redundant tests only after identifying independent coverage of their failure classes.
- Treat upstream contract churn as a reason to verify contracts, not discard regression coverage. Distinguish contract changes, implementation defects and wiring gaps; fix the responsible owner and add a minimal regression.
- Use independent wire→IR and IR→wire expectations, edits, applicable streams and failures; round trips alone can hide symmetric loss. Chat loss tests must assert both permitted loss and protected invariants. Reuse small synthetic fixtures; review external asset version, license, sensitivity and oracle independence.
- Run focused checks, then the required baseline for the changed files in the [development guide](docs/development.md): [Rust](docs/development.md#rust-检查), [JS tooling](docs/development.md#测试语言与-js-工具) or [documentation](docs/development.md#文档与边界). Prose-only changes do not require runtime tests.
- Parallelize only independent, isolated scenarios. Use readiness/events and bounded deadlines; ordered retry/fallback/cancellation tests remain serial, without sleeps hiding races.
- Inspect final diff and report actual results, failures and skipped layers. Compilation, synthetic execution or test existence do not prove general SDK/Agent, real Provider/TLS/network, cache benefit, load, native filesystem/ACL or production behavior.
- Close transient live-service issues under the [retest acceptance rule](docs/probes.md#复测与问题收口); do not add statistical reruns or relax runtime guards to obtain a clean report.

## Documentation and Comments

- Follow the [documentation ownership and maintenance rules](docs/README.md#写作与维护); keep one authoritative owner and link to it instead of copying contracts, field tables or commands.
- Rust comments/docs and Python docstrings use concise English; explain non-obvious rationale with essential source URLs.
- Preserve effective constraints, source attribution and incoming links when consolidating. Do not turn cleanup into contract changes or refresh external verification dates without new evidence.
- Keep history, audits and execution results out of docs, comments and memory. Durable memory points to canonical owners and retains only confirmed preferences, relationships and decision context. Documentation changes alone do not prove behavioral improvement.
