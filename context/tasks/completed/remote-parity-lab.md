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

评估完成，原型未部署。33 项实验测试通过，其中独立参考模型的 520 条边、
1417 个比较步骤全部通过。真实隔离代理完成 14 项基础检查；四个已安装 CLI
版本分别通过 11 项故障检查和审批状态观察。远程临时进程、目录及转发已清理。
完整门禁 334 项自动检查通过，退出 2；14 项外部证据仍待补，release_ready=false。

结论与逐项边界见 Docs/REMOTE-PARITY-LAB.md。增量维护成本中等，可复用
现有状态与写入规则；主要剩余工作是连接层、客户端映射和既有数据迁移。
不能用独立 stdio 实例代替执行中的共享 daemon，也不应由看板管理用户 daemon。
无后端修订号时跨客户端 ABA 和无条件标题写入竞争仍不能完全消除。
旧远程人工状态迁移须保留历史及备注，不应冒充已确认的原生位置或自动触发归档。

原始证据和复现脚本位于 outputs/remote-parity-lab/；它们被 Git 忽略，
故保留本工作树供审阅。桌面完整交互、物理网络/主机故障、审批跨重启和长期负载
尚未证明；四个版本成功不代表任意版本兼容。正式功能接入需另行验收。
