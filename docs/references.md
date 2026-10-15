# References

Primary sources used in design, with pinned revisions and licenses. Web pages are unversioned; recheck them when implementing the protocol. A pinned revision does not mean it is installed or verified locally.

## Pinned sources

| Source | Revision | License | Used for |
|---|---|---|---|
| OpenAI Python SDK | [`be9d666`](https://github.com/openai/openai-python/tree/be9d66628ad7377bd36fe5a76ae6d735843f0e76) (3.19.0) | Apache-2.0 | Required, nullable and union checks for Responses, Chat and Embeddings types; Responses continuation and input-item pagination |
| OpenAI Codex | [`a9b0815`](https://github.com/openai/codex/tree/a9b0815167c27ab13719dd12d13b63e02b70d221) | Apache-2.0 | Codex/SIWC product profile (not a public API); target agent: request shape, session headers, `response.failed` error codes |
| Anthropic Python SDK | [`b4b7916`](https://github.com/anthropics/anthropic-sdk-python/tree/b4b7916deaf4e5570cd627b1ae7ee4394a4de39f) (1.13.0) | MIT | Message, event, thinking and tool types |
| Google Gen AI Python SDK | [`78f6d34`](https://github.com/googleapis/python-genai/tree/78f6d3412caaaed44f99ce57be2d16d0b8c407cb) | Apache-2.0 | generateContent thinking types and Interactions v1 generation controls; deprecated SDK fields remain distinct from effective model support |
| pi | [`4270522`](https://github.com/earendil-works/pi/tree/4270522473e677ddba7b093caa0064a3df51d459) | MIT | Tool constraints and execution layering, history transforms (`transform-messages.ts`), Anthropic/Responses codecs and [thinking-budget presets](refs/2026-10-15-generation-projections.md#pi-thinking-budget-presets) |
| OpenCode | [`e5a557e`](https://github.com/anomalyco/opencode/tree/e5a557efeee02c94486d391afc48f24a49ebc9a5) | MIT | Tool error feedback, history projection, Go Messages routing (`x-api-key`, `anthropic-version: 2023-06-01`) |
| Magpie | [`5104263`](https://github.com/yetone/magpie/tree/51042630ecf44ab639cece917f7da7923d79fbf4) | MIT | Consumer stream counterexamples, publication before content, cache breakpoints, failure scope, cooldown and target affinity; design reference, not ported |
| new-api / RelayKit | [`1d4328e`](https://github.com/QuantumNous/new-api/tree/1d4328e97417a043a161a0dd30a5b129be3ace49/relaykit) | **AGPL-3.0** | Field-mapping ideas only; never port code into this MIT project |

The SDK loopback gate uses the versions locked in `tests/sdk/uv.lock`, independent of the research revisions above.

## Protocol docs

Protocol research notes with dated findings live in `refs/`: [Multimodal carriers](refs/multimodal.md) and [Generation projection contracts](refs/2026-10-15-generation-projections.md).

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
- [Vision](https://platform.claude.com/docs/en/build-with-claude/vision)
- [PDF support](https://platform.claude.com/docs/en/build-with-claude/pdf-support)
- [Files API](https://platform.claude.com/docs/en/build-with-claude/files)

### Google Gemini

Documentation: CC-BY-4.0. Samples: Apache-2.0.

- [generateContent](https://ai.google.dev/api/generate-content)
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
