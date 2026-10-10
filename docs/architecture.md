# 当前架构

MorphieCore 将共享语义库接入最小认证 loopback 网关。这里说明跨模块职责；字段映射、默认值、预算和失败分支由 owning code、注释与独立测试维护。设计理由见 [ADRs](architecture/README.md#架构决策)，未闭合范围见[实施缺口](implementation-status/generation.md)。

```text
Authenticated bounded HTTP input
 → fixed Public Model/task lookup
 → Client Adapter.decode_request
 → task semantics + context/delivery + scoped carriers/fidelity
 → trusted transform / public admission / requirements
 → fixed candidate projection → prepared request/auth → HTTP transport
 → Provider Adapter JSON/SSE/binary decode → task result/Event IR
 → Client Adapter projection → downstream body handoff / commit / complete
```

## 模块所有权

| Owner | 责任 |
|---|---|
| [semantic](../src/semantic/mod.rs) | task 类型、验证、requirements、reducer；标准 context/delivery，不依赖 protocol/provider/execution |
| [adapter](../src/adapter/mod.rs) | 可信双向 profile 选择、统一 request 与目标投影；不访问网络或凭据 |
| [protocol](../src/protocol/mod.rs) | 共用语法、envelope/event codecs、strict JSON、具名适配和来源保真 |
| [lowering](../src/lowering/generation.rs) | 最终不可变语义的目标可表示性；不选择 Provider、不恢复删除值 |
| [provider](../src/provider/mod.rs) | 可信 origin/path、认证材料和 HTTP 错误分类 |
| [credential](../src/credential/mod.rs) | 统一 API key / OAuth manager 与安全文件 store；API key 按认证域和 epoch 绑定，OAuth drivers 管理各自授权合同；提供有序池与短生命周期借用；执行层按显式策略前移，不自动刷新或负载均衡 |
| [topology](../src/topology/mod.rs) | canonical/public model、Route、Endpoint 的固定关系与编译 |
| [execution](../src/execution/mod.rs) | 候选计划、请求准备、增量 intake、响应投影和显式交付生命周期 |
| [SSE transport](../src/transport/sse.rs) / [HTTP transport](../src/transport/http.rs) | 有界 framing / 对已准备可信请求执行 I/O；不解释或改写 IR |
| [gateway](../src/gateway/mod.rs) | 认证、启动准入、预算和实际 HTTP body 所有权 |
| [binary](../src/bin/morphiecore.rs) | 显式私有文件 bootstrap 与 loopback listener |

独立静态图片生成采用 [ImageGeneration task](../src/semantic/task/image_generation.rs)、[类型化请求入口](../src/adapter/images.rs) 与 [Images codec](../src/protocol/openai/images.rs)。[图片 binding](../src/topology/images.rs)在同一 CompiledTopology 中显式声明 public label、canonical identity、单 endpoint 与 Provider operation 路径，不给 Generation 合同填占位字段；有序产物集合共享一次执行和响应级计量，目标 profile 与计量投影策略分别显式选择。Gateway 的 [Images intake](../src/gateway/images.rs)共享认证、受信 transport 和上述 body publication/handoff 生命周期，不增加重试或资源服务。产品绑定归[图片 catalog](../src/topology/catalog/images.rs)，binary 仅激活明确选定的图片标签，不从已有凭据池推定媒体授权。

[Models 目录](../src/gateway/models.rs)从启动激活的各 task 标签构建有界只读视图；[publication metadata](../src/topology/model_metadata.rs)归 topology canonical identity，不进入 task IR 或依赖具体推理 Provider。它复用认证但不经过生成执行链，也不访问上游目录或操作凭据；公开合同归[模型发现](http-gateway.md#标准模型发现)。

独立 Speech 使用 [SpeechSynthesis task](../src/semantic/task/speech_synthesis.rs)、[标准 codec](../src/protocol/openai/speech.rs)、[目标准入/报告投影](../src/lowering/speech.rs)与[显式绑定](../src/topology/speech.rs)。[Speech adapter](../src/adapter/speech.rs)选择标准、[OpenRouter MP3](../src/protocol/openrouter_speech.rs)或[原生 MP3](../src/protocol/aliyun_speech.rs)映射；产品 ID、voice 与 metadata 归 [Speech catalog](../src/topology/catalog/speech.rs)。[Speech intake](../src/gateway/speech.rs)按可信 profile 收集二进制或聚合原生 SSE，沿用 transport、publication/handoff 和取消 owner；binary 显式激活归 [HTTP 指南](http-gateway.md#独立语音生成)。

独立识别使用 [SpeechRecognition task](../src/semantic/task/speech_recognition.rs)、[标准 multipart/JSON codec](../src/protocol/openai/transcription.rs)和[原生 ASR 映射](../src/protocol/aliyun_asr.rs)。[绑定](../src/topology/transcription.rs)与[Gateway intake](../src/gateway/transcription.rs)不创建 Generation 消息、文件服务或 URL 获取器；[Transcription profile](architecture/transcription-profile.md)拥有报告 scope 与投影边界，显式激活归 [HTTP 指南](http-gateway.md#独立语音识别)。

独立文本 Embedding 使用 [task](../src/semantic/task/embedding.rs)、[envelope](../src/adapter/embeddings.rs)、[标准 float codec](../src/protocol/openai/embeddings.rs)与[目标准入](../src/lowering/embeddings.rs)。[固定绑定](../src/topology/embeddings.rs)和 [Gateway intake](../src/gateway/embeddings.rs)复用受信 transport、严格 EOF、取消及共享 handoff，不创建 Generation items、文件服务、输出 token cap 或 retry/fallback。具体范围和显式激活归 [Embedding profile](architecture/embedding-profile.md)。

## 容易混淆的边界

- **注册不等于启用**：catalog 声明与 bootstrap 激活分开。Public Model 的语义准入也不是响应 reported facts 的白名单。现场查询方法见 [AGENTS](../AGENTS.md#current-provider-model-and-compatibility-information)。
- **适配不等于第二份语义**：[WireRules](../src/protocol/adaptation.rs) 声明受信规则，[fidelity](../src/protocol/fidelity.rs) 只保存有界来源/依赖记录。它们不能覆盖最终 IR。
- **策略不拥有 I/O**：[selector](../src/execution/plan.rs) 与 [fallback policy](../src/execution/fallback.rs) 是纯策略；[exchange](../src/gateway/exchange.rs) 协调候选/预算，[intake](../src/gateway/intake.rs) 拥有一个上游 body 的解码/投影，[body](../src/gateway/body.rs) 拥有 publication、acknowledgement 和取消。
- **编码、发布、提交、完成不同**：发布先冻结候选前移，body handoff 确认 commit，严格上游关闭与最终 handoff 决定完成；后续失败只能中止，不能伪造成功或拼接另一 attempt。具体状态转换和竞态处理留在对应实现。
- **缓存不是会话管理**：[cache projection](../src/protocol/cache.rs) 投影显式 Provider carrier；标准 identity hints 与 cache hints 独立。无网关回答缓存、负载均衡或跨请求粘性路由。
- **探测不是产品合同**：[probe tooling](../examples/probe_support/) 拥有计划、共享预算与脱敏结果，[diagnostics](../src/gateway/diagnostics.rs) 提供有界元数据。工具可选项、执行历史和 sink 状态不能决定产品准入。

## 依赖与复用边界

`semantic` 是任务值、validation、requirements 与 reducer 的唯一 owner，不依赖 protocol/provider/topology/execution。Codec、lowering 与 adapter 保持纯映射；协议共同类型不放入某家 Provider IR。Gateway、未来 Agent 的类型化入口都消费同一语义模型；Agent runtime 与恢复属于[项目非目标](architecture/semantic-ir.md#1-语义权威与消费者)，不作为待实现能力。

模块按职责拆分，保留必要公共路径的显式 re-export，不复制 SDK 目录、预建未实现 task 模块或引入万能 raw-JSON 请求。当前保持一个 crate；独立 SDK/crate 与持久化格式仅在有明确消费者后定稿。各目标只采用[已定稿的具名投影](architecture/protocol-and-lowering.md#semantic-loss)，实际准入查 owning lowering/codec；不能从设计推定所有目标均已接线或实例启用，也不改变共享输入或运行时权限。

启动和公共 HTTP 用法见[网关指南](http-gateway.md)，检查命令见[开发指南](development.md)，真实调用的授权与预算见[Probe 指南](probes.md)。
