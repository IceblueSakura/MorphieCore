# 当前开发焦点

## F：Gateway 自有续轮载荷与标准消费者

可观察结果：标准 Responses 能交付、保存并回传 Gateway 自有认证加密载荷，内部原生 signature 不冒充 OpenAI 签发值。切换 Provider 或 Model 时不续传 opaque，同目标必须重验必要依赖。

- F：闭合 reasoning `encrypted_content` 的显式 carrier dispatch、来源容器恢复、下游本地响应时间及固定消费者；复用[纯库载荷](../architecture/anthropic-messages-profile.md#gateway-自有续轮载荷)，不把签发/验证等同于公开交付，不新增独立 attachment。
- 独立失败例：同样错误位对应未执行/执行失败；前缀编辑、owner 删除、签名移挂；载荷篡改/错误密钥/超限；同目标必要回传缺失；跨 Provider/Model 续传 opaque。
- 非目标：D 的原生 SSE reducer、E 的目标激活/真实调用；密钥生命周期与服务端会话存储。Go 分组独立遵循[缓存载体合同](../architecture/protocol-and-lowering.md#cache-affinity-projection)，不从加密 reasoning 反推；客户端新工具错误的公开载体仍须定稿。
- 验证：最低 owner 独立 synthetic 回归与受影响 Rust 离线基线。固定消费者与公开 Gateway 接线只在其前置合同成立后验证，不将纯库通过当作完整 F 验收。
- 停止点：闭合当前可独立验证的静态责任边界；遇到错误载体等实质未决选择时保留明确缺口，不静默降级，不提交或部署。

推进方向、strict 验证边界及延期恢复条件归[next-goal](next-goal.md)，承载/接线缺口归[实施边界](../implementation-status/generation.md)，等待证据或决策的问题归[待决状态](../implementation-status/open-questions.md)。授权与变更要求遵循 [AGENTS.md](../../AGENTS.md)，检查方法归[开发指南](../development.md)，本页不重复合同或操作规则。
