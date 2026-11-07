# 远程连接环境与源身份修复

owner: Hubble
execution_mode: isolated_worker
allowed_write_set: connection config and RPC helpers, remote host factory, app connection helper, connection tests, context/handoffs/remote-connection-identity-worker.md
base_commit: 10c80203284d3f0fe069d3cb003113cdb26a97f6
worktree_path: /home/herman/.codex/worktrees/remote-connection-identity/notion-program
coordinator: codex

创建前协调工作树干净。工作者只在独立工作树修改连接实现及定向测试，
不修改共享登记、状态、日志、版本或测试绑定，不提交，不修改真实配置。
协调者负责审阅、分批整合、绑定测试和提交。

复用 Desktop 登录环境，统一代理和时钟探针的就绪标记与有界输出处理。
源身份排除超时、保活和包装文本，包含解析后的端点及实际用户与数据目录；
来源变化时拒绝写入并保留数据。校时时间安全界不降低。
同一可观测身份的物理克隆、没有修订号的外部竞争仍是上游协议边界。
