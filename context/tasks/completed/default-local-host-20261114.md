# 主机选择默认本机

- owner: codex
- execution_mode: serial_write
- allowed_write_set: index.html; scripts/check-host-filter.mjs; tests/ui_layout.rs; Docs/PROJECT-FILTERS.md; context/tasks/active/default-local-host-20261114.md; context/tasks/completed/default-local-host-20261114.md; context/journal/2026-11-14-default-local-host.md
- base_commit: b593edd2b29304c440c5ca381c48c863c91cb00f
- worktree_path: /Users/hongyong/Developer/codex-session-board
- coordinator: codex
- status: completed

打开页面时默认选择本机，保留手动选择全部主机及远端主机。
沿用已有主机筛选，不持久保存选择，不更改后台连接与归档。
使用已有隔离浏览器回归验证初始范围、项目可用及手动切换。
开发提交不修改版本清单及共享 STATE，不安装或重载日用插件。

已将静态选择项与页面初始筛选统一为本机，文档同步说明每次打开的默认范围。
已有浏览器回归增加初始本机、项目可选及手动全部主机的断言。
`cargo test --locked --offline --test ui_layout long_content_and_project_filters_preserve_browser_column_geometry`
通过，包含 48 组布局与 53 项主机筛选检查。
首次受限运行因本机监听端口被拒绝而失败，获准运行隔离浏览器后通过。
此次仅完成本地开发，不代表已部署或完整目标环境验收完成。
