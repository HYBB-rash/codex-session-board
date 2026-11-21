# D 共享操作登记

owner: Codex 作者 01a1fc68-930a-7cc1-b249-e1c7041b5535
execution_mode: isolated_write
allowed_write_set: 本卡、共享操作登记、Rust 构建及分派、桥接源码及产物、对应测试及 bindings.json
base_commit: b859880165908a848a4982d2cfd9b722e9aeb912
worktree_path: /home/herman/.codex/worktrees/6aa4/notion-program
coordinator: 01a1fc2d-af6f-7d50-9759-33835b51eaeb
branch: codex/refactor-d-shared-operations

已 fetch、rebase。首件核对 `/api/archive-batches`，GET 无正文，经 `board_archive_batches` 回 `{batches}`。POST 经 `board_archive_batch_start` 接收批次 UUID 及 `items`，每项有 id、title、version、inputVersion，可带 sectionVersion，回 `{batch}`。两种操作都不提交原生归档。插件更新亦有同路径双方法。`board_open`、预览、核对归档均声明只读但用 POST。

疑点是旧桥把 POST `/api/sessions` 当作 GET，缺正文的写操作也会转交。页面统一通过 `body === undefined` 决定方法。检查真实离线桥、独立 stdio 进程和临时 HTTP 服务。HTTP 专用身份和校准端点、MCP 专用入口、兼容模式、预览专用回执分别核对。

| 测试 ID | 具体场景及出处 |
| --- | --- |
| `test::board_io::archive_batch_tools_share_http_history` | 本次首件。临时 HTTP 服务先读空列表，stdio MCP 新建，核对真实回执、无效参数及 HTTP/MCP 再读。新增，绑定 MCP-001。 |
| `test::ui_layout::mcp_bridge_preserves_routes_results_and_host_theme` | `scripts/check-app-bridge.mjs` 加载实际离线产物。核对两个双方法路径、私有及未知操作、参数和预览回执。 |
| `test::board_io::native_resource_and_tools_expose_global_app_only_actions` | 原生工具列表，MCP 专用入口、全局和 App 可见性。 |
| `test::legacy_mode::compatibility_tools_and_direct_routes_keep_archive_and_remote_creation_disabled` | 兼容模式工具可见性和直接调用限制。 |
| `test::native_authority::archive::archive_batch_recovers_lost_receipt_after_reopen_without_replay` | 原生归档回执丢失重开，验证分派仍走原业务规则。 |
| `test::ui_layout::reply_preview_preserves_scroll_focus_and_rejects_late_bodies` | E1 相邻边界。旧网页和嵌入资源均实际加载预览。 |
