# 受控 Provider / SDK / pi 探测

这些是显式授权的开发工具，不是生产重试器、压测器或额外协议框架。目标、默认值和暂停项读取入口代码及 `examples/probe_support/catalog.py`；它们不等于产品支持集合，查询方法见 [AGENTS.md](../AGENTS.md#current-provider-model-and-compatibility-information)。存在 key、创建计划或历史成功都不授权新的调用。结果只在当次交付与授权的 ignored run 目录中保存，不再写 Provider 验收页或当前模型清单。

## 先计划，后执行

通用对话 runner 要求可发送的上游输出 token cap，因此不接入 SIWC；不能通过删掉 cap 或重命名旧产品测试项绕过预算合同。SIWC 真实 gate 需另行定稿请求数、时间/资源预算和无法保证上游停算/费用的边界，再取得具体授权。

全部自动 live 入口共享同一个 run 目录和 SQLite 账本。先查询并设置非敏感的 `PROVIDER_ID`、`PUBLIC_MODEL`（不是凭据），显式选取本次目标，不依赖默认模型。计划创建及 dry-run 不读取凭据、不启动服务、不联网：

```sh
uv run --project tests/sdk --locked --offline python examples/probe.py plan \
  testdata/runtime/my-run --providers "$PROVIDER_ID" --model "$PUBLIC_MODEL" \
  --limit 16 --tokens 2048 --continue-oracle
uv run --project tests/sdk --locked --offline python examples/probe.py run \
  testdata/runtime/my-run --model "$PUBLIC_MODEL" --cases json --dry-run
```

目录必须不存在；不覆盖旧计划。`plan.json` 的 hash 与账本绑定，修改预算/目标后不能继续消费原 run。公开 plan view 是副本；进程间 reservation 使用 SQLite 事务。发送前预留额度，已预留或 dispatched 后中断的请求仍计入预算，不能自动退款或重放同一 scenario。计划有效期 24 小时；过期后禁止新发送，但仍可读取报告和完成已预留请求的记录。

授权覆盖这一具体批次后，显式运行：

```sh
cargo build --locked --offline --bin morphiecore
uv run --project tests/sdk --locked --offline python examples/probe.py run \
  testdata/runtime/my-run --live --model "$PUBLIC_MODEL" --cases json
uv run --project tests/sdk --locked --offline python examples/probe.py report testdata/runtime/my-run
```

- Provider 只取固定 catalog 中的绑定。真实执行须用 `MORPHIECORE_PROBE_CREDENTIALS_DIR` 指定已配置的自有 JSON 目录；该变量仅含路径，不含 key 或账户选择。Gateway probe 将选定 models、临时入口 token 和 `max_attempts: 1` 写入 run 下的私有配置，binary 独自加载上游凭据；即使 store 的 pool 开启 fallback，probe 也不隐式多发请求。无 TOML/env key 回退或第三方 auth-cache 搜索。库级 probe 只读取已配置 API-key 池的首项快照，不自动 fallback/refresh。订阅 Provider 仍须明确选择。实际 SDK 客户端只获得临时 gateway token；上游凭据不复制进 run 或进程环境。
- `plan --model` 可重复，将所选 Provider 缩小到精确模型子集。重复、未知或不属于所选 Provider 的模型在读取凭据前拒绝。省略模型筛选则包含所选 Provider 的全部已登记测试绑定，不能将此默认扩大解释为授权。
- `run --model` 可重复，必须属于计划；`--protocol chat|responses`、`--delivery json|sse` 缩小范围。`--effort none|minimal|medium|max` 是明确请求控制，不自动改默认。
- cases：`text`、`json`、`tool`、`history`、`length`、`cancel`、`image`、`image_math`、`file`、`file_url`、`file_continue`、`file_replay`、`file_reasoning`、`file_reasoning_math`；`reasoning` 的目标限制读取 `examples/probe_support/scenarios.py`，不得套用到任意模型。
- `image` 每个协议/交付一请求，使用程序生成的两张无敏感 PNG 与交错文本，oracle 检查按图片顺序返回颜色；最多 512 输出 token，可显式 `--effort none`。不下载图片、不使用账号文件资源、不保存图片或正文；模型准入必须按 catalog 现场查询。独立 PNG 像素/预算守卫在 `tests/sdk/test_probe_core.py`，此场景不证明一般视觉理解质量。
- `image_math` 每个协议/交付一请求，先从两张程序生成的方块图获得视觉计数，再计算固定算术式，以严格 JSON 数值 oracle 验收；预算取 run 的 token cap（最多 2048）。图片像素、计数和算术预期由独立离线检查保护。可用相同输入分别选取 `--effort none|minimal|medium|max`；档位的真实语义和支持按官方页面现场核对，不从名称推导强度排序。
- `file` 的精确目标限制由 `scenarios.py` 与 catalog 维护，仅 Responses；每个交付两请求（首次提取 build marker、显式回传实际 output 与原始文件后提取 patch marker），JSON/SSE 合计四请求。使用程序生成的一页有效 PDF（小于 16 KiB）、最多 512 输出 tokens，marker 只在文件内，不在 prompt 提供答案；沿用共享账本、零重试和正文禁存。场景与文档结构的独立预期归 `tests/sdk/test_probe_core.py`，不证明一般 PDF 质量或扩大文件 URL/ID 准入。
- `file_url` 仅明确选定的 Responses 目标，每种交付两请求、最多512输出tokens：读取公开W3C测试PDF的可见文本，再保留原URL与实际output续轮回答词数。来源/许可归[资源参考](references/multimodal-and-resources.md#5-后续验收的最小单元)；probe不上传/发布文件，Gateway不下载，外部PDF仅用于有授权的live场景。此检查不能证明上游确实下载而非缓存/先验知识回答，也不证明远端内容永久不变。
- `file_replay` 每组两请求：SSE 首轮提取 build marker，随后将原始文件和实际 output 回传至 JSON 续轮提取 patch marker。只选 Responses/SSE 组（第二次交付固定 JSON），最多 512 输出 tokens，首轮失败停止；opaque 未报告时不宣称已验证 opaque 回传。MorphieCore 不 retry/fallback，聚合商内部路由策略不由本地账本保证。
- `file_reasoning_math` 沿用下述三轮/预算与opaque门槛，但先从PDF marker提取数字相乘，再分别加回文件中的两个数字；每轮严格JSON整数oracle，不在prompt提供答案，用于区别简单摘录不产生reasoning与协议拒绝。
- `file_reasoning` 固定目标由 `scenarios.py` 限定，显式 medium reasoning/include，每组三请求 SSE→JSON→SSE、每次最多1024输出tokens；原文件和实际output原样追加。每轮必须报告非空opaque且文件marker正确，缺值不算通过，失败停止；只保存计数不保存密文。该有限场景不证明任意文件或多轮可靠性。
- `file_continue` 使用明确的客户端自有 synthetic 历史（PDF、已知 build marker 的 assistant message、patch 查询），每个交付一请求、最多 512 输出 tokens。它用于隔离续轮问题，不冒充实际上游 transcript，也不能替代 `file` 的真实 output 回传门槛。
- `history` 每个交付四请求，两个实际 lookup 调用/返回；`length` 是 8-token Chat 截断，只有这一场景接受 length；`cancel` 是 Chat SSE 提前关闭和后续普通请求，不证明 Provider 停算/停止计费。
- 默认串行、SDK 零重试，精确限制目标 origin/port/path、model、请求大小、输出 cap 与完整序列化历史。不会因省略 filter 而跳出 run 的模型集合。
- 所有组在 I/O 前登记；未执行的请求显示 `not_run`，中断的 reservation/dispatched 不算通过。HTTP/传输/wire/配置错误停止该目标；工具链失败跳过其依赖轮次。只有计划明确 `--continue-oracle` 时，内容/预期终态失败后才继续独立组。不自动重复失败请求。
- `source_fingerprint` 标记计划创建及各进程预留时的源码/锁文件状态，进程内检测源码变化后拒绝新预留。它不是对正在运行的 binary 的密码学证明；Rust 改动后必须按上述命令重新构建。

## 独立图片生成 probe

图片计划与对话计划分开，不把图片输出套入文本 token cap。`plan --task images` 要求明确 Provider/model 选择，不接受 `--tokens`；图片计划版本 2 使用 `tokens: null` 和显式 `images_per_request`（缺省 1），沿用共用 SQLite 预留、dispatch、停止与源码指纹检查。每次预留占用计划规定的图片数量，整批图片上限为 request limit × images_per_request；实际发送数量必须匹配计划。版本 1 的对话预算和已有账本不变；不同 task 的计划不能互相消费。

```sh
uv run --project tests/sdk --locked --offline python examples/probe.py plan \
  testdata/runtime/image-run --task images --providers "$PROVIDER_ID" \
  --model "$PUBLIC_MODEL" --limit 3
uv run --project tests/sdk --locked --offline python examples/probe.py run \
  testdata/runtime/image-run --model "$PUBLIC_MODEL" --cases image_generate --dry-run
```

公开 Images API 的参数扩展不自动扩大既有付费 probe 矩阵；质量、尺寸、moderation 等新控制须另定场景，现有发送守卫仍拒绝它们。`image_generate` 是缺省、显式单图非流式、null 默认值三种独立请求；可用 `image_generate_minimal`、`image_generate_explicit`、`image_generate_nullable` 分别选择。只有 `images` 协议与 JSON 交付；不使用 reasoning、图片编辑、更多模型或自动 retry。真实发送仍需该矩阵授权、显式 `--live`、已配置的凭据目录与当前 binary；不把 plan 创建当作授权。不设 token cap 或金额 cap 不等于没有 request/image 数量边界，也不是远端费用硬限制。

多图使用独立新计划和显式场景，不扩大旧三场景：`plan --task images --images-per-request 2 --limit 1` 配合 `run --cases image_generate_pair`，其余目标参数同上。该场景一次请求两张图，控制仅为 `n=2, stream=false`；单图计划不能发送它，两图计划也不能消费旧单图场景。工具暂不提供其他数量的场景，不遍历 1–10。

固定 SDK 消费到完整有界 JSON 后，oracle 在内存中逐图校验 Base64、PNG chunk CRC、严格 EOF、有界 zlib 与 8-bit 非交错 RGB/RGBA 像素，集合 decoded bytes 总预算固定为 2 MiB。数量必须匹配计划，任一项失败均使整次失败。固定色块提示只检查每项目标色像素比例，不证明准确几何形状、美学质量或一般图像能力；未覆盖的编码明确失败，不冒充完整解码。不保存 PNG、Base64 或原始响应，只报告实际数量、累计字节数、共同尺寸（若一致）、解码与色彩判据结果。

受控 Gateway 诊断另记录 Images 的实际 input/output token、精确 USD 金额文本及具名投影省略标志（仅在确有报告时）。它们与客户端可见 usage 分开，不从省略字段推定上游未报告；正文和像素不进入该 sink。未知费用不补零，计量与投影失败也不能变成生成成功。

## 共用执行层与结果解释

`examples/probe_support/` 分担 catalog、显式 checks、ledger、SDK collectors、raw wire、listener/send/report 生命周期及固定 scenarios。具名入口只选择场景，不各自实现预算、清理或验收。没有动态插件、业务 transformation 或新 task schema。

实际网络 send 强制流式读取，即使请求 JSON，也先检查 raw 字节预算再交给 SDK。SSE 观察有独立 frame/event/text 上限；终态所在 chunk 保留到真实 HTTP EOF 后再交给会在 DONE 停读的 SDK，防止尾随数据被藏起来。错误使观察状态不可恢复。观察器只针对网关输出，不取代 Rust codec 或完整 SSE 标准验证。

结果分列：`sdk_consumed`、`wire_closed`、`terminal`、`content_ok`、`history_ok`。SDK 成功解析不自动证明 wire 闭合；HTTP 200 不代表生成完成；`length` 不转成 stop。`oracle_failure` 只保存封闭分类，视觉算术的 JSON 格式失败与数值不符分别记录；不会保存异常消息或模型正文，也不会通过解析代码围栏来放宽 oracle。严格文本/JSON/工具 oracle 使用显式异常检查，`python -O` 不会绕过。工具结果要求实际调用和关联正确，并满足明确返回值，不用子串出现代替语义正确。stdout 和持久报告不保存正文、参数、reasoning、opaque 或 credential。

`reported_input_tokens`、`reported_output_tokens`、`reported_reasoning_tokens`、`reported_image_tokens`、`reported_cached_tokens` 仅取 SDK 消费到的实际 reported usage；缺省/null 为未知，不补零，不从可读文本长度估计。非成功 terminal 携带的 actual usage 同样可记录，但不因此标记成功。`reasoning_chars` 按 Chat/Responses 各自 owner 统计可读视图（含 summary/content），排除 opaque 值且避免重复载体计数；它不是推理 token 的替代品。对照时分别报告协议、交付、场景、终态和 oracle。单次耗时包含网络、缓存、生成和消费影响；顺序执行的小样本不能证明档位因果排序、一般模型质量、缓存收益或计费差异。

`ledger.sqlite3` 是权威账本，`summary.json` 是原子替换的派生视图。状态为 `reserved/dispatched/passed/oracle_failed/failed/cancelled/not_run`；取消是预期测试动作，不是成功生成终态。报表字段使用封闭枚举和有界数值，不能借 arbitrary error/message/header 字段保存私有值。

## 原调用诊断

见 [HTTP guide](http-gateway.md#操作者诊断)。runner 为每个 owned binary 创建新的、私有 `gateway-*.jsonl` 文件，以 `x-morphiecore-probe-id = run-id:attempt` 关联同一次请求。诊断在入口认证后才开始，且不向客户端回显。

可观察字段限于最后阶段、完成/中断结果、上游 HTTP、规范化 Retry-After、接收/已 handoff 字节、耗时、固定 decode 失败分类和成功消费的语义事件结构计数。分类不输出未知字段名、异常消息或正文；事件计数不含身份、文本、reasoning、opaque 或认证值，不证明产物完整或终态闭合。`upstream_head_ms`、`first_upstream_bytes_ms` 从认证后的请求处理开始计时，不是 TTFT 或 Provider 纯推理时间；handoff 不等于客户端收到。静态 JSON 的解析可发生于 `terminal`（intake EOF finalize）阶段。

有界队列满、文件预算满、写失败或强杀均可能缺少诊断，缺失必须记为未知，不据此猜测上游状态。原 HTTP 错误映射、取消和交付策略不变；这不是生产可观测性系统，也不因此启用任何自动 retry/backoff。

## 具名入口与库级对照

以下入口保留有意义的场景选择，统一要求 `MORPHIECORE_PROBE_RUN=<existing run>`：

| 入口 | 显式 gate / 场景 |
|---|---|
| `examples/live_nvidia_probe.py` | `MORPHIECORE_NVIDIA_PROBE=1`；目标专用边界场景，选择项读取代码 |
| `examples/live_provider_matrix.py` | `MORPHIECORE_PROVIDER_MATRIX=1`；PROVIDERS/PROTOCOL/DELIVERY/CASE filter，默认只取计划的模型 |
| `examples/live_gateway_probe.py` | `MORPHIECORE_GATEWAY_PROBE=1`；固定文本/工具或真实 opaque 对照，目标限制读取代码 |
| `examples/live_probe.rs` | `MORPHIECORE_PROBE=1`；库级独立链，明确选择计划内目标，每次调用在共享账本登记和结算 |

Rust 入口在凭据加载前验证计划选择，默认不做目录请求。显式 `MORPHIECORE_PROBE_LIST_MODELS=1` 的目录查询也占共享 request slot，不将“免费预检”当作预算外 I/O。所有 native 报告位于 run 子目录。

库级 raw capture 默认关闭；仅另行授权的 synthetic forensic 调查才能启用 `MORPHIECORE_PROBE_CAPTURE=1`。会拒存包含当前 key 的数据并去掉已知 opaque/signature/credential 字段和 error 对象；这不是通用敏感数据脱敏器，也不授权生产内容捕获。redacted capture 不再是原始 wire 或可靠 replay oracle，不进入独立 fixtures。既有 `MORPHIECORE_PROBE_REPLAY_DIR` 和 `examples/replay_chat.rs` 保持离线，不加载 key；只能使用明确选定、符合其旧格式边界的历史 synthetic capture。

## pi

非私有代码已入仓：`examples/pi_probe.py` / `pi_probe.ts`。本机交互配置、真实 key 和 auth 文件不入仓；自动 probe 不读取日常配置，而是在 run 私有子目录生成最小配置，禁用资源发现、重试、compaction 和 cache warming，只注册固定 synthetic read。

```sh
uv run --project tests/sdk --locked --offline python examples/pi_probe.py \
  --package /path/to/pinned-pi --model "$PUBLIC_MODEL" --check --invalid-auth
```

Node 与 pi 类型依赖按[开发指南](development.md#测试语言与-js-工具)准备；可将 `--package` 指向本仓库 `node_modules/@earendil-works/pi-coding-agent`。`--protocol chat|responses` 可显式选择已准入协议；缺省取 probe catalog 的首项，不按模型名推断。协议选择在启动 Gateway 前验证，TS runner 只消费受信选择。

`--check` 使用 synthetic key 和拒绝 CONNECT 的 loopback proxy；错误 token 必须 401 且零上游连接，正确 token 准入必须到达拒绝代理。不需要 live run；未指定时使用临时 synthetic 账本。

真实调用需同一授权 run：

```sh
uv run --project tests/sdk --locked --offline python examples/pi_probe.py \
  --package /path/to/pinned-pi --model "$PUBLIC_MODEL" --run testdata/runtime/my-run --live
```

中继在首次 await 前同步预留本地 slot，验证正文后再占共享 slot；不修补实际认证/Content-Type。其端口为动态 loopback，不再固定占用 18080。strict 字面结果和未 trim 的 typed 正文/wire 比较独立于模型的标点或大小写偏好。synthetic read 只是受控工具 fixture，不证明通用文件工具或 TUI 兼容。

## 离线验证

```sh
uv run --project tests/sdk --locked --offline python -m unittest discover -s tests/sdk -p 'test_*.py'
uv run --project tests/sdk --locked --offline python -O -m unittest discover -s tests/sdk -p 'test_*.py'
npm run typecheck
npm test
cargo test --locked --offline --example live_probe
```

认证/状态/诊断文件、HTTP I/O lifetime 的 owning-layer Rust tests 与固定 SDK loopback 门槛仍见 [development](development.md)。synthetic 429/503、断流、并发预算、source/plan 损坏等证明的是本地边界，不是实际 Provider 的限流概率、长期可用性或推理质量。
