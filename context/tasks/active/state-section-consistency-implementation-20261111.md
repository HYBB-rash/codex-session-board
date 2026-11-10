# 五状态分区一致性实现、验证与上线

- owner: Codex
- execution_mode: isolated_write
- allowed_write_set: src/；tests/；scripts/；acceptance/bindings.json；acceptance/test_remote_release.py；acceptance/test_gate.py；Docs/；插件清单、版本与项目上下文
- base_commit: 4704f93c53d0bec35fc0f7306c02c53e0a0fc1a3
- worktree_path: /home/herman/.codex/worktrees/state-section-consistency/notion-program
- coordinator: 当前会话主代理
- status: pending_validation

用户授权隔离 worktree 实现调查方案，架构微调必须体现在形式化验证中，完成后合并、部署并清理 worktree 与构建产物。
源目录已有其他任务暂存内容及 HEAD 1a86c73 的版本历史违规；保留原状，从此前有效提交建立干净隔离分支 codex/state-section-consistency。
两名既有子代理审阅形式化模型及发布路线；业务源码由主代理修改，验收助手仅在独立目录准备参数化驱动。

## 交付范围

- 日常五状态统一绑定访问、配置严格校验、原生路径一致使用。
- 旧六映射的显式迁移和只读预检，精确账号/缺失分区诊断。
- 独立有限模型穷举验证配置与原生绑定不变量，并对照真实实现。
- 保留冻结六状态契约、参考模型和锁，补扩展验收及实际配置门禁。
- 审阅验收后合并正式仓库、备份和迁移配置、部署核验与工作树清理。

## 不变边界

不发用户业务消息，不把修复作为新增归档资格，不按名称猜账号或分区，不重放不确定写入。
生产切换保留数据和备注，明确处理现有半小时归档；外部缺证不伪称 release_ready。

## 实施进度

已统一有效绑定、严格五项校验、精确账号诊断、迁移 CLI 和只读预检，并增强原生/远程汇总门禁。
独立形式模型覆盖 1,152 条配置边、4,096 组账号组合、600 次正确移动与过期拒绝、30 类故障；五种实现突变均被检出。
审阅发现的数据目录非法值和预检选路分歧已修正，预检与 App 共用连接选择；非法目录与代理哨兵回归通过。
冻结需求、参考模型和锁保持不变。复用 4f2aa5f 的构建门禁打包修正，另纳入浏览器验证 mjs 资产。
原主仓版本历史问题已由另一任务修复；已通过 78a0cbd 整合 main 的布局修复，完整提交钩子通过。
正式已由布局任务部署为 v0.2.82，configId 仍为 ce0664890c3b6c81，六映射尚未迁移。
原配置侧栏 socket 已不存在；现场核对了新侧栏端点和实际 canonical daemon，未修改权限或生产。
真实客户端拖拽因当前 Linux CUA 不支持 drag，需在专用入口准备后取得实际操作证据。
临时突变材料在 /tmp/state-section-mutations-20261111，稳定验收准备目录为 /home/herman/.local/state/codex-session-board/state-section-release-20261111。
所有副本、证据及发布包需先转到稳定位置；任务完成后清理本 worktree 和其 target。
