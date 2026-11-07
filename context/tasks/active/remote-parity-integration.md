# 正式接入远程会话并验收

owner: codex
execution_mode: coordinated_readonly_workers
allowed_write_set: src/, tests/, assets/, examples/, scripts/, Docs/, acceptance/bindings.json, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, config.example.json, README.md, context/, outputs/remote-parity-integration/
base_commit: 4187733
worktree_path: /home/herman/.codex/worktrees/remote-parity-lab/notion-program
coordinator: codex

用户授权正式接入、功能对齐及完成验收。复用实验及现有业务逻辑。
已将实验移到主线 218e78b，保留崩溃恢复、定时归档、排序及项目任务取消记录。
原实验提交和证据仍可通过 49e9057 及 outputs/remote-parity-lab/ 查询。

依次实现受限共享代理传输、按主机业务实例与数据接入、发现及运行观察、
原生映射与交互、真实验收和部署核验。不得降低冻结契约及断言。
真实破坏性测试仅使用专用实验会话。既有工作会话不用于制造故障。
不重新开展已经取消的新建项目任务；远程新建会话仅针对已登记项目。
共享状态与写入由协调者管理；代理只读核对主线和验收矩阵。
