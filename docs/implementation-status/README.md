# 实施边界

本目录区分两类尚未解决的问题，不保留已完成列表、历史审计或执行报告：

| 入口 | 责任 |
|---|---|
| [模型交互缺口](generation.md) | 相对当前设计的承载、表示、接线与验收缺口；不是排期 |
| [当前待决问题](open-questions.md) | 等待证据或语义决策的问题、实施边界与恢复条件；不是已定稿合同 |

- 跨模块结构见[架构](../architecture.md)，行为细节查 owning code、注释和独立测试。
- 推进方向见[下一步目标](../implementation-plans/next-goal.md)，获准切片见[当前焦点](../implementation-plans/current-focus.md)。
- 检查方法见[开发指南](../development.md)，当前 Provider/模型与实例启用按 [AGENTS](../../AGENTS.md#current-provider-model-and-compatibility-information)现场查询。

类型表达、codec 映射、网关接线、实例启用和实际执行分别判断。验证边界归[开发指南](../development.md#文档与边界)，结果只在当次交付与获准 ignored run 中报告。
