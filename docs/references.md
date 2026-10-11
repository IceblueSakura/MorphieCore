# References

Primary sources used in design, with pinned revisions and licenses. Web pages are unversioned; recheck them when implementing the protocol. A pinned revision does not mean it is installed or verified locally.

## Pinned source

| Source | Revision | License | Used for |
|---|---|---|---|
| OpenAI Python SDK | [`be9d666`](https://github.com/openai/openai-python/tree/be9d66628ad7377bd36fe5a76ae6d735843f0e76) (3.19.0) | Apache-2.0 | Required, nullable and union checks for Responses, Chat and Embeddings types |
| OpenAI Codex | [`a69d757`](https://github.com/openai/codex/tree/a69d757cd8ef8310001186865911b69e4b4175e5) | Apache-2.0 | Codex/SIWC product profile (not a public API) |
| Anthropic Python SDK | [`b4b7916`](https://github.com/anthropics/anthropic-sdk-python/tree/b4b7916deaf4e5570cd627b1ae7ee4394a4de39f) (1.13.0) | MIT | Message, event, thinking and tool types |
| pi | [`d1230ea`](https://github.com/earendil-works/pi/tree/d1230ea2000d876b479a69b8b061f9d670f262f5) | MIT | Tool constraints and execution layering, history transforms (`transform-messages.ts`), Anthropic/Responses codecs |
| OpenCode | [`055d95b`](https://github.com/anomalyco/opencode/tree/055d95bb7e278c94baf06235a52cac79dd13ba67) | MIT | Tool error feedback, history projection, Go Messages routing (`x-api-key`, `anthropic-version: 2023-06-01`) |
| Magpie | [`5104263`](https://github.com/yetone/magpie/tree/51042630ecf44ab639cece917f7da7923d79fbf4) | MIT | Consumer stream counterexamples, publication before content, cache breakpoints, failure scope, cooldown and target affinity; design reference, not ported |
| new-api / RelayKit | [`1d4328e`](https://github.com/QuantumNous/new-api/tree/1d4328e97417a043a161a0dd30a5b129be3ace49/relaykit) | **AGPL-3.0** | Field-mapping ideas only; never port code into this MIT project |

The SDK loopback gate uses the versions locked in `tests/sdk/uv.lock`, independent of the research revisions above.

## Protocol docs

**OpenAI:** [Create response](https://developers.openai.com/api/reference/resources/responses/methods/create), [Streaming events](https://developers.openai.com/api/reference/resources/responses/streaming-events), [Reasoning](https://developers.openai.com/api/docs/guides/reasoning), [Structured Outputs](https://developers.openai.com/api/docs/guides/structured-outputs), [Migrate to Responses](https://developers.openai.com/api/docs/guides/migrate-to-responses).

**Anthropic:** [Messages](https://platform.claude.com/docs/en/api/messages), [Streaming](https://platform.claude.com/docs/en/build-with-claude/streaming), [Thinking](https://platform.claude.com/docs/en/build-with-claude/thinking), [Preserved thinking](https://platform.claude.com/docs/en/build-with-claude/preserved-thinking), [Effort](https://platform.claude.com/docs/en/build-with-claude/effort), [Refusals and fallback](https://platform.claude.com/docs/en/build-with-claude/refusals-and-fallback), [Tool calls](https://platform.claude.com/docs/en/agents-and-tools/tool-use/handle-tool-calls), [Stop reasons](https://platform.claude.com/docs/en/build-with-claude/handling-stop-reasons), [Prompt caching](https://platform.claude.com/docs/en/build-with-claude/prompt-caching).

**Google Gemini** (docs CC-BY-4.0, samples Apache-2.0): [generateContent](https://ai.google.dev/api/generate-content), [Thought signatures](https://ai.google.dev/gemini-api/docs/generate-content/thought-signatures), [Function calling](https://ai.google.dev/gemini-api/docs/generate-content/function-calling), [Interactions overview](https://ai.google.dev/gemini-api/docs/interactions-overview), [Interactions API v1](https://ai.google.dev/api/interactions-api-v1), [Interactions streaming](https://ai.google.dev/gemini-api/docs/interactions/streaming), [Thinking](https://ai.google.dev/gemini-api/docs/thinking).

**OpenRouter** (Chat extension shapes): [Reasoning tokens](https://openrouter.ai/docs/reasoning-tokens), [Prompt caching](https://openrouter.ai/docs/guides/best-practices/prompt-caching).

**OpenCode Go:** [docs](https://opencode.ai/docs/go/) (endpoints, session requirements, terms of use).

**JSON Schema:** [2020-12 Core](https://json-schema.org/draft/2020-12/json-schema-core).

## OAuth / OIDC

[RFC 6749](https://www.rfc-editor.org/rfc/rfc6749.html), [RFC 7636 PKCE](https://www.rfc-editor.org/rfc/rfc7636.html), [RFC 8252 native apps](https://www.rfc-editor.org/rfc/rfc8252.html), [RFC 8628 device grant](https://www.rfc-editor.org/rfc/rfc8628.html), [RFC 9700 security BCP](https://www.rfc-editor.org/rfc/rfc9700.html), [RFC 7009 revocation](https://www.rfc-editor.org/rfc/rfc7009.html), [OpenID Connect Core](https://openid.net/specs/openid-connect-core-1_0.html).

Product-specific clients, scopes, endpoints and terms (Grok, ChatGPT SIWC) are cited in the driver comments under `src/credential/`. Confirm OpenAI's terms before any remote or persistent SIWC deployment.
