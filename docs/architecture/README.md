# 语义架构与有效决策

本目录导航当前合同与最终 ADR。目的与概念由 [Semantic Model](semantic-ir.md)维护，接线由[当前架构](../architecture.md)维护，推进顺序由[后续计划](../implementation-plans/next-goal.md)维护。

## 阅读入口

| Owner | 责任 |
|---|---|
| [Semantic Model / IR](semantic-ir.md) | 统一语义、task/内容/资源、Responses/Embedding/Chat 目标、Agent 复用与 IR 缺口决策 |
| [Generation 交互](interaction-contract.md) | 值权威、工具结果、response/turn、有限关系、replay 与报告 |
| [Protocol / lowering](protocol-and-lowering.md) | Codec 与投影、四层能力、固定目标、各目标的具名损失及 fidelity |
| [Execution](execution-model.md) | 有界 I/O、publication/commit、取消、失败与执行权限 |
| [当前架构](../architecture.md) | 实际接线、模块 owner、依赖方向与复用边界 |
| [实现缺口](../implementation-status/generation.md) | 未承载、不可表示、未接线与未验收；不保存完成记录 |

## 当前 profiles

各 profile 维护所选 wire 的准入与交付合同；设计目标与当前实现范围分别核对。

- [Responses](responses-text-profile.md)：请求型 Generation 的现有边界及本地兼容形式。
- [Anthropic Messages](anthropic-messages-profile.md)：所选文本/工具/thinking 合同、纯 wire 接口与接线边界；完整语义/执行主链尚未闭合，不扩大标准客户端准入。
- [Chat](chat-text-profile.md)：单候选兼容路径；[Chat media](chat-media-profile.md)限定 citations/生成音频值、事件与引用；[语音输入](chat-input-audio-profile.md)限定 user WAV/MP3→文本。
- [Speech](speech-profile.md)：独立 TTS task、标准控制与有界二进制产物，不扩大 Chat/Responses 音频准入。
- [Transcription](transcription-profile.md)：独立识别 task、有界上传、实际时序报告与标准 JSON 投影，不提供文件服务。
- [Schema](schema-profile.md)：当前结构/strict/reference 准入，不证明生成 adherence。
- [Embedding](embedding-profile.md)：独立文本批次、稠密 float 向量、索引/维度与计量；不借用 Generation 或文件服务。
- [客户端 Generation 边界](client-generation-profile.md)：无独立 `_openbridge` attachment；typed 语义保留，缺少目标载体且无具名损失许可时明确拒绝。

## 架构决策

ADRs 只保留有效决策、必要理由、后果与直接 owner；不记录决策历史、实施顺序或字段清单。

| ADR | 决策 |
|---|---|
| [0001](decisions/0001-semantic-core.md) | 独立语义权威、标准 API 与 Agent 复用 |
| [0002](decisions/0002-task-ir-and-identities.md) | task、identity、presence 与扩展归属 |
| [0003](decisions/0003-codec-lowering-boundary.md) | codec 与固定目标投影分离 |
| [0004](decisions/0004-capability-separation.md) | 语义、表示、执行与公共合同分离 |
| [0005](decisions/0005-execution-lifecycle.md) | 语义盲执行与显式交付生命周期 |
| [0006](decisions/0006-reasoning-ownership.md) | reasoning/replay 的 owner、依赖与最终性 |
| [0007](decisions/0007-stateless-cache-affinity-and-extensions.md) | cache/context carrier 独立所有权 |
| [0008](decisions/0008-stable-core-and-vendor-adapters.md) | 单一 core、边界映射与声明的兼容损失 |
| [0009](decisions/0009-minimal-http-text-gateway.md) | 最小认证 loopback HTTP 网关 |
| [0010](decisions/0010-canonical-model-fixed-fallback.md) | canonical model 与固定提交前 fallback |
| [0011](decisions/0011-stable-admission-provider-cache.md) | 稳定准入与 Provider-owned cache affinity |
| [0012](decisions/0012-grok-personal-credential-pool.md) | 文件凭据、有序池与授权/执行生命周期分离 |

## 来源与验收

[固定来源](../references/upstream-sync.md)区分 OpenAI 公共标准、SDK 和 Codex 产品 profile；[来源索引](../references/README.md)定位其他 operation。

[验收基线](../references/conformance-baseline.md)定义独立 oracle、变换、损失与失败边界；[开发指南](../development.md)拥有命令。当前行为切片见 [current-focus](../implementation-plans/current-focus.md)，延期范围见[后续计划](../implementation-plans/next-goal.md#延期目标与恢复条件)。
