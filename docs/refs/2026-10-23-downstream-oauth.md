# Downstream OAuth authorization

Research date: **2026-10-23**. Evidence: official specifications, project documentation, release records and selected tagged source inspection. No authorization server was deployed and no agent SDK interoperability was tested. Versions below were the latest releases observed on that date, not dependency pins or security guarantees.

## Goal and protocol fit

The requested future deployment is a shared MorphieCore serving agents on multiple machines. Agent1 may launch agent2; a human approves agent2's access. Prefer existing OAuth infrastructure over a custom registration or credential-issuance protocol.

[OAuth Device Authorization Grant (RFC 8628)](https://www.rfc-editor.org/rfc/rfc8628.html) fits this initial flow: agent2 requests a device code, presents the verification URL/user code, and polls while the user approves in a trusted browser session. Agent1 may relay the URL/user code but need not receive the device code or issued tokens. The SDK/helper stores and refreshes tokens outside model context and ordinary logs.

Device Flow is human-approved headless login, not unattended workload authentication or automatic delegation. Agent1 launching agent2 need not create a security delegation relationship. One registered public OAuth client can represent an application used by multiple agent instances; `client_id` and the user's `sub` do not uniquely identify each instance. Verify independent device/session management before promising per-agent revocation.

For browser-capable native clients, [Authorization Code + PKCE](https://www.rfc-editor.org/rfc/rfc8252.html) remains an alternative. Retain independently revocable API keys or service identities for clients that cannot refresh tokens and for truly unattended workloads. Do not embed a shared client secret in distributed agents.

## Rust authorization-server comparison

| Candidate | Observed evidence | Assessment for this use case |
|---|---|---|
| **[Rauthy v0.37.2](https://github.com/sebadob/rauthy/releases/tag/v0.37.2)**, 2026-10-19, Apache-2.0 | Complete OAuth2/OIDC service with admin/user UI, Device Flow, device refresh-token storage and a [public-client example](https://github.com/sebadob/rauthy/blob/v0.37.2/rauthy-client/examples/device-code/README.md). Hiqlite by default; PostgreSQL alternative. Published independent security audit, with findings addressed in v0.32.1 | **First candidate to validate**, not an accepted dependency |
| **[Kanidm v1.11.2](https://github.com/kanidm/kanidm/releases/tag/v1.11.2)**, 2026-09-11, MPL-2.0 | Actively maintained identity platform, but Device Flow is behind `dev-oauth2-device-flow`; [stable source](https://github.com/kanidm/kanidm/blob/v1.11.2/server/lib/src/idm/oauth2.rs#L1308-L1367) still returns `InvalidGrant` on initiation and has unfinished polling | Exclude from this Device-Flow-first selection; protocol types/design docs are not working support |
| **[FerrisKey v0.9.0](https://github.com/ferriskey/ferriskey/releases/tag/v0.9.0)**, 2026-10-03, Apache-2.0 | Multi-realm IAM with Device Flow handlers, approval UI, discovery advertisement and [integration-test source](https://github.com/ferriskey/ferriskey/blob/v0.9.0/api/tests/device_flow_test.rs). These tests require explicit execution (`#[ignore]`) | Backup candidate; recent implementation and rapid security-boundary changes warrant additional acceptance. No comparable independent audit was verified in this review |
| **[oxide-auth](https://github.com/197g/oxide-auth)** | OAuth2 server library, not a complete identity service; this review did not establish complete Device Flow support | Does not meet the goal of avoiding custom login, consent, storage and administration |
| **[rust-oauth2-server v1.1.0](https://github.com/ianlintner/rust-oauth2-server/releases/tag/v1.1.0)** | Documents Device Flow and agent/token-exchange features; recent releases | Observation only: this review did not establish sufficient independent audit or operational evidence to prefer it |

The `oauth2` and `openidconnect` crates are useful for client/protocol integration, not substitutes for a complete authorization server. The [openidconnect documentation](https://docs.rs/openidconnect/latest/openidconnect/) explicitly excludes full provider credential/session management.

### Rauthy constraints

- [Security policy](https://github.com/sebadob/rauthy/blob/v0.37.2/SECURITY.md): no LTS or security-fix backports while below v1; plan timely upgrades. The [published audit](https://github.com/sebadob/rauthy/blob/v0.37.2/assets/security_audit_report_v0.32.pdf) does not certify every later release.
- [RFC 8707 support](https://github.com/sebadob/rauthy/blob/v0.37.2/book/src/work/resource_indicators.md): Device Flow does **not** process dynamic `resource` parameters. A dedicated MorphieCore client with administrator-configured `default_aud` is the simpler fixed-resource approach; verify the issued and refreshed audience in acceptance.
- [Device storage](https://github.com/sebadob/rauthy/blob/v0.37.2/src/data/src/entity/devices.rs) supports device invalidation and related refresh-token cleanup. This does not make an already issued JWT immediately invalid at an offline-verifying resource server.

## Integration boundaries and validation

Use a separate authorization service; MorphieCore is the resource server, not a new OAuth authorization-server implementation. Keep downstream access tokens separate from upstream Provider credentials. Business requests still cannot select origins, credential locators or auth/proxy headers.

Validate access tokens for the trusted issuer, intended API audience, expiry and granted permissions. For JWTs, verify signatures and allowed algorithms with trusted key discovery; never merely decode claims or accept an ID token as API authorization. Opaque tokens require authenticated introspection. Choose short-lived access tokens with a documented revocation delay, or online checks when immediate rejection of new calls is required. Refresh-token revocation alone does not invalidate offline-verified JWTs.

Before selection, verify:

1. The actual agent SDK accepts a non-built-in issuer and public-client Device Flow, rather than only a hard-coded Provider login.
2. Human approval, denial, expiry, polling backoff and successful API access work end to end.
3. Token storage and automatic refresh work during a long-running agent session; audience and scopes remain correct after refresh.
4. Two agent authorizations can be managed independently; revoking one blocks its renewal without affecting the other. Document the old access-token window and in-flight request policy.
5. MorphieCore rejects invalid signatures, wrong issuer/audience, expired tokens and insufficient permissions.
6. Central deployment separately establishes HTTPS, trusted proxy handling and ownership isolation for history, files and future jobs. This research does not relax current loopback-only ingress.

If future requirements allow agent1 to authorize children without individual human approval, revisit bounded preauthorization or [RFC 8693 Token Exchange](https://www.rfc-editor.org/rfc/rfc8693.html). Neither Device Flow nor Token Exchange automatically supplies user consent, permission attenuation, shared budgets or cascading revocation. Do not make that larger delegation system a prerequisite for the initial flow.
