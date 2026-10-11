# MorphieCore Agent Instructions

## 项目与权威来源

- 基于统一 IR 的模型路由网关。下游先完善 OpenAI Responses（主）与 Chat Completions（兼容），之后开放 Anthropic Messages；上游覆盖 OpenAI Responses/Chat、Anthropic Messages、Google generateContent 与 Interactions。
- 设计原则与已定决策：[docs/architecture.md](docs/architecture.md)。方向、冻结范围与当前切片：[docs/roadmap.md](docs/roadmap.md)。
- 代码与测试是行为的权威；文档只记录概念、决策与方向。可用的 Provider / 模型 / 绑定直接查 `src/provider/catalog.rs`、`src/topology/catalog*`、`src/gateway/bootstrap.rs`，不在文档或记忆中维护清单。

## 授权

- 编辑前查看分支、`git status` 与目标 diff；保留他人修改，与外部修改重叠时停下。
- 未经明确授权不 commit / push、不部署、不发布，不执行凭据生命周期操作（登录、刷新、撤销、迁移）。
- 当前开发任务所需的真实 Provider 调用已获授权，无需逐次确认、无金额上限。每次运行须事先界定目标、请求数、输出上限与超时，串行执行、无隐藏重试，只经项目凭据 loader 取凭据，只输出脱敏的状态与计数。
- 在 roadmap 已批准的阶段内允许大范围删除和破坏性重写，不保留兼容 shim。冻结范围只做重构所需的维护。

## 安全

- 凭据只通过项目 loader 访问（默认 store 见 `src/credential/directory.rs`）。不读取 `.env`、私有 `config/`、OAuth 文件或第三方 auth cache 作为调查或测试输入。
- 真实密钥、私有配置、敏感正文与 opaque 值不得出现在源码、fixture、日志或工具输出中；不打印环境变量值。
- 入口只监听 loopback 且鉴权 fail-closed；业务数据不能选择上游 origin、凭据或认证/代理 header；凭据 locator 与受信 origin 不下发给客户端。
- 内存、帧、事件与时间都有上界；取消必须释放上游；提交后不重放、不伪造成功终态。

## 工程原则

- **一个 IR**：`src/semantic` 不依赖协议、Provider 或执行层；codec 与 lowering 是纯函数，不访问网络、凭据或 registry。运行时目标、凭据、重试状态不进入 IR。
- **协议差异留在协议模块**：不给 IR 加 Provider 专属字段，不引入 raw JSON 请求袋。发现 IR 无法表达的概念时，先向用户说明最小反例和结构选项，再实现。
- **解析策略**：上游响应已知字段严格、未知字段忽略；下游请求中无法表达的关键语义拒绝，可降级字段按具名规则移除并记录（见架构文档第 4 节）。
- **保持简单**：不预建通用框架、多余防御层或推测性抽象；删除不再需要的代码，而不是保留旧路径。新增检查和上限应有具体失败场景支撑。
- **流程**：行为变更先在 roadmap 当前焦点写明目标与退出条件，测试先行。依赖变更同步 `Cargo.lock` 并用 `--locked` 验证。
- Rust 注释与 Python docstring 使用简洁英文，只解释非显然的原因，必要时附来源 URL。
- 不手动查看或编辑 `target/`、`.venv/`、`__pycache__/`；不提交 `testdata/{generated,reports,dist,runtime}/`。

## 验证

- 默认检查离线、只用 synthetic 数据（命令见 [docs/development.md](docs/development.md)）：先跑受影响的目标，再跑完整基线 `cargo test`、`cargo clippy --all-targets -D warnings`、`cargo fmt --check`、`git diff --check`。
- 协议映射需要独立手写的 wire→IR 与 IR→wire 预期；round trip 不能单独作为证据。测试放在最低负责层，不按 Provider 或模型数量堆叠重复用例。
- SDK loopback gate 与真实 Provider 调用是独立的显式步骤，补充而不替代离线回归。
- 汇报实际执行的检查、失败与未覆盖的层；编译或 synthetic 通过不代表真实 Provider、SDK 或生产可用。

## 文档

- 文档集合固定为：`README.md`、`AGENTS.md`、`docs/architecture.md`、`docs/roadmap.md`、`docs/usage.md`、`docs/development.md`、`docs/references.md`、`docs/openapi.json`。不新增分析页、执行报告或历史记录（历史查 Git）。
- 完成的切片从 roadmap 删除；新决策写进架构文档的决策表。
- 只改 Markdown 时检查链接并运行 `git diff --check`。
