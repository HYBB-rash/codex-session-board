# 迁回主检出后归档已发布的 UI 修复工作区

owner: codex-ui-layout
execution_mode: isolated_write
allowed_write_set: context/, outputs/ui-stable-columns/, 本次受管worktree及本任务构建缓存
base_commit: a5deb92
worktree_path: /home/herman/.codex/worktrees/ui-stable-columns/notion-program
coordinator: codex-ui-layout

UI 修复已合并并上线 0.2.82，收尾记录已合并至主线。
用户授权清理已结束任务的 worktree，但 archive_worktree 拒绝移除当前任务的主工作区。
handoff_thread 只支持其他会话，不能由当前会话迁移自己。
需先把本会话迁回主检出 `/home/herman/文档/notion-program`，再调用受管归档工具。
在此之前保留当前检出，不通过直接删除或改写应用元数据绕过保护。
其他远程验收与五状态迁移仍 active，相关工作区保留。

具体证据和续作见 `context/handoffs/ui-worktree-cleanup-20261111.md`。
