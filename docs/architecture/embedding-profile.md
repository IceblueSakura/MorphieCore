# 文本 Embedding profile

Embedding 是独立请求型 task，使用标准 `POST /v1/embeddings`，不借用 Generation message、Responses output 或工具循环。来源为 [OpenAI operation](https://developers.openai.com/api/reference/resources/embeddings/methods/create)、[固定 SDK](../references/upstream-sync.md)的 Embedding 类型、[OpenRouter operation](https://openrouter.ai/docs/api/api-reference/embeddings/create-embeddings)和[百炼同步 API](https://help.aliyun.com/en/model-studio/text-embedding-synchronous-api)。本 profile 只采用文本输入和稠密 float 输出，其他标准 union 不据此准入。

## 语义与表示

[Task](../../src/semantic/task/embedding.rs)拥有有序非空文本批次、维度意图、输入索引、精确数值稠密向量和 operation usage；[envelope](../../src/adapter/embeddings.rs)拥有 public model、实际 reported model、编码意图与 client identity。模型目标、凭据和运行状态不进入 task。

单文本与单元素文本数组等价归一化，不拼接批次或重排文本。结果保留原列表顺序，输入关联由 index 决定；唯一索引必须完整覆盖请求输入。维度一致，显式维度须由每个向量满足；默认维度不从请求补成报告。向量保存精确 JSON Number，不通过 f32/f64 转换改写数值；有限数值范围检查不是量化。插入、替换或删除后的最终请求重新验证，旧结果必须重新检查关联和维度。

Usage 保留实际报告与 basis，未知不补零，也不从文本长度计算 tokens。标准输出要求完整 usage；缺失或 malformed 报告不能伪造成成功。DashScope input-only aggregate 的具名归一化和 ancillary request ID 的标准投影只归[映射合同](protocol-and-lowering.md#独立-embedding-的计量归一化与请求标识投影)。Schema、原始重复 key、数值、关联与累计预算分别由 [codec](../../src/protocol/openai/embeddings.rs)、task 和独立测试维护。

可选控制逐字段保留缺省，null 不准入。本片 `encoding_format` 省略或显式 float 都选择 float；固定 Python SDK 的 Base64 convenience default 不属于该分支，调用方须显式传 `encoding_format="float"`。Base64/float32 转换、token 输入、稀疏/多模态向量和 Batch API 留给独立后续切片。

## 目标准入与执行

[Lowering](../../src/lowering/embeddings.rs)检查最终批次、维度和 identity 的目标准入；显式批次数与维度的乘积超过累计向量硬预算时，在 I/O 前拒绝。不联网、不查 registry、不裁剪控制。[Topology](../../src/topology/embeddings.rs)编译独立 operation、canonical identity、固定单 endpoint 和执行合同；产品参数归 [catalog](../../src/topology/catalog/embeddings.rs)。注册不证明实例激活、账户资格或实际推理。

[Gateway](../../src/gateway/embeddings.rs)复用认证、并发 permit、绝对 deadline、取消及实际 body handoff。完整有界 JSON、必要报告、索引/维度和严格 EOF 全部通过后才发布；任何一项失败都不返回批次前缀，不补发请求，不注入 Generation 输出 token cap。

嵌入方通过 `CompiledTopology::with_embeddings` 编译绑定，再由 `Gateway::new_with_embeddings` 的独立 `EmbeddingEntry` 列表显式激活；旧构造器不启用 Embedding。Binary 要求在 `models` 明确选定标签和匹配的单来源、禁 fallback API-key pool；已有同 Provider 的 Generation/media 池不自动扩张入口。可信路径与来源由绑定拥有，业务 JSON 不选择 origin、账户或 Provider 路由。

百炼的[域名合同](https://help.aliyun.com/en/model-studio/regions#migrate-to-workspace-dedicated-domain)区分现有 DashScope 域名和推荐 workspace 域名；新增 operation 不授权迁移既有 Provider/凭据。文件服务不是 Embedding 主链的前置，也不由“输入可以来自文件”的客户端示例推定网关拥有上传或存储能力。

公开接口与示例归[HTTP 指南](../http-gateway.md#独立文本-embedding)，边界实例归 [semantic tests](../../tests/semantic/embeddings.rs)、[独立绑定测试](../../tests/semantic/embedding_bindings.rs)、[body checks](../../src/gateway/embedding_tests.rs)和既有 [Router smoke](../../tests/gateway.rs)。
