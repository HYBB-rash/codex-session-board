# 2026-11-13 并行开发与交付设计

用户要求明确 Mac 主开发、多 Agent 隔离、Linux 本机热修与即时体验的完整流程。
完成 `Docs/PARALLEL-DEVELOPMENT.md` 设计稿，并从 Mac 开发文档添加入口。
方案优先复用 Codex worktree/local environments、现有 fixture、Git、双平台 CI 和更新器。
区分现有能力与待实现命令；当前逐提交升版、检查、签名及完整验收规则保持有效。
后续按开发启动器、环境动作、合法串行集成、版本切换、CI 调度、Linux 读回逐步实施。
本轮仅项目内文档及按现规则准备的版本提交，不推送或部署，不操作远端及真实插件。
