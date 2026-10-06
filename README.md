# MorphieCore

MorphieCore 建立**可由 Gateway 与未来自研 Agent 复用的模型交互 Semantic Model / IR**，以尽量低的语义损失连接不同 Provider，并向下游提供稳定的标准 API。项目尚未上线；优先稳定概念、所有权与不变量，不冻结当前 Rust 类型或照搬协议 DTO。

Generation 主线是 Agent-first 的 Text/Image/File 交互，以规范 Responses 为主要接口；Chat Completions 仅作允许声明损失的兼容路径。独立多模态 task 不强塞 Responses。有效合同归[语义架构](docs/architecture/README.md)，推进顺序、文件扩展延期及后续媒体范围统一归[后续计划](docs/implementation-plans/next-goal.md)，不代表当前能力已经扩大。

项目展示名为 **MorphieCore**；Rust crate 与主程序为 `morphiecore`，凭据 CLI 为 `morphiecore-auth`。开发工具环境变量统一使用 `MORPHIECORE_` 前缀。

## 当前范围

当前工作区包含 Rust 语义库、统一文件凭据管理器与最小认证 loopback 网关，不是完整标准实现或生产就绪服务。

- HTTP 入口为 Chat Completions / Responses，提供受限的无状态文本输出与选定 URL/inline 图片输入。库与可嵌入 Gateway 另有标准 Responses inline/URL 文件承载，但仍需模型/目标显式准入，不由 codec 推定启用。工具图片结果有独立准入，不能由 user 图片支持推定；协议、模型与实例启用分别核查。
- 标准 Models 列表/查询仅公开本实例已激活 public labels；字段来源、嵌入准入与无微调模型删除权限的边界见[模型发现](docs/http-gateway.md#标准模型发现)，不请求上游目录或承诺实际推理可用。
- Gateway 另有显式绑定的标准 `/v1/images/generations` 静态图片生成切片，使用独立 ImageGeneration task 和有序 inline 产物集合；图片绑定须显式选择并配置凭据池，不随已有 Chat/Responses 默认启用，入口与边界见 [HTTP 指南](docs/http-gateway.md#独立图片生成)。
- 库与 Gateway 支持独立 `/v1/audio/speech` 的有界二进制 TTS 与 `/v1/audio/transcriptions` 的文件上传/JSON 分支，binary 仅激活显式选定且配置匹配凭据池的音频绑定，不因已有对话凭据自动启用；不含下游音频 SSE、Realtime 或声音资源服务，入口见 [Speech](docs/http-gateway.md#独立语音生成)与[Transcription](docs/http-gateway.md#独立语音识别)。
- 同协议与跨协议都经过共享 IR、验证和目标可表示性检查，再按 operation 交付 JSON、SSE 或二进制；不可表示的语义明确拒绝，不承诺任意无损转换。
- 凭据只从操作者指定的自有文件加载；显式池策略允许受预算约束的提交前 fallback，不提供普通请求内登录、自动 refresh、负载均衡或会话管理。
- 缓存亲和利用 Provider 原生功能和声明的 carrier，不实现网关回答缓存；前缀稳定不证明命中或收益。

当前模块接线见[架构](docs/architecture.md)，尚未闭合的语义、表示、执行与验收范围见[Generation 缺口](docs/implementation-status/generation.md)。推进方向由[next-goal](docs/implementation-plans/next-goal.md)维护，获准行为切片由[current-focus](docs/implementation-plans/current-focus.md)维护，等待证据或语义决策的问题归[待决状态](docs/implementation-status/open-questions.md)；设计或计划不授予操作权限。

## 启动入口

语义库构造不读取私有配置。`morphiecore` binary 从[默认或显式覆盖的自有凭据目录](docs/credentials.md#自有文件目录)和入口配置启动；命令本身不发生成请求：

```sh
cargo run --locked --offline --bin morphiecore
```

先按[HTTP 指南](docs/http-gateway.md)准备入口配置，账户与池操作见[凭据指南](docs/credentials.md)。真实登录、推理或付费测试需独立授权，使用[受控 probe](docs/probes.md)，不属于默认检查。

## 验证

检查命令、工具链与依赖准备统一见[开发指南](docs/development.md)：[Rust 基线](docs/development.md#rust-检查)、[TS 工具](docs/development.md#测试语言与-js-工具)、[固定 OpenAI SDK loopback](docs/development.md#固定-openai-sdk-loopback)和[文档检查](docs/development.md#文档与边界)。SDK loopback 单独显式运行，真实 Provider probe 不属于默认检查。

检查结果只在当次交付中报告；synthetic 执行不证明真实 Provider、一般 SDK/Agent、网络、负载或生产兼容性。

## 文档

实现事实优先由源码、邻近注释和独立测试维护。当前 Provider/model 与实例准入按 [AGENTS 查询方法](AGENTS.md#current-provider-model-and-compatibility-information)现场核对，不在 Markdown 或记忆中维护库存及测试结果。

[文档索引](docs/README.md)区分设计合同、实现缺口、操作指南和[固定来源](docs/references/README.md)。项目只维护一套当前架构，不保留平行的历史架构入口。

原创代码和文档采用 [MIT License](LICENSE)。外部资料保留各自来源与必要 attribution。
