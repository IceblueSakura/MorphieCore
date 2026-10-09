# MorphieCore 文档

文档保留当前合同、最终 ADR、未完成方向、操作方法和必要来源。字段、默认值、预算、注册与独立预期由源码和测试维护。

## 按任务阅读

- 运行接入：[HTTP](http-gateway.md)与[凭据](credentials.md)。
- 修改行为：[当前焦点](implementation-plans/current-focus.md) → 相关[语义合同](architecture/README.md)与源码 → [开发检查](development.md)。
- 选择工作：[后续计划](implementation-plans/next-goal.md)、[实现缺口](implementation-status/generation.md)与[待决问题](implementation-status/open-questions.md)。
- 核对协议：[固定来源](references/README.md)；真实验证方法见[Probe](probes.md)。

## 内容所有权

| Owner | 内容 |
|---|---|
| [README](../README.md) / [AGENTS](../AGENTS.md) | 项目入口 / 授权、变更与安全规则 |
| [架构](architecture.md) / [语义架构与 ADR](architecture/README.md) | 接线职责 / 最终设计与跨模块合同 |
| [计划](implementation-plans/README.md) / [状态](implementation-status/README.md) | 当前切片与后续方向 / 未完成缺口与待决点 |
| [开发](development.md)、[HTTP](http-gateway.md)、[凭据](credentials.md)、[Probe](probes.md) | 操作与验证方法 |
| [来源](references/README.md) | 固定版本、必要协议差异、出处与许可 |

## 写作与维护

- 一项事实只有一个 owner；上层页面链接 owner，不复制字段表、规则或操作清单。
- ADR 只记录最终决策、必要理由和后果。参考页只记录外部来源与必要差异，不重复本项目采用决定。
- 已完成计划、决策过程、比较分析和执行报告移出当前文档，不另存历史页；历史查 Git，获准运行产物留在 ignored run。
- 写明确的适用范围和正向规则，将有效拒绝条件、安全边界集中放在负责页；避免每段重复免责声明或追加假设性防线。
- 保留未决事项、有效约束、来源版本与许可；整理不改变合同，不刷新外部验证日期。移动内容时同步入链与锚点。
- 动态 Provider/model、账户资格与实例启用按 [AGENTS 查询流程](../AGENTS.md#current-provider-model-and-compatibility-information)核对。验证按[开发指南](development.md#文档与边界)，仅文档修改不运行模型或改变行为。
