# PR 值班工具合入与定时任务迁移

owner: codex
execution_mode: isolated_write
allowed_write_set: 值班工具、相关门禁与测试、值班说明和本任务记录
base_commit: 95caa8479396f4d156fb995443ad44e0a2f68533
worktree_path: /home/herman/.codex/worktrees/pr-duty-main-integration/notion-program
coordinator: pr-duty-main-merge-20261120
status: active

从远端 `main` 移植试点提交 `6a577d6` 中的值班工具，保留主线更新。
独立审阅、准确头 PR CI 和双平台组合门禁通过后，原地迁移 Codex 定时任务 `pr`。
在原会话核验两次正式只读扫描的源码、命令、结果和模型。正式写入由明确交接的 PR 开始。
