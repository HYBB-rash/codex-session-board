# 正式 PR 发现与领取

owner: codex
execution_mode: isolated_write
allowed_write_set: PR 值班 CLI、skill、项目约定、使用说明、验证与本任务记录
base_commit: 8a44008c926cdbbe6d7cc4114a8a2c54c1dbfe78
worktree_path: /home/herman/.codex/worktrees/pr-duty-discovery-claim/notion-program
coordinator: pr-duty-claim-20261121
status: active

从 GitHub 完整查询目标为 `main` 的开放 PR，按就绪标签和来源规则领取，复用作者登记、审阅和队列。
实现经审阅与组合门禁合入后，在原任务 `pr` 的两轮之间切换。
当前首件为 PR #31，若已由其他队列接管则复用该队列。
正式处理与作者送达分别保留可核对的证据。
