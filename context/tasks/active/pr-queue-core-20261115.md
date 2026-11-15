# PR 队列核心

owner: codex/queue_reuse_review
execution_mode: isolated_write
allowed_write_set: scripts/pr_queue.py、scripts/pr-queue、acceptance/test_pr_queue.py、本任务记录
base_commit: 2a750fe283887a831db87ecac718ddadc424cb7e
worktree_path: /Users/hongyong/.codex/worktrees/pr-queue-core-20261115/codex-session-board
coordinator: 01a1d38b-1ef8-7392-ba78-caa7935c5ab4
status: ready_for_coordinator

实现固定来源、作者反馈、机械合并、依赖阻塞和重入队。复用现有交付报告与候选工具，
不修改业务代码，不修改共享状态和测试绑定。通知传输由协调者提供。

核心复用 validate 报告和 integrate 发布入口；冲突只机械回滚。相同反馈请求复用同一
评论与 transport receipt 路径，必须原会话 ACK 后才允许新来源重新入队。
历史演练仅允许 refs/heads/codex/rehearsal-main-*，真实队列只允许 refs/heads/main。
针对性验证：python3 -m unittest acceptance.test_pr_queue。测试绑定由协调者维护。
