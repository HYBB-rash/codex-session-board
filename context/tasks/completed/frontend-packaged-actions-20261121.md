# 前端收尾 · 网页与包内 MCP 的同组操作

- owner: 前端收尾作者，`01a1fc68-c750-7091-b1fa-266a0418fea4`
- execution_mode: isolated_write
- coordinator: 五轮重构协调会话，串行合入由 `01a1fc9c-cc9a-7ba0-b8e4-43a8a606dc3d` 负责
- base_commit: `b794c8a076760e7d5ea6936a118a168b57f7fa95`
- worktree_path: `/home/herman/.codex/worktrees/frontend-packaged-actions/notion-program`
- branch: `codex/frontend-packaged-actions`
- allowed_write_set: 本卡与同名首件记录、开发 fixture 的归档身份应答、双入口集成验证、相关绑定、T0 指定的两个测试；组件合入后经核实已被替代的页面或构建路径

首件用同一离线插件包启动隔离 Rust HTTP 服务和 MCP 进程。网页打开真实 HTTP 根路径，嵌入页面读取包进程 `resources/read` 返回的 HTML 原文。两个入口使用同一套卡片操作，通过独立读回核对最终结果。
主要疑点是包内页面调用的工具是否到达同一已核验服务、筛选后的归档目标和版本是否准确、关闭再打开后的批次能否从后端恢复。

`test::ui_layout::mcp_bridge_preserves_routes_results_and_host_theme` 验证桥接协议；
`test::ui_layout::reply_preview_preserves_scroll_focus_and_rejects_late_bodies` 验证预览交互与竞态。
这两项现有测试使用模拟应答。包启动检查 `scripts/verify_package_startup.py` 已连通实际 MCP 和 HTTP 服务。
本项认领包内原样 HTML 与网页的共同操作结果，关联 `MCP-001`、`NAV-001` 和 `UI-002`。复用上述入口，项目的真实 Codex 容器验收单列。

最终核验以 E1、E2、E3、筛选和排序实际进入主线的源码为准。到时逐项对照旧 DOM、事件监听、重绘入口及离线构建产物，保留各功能作者的业务归属。领取时主线尚未包含这些组件。
