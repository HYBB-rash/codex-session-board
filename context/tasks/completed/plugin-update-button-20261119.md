# 看板内更新

owner: codex
execution_mode: isolated_write
allowed_write_set: src/, scripts/, index.html, assets/, acceptance/, Docs/, context/tasks/active/plugin-update-button-20261119.md, context/results/
base_commit: 64e74f96ff2e977f177dbaf7ef77539f35f93ec5
worktree_path: /home/herman/.codex/worktrees/plugin-update-button/notion-program
coordinator: codex

按钮跟随本机平台候选频道。复用包校验、原生安装与安装回执，隔离验证实际安装后的升级、重载、数据保留与失败恢复。

2026-11-19 在 Fedora Linux 上使用独立 Codex `0.169.0` 后台、插件缓存、配置、数据库与浏览器 profile。两份实际安装包由 `5951007` 的功能源码构建。第二份包只在临时源码副本中升为 `0.4.13`。使用隔离 Git 仓库模拟频道快进。另从 GitHub 下载并校验正式 Linux 候选 `0.4.12`，频道提交 `c4ecd2508a42200aeae6e6e66160df6f55d0048a`。

在隔离容器中打开已安装插件提供的 MCP 界面资源，通过实际 Codex 后台调用 MCP 工具。每次双击只提交一次更新。

- 同频道版本显示「已是最新版 0.4.12」，安装回执与缓存保留。
- 频道临时缺失时显示 Git 错误与「原安装未更改」，恢复频道后可重试。
- 点击更新并关闭页面。同一 Codex 后台请求重载后，重新打开看板读回 `0.4.13`，二进制摘要与安装回执一致，界面显示「新版 0.4.13 已生效，看板已恢复」。
- 控制 socket 权限检查失败时显示「已安装，待重载」，独立读回仍为 `0.4.12`。重启隔离后台并打开看板后读回 `0.4.13`。
- 两次升级的配置文件摘要一致。验收会话的阻塞状态、备注和版本 `7` 保留。最终验收从空历史开始，升级后 MCP 快照只含验收会话。

验收修正了插件环境缺少 `CODEX_HOME` 时历史补读落入日用目录的问题。最终截图来自重新取证的隔离环境。作者交付物是本地工作树及验收记录。正式候选包由现有合并发布流程交付。

报告、截图、测试重放脚本及 GitHub 包摘要位于 `/home/herman/.local/state/codex-session-board/update-button-20261119/`。
