# 批量归档结果与恢复

owner: Fedora 总协调，E3 页面作者
execution_mode: isolated_write
allowed_write_set: assets/board-archive*、Vue 构建入口、index.html 的归档区、scripts/check-host-filter.mjs 的归档操作、对应任务与结果
base_commit: 735b368cb08aa2ce3c3c6db8f6f643316b362f4c
worktree_path: /home/herman/.codex/worktrees/refactor-batch-archive-page/notion-program
coordinator: 五轮重构总协调

维护者修改归档结果时，在一处区分服务端记下的逐项结果和当前页面仍在发送的请求。首件用持久批次打开结果列表，让已确认、待核对、新输入、明确拒绝、正在提交与尚未提交逐项可见。点击核对只查询归档状态。

认领 `test::ui_layout::long_content_and_project_filters_preserve_browser_column_geometry` 中丢回执、重开与手动核对的浏览器场景，以及 `test::archive_flow::archive_mcp_resource_exposes_only_an_explicit_app_action` 内与归档按钮绑定方式有关的断言。十卡浏览器验收与原生写入回归由独立作者分别取证。交付时接入 E2、B2 和 C1 已合入的主线，沿用同一份资源构建。

组合作者树为 `/home/herman/.codex/worktrees/refactor-archive-vue-integration/notion-program`。原分支 `6d6f259` 保留 B2 组合证据。最终重放基线 `71c36f313bce2c7f8ff1ef801017ee87c8ab072c` 已包含 Vue 卡片、编辑、拖动、筛选、排序、预览选择和原生十卡夹具。组合实现与浏览器和后端回归对应 `a816660d36850c6a17bf72087b051f2421a61de1`。
