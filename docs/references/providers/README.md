# Provider 官方来源入口

这里只保存检索入口和必要出处，**不是 MorphieCore 支持列表、API 快照或测试记录**。当前绑定、协议准入和实例启用情况按 [AGENTS.md 的查询方法](../../../AGENTS.md#current-provider-model-and-compatibility-information)从代码现场确认；上游字段、模型、地域、价格和可用性按任务需要重新查阅官方来源。

链接存在不证明 API 合同未变。网页和 Models 目录也不证明账号权限或生成请求成功；凭据查询、真实调用和付费验证仍需独立授权。本索引不声明外部页面已重新核验。下列资料属于各自发布方；这里只提供链接，不复制其实现或测试数据。

## Google Gemini

- [GenerateContent reference](https://ai.google.dev/api/generate-content)
- [Interactions overview](https://ai.google.dev/gemini-api/docs/interactions-overview)、[API reference](https://ai.google.dev/api/interactions-api)、[v1 reference](https://ai.google.dev/api/interactions-api-v1)
- [GenerateContent thought signatures](https://ai.google.dev/gemini-api/docs/generate-content/thought-signatures)、[function calling](https://ai.google.dev/gemini-api/docs/generate-content/function-calling)
- [Interactions thinking](https://ai.google.dev/gemini-api/docs/thinking)、[streaming](https://ai.google.dev/gemini-api/docs/streaming)
- [Context caching](https://ai.google.dev/gemini-api/docs/generate-content/caching)、[structured outputs](https://ai.google.dev/gemini-api/docs/generate-content/structured-output)
- [OpenAI compatibility](https://ai.google.dev/gemini-api/docs/openai)

GenerateContent、Interactions 和 OpenAI-compatible 接口是不同合同；选择具体 API 版本后分别核对，不混用 guide 示例或推定本地已接入。Vertex AI 的资源与认证边界需另行核对。

## ModelBest

- [Hosted Chat API guide](https://github.com/OpenBMB/MiniCPM-V/blob/main/docs/api.md)
- [MiniCPM model and local inference sources](https://github.com/OpenBMB/MiniCPM)
- [Operator platform](https://platform.modelbest.cn)

Hosted API examples, local model deployment and authenticated model discovery are distinct sources. Example model spelling is not an automatic alias; precise upstream IDs and admission belong to the current catalogs. OpenAI compatibility does not imply a native Responses operation, complete media union or verified inference access.

## Anthropic

- [Messages reference](https://platform.claude.com/docs/en/api/messages)、[streaming](https://platform.claude.com/docs/en/build-with-claude/streaming)
- [Thinking](https://platform.claude.com/docs/en/build-with-claude/thinking)、[tool/multi-turn workflows](https://platform.claude.com/docs/en/build-with-claude/thinking-tool-workflows)
- [Tool calls/results](https://platform.claude.com/docs/en/agents-and-tools/tool-use/handle-tool-calls)、[server tools](https://platform.claude.com/docs/en/agents-and-tools/tool-use/server-tools)、[fine-grained tool streaming](https://platform.claude.com/docs/en/agents-and-tools/tool-use/fine-grained-tool-streaming)
- [Stop reasons](https://platform.claude.com/docs/en/build-with-claude/handling-stop-reasons)、[prompt caching](https://platform.claude.com/docs/en/build-with-claude/prompt-caching)
- [Citations](https://platform.claude.com/docs/en/build-with-claude/citations)、[structured outputs](https://platform.claude.com/docs/en/build-with-claude/structured-outputs)、[compaction](https://platform.claude.com/docs/en/build-with-claude/compaction-threshold)

Messages 基础合同、beta 能力及各云平台的 wrapper/资源合同分别固定；SDK 自动工具循环或恢复建议不构成网关执行授权。

## Alibaba Cloud Model Studio

- [OpenAI-compatible Chat](https://help.aliyun.com/zh/model-studio/qwen-api-via-openai-chat-completions)
- [OpenAI-compatible Responses](https://help.aliyun.com/zh/model-studio/qwen-api-via-openai-responses)
- [Structured output](https://help.aliyun.com/zh/model-studio/qwen-structured-output)
- [DashScope API](https://help.aliyun.com/zh/model-studio/developer-reference/use-qwen-by-calling-api)
- [Token Plan 个人版](https://help.aliyun.com/en/model-studio/token-plan-personal-overview)、[多模态生成接入](https://help.aliyun.com/en/model-studio/token-plan-multimodal-gen)：套餐资格与各 operation 的 wire 分开确认，不由对话 OpenAI 兼容入口推定 Speech 兼容。

## DeepSeek

- [Chat Completions](https://api-docs.deepseek.com/api/create-chat-completion)
- [Responses](https://api-docs.deepseek.com/guides/responses_api/)
- [JSON output](https://api-docs.deepseek.com/guides/json_mode/)、[function calling](https://api-docs.deepseek.com/guides/function_calling/)、[strict tools](https://api-docs.deepseek.com/guides/tool_calls/)
- [Thinking](https://api-docs.deepseek.com/guides/thinking_mode)、[vision](https://api-docs.deepseek.com/guides/vision)

## Kimi

- [API overview](https://platform.kimi.com/docs/api/overview)、[Chat](https://platform.kimi.com/docs/api/chat)
- [Models and parameters](https://platform.kimi.com/docs/api/models-overview)

## LongCat

- [Quick start](https://longcat.chat/platform/docs/)
- [Chat reference](https://longcat.chat/platform/docs/api/chat.html)
- [Codex configuration](https://longcat.chat/platform/docs/Codex.html)

## NVIDIA

- [NIM LLM API reference](https://docs.nvidia.com/nim/large-language-models/latest/api-reference.html)
- [API Catalog](https://build.nvidia.com/)
- [Authentication](https://docs.nvidia.com/nemo/retriever/26.5.0/extraction/api-keys)

## OpenCode Go

- [Go usage policy, client/session requirements and model-specific endpoints](https://opencode.ai/docs/go/)
- [OpenAI-compatible adapter package](https://ai-sdk.dev/providers/openai-compatible-providers)

## OpenRouter

- 独立 [TTS guide](https://openrouter.ai/docs/guides/overview/multimodal/tts)、[Create speech](https://openrouter.ai/docs/api/api-reference/tts/create-speech.md)与[公开 Speech 模型导航](https://openrouter.ai/api/v1/models?output_modalities=speech)：按模型详情链接核对 endpoint，不能把文档示例当作当前产品注册；与标准 Speech 的默认值、控制位置及格式差异须由所选 profile 明确处理。
- 独立 [Images guide](https://openrouter.ai/docs/guides/overview/multimodal/image-generation.md)、[Generate an image](https://openrouter.ai/docs/api/api-reference/images/generate-an-image.md)：采用其中 OpenAPI 3.1.0 / API `1.0.0` 的 `POST /images` 静态合同；该 schema 声明 MIT 许可。它与 Chat image carrier、OpenAI `/images/generations` 的路径、请求 null 载体和计量结构不同；具体具名映射归 [Images codec](../../../src/protocol/openrouter_images.rs)，不复制动态能力或定价。
- [Image models](https://openrouter.ai/api/v1/images/models) 提供专用 per-endpoint 导航；采用参数按具体 endpoint 核对，不把模型层 union 或通用 Models 参数当作 Images 准入。

- [Chat](https://openrouter.ai/docs/api/api-reference/chat/send-chat-completion-request)、[Responses](https://openrouter.ai/docs/api/reference/responses/overview)
- [Models](https://openrouter.ai/docs/api/api-reference/models/get-models)、[model endpoints](https://openrouter.ai/docs/api/api-reference/models/get-endpoints-for-a-model)
- [API overview](https://openrouter.ai/docs/api_reference/overview)、[streaming](https://openrouter.ai/docs/api_reference/streaming)、[Responses reasoning](https://openrouter.ai/docs/api_reference/responses/reasoning)
- [Provider routing](https://openrouter.ai/docs/guides/routing/provider-selection)、[server tools](https://openrouter.ai/docs/guides/features/server-tools)、[web search](https://openrouter.ai/docs/guides/features/server-tools/web-search)
- [Plugins](https://openrouter.ai/docs/guides/features/plugins)、[router metadata](https://openrouter.ai/docs/guides/features/router-metadata)

## Xiaomi MiMo

- [Chat](https://mimo.mi.com/docs/zh-CN/api/chat/openai-api)、[Responses](https://mimo.mi.com/docs/zh-CN/api/chat/responses)、[Models](https://mimo.mi.com/docs/zh-CN/api/model/list-models)
- [Structured output](https://mimo.mi.com/docs/en-US/quick-start/usage-guide/text-generation/structured-output)
- [Images](https://mimo.mi.com/docs/zh-CN/quick-start/usage-guide/multimodal-understanding/image-understanding)
- [Audio understanding](https://mimo.mi.com/docs/en-US/quick-start/usage-guide/multimodal-understanding/audio-understanding)
- [Speech recognition](https://mimo.mi.com/docs/en-US/api/audio/Speech-Recognition)、[text-to-speech](https://mimo.mi.com/docs/en-US/api/audio/Text-to-Speech)
- [Voice design](https://mimo.mi.com/docs/en-US/api/audio/Voice-Design)、[voice clone](https://mimo.mi.com/docs/en-US/api/audio/Voice-Clone)

## Zhipu / Z.AI

- [OpenAI SDK compatibility](https://docs.bigmodel.cn/cn/guide/develop/openai/introduction)
- [Structured output](https://docs.bigmodel.cn/cn/guide/capabilities/struct-output)
