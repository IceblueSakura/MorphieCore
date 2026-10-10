# Anthropic Messages 文本与客户端工具合同

本页维护所选原生库场景的合同。[wire 类型/校验](../../src/protocol/anthropic/mod.rs)、所选[静态语义映射](../../src/protocol/anthropic/semantic.rs)、typed adapter/admission 与显式原请求上下文的 JSON intake 可独立使用；SSE、标准消费者与实例准入仍未闭合。共享概念归 [Generation 交互](interaction-contract.md)，通用投影边界归 [protocol/lowering](protocol-and-lowering.md)，未完成工作归[接入计划](../implementation-plans/anthropic-messages-draft.md)。它不是公开 `/v1/messages` 或标准 Responses 的新准入。

## 范围与采用依据

所选用途为文本请求、普通客户端 function 调用、调用者提供的文本结果/错误，以及同目标普通 thinking 的必要回传。采用[固定来源](../references/agent-protocol-adaptation.md)：pi `d1230ea` 的工具/执行分层和原生历史用法、OpenCode `055d95b` 的执行校验与错误反馈、RelayKit `1d4328e` 的字段映射；协议形状由固定 Anthropic SDK 与原生规范决定。跨语言参考算法，不移植 AGPL 实现。

| 固定入口 | 采用行为 | 对消费者的代价 | 本地 owner / 差额 | 独立反例 |
|---|---|---|---|---|
| pi Tool / constrained sampling、OpenCode Tool.wrap | 参数声明、生成约束、执行校验分层；先选非 strict function | 不保证模型输出符合 Schema，调用者仍校验和决定是否执行 | [FunctionTool](../../src/semantic/task/generation/tool.rs)、[Schema](schema-profile.md)；无需先建设 prefer/require 策略 | 合法 JSON 的参数缺必填字段，保留观察，调用者反馈错误 |
| pi convertTools、RelayKit function→input_schema | 直接传递完整的已准入 object Schema，不采用字段筛选或补值 | 缺 Schema、非 object 根或未支持的 strict 意图拒绝，不能靠删约束通过 | SchemaDocument 与纯 lowering；缺原生接线，不缺 Schema owner | 保留 enum、additionalProperties 和键顺序，不能只留下 properties/required |
| pi convertMessages、RelayKit tool_use/tool_result | 普通调用、结构化参数、具名选择、文本结果与 call ID 直接映射 | 非法 ID/name、错引用和不支持的消息安排明确失败，不自动修复 | 工具、[消息容器](interaction-contract.md#身份分组与依赖)与原生 codec | 两个不同 ID 经字符替换后碰撞，不能合并调用 |
| pi prepareToolCall/executePreparedToolCall、OpenCode InvalidArgumentsError | 参数拒绝与执行异常都可反馈错误正文，但执行事实独立保留 | 原生错误位不说明是否执行；调用者提供已知事实，Gateway 不推断 | [结果错误合同](interaction-contract.md#client-tool-result-errors)与 ToolResult；客户端新错误的标准载体另定 | 相同 is_error=true 分别来自校验拒绝、执行异常，不能都解码为已执行失败 |
| pi thinkingSignature / transformMessages、OpenCode 历史投影 | 同目标保留 thinking 值、owner、顺序和必要前缀；不采用缺结果补全或签名降文本 | 编辑前缀后拒绝旧绑定；标准 Responses 载体另定 | ReasoningItem / Replay / Fidelity；需原生 intake、最终化和配置/历史依赖接线 | signature-only 必须保留；改变 system 后重算 hash 不能放行 |

Custom/grammar 包装、namespace、server/program tools、媒体、redacted thinking、cache breakpoint/TTL、跨目标 history 与更广 stop/usage 不属于本合同。已有标准客户端和 strict-default 拒绝不变。

## 纯 wire 接口与接线边界

`Profile::AdaptiveTextTools` 的 request、Message 与单事件类型是 codec 表示，不是另一份 IR 或 Agent 执行入口。请求支持缺省控制，以及显式 adaptive/display、low/medium/high effort；其他控制、strict=true、未选中的 union/beta/报告字段明确拒绝。工具复用 SchemaDocument 与 StructuredValue 权威，不保留第二份可写参数或 Schema。

原始入口使用共享严格 JSON parser；typed 编辑后编码也重验结构、集合、总 bytes 与分配前的聚合节点预算。外围表示可以重编码，Schema/参数顺序、精度、presence、原始文字和签名值不改写。完整 Message 需要非 null stop，message_start 采用独立 opening 形状；单事件校验不证明顺序、block 最终化、message closure 或 HTTP EOF。后者必须由独立流 reducer/intake 验证，不能从初始 `{}` 或 signature_delta 构造完整语义观察。

原生 envelope 进入[共享 metadata](../../src/protocol/decoded.rs)，创建时间保持 None，不制造 instruction echo。静态语义入口需要显式 Provider/Model 兼容标签和调用者分配的 LocalScope；响应 intake 同时取得实际原请求，在共享历史上绑定必要前缀。NativeProjection 的有界 owner receipts 区分 Raw object 归一化与实际省略的执行报告，不保留另一份参数正文；只经过 wire 校验不代表语义准入或工具可执行。

[上游 adapter](../../src/adapter/upstream.rs)与客户端 OpenAI Profile 分开；[operation 声明](../../src/provider/definition.rs)决定固定 path/auth，编译和准备都检查 origin、path 与 credential kind/domain。Messages 使用 x-api-key，不全局切换 Chat 的认证；业务数据不能覆盖这些绑定。[低层原生准备](../../src/execution/messages.rs)只对已选 endpoint 和匹配模型验证 wire/headers/budget，不取凭据、不联网，也不替代语义准入。Go session 由中性上下文按[内部缓存投影](protocol-and-lowering.md#cache-affinity-projection)派生；纯原生 wire 准备仍接受独立的显式 Provider 分组，不将该低层参数暴露为下游 HTTP 输入。

[Request::from_generation](../../src/adapter/request.rs)提供非 OpenAI DTO 的 typed 入口；native adapter、candidate preflight 与 prepare 共用所选语义投影，保留 operation path/auth 和显式 session 的检查。有限 Public Model 合同只可声明 profile 实际实现的语义，native replay 的 Provider scope 必须与固定 endpoint 一致；不发布空壳能力。[Attempt::native_messages](../../src/execution/attempt.rs)只在已有显式原请求上下文时接收有界静态 JSON，完整字节输入结束后才 materialize，完成/取消后释放原请求。SSE intake、Gateway/binary 激活仍拒绝；库绑定不等于指定目标可用。静态语义片不接受空 system blocks 或流式意图，不将其改成缺省。

## 请求、Schema 与消息位置

- 普通工具保留 name、description presence 和完整 parameters；`input_schema` 必须由调用者明确提供，是已通过共享结构检查的、未声明方言/外部资源的 object-root Schema。首选 `Explicit(false)` 或 `Omitted(NonStrict)`，分别保持 false 或缺省；不将 `Omitted(NormalizeSchema)` 当成 NonStrict，也不删除 `Explicit(true)`。严格生成、output_schema、活动 dispatch 与其他未选中工具控制另片定稿。
- 生成约束不改变执行参数合同。不采用 pi 的可选字段转 required+nullable、执行值 coercion 或 optional-null 删除，不采用 RelayKit 的缺 type/properties 补值。本地只验证 Schema 结构；合法对象是否满足参数 Schema、工具内容及权限由调用者检查，Gateway 不执行 evaluator 或工具。
- `Auto`、`None`、`Required`、非 namespace 普通 function 的 `Specific(name)` 分别对应原生 auto、none、any、tool。具名选择须引用当前定义；并行开关只在原生有对应载体的选择上精确映射为 `disable_parallel_tool_use` 的反值，不默默忽略。模型/profile 准入仍独立；forced choice 可改变 thinking 行为，不能用它证明 auto 路径的 thinking 回传。
- 只选择当前请求顶层的 system 文本指令，不改 authority/scope/phase，不将 Developer 或中途指令折叠到顶层。当前工具配置必须在请求中完整声明；不引入 deferred tool changes。
- 原生 user/assistant 容器与 block 顺序显式映射。Text、Reasoning、ToolCall、ToolResult 各自拥有正文，容器引用它们；结果位于 user 容器不改变其结果身份。保留原生来源容器，不凭相邻推断新来源分组。
- 首选 history 由调用者明确给出合法原生安排：调用所在 assistant 后紧接结果所在 user，结果 blocks 在该 user 的普通文本之前；所有待结果调用须有唯一结果引用。不跨指令或其他消息搬移结果，也不制造缺失结果。无来源容器的跨协议重组尚未定稿；不能继承 Chat 重组规则。所选模型不接受 assistant prefill 时明确拒绝，不补造 user 文本。

## 参数、身份与工具结果

`tool_use.input` 必须是完整 JSON object，映射到唯一 StructuredValue；数字精度、对象顺序和 call 引用域保留。缺 input、null、重复键、非法 JSON 与未闭合构造不能补成 `{}`。完整但不符合参数 Schema 的 object 是可交付的模型观察，不是 Gateway 已验证的可执行输入。

`anthropic.function-json-object.v1` 是 request/history → 原生 tool_use 的有限表示归一化：完整 Raw 参数只在共享有界 parser 验证为 object 后，以派生只读 view 编码；Structured 参数直接编码。不改变数值、键顺序或字符串值，只允许外围 JSON 空白/转义写法变化；不保存第二份可写参数。空串、数组/scalar、duplicate keys 和 StructuredPartial 拒绝。它不授权反向输出 Responses 字符串参数，也不要求恢复原来的 JSON 排版。

名称与同目标 call ID 原样使用；若不满足原生限制或引用不唯一则拒绝。没有截断/替换 ID、按名称关联、猜参数或名称修正。标准消费者需要别名时另定碰撞与逆映射合同。

所选 `tool_result.content` 是显式文本字符串或有序 text blocks；缺 content、结构化结果或媒体不自动 stringify。空字符串与显式空 blocks 保持各自形式；目标无法接受的空值拒绝，不换成空格或 `"..."`。错误标记保留缺省、false、true；null/非 boolean 拒绝。三个值都不能补成成功执行报告，正文中的 `"Error"` 也不是错误位的替代品。

`anthropic.omit-tool-execution-detail.v1` 只适用于 request/history → 原生 tool_result：

- 对调用者已声明的成功执行、明确未执行或执行失败，可从候选 wire 投影省略原生没有位置的执行阶段；结果错误报告、全文和 call 关联不变。失败/未执行须已明确报告错误，成功执行须没有相反的错误报告；不能从 execution 补造 is_error 或错误正文。
- 首选只允许没有额外执行 code、没有产物生命周期报告的文本结果。取消、显式结果未知、额外 code/状态等尚未定稿的报告仍拒绝，不猜成普通失败。仅有原生错误位、没有执行报告的结果直接编码，不应用此省略规则。
- 原始 typed history 保留实际执行事实；投影重验结构、依赖和 requirements，以有界诊断区分已省略与原本未报告。Native 保存后只能恢复错误位与正文，不能从 wire 重建已省略的阶段。
- 这项损失仅允许模型收到既有文本反馈，不保证跨原生客户端保存后仍能审计执行阶段。它不适用于 response/event、Responses、Chat 或 Provider-executed 结果；工具错误不会把 Generation outcome 改成 Failed。

调用者可针对参数校验失败或执行异常生成实际错误反馈，再自主决定是否发送后继模型请求。错误反馈不是 transport retry、工具执行授权或 Gateway 自动修复循环。

## Thinking 与同目标回传

所选普通 thinking 的可见文字是摘要，进入 ReasoningContent::Summary；为空时不制造可读 part。signature 属于同一 ReasoningItem 的 AnthropicMessagesThinking 载荷，不成为 ResponsesEncrypted。即使文字为空，也保留 owner、容器位置和 signature。

- 默认控制留给受信模型/profile，不把缺省解释为关闭。所选默认 adaptive 路径不插入旧式 enabled/budget_tokens，不擅自填 effort 或删除显式 sampling。静态原生映射保留显式 adaptive/display/effort，不因此扩大标准目标或产品绑定，也不从 pi 的通用默认推断具体模型行为。
- `max_tokens` 是包含 thinking 的总输出上限，不是 reasoning 的独立硬上限；小预算可能在可见回答出现前合法截断。请求 summarized display 不解除必要签名回传。
- 静态完整 block 验证后最终化；SSE 在原 block 中依次构造 thinking/signature，content_block_stop 后才是完整值。非空签名或初始空对象不证明 block 完成；迟到 signature、缺 closure、error、取消与异常 EOF 不恢复成功。D 仍需固定完整事件 grammar。
- 同目标保存→追加结果→回传须保持整块文字/签名及原始顺序。必要签名丢失时拒绝保持同等续轮的请求；不生成空签名、不把可见 thinking 降为 assistant text，也不从来源记录恢复删除值。
- 绑定在受信 intake 取得原请求与输出前缀时建立，覆盖 system、tools、之前的 messages/blocks 和来源容器安排，包括同一输出中位于本 block 之前的内容。编辑、重排或删除该前缀使绑定失效；只追加本 block 之后的工具结果/新输入不自行改变它。不能到编码时对修改后的 history 重新取证，不能用笼统“同 key/model/session”替代依赖检查。
- 上述是本地必要条件，不证明 wire 前缀符合 issuer 检查、聚合层账号稳定或签名确被模型使用。账号/目标来源未知需明确报告；不采用同目标 prefix mismatch 自动 drop_block。切换 Provider 或 Model 时不续传 opaque；可见 thinking 的目标转换仍须独立定稿，不反向修改原观察。

## Gateway 自有续轮载荷

标准 Responses 回传优先采用 Gateway 自有认证加密载荷，候选位置是 reasoning 的 `encrypted_content`。`GatewayContinuation` 与 `AnthropicMessagesThinking`、`ResponsesEncrypted` 分开；前者由 Gateway 签发和解释，不是裸原生 signature 改名，不承诺其他 Gateway/OpenAI 服务可以解封。

[载荷 codec](../../src/protocol/gateway_replay.rs)只接收显式调用者密钥、已绑定的原生历史、受信兼容目标与认证 principal。认证加密绑定版本、principal、Provider/Model、原生 signature、可见 owner 值、必要前缀和显式期限；不携带凭据、credential locator、upstream origin 或另一份用户正文。签发使用系统随机 nonce，错误不回显内容。

客户端 conversation ID 和 cache key 不是认证或 issuer 证据，所选载荷不强制绑定它们；更换缓存分组不自行改变原生 replay 依赖。只有明确的上游 session-bound continuation 或独立产品隔离要求，才另定 session 绑定。相同 Provider/Model 不证明相同上游账号，必要来源范围仍须由受信执行边界保证，不能用 conversation ID 替代。

跨请求前缀使用有版本的[原生投影编码](../../src/protocol/anthropic/semantic_request.rs)，不序列化 Rust Debug、进程内 ID 或整份 IR。先验证当前 typed history 与认证载荷中的依赖，再建立本次 intake 的本地绑定；载荷不是恢复已删改正文的备份。签名仅在同 Provider/Model、有效期限和匹配依赖下恢复；目标变化只产生去除 opaque 的目标副本，并移除 opaque-only 空 owner，原观察不变。

纯库签发/验证不启用公开字段或密钥 loader。标准 Responses 的显式 carrier dispatch、来源容器恢复、下游本地时间、客户端新错误及固定消费者闭环仍未闭合；generic OpenAI codecs 不从字符串前缀猜格式，不接受 GatewayContinuation 作为 Provider 的 ResponsesEncrypted。Go 分组投影独立于该载荷；密钥生命周期与服务端存储不属于 codec。

## Envelope、进度与计量

原生 Message 的 ID/model 是已报告 envelope，未报告的 created 时间保持缺省；不填零、当前时间或调用者请求值。[共享 ResponseMetadata](../../src/protocol/decoded.rs)以 `Option<Number>` 拥有创建时间，独立校验实际报告的值，不要求增加 task 内时间戳。标准下游本地响应时间的 owner 另定；原生缺省不放宽标准目标的必填检查。

所选非流式 `stop_reason` 必须非 null；end_turn 映射 Completed / TurnFinished，tool_use 映射 Completed / AwaitingToolResults，max_tokens 映射 Incomplete / MaxOutputTokens，进度不补为 TurnFinished。相应组合按共享 validator 检查：没有待结果调用却报告 tool_use、或仍有待结果调用却报告 end_turn 不能修补成功。更广 stop/refusal 事实仍明确拒绝，不套用 pi 的统一 stop 别名；截断、response closure、产物完成与 transport EOF 分开。

Usage 归 operation。原生 input_tokens 不含 cache read/write，output_tokens 包含 thinking；分别保存实际报告，使用现有 ExcludesCacheReadAndWrite / IncludesReasoning 关系和具名派生 view。仅在所有必要计数存在时推导 inclusive input，不将其另存成上游 total；缺 cache/reasoning 明细保持未知，显式零仍是报告。SSE 累计快照更新而不求和，最终性随合法 message closure 确定；更广计量报告另片准入，不从文字长度猜测。

## 独立 synthetic 场景

以下为独立编写的合同例子，不由参考 encoder 或被测代码生成。`synthetic-model` 与签名均为占位值，不可作为真实调用或 issuer 校验样本；实现时在最低 owner 编写可执行的独立 wire→IR 和 IR→wire 预期，不以这里的结构检查宣称 native codec 已通过。

### 成功工具循环

首请求明确提供非 strict 工具：

```json
{
  "model": "synthetic-model",
  "max_tokens": 512,
  "system": "Use the supplied lookup tool.",
  "tools": [{
    "name": "lookup",
    "input_schema": {
      "type": "object",
      "properties": {"key": {"type": "string", "enum": ["A", "B"]}},
      "required": ["key"],
      "additionalProperties": false
    },
    "strict": false
  }],
  "tool_choice": {"type": "auto"},
  "messages": [{"role": "user", "content": "Look up A."}]
}
```

独立响应：

```json
{
  "id": "msg_synthetic_1",
  "type": "message",
  "role": "assistant",
  "model": "synthetic-model",
  "content": [{"type": "tool_use", "id": "toolu_synthetic_1", "name": "lookup", "input": {"key": "A"}}],
  "stop_reason": "tool_use",
  "stop_sequence": null,
  "usage": {"input_tokens": 11, "output_tokens": 7}
}
```

预期为一个 Structured ToolCall、显式 assistant 容器、Completed / AwaitingToolResults；created 和 cache 明细未报告，不能补零。调用者独立提供结果正文 `"value-A"`、无错误报告及已知成功执行，按上述有限规则发送下一轮；配置不变，仅将 messages 替换为下面的独立预期：

```json
[
  {"role": "user", "content": "Look up A."},
  {"role": "assistant", "content": [{"type": "tool_use", "id": "toolu_synthetic_1", "name": "lookup", "input": {"key": "A"}}]},
  {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "toolu_synthetic_1", "content": "value-A"}]}
]
```

结果省略的成功执行细节留在原 typed history；从上方 wire 读回只能得到未报告的执行阶段。最终模型文本响应 end_turn 才表明 TurnFinished。Gateway 不运行 lookup。

### 参数拒绝与执行异常

将上方 input 改为 `{}`：它仍是完整可交付的 object，但调用者校验拒绝、明确未执行，并提供下面的 user 结果；先前 assistant 观察原样保留。

```json
{
  "role": "user",
  "content": [{
    "type": "tool_result",
    "tool_use_id": "toolu_synthetic_1",
    "content": "Not executed: required key is missing.",
    "is_error": true
  }]
}
```

另一个独立分支在参数 `{"key":"A"}` 已通过校验后发生执行异常，调用者报告执行失败，正文为 `"Execution failed: lookup unavailable."`，同样使用 is_error=true。Wire intake 只能确认两者是错误反馈；不能解析英语正文推断执行阶段。后继模型是否重发新调用由调用者决定，新调用具有新 ID；旧结果不移挂，最多一次修正的 probe 预算归计划。

### Signature-only 的必要回传

独立 assistant 历史 block 数组：

```json
[
  {"type": "thinking", "thinking": "", "signature": "synthetic-signature-1"},
  {"type": "tool_use", "id": "toolu_synthetic_2", "name": "lookup", "input": {"key": "B"}}
]
```

预期为无可读 part 的 ReasoningItem 加 ToolCall，显式容器顺序不变；保存后仅追加对应工具结果，回传必须仍为原两块。删除 thinking owner、丢掉签名、替换 system/Schema、修改早期消息或移挂签名都拒绝；重建相同普通来源标签或对新前缀重新 hash 不改变结论。将同一独立 block 拆成 SSE 的 signature_delta 后，content_block_stop 前仍不允许作为完整 history；这些是实现验收要求，不是已执行的测试。
