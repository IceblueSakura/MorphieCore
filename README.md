# MorphieCore

MorphieCore is a local gateway that hides LLM Provider differences behind a single typed Intermediate Representation (IR). Clients communicate via OpenAI Chat Completions, OpenAI Responses, or Anthropic Messages; upstream protocols also include Google generateContent and Interactions.

`pi` and `opencode` are the primary verified clients. See [current status](docs/roadmap.md#current-status) for active scope and roadmap gates.

## Documentation map

| Area | Authoritative document | Purpose |
|---|---|---|
| Use the gateway | [Usage](docs/usage.md) | Credentials, configuration, startup, requests, history, and errors |
| Accepted design | [Architecture](docs/architecture.md) | Shared principles, domain contracts, IR semantics, and settled decisions |
| Planned work | [Roadmap](docs/roadmap.md) | Current scope, work packages, dependencies, and exit gates |
| Proposed designs | [API expansion design](docs/api-expansion-design.md) | Operation proposals and future candidates |
| Frozen review proposal | [Provider account redesign plan](docs/provider-account-redesign-plan.md) | Multi-account requirements, proposed recovery policy, and unresolved decisions; no implementation approval |
| Develop and verify | [Development](docs/development.md) | Tooling, offline checks, SDK gates, and acceptance procedures |
| Sources and evidence | [References](docs/references.md) | Pinned sources, licenses, and dated research notes |
| Implemented HTTP surface | [OpenAPI](docs/openapi.json) | OpenAPI specification of supported HTTP endpoints |
| Agent workflow | [AGENTS.md](AGENTS.md) | Authorization, security, engineering, and publication rules |

## Operating boundaries

- **Generation:** JSON/SSE streaming, function tools, reasoning/replay carriers, and selected media inputs. Supported features depend on model and endpoint capabilities.
- **Routing:** Bounded, cache-affine failover without load balancing. Replay is forbidden once a response is published downstream.
- **Credentials:** Managed via local application store (`0700` directory, `0600` files). Background maintenance automatically renews OAuth accounts before expiration. The CLI listener is loopback-only (`127.0.0.1`).
- **Responses history:** Gateway-owned in-memory TTL storage; does not rely on Provider-held conversation state.
- **Task independence:** Standalone tasks (such as [Embeddings](docs/architecture.md#independent-embedding-path)) operate independently of Generation; see [stage-4 scope](docs/roadmap.md#stage-4-scope).

## Quick start

Configuration templates live in [`config/`](config/). Copy them to private deployment storage and set valid API keys before starting:

```sh
# Run the gateway
cargo run --locked --offline --bin morphiecore

# Credential management CLI
cargo run --locked --offline --bin morphiecore-auth -- --help

# Upstream model diagnostics (without starting the gateway)
cargo run --locked --offline --bin morphiecore-upstream -- --help
```

See [Usage](docs/usage.md) for full configuration and startup details.

## Source navigation

- **Architecture layers:** See [Architecture's layer map](docs/architecture.md#layers-and-data-flow).
- **Providers and topology:** Defined in [providers.json](src/provider/providers.json), [catalog loader](src/provider/catalog.rs), [topology catalog](src/topology/catalog.rs), and [bootstrap](src/gateway/startup/bootstrap.rs).
- **Test suites:** Mapped in [Development](docs/development.md#test-layout).

MorphieCore is licensed under the [MIT License](LICENSE). External references preserve their original licenses.
