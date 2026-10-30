# References

This document separates protocol authority, reproducible regression pins, implementation references and dated research. Current behavior belongs to [Architecture](architecture.md), scope to [Roadmap](roadmap.md), and execution/acceptance to [Development](development.md).

## Contents

- [Source guides](#source-guides)
- [Pinned sources](#pinned-sources)
- [Test source repositories](#test-source-repositories)
- [Research notes](#research-notes)
- [Protocol docs](#protocol-docs)
  - [OpenAI](#openai)
  - [Anthropic](#anthropic)
  - [Google Gemini](#google-gemini)
  - [OpenRouter](#openrouter)
  - [OpenCode Go](#opencode-go)
  - [JSON Schema](#json-schema)
- [OAuth and OIDC](#oauth-and-oidc)

## Source guides

Start here for source navigation rather than an old field-support matrix. Reviewed **2026-10-27**; each guide records release, immutable revision, license, useful paths and limits.

| Reference | Long-term use | Evidence boundary |
|---|---|---|
| [OpenAI / Anthropic / Google official SDKs](refs/2026-10-27-official-sdk-sources.md) | Request/output unions, serializers, accumulators, version/retry defaults, literal fixtures and SDK disagreements | Official source evidence, not automatic wire conformance or pin promotion |
| [pi](refs/2026-10-27-client-and-proxy-sources.md#pi-actual-consumer-and-history-behavior) | Actual history, replay, strict-tool fallback, thinking allocation, compaction and event consumers | Client behavior; acceptance uses current host-installed Agents |
| [Magpie](refs/2026-10-27-client-and-proxy-sources.md#magpie-cache-affinity-and-failure-ownership) | Cache affinity, scoped failures, publication and refresh ownership | Mechanism reference; post-publication continuation and inferred usage are not gateway policies |
| [LiteLLM](refs/2026-10-27-client-and-proxy-sources.md#litellm-directional-bridges-and-counterexamples) | Directional bridge state, tool indices, provider conversions and counterexamples | Not protocol authority; `enterprise/` is excluded from MIT |
| [CLIProxyAPI](refs/2026-10-27-client-and-proxy-sources.md#cliproxyapi-bootstrap-retries-late-results-and-refresh) | Bootstrap isolation, stale results/cooldown, late usage and refresh epochs | Do not copy identity repair, successful EOF synthesis or round-robin policy |

Prefer official service contracts for semantics, versioned SDKs for concrete shapes/consumer behavior, and implementation projects for mechanisms or counterexamples. When sources disagree, record the discrepancy rather than selecting the most permissive behavior. Source tests become regression coverage only through the existing asset process.

## Pinned sources

### Regression release selection

Resolved on **2026-10-23**. [tests/upstream/sources.lock.json](../tests/upstream/sources.lock.json) is the authority for release tags, commits, licenses and candidate fixture paths. Package execution is separately locked by Cargo, uv, npm and Nix. A source pin is not fixture integration, design acceptance or live compatibility.

| Source | Stable selection |
|---|---|
| OpenAI Python / Node | 3.37.0 / 7.43.0 |
| Anthropic Python / TypeScript | 2.1.0 / 1.1.0; API migration target `2026-10-21` |
| Google Gen AI Python / JavaScript | 2.33.0 / 2.31.0; keep v1/v1beta contracts distinct |
| Vercel | `ai` 7.0.150; OpenAI 4.0.101, Anthropic 4.0.87, Google 4.0.104; package release tags, not monorepo HEAD |
| pi / OpenCode | earendil-works/pi 1.4.0 / OpenCode 1.19.4 reference sources; actual Agent acceptance uses current host-installed versions independently |
| EventSource Parser | 4.1.2 |
| WPT / Google protobufs | Immutable source snapshots; these sources have no selected package release |
| Codex | 0.164.0 reference source only; no additional client activation |

OpenCode's selected release corresponds to npm `opencode-ai`. `pi` source uses the `earendil-works/pi` fork.

The [stable-release protocol recheck](refs/2026-10-23-stable-protocol-contracts.md) records the migration and fixture boundaries. The [SDK source guide](refs/2026-10-27-official-sdk-sources.md#source-selection-and-licenses) records newer inspected releases separately. Neither research refresh nor a source clone changes the regression selection.

### Telemetry dependency selection

Resolved from crates.io release metadata on **2026-10-24**:

| Packages | Selected release / source revision | License |
|---|---|---|
| `tracing` | 0.1.44 / [`2d55f6f`](https://github.com/tokio-rs/tracing/tree/2d55f6faf9be83e7e4634129fb96813241aac2b8/tracing) | MIT |
| `tracing-subscriber` | 0.3.23 / [`54ede4d`](https://github.com/tokio-rs/tracing/tree/54ede4d5d85a536aed5485c5213011d9ec961935/tracing-subscriber) | MIT |
| `tracing-opentelemetry` | 0.34.0 / [`8c1a02d`](https://github.com/tokio-rs/tracing-opentelemetry/tree/8c1a02d68034de317607ada0b1dd0d98abefd6cd) | MIT |
| `opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp`, `opentelemetry-appender-tracing` | 0.33.1 / [`372ebbc`](https://github.com/open-telemetry/opentelemetry-rust/tree/372ebbc74d4dba71bb9820732f57f7316b4e7a1e) | Apache-2.0 |

Primary contract entry points: [OTLP HTTP/runtime/configuration](https://docs.rs/opentelemetry-otlp/0.33.1/opentelemetry_otlp/), [log bridge mapping](https://docs.rs/opentelemetry-appender-tracing/0.33.1/opentelemetry_appender_tracing/), [trace bridge configuration](https://docs.rs/tracing-opentelemetry/0.34.0/tracing_opentelemetry/struct.OpenTelemetryLayer.html), and [batch log processor source](https://github.com/open-telemetry/opentelemetry-rust/blob/372ebbc74d4dba71bb9820732f57f7316b4e7a1e/opentelemetry-sdk/src/logs/batch_log_processor.rs). See [Development](development.md#telemetry-ecosystem-checks) for executable checks.

Historical revisions remain next to the specific dated observations that need them, not in a second version catalog. The SDK loopback gate uses `tests/sdk/uv.lock`; source pins and package execution locks are distinct. Passing old scenarios under a new SDK does not accept new protocol semantics.

## Test source repositories

Selected on 2026-10-23 for [integrated text/SSE redesign acceptance and later operations](roadmap.md#test-supplementation). Release revisions are fixed in the source lock; the links below identify fixture families, not imported coverage. [Development](development.md#upstream-test-sources) owns acquisition, license/provenance checks and later release updates.

The imported subset is identified in [assets.lock.json](../tests/upstream/assets.lock.json), with [explicit fixture adaptations and exclusions](refs/2026-10-23-stable-protocol-contracts.md#imported-asset-boundaries). Selected Anthropic 2.x/1.x SDK sources use Apache-2.0; older MIT source evidence does not determine their licenses.

| Source | Repositories | Relevant assets |
|---|---|---|
| OpenAI official SDKs | [Python](https://github.com/openai/openai-python), [Node](https://github.com/openai/openai-node) | Chat/Responses request, response, streaming, parser and returned-history tests; applicable interruption, backpressure and cleanup cases. Operation/resource tests are candidates for selected stage-4 slices |
| Anthropic official SDKs | [Python](https://github.com/anthropics/anthropic-sdk-python), [TypeScript](https://github.com/anthropics/anthropic-sdk-typescript) | Messages blocks, tool-input fragments, thinking/signatures, partial output and history fixtures; not automatic tool runners |
| Google Gen AI official SDKs | [Python](https://github.com/googleapis/python-genai), [JavaScript](https://github.com/googleapis/js-genai) | generateContent/Interactions wire and stream cases within selected upstream directions; operation-specific Image/Embedding candidates. Not automatic function calling or new native client admission |
| Vercel AI SDK | [Repository](https://github.com/vercel/ai) | Official-protocol and Provider packages, including `openai`, `anthropic`, `google`, `alibaba`, `deepseek`, `xai` and `openai-compatible`; raw JSON/chunk fixtures and applicable adapter assertions, including image-model candidates for stage 4. Not its IR or implicit repair policies |
| Web Platform Tests | [EventSource](https://github.com/web-platform-tests/wpt/tree/master/eventsource) | SSE format/parser cases; browser-only API, reconnection and security-context behavior are separate |
| EventSource Parser | [Repository and tests](https://github.com/rexxars/eventsource-parser) | Incremental parsing, multiline/multibyte, buffering and stream regressions; reference behavior is not authority for gateway resource budgets |

Image test paths observed in the 2026-10-23 source inventory include OpenAI Python `tests/api_resources/test_images.py`, Google Python `google/genai/tests/models/test_generate_images.py` and `test_edit_image.py`, and Vercel `packages/openai/src/image/openai-image-model.test.ts`, `packages/google/src/google-image-model.test.ts` and `packages/xai/src/xai-image-model.test.ts`. These are inspection candidates, not an audited coverage claim or selected Generate/Edit/streaming contract. Recheck their contents and revisions when selecting each operation.

## Research notes

Keep dated evidence that is not better owned by a current source guide, accepted contract or executable test. Do not maintain repair diaries or duplicate support matrices here. Older dates are not themselves grounds for deletion: unresolved source conflicts and future-operation evidence remain useful.

### Generation and client boundaries

| Note | Retained evidence |
|---|---|
| [Protocol reset, 2026-10-23](refs/2026-10-23-stable-protocol-contracts.md) | API migration, strictness/cache/effort distinctions and imported-fixture boundaries |
| [Request boundaries, 2026-10-26](refs/2026-10-26-standard-request-boundaries.md) | Independent request/history union, nullability, grammar and replay counterexamples; resolved disposition, not an open defect list |
| [Client/IR boundaries, 2026-10-27](refs/2026-10-27-client-ir-boundaries.md) | Output/history distinctions, dated capability families, item status, metadata, binding-history source gaps, semantic-equivalence review, custom-tool source disagreement and the 2026-10-30 conversion-ladder inventory |
| [Google and cross-protocol distinctions, 2026-10-17 onward](refs/2026-10-17-generation-contracts.md) | Lifecycle/resource versus event meaning, unresolved streamed score scope and sparse accounting relationships |
| [Instruction authority, 2026-10-25](refs/2026-10-25-instruction-authority.md) | Application versus vendor authority, legacy roles and Provider/Vercel comparisons |
| [Model discovery, 2026-10-26](refs/2026-10-26-model-discovery.md) | Capacity source/applicability, unknown limits and pi image carriers |
| [Chat custom tools, 2026-10-16](refs/2026-10-16-chat-custom-tools.md) | Original complete-value carriers; later SSE disagreement linked separately |
| [Bailian compatible requests, 2026-10-19](refs/2026-10-19-bailian-compatible-requests.md) | Model-specific ceilings, ignored controls, aliases and compatible-versus-RPC distinctions |

### Independent operations and future design inputs

| Note | Retained evidence |
|---|---|
| [Multimodal carriers, 2026-10-12 onward](refs/2026-10-12-multimodal-carriers.md) | User/tool-result placement, credential-bound files and generated-media carriers |
| [Embedding and Images, 2026-10-22/23](refs/2026-10-22-embedding-image-contracts.md) | REST/SDK/protobuf discrepancies, numeric encoding, bounded Embedding characterization and unresolved Generate/Edit/SSE contracts; not blanket SDK/live acceptance |
| [Grok Imagine, 2026-10-22](refs/2026-10-22-grok-imagine.md) | Image-quality/schema differences, video polling and unverified entitlement |
| [MiniMax H3 video, 2026-10-22](refs/2026-10-22-minimax-h3-video.md) | v2 task/content/error/callback protocol versus incompatible v1 file-ID envelope |
| [Decision APIs, 2026-10-23](refs/2026-10-23-decision-contracts.md) | OpenAI/TypeSafe and alternative API differences, confidence/grouping/media and discovery/compression gaps |
| [Downstream OAuth, 2026-10-23](refs/2026-10-23-downstream-oauth.md) | Device Flow, Rust authorization-server comparisons, refresh/revocation and deployment boundaries |

Research is not design approval or a new work queue. Recheck mutable sources before a new protocol claim; keep SDK shape, synthetic evidence, installed-client behavior and live Provider observations distinct.

## Protocol docs

### OpenAI

- [Create response](https://developers.openai.com/api/reference/resources/responses/methods/create)
- [Delete response](https://developers.openai.com/api/reference/resources/responses/methods/delete)
- [Streaming events](https://developers.openai.com/api/reference/resources/responses/streaming-events)
- [Reasoning](https://developers.openai.com/api/docs/guides/reasoning)
- [Compaction](https://developers.openai.com/api/docs/guides/compaction)
- [Structured Outputs](https://developers.openai.com/api/docs/guides/structured-outputs)
- [Migrate to Responses](https://developers.openai.com/api/docs/guides/migrate-to-responses)
- [File inputs](https://developers.openai.com/api/docs/guides/file-inputs)
- [Images and vision](https://developers.openai.com/api/docs/guides/images-vision)

### Anthropic

- [Messages](https://platform.claude.com/docs/en/api/messages)
- [Streaming](https://platform.claude.com/docs/en/build-with-claude/streaming)
- [Thinking](https://platform.claude.com/docs/en/build-with-claude/thinking)
- [Preserved thinking](https://platform.claude.com/docs/en/build-with-claude/preserved-thinking)
- [Effort](https://platform.claude.com/docs/en/build-with-claude/effort)
- [Refusals and fallback](https://platform.claude.com/docs/en/build-with-claude/refusals-and-fallback)
- [Tool calls](https://platform.claude.com/docs/en/agents-and-tools/tool-use/handle-tool-calls)
- [Stop reasons](https://platform.claude.com/docs/en/build-with-claude/handling-stop-reasons)
- [Prompt caching](https://platform.claude.com/docs/en/build-with-claude/prompt-caching)
- [Mid-conversation system messages](https://platform.claude.com/docs/en/build-with-claude/mid-conversation-system-messages)
- [Vision](https://platform.claude.com/docs/en/build-with-claude/vision)
- [PDF support](https://platform.claude.com/docs/en/build-with-claude/pdf-support)
- [Files API](https://platform.claude.com/docs/en/build-with-claude/files)

### Google Gemini

Documentation: CC-BY-4.0. Samples: Apache-2.0.

- [generateContent](https://ai.google.dev/api/generate-content)
- [CachedContent resources](https://ai.google.dev/api/caching)
- [Context caching](https://ai.google.dev/gemini-api/docs/caching)
- [Thought signatures](https://ai.google.dev/gemini-api/docs/generate-content/thought-signatures)
- [Function calling](https://ai.google.dev/gemini-api/docs/generate-content/function-calling)
- [Interactions overview](https://ai.google.dev/gemini-api/docs/interactions-overview)
- [Interactions API v1](https://ai.google.dev/api/interactions-api-v1)
- [Interactions streaming](https://ai.google.dev/gemini-api/docs/interactions/streaming)
- [Thinking](https://ai.google.dev/gemini-api/docs/thinking)
- [File input methods](https://ai.google.dev/gemini-api/docs/file-input-methods)
- [Video understanding](https://ai.google.dev/gemini-api/docs/video-understanding)

### OpenRouter

Source for Chat reasoning and video extension shapes.

- [Reasoning tokens](https://openrouter.ai/docs/reasoning-tokens)
- [Video inputs](https://openrouter.ai/docs/guides/overview/multimodal/videos)
- [Prompt caching](https://openrouter.ai/docs/guides/best-practices/prompt-caching)

### OpenCode Go

Endpoints, session requirements and terms of use.

- [Go documentation](https://opencode.ai/docs/go/)

### JSON Schema

- [2020-12 Core](https://json-schema.org/draft/2020-12/json-schema-core)

## OAuth and OIDC

- [RFC 6749](https://www.rfc-editor.org/rfc/rfc6749.html)
- [RFC 7636 PKCE](https://www.rfc-editor.org/rfc/rfc7636.html)
- [RFC 8252 native apps](https://www.rfc-editor.org/rfc/rfc8252.html)
- [RFC 8628 device grant](https://www.rfc-editor.org/rfc/rfc8628.html)
- [RFC 9700 security BCP](https://www.rfc-editor.org/rfc/rfc9700.html)
- [RFC 7009 revocation](https://www.rfc-editor.org/rfc/rfc7009.html)
- [OpenID Connect Core](https://openid.net/specs/openid-connect-core-1_0.html)

Product-specific clients, scopes, endpoints and terms (Grok, ChatGPT SIWC) are cited in the driver comments under `src/credential/`. Confirm OpenAI's terms before any remote or persistent SIWC deployment.
