# 开发

## 环境

[flake.nix](../flake.nix) 提供开发环境（`flake.lock` 固定 nixpkgs），无需 rustup：

```sh
nix develop          # Rust 工具链、Python 3.13、uv
nix develop .#rust   # 仅 Rust
nix flake check
```

也可配合 direnv 使用根目录 `.envrc`（需手动 `direnv allow`）。工具链更新单独执行 `nix flake update nixpkgs` 后跑完整基线。

## 基线检查

```sh
cargo test --locked --offline
cargo clippy --locked --offline --all-targets -- -D warnings
cargo fmt -- --check
git diff --check
```

先跑受影响的目标，例如 `cargo test --locked --offline --test semantic reasoning::`，再跑完整基线。默认检查只用 synthetic 数据，不访问真实 Provider，不读取私有配置。

## 测试布局

| Target | 范围 |
|---|---|
| `src/**` 单元测试 | 模块内部逻辑：凭据 manager/store/driver、gateway 各入口、affinity 等 |
| `tests/semantic` | IR、codec、lowering、replay、编辑失效 |
| `tests/transport` | SSE framing、各协议流解码、body 生命周期 |
| `tests/credential` | `morphiecore-auth` 黑盒（synthetic 文件与 authority） |
| `tests/gateway` | 真实 Router → synthetic Provider 冒烟，以及隔离环境的 binary 启动 |
| `tests/sdk_loopback` | 显式 ignored：固定版本 OpenAI Python SDK 对 Gateway 的消费与续轮 |

编写协议映射测试时，wire→IR 与 IR→wire 的预期分别独立手写，不用被测 encoder 生成 oracle；round trip 只能作补充。共享的 synthetic builder 放在 `tests/support/`。

## SDK loopback gate

Python 版本由 `tests/sdk/.python-version` 固定，依赖由 `tests/sdk/uv.lock` 锁定，环境装在被忽略的 `tests/sdk/.venv/`：

```sh
uv sync --project tests/sdk --locked
uv run --project tests/sdk --locked --offline cargo test --locked --offline --test sdk_loopback -- --ignored --test-threads=1
uv run --project tests/sdk --locked --offline python -m unittest discover -s tests/sdk -p 'test_*.py'
```

所有 listener 都是临时 loopback，凭据为 synthetic，不使用环境代理或真实 Provider。

## 真实 Provider 验证

授权范围见 [AGENTS.md](../AGENTS.md)。当前没有常驻的 probe 工具：需要时在 `examples/` 写最小入口，通过项目凭据 loader 取凭据，明确请求数、输出上限与超时，串行执行、无隐藏重试，只输出脱敏的状态与计数，不保存正文或 opaque 值。真实调用是离线测试的补充，不能替代独立的 synthetic 回归。

## 文档

文档集合与维护规则见 [AGENTS.md](../AGENTS.md#文档)。只改 Markdown 时检查链接并运行 `git diff --check`。
