# 部署待归档正序

owner: codex
execution_mode: serial_write
allowed_write_set: context/STATE.yaml, context/tasks/active/deploy-archive-oldest-first.md, context/tasks/completed/deploy-archive-oldest-first.md, context/journal/2026-11-06-deploy-archive-oldest-first.md, bin/, outputs/, 当前 session-board-rust 插件部署与服务
base_commit: 97bed312b24e3d2e9a2c9a3be3df7a232791c21f
worktree_path: /home/herman/文档/notion-program
coordinator: codex

用户明确要求部署到当前看板并查看效果。
当前 Rust 看板服务使用 outputs/rust-rewrite/native-live/config.json 和原数据目录，端口 18767。
构建新二进制，更新当前 Rust 插件，核验服务身份后重启并打开看板。
保留配置、数据库与原 Notion 服务停用状态；未获提交授权，不提交或升版。

部署日期：2026-11-06。

- release 构建通过，沿用仓库当前版本 0.1.15。
- 当前 marketplace 源仍为 outputs/codex-session-board-plugin.hgDZ0Q，更新前复制到 outputs/codex-session-board-plugin.pre-archive-sort。
- 安装缓存更新至 session-board-rust/codex-session-board/0.1.15，二进制 SHA256 与本地构建一致。
- 保留原配置环境变量；stop 确认原服务退出，launch 新服务 PID 1852221，端口 18767，原数据目录不变。
- 通过桌面 bridge 执行 skills/list forceReload 和 config/mcpServer/reload，均成功。
- board_view 打开原生 MCP App 并跳转待归档列，112 条会话载入、自动状态连接正常。待归档 9 张卡片显示从 11/5 19:46 到 11/6 16:27 正序。
- HTTP 页面含新排序，原生客户端 DOM 与截图均确认最早在前。
- 全量发布契约的其他真实环境项目尚未补证，仍不得宣称完整验收通过。
