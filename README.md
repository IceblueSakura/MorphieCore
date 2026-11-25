# MorphieCore

MorphieCore is a local gateway that hides LLM Provider differences behind a single typed Intermediate Representation (IR). Clients communicate via OpenAI Chat Completions, OpenAI Responses, or Anthropic Messages; upstream protocols also include Google Interactions.

`pi`, `opencode`, and `codex` are the primary verified clients. See [current status](docs/roadmap/index.md#current-status) for active scope and roadmap gates.

## Documentation map

| Area | Authoritative document | Purpose |
|---|---|---|
| Use the gateway | [Usage](docs/usage/index.md) | Credentials, configuration, startup, requests, history, and errors |
| Accepted design | [Architecture](docs/architecture/index.md) | Shared principles, domain contracts, IR semantics, and settled decisions |
| Planned work | [Roadmap](docs/roadmap/index.md) | Current scope, unified unscheduled issues, work packages, dependencies, and exit gates |
| Proposed designs | [API expansion proposals](docs/proposals/index.md) | Operation proposals and future candidates |
| Development status | [Development](docs/development.md) | Overall acceptance status, tooling, test layout, and shared verification conventions |
| Sources and evidence | [References](docs/references.md) | Pinned sources, licenses, and dated research notes |
| Implemented HTTP surface | [OpenAPI](docs/openapi.json) | OpenAPI specification of supported HTTP endpoints |
| Agent workflow | [AGENTS.md](AGENTS.md) | Authorization, security, engineering, and publication rules |

## Operating boundaries

- **Generation:** JSON/SSE streaming, function tools, reasoning/replay carriers, and selected media inputs. Supported features depend on model and endpoint capabilities.
- **Routing:** Bounded, cache-affine failover without load balancing. Replay is forbidden once a response is published downstream.
- **Credentials:** Managed via local application store (`0700` directory, `0600` files). Background maintenance automatically renews OAuth accounts before expiration. The CLI listener is loopback-only (`127.0.0.1`).
- **Responses history:** Full-history stateless requests only. Stateful APIs are experimental and disabled until explicit activation; see the [stateful boundary](docs/architecture/history-and-replay.md#stateful-history).
- **Task independence:** Standalone tasks (such as [Embeddings](docs/architecture/index.md#independent-embedding-path)) operate independently of Generation; see [stage-4 pending scope](docs/roadmap/index.md#stage-4-pending-scope).

## Quick start

Follow [configuration templates](docs/usage/gateway-configuration.md#configuration-templates) to select a private directory, copy the public examples and provision credentials before starting:

```sh
# Run the gateway
cargo run --locked --offline --bin morphiecore

# Credential management CLI
cargo run --locked --offline --bin morphiecore-auth -- --help

# Upstream model diagnostics (without starting the gateway)
cargo run --locked --offline --bin morphiecore-upstream -- --help
```

See [Usage](docs/usage/index.md) for full configuration and startup details.

## Source navigation

- **Architecture layers:** See [Architecture's layer map](docs/architecture/index.md#layers-and-data-flow).
- **Providers and topology:** [Shared Provider metadata](docs/architecture/index.md#shared-provider-metadata) links the embedded data, compilation and startup binding.
- **Test suites:** Mapped in [Development](docs/development.md#test-layout).
- **Documentation gate:** Run `nix develop .#docs --offline --command python3 tests/check_docs.py`; see [documentation checks](docs/development.md#documentation-checks).

MorphieCore is licensed under the [MIT License](LICENSE). External references preserve their original licenses.
