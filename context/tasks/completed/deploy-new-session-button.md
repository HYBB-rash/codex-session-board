# 部署并验证新建会话按钮

owner: codex
execution_mode: serial_write
allowed_write_set: context/, bin/, outputs/, 当前 session-board-rust 插件部署与服务
base_commit: 5d24617
worktree_path: /home/herman/文档/notion-program
coordinator: codex

用户授权部署到当前看板并验证新建会话按钮。
沿用现有配置与数据库，构建 0.1.17，更新当前插件及受管服务。
通过原生 UI 点击按钮，核实空白新会话页，不发送消息。
记录真实结果；不推送，不将本项实测冒充全量发布验收。

2026-11-06 部署与实测结果：

- release 构建 0.1.17 完成。生成的部署副本清单同步为 0.1.17，
  保留已部署 .mcp.json 中原 CODEX_SESSION_BOARD_CONFIG。
- marketplace 原内容备份到 outputs/codex-session-board-plugin.pre-new-session-20261106。
- codex plugin add 安装缓存 0.1.17；构建二进制与安装缓存 SHA256 一致。
- stop 核验旧服务退出，launch 新服务 PID 2069916，端口仍为 18767。
  /api/info 确认版本 0.1.17 及原数据目录。
- skills/list forceReload 与 config/mcpServer/reload 成功，
  桌面 MCP 状态显示 0.1.17，提供 board_new。
- 原生看板截图显示 113 条会话、自动状态已连接，以及导航右侧的新按钮。
  使用真实窗口坐标点击按钮后进入空白新会话输入页，
  显示“你想让我们在 Notion任务看板管理 中构建什么？”、
  项目与“此计算机”选择、空白输入框。未输入或发送任何消息。
- 点击后 /api/sessions 仍为 113 条；扫描完整、运行观察连接正常。
- 本次用户目标已实测达成。完整发布验收其他项目仍缺证，
  不宣称 release_ready=true。仅部署记录变更，未再次提交或推送。
