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

| Source | Revision | License | Used for |
|---|---|---|---|
| OpenAI Python SDK | [`547ecb1`](https://github.com/openai/openai-python/tree/547ecb1e4fc233f19030736c29fee4374dfbaca1) (3.35.1) | Apache-2.0 | Stage-1 baseline, latest release checked on 2026-10-19: required, nullable and union checks for Responses, Chat and Embeddings types; Responses continuation and input-item pagination. The SDK loopback gate locks the same release |
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

The SDK loopback gate uses the versions locked in `tests/sdk/uv.lock`, independent of the research revisions above.

## Test source repositories

Selected on 2026-10-23 for [current text/SSE supplementation and later stage-4 acceptance](roadmap.md#test-supplementation). These repository links identify sources to track, not imported revisions or verified compatibility. [Development](development.md#upstream-test-sources) owns source linking, license/provenance checks and latest-candidate versus verified-baseline updates; existing pins above remain historical evidence.

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
| Decision APIs and SDKs | [Decision contracts](refs/2026-10-23-decision-contracts.md) | OpenAI/TypeSafe wire baselines, Perplexity/Cloudflare differences, confidence/grouping/media semantics and dual-SDK discovery/compression gaps; checked 2026-10-23. Mutable source research only, no pinned SDK acceptance or live inference |
| Downstream authorization | [OAuth2 and Rust authorization servers](refs/2026-10-23-downstream-oauth.md) | Device Flow fit, Rauthy/Kanidm/FerrisKey version evidence, SDK refresh and revocation constraints; checked 2026-10-23, no deployment or interoperability acceptance |
| Media and resource carriers | [Multimodal carriers](refs/multimodal.md) | Input/tool-result media, Provider-owned file references and generated-media sources; collected 2026-10-12 with dated updates |
| Embedding and standalone Images | [Embedding and Image contracts](refs/2026-10-22-embedding-image-contracts.md) | Official input/encoding/defaults, pinned SDK behavior, Google Discovery revision/fingerprint and SDK/protobuf discrepancies, ancillary compatibility, Edit and SSE owners; checked 2026-10-22. Static Generate defaults, report gaps and code seams rechecked 2026-10-23; design evidence only, no SDK or live acceptance |
| Grok Imagine images and video | [Imagine contracts](refs/2026-10-22-grok-imagine.md) | Official generation/polling shapes, image-quality retirement, lite capabilities and unresolved schema/OAuth evidence; checked 2026-10-22, no live generation acceptance |
| MiniMax H3 video | [H3 video contracts](refs/2026-10-22-minimax-h3-video.md) | Proprietary async task protocol (`/v2/video_generation` create, query, list, cancel), `content[]` roles, dual error carriers, callback challenge and the incompatible v1 file-ID envelope; checked 2026-10-22, no live generation acceptance |
| Generation controls and projection | [Generation projection contracts](refs/2026-10-15-generation-projections.md) | Protocol/dialect distinctions, reasoning, budgets, sampling and service controls; researched 2026-10-15 |
| Chat custom tools | [Custom-tool carriers](refs/2026-10-16-chat-custom-tools.md) | Native definition, call/result and missing SSE carriers; checked 2026-10-16 |
| Generation contracts and client behavior | [Contract boundaries](refs/2026-10-17-generation-contracts.md) | Dated SDK/source and binding evidence; 2026-10-22 adds Go/Haiku inputs/TTL/constrained profiles, controlled client lifecycle, bounded service/logging checks, separate live timing and a documentation-only model-enumeration recheck. Full conformance, retention, latency SLOs and real failover remain unproven |
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
