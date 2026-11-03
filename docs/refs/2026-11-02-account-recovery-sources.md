# Account recovery source boundaries

Checked **2026-11-02** for the accepted [account rewrite](../provider-account-redesign-plan.md). This note records dependency and official-service evidence, not runtime acceptance or a new policy selection.

## Locked dependency selection

The crates.io API (`https://crates.io/api/v1/crates/<name>`) reports the following locked versions as the current non-yanked stable releases. Retain the existing exact pins and `Cargo.lock`; no upgrade or replacement library is needed for this rewrite.

| Area | Packages and releases |
|---|---|
| HTTP and TLS | `reqwest` 0.13.5, `hyper` 1.12.0, `hyper-util` 0.1.21, `rustls` 0.23.46, `tokio-rustls` 0.26.6 |
| Runtime | `tokio` 1.54.1, `tokio-util` 0.7.20 |
| Parsing and CLI | `serde` 1.0.229, `serde_json` 1.0.154, `clap` 4.6.7, `url` 2.5.8 |
| Storage | `fs-mistrust` 0.16.0, `atomicwrites` 0.4.4 |
| OAuth and hints | `oauth2` 5.0.0, `jsonwebtoken` 11.1.0, `httpdate` 1.0.3 |

Independently downloaded reqwest, hyper, hyper-util and tokio-rustls release archives from `static.crates.io` matched their `Cargo.lock` SHA-256 checksums. Inspection used those sources, not build caches. Protocol/SDK fixture pins remain independently owned by [the upstream source lock](../../tests/upstream/sources.lock.json).

## Transport facts before classification

- [reqwest `Error`](https://docs.rs/reqwest/0.13.5/src/reqwest/error.rs.html): `source()` retains the nested cause; `is_connect()` walks the hyper-util cause, and `is_timeout()` walks nested timeout/I/O causes. Check connection phase and timeout together before collapsing either fact.
- [hyper-util legacy client](https://docs.rs/hyper-util/0.1.21/src/hyper_util/client/legacy/client.rs.html): its error retains a boxed source and exposes `is_connect()`. Connection-stage failure alone does not distinguish transient I/O from TLS/configuration failure.
- [hyper `Error`](https://docs.rs/hyper/1.12.0/src/hyper/error.rs.html): `is_incomplete_message()` supplies positive HTTP/1 premature-termination evidence. The documented display text and internal source composition are not stable parsing contracts.
- [tokio-rustls stream](https://docs.rs/tokio-rustls/0.26.6/src/tokio_rustls/common/mod.rs.html): TLS packet-processing failure becomes `io::ErrorKind::InvalidData` with the TLS error inside. Do not treat every nested I/O error as transient.

At the review baseline, `transport/http.rs::send` loses connection-timeout phase by checking timeout first; body intake reduces errors to timeout/read, and protocol-terminal absence becomes a generic upstream error. Extract bounded typed facts at these boundaries, without keeping URLs or raw third-party errors. A clean HTTP EOF without a protocol terminal remains malformed output, not evidence of a transport interruption. Verify concrete source-chain shapes with local wire failures before relying on them for resend permission.

Existing `HttpTransport` disables redirects and reqwest retries. Keep that single-send transport and the existing publication/EOF fences. The accepted executor, not an HTTP client retry policy, owns the one additional same-target send.

## SIWC registration, rotation and errors

Read the verbatim official [accounts and sessions](https://developers.openai.com/siwc/token-sharing-open-source/profiles-and-sessions) and [errors and recovery](https://developers.openai.com/siwc/token-sharing-open-source/errors-and-recovery) pages on the date above:

- Keep issued clients, verified identities and token sets together; returning sign-in uses the saved issued client and stable host ID. Revocation ends the renewable session but does not delete the registration.
- Refresh uses the saved issued client and latest refresh material. Serialize each session's rotation across processes. The service does not grant a general right to replay uncertain refreshes.
- `subscription_sharing_usage_limit_exceeded` is quota evidence without an implied reset time. `subscription_sharing_usage_unavailable` and `subscription_sharing_user_unavailable` are temporary service failures; they do not justify erasing credentials.
- `invalid_grant`, `invalid_refresh_token`, `token_expired`, `refresh_token_expired`, `refresh_token_invalidated` and `refresh_token_reused` require unusable-token handling. `invalid_client` requires client-configuration repair.
- The network/5xx retry instruction under session termination concerns **revocation**, not permission to retry a potentially consumed refresh token. Preserve uncertainty/quarantine unless a profile supplies positive safe-reuse evidence.

Provider-scoped subject uniqueness and a single managed login are MorphieCore's accepted account rules; the service allows separate registrations with the same email. Do not infer security identity from email or equate an internal OAuth profile name with a configured Provider ID.

The existing pinned official Responses fixtures and SDK consumers remain the independent wire/consumer expectations. A successfully delivered `response.failed` is still a failed upstream task: it cannot recover health or commit conversation affinity.

## Provider-specific failure evidence

Rechecked on **2026-11-02** before replacing global substring classification:

- Alibaba [Model Studio error codes](https://help.aliyun.com/en/model-studio/error-code) define 429 `Throttling.AllocationQuota` / `insufficient_quota` as TPS/TPM throttling. The [Token Plan FAQ](https://help.aliyun.com/en/model-studio/token-plan-personal-faq) instead defines 429 `insufficient_quota` as exhausted monthly plan allowance. Provider identity must precede interpreting that exact code; neither message text nor “OpenAI-compatible” identifies the billing domain.
- Pinned OpenCode [error serialization](https://github.com/anomalyco/opencode/blob/148842ef7cb1bc34192b3fff57d0f9721ab9c801/packages/console/app/src/routes/zen/util/handler.ts#L482-L532) returns HTTP 401 for `CreditsError`, `MonthlyLimitError` and `UserLimitError`, alongside auth/model errors. Its exact `error.type` overrides generic status meaning. `GoUsageLimitError`, `FreeUsageLimitError` and `BlackUsageLimitError` use 429 and optional `Retry-After`; `RateLimitError` is a separate throughput class.
- OpenRouter [typed errors](https://openrouter.ai/docs/api/reference/errors-and-debugging) expose canonical `error_type` across API skins, including mid-stream errors. Native protocol spelling can be lossy; interpret confirmed canonical fields at the Provider boundary, before generic status fallback.
- The inspected primary references establish no configured Provider's special 419 contract. Do not invent a global auth mapping. Unknown structured codes retain actual status fallback; an unknown in-band error cannot gain resend permission merely from a synthetic HTTP-like hint.

**Follow-up recheck, 2026-11-03:** the SIWC and OpenRouter pages above confirm that SIWC temporary-service codes belong only to the `chatgpt` access contract. OpenRouter preserves canonical `error_type` in Chat error metadata, the Responses object's top level and the Messages error object; native code spelling alone can lose auth/quota meaning. Bounded readers retain only recognized finite tokens and reject malformed consumed canonical values. Standard Responses delivery coarsens confirmed SIWC codes under [the named disposition](../architecture.md#conversion-ladder) without changing source failure evidence or the failed task outcome.

These facts support Provider-owned finite classification, not operator-defined rules or probing private accounts. SIWC quota codes imply no reset deadline. Anthropic subscription-unified headers require a confirmed owning service contract; a Messages wire binding alone does not activate them.
