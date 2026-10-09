# ADR 0012: File-backed Credential Management and Ordered Pools

## Status

Accepted.

## Decision

- CredentialManager owns API keys and profile-bound OAuth grants with separate lifecycles. Drivers own authority/client wire and verified identity; JWT validation uses trusted issuer, algorithms, keys, audience and nonce.
- One application-owned file store serves CLI and Gateway, with explicit path overrides. Bounded Provider-namespace documents merge independently owned records under short transactions and stable operation locks; no document lock spans network or user interaction.
- Private filesystem checks, atomic publication, revisions and recovery markers protect storage. Failed login preserves the active grant. Refresh removes reusable secrets before transmission; uncertain consumption requires reauthorization, while received SIWC replacements can remain non-borrowable pending verification.
- API-key and OAuth access bindings pin their respective private identity/material generation or verified principal. Material/state changes invalidate affected bindings; already-borrowed attempt snapshots may finish.
- Pools declare ordered compatible credential references and opt-in bounded fallback under [ADR 0010](0010-canonical-model-fixed-fallback.md). Ordinary requests borrow access but do not log in or refresh. SIWC is single-registration, single-attempt and non-fallback.
- SIWC serves one user within the same self-developed application, with local credential ownership and model execution. Internal workers submit that user's authorized tasks without holding refresh/ID tokens. This scope is not provider approval; remote persistence requires resolution of the [applicable terms](../../references/siwc-login.md#远程-host-与分布式应用边界).
- Credentials and locators remain outside task IR and downstream diagnostics. Multi-member source-bound replay needs explicit affinity ownership; local removal and remote revocation are separate operations.

## Rationale and consequences

One store removes competing credential-selection paths while permitting coordinated CLI management without a daemon. Mature libraries own generic filesystem/serialization mechanics; the project owns identity, token-consumption, replay and fallback policy. The store requires cooperating writers and supported local filesystem guarantees; it is not distributed coordination or isolation from malicious same-user/administrator processes.

Owners: [manager](../../../src/credential/manager.rs), [store](../../../src/credential/store.rs), [filesystem](../../../src/credential/storage_fs.rs), [Windows ACL checks](../../../src/credential/storage_windows.rs), [access](../../../src/credential/access.rs) and [pools](../../../src/credential/pool.rs). Operations: [credential guide](../../credentials.md). Protocol sources: [Grok](../../references/grok-login.md), [SIWC](../../references/siwc-login.md).
