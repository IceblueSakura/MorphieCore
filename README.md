# MorphieCore

MorphieCore is a local gateway that hides Provider differences from agent clients through one typed intermediate representation (IR). Clients use OpenAI Chat Completions, OpenAI Responses or Anthropic Messages; upstream protocols also include Google generateContent and Interactions.

The project is in core design and validation. pi and opencode are the primary clients. Codec support, public admission and accepted client loops are separate claims; see [current status](docs/roadmap.md#current-status). Other agents require separately scheduled acceptance.

The accepted [standards reset](docs/roadmap.md#standards-reset-and-redesign) first pins current stable dependencies and source releases, then integrates upstream protocol tests into the breaking semantic redesign. Historical mappings and client acceptance are not the new conformance baseline.

## Documentation map

This is the entry point, not a second specification. Each document has one primary responsibility:

| Area | Authoritative document | Read it for |
|---|---|---|
| Use the gateway | [Usage](docs/usage.md) | Credentials, configuration, startup, requests, history and errors |
| Accepted design | [Architecture](docs/architecture.md) | Shared principles, domain contracts, ownership and settled decisions |
| Planned work | [Roadmap](docs/roadmap.md) | Current scope, work packages, dependencies, approval and exit gates |
| Proposed designs | [API expansion design](docs/api-expansion-design.md) | Operation/resource alternatives and unresolved choices, not activation |
| Develop and verify | [Development](docs/development.md) | Tooling, offline checks, SDK gates and bounded real acceptance procedures |
| Sources and evidence | [References](docs/references.md) and its [research-note index](docs/references.md#research-notes) | Pinned sources, licenses and dated protocol findings |
| Implemented HTTP surface | [OpenAPI](docs/openapi.json) | API summary; codecs and tests remain the behavior specification |
| Agent workflow | [AGENTS.md](AGENTS.md) | Authorization, security, engineering and publication rules |

Read by task:

- **Run locally:** [environment](docs/development.md#environment) → [credentials](docs/usage.md#prepare-credentials) → [configuration](docs/usage.md#configure-the-gateway) → [startup](docs/usage.md#start-the-gateway).
- **Implement the next slice:** [execution sequence](docs/roadmap.md#execution-sequence-and-work-package-rules) → relevant Architecture contract or API design → [verification](docs/development.md#verification-gates).
- **Investigate compatibility:** [protocol baselines](docs/architecture.md#protocol-baselines-and-provider-dialects) → [pinned evidence](docs/references.md#pinned-sources) → [integration boundaries](docs/roadmap.md#integration-handoff).

## Operating boundaries

- Generation has JSON/SSE, tool and reasoning/replay carriers and selected media inputs. A carrier is not a promise that every model or client loop works.
- Routing is bounded, cache-affine failover, not load balancing. Publication forbids replay.
- Credentials come from the application store; inference never logs in or refreshes credentials. The CLI listener is loopback-only.
- Responses history is gateway-owned TTL memory, not durable execution or Provider-held conversation state.
- Standalone tasks expand only through selected [stage-4 slices](docs/roadmap.md#stage-4-scope). The [Embedding path](docs/architecture.md#independent-embedding-path) is independent of Generation; planned Files, other media and stage-5 APIs are not implied by it.

## Run

Copyable [configuration templates](docs/usage.md#configuration-templates) live in [`config/`](config/), including an optional Nginx TLS proxy. Copy them to private deployment storage and replace the deliberately invalid Agent-key placeholders before starting.

After preparing credentials, a pool, `gateway.json`, `auth.json` and `models.json` using [Usage](docs/usage.md):

```sh
cargo run --locked --offline --bin morphiecore
```

Credential management is a separate CLI:

```sh
cargo run --locked --offline --bin morphiecore-auth -- --help
```

For manual model enumeration and one-request upstream diagnostics, use
[`morphiecore-upstream`](docs/usage.md#manual-upstream-diagnostics). It uses the
local credential store without starting the gateway or activating models:

```sh
cargo run --locked --offline --bin morphiecore-upstream -- --help
```

## Source navigation

[Architecture's layer map](docs/architecture.md#layers-and-data-flow) owns module responsibilities. Current Providers and model bindings come from [shared Provider metadata](src/provider/providers.json), [its loader](src/provider/catalog.rs), [topology catalog](src/topology/catalog.rs) and [startup](src/gateway/startup/bootstrap.rs), not a duplicated model list in documentation. [Development](docs/development.md#test-layout) maps the test suites.

Original code and docs use the [MIT License](LICENSE). External material keeps its source and license.
