# MorphieCore

A central gateway built on one unified IR that hides Provider differences from agent loops (pi and opencode first, then Codex and Claude Code). Clients speak OpenAI Chat Completions, Responses or Anthropic Messages; upstream Providers speak OpenAI-compatible, Anthropic Messages and Google generateContent. The project is in core design and validation.

## Status

- **Working:** downstream Responses, Chat and Anthropic Messages (JSON and SSE; text, tools, reasoning, images and some file inputs); upstream OpenAI Responses and Chat with several compatible dialects native Anthropic Messages and Google generateContent; failover across a model's Providers and keys with held streams, cooldown and per-conversation affinity; file credentials with ordered pools; Provider-native cache affinity. pi passes real-client acceptance on all three downstream protocols.
- **Frozen:** the standalone Embeddings, Images, Speech and Transcription endpoints, and new Providers.
- **Next:** the Google Interactions upstream as a second binding of `gemini-3.8-flash`. Real failover acceptance, the coverage pass and Claude Code acceptance are deferred. See the [roadmap](docs/roadmap.md).

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
