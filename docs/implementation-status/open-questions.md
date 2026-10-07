# 当前待决问题

本页集中维护**因证据或语义决策不足而暂缓的问题**：未决点、实施边界和重新选片所需证据。它不是任务排期、行为合同、测试报告或完成记录，也不授予执行权限。问题定稿后，将有效决定归入 owning contract、获准行为归入 [current-focus](../implementation-plans/current-focus.md)，并移除本页对应条目，不累积历史。

基础语义归[交互合同](../architecture/interaction-contract.md#typed-replay-与信任)，实现缺口概览归 [Generation 状态](generation.md)，推进方向归 [next-goal](../implementation-plans/next-goal.md)。本页不复制完整缺口清单或 Provider 能力矩阵。

<a id="replay-attachment-owner"></a>
Replay 附件所有权归[交互合同](../architecture/interaction-contract.md#typed-replay-与信任)，实际行为切片归[current-focus](../implementation-plans/current-focus.md)，不在待决页维护第二份结构合同。

## Reasoning opaque 的闭合后权威

### 状态与实施边界

**等待更多测试证据，暂缓行为变更。** 不作为下一片或其他模态的前置。客户端不可自行改写 opaque 的基础语义不要求新增严格校验器；现有类型、接受/拒绝及依赖检查保持不变，不增加终态补全、替换、删除或通用 replay 更新事件。

当前 Responses 的最终化与终态一致性规则归 [profile](../architecture/responses-text-profile.md#reasoning-replay-authority)。不能把维持现状理解为已经证明现有策略适用于所有上游，也不能把延期理解为撤销现有检查。

### 待区分的变化

以下只描述**同一响应、同一已闭合 item** 的后续报告，不包含进行中值到 item-done 的正常最终化，也不比较不同响应的密文。`A`、`B` 为不同的合成非空值；“未报告”不将 absent、null、空字符串自动视为等价。

| item-done → response 终态 | 待明确的问题 |
|---|---|
| 未报告 → `A` | 是否允许迟到补全；以哪个事件确认值的完整性与回放权威？ |
| `A` → `B` | 是可接受的替换、重新签发还是协议冲突；谁有权确认，旧值是否仍可用？ |
| `A` → 未报告 | 是终态省略报告还是撤销已有值；能否继续保留 `A`？ |

不能仅按“更晚”或“更早已完成”决定权威；密文不同既不证明内部 reasoning 不同，也不证明两值等价。客户端能继续对话不证明原 reasoning 状态得到保持。

### 需要定稿的边界

- **来源与适用范围**：具体格式、签发方、目标和必要身份范围是什么；不把 Provider/model 字符串或一次 access token 当作普遍兼容性判据，不预设有效期或重新签发机制。
- **owner 与依赖**：如何确认是同一项；可见内容、顺序、关联或设置中哪些属于该格式的依赖，不预设整个 reasoning 对象的每个字段都被签名绑定。
- **交付与最终权威**：下游已经消费 item-done 后，如何获知获准的补全或更新；静态结果、流式最终结果和下一轮 history 使用哪个值，不能只在内部悄悄修改。
- **失败与完成条件**：值完整、item 闭合、response 状态、严格 EOF 与回放条件怎样组合；缺少必要事件、失败或中断不能靠终态密文补成成功。

这些问题未定稿前，不预建 `ReplayUpdated` 状态机、客户端完整性凭证、持久回放服务或私有 wire 字段；也不将未决变化藏进 fidelity 或计量损失策略。

### 恢复选片所需证据

1. 明确所选标准/profile 对各事件和字段 presence 的合同，并确认问题属于共享 IR 缺口、目标 wire 差异还是消费者处理差异。
2. 用最小合成事件序列分别覆盖上述三种变化，以及 item/响应身份不匹配、必要事件缺失、截断和终态失败；区分接收、保存、交付与实际回传的值。
3. 对照固定 pi 与官方 SDK 的实际消费和回放路径，而非只看最终显示内容。客户端策略是证据，不替代标准，也不自动成为 Gateway 默认行为。
4. 如需真实上游验证，另定目标、请求矩阵、预算和脱敏范围；只记录必要的字段存在性、值相等性、阶段与结果，不记录真实 opaque 或正文。一次续轮接受不证明不同密文普遍等价、相同 reasoning 被使用或长期有效。
5. 证据足以决定 authority、适用前提和交付后果后，再按 [current-focus](../implementation-plans/current-focus.md)选最小行为切片；不要求一次解决所有格式和目标。

### 来源与实现入口

- [固定 OpenAI 来源](../references/upstream-sync.md)、[Responses 流事件](https://developers.openai.com/api/reference/resources/responses/streaming-events)与 [reasoning 指南](https://developers.openai.com/api/docs/guides/reasoning)：核对最终化与回传要求，不从回传不可改写推出上游事件间永不变化。
- [pi 固定版本参考](../references/pi-provider-abstraction.md)：版本与许可由该参考拥有；相关 [Responses 消费与回放源码](https://github.com/earendil-works/pi/blob/cd32f7725fdbddbaecdff5b1e68491563394e0ca/packages/ai/src/api/openai-responses-shared.ts)用于定位客户端策略，不作为上游行为保证。
- [Replay 值](../../src/semantic/task/generation/replay.rs)、[事件 reducer](../../src/semantic/task/generation/event.rs)、[Responses event codec](../../src/protocol/openai/events/decode.rs)及[独立 SSE 反例](../../tests/transport/responses_sse.rs)：当前实现与验证边界，不在本页复制字段或测试结果。
