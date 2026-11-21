# 卡片预览选中状态

- owner: 前端收尾作者
- execution_mode: isolated_write
- allowed_write_set: 本卡、卡片和预览组件、首页装配与构建产物、现有双入口预览测试
- base_commit: `eb9b179470af72d1058fa4eb0fe9e61eb05a32eb`
- worktree_path: `/home/herman/.codex/worktrees/vue-preview-selection/notion-program`
- coordinator: 五轮总协调，交唯一合入队列

沿用 `test::ui_layout::reply_preview_preserves_scroll_focus_and_rejects_late_bodies`，通过 HTTP 和嵌入资源检查打开、切换、刷新、关闭后的选中项及无关卡片是否被重写。保留已有键盘焦点、滚动和迟到回复检查。
