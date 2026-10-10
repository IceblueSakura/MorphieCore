# 模型交互实施边界与缺口

本页按负责层记录未完成边界。设计归[Semantic Model](../architecture/semantic-ir.md)，当前接线归[架构](../architecture.md)，优先级与延期归[next-goal](../implementation-plans/next-goal.md)。

## 文件范围与延期边界

维护 Responses user inline/URL 基础输入。Issuer-bound ID、工具/生成文件、更多格式/目标、Chat 文件投影及文件服务仍延期；恢复条件归[计划](../implementation-plans/next-goal.md#后续选片条件)。资源 identity、用途和引用坐标的纯语义工作可独立推进。

## 语义与表示缺口

| 范围 | 未闭合内容 / owner |
|---|---|
| 资源与引用 | issuer-bound 原生映射、更多有单位坐标、访问/期限条件的目标表示与资源生命周期；[资源表](../../src/semantic/task/generation/resource_table.rs)、[引用](../../src/semantic/task/generation/citation.rs) |
| 独立 task / 媒体 | Embedding 的 token/Base64/稀疏/多模态/Batch 增量、流式图片/产物、通用音频 history、完整字幕/说话人/翻译、更多编码及 Realtime；各[profile](../architecture/README.md#当前-profiles)与[计划](../implementation-plans/next-goal.md#延期目标与恢复条件) |
| 标准客户端覆盖 | 所选 profile 以外的 standard union、结构化参数/结果、执行报告、进度和非 operation-final usage 的公开载体；[客户端边界](../architecture/client-generation-profile.md) |
| 原生协议 | Google Interactions / Anthropic Messages codec、adapter 和执行主链 |
| 交互依赖 | 跨 response 逻辑 turn、continuation、部分结果 history 的执行准入，以及逐格式 replay 的原生交付；[交互合同](../architecture/interaction-contract.md) |
| Provider 工具 / 配置 | 更广工具动作、内容、来源引用、定义修订、配置和受保护变换的原生映射；[Provider 观察](../../src/semantic/task/generation/provider.rs)、[配置](../../src/semantic/task/generation/configuration.rs) |
| Replay | 非 reasoning 附件的公开 carrier、一般异构分组/非连续关系的回传；[分组投影](../architecture/protocol-and-lowering.md#message-envelope-projection) |
| 跨目标 history | opaque/thinking 投影的来源、格式、依赖和标准回传合同；[延期方向](../implementation-plans/next-goal.md#cross-target-history) |
| Opaque 权威 | 已闭合 item 的迟到补全、替换与撤销；[待决问题](open-questions.md#reasoning-opaque-的闭合后权威) |
| 限定工具选择 | 固定标准类型缺少 namespace-qualified `tool_choice` 位置；[namespace profile](../architecture/responses-text-profile.md#tool-namespaces) |
| Cache / usage | Provider 断点、TTL、远端缓存资源与更广 scoped 报告映射；[cache](../../src/semantic/cache.rs)、[usage views](../../src/semantic/task/generation/usage_views.rs) |
| 文本 / Schema | 未准入的控制/history、概率、reported context、多 part、方言/引用坐标、adherence evaluator 与更广 timestamp/metadata；[Chat](../architecture/chat-text-profile.md)、[Responses](../architecture/responses-text-profile.md)、[Schema](../architecture/schema-profile.md) |

库类型、wire 可表示、HTTP 接线、实例激活与上游接受分别核对；刻意的 profile 非目标不是待修 bug。现行具名投影之外的不可表示值仍拒绝。

## 扩展与执行缺口

- CustomSections / observation headers 的响应载体、namespace/版本、body/header 一致性，以及 turn 的来源与生命周期。
- 活动 continuation/conversation、store/background、compaction、WebSocket、hosted/dynamic tools 与资源操作的完整主链。
- 更广失败分类、credential affinity、动态 registry、同候选 retry、调度与 session 服务。现有固定 Route/pool 不包含这些策略。
- SIWC 远程持久化/复制的适用条款待确认，见[来源](../references/siwc-login.md#远程-host-与分布式应用边界)。

执行权限、publication/commit 与取消边界归[execution model](../architecture/execution-model.md)，Agent orchestration 属于调用方。

## 验收缺口

特定功能组合、标准 union 的 required/null/跨 kind、目标 wire、标准消费与必要历史回传仍按实际需求补证据。各层入口为 [semantic](../../tests/semantic.rs)、[transport](../../tests/transport.rs)、[credential](../../tests/credential.rs)、[gateway](../../tests/gateway.rs)及显式 [SDK gate](../../tests/sdk_loopback.rs)。

一般模型质量、Provider/TLS/network、缓存收益、负载/生产、原生平台/文件系统/ACL/断电分别验收。方法归[开发指南](../development.md)与[验收基线](../references/conformance-baseline.md)，运行结果不存本页。
