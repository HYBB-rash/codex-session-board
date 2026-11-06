# 看板新建会话入口

owner: codex
execution_mode: serial_write
allowed_write_set: index.html, src/app.rs, src/mcp.rs, tests/board_io.rs, README.md, context/
base_commit: b052b300013ecb5da1adc4a3616619dc0ce1257f
worktree_path: /home/herman/文档/notion-program
coordinator: codex

按截图在状态导航右侧增加新建会话按钮。复用配置的客户端命令，
打开当前客户端支持的 codex://threads/new 空白输入页，不发送消息。
保留既有未提交改动，不提交、不部署、不改变验收契约。
官方文档搜索未提供深链细节，已核对本机客户端 app.asar 的链接生成逻辑。

已实现导航右侧按钮、HTTP 路由和仅供 App 使用的 MCP 工具。
已有会话跳转继续校验已发现 UUID，新入口仅打开固定空白页。
22 个 board_io 测试、按钮真实脚本的成功/重复点击/错误恢复检查、
cargo fmt --check 和严格 Clippy 通过。完整验收报告在
/tmp/session-board-new-session-acceptance.json，真实客户端证据仍待补充。
完整验收的 228 个自动测试通过，退出 2，release_ready=false。
./scripts/check 因已有及本次未提交改动退出 1，其余检查通过。
未提交、未升版、未部署，因此当前运行实例尚未载入该入口。
