# References

This page indexes primary sources, licenses and dated research notes. It does not select design or certify implementation: [Architecture](architecture.md) owns accepted contracts and [Roadmap](roadmap.md) owns current scope and acceptance. Web pages are unversioned; recheck them when implementing the protocol. A pinned revision does not mean it is installed or verified locally.

## Contents

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
- [OAuth / OIDC](#oauth--oidc)

## Pinned sources

### Current stable release selection

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

OpenCode's selected release was cross-checked with npm `opencode-ai`. The pi source is the existing earendil-works fork, not the unrelated `@mariozechner/pi-coding-agent` release line. Do not replace the intended client by matching a package name alone.

The [stable-release protocol recheck](refs/2026-10-23-stable-protocol-contracts.md) records material source changes and disagreements. These selections supersede the earlier mapping baseline for the [accepted reset](roadmap.md#standards-reset-and-redesign), without retroactively changing dated evidence. The [2026-10-24 candidate review](refs/2026-10-23-stable-protocol-contracts.md#latest-candidate-review-2026-10-24) identifies newer Anthropic Python/TypeScript and Vercel core releases; the table is the verified baseline, not a claim of today's latest versions.

### Telemetry dependency selection

Resolved from crates.io release metadata and published package `.cargo_vcs_info.json` on **2026-10-24**. Exact package versions/checksums live in `Cargo.toml`/`Cargo.lock`; the same tracing/SDK pins enter local production wiring on **2026-10-25**, while OTLP export and metrics feasibility remain test-only. These are not new protocol assets in `tests/upstream/`.

| Packages | Selected release / source revision | License |
|---|---|---|
| `tracing` | 0.1.44 / [`2d55f6f`](https://github.com/tokio-rs/tracing/tree/2d55f6faf9be83e7e4634129fb96813241aac2b8/tracing) | MIT |
| `tracing-subscriber` | 0.3.23 / [`54ede4d`](https://github.com/tokio-rs/tracing/tree/54ede4d5d85a536aed5485c5213011d9ec961935/tracing-subscriber) | MIT |
| `tracing-opentelemetry` | 0.34.0 / [`8c1a02d`](https://github.com/tokio-rs/tracing-opentelemetry/tree/8c1a02d68034de317607ada0b1dd0d98abefd6cd) | MIT |
| `opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp`, `opentelemetry-appender-tracing` | 0.33.1 / [`372ebbc`](https://github.com/open-telemetry/opentelemetry-rust/tree/372ebbc74d4dba71bb9820732f57f7316b4e7a1e) | Apache-2.0 |

The [0.33.1 release](https://github.com/open-telemetry/opentelemetry-rust/releases/tag/opentelemetry-0.33.1) stabilizes Logs/Metrics OTLP exporters; Traces remain Beta. Use published dependencies and versioned source over stale overview/examples: tracing bridge 0.34 accepts OTel 0.33, and OTLP's reqwest 0.13 range resolves to the project's existing client.

Primary contract entry points: [OTLP HTTP/runtime/configuration](https://docs.rs/opentelemetry-otlp/0.33.1/opentelemetry_otlp/), [log bridge mapping](https://docs.rs/opentelemetry-appender-tracing/0.33.1/opentelemetry_appender_tracing/), [trace bridge configuration](https://docs.rs/tracing-opentelemetry/0.34.0/tracing_opentelemetry/struct.OpenTelemetryLayer.html), and [batch log processor source](https://github.com/open-telemetry/opentelemetry-rust/blob/372ebbc74d4dba71bb9820732f57f7316b4e7a1e/opentelemetry-sdk/src/logs/batch_log_processor.rs). [Development](development.md#telemetry-ecosystem-checks) owns executable evidence and limits.

The separately reviewed [tracing-appender 0.2.5](https://docs.rs/tracing-appender/0.2.5/tracing_appender/) is not a drop-in replacement for private size-bounded, locked, tail-repaired storage. [tower-http 0.7.1 tracing](https://docs.rs/tower-http/0.7.1/tower_http/trace/) offers HTTP/body polling hooks, not Generation terminal or body-drop authority. Neither is added by this slice; production file storage and gateway lifecycle retain their current owners.

### Historical evidence

The following revisions explain prior decisions and observations. They are not active dependency pins or normative regression expectations for the redesign.

| Source | Revision | License | Used for |
|---|---|---|---|
| OpenAI Python SDK | [`547ecb1`](https://github.com/openai/openai-python/tree/547ecb1e4fc233f19030736c29fee4374dfbaca1) (3.35.1) | Apache-2.0 | Historical stage-1 baseline, checked on 2026-10-19: required, nullable and union checks for Responses, Chat and Embeddings types; Responses continuation and input-item pagination. Current SDK pins are listed above |
| OpenAI Codex | [`a9b0815`](https://github.com/openai/codex/tree/a9b0815167c27ab13719dd12d13b63e02b70d221) | Apache-2.0 | Downstream-agent reference: client request shape, session identity and failures, not support for the legacy Codex backend. Not authority for the separate public SIWC endpoint or its authentication |
| Anthropic Python SDK | [`bcd55fe`](https://github.com/anthropics/anthropic-sdk-python/tree/bcd55fe1d3f0f1240765f8cef04295b67337f55a) (1.15.0) | MIT | Stage-1 baseline, latest release checked on 2026-10-19: message, event, thinking and tool types, including beta mid-conversation, tool-change and block-binding types |
| Google Gen AI Python SDK | [`78f6d34`](https://github.com/googleapis/python-genai/tree/78f6d3412caaaed44f99ce57be2d16d0b8c407cb) | Apache-2.0 | generateContent thinking, cached-context and probability-safety types, plus Interactions v1 generation controls; deprecated SDK fields remain distinct from effective model support |
| Google API protobuf definitions | [`8cc2927`](https://github.com/googleapis/googleapis/tree/8cc2927c2ea64e6876568683050deabf2b52ce56/google/ai/generativelanguage/v1beta) | Apache-2.0 | Safety rating scalar defaults, per-category uniqueness and blocked-prompt/candidate separation |
| Google Interactions OpenAPI | [v1](https://ai.google.dev/static/api/interactions-v1.openapi.json) / [v1beta](https://ai.google.dev/static/api/interactions.openapi.json), read 2026-10-21 | See Google documentation notices | Supplemental service-field evidence. Mutable URLs; observation date and SHA-256 fingerprints are recorded in the [source recheck](refs/2026-10-17-generation-contracts.md#google-text-report-source-recheck-2026-10-21), not a new live acceptance baseline |
| pi client baseline | [`71023ae`](https://github.com/earendil-works/pi/tree/71023aec1546d6d677a65669c5665c0927ce0dd0) (v1.3.0) | MIT | Stage-1 client oracle and the installed binary on 2026-10-19, including the built-in `anthropic` Claude Haiku 5.5 profile; history probe source. Production-profile compaction, strictness and retry paths rechecked on 2026-10-21, without new runtime acceptance |
| pi | [`4270522`](https://github.com/earendil-works/pi/tree/4270522473e677ddba7b093caa0064a3df51d459) | MIT | Tool constraints and execution layering, history transforms (`transform-messages.ts`), Anthropic/Responses codecs and [thinking-budget presets](refs/2026-10-15-generation-projections.md#pi-thinking-budget-presets) |
| pi thinking allocation | [`4a5d372`](https://github.com/earendil-works/pi/tree/4a5d37291531ad48b8e536c892557877d219463c) | MIT | Current thinking-budget helpers and target-specific budget branches, rechecked for P3; does not replace the broader client pin above |
| pi visible-history transfer | [`8efd314`](https://github.com/earendil-works/pi/tree/8efd314fa3d3232ce197d8d7a6a5660530b9d83e) | MIT | `transform-messages.ts`, active-tool-turn regression, Responses pairing-ID handling and unsigned Google history; synthetic local probe, not real Provider acceptance |
| OpenCode | [`e5a557e`](https://github.com/anomalyco/opencode/tree/e5a557efeee02c94486d391afc48f24a49ebc9a5) | MIT | Tool error feedback, history projection, Go Messages routing (`x-api-key`, `anthropic-version: 2023-06-01`); production-profile compaction/retry source rechecked on 2026-10-21, not a new installed-binary claim |
| Installed OpenCode Responses serializer | [`e5ecb57`](https://github.com/anomalyco/opencode/blob/e5ecb57/packages/ai/src/protocols/open-responses.ts) (2.0.6) | MIT | Checked on 2026-10-20: assistant history retains item ID/phase but emits `output_text` without `annotations`; a separate input-compatibility decision, not output-schema authority |
| Magpie | [`5104263`](https://github.com/yetone/magpie/tree/51042630ecf44ab639cece917f7da7923d79fbf4) | MIT | Consumer stream counterexamples, publication before content, cache breakpoints, failure scope, cooldown and target affinity; design reference, not ported |
| new-api / RelayKit | [`1d4328e`](https://github.com/QuantumNous/new-api/tree/1d4328e97417a043a161a0dd30a5b129be3ace49/relaykit) | **AGPL-3.0** | Field-mapping ideas only; never port code into this MIT project |

The SDK loopback gate uses `tests/sdk/uv.lock`; its current release selection is above. Passing legacy scenarios with new SDKs does not accept the new protocol design.

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

| Domain | Note | Evidence boundary |
|---|---|---|
| Stable-release protocol reset | [Stable protocol contracts](refs/2026-10-23-stable-protocol-contracts.md) | Latest release/API migration evidence, standard-vs-SDK differences and new test entry points; checked 2026-10-23. Not a conformance certificate |
| Instruction authority and role migration | [Current, legacy and Provider roles](refs/2026-10-23-stable-protocol-contracts.md#instruction-authority-research-current-legacy-and-provider-dialects) | Additional 2026-10-25 official-source research separates application system/developer from vendor-reserved authority, Messages rollout/scope and DeepSeek/Alibaba/Xiaomi/OpenRouter dialect behavior; directional recommendations, not selected redesign or live acceptance |
| Provider instruction verification | [Vercel cross-check](refs/2026-10-23-stable-protocol-contracts.md#vercel-provider-instruction-cross-check-2026-10-25) | Pinned Vercel converters plus official operation schemas identify Zhipu/NVIDIA Chat carrier gaps, a Zhipu Responses discriminator gap and string-only/comparator/evidence boundaries; checked 2026-10-25 with synthetic preparation, not live acceptance or selected repairs |
| Decision APIs and SDKs | [Decision contracts](refs/2026-10-23-decision-contracts.md) | OpenAI/TypeSafe wire baselines, Perplexity/Cloudflare differences, confidence/grouping/media semantics and dual-SDK discovery/compression gaps; checked 2026-10-23. Mutable source research only, no pinned SDK acceptance or live inference |
| Downstream authorization | [OAuth2 and Rust authorization servers](refs/2026-10-23-downstream-oauth.md) | Device Flow fit, Rauthy/Kanidm/FerrisKey version evidence, SDK refresh and revocation constraints; checked 2026-10-23, no deployment or interoperability acceptance |
| Media and resource carriers | [Multimodal carriers](refs/multimodal.md) | Input/tool-result media, Provider-owned file references and generated-media sources; collected 2026-10-12 with dated updates |
| Embedding and standalone Images | [Embedding and Image contracts](refs/2026-10-22-embedding-image-contracts.md) | Official input/encoding/defaults, pinned SDK behavior, Google Discovery revision/fingerprint and SDK/protobuf discrepancies, ancillary compatibility, Edit and SSE owners; checked 2026-10-22. Static Generate defaults, report gaps and code seams rechecked 2026-10-23; design evidence only, no SDK or live acceptance |
| Grok Imagine images and video | [Imagine contracts](refs/2026-10-22-grok-imagine.md) | Official generation/polling shapes, image-quality retirement, lite capabilities and unresolved schema/OAuth evidence; checked 2026-10-22, no live generation acceptance |
| MiniMax H3 video | [H3 video contracts](refs/2026-10-22-minimax-h3-video.md) | Proprietary async task protocol (`/v2/video_generation` create, query, list, cancel), `content[]` roles, dual error carriers, callback challenge and the incompatible v1 file-ID envelope; checked 2026-10-22, no live generation acceptance |
| Generation controls and projection | [Generation projection contracts](refs/2026-10-15-generation-projections.md) | Protocol/dialect distinctions, reasoning, budgets, sampling and service controls; researched 2026-10-15 |
| Chat custom tools | [Custom-tool carriers](refs/2026-10-16-chat-custom-tools.md) | Native definition, call/result and missing SSE carriers; checked 2026-10-16 |
| Generation contracts and client behavior | [Contract boundaries](refs/2026-10-17-generation-contracts.md) | Dated SDK/source and binding evidence; [2026-10-24 current-host reacceptance](refs/2026-10-17-generation-contracts.md#current-host-reacceptance-2026-10-24) separates 12 synthetic Agent scenarios, native Provider examples and pi/OpenCode Token Plan Chat delivery/replay acceptance after the DashScope billing diagnosis. Raw upstream fidelity, full conformance, retention, latency SLOs and real failover remain unproven |
| Same-protocol field and event inventory | [Same-protocol Generation inventory](refs/2026-10-19-same-protocol-inventory.md) | Pinned Chat/Responses/Messages fields, shared Aliyun billing dialect, Grok/public SIWC directions, scoped-report exclusions and history; baseline checked 2026-10-19, reported text presence updated 2026-10-20 |
| Bailian compatible requests | [Bailian request subset](refs/2026-10-19-bailian-compatible-requests.md) | Official Chat/Responses parameter definitions, model-specific effort/caps, ignored-field refusals and existing-IR boundary; checked 2026-10-19. Native RPC is reference only |

Notes preserve the source revision and observation date, including superseded recommendations and narrower historical checks. Their conclusions are design inputs, not a second work queue or a replacement for current decisions. Recheck mutable sources before making a new protocol claim; distinguish SDK shape, client behavior, synthetic evidence and live Provider evidence.

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

## OAuth / OIDC

- [RFC 6749](https://www.rfc-editor.org/rfc/rfc6749.html)
- [RFC 7636 PKCE](https://www.rfc-editor.org/rfc/rfc7636.html)
- [RFC 8252 native apps](https://www.rfc-editor.org/rfc/rfc8252.html)
- [RFC 8628 device grant](https://www.rfc-editor.org/rfc/rfc8628.html)
- [RFC 9700 security BCP](https://www.rfc-editor.org/rfc/rfc9700.html)
- [RFC 7009 revocation](https://www.rfc-editor.org/rfc/rfc7009.html)
- [OpenID Connect Core](https://openid.net/specs/openid-connect-core-1_0.html)

Product-specific clients, scopes, endpoints and terms (Grok, ChatGPT SIWC) are cited in the driver comments under `src/credential/`. Confirm OpenAI's terms before any remote or persistent SIWC deployment.
