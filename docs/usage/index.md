# Usage Guide

This guide owns local setup, configuration, credential management, client-facing request procedures, and HTTP API operations for implemented interfaces.

The usage documentation is modularized into specialized domain guides:

| Domain | Authoritative document | Responsibilities |
|---|---|---|
| **Credentials & Diagnostics** | [Credentials and Diagnostics](credentials-and-diagnostics.md) | `morphiecore-auth` (API keys, account declaration, OAuth), `morphiecore-upstream` (probing, model listing) |
| **Gateway Configuration** | [Gateway Configuration and Deployment](gateway-configuration.md) | Configuration templates, Agent API keys, diagnostics capture, model definitions, daemon startup, and Nginx reverse proxy |
| **Client Requests** | [Client Request Guide](client-requests.md) | Request wire formats (Responses, Chat Completions, Messages), cache markers, custom tools, media, stored responses, and cache affinity |
| **API Reference** | [HTTP API Reference and Errors](api-reference.md) | Standalone endpoints (Embeddings, Model discovery, Capability notices) and unified error mapping |

[Architecture](../architecture/index.md) explains the underlying contracts, and [Roadmap](../roadmap/index.md) separates selected scope from acceptance. For tooling and verification, see [Development](../development.md). Planned endpoints in the [API design](../proposals/index.md) are not instructions for currently available APIs.

## Quick start

Configuration templates live in [`config/`](../../config/). Copy them to private deployment storage and set valid API keys before starting:

```sh
# Run the gateway
cargo run --locked --offline --bin morphiecore

# Credential management CLI
cargo run --locked --offline --bin morphiecore-auth -- --help

# Upstream model diagnostics (without starting the gateway)
cargo run --locked --offline --bin morphiecore-upstream -- --help
```
