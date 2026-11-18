---
owner: agent
execution_mode: isolated_write
allowed_write_set: "preview RPC, runtime observation, board HTTP and MCP App UI, associated tests and documentation"
base_commit: 769f1f01a9d701d90cd07c7fc931632371b5ba4b
worktree_path: /home/herman/.codex/worktrees/final-reply-preview/notion-program
coordinator: integration
---

# 会话卡片最终答复预览

2026-11-18，用户试用原型后批准开发。卡片下方独立按钮打开右侧阅读面板。
读取当前轮次，保留原有点击跳转、拖动、筛选和滚动位置。
运行中、连接不明或无明确最终标记时隐藏。允许状态传播延迟，点击重新读回核对。
日用客户端和部署由独立交付阶段管理。

开发完成。看板快照只提供轮次 ID，点击后用同一主机的只读连接取得最终正文。
面板在轮次变化或连接不可用时关闭。通过 Rust 轮次回归、隔离浏览器及 MCP 桥接验证。
作者交付记录以验证工具生成的报告为准。正式客户端实装和目标主机验收待发布阶段完成。
