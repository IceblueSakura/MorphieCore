# 当前待决问题

本页只记录等待证据或语义选择的事项。定稿后将决定归入 ADR/owning contract，并移除条目；推进顺序归[next-goal](../implementation-plans/next-goal.md)。

## Reasoning opaque 的闭合后权威

### 状态与实施边界

等待事件与消费者证据，暂缓行为调整。现行接受/拒绝归 [Responses profile](../architecture/responses-text-profile.md#reasoning-replay-authority)，该问题不作为其他功能前置。

### 待区分的变化

限定同一响应、同一已闭合 item 的后续报告；`A`、`B` 是不同的合成非空值。

| item-done → response 终态 | 待明确的问题 |
|---|---|
| 未报告 → `A` | 迟到补全的合法条件与完整性确认 |
| `A` → `B` | 替换、重新签发或冲突的判据，及旧值有效性 |
| `A` → 未报告 | 省略与撤销的区分，是否保留 `A` |

Absent、null、空字符串逐字段判断；不同响应的密文不在此比较范围。

### 需要定稿的边界

- 格式、签发方、可信 scope、身份和期限。
- 同一 owner 的识别及内容、顺序、关联、设置依赖。
- item-done 后的更新如何交付，静态/流式/history 使用哪个值。
- 值完整、item 闭合、response 状态、EOF 与回放条件的组合。

### 恢复选片所需证据

1. 固定标准/profile 对事件与 presence 的合同，定位 IR、wire 或消费者缺口。
2. 独立合成序列覆盖三种变化、身份不匹配、必要事件缺失、截断和失败。
3. 对照固定官方 SDK 与 [pi 消费/回放源码](../references/pi-provider-abstraction.md)，区分接收、保存、交付和实际回传。
4. 必要 live 观察使用固定目标和预算，仅保留所需 presence/equality 与结果。续轮接受本身不足以决定不同密文的权威。

证据足以确定 authority 和交付后果后，按 [current-focus](../implementation-plans/current-focus.md)选择行为切片。

### 来源与实现入口

[固定 OpenAI 来源](../references/upstream-sync.md)、[Responses events](https://developers.openai.com/api/reference/resources/responses/streaming-events)、[reasoning guide](https://developers.openai.com/api/docs/guides/reasoning)；[Replay 值](../../src/semantic/task/generation/replay.rs)、[reducer](../../src/semantic/task/generation/event.rs)、[event codec](../../src/protocol/openai/events/decode.rs)与[独立反例](../../tests/transport/responses_sse.rs)。
