# PR 队列核心

owner: codex/queue_reuse_review
execution_mode: isolated_write
allowed_write_set: scripts/pr_queue.py、scripts/pr-queue、acceptance/test_pr_queue.py、本任务记录
base_commit: 2a750fe283887a831db87ecac718ddadc424cb7e
worktree_path: /Users/hongyong/.codex/worktrees/pr-queue-core-20261115/codex-session-board
coordinator: 01a1d38b-1ef8-7392-ba78-caa7935c5ab4
status: active

实现固定来源、作者反馈、机械合并、依赖阻塞和重入队。复用现有交付报告与候选工具，
不修改业务代码，不修改共享状态和测试绑定。通知传输由协调者提供。
