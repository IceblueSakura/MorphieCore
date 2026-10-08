# 固定上游来源

这里固定设计所使用的来源，不记录同步过程、历史差异或测试结果。官方页面的既有基线日期为 **2026-09-23**；动态网页没有永久版本，本地文档整理不刷新该日期，也不代表重新核验上游。

## 版本与许可

| 来源 | 固定版本 | 用途与边界 |
|---|---|---|
| OpenAI Python SDK | [`be9d66628ad7377bd36fe5a76ae6d735843f0e76`](https://github.com/openai/openai-python/tree/be9d66628ad7377bd36fe5a76ae6d735843f0e76)，`3.19.0`，[Apache-2.0](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/LICENSE) | 交叉核对类型、required/nullable/union；不是运行验证 |
| Codex 既有基线 | [`a69d757cd8ef8310001186865911b69e4b4175e5`](https://github.com/openai/codex/tree/a69d757cd8ef8310001186865911b69e4b4175e5)，[Apache-2.0](https://github.com/openai/codex/blob/a69d757cd8ef8310001186865911b69e4b4175e5/LICENSE) | 既有语义/codec 来源，不是 OpenAI 公共 API；新的登录与客户端上下文来源分别由 [ChatGPT 参考](chatgpt-login.md)和 [扩展与上下文](extensions-and-context.md)固定，不由专项参考隐式升级本基线 |
| 本地消费者 gate | [pyproject.toml](../../tests/sdk/pyproject.toml) 与 [uv.lock](../../tests/sdk/uv.lock) | 可执行依赖由锁文件拥有，不由研究版本隐式升级；命令见[开发指南](../development.md#固定-openai-sdk-loopback) |

## 一手入口

- [Create a response](https://developers.openai.com/api/reference/resources/responses/methods/create)
- [Streaming events](https://developers.openai.com/api/reference/resources/responses/streaming-events)
- [Reasoning guide](https://developers.openai.com/api/docs/guides/reasoning)
- [Structured Outputs](https://developers.openai.com/api/docs/guides/structured-outputs)
- [WebSocket mode](https://developers.openai.com/api/docs/guides/websocket-mode)
- SDK [Responses types](https://github.com/openai/openai-python/tree/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses)：分别核对 create params、input/output item、response snapshot 与 stream event，不能用 input 简写放宽完整响应。
- SDK [Chat types](https://github.com/openai/openai-python/tree/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/chat)、[usage](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/completion_usage.py)、[reasoning](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/shared/reasoning.py)。
- Codex [client](https://github.com/openai/codex/blob/a69d757cd8ef8310001186865911b69e4b4175e5/codex-rs/core/src/client.rs)、[responses metadata](https://github.com/openai/codex/blob/a69d757cd8ef8310001186865911b69e4b4175e5/codex-rs/core/src/responses_metadata.rs)、[Responses endpoint](https://github.com/openai/codex/blob/a69d757cd8ef8310001186865911b69e4b4175e5/codex-rs/codex-api/src/endpoint/responses.rs)、[headers](https://github.com/openai/codex/blob/a69d757cd8ef8310001186865911b69e4b4175e5/codex-rs/codex-api/src/requests/headers.rs)、[items](https://github.com/openai/codex/blob/a69d757cd8ef8310001186865911b69e4b4175e5/codex-rs/protocol/src/models.rs)。

## 使用规则

公开 reference/guide 定义目标语义，SDK 用于交叉核对。Open Responses、Codex tolerant parser 和其他网关的兼容策略不是官方标准替代品。冲突须在受影响合同或 owning code 中明确分类，不默默选取更方便实现的一份。

新增子域时固定相关 schema/SDK/profile，并分别确认 request、response、event 和生命周期，不能从事件名猜出完整任务。按需保留必要来源和许可，不保存页面抽取日志、hash 清单或历史分析。字段准入看当前代码；Provider 可用性与 SDK/Agent 执行需要另外验证。

<a id="client-managed-s2"></a>
<a id="reasoning-schema-sources"></a>
## 推理控制与 Schema 设计证据

本节固定 `s2-controls-2026-10-07` 的概念依据，查阅日期为 **2026-10-07**；不升级上方 OpenAI 基线，也不声明 Google/Anthropic codec 或实例准入。抽象 fixture 不冒充原生响应，具体接线时仍须固定对应 SDK/schema 修订。

| 来源与分支 | 约束的语义区别 |
|---|---|
| [Interactions API v1](https://ai.google.dev/api/interactions-api-v1) 的 generation config；[thinking](https://ai.google.dev/gemini-api/docs/thinking) | thinking level 与 summary 显示分开；总输出 token 上限包含思考，不把 effort 当硬 token 保证。不是 generateContent Part 的引用/附件合同 |
| Anthropic Messages 的 [adaptive/steering](https://platform.claude.com/docs/en/build-with-claude/adaptive-thinking) 与 [manual extended thinking](https://platform.claude.com/docs/en/build-with-claude/extended-thinking) 分支 | adaptive 可以与 effort 组合；manual `budget_tokens` 是目标而非严格上限，不能伪装硬约束。显示 omitted 不代表未生成思考。原生最小预算、总输出/交错思考规则属于具体 profile，不硬编码为所有 IR 值的限制 |
| [JSON Schema 2020-12 Core](https://json-schema.org/draft/2020-12/json-schema-core)，§8.1.1、§8.2.3.1 | `$schema` 声明方言；`$ref` 指向 Schema 位置，URI 是标识而非联网命令；boolean Schema 与递归图需要独立表达。采用的有限词汇、禁止 rebasing/dynamic refs 及预算仍归 [Schema profile](../architecture/schema-profile.md) |

硬 reasoning 上限是调用者的独立约束反例，不把任何 `budget_tokens` 字段未经证据直接映射为硬上限；当前无等价 carrier 时明确拒绝。来源仅作概念依据，样本由本地独立编写；Google 文档 CC-BY-4.0、示例 Apache-2.0 的 attribution 保持，不复制供应商 SDK、真实正文或外部实现。

<a id="provider-actions"></a>
## Provider 动作设计证据

证据标识为 `provider-actions-2026-10-07`，查阅日期 **2026-10-07**。以下只固定搜索动作及其观察/引用边界，不升级其他来源基线，不声明原生接入或完整 web-search union。行为归[交互合同](../architecture/interaction-contract.md#provider-tool-observations)，抽象反例由本地独立编写。

| API family / 固定分支 | 采用的区别与实施边界 |
|---|---|
| OpenAI Responses；上方固定 SDK 的 [`response_function_web_search.py`](https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses/response_function_web_search.py) 与 [web search guide](https://developers.openai.com/api/docs/guides/tools-web-search) | 原生复合 item 报告 search/open_page/find_in_page；search 可以不报告查询，SDK 分别声明可选 `query` 与 `queries`，不据此猜二者等价；find 的 URL/模式与 open-page 可选 URL 分开。答案及引用可以在独立 message 中，不能凭邻接制造结果边。`sources` 与原生输出接线不在动作切片，现有目标仍拒绝整个 Provider 观察 |
| Google Interactions **v1 Step**；[reference](https://ai.google.dev/api/interactions-api-v1) 的 GoogleSearchCallStep/GoogleSearchResultStep 与 [streaming](https://ai.google.dev/gemini-api/docs/interactions/streaming) | `google_search_call` 的 `id` 与 `arguments.queries`、结果 `call_id` 分开；`step.start` 可仅有 ID/type，动作随后由 `step.delta` 报告，不能将开头缺参数当作完整空动作。`signature` 位于 Step，结果也可有自身 signature；动作/关联反例不充当原生 parser，附件语义归下方 Replay 来源 |
| Anthropic Messages；`web_search_20250305` direct 分支的 [web search guide](https://platform.claude.com/docs/en/agents-and-tools/tool-use/web-search-tool) | `server_tool_use` 的 ID、`input.query` 与结果 `tool_use_id` 区分；参数可以经 `input_json_delta` 报告，block 开始不等于参数闭合。结果错误不等于 HTTP/Generation 失败；不因类型化动作实现而启用动态 filtering、执行 code 或构造 native blocks |
| Google **generateContent v1beta Part**；[Part reference](https://ai.google.dev/api/generate-content#Part) | `thoughtSignature` 属于 Part，与 Interactions Step 的 signature/引用域不互换。该页称 `toolCall` 为 server-side，同时将 `toolResponse` 描述为 client-populated；不能从这一措辞冲突认定客户端拥有执行责任。本片不采用它决定 executor、结果生产者或 Google 原生映射，保留独立来源域反例 |

来源标签由受信 intake/caller 声明，核心不解析标签猜 family，也不据字符串认证上游。未知/冲突原生事实需要 owning parser 的明确诊断；本片无该 parser，只对显式域不匹配给出关联诊断。Google 页面按 CC-BY-4.0、示例按 Apache-2.0 attribution；SDK 许可沿用上方 Apache-2.0 来源，不复制外部实现、真实正文或 opaque。

<a id="replay-attachments"></a>
## Owner-local Replay 来源

证据标识 `replay-attachments-2026-10-07`，查阅日期 **2026-10-07**；只约束选定附件及最终化，不升级其他固定基线或声明跨模型兼容。Google/Anthropic 使用独立 typed 反例，不伪造原生 parser。

| 来源 | Owner 与最终化边界 |
|---|---|
| OpenAI Responses 的 [reasoning](https://developers.openai.com/api/docs/guides/reasoning)、固定 SDK reasoning item 与[流事件](https://developers.openai.com/api/reference/resources/responses/streaming-events) | `encrypted_content` 归 reasoning item；现行 partial→item-done 最终化及闭合后拒绝边界只归 [Responses profile](../architecture/responses-text-profile.md#reasoning-replay-authority)，不外推给其他格式 |
| Google Interactions **v1** 的 [thinking](https://ai.google.dev/gemini-api/docs/thinking)、[Step 流事件](https://ai.google.dev/gemini-api/docs/interactions/streaming)和 [Step reference](https://ai.google.dev/api/interactions-api-v1) | thought 的 signature 独立于可选 summary，最后一个 thought-signature delta 在 step.stop 前到达；function/Provider Step 的 signature 附着于该 Step，不移入独立 reasoning。选定范围是文本/opaque-only thought 及函数/Provider 观察，非完整 Step union |
| Google generateContent **v1beta** 的 [Part reference](https://ai.google.dev/api/generate-content#Part) | thoughtSignature 属于原 Part，不是 Interactions Step 字段。此处选定 function call 与 assistant 文本/图片 Part 的静态承载；语义 call item 是该调用的 owner，不能将签名移给邻接消息或媒体。媒体分片和原生流 parser 不由此恢复 |
| Anthropic Messages 的 [thinking](https://platform.claude.com/docs/en/build-with-claude/thinking) | signature 归 thinking block；可见 thinking 为空仍保留它。signature_delta 在 content_block_stop 前到达，opaque 不解析、不重建；选定普通 thinking block，不将 redacted-thinking 或工具结果 encrypted_content 混为同一格式 |

具体合法节点与预算归[Replay owner](../../src/semantic/task/generation/replay.rs)及验证器。纯规范事件只接收已经组装完整的非 reasoning 附件，并在 owner 闭合前最终化；没有新增原生 opaque 分片 parser。组只作有来源的依赖，不因类型对称性制造组载荷。

本地绑定保护 owner 与显式声明的历史/组/配置依赖；目标要求精确匹配受信 origin，未知 origin 或兼容性没有隐式允许。标签与本地指纹不是签发方验证，不能由来源文档推导任意模型/版本可互换。未保留的 raw/未知字段不声称字节保真，typed 值与脱敏绑定也不证明真实上游接受。Google attribution 沿用上方许可。
