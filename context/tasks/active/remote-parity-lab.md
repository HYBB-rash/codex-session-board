# 远程功能等价实验与维护成本评估

owner: codex
execution_mode: coordinated_readonly_workers
allowed_write_set: src/, tests/, examples/, scripts/, Docs/, acceptance/bindings.json, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, context/, outputs/remote-parity-lab/
base_commit: b7acaa72fb62e2ffaf0d44292956a31d34bee036
worktree_path: /home/herman/.codex/worktrees/remote-parity-lab/notion-program
coordinator: codex

用户要求专用工作树实现并测试已识别的远程功能等价风险，明确维护成本及可靠性。
原仓库已干净后建立独立工作树；不合并、不部署、不改日常服务配置或工作会话。
协调者唯一写入；两名只读代理分别核对客户端路由和独立测试矩阵。

交付实验原型、可复现故障测试、真实能力取证及逐项结论。
优先复用 Rust Store、状态模型和现有 RPC，不另造完整看板。
真实写入限实验创建且身份已确认的隔离会话；工作会话仅只读。
未能观察或实际验证的行为明确保留限制，不将模拟成功作为真实环境验收。
保留冻结 31 条契约、参考模型和锁；完整门禁仍如实报告既有缺证。
