# PR 手动交接

owner: codex
execution_mode: isolated_write
allowed_write_set: scripts/pr_task*.py、scripts/pr_queue_closeout.py、对应验收测试与绑定、PR 文档、项目 pr-task skill、本任务记录
base_commit: 6b985e0cd7defc9dea9a0b4c891b273b3260b363
worktree_path: /Users/hongyong/.codex/worktrees/pr-task-manual-20261117
coordinator: 01a1e3fb-c015-7200-8dc9-1223b83cbb21
status: in_progress

用户要求先手动体验开发、独立审阅、修复与集成交接，暂不自动衔接各会话。
复用已有验证、提交、GitHub 反馈及队列，增加不依赖客户端后台的持久手动模式。
只读刷新不得发送消息、修改 PR 或推进本地生命周期；交付与收尾显式执行。
旧自动任务保持原有行为，不迁移或停止既有定时器。

验证覆盖注册、查询、CI 及源码变更、反馈阻塞、重复交付和终态收尾。
本轮使用隔离 fixture 和项目验证；真实业务任务由用户随后选择，不冒称已实跑。
