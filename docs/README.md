# MorphieCore 文档

**实现事实优先放在代码、邻近注释和独立测试中。** Markdown 只保留不能由局部实现充分表达的决策、跨模块合同、设计缺口、来源和操作方法。

## 按任务阅读

- **运行与接入**：[HTTP 指南](http-gateway.md) → 所需的[凭据操作](credentials.md)；操作授权归 [AGENTS.md](../AGENTS.md#scope-and-authorization)，真实验证方法归 [Probe 指南](probes.md)。
- **修改行为**：[当前焦点](implementation-plans/current-focus.md)与[后续计划](implementation-plans/next-goal.md) → [语义架构索引](architecture/README.md)中的相关合同 → owning code、独立测试与[开发检查](development.md)。不要求通读所有参考资料。
- **定位缺口**：[实施边界索引](implementation-status/README.md)区分实现缺口与等待证据的问题；不要把缺口列表当排期。
- **核对协议**：[来源索引](references/README.md)定位固定标准、SDK 与产品 profile；动态准入按 [AGENTS 查询流程](../AGENTS.md#current-provider-model-and-compatibility-information)现场核对。

## 内容所有权

| 入口 | 责任 |
|---|---|
| [根 README](../README.md) | 产品定位、使用和构建入口 |
| [AGENTS.md](../AGENTS.md) | 授权、安全、变更和验证规则；不复制架构或字段表 |
| [当前架构](architecture.md) | 跨模块职责与数据流；细节链接源码 |
| [设计与 ADR](architecture/README.md) / [Semantic Model](architecture/semantic-ir.md) | Gateway/Agent 共用语义，标准 API 与缺口决策；[投影合同](architecture/protocol-and-lowering.md)拥有 Chat 损失规则；子域不另设主线 |
| [模型交互缺口](implementation-status/generation.md) | 尚未准入、不可表示、未接线与验收缺口；不列完成记录 |
| [当前待决问题](implementation-status/open-questions.md) | 等待证据或语义决策的具体问题、实施边界与恢复条件；不作为排期、合同或测试报告 |
| [实施计划索引](implementation-plans/README.md)：[当前焦点](implementation-plans/current-focus.md) / [后续计划](implementation-plans/next-goal.md) | 获准行为切片 / 推进方向；不是授权来源 |
| [开发指南](development.md) | 本地检查方法与验证层级，不记录执行结果 |
| [HTTP 指南](http-gateway.md) / [OpenAPI](openapi.json) | 启动与公共 HTTP 接口 |
| [凭据管理](credentials.md) | API key / OAuth 的统一文件管理、各自生命周期与存储恢复边界 |
| [Probe 指南](probes.md) | 显式计划、预算、执行和诊断边界 |
| [来源入口](references/README.md) | 必要标准出处、固定版本和许可；动态信息按需重查 |

## 写作与维护

- 新规则先落在 owning type、validation、codec 或独立回归；非显然的协议、兼容、安全和资源理由放在邻近注释。不要用文档弥补缺失的可执行约束。
- ADR 只维护当前生效方案、必要理由和约束，不记录决策过程、弃用方案或实施先后。字段、默认值、预算、注册和测试场景归代码；操作命令归指南。跨模块合同不能仅因实现尚缺而删除。
- 一项事实只保留一个 owner，并从索引或上层指南直接链接；README 不复制模块目录、计划、检查命令或 profile 字段表；依赖约束只归架构 owner。避免相邻文档互相转述或形成阅读依赖链。profile 文档解释准入，精确 wire 接受/拒绝由源码和独立预期维护；不能把偶然行为当成设计。
- 不保留历史分析、项目比较、审计报告、测试结果或完成日记，离线/SDK 结果也不例外。历史问题查 Git；当次结果在交付中报告，获准产物仅留 ignored run，不迁入注释。
- Provider/模型、实例启用及上游可用性按 [查询流程](../AGENTS.md#current-provider-model-and-compatibility-information)核对。资料链接、类型和测试存在都不是成功执行的证明；评估时分别报告设计、库级表示、HTTP 接线、消费者与外部执行，不用字段数、测试数或无明确分母的百分比替代闭合判据。
- 跨会话记忆只保留必要的用户偏好、决策背景与 canonical owner 导航，不复制合同、动态库存或运行结果。旧记忆与现行来源冲突时明确更正，不复活失效报告、旧入口或历史授权。
- 保留仍使用的外部材料的来源、固定版本和许可；不为本地整理刷新外部验证日期，不复制动态 API 清单。
- 合并重复说明、字段表或推测性结构时保留有效约束及 attribution，同步更新入链；不为整理另建审计页、完成记录或第二份合同。

文档验证步骤归[开发指南](development.md#文档与边界)。仅文档修改不宣称运行时或 Agent 行为改善。
