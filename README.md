# MorphieCore

A central gateway built on one unified IR that hides Provider differences from agent loops (pi and opencode first, then Codex and Claude Code). Clients speak OpenAI Chat Completions or Responses; upstream Providers speak OpenAI-compatible, Anthropic Messages and, later, Google protocols. The project is in core design and validation.

## Status

- **Working:** downstream Responses and Chat (JSON and SSE; text, tools, reasoning, images and some file inputs); upstream OpenAI Responses and Chat with several compatible dialects; Anthropic Messages upstream through explicitly compiled topology; file credentials with ordered pools; Provider-native cache affinity.
- **Frozen:** the standalone Embeddings, Images, Speech and Transcription endpoints, and new Providers.
- **Next:** OpenAI clients on the native Messages binding, then held streams with pre-content fallback, then routing with cooldown and target affinity, then Google generateContent and Interactions, then downstream Anthropic Messages. See the [roadmap](docs/roadmap.md).

## Quick start

```sh
cargo run --locked --offline --bin morphiecore        # gateway
cargo run --locked --offline --bin morphiecore-auth   # credential management
```

Configuration, endpoints and credentials: [usage](docs/usage.md).

## Layout

| Path | Contents |
|---|---|
| `src/semantic` | IR: task types, validation, event reducer |
| `src/protocol` | Wire codecs, capabilities, dialects and protocol-specific projections |
| `src/lowering`, `src/adapter` | Cross-protocol capability dispatch, generic requests and errors; legacy adapters for frozen tasks |
| `src/topology`, `src/provider` | Model → Route → Endpoint bindings; Provider definitions |
| `src/execution`, `src/transport` | Candidate execution; HTTP and SSE |
| `src/gateway` | HTTP gateway |
| `src/credential` | Credential store, API keys, OAuth |
| `tests/` | Offline tests and the SDK loopback gate |

## Documentation

- [Architecture](docs/architecture.md): principles and settled decisions
- [Roadmap](docs/roadmap.md): phases, frozen scope, current focus
- [Usage](docs/usage.md): startup, configuration, HTTP API, credentials
- [Development](docs/development.md): environment, checks, tests
- [References](docs/references.md): pinned sources and protocol docs
- Contribution rules: [AGENTS.md](AGENTS.md)

Original code and docs are under the [MIT License](LICENSE); external material keeps its source and license.
