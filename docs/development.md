# 开发指南

开发对象为 Rust 语义库、凭据组件、最小 loopback 网关与独立离线验收。启动方式见 [HTTP 指南](http-gateway.md)；真实 Provider probe 不属于默认验证。

## 变更流程

授权、Git 工作区保护、行为切片与 TDD 要求统一见 [AGENTS.md](../AGENTS.md)。按[文档导航](README.md#按任务阅读)定位当前合同、受影响源码/测试与固定协议资料；IR 结构选择按 [IR 缺口规则](architecture/semantic-ir.md#4-ir-不足与标准载体缺口)先报告与定稿，不以 adapter/custom API 绕过。

先运行最低职责层的受影响检查，再按变更类型执行下列基线。Codec 的 decode 与 encode 使用独立预期，覆盖 IR 插入、替换、删除及失败边界；[Chat 有损兼容](architecture/protocol-and-lowering.md#semantic-loss)还须断言允许损失、受保护不变量和静态/事件一致性。Round trip 不能自证或要求恢复已丢失信息。

已有 ClientManaged 场景的语义需求、独立预期与来源由[用例清单](../tests/fixtures/client_managed_cases.json)索引，不在计划里复制回归矩阵。必要 opaque 分别验证“实际报告且回传”与“未报告”，后者不能替代前者；纯库、固定 SDK 与真实 Provider 的验收边界分别报告。

完成时检查最终 diff 与引用，报告实际检查、失败与未验收范围；按当前合同验收，不以旧版功能对等为门槛。

## Nix 开发环境

根 [flake.nix](../flake.nix) 提供可复用的开发环境；`flake.lock` 固定 nixpkgs 修订，不依赖全局 Nix registry 或 rustup。需启用 Nix 的 `nix-command` 和 `flakes`。输出覆盖 `x86_64-linux`、`aarch64-linux` 和 `aarch64-darwin`；其他平台不在当前声明范围内。

```sh
nix develop          # Rust、Node、Python、uv 和 Nix 格式化工具
nix develop .#rust   # 仅 Rust 工具链、rust-analyzer 与原生编译依赖
nix flake check      # 当前平台的工具可执行性与 Node 版本检查
nix fmt -- --check flake.nix
```

两个 shell 共享 Rust/Cargo、clippy、rustfmt、源码路径与 C/C++ 编译环境，并提供 `aws-lc-sys` / `ring` 所需的 CMake 和 pkg-config。默认 shell 的 Node 必须与 `package.json` 的精确版本一致；不匹配时求值失败，应显式维护环境锁，而不是放宽项目版本约束。

Nix 提供 Python 3.13 分支供一般脚本使用；SDK gate 的精确补丁版本仍归 `tests/sdk/.python-version`，由 uv 按下文管理，不保证与 shell 的 `python3` 补丁版本相同。进入 shell 不执行 Cargo/npm/uv 依赖安装，不读取私有配置、不登录、不启动 Gateway，也不运行 probe。首次取得 Nix 工具可能需要下载；语言依赖继续使用各自锁文件，离线检查仍需预先准备缓存。

可将检查作为一次性命令执行，例如 `nix develop --command cargo fmt -- --check`；环境也可从其他目录通过 `nix develop /path/to/MorphieCore` 复用。Git 工作树中的 flake 只包含已纳入 Git 的文件，新增 `flake.nix` / `flake.lock` 须由操作者纳入版本管理后再使用，不使用 `path:.` 将整个工作目录（含私有配置）复制到 Nix store。

可选自动切换使用根 [.envrc](../.envrc)：在系统配置好 direnv 与 nix-direnv 的 shell 集成后，审核文件并显式执行 `direnv allow`；该动作不由项目自动执行。缓存写入被忽略的 `.direnv/`。

工具链更新单独执行 `nix flake update nixpkgs`，审阅 `flake.lock` 差异后运行 `nix flake check` 和受影响的语言基线。Nix 工具检查不代替下文的 Rust、TS 或显式 SDK gate，也不证明其他平台的运行结果。

## Rust 检查

Rust/Cargo 使用 stable 工具链与配套 rustfmt/clippy；可由锁定 nixpkgs 等声明式开发环境提供，不要求 rustup。根 `rust-toolchain.toml` 为识别该文件的环境声明 stable，原生 Nix Cargo 不通过它选择版本；实际工具链由开发环境锁定。工具链更新是显式环境维护，不与普通测试捆绑；验证时报告实际版本，不为普通项目检查切换全局工具链。

集成测试按以下入口分工，按职责筛选，不按实现批次增加 binary：

| Target | 模块与边界 |
|---|---|
| `semantic` | `tests/semantic/`：纯语义、codec/lowering、adapter 隔离、编辑失效与 replay 保真；纯 codec 预期不依赖产品 catalog，绑定准入另行检查 |
| `credential` | `src/credential/` 的 manager/driver/store 单测与 `tests/credential.rs` 的独立 CLI/进程边界；只用 synthetic authority/账户文件，不依赖 Gateway。签名 fixtures 均为本地合成密钥 |
| `transport` | `tests/transport/`：framing、Responses/Chat SSE、Chat envelope、body lifecycle；基础 framer、增量 Attempt/ResponseDelivery、实际 I/O commit 边界和 synthetic body I/O 各自验证 |
| `gateway` | 一个真实 Router→synthetic HTTP Provider smoke；另一个隔离环境 binary bootstrap gate，使用 synthetic keys 与拒绝出站的 loopback 代理，不调用真实 Provider |
| `sdk_loopback` | 显式 ignored 的固定 Python SDK codec fixture gates，以及真实 Gateway Router→synthetic Provider 的双协议 JSON/SSE 续轮 gate，不进入默认外部依赖检查 |

`tests/support/` 只共享必要的 synthetic builders 和独立 wire 预期，不从被测 encoder 生成 oracle。测试按责任层组织：凭据匹配检查不构造整座 Gateway，codec 检查不查询模型 catalog，候选/route 检查归 execution/topology。保留默认 topology 的编译检查，不复制逐模型清单。删减重复 smoke 或快照前，确认独立的语义、安全、资源和交付反例仍有覆盖；不以测试数量为目标。

先运行受影响模块，例如：

```sh
cargo test --locked --offline --test semantic reasoning::
cargo test --locked --offline --test transport
```

再执行完整基线（clippy 的 `--all-targets` 也检查测试与 helper）：

```sh
cargo test --locked --offline
cargo clippy --locked --offline --all-targets -- -D warnings
cargo fmt -- --check
git diff --check
```

`--offline` 需要预先可用的依赖缓存；不要为离线检查隐式调用 Provider。修改依赖后同步 `Cargo.lock`，再重跑 locked 检查，避免顺带升级无关依赖。

## 测试语言与 JS 工具

IR、codec/lowering、资源和生命周期测试保留 Rust，直接验证真实类型和 owner；独立 wire 预期不由被测 encoder 生成。Python 验证 Python SDK 的解析/派生视图与 probe 控制层；JS/pi 消费端使用 TS。不要为跨语言测试增加第二套 IR 或预算状态机。CLI 黑盒仍在 Cargo 默认 gate，不能因迁移丢出常规检查。

Node 版本和 TS/pi 类型依赖由根 [package.json](../package.json) 与锁文件固定；首次准备依赖：

```sh
npm ci --ignore-scripts --no-audit --no-fund --engine-strict
npm run typecheck
npm test
```

已有缓存时可给 `npm ci` 加 `--offline`。Node 原生执行 TS 不等于类型检查，两个命令均需通过。Node 单测只用 synthetic 数据，不导入 pi runtime 或联网；实际 pi synthetic/live gate 仍须显式运行。更改工具或锁文件必须进入 probe 源码指纹。子进程有界收集 stdout/stderr，失败时终止并回收，不以日志缓冲代替 deadline。

## 固定 OpenAI SDK loopback

Python 版本由 `tests/sdk/.python-version` 固定；SDK 与测试依赖在 `tests/sdk/pyproject.toml` 声明，全部传递依赖和下载 hash 由 `tests/sdk/uv.lock` 固定。环境只安装到被忽略的 `tests/sdk/.venv/`，不向系统 Python 安装 package；依赖由 uv 管理，不再额外安装 pip。不要用未锁定的安装命令让环境偏离锁文件。

Python 分支选择优先生态兼容性，不要求跟随最新 minor；优先采用所选受支持分支的最新正式补丁。变更分支须同步约束与锁文件，并重新执行 Python/SDK 验收，不能仅凭版本较旧就宣称兼容性更好。

公开 OpenAPI 共用 `tests/sdk/test_openapi_schema.py`：使用锁定环境中的 `jsonschema` 一次检查全部 component schemas、内部引用及各 operation 的独立合法/非法实例：

```sh
uv run --project tests/sdk --locked --offline python -m unittest discover -s tests/sdk -p test_openapi_schema.py
```

Schema 不证明运行时字节边界：Base64 内容/扩张、集合累计预算、请求/响应数量关系、精确数值、真实 multipart framing/重复字段、报告完整性、EOF、准入和取消仍归 Rust；`contentEncoding` 注解不等于二进制验证。固定 SDK 的同一个 Gateway gate 覆盖 Images、Speech eager/streaming-response 与 synthetic WAV 消费，以及 native TTS/ASR 映射，不额外复制生产 Router smoke，也不证明低延迟、真实质量或 Provider 准入。

完整控制/目标值矩阵归最低 codec/admission owner；SDK 层保留客户端特有的序列化、消费、续轮与错误分支，不按交付模式或模型标签重复相同检查。列表一次验证全部激活 metadata，retrieve 使用代表性标签；各实现独立的生命周期和安全边界不能按表面相似度合并。

首次准备需要依赖下载；已有缓存可为 sync 加 `--offline`。若 uv 的下载索引尚不包含固定 Python 版本，使用临时新版 uv 或准备匹配的解释器，不通过降低 `.python-version` 或取消锁定来绕过：

```sh
uv sync --project tests/sdk --locked
uv pip check --directory tests/sdk --offline
uv run --project tests/sdk --locked --offline cargo test --locked --offline --test sdk_loopback -- --ignored --test-threads=1
```

[SDK target](../tests/sdk_loopback.rs)包含 codec fixture 和真实 Gateway→synthetic Provider 两类边界。前者检验固定 SDK 对完整 JSON/SSE、工具/derived view/opaque history 的消费与回放；后者检验认证、可信模型绑定、预算、实际 HTTP body 和续轮接线。预期 wire 独立编写，不由被测 encoder 或模型正文推导，也不从缺失报告补造 SDK 字段。

SDK 场景使用不会被 Python 优化模式移除的显式检查；普通/优化模式的错误结果反例归 Python 单测。所有 listener 都是临时 literal loopback，凭据与工具结果均为 synthetic；禁止环境代理、真实 Provider、自动重试和私有配置读取。进程必须有 deadline 与取消清理。这些 gates 不证明一般 SDK/Agent、网络、负载或生产兼容性；更细的场景以 owning tests 为准，不在指南复制矩阵。

## 受控真实 Provider gates

真实调用授权归 [AGENTS.md](../AGENTS.md#standing-authorization-for-live-provider-verification)；统一入口、计划、共享账本、执行预算、原调用诊断和具名场景归 [probe 指南](probes.md)。

离线回放仅处理明确指定的获准 capture，不读凭据、不联网，也不把同源 capture 当作独立 fixture。使用入口前检查 [library probe](../examples/live_probe.rs) 的 replay 分支或 [replay CLI](../examples/replay_chat.rs) 的实际参数范围。原始正文捕获默认关闭；诊断和报告只保存白名单元数据，不能记录 credentials、正文或 opaque 值。

## pi 探测诊断守卫

[正文观察器](../examples/provider_probe_observation.ts)只在内存中比较同一次响应，输出有界分类；[header 守卫](../examples/probe_http_headers.ts)不修补客户端认证。独立 Node 防线与显式 pi 运行方式见 [probe 指南](probes.md#离线验证)。客户端认证拒绝与正确认证后的出站拒绝是不同边界，不能用注入正确 token 的中继掩盖错误。

## 文档与边界

内容归属与写作规则见[文档索引](README.md#写作与维护)，注释语言与安全要求见 [AGENTS.md](../AGENTS.md)。仅修改 Markdown 时执行以下检查，不运行与改动无关的 runtime tests：

1. **结构与职责**：标题层级、入口导航与内容 owner 一致；没有丢失有效约束、未决问题或必要来源，也没有把设计写成已实现能力。
2. **链接与锚点**：检查修改页面及其入链的相对路径、标题锚点和显式 anchor；移动或合并内容后同步更新引用。外部来源未重新核验时保留既有日期并说明边界。
3. **示例与合同**：对照 owning CLI、配置或 Schema 检查命令和占位符；使用合成值，不为验证示例执行登录、服务启动、真实调用或读取私有配置。
4. **最终差异**：检查完整 diff，确认只有授权范围内的修改，然后执行：

   ```sh
   git diff --check
   ```

结构性检查不证明运行时或 Agent 行为改善。完成时区分静态检查、Rust tests、固定 SDK/loopback、真实 Provider、负载和生产验证；默认基线不包含真实 Provider、付费 API、部署或外部发布。
