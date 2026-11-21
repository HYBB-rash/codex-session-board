# 回复预览右侧空白的物理点击

- owner: E1 回复预览作者
- execution_mode: isolated_write
- coordinator: Linux 合入队列
- base_commit: `27e90305416c96e14177f00690c510f262f9c17b`
- worktree_path: `/home/herman/.codex/worktrees/preview-gutter-hit/notion-program`
- allowed_write_set: 本卡、同名结果记录、`scripts/check-reply-preview.mjs`

核对队列相邻套件的 MCP 窄屏失败，记录按下和点击的实际目标。用独立提交验证页面空白可以收起已打开的阅读栏，并保留横向滚动和布局断言。
