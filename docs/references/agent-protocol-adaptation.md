# Agent 用法与协议转换来源

本页只维护所选源码、协议入口及必要区别。所选原生映射归 [Anthropic 合同](../architecture/anthropic-messages-profile.md)，待实施工作归 [Anthropic 接入计划](../implementation-plans/anthropic-messages-draft.md)，有效语义与损失规则归 [Semantic Model](../architecture/semantic-ir.md) / [protocol/lowering](../architecture/protocol-and-lowering.md)。源码中的策略不是供应商规范，也不是运行兼容性证明。

## 固定源码与许可

| 来源 | 固定修订 | 许可 |
|---|---|---|
| pi | [`d1230ea2000d876b479a69b8b061f9d670f262f5`](https://github.com/earendil-works/pi/tree/d1230ea2000d876b479a69b8b061f9d670f262f5) | [MIT](https://github.com/earendil-works/pi/blob/d1230ea2000d876b479a69b8b061f9d670f262f5/LICENSE) |
| OpenCode | [`055d95bb7e278c94baf06235a52cac79dd13ba67`](https://github.com/anomalyco/opencode/tree/055d95bb7e278c94baf06235a52cac79dd13ba67) | [MIT](https://github.com/anomalyco/opencode/blob/055d95bb7e278c94baf06235a52cac79dd13ba67/LICENSE) |
| new-api / RelayKit | [`1d4328e97417a043a161a0dd30a5b129be3ace49`](https://github.com/QuantumNous/new-api/tree/1d4328e97417a043a161a0dd30a5b129be3ace49/relaykit) | [AGPL-3.0](https://github.com/QuantumNous/new-api/blob/1d4328e97417a043a161a0dd30a5b129be3ace49/LICENSE) |

这些修订用于本专项，不升级 [pi 既有 Provider/ClientManaged 来源](pi-provider-abstraction.md)、[OpenAI/Codex 基线](upstream-sync.md)或本地消费者锁文件。直接复用、移植代码或测试资产前检查许可证与依赖；new-api 的方案参考不等于可以把 AGPL 实现直接混入 MIT 项目。独立预期按协议和用途编写，不从第三方转换输出反推。

## 工具声明、生成与执行

| 固定入口 | 参考边界 |
|---|---|
| pi [Tool 类型](https://github.com/earendil-works/pi/blob/d1230ea2000d876b479a69b8b061f9d670f262f5/packages/ai/src/types.ts)、[constrained sampling](https://github.com/earendil-works/pi/blob/d1230ea2000d876b479a69b8b061f9d670f262f5/packages/ai/src/api/constrained-sampling.ts) | 执行参数与模型端约束分开；JSON Schema 有 prefer/require，grammar 不支持时回到普通工具编码 |
| pi [Responses 映射](https://github.com/earendil-works/pi/blob/d1230ea2000d876b479a69b8b061f9d670f262f5/packages/ai/src/api/openai-responses-shared.ts)、[测试入口](https://github.com/earendil-works/pi/blob/d1230ea2000d876b479a69b8b061f9d670f262f5/packages/ai/test/constrained-sampling.test.ts) | 单字符串参数与 custom input 的包装、事件和历史映射 |
| pi [Agent loop](https://github.com/earendil-works/pi/blob/d1230ea2000d876b479a69b8b061f9d670f262f5/packages/agent/src/agent-loop.ts)、[参数校验](https://github.com/earendil-works/pi/blob/d1230ea2000d876b479a69b8b061f9d670f262f5/packages/ai/src/utils/validation.ts) | 准备/校验后执行、错误结果与继续请求；外层 Schema 校验不等于 grammar 或工具内容校验 |
| OpenCode [工具包装](https://github.com/anomalyco/opencode/blob/055d95bb7e278c94baf06235a52cac79dd13ba67/packages/opencode/src/tool/tool.ts)、[apply_patch](https://github.com/anomalyco/opencode/blob/055d95bb7e278c94baf06235a52cac79dd13ba67/packages/opencode/src/tool/apply_patch.ts) | 字符串参数、原工具参数解码、patch 内容检查与执行权限分层 |
| OpenCode [工具接线](https://github.com/anomalyco/opencode/blob/055d95bb7e278c94baf06235a52cac79dd13ba67/packages/opencode/src/session/tools.ts)、[LLM 调用](https://github.com/anomalyco/opencode/blob/055d95bb7e278c94baf06235a52cac79dd13ba67/packages/opencode/src/session/llm.ts)、[invalid 工具](https://github.com/anomalyco/opencode/blob/055d95bb7e278c94baf06235a52cac79dd13ba67/packages/opencode/src/tool/invalid.ts) | 目标 Schema 与执行校验分开；错误名称的有限修正及模型可读的错误反馈 |
| RelayKit [请求转换](https://github.com/QuantumNous/new-api/blob/1d4328e97417a043a161a0dd30a5b129be3ace49/relaykit/relayconvert/request_registry.go)、[工具映射](https://github.com/QuantumNous/new-api/blob/1d4328e97417a043a161a0dd30a5b129be3ace49/relaykit/relayconvert/internal/toolconv/encode.go) | 工具抽取/附加、custom 字符串包装、选择与 namespace 映射、约束损失诊断 |
| RelayKit [静态响应还原](https://github.com/QuantumNous/new-api/blob/1d4328e97417a043a161a0dd30a5b129be3ace49/relaykit/relayconvert/internal/oai_chat/to_oai_responses_resp.go)、[流式还原](https://github.com/QuantumNous/new-api/blob/1d4328e97417a043a161a0dd30a5b129be3ace49/relaykit/relayconvert/internal/oai_chat/to_oai_responses_stream_resp.go) | 请求级工具记录驱动逆转换；宽松解包和等待完整参数分别影响错误行为与增量交付 |

<a id="history"></a>
## 历史与目标适配

- pi [transformMessages](https://github.com/earendil-works/pi/blob/d1230ea2000d876b479a69b8b061f9d670f262f5/packages/ai/src/api/transform-messages.ts)：按 Provider/API/Model 区分回放；跨模型转换可见 thinking、去除签名并同步工具 ID 引用。它也包含媒体占位与缺失结果补全，不能把这些行为混同于编码别名。
- pi [Anthropic codec](https://github.com/earendil-works/pi/blob/d1230ea2000d876b479a69b8b061f9d670f262f5/packages/ai/src/api/anthropic-messages.ts)：原生工具、thinking/redacted thinking、signature 及工具错误回传。
- OpenCode [历史消息投影](https://github.com/anomalyco/opencode/blob/055d95bb7e278c94baf06235a52cac79dd13ba67/packages/opencode/src/session/message-v2.ts)、[Provider transform](https://github.com/anomalyco/opencode/blob/055d95bb7e278c94baf06235a52cac79dd13ba67/packages/opencode/src/provider/transform.ts)：模型切换、工具结果媒体位置、目标 Schema 与 ID 适配。执行中断记录来自 Agent 状态，Gateway 不能据缺少结果推定执行结论。
- RelayKit [Claude→Responses](https://github.com/QuantumNous/new-api/blob/1d4328e97417a043a161a0dd30a5b129be3ace49/relaykit/relayconvert/internal/claude_messages/to_oai_responses_req.go)、[Responses→Claude](https://github.com/QuantumNous/new-api/blob/1d4328e97417a043a161a0dd30a5b129be3ace49/relaykit/relayconvert/internal/oai_responses/to_claude_messages_req.go)、[结束原因](https://github.com/QuantumNous/new-api/blob/1d4328e97417a043a161a0dd30a5b129be3ace49/relaykit/reasonmap/reasonmap.go)、[响应损失](https://github.com/QuantumNous/new-api/blob/1d4328e97417a043a161a0dd30a5b129be3ace49/relaykit/relayconvert/internal/toolconv/response.go)：字段映射与省略案例，不证明必要 signature、约束或逻辑进度得到保留。

包装后的文本可保持相同，但生成约束、模型行为与错误概率可能改变；成功序列化不证明工具可执行或任务完成。验证层次归 [conformance](conformance-baseline.md)，不由源码测试的存在推定实测结果。

<a id="anthropic-native"></a>
## 所选 Anthropic 原生来源

以下沿用接入准备的协议依据；实现前复核所选 operation/profile，不从源码修订推定部署实例、账号资格或模型准入。

- Anthropic Python SDK [`v1.13.0` / `b4b7916deaf4e5570cd627b1ae7ee4394a4de39f`](https://github.com/anthropics/anthropic-sdk-python/tree/b4b7916deaf4e5570cd627b1ae7ee4394a4de39f)，[MIT](https://github.com/anthropics/anthropic-sdk-python/blob/b4b7916deaf4e5570cd627b1ae7ee4394a4de39f/LICENSE)：Message、事件、thinking 控制的 required/nullable/union 交叉核对；不是本地已安装消费者。
- [Messages](https://platform.claude.com/docs/en/api/messages)、[streaming](https://platform.claude.com/docs/en/build-with-claude/streaming)、[thinking](https://platform.claude.com/docs/en/build-with-claude/thinking)、[effort](https://platform.claude.com/docs/en/build-with-claude/effort)、[Haiku migration](https://platform.claude.com/docs/en/models/haiku-5-5/migration-guide)：原生输入、控制、终态与回放；模型特定 guide 和所选 schema 共同约束映射。
- [OpenCode Go](https://opencode.ai/docs/go/)：operation、编码 Agent 用途、客户端与 session 要求及服务条款；动态信息实施时重新查询。
- 固定 OpenCode [Messages 路由](https://github.com/anomalyco/opencode/blob/055d95bb7e278c94baf06235a52cac79dd13ba67/packages/console/app/src/routes/zen/go/v1/messages.ts)与 [Anthropic helper](https://github.com/anomalyco/opencode/blob/055d95bb7e278c94baf06235a52cac79dd13ba67/packages/console/app/src/routes/zen/util/provider/anthropic.ts)：`/zen/go/v1/messages` 读取 `x-api-key`，helper 缺省设置 `anthropic-version: 2023-06-01`；后者不证明 Go ingress 强制检查该 header，也不等同于 Chat 或 Zen balance 路径。

### 文本工具与回传的补充入口

- 同一固定 SDK 的 [ToolParam](https://github.com/anthropics/anthropic-sdk-python/blob/b4b7916deaf4e5570cd627b1ae7ee4394a4de39f/src/anthropic/types/tool_param.py)、[ToolUseBlock](https://github.com/anthropics/anthropic-sdk-python/blob/b4b7916deaf4e5570cd627b1ae7ee4394a4de39f/src/anthropic/types/tool_use_block.py)、[ToolResultBlockParam](https://github.com/anthropics/anthropic-sdk-python/blob/b4b7916deaf4e5570cd627b1ae7ee4394a4de39f/src/anthropic/types/tool_result_block_param.py)、[Message](https://github.com/anthropics/anthropic-sdk-python/blob/b4b7916deaf4e5570cd627b1ae7ee4394a4de39f/src/anthropic/types/message.py)与 [Usage](https://github.com/anthropics/anthropic-sdk-python/blob/b4b7916deaf4e5570cd627b1ae7ee4394a4de39f/src/anthropic/types/usage.py)：required object input、可选错误位、响应时间缺省与 input/cache 的计数区别；宽泛 Dict 不证明全部 Schema 关键词或工具 union 已准入。
- 原生 [Define tools](https://platform.claude.com/docs/en/agents-and-tools/tool-use/implement-tool-use)、[Handle tool calls](https://platform.claude.com/docs/en/agents-and-tools/tool-use/handle-tool-calls)：tool_choice、结果所在消息和顺序；is_error 同时用于参数无效与执行错误，不独立报告执行阶段。文中的模型修正行为不是 Gateway 自动循环合同。
- [Preserved thinking](https://platform.claude.com/docs/en/build-with-claude/preserved-thinking)：签名依赖之前的 system/tools/messages，保留原 block 与顺序；账号关联、前缀编辑和 drop 行为分别处理。模型特定缺省仍按上方 migration guide 核对，不由 SDK 通用注释覆盖。
- 固定 RelayKit 的 [Schema helper](https://github.com/QuantumNous/new-api/blob/1d4328e97417a043a161a0dd30a5b129be3ace49/relaykit/relayconvert/internal/shared/claude/schema.go)、[tool choice](https://github.com/QuantumNous/new-api/blob/1d4328e97417a043a161a0dd30a5b129be3ace49/relaykit/relayconvert/internal/shared/claude/tool_choice.go)：function 的 input_schema 与 auto/none/required/具名选择映射；补 type/properties 是其归一化策略，不是原生 required 字段可省略的证明。
