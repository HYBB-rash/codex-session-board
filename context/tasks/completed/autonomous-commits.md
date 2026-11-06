# 自主提交规则与本次改动提交

owner: codex
execution_mode: serial_write
allowed_write_set: AGENTS.md, Cargo.toml, Cargo.lock, Docs/, README.md, scripts/, acceptance/bindings.json, src/, tests/, index.html, context/
base_commit: 48ee1e16bae9399cab9a37967f9887a509d2a973
worktree_path: /home/herman/文档/notion-program
coordinator: codex

2026-11-07 用户要求提交已有功能改动，并将今后本地提交全部改为由 Agent 自主决定。
更新项目约定、迁移说明和检查器提示，记录长期偏好；不扩展为推送或部署授权。
按每批最多 13 文件、400 行、每次补丁升版执行可构建、可验证的提交。
原归档按钮任务的真实客户端验收仍独立保持待验证。

已更新 AGENTS.md、Docs/MIGRATION.md 和检查器提交提示，并记录用户长期偏好。
本轮按规则拆成五批本地提交，版本从 0.1.29 依次推进至 0.1.34。
前四批分别为 a3c3ff2、2c65c66、5daa300、1ea6e30，最后一批包含协议测试与上下文收尾。
各批都审阅暂存内容并通过 ./scripts/check --pre-commit，以及钩子的严格 Clippy 和测试。
最终统一验收自动部分为 265 项，真实客户端证据仍待补充，release_ready=false。
未推送、未部署，未归档用户真实会话。本机未找到 agent-git，沿用现有 Git 签名配置。
