# E2a 卡片与列展示

- owner: E2a Fedora 作者
- execution_mode: isolated_write
- coordinator: Herman / 01a1fc2d-af6f-7d50-9759-33835b51eaeb
- base_commit: ba7d2a3
- worktree_path: /home/herman/.codex/worktrees/e2a-card-display-fedora/notion-program
- branch: codex/e2a-card-display-fedora
- status: implementing
- allowed_write_set: 卡片和列的 Vue/TSX 源码与生成资产、index.html 的对应接线、E1 构建入口的增量接入、卡片浏览器回归及关联测试和本项独有记录。

## 维护动作与首件

给卡片增加已有字段的提示，只改组件，不另列刷新条件。
卡片与列只消费真实保存回执、归列和资格，不复制 A、B1、B2 的业务规则。
E1 负责 Vue/TypeScript 离线构建与预览，本项接入实际交付结果。
首件从隔离 Rust fixture 读取快照，对照卡片、列计数及既有展开、焦点和滚动。
网页与实际包的 MCP resources/read 原始 HTML 分别运行。

## 依赖和边界

接单已获取 origin/main，基线已包含 B1。
A 作者工作树 51f3 已交付确认卡片回执，E1 作者工作树 ae2e 正在交付。
B2 单卡与批量资格由原作者负责，先读公开接口后再接入。
依赖未进主线时仅以真实依赖提交组合验证，最终从依赖齐全的 origin/main 重基。
不改版本、共享 STATE、journal、任务注册、部署或推送。
每个动作独立记录、审阅、最终头检查并交给 Fedora 集成者。
E2b 接管编辑保存，E2c 接管拖动及跨列刷新，分别交付。

## 测试归属

E2 接管迁移直接触及的卡片、列、编辑、拖动、计数和焦点的脆弱源码断言。
复用 `test::ui_layout::light_ui_refresh_health_and_catalog_preserve_interaction`、
`test::ui_layout::transfer_feedback_waits_for_receipts_and_preserves_interaction` 和
`test::ui_layout::long_content_and_project_filters_preserve_browser_column_geometry`。
E1 保留预览、桥接与打包入口归属；B2 保留操作资格；E3 保留批量归档及恢复。
涉及归档按钮源码断言时只迁移接线断言，保留原归档约束和后端资格断言。
真实 Codex 容器、现场主机及完整 31 条目标环境验收保持待验证。

## 首件记录

实际 Rust fixture 返回 3 张本机卡、5 列，页面身份、标题、列位置和计数逐项一致。
证据为 `/private/tmp/refactor-e2-first-baseline.json` 和同名 PNG。
独立 Chrome 进程组已退出且 profile 已删除，fixture 已停止并确认清理。
现组合 A 实际提交 `d593d51` 和 E1 的 `de015d3`、`1b8f170`、
`239084a`、`8e7904c` 开发，不复制另一作者实现。
已确认旧 `check-light-ui.mjs` 将“标题变化换掉整张卡”写成断言；
迁移同时改为检查更新后的标题、焦点及展开内容，不要求 DOM 重建。

Fedora 从 Mac 只读取回 `dca33b4`、`ab04618` 两个提交。与 B2 组合后，卡片、归档按钮及列计数复用 `cardActions`。浏览器已复现归档回执丢失后旧按钮被重新启用，改由 Vue 按最新资格更新。HTTP 页面与离线面板还覆盖未归列的空会话、真实打开及关闭预览后的焦点，以及单独更新展示字段时保留备注、项目展开和滚动位置。
