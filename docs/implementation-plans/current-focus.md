# 当前开发焦点

## E：所选原生目标的功能验证

可观察结果：指定原生目标的文本、工具成功/错误与实际 signature 回传具有证据。复用[独立库绑定](../../src/topology/catalog/messages.rs)与[受控原生入口](../probes.md#native-messages-probe)，不激活公开 Gateway，不以 wire 声明或另一个模型的成功推定资格。

- 先独立验证 origin/path、x-api-key、版本/UA/session、完整原请求及单来源凭据；保持默认 thinking，不发送未验证的 reasoning 控制，不加入默认 probe。
- 有限矩阵：文本 JSON/SSE 2 请求、工具两轮双向交付 4、synthetic 错误反馈最多 3、实际 signature 两轮双向交付 4、截断 1；未声明 discovery 路径时不猜，合计最多 14。cap≤2048、接收≤2 MiB、单次≤120 秒、run≤1800 秒，零 retry/fallback/redirect。
- 下一行为边界：固定 SDK 声明的可选 nullable stop/容器/报告尚未完全映射；当前未知字段（包括 null）保持拒绝。Inactive 表示与非空结构化 stop/refusal/计量分开定稿；遇到 IR gap 先说明 typed owner 选项，不藏入 fidelity 或直接丢字段。
- 停止点：native evidence 闭合所选范围即提交；未报告 signature 保留该分支缺口。官方页面不可获取时保留事实空缺，不更新外部验证日期，不宣称标准消费者或实例已激活。

## F：Gateway 自有续轮载荷与标准消费者

可观察结果：标准 Responses 能交付、保存并回传 Gateway 自有认证加密载荷，内部原生 signature 不冒充 OpenAI 签发值。切换 Provider 或 Model 时不续传 opaque，同目标必须重验必要依赖。

- F：闭合完整标准响应的投影、下游本地响应时间及固定消费者；复用[显式 item 载荷与认证前缀恢复](../architecture/anthropic-messages-profile.md#gateway-自有续轮载荷)，不把 item 级签发/验证等同于完整或公开交付，不新增独立 attachment。
- 独立失败例：同样错误位对应未执行/执行失败；前缀编辑、owner 删除、签名移挂；载荷篡改/错误密钥/超限；同目标必要回传缺失；跨 Provider/Model 续传 opaque。
- 非目标：E 的目标激活/真实调用、增量 native→标准事件交付、密钥生命周期与服务端会话存储。Go 分组独立遵循[缓存载体合同](../architecture/protocol-and-lowering.md#cache-affinity-projection)，不从加密 reasoning 反推；客户端新工具错误的公开载体仍须定稿。
- 验证：最低 owner 独立 synthetic 回归与受影响 Rust 离线基线。固定消费者与公开 Gateway 接线只在其前置合同成立后验证，不将纯库通过当作完整 F 验收。
- 停止点：闭合当前可独立验证的静态责任边界；遇到错误载体等实质未决选择时保留明确缺口，不静默降级，不提交或部署。

推进方向、strict 验证边界及延期恢复条件归[next-goal](next-goal.md)，承载/接线缺口归[实施边界](../implementation-status/generation.md)，等待证据或决策的问题归[待决状态](../implementation-status/open-questions.md)。授权与变更要求遵循 [AGENTS.md](../../AGENTS.md)，检查方法归[开发指南](../development.md)，本页不重复合同或操作规则。
