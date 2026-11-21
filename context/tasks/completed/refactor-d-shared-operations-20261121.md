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
| `test::native_authority::entrypoints::preview_tool_keeps_reply_out_of_model_content` | 实际 stdio 调用预览；与同一隔离 HTTP 服务读出的正文逐字段比较，整个 MCP 回执只含空 content 和 `_meta.previewReply`。新增，绑定 MCP-001。 |

## 作者交付

最终队列基线为 `b794c8a076760e7d5ea6936a118a168b57f7fa95`，实现及测试提交为 `804e92ca174caeec97e20bce45eab285dba1c7d0`。起始基线已逐次接入 C1、T2、B1，再接入协调者冻结的 T1 队列。B1 到 T1 的六个 D 提交经 `git range-diff` 逐项等价。

18 个共享操作共用 16 条路径。原有四处人工登记分别位于浏览器桥、MCP 工具列表、MCP 到 HTTP 的转发和 App 的 HTTP 分派。现只在 `assets/board-operations.json` 登记方法、路径、工具名及标题和只读提示。增加业务操作时实现 `Action` 分支及对应参数描述，运行 `node scripts/build-app-bridge.mjs` 重建离线包。E1 如改桥接构建，后合入的一方须基于组合入口重生成，再验桥接与预览。HTTP 私有端点及 MCP 私有 `board_view` 仍由原入口负责。

独审在临时副本只改 `board_connect` 工具名、保留旧 bundle，原 56 项桥接断言全部通过。修复后，`test::ui_layout::mcp_bridge_preserves_routes_results_and_host_theme` 的第 57 项逐项比对登记和实际调用，这一故障会令测试退出 1。原工具名、同路径双方法、未知拒绝及预览的独立预期也保留。预览测试首轮遇到原生 fixture 不支持 `notLoaded`，已补入稳定的轮次确认回应。

B1 组合头 `f5b955ca75ae99550bcb5c0786ea7eefc46ed3b2` 的 Linux 定向 108 项、Clippy、源码包预检、离线产物逐字节复建、项目和上下文检查均通过。独审在该头复测了工具改名反例、归档跨入口、预览及三组 B1 归列。本卡随分支交协调者；最终队列头回执随作者交付消息提供，合入顺序由 Herman 决定。
