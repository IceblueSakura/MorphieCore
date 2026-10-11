# MorphieCore

以统一 IR 为核心的模型路由网关：下游提供标准 OpenAI Responses（主接口）与 Chat Completions（兼容接口），上游连接 OpenAI 兼容、Anthropic Messages 与 Google Gemini 等不同协议的 Provider。项目处于核心设计与验证阶段。

## 状态

- 已接入：下游 Responses / Chat（JSON 与 SSE，文本、工具、reasoning、图片与部分文件输入）；上游 OpenAI Responses / Chat 及多家兼容方言；文件凭据与有序池；Provider 原生缓存亲和。
- 冻结：独立 Embeddings、Images、Speech、Transcription 入口，以及新 Provider 扩展。
- 进行中：协议层重构、Anthropic 上游接入，随后是 Google generateContent / Interactions 与下游 Anthropic Messages。详见[路线](docs/roadmap.md)。

## 快速开始

```sh
cargo run --locked --offline --bin morphiecore        # 网关
cargo run --locked --offline --bin morphiecore-auth   # 凭据管理
```

配置、接口与凭据操作见[使用](docs/usage.md)。

## 代码结构

| 目录 | 内容 |
|---|---|
| `src/semantic` | IR：任务类型、校验、事件 reducer |
| `src/protocol` | 各协议 wire 编解码与方言规则 |
| `src/lowering`、`src/adapter` | 目标投影与请求/响应适配 |
| `src/topology`、`src/provider` | 模型 → 路由 → 端点绑定，Provider 定义 |
| `src/execution`、`src/transport` | 候选执行、HTTP/SSE |
| `src/gateway` | HTTP 网关 |
| `src/credential` | 凭据 store、API key、OAuth |
| `tests/` | 离线测试与 SDK loopback gate |

## 文档

- [架构](docs/architecture.md)：设计原则与已定决策
- [路线](docs/roadmap.md)：阶段、冻结范围与当前焦点
- [使用](docs/usage.md)：启动、配置、HTTP 接口、凭据
- [开发](docs/development.md)：环境、检查、测试布局
- [参考来源](docs/references.md)：固定版本与协议文档
- 贡献规则：[AGENTS.md](AGENTS.md)

原创代码与文档采用 [MIT License](LICENSE)；外部材料保留其来源与许可。
