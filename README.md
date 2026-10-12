# MorphieCore

MorphieCore is a local gateway that hides Provider differences from agent clients. It translates requests and responses through one typed intermediate representation (IR).

Clients use OpenAI Chat Completions, OpenAI Responses or Anthropic Messages. Upstream protocols include OpenAI-compatible APIs, Anthropic Messages, Google generateContent and Google Interactions.

The project is in core design and validation. pi and opencode are the first target clients; Codex and Claude Code follow.

## Start here

| Goal | Read |
|---|---|
| Configure and run the gateway | [Usage](docs/usage.md) |
| Understand the design | [Architecture](docs/architecture.md) |
| Build and verify changes | [Development](docs/development.md) |
| Check current scope and future work | [Roadmap](docs/roadmap.md) |
| Find protocol sources and licenses | [References](docs/references.md) |
| Inspect the HTTP API summary | [OpenAPI](docs/openapi.json) |

## Scope

- Generation supports JSON and SSE, tool calls, reasoning replay and selected media inputs.
- Routing uses bounded failover across a model's Providers and credentials, with conversation affinity. It is not load balancing.
- Credentials come from the application's private file store. Requests never log in or refresh credentials.
- Standalone Embeddings, Images, Speech and Transcription endpoints are frozen.
- Stored Responses history and file services are future work.

Code and tests define behavior. The [roadmap](docs/roadmap.md) separates implemented work from deferred acceptance checks. Supported models and bindings come from the catalogs, not this page.

## Run

Prepare credentials, a pool and `gateway.json` as described in [Usage](docs/usage.md), then run:

```sh
cargo run --locked --offline --bin morphiecore
```

The gateway listens on loopback only. Credential management uses a separate CLI:

```sh
cargo run --locked --offline --bin morphiecore-auth -- --help
```

For the build environment, see [Development](docs/development.md#environment).

## Code layout

| Path | Responsibility |
|---|---|
| `src/semantic` | Task IR, validation and stream reduction |
| `src/protocol` | Wire codecs, dialects and protocol-specific projections |
| `src/lowering`, `src/adapter` | Capability dispatch, request preparation and frozen-task adapters |
| `src/topology`, `src/provider` | Model routes, endpoint bindings and Provider definitions |
| `src/execution`, `src/transport` | Candidate execution, HTTP and SSE |
| `src/gateway` | Ingress, startup, routing and body delivery |
| `src/credential` | Private store, API keys, OAuth and pools |
| `tests/` | Offline contracts and separate SDK gates |

Agent rules and authorization are in [AGENTS.md](AGENTS.md).

Original code and docs use the [MIT License](LICENSE). External material keeps its source and license.
