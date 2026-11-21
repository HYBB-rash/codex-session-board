# E1 · Vue 回复预览

- owner: E1 author
- execution_mode: isolated_write
- coordinator: Herman / 01a1fc2d-af6f-7d50-9759-33835b51eaeb
- base_commit: b859880165908a848a4982d2cfd9b722e9aeb912
- worktree_path: /Users/hongyong/.codex/worktrees/ae2e/codex-session-board
- branch: codex/refactor-e1-vue-preview
- status: in_progress
- allowed_write_set: index.html 的预览实现与接线；assets/reply-*；assets/app-bridge.package*；scripts/build-app-bridge.mjs；预览相关浏览器及 Rust 测试；必要构建/打包边界；本项独有任务和结果记录。

## 维护动作与首件

修改回复阅读行为时，到预览组件查看唯一一份状态和事件处理。
布局由 .vue 模板组合，交互按钮用 TSX；网页与 MCP 资源使用同一份离线产物。
首件在两个入口打开长回复，滚动后接收新答复提示，保留旧正文和阅读位置，主动切换后显示新正文。
同次删除旧预览手工 DOM 实现。A 保存、D 映射、E2 卡片和编辑不在本项范围。

## 疑点与验证

- 先实际编译 .vue 和 TSX，再锁定实现；esbuild 不做类型检查，必须运行 vue-tsc。
- 现有构建入口已有临时 npm 安装和逐字节核对。复用，不另建运行时依赖。
- assets 源码平放，沿用源码包与指纹覆盖，不修改 31 条需求、锁或独立模型。
- 现有 sandbox 测试手工拼接 MCP HTML，不足以证明实际包内资源；增加真实二进制 resources/read 输入。
- 只用隔离 fixture。真实 Codex 客户端待独立授权和取证，不安装或重载日用插件。
- T3 由本项负责。以实际交互、正文、焦点、滚动和请求结果替换对旧实现的依赖。
  对照验证等价改写仍通过、迟到结果覆盖新选择的真实错误会失败。

## 认领完整测试 ID

```text
test::ui_layout::reply_preview_preserves_scroll_focus_and_rejects_late_bodies
test::ui_layout::mcp_bridge_preserves_routes_results_and_host_theme
test::board_io::native_resource_and_tools_expose_global_app_only_actions
test::board_io::real_stdio_mcp_handshake_and_resource_round_trip
lib::codex_session_board::app::preview::tests::preview_markdown_formats_text_without_loading_untrusted_content
lib::codex_session_board::app::preview::tests::reply_reader_only_returns_the_expected_completed_turn
lib::codex_session_board::app::preview::tests::reply_reader_distinguishes_empty_from_unknown_and_rechecks_changed_turns
lib::codex_session_board::app::preview::tests::reply_endpoint_reads_only_on_click_and_retires_on_transport_failure
lib::codex_session_board::app::preview::tests::snapshot_rechecks_idle_turns_when_the_list_revision_is_unchanged
lib::codex_session_board::app::preview::tests::snapshot_tracks_observed_turns_across_same_second_transitions
```

## 研究依据

- Vue 官方 TypeScript overview 和 render-function 指南：vue-tsc 检查 SFC，TSX 使用 Vue JSX 类型。
- Vue core compiler-sfc README：生产构建的 compileScript / inlineTemplate。
- Vue jsx-runtime 官方实现：jsx/jsxs 使用 Vue h。
- esbuild 官方 plugins / content-types：onLoad 编译 SFC，构建不会检查 TypeScript。
- 本仓库 Docs/LIGHT-UI.md：每包一行的锁、生成许可证、依赖变化保持完整自动门禁。

## 交付边界

最终准确头独立只读审阅，执行项目检查、相关测试、离线构建及浏览器/实际嵌入产物验证。
记录失败与修复，不把浏览器或资源验证写成真实客户端签收。
Herman 决定合入顺序；本项不更新 main、不推送、不部署。
共享 STATE、日志、归档和登记由协调者处理。
