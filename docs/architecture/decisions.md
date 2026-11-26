<a id="settled-decisions"></a>
# Settled Architectural Decisions

Authoritative log of accepted cross-cutting architectural decisions and key Provider-wide policies across MorphieCore subsystems.

See [Architecture Overview](index.md) for system boundaries and core IR semantics, [Protocol adaptation](adaptation.md) for cross-provider rules, and [Provider dialects](provider-dialects.md) for per-Provider contracts.

## Contents

- [Scope and activation decisions](#scope-and-activation-decisions)
- [Semantic ownership decisions](#semantic-ownership-decisions)
- [Projection and reasoning decisions](#projection-and-reasoning-decisions)
- [History and cache decisions](#history-and-cache-decisions)
- [Routing and security decisions](#routing-and-security-decisions)
- [Delivery and evidence decisions](#delivery-and-evidence-decisions)

### Scope and activation decisions

| Decision | Scope | Accepted Architectural Consequence |
|---|---|---|
| Standards reset & redesign | Architecture | Pinned official standards and upstream test suites govern baseline behavior, with request and delivery handling directed by the conversion ladder. [Ladder](adaptation.md#conversion-ladder) |
| Standard downstream APIs | Protocols | Chat, Responses, and Messages use official semantic carriers only without private client replay profiles or semantic retention extensions. [Stateless boundary](history-and-replay.md#stateless-api-boundaries) |
| Latest Messages baseline | Codec | Standardizes on current official 2026-10-21 body grammar (admitting `2023-06-01` over it); removed legacy sampling fields validate then omit, and `anthropic-beta` headers are ignored. |
| Google Interactions exclusive deployment | Google | Uses Google Interactions (`/v1/interactions`) exclusively for all deployed Gemini generation and `gemini-nano-banana-2.1` image paths after removing `generateContent`, aligning stateless Responses with explicit step/reasoning ownership. [Google Interactions](provider-dialects.md#google-interactions) |
| OpenAI Platform route ownership | Catalog | Uses the direct `openai` API-key Provider for OpenAI Platform models rather than retired OpenRouter bindings while preserving isolated `chatgpt` SIWC provenance. [Replay isolation](history-and-replay.md#cross-protocol-and-cross-provider-history) |
| Shared Aliyun dialect & verified-only strictness | Bailian | DashScope and TokenPlan share the `Bailian` dialect and default to `bailian.relax-function-strictness.v1` unless an exact model/API pair is independently verified; structured text strictness and tool selection stay strict. [Bailian policy](provider-dialects.md#shared-aliyun-billing-dialect) |
| SIWC flat tool carriage | SIWC | Candidate-local lowering moves flat function/custom definitions into a leading developer `additional_tools` batch while preserving leaf identities, schemas, selection, and history order. [Subscription Responses](provider-dialects.md#subscription-responses) |
| Capability discovery boundary | Discovery | Enhanced model descriptors (`GET /v1/morphiecore/models`) and `/v1/morphiecore/capabilities` reuse the compiled execution projector within a fail-closed 4 MiB budget without exposing candidate order/identity or ranking fallbacks by conversion loss. [Discovery](adaptation.md#generation-text-model-discovery) |
| Native static image delivery & media upgrades | Images | Uses default OpenAI Images JSON/SSE shape with bounded `delivery.ffmpeg` format normalization and gateway preparation timestamps when upstream omits `created`; keeps only the latest qualified release per media variant without aliases. [Static images](adaptation.md#native-static-images) |
| Independent Embedding & Decision activation | Standalone tasks | Decouples Embeddings and Decision from Generation. Explicit bindings select the upstream and a downstream SDK family; separate roots preserve the incompatible discovery envelopes. A Generation label does not activate Decision. [Decision profile](index.md#decision-api-direction) |
| Partial delivery | Roadmap | Delivers usable principal workflows, recorded low-loss conversions, and verified affected paths, deferring unresolved boundaries rather than blocking on closed observations. [Exit criteria](../roadmap/index.md#partial-delivery-exit) |

### Semantic ownership decisions

| Decision | Scope | Accepted Architectural Consequence |
|---|---|---|
| Standard request boundary | Ingress | Public client intake enforces official protocol syntax; Provider dialects remain isolated to upstream egress. |
| Conversion ladder | Semantics | Ordered precedence: same-name (`L1`) → same-semantic (`L2`) → partial lossy (`L3`) → omit (`L4`) → reject (`L5`) only when omission misrepresents satisfaction; unknown target facts delegate upstream under unified disposition ownership. [Ladder](adaptation.md#conversion-ladder) |
| Runtime-owned execution strategy | Gateway | Rejects target restrictions requiring strategy replacement pre-dispatch with actionable errors; never silently disables thinking or rewrites history. [Boundary](index.md#system-boundaries) |
| Candidate strict-tool & cardinality ownership | Tools | Shared lowering owns conditional strict resolution (falling back non-strict when unclosed or exceeding grammar budget while guarding explicit `strict:true`), directional call cardinality normalization, and grouped call-ID association. [Google strictness](provider-dialects.md#google-strict-function-arguments) |
| Unified ordered-tool & phase projection | Tools & IR | One shared batch/disposition model projects ordered tool additions across Chat, Responses, Messages, and Interactions without altering immutable source IR; non-Responses targets omit `phase`. [Ordered tools](index.md#ordered-tool-availability) |
| Semantic-equivalence compatibility | Semantics | Protocol-defined equivalent empty/null defaults map losslessly (`L2`) without loss receipts, distinct from meaningful `L3`/`L4` removals and strict upstream snapshot checks. [L2 rules](adaptation.md#semantic-equivalence-compatibility) |
| IR simplification & canonical roles | Semantics | Unifies application instructions under canonical `Developer`, preserves participant ownership where supported, uses `Option<T>` presence except contract-defined nulls, and decouples `UsageReport` scopes. |

### Projection and reasoning decisions

| Decision | Scope | Accepted Architectural Consequence |
|---|---|---|
| Independent activation, effort scope & limits | Reasoning | Decouples boolean activation from `Reasoning`/`Response` effort scopes and enforces target-qualified native effort/mandatory-thinking bounds identically across dispatch and discovery. [Reasoning controls](adaptation.md#reasoning-controls) |
| Explicit effort precedence & preset quantization | Reasoning | Preserves explicit effort over conflicting numeric budgets, quantizes absent-effort budgets across 6 canonical tiers (ties upward), and rejects budgets exceeding explicit output ceilings. [Quantization](adaptation.md#thinking-budget-quantization) |
| Ordered effort projection | Reasoning | Resolves source-timed `FollowingResponse`/`NewInputTurn` updates before target conversion, keeping equivalent native markers and rejecting invalidated protected replay on earlier prefix edits. [Ordered effort](adaptation.md#ordered-effort-projection) |
| Low-loss output ceilings | Ceilings | Preserves equivalent scopes (`L2`), clamps to model maxima or adapts reply-only scope (`L3`), and omits uncarriable ceilings (`L4`) without tokenizer estimation or overwriting source absence. [Output ceilings](adaptation.md#output-ceiling-adaptation) |
| Downstream schema completion & terminal projection | Delivery | Fills missing downstream schema fields via upstream facts → request inference (including no-stop Messages `end_turn`) → byte estimates, while coercing tool-result errors when native flags are absent. [Schema completion](adaptation.md#downstream-schema-completion) |

### History and cache decisions

| Decision | Scope | Accepted Architectural Consequence |
|---|---|---|
| Append-only & opaque replay isolation | History | Projects earlier turns immutably; envelopes opaque tokens in deployment/origin-scoped `mc1.` carriers (service-source scope for native OpenAI reasoning, model-specific target scope for other signatures/compaction). [Reasoning replay](history-and-replay.md#reasoning-replay) |
| Visible-history transfer without text conversion | History | Omits uncarried reasoning owners under named `L3` rules while preserving visible messages and tool links; never converts reasoning into assistant text or forges signatures. |
| Stateful API freeze | Responses & upstreams | Downstream storage, reference continuation, and stored-resource operations remain disabled/rejected (`store:false`/absent/null stateless only) until explicitly enabled. [Stateful boundary](history-and-replay.md#stateful-history) |
| Compacted context & owner-local caches | History & Cache | Admits owner-local cache breakpoints across routes (omitting per candidate when uncarried), preserves opaque compaction items where supported, and rejects uncarriable compaction targets. |

### Routing and security decisions

| Decision | Scope | Accepted Architectural Consequence |
|---|---|---|
| Cache-first bounded recovery | Routing | Single pre-dispatch candidate selection preferring configured order, advisory cooldowns, and cache affinity, with at most one pre-publication same-target retry and no post-dispatch switching or load balancing. [Execution policy](runtime-and-routing.md#failover) |
| Provider-owned credentials & account identity | Accounts | Each Provider fixes one credential kind and optional OAuth profile; versioned ordered named accounts bind explicit targets without backward-compatibility migrations, aliases, or ambient secret reuse. [Account model](index.md#shared-provider-metadata) |
| Managed OAuth lifecycle & quarantine | OAuth | Background renewal locks accounts expiring within 120s, preserves removal tombstones, quarantines refresh reuse across uncertain rotations until verification, and resumes network/timeout verification automatically. [OAuth maintenance](runtime-and-routing.md#oauth-maintenance) |
| Ingress auth, entry path safety & reverse proxy | Security | Enforces loopback ingress, constant-time Agent authentication, unescaped concrete entry paths, and reverse-proxy TLS termination without leaking private origins or credentials. |
| Application configuration ownership | Config | Single current strict schema; operators configure service budgets and deadlines while protocol and safety ceilings stay code-owned. [Current fields](../usage/gateway-configuration.md#configure-the-gateway) |

### Delivery and evidence decisions

| Decision | Scope | Accepted Architectural Consequence |
|---|---|---|
| Honest streaming, EOF & settlement | Delivery | Responses, native Messages, and Google run always-streaming intake; publication freezes failover, completion requires validated EOF, and synchronous final settlement resists post-final cancellation without relaxing live cooldowns. [Delivery](runtime-and-routing.md#streaming-and-delivery) |
| Adapter-owned delivery projection & evidence | Evidence | Single `ClientProjection` selects client facts (`SSE` events reduced for `JSON`), paired with bounded `EvidenceDelta` (`Conversion<T>`) and adapter-coarsened error/terminal envelopes. [Delivery ownership](adaptation.md#delivery-projection-ownership) |
| OpenTelemetry observability & opt-in exchange capture | Observability | Uses standard `tracing` + bounded OTLP HTTP/JSON export decoupled from execution outcomes, with four optional auth-redacted capture points for WARN/ERROR diagnosis. [Observability](runtime-and-routing.md#observability) |
| Upstream test pins & high-signal pruning | Verification | Pinned official SDK/upstream fixtures anchor protocol regressions, while implementation-restatement and redundant permutation tests are pruned in favor of directional/safety assertions. [Selection policy](../development.md#test-selection-and-pruning) |
