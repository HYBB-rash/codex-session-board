# 保存交付包后发布渠道

owner: codex
execution_mode: isolated-worker
allowed_write_set: scripts/release*, scripts/package_migration.py, acceptance/test_saved_release.py, .github/workflows/plugin-candidate.yml, Docs/, context/tasks/active/saved-release-delivery-20261113.md
base_commit: c6e49e99b08bf542757c184683860241c46e5d14
worktree_path: /home/herman/.codex/worktrees/saved-release-delivery/notion-program
coordinator: 当前会话；singleton-integration 已先合入 main

采用 GitHub Actions、Releases 和现有插件打包器。版本标签绑定源码，平台包先保存并校验，渠道只消费已保存的包。
本地与 CI 复用同一入口，遇到同名包重新读取验证，禁止覆盖。双平台齐全后才发布不可变预发布；稳定发布保留完整验收门槛。
已同步 singleton-integration 的 c6e49e9 基线，并在其完成后提交。
8 项发布回归覆盖包复用、缺少平台、源码冲突、上传竞争、失败重试、稳定证据和不可变校验。
GitHub 仓库已启用不可变 Release；CI 默认令牌没有管理员设置读取权限时，发布后仍必须确认 immutable=true 才能写渠道。
真实双平台运行、候选包和渠道提交保存在 GitHub Actions、Releases 及 candidate/* 分支；本轮最终回执报告安装切换结果。
完整验收的目标环境证据仍未齐全，不能将自动回归或候选发布称为稳定验收完成。
