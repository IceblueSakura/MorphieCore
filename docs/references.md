# 参考来源

只记录设计用到的一手来源、固定版本与许可。动态网页没有固定版本，实施对应协议时重新核对；固定源码修订不代表本地已安装或已验证。

## 固定源码

| 来源 | 修订 | 许可 | 用途 |
|---|---|---|---|
| OpenAI Python SDK | [`be9d666`](https://github.com/openai/openai-python/tree/be9d66628ad7377bd36fe5a76ae6d735843f0e76)（3.19.0） | Apache-2.0 | Responses / Chat / Embeddings 类型的 required、nullable、union 核对 |
| OpenAI Codex | [`a69d757`](https://github.com/openai/codex/tree/a69d757cd8ef8310001186865911b69e4b4175e5) | Apache-2.0 | Codex/SIWC 产品 profile（非公共 API） |
| Anthropic Python SDK | [`b4b7916`](https://github.com/anthropics/anthropic-sdk-python/tree/b4b7916deaf4e5570cd627b1ae7ee4394a4de39f)（1.13.0） | MIT | Message、事件、thinking、工具类型核对 |
| pi | [`d1230ea`](https://github.com/earendil-works/pi/tree/d1230ea2000d876b479a69b8b061f9d670f262f5) | MIT | 工具约束与执行分层、历史转换（`transform-messages.ts`）、Anthropic/Responses codec |
| OpenCode | [`055d95b`](https://github.com/anomalyco/opencode/tree/055d95bb7e278c94baf06235a52cac79dd13ba67) | MIT | 工具错误反馈、历史投影、Go 的 Messages 路由（`x-api-key`、`anthropic-version: 2023-06-01`） |
| new-api / RelayKit | [`1d4328e`](https://github.com/QuantumNous/new-api/tree/1d4328e97417a043a161a0dd30a5b129be3ace49/relaykit) | **AGPL-3.0** | 协议转换的字段映射参考；只参考方案，不移植代码到本 MIT 项目 |

SDK loopback gate 实际使用的版本由 `tests/sdk/uv.lock` 决定，与上表研究用修订无关。

## 协议文档

**OpenAI**：[Create response](https://developers.openai.com/api/reference/resources/responses/methods/create)、[Streaming events](https://developers.openai.com/api/reference/resources/responses/streaming-events)、[Reasoning](https://developers.openai.com/api/docs/guides/reasoning)、[Structured Outputs](https://developers.openai.com/api/docs/guides/structured-outputs)、[迁移到 Responses](https://developers.openai.com/api/docs/guides/migrate-to-responses)。

**Anthropic**：[Messages](https://platform.claude.com/docs/en/api/messages)、[Streaming](https://platform.claude.com/docs/en/build-with-claude/streaming)、[Thinking](https://platform.claude.com/docs/en/build-with-claude/thinking)、[Preserved thinking](https://platform.claude.com/docs/en/build-with-claude/preserved-thinking)、[Tool calls](https://platform.claude.com/docs/en/agents-and-tools/tool-use/handle-tool-calls)、[Stop reasons](https://platform.claude.com/docs/en/build-with-claude/handling-stop-reasons)、[Prompt caching](https://platform.claude.com/docs/en/build-with-claude/prompt-caching)。

**Google Gemini**（文档 CC-BY-4.0，示例 Apache-2.0）：[generateContent](https://ai.google.dev/api/generate-content)、[Thought signatures](https://ai.google.dev/gemini-api/docs/generate-content/thought-signatures)、[Function calling](https://ai.google.dev/gemini-api/docs/generate-content/function-calling)、[Interactions overview](https://ai.google.dev/gemini-api/docs/interactions-overview)、[Interactions API v1](https://ai.google.dev/api/interactions-api-v1)、[Interactions streaming](https://ai.google.dev/gemini-api/docs/interactions/streaming)、[Thinking](https://ai.google.dev/gemini-api/docs/thinking)。

**OpenRouter**（Chat 扩展字段参照）：[Reasoning tokens](https://openrouter.ai/docs/reasoning-tokens)、[Prompt caching](https://openrouter.ai/docs/guides/best-practices/prompt-caching)。

**OpenCode Go**：[文档](https://opencode.ai/docs/go/)（端点、session 要求与使用条款）。

**JSON Schema**：[2020-12 Core](https://json-schema.org/draft/2020-12/json-schema-core)。

## OAuth / OIDC

[RFC 6749](https://www.rfc-editor.org/rfc/rfc6749.html)、[RFC 7636 PKCE](https://www.rfc-editor.org/rfc/rfc7636.html)、[RFC 8252 native apps](https://www.rfc-editor.org/rfc/rfc8252.html)、[RFC 8628 device grant](https://www.rfc-editor.org/rfc/rfc8628.html)、[RFC 9700 security BCP](https://www.rfc-editor.org/rfc/rfc9700.html)、[RFC 7009 revocation](https://www.rfc-editor.org/rfc/rfc7009.html)、[OpenID Connect Core](https://openid.net/specs/openid-connect-core-1_0.html)。

具体产品（Grok、ChatGPT SIWC）的 client、scope、endpoint 与使用条款以 `src/credential/` 中各 driver 的注释来源为准。SIWC 的远程托管或持久化部署需先确认 OpenAI 条款。
