# 当前开发焦点

## 当前范围

**当前方向是[三协议约束下的 ClientManaged Generation IR 完善](client-managed-ir.md)。当前没有启动中的代码行为切片。** 下一子片优先补 S2.1 的配置修订标识/历史工具定义关联，再补 S2.2 reasoning mode/预算组合与 Schema 方言；S1.2 的动作/参数表达仍须按具体语义样本核对，不以局部检查宣称整个 S1 或 M1/M2 完成。

当前实现落点和后续退出条件统一见[计划的实施边界](client-managed-ir.md#当前实施边界与剩余工作)。Provider 观察采用[独立 typed 分支](../architecture/interaction-contract.md#provider-tool-observations)；后续补足动作/参数时保留已报告事实，不以 generic JSON 或虚构函数调用替代。规范结果增量事件不等于原生分片接入，E01 的配置修订/原定义关联不能由后继请求可构造推定。Google/Anthropic codecs、原生 hosted 工具输出选片、工具执行、模型 I/O、自动续轮、持久化和远端恢复不随纯核心实现恢复。

同类 replay 组的重叠规则归[交互合同](../architecture/interaction-contract.md#身份分组与依赖)。后续关系迁移不增加第二张可写 membership 或 ResultOf 表；无标准载体的关系维持严格拒绝，不因新类型自动激活接口、工具执行或模型 I/O。

范围、依赖和测试归属统一归该计划，不在本页维护第二份清单。三协议是语义设计依据；Google/Anthropic 原生实现、ServerManaged 和 Agent 执行/恢复延期。现有标准文本/function、模型发现、图片与请求型音频保留为回归边界；音频扩展按[后续计划](next-goal.md)独立选片。

文件产品扩展与 SIWC 实例准入仍按既有恢复条件处理；资源用途、locator 与引用的纯 IR 完善不等于开启文件服务。低延迟交付、下游 SSE 或转录扩展不自动恢复 Realtime、声音资源服务或通用媒体框架。

现有能力维护仍遵守以下合同：

- **请求型音频**：[Speech](../architecture/speech-profile.md) 与 [Transcription profile](../architecture/transcription-profile.md)拥有独立 task、标准分支与原生映射；[具名附属报告投影](../architecture/protocol-and-lowering.md#独立音频的附属报告投影)不授权丢弃请求控制、正文、失败或闭合约束。嵌入/binary 激活归 [HTTP 指南](../http-gateway.md)。EOF、SDK 消费或合成音频不能证明真实质量、低延迟或上游费用上限，不扩大 Chat 音频与 replay 准入，也不解除 Token Plan 的工具交互式使用限制。
- **基础静态图片生成**：数量、报告与严格交付边界见 [HTTP 指南](../http-gateway.md#独立图片生成)；[具名计量损失](../architecture/protocol-and-lowering.md#独立-images-的计量投影)不授权丢弃请求控制、篡改产物报告或隐藏预算失败。
- **基础文件输入**：仅维持 Responses user [inline](../architecture/responses-text-profile.md#user-inline-file-input) / [URL](../architecture/responses-text-profile.md#user-file-url-input) 输入及必要正确性、安全维护；不从 PDF carrier 推定所有格式、来源或工具文件均准入，也不新增 `/v1/files` 或 file_id 服务。
- **客户端与迁移**：遵守[客户端合同](../architecture/client-generation-profile.md)，不恢复独立 `_openbridge` 或隐式兼容入口；允许破坏性重写不免除 IR 结构缺口报告，也不提前应用未定稿 Chat 损失规则。

文件与必要 opaque 回传的验证分别覆盖“实际报告且回传”和“未报告”；后者即便内容正确也不证明 opaque 路径。显式 reasoning 控制不充当已生成 reasoning 的事实，有限场景通过不等于一般可靠性。真实验证授权归 [AGENTS.md](../../AGENTS.md#standing-authorization-for-live-provider-verification)，每次仍明确任务场景、有限矩阵与执行限制；计划不改变私有 activation、部署、凭据或真实工具副作用的授权边界。

## 待决问题与实施边界

等待证据或语义决策的问题归[待决状态](../implementation-status/open-questions.md)。Reasoning opaque 的闭合后权威暂缓，不作为当前行为切片或其他模态的前置；本页不重复其问题清单，文档澄清不制造校验或实现任务。

## 新切片的定稿要求

后续选片复用已有标准边界、类型化消费与回传验证；发现新的标准或消费差异时，以独立反例定位最低 owner，不重复重建已满足的合同。

后续行为实施先在本页定稿可观察结果、需求、不变量、失败例、非目标与验证边界；出现未覆盖的结构或标准分歧时先更新/确认切片，不以计划代替操作授权。文档整理不创建行为切片；实现缺口归[状态文档](../implementation-status/generation.md)。
