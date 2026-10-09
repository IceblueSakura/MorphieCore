# ADR 0009: Minimal Authenticated Loopback Gateway

## Status

Accepted.

## Decision

- Startup binds compiled topology, explicit entries and credentials. Authenticate before collecting business bodies.
- Strict bounded input resolves public model/task. Trusted output-budget policy precedes requirements and rejects excessive explicit limits rather than clipping them.
- Transport receives prepared trusted parts. Proxy policy is operator configuration; embedded constructors remain opt-in.
- Replay scope is bound to trusted entry/auth ownership. Incremental delivery uses publication, handoff and completion stages from the [execution model](../execution-model.md).
- Concurrency, bodies, payloads and absolute exchange duration are bounded. Timeout, cancellation, shutdown and late failure release resources.

## Rationale and consequences

The gateway exercises the library's semantic path without a general service framework. Bootstrap uses the owned store and explicit pools; request data cannot select credentials, headers, origins or scripts. Optional private diagnostics are bounded and nonblocking, and sink failures do not affect responses.

Owner: [HTTP guide](../../http-gateway.md). Implementation: [bootstrap](../../../src/gateway/bootstrap.rs), [admission](../../../src/gateway/admission.rs), [transport](../../../src/transport/http.rs) and [body](../../../src/gateway/body.rs).
