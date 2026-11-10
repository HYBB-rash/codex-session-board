# 五状态分区一致性实现、验证与上线

- owner: Codex
- execution_mode: isolated_write
- allowed_write_set: src/；tests/；scripts/；acceptance/bindings.json；acceptance/test_remote_release.py；Docs/；版本与项目上下文
- base_commit: 4704f93c53d0bec35fc0f7306c02c53e0a0fc1a3
- worktree_path: /home/herman/.codex/worktrees/state-section-consistency/notion-program
- coordinator: 当前会话主代理
- status: implementing

用户授权隔离 worktree 实现调查方案，架构微调必须体现在形式化验证中，完成后合并、部署并清理 worktree 与构建产物。
源目录已有其他任务暂存内容及 HEAD 1a86c73 的版本历史违规；保留原状，从此前有效提交建立干净隔离分支 codex/state-section-consistency。
两名既有子代理只读审阅形式化模型方案及发布验收路线；所有写入由主代理完成。

## 交付范围

- 日常五状态统一绑定访问、配置严格校验、原生路径一致使用。
- 旧六映射的显式迁移和只读预检，精确账号/缺失分区诊断。
- 独立有限模型穷举验证配置与原生绑定不变量，并对照真实实现。
- 保留冻结六状态契约、参考模型和锁，补扩展验收及实际配置门禁。
- 审阅验收后合并正式仓库、备份和迁移配置、部署核验与工作树清理。

## 不变边界

不发用户业务消息，不把修复作为新增归档资格，不按名称猜账号或分区，不重放不确定写入。
生产切换保留数据和备注，明确处理现有半小时归档；外部缺证不伪称 release_ready。
