# Chat Completions：当前兼容 profile

Chat 是必须维护的公开兼容接口，设计允许[声明范围内的语义损失](protocol-and-lowering.md#semantic-loss)。**本页描述当前严格实现，不是新损失规则已经生效的声明。** 每条降级需有方向、前提、后果、必要依赖与独立预期；不能将兼容目标理解为任意丢字段或放宽 wire grammar。

Chat 与 Responses 使用同一 Generation IR、reducer 和严格 JSON 边界，不建立第二 IR。当前是单候选 profile，不是完整 Chat 标准或运行实例准入。固定标准/消费者版本归 [upstream-sync](../references/upstream-sync.md)，HTTP 接线归[网关指南](../http-gateway.md)。

## Entry points and ownership

[Chat envelope](../../src/protocol/openai/chat_envelope.rs)负责完整 bytes/JSON 边界，[Chat codec](../../src/protocol/openai/chat.rs)负责 task 映射，[Chat SSE](../../src/protocol/openai/chat_sse.rs)负责 framing/事件连接。它们不查 topology、credential 或网络。Context/delivery 拥有 model label、n、stream/options，不属于 message history。

Dialect/rules 从受信 [Adapter](../../src/adapter/mod.rs)选择，不是统一厂商字段超集。低层 standard profile 不因 MorphieCore/Provider profile 存在而接受其 reasoning、cache 或 usage carrier。所有默认和 alias 的条件归 named rules/owning codec，而非维护动态 Provider 表。

## Current admission

- 单个 candidate，完整 response/chunks 校验各自 identity/model/created、choice index、message/delta 与终态。多候选在本 profile 刻意拒绝，不是允许取 choice 0 后丢其他候选。
- 指令保留 authority/位置；user/assistant 文本 string 与有序 text parts 按 profile canonicalize。Tool result string 与 parts 的区别独立保持，不能套用消息合并规则。
- Selected user URL/inline 图片复用[图片输入](responses-text-profile.md#user-image-input)合同；工具图片、file ID 与更广媒体不由此推定。
- Generated audio、URL citations 和引用 history 由 [Chat media](chat-media-profile.md)单独拥有，不能将其固定值组合当通用媒体 IR，也不自动激活产品音频能力。
- 生成控制、概率、function tools、输出约束和请求/响应 context 均按字段 presence 与 profile 验证。Metadata、service tier、fingerprint 等 reported facts 不从 request 补齐；无 stream 位置的静态事实当前投影失败。
- Responses phase、non-complete instruction、active custom/program/configuration 等目前不能直接投影 Chat；不同 item、refusal、reasoning 或多 part grouping 不静默扁平化。具体失败由 [lowering](../../src/lowering/generation.rs)与独立 tests 维护；将来的有损规则只在其声明范围替换相应拒绝。

## Explicit message groups

Chat assistant message 是显式 owner，包括 tool-only 输出；`ToolCall.message` 是 attached function calls 的唯一 membership 权威。`message_groups()` 只借用派生 contiguous group，不猜相邻独立 call 的归属。

重排保持 identity 并重验 group，删除 owner 或移出组必须显式修复。Refusal 与 attached calls 的冲突在静态/事件首次发生时拒绝。标准 Responses 缺 membership carrier，现行 request/static/event 投影拒绝；[客户端边界](client-generation-profile.md)也不提供私有位置，不能把缺少 carrier 理解为已经允许丢失关系。

Owners：[group view](../../src/semantic/task/generation/group.rs)、[message group tests](../../tests/semantic/message_groups.rs)、[cross-profile tests](../../tests/semantic/group_projection.rs)。未来兼容策略必须区分仅展示信息与必要续轮关系，不能用通用 flatten 丢掉后者。

## Text and refusal probabilities

概率分别绑定 text/refusal owner，不附到 tool arguments 或 reasoning。Absent/null/explicit empty 的含义逐字段保持，不从 token 字符串猜 byte 数据。替换正文清除依赖概率，删除概率不能从 fidelity 恢复。

Request `logprobs` 与 `top_logprobs` 有组合约束；响应概率是独立报告，不从 request 补齐。静态、delta 与 final snapshot 不可重复、撤回或相互矛盾。Responses 所需完整 token/byte details 与 Chat 可缺省形式不同，refusal probabilities 也没有标准 Responses 位置；当前跨目标不满足时拒绝。精确范围与失败例归 [logprob tests](../../tests/semantic/chat_logprobs.rs)及 [codec](../../src/protocol/openai/chat_logprobs.rs)。

## Reported token details

Usage 的缺失/null 保持 unknown，显式零仍是报告。输入/输出细分以各自 total 为边界；prediction 的互斥与加法条件按 typed owner 校验，重叠的 text/reasoning/cache counts 不用于盲目求和。累计事件不是增量 token 相加。

Standard Chat、Responses 及 MorphieCore modality carriers 的位置不相同。现行 profile 对不可投影 facts 拒绝，不悄悄丢弃或把缺失补零；Chat 降级未来若省略部分报告，须按[损失合同](protocol-and-lowering.md#semantic-loss)区分省略与未报告。Audio details 归 [media usage](chat-media-profile.md#usage-and-unsupported-targets)，其他独立证据归 [usage](../../tests/semantic/usage.rs)与[image usage](../../tests/semantic/image_usage.rs)。

## Function selection and tool results

Function definitions、选择模式/allowed tools、parallel intent 和调用/结果分别拥有权威。选择不删改 definitions，不改 strict 默认，不启用 hosted/custom 执行。Call identity 而非名称或位置关联 result。

Result 的文本/parts、空正文/空数组及顺序保持；单 text result part 不自动折叠成 string。Pending-result view 从最终 history 重算，只报告缺失关联，不推断执行成功、turn 结束或下次请求就绪，见[continuation](responses-text-profile.md#response-outcome-and-continuation)。工具身份、参数与必要结果/replay 依赖不属于普通 Chat 损失许可。

## Scoped reasoning and unsupported fields

Readable reasoning 与普通 assistant text 分开。具名 Chat encrypted/structured carrier 映射到同一 format-bound replay owner，不另存 token；可读文本不是 opaque 的替代。Partial token 或 non-final owner 不可投影为可回放的最终值，即使 candidate 已因 length/content_filter 终止。

History 按 role 验证，包括存在但为 null 的不适用字段；未知内容不能静默吞掉。只有具名 profile 可接受 reasoning aliases、checked view 或 inactive 形式，wire rule 不改变必要语义。Owners：[reasoning codec](../../src/protocol/openai/chat_reasoning.rs)、[boundary tests](../../tests/semantic/reasoning_boundary.rs)、[wire tests](../../tests/semantic/chat_wire.rs)。

## Output constraint shells

Chat `response_format` 与 Responses `text.format` 共享输出意图，但 wrapper、presence 与 function strict 默认不同，不能仅换顶层名字。Schema 由最终 typed 值拥有，投影不删 required、不排序、不制造默认约束。当前 `response_format:null` 是本地选择，不是所有标准服务器接受的证明；inactive/default 与显式 text 形式分别保留。

结构/strict/局部引用的有效约束归 [Schema profile](schema-profile.md)。Request constraint 不作为 response echo 或生成 adherence 证明；不可等价投影的约束不能用普通兼容降级去掉。

## Derived view replay

当前 history 对 SDK `parsed`、`parsed_arguments` 只按声明规则核对其与权威 raw string 的派生关系，然后丢弃。Coercion/default/alias 不能成为第二份正文。工具 call 的 leaked `index` 若被准入，需匹配该 profile 坐标并作为表示 artifact 丢弃，不成为 identity。

这些便利准入不扩张 upstream response/chunk schema；改变 raw 值不复活旧 view。共享规则见 [Responses derived replay](responses-text-profile.md#derived-replay-views)及[独立 tests](../../tests/semantic/parsed_replay.rs)。

## Delivery and terminal rules

`stream_options` 只与 stream 组合，presence 与 effective defaults 分开。请求 include_usage 时编码必须收到真实 Usage 后才可闭合；不请求时允许省略对应 wire tail而不篡改内部报告。Padding 只属交付，预算独立，需调用方提供对应 seed/disabled 策略，不能耗尽后暗关。

`length` 与 `content_filter` 保持不同 incomplete 原因，refusal 内容独立；未知 finish 不能当 stop。Finish reason 只结束 candidate，真实 `[DONE]` 和严格 framing/EOF 才关闭流；缺失、早到、重复终态、终态后业务数据、截断与取消均不能恢复成功。

具名 normalization 可处理 continuation 的 absent/null delta、checked index、空 opening 或 metadata drift，但 id/model/工具身份及实际内容冲突仍失败。投影事件与静态结果必须一致，不能靠 SDK accumulator 的宽松推断补 finish、audio 或其他内容。

当前应用层 budget、背压、publication/commit、取消与 late-error 行为归[执行合同](execution-model.md)；改变 Chat 兼容策略不能改变它们。Owners：[transport checks](../../tests/transport/chat.rs)、[event codec](../../src/protocol/openai/events/mod.rs)和[SDK consumer](../../tests/sdk/chat_text_loop.py)。执行命令见[开发指南](../development.md)，结果不写入 profile。
