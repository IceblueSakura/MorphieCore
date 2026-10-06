# 客户端 Generation 边界

公开主目标是[规范 Responses](semantic-ir.md#3-客户端-api-目标与扩展边界)，Chat 仅作有界有损的兼容路径。HTTP Responses 选择 `Dialect::Standard`，Chat 保持 `Dialect::MorphieCore`；Provider 的 reasoning、usage、cache 等具名规则不能扩张标准下游。标准无载体时拒绝而不是暗中省略，已实现范围仍按 owning codec/profile 核对，不代表整个标准已符合。

## 无独立私有 attachment

`_openbridge` 是被禁止的独立协议字段的精确拼写，不随项目更名而替换；MorphieCore 不引入同义 carrier。

请求、响应及事件的 envelope/item **不接受或输出独立 `_openbridge` 字段**，包括 null、空对象及带版本的对象。不存在开关恢复、同义替代字段或私有事件。普通文本、raw tool arguments/output 或允许任意键的用户 metadata 中的同名业务数据不被当作协议 attachment。

需要标准载体的正文、工具 call/result、图片与原生 Responses encrypted content 继续使用其标准位置。Typed 结构化参数/结果、执行报告、message-call membership、显式 interaction progress、scoped usage 和其他格式 replay 仍由共享语义 owner 表达；无载体时按目标拒绝，不能把它们 stringify、去掉关联或藏入 fidelity。

必要 replay 的 format、finality、scope 与依赖规则不因客户端字段删除而减弱。未来 Agent 可以直接消费同一 typed 模型，不要求客户端保留 unknown fields，也不从普通 hash 或 scope label 推定 issuer 认证。

## 交付与失败

- 含私有 attachment 的请求在上游 I/O 前拒绝；标准 field presence 与 raw JSON 验证继续生效。
- 不可表示的静态输出失败；已经发布的事件流只能中止，不能补造 successful terminal 或切换 attempt。
- 静态、opening/done 与 terminal snapshot 使用一致准入；拒绝后 decoder/encoder 不能恢复成功。
- 客户端接收了标准形状不证明全部语义可跨协议回放；Chat 损失必须遵循[具名投影合同](protocol-and-lowering.md#semantic-loss)，不反向放宽 Responses。

Owners：[Responses codec](../../src/protocol/openai/responses.rs)、[静态结果](../../src/protocol/openai/static_response.rs)、[事件](../../src/protocol/openai/events/mod.rs)、[lowering](../../src/lowering/generation.rs)。独立反例见[客户端边界](../../tests/semantic/client_carrier.rs)，实际 HTTP admission/abort 见[Gateway smoke](../../tests/gateway.rs)；typed 值与依赖的测试仍留在各语义 owner。

是否重建私有扩展在迁移完成后另行决定；当前不维护旧 wire 兼容承诺，不提前设计替代 carrier。HTTP 接线归[网关指南](../http-gateway.md)，实施顺序归[后续计划](../implementation-plans/next-goal.md)。
