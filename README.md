# MorphieCore

MorphieCore 是 Gateway 与未来 Agent 共用的模型交互 Semantic Model / IR，通过标准 API 连接不同 Provider。Generation 以 OpenAI Responses 为主接口，Chat Completions 为具名有损兼容路径；独立媒体 operation 使用各自 task。

项目处于开发阶段。设计权威见[语义架构](docs/architecture/README.md)，当前接线见[架构](docs/architecture.md)，未完成方向见[后续计划](docs/implementation-plans/next-goal.md)。

## 当前范围

- 共享 Rust 语义库与认证 loopback Gateway；Chat/Responses 的文本、工具、reasoning 与图片输入按具体 profile 和目标准入。
- Responses 支持选定文件输入；标准 Chat 支持 WAV/MP3 语音输入→文本，范围见[音频输入 profile](docs/architecture/chat-input-audio-profile.md)。
- 独立 Images、Speech、Transcription 接口与只读 Models 目录。API 用法和显式激活见[HTTP 指南](docs/http-gateway.md)。
- 独立标准 Embeddings 文本/float 分支，按目标显式激活；范围见[Embedding profile](docs/architecture/embedding-profile.md)。
- 同协议与跨协议共用 validation/lowering；允许的损失见[投影合同](docs/architecture/protocol-and-lowering.md#semantic-loss)。
- 自有文件凭据与显式有序池；Provider 原生缓存亲和。Agent orchestration、会话服务与动态路由不属于当前实现。

当前 Provider/model 与实例准入查源码和 [AGENTS 查询方法](AGENTS.md#current-provider-model-and-compatibility-information)，不在文档维护库存。

## 启动入口

主程序为 `morphiecore`，凭据 CLI 为 `morphiecore-auth`，开发变量使用 `MORPHIECORE_` 前缀。先按[HTTP 指南](docs/http-gateway.md)准备入口，凭据路径与管理见[凭据指南](docs/credentials.md)。

```sh
cargo run --locked --offline --bin morphiecore
```

## 验证

检查方法统一见[开发指南](docs/development.md)。默认检查使用 synthetic 数据；固定 SDK loopback 是独立显式 gate，真实 Provider 使用[受控 probe](docs/probes.md)。授权与安全规则归 [AGENTS.md](AGENTS.md)。

## 文档

[文档索引](docs/README.md)导航当前合同、最终 ADR、未完成计划和固定来源。实现细节归源码与独立测试，执行结果在当次交付中报告，决策过程查 Git。

原创代码和文档采用 [MIT License](LICENSE)；外部材料保留其来源与许可。
