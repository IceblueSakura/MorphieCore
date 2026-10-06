# ADR 0009: Minimal HTTP Text Gateway

## Status

Accepted. Fixed multi-member execution is governed by [ADR 0010](0010-canonical-model-fixed-fallback.md); this record does not constrain every entry to one candidate.

## Decision

- Expose authenticated Chat/Responses Generation over loopback using the shared adapter/execution chain. Startup binds compiled topology, explicit entries and credentials; business JSON cannot supply them.
- Authenticate before body collection. Resolve public model/task from bounded strict JSON before semantic decode. Apply trusted output-budget policy before deriving requirements; reject excessive explicit limits instead of clipping them.
- HTTP transport receives prepared trusted parts. It does not mutate IR, forward inbound headers, inherit ambient proxies, follow redirects or retry implicitly.
- Bind replay scope to trusted entry and authentication ownership. This internal scope is not attestation of a client token's issuer.
- Use bounded incremental delivery and explicit acknowledgement. Body handoff to server transport is the conservative commit boundary, not proof of peer receipt. Completion requires validated upstream closure and final handoff.
- Bound concurrency, body collection, payload sizes and absolute exchange duration. Backpressure cannot suspend the upstream deadline. Timeout, cancellation, shutdown and late failure release resources without a fabricated terminal.

## Rationale and consequences

A minimal runnable gateway exercises the same semantic path as the library without importing a general service framework. Bootstrap reads explicit private configuration and credential-pool files only, with no ambient secret or account fallback. Dynamic registry, multi-user credential management and arbitrary configuration scripts require separate decisions. Optional diagnostics remain bounded, private and nonblocking; sink failure cannot affect business responses or expose upstream diagnostics downstream.

Owners: [bootstrap](../../../src/gateway/bootstrap.rs), [admission](../../../src/gateway/admission.rs), [HTTP transport](../../../src/transport/http.rs), [body lifecycle](../../../src/gateway/body.rs). Startup and public HTTP usage belong to the [gateway guide](../../http-gateway.md), not an ADR implementation snapshot.
